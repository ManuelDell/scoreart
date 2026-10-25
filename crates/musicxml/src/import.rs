//! MusicXML (score-partwise) → [`Score`]. Written from the MusicXML 4.0 specification.
//! Unknown elements are ignored; what we read is listed in `docs/file-format-parity.md`.

use std::collections::BTreeMap;

use scoreart_model::{
    Articulation, Barline, Clef, Duration, Dynamic, Event, EventKind, KeySig, MeasureInfo, Meta, Note,
    NoteValue, Part, PartMeasure, Pitch, Score, StaffMeasure, Step, TPQ, Tempo, TimeSig, TimeSymbol, Tuplet,
    Voice, clip, limits,
};

use crate::Error;
use crate::dom::Element;

/// A note or rest as read, before it is placed into voices.
struct Raw {
    staff: usize,
    voice: u32,
    onset: u64,
    ticks: u64,
    event: Event,
}

pub fn read(root: &Element) -> Result<Score, Error> {
    match root.name.as_str() {
        "score-partwise" => {}
        "score-timewise" => return Err(Error::Unsupported("score-timewise MusicXML".into())),
        other => return Err(Error::Unsupported(format!("root element <{other}>"))),
    }
    let mut score = Score {
        meta: read_meta(root),
        ..Score::default()
    };

    // Part list: names and MIDI programs.
    let mut infos: Vec<(String, String, String, u8)> = Vec::new();
    if let Some(list) = root.child("part-list") {
        for sp in list.children_named("score-part") {
            let id = sp.attr("id").unwrap_or_default().to_string();
            let name = sp.child_text("part-name").unwrap_or_default().to_string();
            let abbr = sp.child_text("part-abbreviation").unwrap_or_default().to_string();
            let program = sp
                .path(&["midi-instrument", "midi-program"])
                .and_then(|e| e.text.trim().parse::<i32>().ok())
                .map(|p| (p - 1).clamp(0, 127) as u8)
                .unwrap_or(0);
            infos.push((id, name, abbr, program));
        }
    }

    for (pi, part_el) in root.children_named("part").enumerate() {
        if pi >= limits::MAX_PARTS {
            break;
        }
        let id = part_el.attr("id").unwrap_or_default().to_string();
        let (name, abbr, program) = infos
            .iter()
            .find(|i| i.0 == id)
            .map(|i| (i.1.clone(), i.2.clone(), i.3))
            .unwrap_or_else(|| (format!("Part {}", pi + 1), String::new(), 0));
        let part = read_part(&mut score, pi == 0, part_el, id, name, abbr, program)?;
        score.parts.push(part);
    }
    if score.parts.is_empty() {
        return Err(Error::Invalid("no parts".into()));
    }
    if let Some(last) = score.measures.last_mut()
        && last.barline == Barline::Regular
    {
        last.barline = Barline::Final;
    }
    score.normalize();
    Ok(score)
}

fn read_meta(root: &Element) -> Meta {
    let mut m = Meta::default();
    if let Some(t) = root.path(&["work", "work-title"]) {
        m.title = clip(t.text.trim());
    }
    if let Some(t) = root.child_text("movement-title") {
        if m.title.is_empty() {
            m.title = clip(t);
        } else {
            m.subtitle = clip(t);
        }
    }
    if let Some(id) = root.child("identification") {
        for c in id.children_named("creator") {
            match c.attr("type") {
                Some("composer") => m.composer = clip(c.text.trim()),
                Some("lyricist") | Some("poet") => m.lyricist = clip(c.text.trim()),
                _ => {}
            }
        }
        if let Some(r) = id.child_text("rights") {
            m.copyright = clip(r);
        }
    }
    m
}

fn clef_of(el: &Element) -> Clef {
    let sign = el.child_text("sign").unwrap_or("G");
    let line = el.child_num("line").unwrap_or(0.0) as i32;
    let oct = el.child_num("clef-octave-change").unwrap_or(0.0) as i32;
    match (sign, line) {
        ("G", _) if oct < 0 => Clef::Treble8vb,
        ("G", _) => Clef::Treble,
        ("F", _) => Clef::Bass,
        ("C", 4) => Clef::Tenor,
        ("C", _) => Clef::Alto,
        ("percussion", _) => Clef::Percussion,
        _ => Clef::Treble,
    }
}

