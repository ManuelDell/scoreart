//! MusicXML (score-partwise) → [`Score`]. Written from the MusicXML 4.0 specification.
//! Unknown elements are ignored.

use std::collections::{BTreeMap, HashMap};

use scoreart_model::marks::{MARKS, XmlGroup};
use scoreart_model::{
    Anchor, Arpeggio, Articulation, Barline, Clef, Duration, Dynamic, Event, EventKind, Grace, Jump, KeySig,
    MeasureInfo, Meta, Note, NoteHead, NoteValue, Part, PartMeasure, PartSound, Pitch, Score, Spanner,
    SpannerKind, StaffMeasure, Step, TPQ, Tempo, TimeSig, TimeSymbol, Tremolo, Tuplet, Voice, clip, limits,
};

use crate::Error;
use crate::dom::Element;

/// A line that starts or stops on a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Family {
    Slur,
    Gliss,
    Trill,
}

/// A note or rest as read, before it is placed into voices.
struct Raw {
    staff: usize,
    voice: u32,
    onset: u64,
    ticks: u64,
    event: Event,
    starts: Vec<(Family, String, SpannerKind)>,
    stops: Vec<(Family, String)>,
}

/// A line start or stop from a `<direction>` at a time position.
#[derive(Clone, Debug)]
enum DirMark {
    Start { key: String, kind: SpannerKind },
    Stop { key: String },
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
    let mut infos: Vec<(String, String, String, u8, PartSound)> = Vec::new();
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
            let mut sound = PartSound::default();
            if let Some(v) = sp.path(&["score-instrument", "virtual-instrument"]) {
                sound.library = clip(v.child_text("virtual-library").unwrap_or_default().trim());
                sound.preset = clip(v.child_text("virtual-name").unwrap_or_default().trim());
            }
            if let Some(v) = sp
                .path(&["midi-instrument", "volume"])
                .and_then(|e| e.text.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite())
            {
                sound.volume = v.round().clamp(0.0, 100.0) as u8;
            }
            if let Some(v) = sp
                .path(&["midi-instrument", "pan"])
                .and_then(|e| e.text.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite())
            {
                // Degrees (-180…180, ±90 = hard left/right) → -100…100.
                let deg = if v.abs() > 90.0 { v.signum() * 180.0 - v } else { v };
                sound.pan = (deg / 0.9).round().clamp(-100.0, 100.0) as i8;
            }
            infos.push((id, name, abbr, program, sound));
        }
    }

    for (pi, part_el) in root.children_named("part").enumerate() {
        if pi >= limits::MAX_PARTS {
            break;
        }
        let id = part_el.attr("id").unwrap_or_default().to_string();
        let (name, abbr, program, sound) = infos
            .iter()
            .find(|i| i.0 == id)
            .map(|i| (i.1.clone(), i.2.clone(), i.3, i.4.clone()))
            .unwrap_or_else(|| (format!("Part {}", pi + 1), String::new(), 0, PartSound::default()));
        let mut part = read_part(&mut score, pi, part_el, id, name, abbr, program)?;
        part.sound = sound;
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

/// The catalogue mark for a MusicXML element in a notations group.
fn mark_for(group: XmlGroup, el: &Element) -> Option<Articulation> {
    let name = el.name.as_str();
    let text = el.text.trim();
    let candidates = MARKS.iter().filter(|d| d.xml == group);
    // Fermatas: the shape is the element's text.
    if group == XmlGroup::Fermata {
        let shape = if text.is_empty() { "normal" } else { text };
        return MARKS
            .iter()
            .find(|d| d.xml == XmlGroup::Fermata && d.xml_name == shape)
            .or_else(|| MARKS.iter().find(|d| d.id == "fermata"))
            .and_then(|d| Articulation::from_id(d.id));
    }
    // A SMuFL name wins (other-* elements, special variants).
    if let Some(smufl) = el.attr("smufl")
        && let Some(d) = MARKS
            .iter()
            .find(|d| d.glyph_above == smufl || d.glyph_below == smufl)
    {
        return Articulation::from_id(d.id);
    }
    let mut best = None;
    for d in candidates.filter(|d| d.xml_name == name) {
        match d.xml_attr {
            Some(("smufl", _)) => {}
            Some((k, v)) => {
                let actual = if k == "value" { Some(text) } else { el.attr(k) };
                if actual == Some(v) {
                    return Articulation::from_id(d.id);
                }
            }
            None => best = best.or(Some(d)),
        }
    }
    best.and_then(|d| Articulation::from_id(d.id))
}

fn parse_pitch(p: &Element, step_tag: &str, oct_tag: &str) -> Pitch {
    let step = p
        .child_text(step_tag)
        .and_then(|s| s.chars().next())
        .and_then(Step::from_letter)
        .unwrap_or(Step::C);
    let alter = p.child_num("alter").unwrap_or(0.0).round() as i8;
    let octave = p.child_num(oct_tag).unwrap_or(4.0) as i32;
    Pitch::new(step, alter, octave)
}

/// Chord-symbol text from a `<harmony>` element.
fn harmony_text(h: &Element) -> Option<String> {
    let root = h.child("root")?;
    let step = root.child_text("root-step")?.chars().next()?;
    let alter = match root.child_num("root-alter").map(|a| a.round() as i32) {
        Some(1) => "#",
        Some(-1) => "b",
        _ => "",
    };
    let kind_el = h.child("kind");
    let suffix = kind_el
        .and_then(|k| k.attr("text").map(str::to_string))
        .or_else(|| {
            kind_el.map(|k| {
                match k.text.trim() {
                    "major" | "" => "",
                    "minor" => "m",
                    "dominant" => "7",
                    "major-seventh" => "maj7",
                    "minor-seventh" => "m7",
                    "diminished" => "dim",
                    "diminished-seventh" => "dim7",
                    "augmented" => "+",
                    "half-diminished" => "m7b5",
                    "suspended-fourth" => "sus4",
                    "suspended-second" => "sus2",
                    "major-sixth" => "6",
                    "minor-sixth" => "m6",
                    "dominant-ninth" => "9",
                    "power" => "5",
                    _ => "",
                }
                .to_string()
            })
        })
        .unwrap_or_default();
    let bass = h
        .child("bass")
        .and_then(|b| {
            let s = b.child_text("bass-step")?.chars().next()?;
            let a = match b.child_num("bass-alter").map(|a| a.round() as i32) {
                Some(1) => "#",
                Some(-1) => "b",
                _ => "",
            };
            Some(format!("/{s}{a}"))
        })
        .unwrap_or_default();
    Some(clip(&format!("{step}{alter}{suffix}{bass}")))
}

fn jump_from_words(w: &str) -> Option<Jump> {
    let t = w.trim().trim_end_matches('.').to_ascii_lowercase();
    Some(match t.as_str() {
        "fine" => Jump::Fine,
        "to coda" => Jump::ToCoda,
        "d.c" | "da capo" => Jump::DaCapo,
        "d.c. al fine" | "da capo al fine" => Jump::DaCapoAlFine,
        "d.c. al coda" | "da capo al coda" => Jump::DaCapoAlCoda,
        "d.s" | "dal segno" => Jump::DalSegno,
        "d.s. al fine" | "dal segno al fine" => Jump::DalSegnoAlFine,
        "d.s. al coda" | "dal segno al coda" => Jump::DalSegnoAlCoda,
        _ => return None,
    })
}

fn read_part(
    score: &mut Score,
    pi: usize,
    part_el: &Element,
    id: String,
    name: String,
    abbreviation: String,
    program: u8,
) -> Result<Part, Error> {
    let first_part = pi == 0;
    let mut divisions: u64 = 1;
    let mut staves: usize = 1;
    let mut clefs: BTreeMap<usize, Clef> = BTreeMap::new();
    let mut measures: Vec<PartMeasure> = Vec::new();
    // Open lines: (staff, voice, family, number) for note lines; (staff, key) for directions.
    let mut open_note: HashMap<(usize, usize, Family, String), (SpannerKind, Anchor)> = HashMap::new();
    let mut open_dir: HashMap<(usize, String), (SpannerKind, usize, Anchor)> = HashMap::new();
    let mut open_ending: Option<(u8, usize)> = None;
    let mut repeat_staves: Vec<bool> = vec![false; limits::MAX_STAVES];

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
        let mut pending_text: Vec<(usize, u64, String, bool)> = Vec::new(); // (staff, t, text, is_chord_symbol)
        let mut dir_marks: Vec<(usize, u64, DirMark)> = Vec::new();
        let mut graces: HashMap<(usize, u32), Vec<Grace>> = HashMap::new();
        let mut t: u64 = 0;
        let mut last_onset: u64 = 0;

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
                    for ms in c.children_named("measure-style") {
                        if let Some(r) = ms.child("measure-repeat") {
                            let on = r.attr("type") == Some("start");
                            match ms.attr("number").and_then(|n| n.parse::<usize>().ok()) {
                                Some(n) if n >= 1 => {
                                    if let Some(x) = repeat_staves.get_mut(n - 1) {
                                        *x = on;
                                    }
                                }
                                _ => repeat_staves.iter_mut().for_each(|x| *x = on),
                            }
                        }
                    }
                }
                "note" => {
                    if c.has("cue") {
                        continue;
                    }
                    let staff = c
                        .child_num("staff")
                        .map(|s| (s as usize).max(1) - 1)
                        .unwrap_or(0)
                        .min(limits::MAX_STAVES - 1);
                    let voice = c.child_num("voice").map(|v| v as u32).unwrap_or(1);
                    let pitch = c
                        .child("pitch")
                        .map(|p| parse_pitch(p, "step", "octave"))
                        .or_else(|| {
                            c.child("unpitched")
                                .map(|p| parse_pitch(p, "display-step", "display-octave"))
                        });
                    // Grace notes wait for the next real note of their voice.
                    if let Some(g) = c.child("grace") {
                        let list = graces.entry((staff, voice)).or_default();
                        if let Some(p) = pitch {
                            if c.has("chord")
                                && let Some(last) = list.last_mut()
                            {
                                if last.pitches.len() < limits::MAX_CHORD_NOTES {
                                    last.pitches.push(p);
                                }
                            } else if list.len() < limits::MAX_GRACE {
                                let value = c
                                    .child_text("type")
                                    .and_then(NoteValue::from_xml_name)
                                    .unwrap_or(NoteValue::Eighth);
                                list.push(Grace {
                                    pitches: vec![p],
                                    value,
                                    slash: g.attr("slash") == Some("yes"),
                                });
                            }
                        }
                        continue;
                    }
                    let dur = c.child_num("duration").unwrap_or(0.0).max(0.0) as u64;
                    let ticks = dur.saturating_mul(u64::from(TPQ)) / divisions;
                    let is_chord = c.has("chord");
                    let onset = if is_chord { last_onset } else { t };
                    let note = pitch.map(|p| {
                        let mut n = Note::new(p);
                        n.tie = c.children_named("tie").any(|t| t.attr("type") == Some("start"))
                            || c.children_named("notations")
                                .any(|no| no.children_named("tied").any(|t| t.attr("type") == Some("start")));
                        if let Some(h) = c.child("notehead") {
                            n.head = NoteHead::from_id(h.text.trim()).unwrap_or_default();
                            n.paren = h.attr("parentheses") == Some("yes");
                        }
                        if let Some(a) = c.child("accidental")
                            && (a.attr("cautionary") == Some("yes")
                                || a.attr("parentheses") == Some("yes")
                                || a.attr("editorial") == Some("yes"))
                        {
                            n.forced = Some(scoreart_model::Accidental::from_alter(p.alter));
                        }
                        for no in c.children_named("notations") {
                            if let Some(f) = no.path(&["technical", "fingering"]) {
                                let txt = f.text.trim();
                                if !txt.is_empty() {
                                    n.fingering = Some(clip(txt));
                                }
                            }
                        }
                        n
                    });
                    // Marks and lines on this note (also read on chord notes; duplicates are ignored).
                    let mut marks: Vec<Articulation> = Vec::new();
                    let mut tremolo = Tremolo::None;
                    let mut arpeggio = Arpeggio::None;
                    let mut starts = Vec::new();
                    let mut stops = Vec::new();
                    for no in c.children_named("notations") {
                        for ch in &no.children {
                            match ch.name.as_str() {
                                "articulations" => marks.extend(
                                    ch.children
                                        .iter()
                                        .filter_map(|a| mark_for(XmlGroup::Articulations, a)),
                                ),
                                "fermata" => marks.extend(mark_for(XmlGroup::Fermata, ch)),
                                "ornaments" => {
                                    for o in &ch.children {
                                        match o.name.as_str() {
                                            "tremolo" => {
                                                let n = o.text.trim().parse::<u8>().unwrap_or(3).clamp(1, 4);
                                                tremolo = match o.attr("type") {
                                                    Some("unmeasured") => Tremolo::Buzz,
                                                    Some("start") | Some("stop") => tremolo,
                                                    _ => Tremolo::Strokes(n),
                                                };
                                            }
                                            "wavy-line" => {
                                                let num = o.attr("number").unwrap_or("1").to_string();
                                                match o.attr("type") {
                                                    Some("start") => {
                                                        let kind = if o
                                                            .attr("smufl")
                                                            .is_some_and(|v| v.contains("Vibrato"))
                                                        {
                                                            SpannerKind::Vibrato
                                                        } else {
                                                            SpannerKind::TrillLine
                                                        };
                                                        starts.push((Family::Trill, num, kind));
                                                    }
                                                    Some("stop") => stops.push((Family::Trill, num)),
                                                    _ => {}
                                                }
                                            }
                                            _ => marks.extend(mark_for(XmlGroup::Ornaments, o)),
                                        }
                                    }
                                }
                                "technical" => marks.extend(
                                    ch.children
                                        .iter()
                                        .filter(|x| x.name != "fingering")
                                        .filter_map(|x| mark_for(XmlGroup::Technical, x)),
                                ),
                                "arpeggiate" => {
                                    arpeggio = match ch.attr("direction") {
                                        Some("up") => Arpeggio::Up,
                                        Some("down") => Arpeggio::Down,
                                        _ => Arpeggio::Normal,
                                    }
                                }
                                "non-arpeggiate" => arpeggio = Arpeggio::Bracket,
                                "slur" => {
                                    let num = ch.attr("number").unwrap_or("1").to_string();
                                    match ch.attr("type") {
                                        Some("start") => starts.push((Family::Slur, num, SpannerKind::Slur)),
                                        Some("stop") => stops.push((Family::Slur, num)),
                                        _ => {}
                                    }
                                }
                                "glissando" | "slide" => {
                                    let num = ch.attr("number").unwrap_or("1").to_string();
                                    let kind = if ch.attr("line-type") == Some("wavy") {
                                        SpannerKind::WavyGlissando
                                    } else {
                                        SpannerKind::Glissando
                                    };
                                    match ch.attr("type") {
                                        Some("start") => starts.push((Family::Gliss, num, kind)),
                                        Some("stop") => stops.push((Family::Gliss, num)),
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    // A trill line draws its own "tr": don't add the trill mark twice.
                    if starts.iter().any(|(_, _, k)| *k == SpannerKind::TrillLine) {
                        marks.retain(|m| m.id() != "trill");
                    }
                    if is_chord {
                        if let Some(prev) = raws
                            .iter_mut()
                            .rev()
                            .find(|r| r.staff == staff && r.voice == voice)
                        {
                            if let (Some(n), EventKind::Chord(notes)) = (note, &mut prev.event.kind)
                                && notes.len() < limits::MAX_CHORD_NOTES
                            {
                                notes.push(n);
                                notes.sort_by_key(|n| (n.pitch.diatonic(), n.pitch.alter));
                            }
                            for m in marks {
                                if !prev.event.articulations.contains(&m) {
                                    prev.event.articulations.push(m);
                                }
                            }
                            if arpeggio != Arpeggio::None {
                                prev.event.arpeggio = arpeggio;
                            }
                            prev.starts.extend(starts);
                            prev.stops.extend(stops);
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
                    marks.dedup();
                    event.articulations = marks;
                    event.tremolo = tremolo;
                    event.arpeggio = arpeggio;
                    if let Some(l) = c.child("lyric").and_then(|l| l.child_text("text")) {
                        event.lyric = Some(clip(l));
                    }
                    if let Some(g) = graces.remove(&(staff, voice)) {
                        event.grace = g;
                    }
                    raws.push(Raw {
                        staff,
                        voice,
                        onset,
                        ticks,
                        event,
                        starts,
                        stops,
                    });
                    last_onset = onset;
                    t = t.saturating_add(ticks);
                }
                "backup" => {
                    let d = c.child_num("duration").unwrap_or(0.0).max(0.0) as u64;
                    t = t.saturating_sub(d.saturating_mul(u64::from(TPQ)) / divisions);
                }
                "forward" => {
                    let d = c.child_num("duration").unwrap_or(0.0).max(0.0) as u64;
                    t = t.saturating_add(d.saturating_mul(u64::from(TPQ)) / divisions);
                }
                "harmony" => {
                    let staff = c.child_num("staff").map(|s| (s as usize).max(1) - 1).unwrap_or(0);
                    if let Some(text) = harmony_text(c) {
                        pending_text.push((staff, t, text, true));
                    }
                }
                "direction" => {
                    let staff = c.child_num("staff").map(|s| (s as usize).max(1) - 1).unwrap_or(0);
                    let system_text = c.attr("system") == Some("only-top");
                    let has_tempo = c.path(&["direction-type", "metronome"]).is_some()
                        || c.child("sound").is_some_and(|s| s.attr("tempo").is_some());
                    let mut words_here: Option<String> = None;
                    for dt in c.children_named("direction-type") {
                        for k in &dt.children {
                            match k.name.as_str() {
                                "dynamics" => {
                                    for d in &k.children {
                                        let id = if d.name == "other-dynamics" {
                                            d.text.trim().to_string()
                                        } else {
                                            d.name.clone()
                                        };
                                        if let Some(dy) = Dynamic::from_id(&id) {
                                            pending_dyn.push((staff, t, dy));
                                        }
                                    }
                                }
                                "words" => {
                                    let w = k.text.trim();
                                    if !w.is_empty() {
                                        words_here = Some(match words_here.take() {
                                            Some(prev) => format!("{prev} {w}"),
                                            None => w.to_string(),
                                        });
                                    }
                                }
                                "rehearsal" => {
                                    if first_part && let Some(info) = score.measures.get_mut(mi) {
                                        info.rehearsal = Some(clip(k.text.trim()));
                                    }
                                }
                                "segno" => {
                                    if first_part && let Some(info) = score.measures.get_mut(mi) {
                                        info.jumps.push(Jump::Segno);
                                    }
                                }
                                "coda" => {
                                    if first_part && let Some(info) = score.measures.get_mut(mi) {
                                        let j = if k.attr("smufl") == Some("codaSquare") {
                                            Jump::VariedCoda
                                        } else {
                                            Jump::Coda
                                        };
                                        info.jumps.push(j);
                                    }
                                }
                                "metronome" => {
                                    if first_part
                                        && let Some(bpm) = k.child_num("per-minute")
                                        && let Some(info) = score.measures.get_mut(mi)
                                    {
                                        info.tempo = Some(Tempo {
                                            bpm: bpm.clamp(10.0, 400.0) as u16,
                                        });
                                    }
                                }
                                "wedge" => {
                                    let key = format!("wedge{}", k.attr("number").unwrap_or("1"));
                                    match k.attr("type") {
                                        Some("crescendo") => dir_marks.push((
                                            staff,
                                            t,
                                            DirMark::Start {
                                                key,
                                                kind: SpannerKind::Crescendo,
                                            },
                                        )),
                                        Some("diminuendo") => dir_marks.push((
                                            staff,
                                            t,
                                            DirMark::Start {
                                                key,
                                                kind: SpannerKind::Diminuendo,
                                            },
                                        )),
                                        Some("stop") => dir_marks.push((staff, t, DirMark::Stop { key })),
                                        _ => {}
                                    }
                                }
                                "octave-shift" => {
                                    let key = format!("8va{}", k.attr("number").unwrap_or("1"));
                                    let size =
                                        k.attr("size").and_then(|s| s.parse::<i32>().ok()).unwrap_or(8);
                                    let oct = match size {
                                        15 => 2,
                                        22 => 3,
                                        _ => 1,
                                    };
                                    match k.attr("type") {
                                        // "down": the notes are printed lower than they sound → 8va.
                                        Some("down") => dir_marks.push((
                                            staff,
                                            t,
                                            DirMark::Start {
                                                key,
                                                kind: SpannerKind::Ottava(oct),
                                            },
                                        )),
                                        Some("up") => dir_marks.push((
                                            staff,
                                            t,
                                            DirMark::Start {
                                                key,
                                                kind: SpannerKind::Ottava(-oct),
                                            },
                                        )),
                                        Some("stop") => dir_marks.push((staff, t, DirMark::Stop { key })),
                                        _ => {}
                                    }
                                }
                                "pedal" => {
                                    let key = "pedal".to_string();
                                    match k.attr("type") {
                                        Some("start") => dir_marks.push((
                                            staff,
                                            t,
                                            DirMark::Start {
                                                key,
                                                kind: SpannerKind::Pedal,
                                            },
                                        )),
                                        Some("stop") => dir_marks.push((staff, t, DirMark::Stop { key })),
                                        _ => {}
                                    }
                                }
                                "dashes" | "bracket" => {
                                    let key = format!("line{}", k.attr("number").unwrap_or("1"));
                                    match k.attr("type") {
                                        Some("start") => {
                                            let w =
                                                words_here.clone().unwrap_or_default().to_ascii_lowercase();
                                            let kind = if w.starts_with("cresc") {
                                                Some(SpannerKind::CrescendoLine)
                                            } else if w.starts_with("dim") || w.starts_with("decresc") {
                                                Some(SpannerKind::DiminuendoLine)
                                            } else if w.starts_with("let ring") {
                                                Some(SpannerKind::LetRing)
                                            } else if w.starts_with("p.m") {
                                                Some(SpannerKind::PalmMute)
                                            } else {
                                                None
                                            };
                                            if let Some(kind) = kind {
                                                words_here = None;
                                                dir_marks.push((staff, t, DirMark::Start { key, kind }));
                                            }
                                        }
                                        Some("stop") => dir_marks.push((staff, t, DirMark::Stop { key })),
                                        _ => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    let has_metronome = c.path(&["direction-type", "metronome"]).is_some();
                    if let Some(s) = c.child("sound")
                        && first_part
                        && let Some(tempo) = s.attr("tempo").and_then(|v| v.parse::<f64>().ok())
                        && tempo.is_finite()
                    {
                        let bpm = tempo.clamp(10.0, 400.0) as u16;
                        // A tempo word that repeats the current tempo doesn't change it.
                        let current = if mi == 0 {
                            None
                        } else {
                            Some(score.tempo_at(mi - 1))
                        };
                        if (has_metronome || current != Some(bpm))
                            && let Some(info) = score.measures.get_mut(mi)
                        {
                            info.tempo = Some(Tempo { bpm });
                        }
                    }
                    if let Some(w) = words_here {
                        if let Some(j) = jump_from_words(&w) {
                            if first_part && let Some(info) = score.measures.get_mut(mi) {
                                info.jumps.push(j);
                            }
                        } else if has_tempo {
                            if first_part && let Some(info) = score.measures.get_mut(mi) {
                                info.tempo_text = Some(clip(&w));
                            }
                        } else if system_text {
                            if first_part && let Some(info) = score.measures.get_mut(mi) {
                                info.text = Some(clip(&w));
                            }
                        } else {
                            pending_text.push((staff, t, w, false));
                        }
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
                    if let Some(e) = c.child("ending") {
                        let num = e
                            .attr("number")
                            .and_then(|n| n.split([',', ' ']).next())
                            .and_then(|n| n.trim().parse::<u8>().ok())
                            .unwrap_or(1)
                            .clamp(1, 9);
                        match e.attr("type") {
                            Some("start") => open_ending = Some((num, mi)),
                            Some("stop") | Some("discontinue") => {
                                let (n, from) = open_ending.take().unwrap_or((num, mi));
                                for m in from..=mi {
                                    if let Some(x) = score.measures.get_mut(m) {
                                        x.volta = Some(n);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        // Attach dynamics and texts to the event that starts at (or right after) their position.
        let target = |raws: &mut Vec<Raw>, staff: usize, at: u64| -> Option<usize> {
            let s = staff.min(staves - 1);
            raws.iter()
                .enumerate()
                .filter(|(_, r)| r.staff == s && r.onset >= at)
                .min_by_key(|(_, r)| (r.onset, r.voice))
                .map(|(i, _)| i)
                .or_else(|| {
                    raws.iter()
                        .enumerate()
                        .filter(|(_, r)| r.staff == s)
                        .max_by_key(|(_, r)| r.onset)
                        .map(|(i, _)| i)
                })
        };
        for (staff, at, dy) in pending_dyn {
            if let Some(i) = target(&mut raws, staff, at)
                && let Some(r) = raws.get_mut(i)
            {
                r.event.dynamic.get_or_insert(dy);
            }
        }
        for (staff, at, text, chord) in pending_text {
            if let Some(i) = target(&mut raws, staff, at)
                && let Some(r) = raws.get_mut(i)
            {
                if chord {
                    r.event.chord_symbol = Some(text);
                } else {
                    r.event.text = Some(match r.event.text.take() {
                        Some(prev) => clip(&format!("{prev} {text}")),
                        None => clip(&text),
                    });
                }
            }
        }

        // Group into staves and voices; voice numbers map to indices in ascending order per staff.
        let mut pm = PartMeasure { staves: Vec::new() };
        // Where each raw ended up: (staff, voice index, onset tick).
        let mut placed: Vec<Option<(usize, usize, u32)>> = vec![None; raws.len()];
        for s in 0..staves {
            let mut sm = StaffMeasure {
                voices: Vec::new(),
                clef: clef_changes.get(&s).copied(),
                measure_repeat: repeat_staves.get(s).copied().unwrap_or(false),
            };
            let mut numbers: Vec<u32> = raws.iter().filter(|r| r.staff == s).map(|r| r.voice).collect();
            numbers.sort_unstable();
            numbers.dedup();
            for (vi, vn) in numbers.into_iter().take(limits::MAX_VOICES).enumerate() {
                let mut v = Voice::default();
                let mut pos: u64 = 0;
                let mut items: Vec<(usize, &Raw)> = raws
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.staff == s && r.voice == vn)
                    .collect();
                items.sort_by_key(|(_, r)| r.onset);
                for (ri, r) in items {
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
                    if let Some(p) = placed.get_mut(ri) {
                        *p = Some((s, vi, pos.min(u64::from(u32::MAX)) as u32));
                    }
                    pos = pos.saturating_add(r.ticks);
                    if v.events.len() >= limits::MAX_EVENTS_PER_VOICE {
                        break;
                    }
                }
                sm.voices.push(v);
            }
            pm.staves.push(sm);
        }

        // Lines on notes.
        for (ri, r) in raws.iter().enumerate() {
            let Some(Some((s, v, tick))) = placed.get(ri).copied() else {
                continue;
            };
            let here = Anchor { measure: mi, tick };
            for (fam, num) in &r.stops {
                if let Some((kind, start)) = open_note.remove(&(s, v, *fam, num.clone())) {
                    score.spanners.push(Spanner {
                        kind,
                        part: pi,
                        staff: s,
                        voice: v,
                        start,
                        end: here,
                    });
                }
            }
            for (fam, num, kind) in &r.starts {
                open_note.insert((s, v, *fam, num.clone()), (*kind, here));
            }
        }
        // Lines from directions, anchored to voice 1 of their staff.
        for (staff, at, mark) in dir_marks {
            let s = staff.min(staves - 1);
            let onsets: Vec<u32> = pm
                .staves
                .get(s)
                .and_then(|sm| sm.voices.first())
                .map(|v| v.onsets())
                .unwrap_or_default();
            let at32 = at.min(u64::from(u32::MAX)) as u32;
            match mark {
                DirMark::Start { key, kind } => {
                    let tick = onsets
                        .iter()
                        .copied()
                        .find(|o| *o >= at32)
                        .or(onsets.last().copied())
                        .unwrap_or(0);
                    open_dir.insert((s, key), (kind, 0, Anchor { measure: mi, tick }));
                }
                DirMark::Stop { key } => {
                    let tick = onsets
                        .iter()
                        .copied()
                        .filter(|o| *o < at32)
                        .max()
                        .or(onsets.first().copied())
                        .unwrap_or(0);
                    if let Some((kind, v, start)) = open_dir.remove(&(s, key)) {
                        score.spanners.push(Spanner {
                            kind,
                            part: pi,
                            staff: s,
                            voice: v,
                            start,
                            end: Anchor { measure: mi, tick },
                        });
                    }
                }
            }
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
        sound: Default::default(),
    })
}