fn articulations_of(n: &Element) -> Vec<Articulation> {
    let mut out = Vec::new();
    for notations in n.children_named("notations") {
        if let Some(a) = notations.child("articulations") {
            for c in &a.children {
                let art = match c.name.as_str() {
                    "staccato" => Some(Articulation::Staccato),
                    "staccatissimo" | "spiccato" => Some(Articulation::Staccatissimo),
                    "accent" => Some(Articulation::Accent),
                    "tenuto" => Some(Articulation::Tenuto),
                    "strong-accent" => Some(Articulation::Marcato),
                    _ => None,
                };
                if let Some(a) = art
                    && !out.contains(&a)
                {
                    out.push(a);
                }
            }
        }
        if notations.has("fermata") && !out.contains(&Articulation::Fermata) {
            out.push(Articulation::Fermata);
        }
    }
    out
}

fn tie_start(n: &Element) -> bool {
    n.children_named("tie").any(|t| t.attr("type") == Some("start"))
        || n.children_named("notations")
            .any(|no| no.children_named("tied").any(|t| t.attr("type") == Some("start")))
}

fn read_part(
    score: &mut Score,
    first_part: bool,
    part_el: &Element,
    id: String,
    name: String,
    abbreviation: String,
    program: u8,
) -> Result<Part, Error> {
    let mut divisions: u64 = 1;
    let mut staves: usize = 1;
    let mut clefs: BTreeMap<usize, Clef> = BTreeMap::new();
    let mut measures: Vec<PartMeasure> = Vec::new();
    let mut time = score.measures.first().and_then(|m| m.time).unwrap_or_default();

    for (mi, m_el) in part_el.children_named("measure").enumerate() {
        if mi >= limits::MAX_MEASURES {
            break;
        }
        if score.measures.len() <= mi {
            score.measures.push(MeasureInfo::default());
        }
        let mut raws: Vec<Raw> = Vec::new();
        let mut clef_changes: BTreeMap<usize, Clef> = BTreeMap::new();
        let mut pending_dyn: Vec<(usize, u64, Dynamic)> = Vec::new();
        let mut t: u64 = 0;
        let mut last_onset: u64 = 0;
        let mut measure_end: u64 = 0;

        for c in &m_el.children {
            match c.name.as_str() {
                "attributes" => {
                    if let Some(d) = c.child_num("divisions") {
                        divisions = (d as u64).clamp(1, 1_000_000);
                    }
                    if let Some(s) = c.child_num("staves") {
                        staves = (s as usize).clamp(1, limits::MAX_STAVES);
                    }
                    if first_part {
                        let info = score
                            .measures
                            .get_mut(mi)
                            .ok_or(Error::Invalid("measure".into()))?;
                        if let Some(k) = c.child("key")
                            && let Some(f) = k.child_num("fifths")
                        {
                            info.key = Some(KeySig::new(f as i32));
                        }
                        if let Some(ts) = c.child("time") {
                            let beats = ts
                                .child_text("beats")
                                .and_then(|b| {
                                    b.split('+')
                                        .map(|x| x.trim().parse::<i64>().ok())
                                        .sum::<Option<i64>>()
                                })
                                .unwrap_or(4);
                            let bt = ts.child_num("beat-type").unwrap_or(4.0) as i64;
                            if let Ok(mut sig) = TimeSig::new(beats, bt) {
                                sig.symbol = match ts.attr("symbol") {
                                    Some("common") => TimeSymbol::Common,
                                    Some("cut") => TimeSymbol::Cut,
                                    _ => TimeSymbol::Normal,
                                };
                                info.time = Some(sig);
                                time = sig;
                            }
                        }
                    }
                    for cl in c.children_named("clef") {
                        let n = cl
                            .attr("number")
                            .and_then(|n| n.parse::<usize>().ok())
                            .unwrap_or(1)
                            .max(1)
                            - 1;
                        if n < limits::MAX_STAVES {
                            if mi == 0 && t == 0 {
                                clefs.insert(n, clef_of(cl));
                            } else {
                                clef_changes.insert(n, clef_of(cl));
                            }
                        }
                    }
                }
                "note" => {
                    if c.has("grace") || c.has("cue") {
                        continue;
                    }
                    let dur = c.child_num("duration").unwrap_or(0.0).max(0.0) as u64;
                    let ticks = dur.saturating_mul(u64::from(TPQ)) / divisions;
                    let staff = c
                        .child_num("staff")
                        .map(|s| (s as usize).max(1) - 1)
                        .unwrap_or(0)
                        .min(limits::MAX_STAVES - 1);
                    let voice = c.child_num("voice").map(|v| v as u32).unwrap_or(1);
                    let is_chord = c.has("chord");
                    let onset = if is_chord { last_onset } else { t };
                    let note = c.child("pitch").map(|p| {
                        let step = p
                            .child_text("step")
                            .and_then(|s| s.chars().next())
                            .and_then(Step::from_letter)
                            .unwrap_or(Step::C);
                        let alter = p.child_num("alter").unwrap_or(0.0).round() as i8;
                        let octave = p.child_num("octave").unwrap_or(4.0) as i32;
                        let mut n = Note::new(Pitch::new(step, alter, octave));
                        n.tie = tie_start(c);
                        n
                    });
                    let unpitched = c.child("unpitched").map(|p| {
                        let step = p
                            .child_text("display-step")
                            .and_then(|s| s.chars().next())
                            .and_then(Step::from_letter)
                            .unwrap_or(Step::C);
                        let octave = p.child_num("display-octave").unwrap_or(5.0) as i32;
                        Note::new(Pitch::new(step, 0, octave))
                    });
                    let note = note.or(unpitched);
                    if is_chord {
                        if let (Some(n), Some(prev)) = (
                            note,
                            raws.iter_mut()
                                .rev()
                                .find(|r| r.staff == staff && r.voice == voice),
                        ) && let EventKind::Chord(notes) = &mut prev.event.kind
                            && notes.len() < limits::MAX_CHORD_NOTES
                        {
                            notes.push(n);
                            notes.sort_by_key(|n| (n.pitch.diatonic(), n.pitch.alter));
                        }
                        continue;
                    }
                    let mut duration = c
                        .child_text("type")
                        .and_then(NoteValue::from_xml_name)
                        .map(|v| Duration::new(v, c.children_named("dot").count().min(3) as u8));
                    if let (Some(d), Some(tm)) = (duration.as_mut(), c.child("time-modification")) {
                        let a = tm.child_num("actual-notes").unwrap_or(1.0) as i64;
                        let n = tm.child_num("normal-notes").unwrap_or(1.0) as i64;
                        if (2..=32).contains(&a) && (1..=32).contains(&n) && a != n {
                            d.tuplet = Some(Tuplet {
                                actual: a as u8,
                                normal: n as u8,
                            });
                        }
                    }
                    // Trust the sounding duration when the written type disagrees (or is missing).
                    let duration = match duration {
                        Some(d) if u64::from(d.ticks()) == ticks => d,
                        _ => Duration::from_ticks(ticks.min(u64::from(u32::MAX)) as u32)
                            .or(duration)
                            .unwrap_or(Duration::new(NoteValue::Quarter, 0)),
                    };
                    let mut event = match note {
                        Some(n) => Event::chord(vec![n], duration),
                        None => Event::rest(duration),
                    };
                    event.articulations = articulations_of(c);
                    if let Some(l) = c.child("lyric").and_then(|l| l.child_text("text")) {
                        event.lyric = Some(clip(l));
                    }
                    raws.push(Raw {
                        staff,
                        voice,
                        onset,
                        ticks,
                        event,
                    });
                    last_onset = onset;
                    t = t.saturating_add(ticks);
                    measure_end = measure_end.max(t);
                }
                "backup" => {
                    let d = c.child_num("duration").unwrap_or(0.0).max(0.0) as u64;
                    t = t.saturating_sub(d.saturating_mul(u64::from(TPQ)) / divisions);
                }
                "forward" => {
                    let d = c.child_num("duration").unwrap_or(0.0).max(0.0) as u64;
                    t = t.saturating_add(d.saturating_mul(u64::from(TPQ)) / divisions);
                    measure_end = measure_end.max(t);
                }
                "direction" => {
                    let staff = c.child_num("staff").map(|s| (s as usize).max(1) - 1).unwrap_or(0);
                    for dt in c.children_named("direction-type") {
                        if let Some(d) = dt.child("dynamics") {
                            for k in &d.children {
                                if let Some(dy) = Dynamic::from_id(&k.name) {
                                    pending_dyn.push((staff, t, dy));
                                }
                            }
                        }
                        if first_part
                            && let Some(w) = dt.child_text("words")
                            && !w.is_empty()
                            && let Some(info) = score.measures.get_mut(mi)
                            && info.text.is_none()
                        {
                            info.text = Some(clip(w));
                        }
                        if first_part
                            && let Some(mm) = dt.child("metronome")
                            && let Some(bpm) = mm.child_num("per-minute")
                            && let Some(info) = score.measures.get_mut(mi)
                        {
                            info.tempo = Some(Tempo {
                                bpm: bpm.clamp(10.0, 400.0) as u16,
                            });
                        }
                    }
                    if first_part
                        && let Some(s) = c.child("sound")
                        && let Some(tempo) = s.attr("tempo").and_then(|v| v.parse::<f64>().ok())
                        && tempo.is_finite()
                        && let Some(info) = score.measures.get_mut(mi)
                    {
                        info.tempo = Some(Tempo {
                            bpm: tempo.clamp(10.0, 400.0) as u16,
                        });
                    }
                }
                "sound" => {
                    if first_part
                        && let Some(tempo) = c.attr("tempo").and_then(|v| v.parse::<f64>().ok())
                        && tempo.is_finite()
                        && let Some(info) = score.measures.get_mut(mi)
                    {
                        info.tempo = Some(Tempo {
                            bpm: tempo.clamp(10.0, 400.0) as u16,
                        });
                    }
                }
                "barline" if first_part => {
                    let info = score
                        .measures
                        .get_mut(mi)
                        .ok_or(Error::Invalid("measure".into()))?;
                    let right = c.attr("location").unwrap_or("right") == "right";
                    if let Some(r) = c.child("repeat") {
                        match r.attr("direction") {
                            Some("backward") => info.barline = Barline::RepeatEnd,
                            Some("forward") => info.repeat_start = true,
                            _ => {}
                        }
                    } else if right {
                        match c.child_text("bar-style") {
                            Some("light-heavy") => info.barline = Barline::Final,
                            Some("light-light") => info.barline = Barline::Double,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        let _ = (time, measure_end);

        // Attach dynamics to the event that starts at (or right after) their position.
        for (staff, at, dy) in pending_dyn {
            if let Some(r) = raws
                .iter_mut()
                .filter(|r| r.staff == staff.min(staves - 1) && r.onset >= at)
                .min_by_key(|r| (r.onset, r.voice))
            {
                r.event.dynamic.get_or_insert(dy);
            }
        }

        // Group into staves and voices; voice numbers map to indices in ascending order per staff.
        let mut pm = PartMeasure { staves: Vec::new() };
        for s in 0..staves {
            let mut sm = StaffMeasure {
                voices: Vec::new(),
                clef: clef_changes.get(&s).copied(),
            };
            let mut numbers: Vec<u32> = raws.iter().filter(|r| r.staff == s).map(|r| r.voice).collect();
            numbers.sort_unstable();
            numbers.dedup();
            for vn in numbers.into_iter().take(limits::MAX_VOICES) {
                let mut v = Voice::default();
                let mut pos: u64 = 0;
                let mut items: Vec<&Raw> = raws.iter().filter(|r| r.staff == s && r.voice == vn).collect();
                items.sort_by_key(|r| r.onset);
                for r in items {
                    if r.onset > pos {
                        for d in Duration::decompose((r.onset - pos).min(u64::from(u32::MAX)) as u32) {
                            v.events.push(Event::rest(d));
                        }
                        pos = r.onset;
                    }
                    if r.onset < pos {
                        continue; // overlapping events in one voice: keep the first
                    }
                    v.events.push(r.event.clone());
                    pos = pos.saturating_add(r.ticks);
                    if v.events.len() >= limits::MAX_EVENTS_PER_VOICE {
                        break;
                    }
                }
                sm.voices.push(v);
            }
            pm.staves.push(sm);
        }
        measures.push(pm);
    }

    let mut clef_list: Vec<Clef> = (0..staves)
        .map(|s| clefs.get(&s).copied().unwrap_or(Clef::Treble))
        .collect();
    if staves == 2
        && !clefs.contains_key(&1)
        && let Some(c) = clef_list.get_mut(1)
    {
        *c = Clef::Bass;
    }
    Ok(Part {
        id,
        name,
        abbreviation,
        program,
        clefs: clef_list,
        measures,
    })
}
