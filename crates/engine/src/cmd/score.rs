//! Score-wide commands: new score, properties, measures, signatures, parts.

use serde_json::{Value, json};

use scoreart_model::{
    Barline, BeatUnit, Clef, Instrument, InstrumentChange, KeySig, Line, PageFormat, PartSound, Rearrange,
    Score, TPQ, Tempo, TimeSig, TimeSymbol, clip, instrument, limits,
};

use super::{CommandSpec, always, need_i64, need_str, p_i64, p_str, p_usize};
use crate::{Error, Result, Selection, Session, View};

fn has_parts(s: &Session) -> std::result::Result<(), String> {
    if s.score.parts.is_empty() {
        Err("Add an instrument first".into())
    } else {
        Ok(())
    }
}

/// The measures a measure-level command applies to: the selected range, or the focus measure.
fn target_measures(s: &Session) -> (usize, usize) {
    match s.selection {
        Some(Selection::Measures { first, last, .. }) => (first, last),
        _ => {
            let (_, _, m) = s.focus();
            (m, m)
        }
    }
}

fn parse_time(t: &str) -> Result<TimeSig> {
    match t.trim() {
        "c" | "C" | "common" => Ok(TimeSig {
            symbol: TimeSymbol::Common,
            ..TimeSig::default()
        }),
        "cut" | "alla-breve" => Ok(TimeSig {
            beats: 2,
            beat_type: 2,
            symbol: TimeSymbol::Cut,
        }),
        other => {
            let (a, b) = other
                .split_once('/')
                .ok_or_else(|| Error::Param(format!("time like 3/4, not {other:?}")))?;
            let beats = a
                .trim()
                .parse::<i64>()
                .map_err(|_| Error::Param("beats".into()))?;
            let bt = b
                .trim()
                .parse::<i64>()
                .map_err(|_| Error::Param("beat type".into()))?;
            Ok(TimeSig::new(beats, bt)?)
        }
    }
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "file.new",
            label: "New score",
            menu: &["File"],
            shortcut: Some("Ctrl+N"),
            params: "title?, composer?, instruments? ([ids], default [\"piano\"]), names? ([names of those instruments, e.g. translated]), time? (\"4/4\"), key? (fifths -7…7), measures? (16), tempo? (beats per minute; in 6/8, 9/8, 12/8 dotted quarters)",
            enabled: always,
            run: |s, p| {
                let ids: Vec<String> = match p.get("instruments") {
                    Some(Value::Array(a)) => {
                        a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()
                    }
                    Some(Value::String(one)) => one.split(',').map(|x| x.trim().to_string()).collect(),
                    _ => vec!["piano".into()],
                };
                let mut insts: Vec<&Instrument> = Vec::new();
                for id in ids.iter().take(limits::MAX_PARTS) {
                    insts.push(
                        instrument(id).ok_or_else(|| Error::Param(format!("unknown instrument {id}")))?,
                    );
                }
                let time = parse_time(p_str(p, "time").unwrap_or("4/4"))?;
                let key = KeySig::new(p_i64(p, "key").unwrap_or(0).clamp(-7, 7) as i32);
                let measures = p_usize(p, "measures")
                    .unwrap_or(16)
                    .clamp(1, limits::MAX_MEASURES);
                let mut score = Score::new(
                    p_str(p, "title").unwrap_or("Untitled score"),
                    &insts,
                    time,
                    key,
                    measures,
                );
                score.meta.composer = clip(p_str(p, "composer").unwrap_or(""));
                score.meta.created = s.today_iso().unwrap_or_default();
                if let Some(Value::Array(names)) = p.get("names") {
                    for (part, n) in score.parts.iter_mut().zip(names) {
                        if let Some(n) = n.as_str().filter(|n| !n.trim().is_empty()) {
                            part.name = clip(n);
                        }
                    }
                }
                if let Some(t) = p_i64(p, "tempo")
                    && let Some(m) = score.measures.first_mut()
                {
                    // Compound meters (6/8, 9/8, 12/8) count dotted quarters.
                    let compound = time.beat_type == 8 && time.beats.is_multiple_of(3) && time.beats > 3;
                    m.tempo = Some(Tempo {
                        bpm: t.clamp(10, 400) as u16,
                        unit: if compound {
                            BeatUnit::DottedQuarter
                        } else {
                            BeatUnit::Quarter
                        },
                    });
                }
                let view = s.view;
                let sounds = std::mem::take(&mut s.sounds);
                *s = Session {
                    view,
                    sounds,
                    clock: s.clock,
                    date_style: s.date_style,
                    ..Session::new(score)
                };
                s.dirty = true;
                Ok(Value::Null)
            },
            undoable: false,
        },
        CommandSpec {
            id: "score.meta",
            label: "Score properties",
            menu: &["Format"],
            shortcut: None,
            params: "title?, subtitle?, composer?, lyricist?, copyright?",
            enabled: always,
            run: |s, p| {
                let m = &mut s.score.meta;
                for (k, field) in [
                    ("title", &mut m.title),
                    ("subtitle", &mut m.subtitle),
                    ("composer", &mut m.composer),
                    ("lyricist", &mut m.lyricist),
                    ("copyright", &mut m.copyright),
                ] {
                    if let Some(v) = p_str(p, k) {
                        *field = clip(v);
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.append",
            label: "Append measures",
            menu: &["Add", "Measures"],
            shortcut: Some("Ctrl+B"),
            params: "count? (1)",
            enabled: always,
            run: |s, p| {
                let n = p_usize(p, "count").unwrap_or(1).clamp(1, 1000);
                let at = s.score.measures.len();
                s.score.insert_measures(at, n)?;
                Ok(json!({ "measures": s.score.measures.len() }))
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.insert",
            label: "Insert measures before selection",
            menu: &["Add", "Measures"],
            shortcut: Some("Ins"),
            params: "count? (1)",
            enabled: always,
            run: |s, p| {
                let n = p_usize(p, "count").unwrap_or(1).clamp(1, 1000);
                let (m, _) = target_measures(s);
                s.score.insert_measures(m, n)?;
                s.selection = None;
                Ok(json!({ "measures": s.score.measures.len() }))
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.delete",
            label: "Delete selected measures",
            menu: &["Edit"],
            shortcut: Some("Ctrl+Del"),
            params: "",
            enabled: |s| {
                if s.score.measures.len() > 1 {
                    Ok(())
                } else {
                    Err("A score needs at least one measure".into())
                }
            },
            run: |s, _| {
                let (a, b) = target_measures(s);
                s.score.delete_measures(a, b - a + 1)?;
                s.selection = None;
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.time",
            label: "Time signature",
            menu: &[],
            shortcut: None,
            params: "time: \"3/4\", \"6/8\", \"common\", \"cut\" … (from the selected measure)",
            enabled: always,
            run: |s, p| {
                let t = parse_time(need_str(p, "time")?)?;
                let (m, _) = target_measures(s);
                if let Some(info) = s.score.measures.get_mut(m) {
                    info.time = Some(t);
                }
                s.score.normalize();
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.key",
            label: "Key signature",
            menu: &[],
            shortcut: None,
            params: "fifths: -7 (7 flats) … 7 (7 sharps)",
            enabled: always,
            run: |s, p| {
                let k = KeySig::new(need_i64(p, "fifths")?.clamp(-7, 7) as i32);
                let (m, _) = target_measures(s);
                if let Some(info) = s.score.measures.get_mut(m) {
                    info.key = Some(k);
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.clef",
            label: "Clef",
            menu: &[],
            shortcut: None,
            params: "clef: treble|treble-8vb|bass|alto|tenor|percussion (selected staff, from the selected measure)",
            enabled: has_parts,
            run: |s, p| {
                let c = need_str(p, "clef")?;
                let clef = Clef::from_id(c).ok_or_else(|| Error::Param(format!("unknown clef {c}")))?;
                let (part, staff, m) = s.focus();
                if m == 0 {
                    if let Some(slot) = s.score.parts.get_mut(part).and_then(|pt| pt.clefs.get_mut(staff)) {
                        *slot = clef;
                    }
                } else if let Some(sm) = s.score.staff_measure_mut(part, staff, m) {
                    sm.clef = Some(clef);
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.barline",
            label: "Barline",
            menu: &[],
            shortcut: None,
            params: "type: regular|double|final|repeat-end (end of the selected measure)",
            enabled: always,
            run: |s, p| {
                let b = match need_str(p, "type")? {
                    "regular" => Barline::Regular,
                    "double" => Barline::Double,
                    "final" => Barline::Final,
                    "repeat-end" => Barline::RepeatEnd,
                    other => return Err(Error::Param(format!("unknown barline {other}"))),
                };
                let (_, m) = target_measures(s);
                if let Some(info) = s.score.measures.get_mut(m) {
                    info.barline = b;
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.repeat-start",
            label: "Start repeat",
            menu: &[],
            shortcut: None,
            params: "",
            enabled: always,
            run: |s, _| {
                let (m, _) = target_measures(s);
                if let Some(info) = s.score.measures.get_mut(m) {
                    info.repeat_start = !info.repeat_start;
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.tempo",
            label: "Tempo",
            menu: &["Add", "Text"],
            shortcut: None,
            params: "bpm: beats per minute (0 removes), unit? eighth|dotted-eighth|quarter|dotted-quarter|half|dotted-half (default quarter; e.g. dotted-quarter for 6/8)",
            enabled: always,
            run: |s, p| {
                let bpm = need_i64(p, "bpm")?;
                let unit = match p_str(p, "unit") {
                    None => BeatUnit::Quarter,
                    Some(u) => {
                        BeatUnit::from_id(u).ok_or_else(|| Error::Param(format!("no beat unit {u:?}")))?
                    }
                };
                let (m, _) = target_measures(s);
                if let Some(info) = s.score.measures.get_mut(m) {
                    info.tempo = if bpm <= 0 {
                        None
                    } else {
                        Some(Tempo {
                            bpm: bpm.clamp(10, 400) as u16,
                            unit,
                        })
                    };
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.length",
            label: "Measure length (pickup)",
            menu: &[],
            shortcut: None,
            params: "length: \"1/8\", \"3/4\" … (a fraction of a whole note; \"full\" = as the time signature), counted? true|false (in the measure numbering; a short first measure — a pickup — is not counted by default) — on the selected measures",
            enabled: always,
            run: |s, p| {
                let l = need_str(p, "length")?.trim();
                let ticks = if l == "full" || l.is_empty() {
                    None
                } else {
                    let (a, b) = l
                        .split_once('/')
                        .ok_or_else(|| Error::Param(format!("length like 1/8, not {l:?}")))?;
                    let a: u32 = a.trim().parse().map_err(|_| Error::Param("length".into()))?;
                    let b: u32 = b.trim().parse().map_err(|_| Error::Param("length".into()))?;
                    if a == 0 || !b.is_power_of_two() || b > 64 || a > 64 * b {
                        return Err(Error::Param(format!("no measure length {l}")));
                    }
                    Some(TPQ * 4 * a / b)
                };
                let counted = p.get("counted").and_then(Value::as_bool);
                let (first, last) = target_measures(s);
                for m in first..=last {
                    let full = s.score.time_at(m).measure_ticks();
                    let Some(info) = s.score.measures.get_mut(m) else {
                        continue;
                    };
                    info.actual = ticks.filter(|t| *t != full);
                    info.uncounted = match counted {
                        Some(c) => !c,
                        None => m == 0 && info.actual.is_some_and(|t| t < full),
                    };
                }
                s.score.normalize();
                Ok(json!({ "ticks": ticks }))
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.text",
            label: "System text",
            menu: &["Add", "Text"],
            shortcut: None,
            params: "text (empty removes)",
            enabled: always,
            run: |s, p| {
                let t = p_str(p, "text").unwrap_or("").trim();
                let (m, _) = target_measures(s);
                if let Some(info) = s.score.measures.get_mut(m) {
                    info.text = if t.is_empty() { None } else { Some(clip(t)) };
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "part.add",
            label: "Add instrument",
            menu: &["Add", "Instruments"],
            shortcut: Some("I"),
            params: "name? (shown name, default the instrument's), instrument: piano|organ|voice|soprano|alto|tenor|bass-voice|women|men|choir|flute|oboe|bassoon|violin|viola|cello|contrabass|guitar|trombone|tuba",
            enabled: |s| {
                if s.score.parts.len() < limits::MAX_PARTS {
                    Ok(())
                } else {
                    Err("Too many instruments".into())
                }
            },
            run: |s, p| {
                let id = need_str(p, "instrument")?;
                let inst = instrument(id).ok_or_else(|| Error::Param(format!("unknown instrument {id}")))?;
                s.score.add_part(inst);
                if let Some(n) = p_str(p, "name").filter(|n| !n.trim().is_empty())
                    && let Some(part) = s.score.parts.last_mut()
                {
                    part.name = clip(n);
                }
                Ok(json!({ "part": s.score.parts.len() - 1 }))
            },
            undoable: true,
        },
        CommandSpec {
            id: "part.remove",
            label: "Remove instrument",
            menu: &["Edit"],
            shortcut: None,
            params: "part? (index; default: the selected part)",
            enabled: |s| {
                if s.score.parts.len() > 1 {
                    Ok(())
                } else {
                    Err("A score needs at least one instrument".into())
                }
            },
            run: |s, p| {
                let part = p_usize(p, "part").unwrap_or_else(|| s.focus().0);
                if part >= s.score.parts.len() {
                    return Err(Error::Param("no such part".into()));
                }
                s.score.remove_part(part)?;
                s.selection = None;
                s.cursor = Default::default();
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "part.move",
            label: "Move instrument",
            menu: &[],
            shortcut: None,
            params: "to (index), part? (default: the selected part)",
            enabled: has_parts,
            run: |s, p| {
                let part = p_usize(p, "part").unwrap_or_else(|| s.focus().0);
                let to = p_usize(p, "to").ok_or_else(|| Error::Param("missing to".into()))?;
                s.score
                    .move_part(part, to.min(s.score.parts.len().saturating_sub(1)))?;
                s.selection = None;
                s.cursor = Default::default();
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "part.rename",
            label: "Rename instrument",
            menu: &[],
            shortcut: None,
            params: "name, abbreviation?, part? (default: the selected part)",
            enabled: has_parts,
            run: |s, p| {
                let part = p_usize(p, "part").unwrap_or_else(|| s.focus().0);
                let name = need_str(p, "name")?;
                let pt = s
                    .score
                    .parts
                    .get_mut(part)
                    .ok_or_else(|| Error::Param("no such part".into()))?;
                pt.name = clip(name);
                if let Some(a) = p_str(p, "abbreviation") {
                    pt.abbreviation = clip(a);
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "part.change",
            label: "Change instrument",
            menu: &[],
            shortcut: None,
            params: "instrument (id: piano, violin, flute …), part? (default: the selected part), name? (shown name), mode?: merge (default: every line goes into the new instrument, extra staves become voices) | split (staff/voice becomes a new part with the instrument, the rest stays) | discard (only staff/voice is kept), staff?, voice? (1…, the line for split and discard; default 1)",
            enabled: has_parts,
            run: |s, p| {
                let part = p_usize(p, "part").unwrap_or_else(|| s.focus().0);
                let id = need_str(p, "instrument")?;
                let inst = instrument(id).ok_or_else(|| Error::Param(format!("unknown instrument {id}")))?;
                let keep = Line {
                    staff: p_usize(p, "staff").unwrap_or(1).saturating_sub(1),
                    voice: p_usize(p, "voice").unwrap_or(1).saturating_sub(1),
                };
                let how = match p_str(p, "mode").unwrap_or("merge") {
                    "merge" => Rearrange::Merge,
                    "split" => Rearrange::Split { keep },
                    "discard" => Rearrange::Discard { keep },
                    m => return Err(Error::Param(format!("unknown mode {m} (merge, split, discard)"))),
                };
                if part >= s.score.parts.len() {
                    return Err(Error::Param("no such part".into()));
                }
                let now = s.score.rearrange_part(part, inst, how)?;
                if let Some(n) = p_str(p, "name").filter(|n| !n.trim().is_empty())
                    && let Some(pt) = s.score.parts.get_mut(now)
                {
                    pt.name = clip(n);
                }
                s.selection = None;
                s.cursor = Default::default();
                Ok(json!({ "part": now }))
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.instrument-change",
            label: "Instrument change",
            menu: &[],
            shortcut: None,
            params: "instrument (id; \"\" removes), part?, measure? (default: the selection). The part plays the new instrument from this measure on (\"To Flute\")",
            enabled: has_parts,
            run: |s, p| {
                let (fp, _, fm) = s.focus();
                let part = p_usize(p, "part").unwrap_or(fp);
                let measure = p_usize(p, "measure").unwrap_or(fm);
                let id = p_str(p, "instrument").unwrap_or("");
                let change = if id.is_empty() {
                    None
                } else {
                    let inst =
                        instrument(id).ok_or_else(|| Error::Param(format!("unknown instrument {id}")))?;
                    Some(InstrumentChange {
                        name: inst.name.to_string(),
                        abbreviation: inst.abbreviation.to_string(),
                        program: inst.program,
                    })
                };
                let pm = s
                    .score
                    .parts
                    .get_mut(part)
                    .and_then(|pt| pt.measures.get_mut(measure))
                    .ok_or_else(|| Error::Param("no such part or measure".into()))?;
                pm.instrument_change = change;
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "part.visible",
            label: "Show or hide instrument",
            menu: &[],
            shortcut: None,
            params: "visible: true|false, staff? (1…; default: every staff of the part), part? (default: the selected part). Hidden staves still play",
            enabled: has_parts,
            run: |s, p| {
                let part = p_usize(p, "part").unwrap_or_else(|| s.focus().0);
                let on = p
                    .get("visible")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| Error::Param("visible: true or false".into()))?;
                let pt = s
                    .score
                    .parts
                    .get_mut(part)
                    .ok_or_else(|| Error::Param("no such part".into()))?;
                match p_usize(p, "staff") {
                    Some(st) if st >= 1 && st <= pt.staff_count() => {
                        let n = pt.staff_count();
                        if pt.hidden.len() < n {
                            pt.hidden.resize(n, false);
                        }
                        if let Some(h) = pt.hidden.get_mut(st - 1) {
                            *h = !on;
                        }
                        if pt.hidden.iter().all(|h| !h) {
                            pt.hidden.clear();
                        }
                    }
                    Some(_) => return Err(Error::Param("no such staff".into())),
                    None => pt.set_visible(on),
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "part.harmony",
            label: "Chord symbol playback",
            menu: &[],
            shortcut: None,
            params: "any of: play (true|false), instrument (an instrument id, e.g. piano, guitar) or program (General MIDI 0…127), volume (0…100); part? (default: the selected part) — how the part's chord symbols sound",
            enabled: has_parts,
            run: |s, p| {
                let part = p_usize(p, "part").unwrap_or_else(|| s.focus().0);
                let program = match p.get("instrument").and_then(Value::as_str) {
                    Some(id) => Some(
                        scoreart_model::INSTRUMENTS
                            .iter()
                            .find(|i| i.id == id)
                            .map(|i| i.program)
                            .ok_or_else(|| Error::Param(format!("no such instrument: {id}")))?,
                    ),
                    None => p_usize(p, "program").map(|v| v.min(127) as u8),
                };
                let h = &mut s
                    .score
                    .parts
                    .get_mut(part)
                    .ok_or_else(|| Error::Param("no such part".into()))?
                    .harmony;
                if let Some(on) = p.get("play").and_then(Value::as_bool) {
                    h.play = on;
                }
                if let Some(pr) = program {
                    h.program = pr;
                }
                if let Some(v) = p_usize(p, "volume") {
                    h.volume = v.min(100) as u8;
                }
                Ok(json!({ "play": h.play, "program": h.program, "volume": h.volume }))
            },
            undoable: true,
        },
        CommandSpec {
            id: "part.hide_empty",
            label: "Hide empty staves",
            menu: &[],
            shortcut: None,
            params: "hide: true|false, part? (default: the selected part) — leave the part's staves out of systems in which they have nothing to play",
            enabled: has_parts,
            run: |s, p| {
                let part = p_usize(p, "part").unwrap_or_else(|| s.focus().0);
                let on = p
                    .get("hide")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| Error::Param("hide: true or false".into()))?;
                let pt = s
                    .score
                    .parts
                    .get_mut(part)
                    .ok_or_else(|| Error::Param("no such part".into()))?;
                pt.hide_empty = if on {
                    vec![true; pt.staff_count()]
                } else {
                    Vec::new()
                };
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.break",
            label: "Line or page break",
            menu: &[],
            shortcut: None,
            params: "kind: line|page|section|none (toggles), measure? (default: the selection) — a new system or page starts after the measure; a section also: pause? (seconds before the next section, default 3), long_names? (true: the next system shows the long instrument names), restart_numbers? (true: measures count from 1 again); given with an existing section break they change it",
            enabled: always,
            run: |s, p| {
                let (_, last) = target_measures(s);
                let m = p_usize(p, "measure").unwrap_or(last);
                let kind = p_str(p, "kind").unwrap_or("line");
                let info = s
                    .score
                    .measures
                    .get_mut(m)
                    .ok_or_else(|| Error::Param("no such measure".into()))?;
                match kind {
                    "line" => {
                        info.system_break = !info.system_break;
                        info.page_break = false;
                        info.section = None;
                    }
                    "page" => {
                        info.page_break = !info.page_break;
                        info.system_break = false;
                        info.section = None;
                    }
                    "section" => {
                        let settings = ["pause", "long_names", "restart_numbers"]
                            .iter()
                            .any(|k| p.get(*k).is_some());
                        let mut sec = match info.section {
                            // Toggled off, unless the call only changes its settings.
                            Some(_) if !settings => {
                                info.section = None;
                                return Ok(Value::Null);
                            }
                            Some(sec) => sec,
                            None => scoreart_model::SectionBreak::default(),
                        };
                        if let Some(v) = p.get("pause").and_then(Value::as_f64).filter(|v| v.is_finite()) {
                            sec.pause = v.clamp(0.0, 60.0) as f32;
                        }
                        if let Some(v) = p.get("long_names").and_then(Value::as_bool) {
                            sec.long_names = v;
                        }
                        if let Some(v) = p.get("restart_numbers").and_then(Value::as_bool) {
                            sec.restart_numbers = v;
                        }
                        info.section = Some(sec);
                        info.system_break = false;
                    }
                    "none" => {
                        info.system_break = false;
                        info.page_break = false;
                        info.section = None;
                    }
                    other => {
                        return Err(Error::Param(format!(
                            "kind: line, page, section or none, not {other}"
                        )));
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "staff.spacer",
            label: "Spacer",
            menu: &[],
            shortcut: None,
            params: "type: up|down|fixed (least room above / below the staff, or exactly this much below it) or none (removes all three), height? (staff spaces, default 3; 0 removes that spacer) — on the selected measures of the selected staff",
            enabled: has_parts,
            run: |s, p| {
                let ty = need_str(p, "type")?;
                let height = p
                    .get("height")
                    .and_then(Value::as_f64)
                    .filter(|v| v.is_finite())
                    .map_or(3.0, |v| v.clamp(0.0, 100.0) as f32);
                let (part, staff, _) = s.focus();
                let (first, last) = target_measures(s);
                for m in first..=last {
                    let Some(sm) = s
                        .score
                        .parts
                        .get_mut(part)
                        .and_then(|pt| pt.measures.get_mut(m))
                        .and_then(|pm| pm.staves.get_mut(staff))
                    else {
                        return Err(Error::Param("no such staff".into()));
                    };
                    let v = (height > 0.0).then_some(height);
                    match ty {
                        "up" => sm.spacer_up = v,
                        "down" => {
                            sm.spacer_down = v;
                            sm.spacer_fixed = None;
                        }
                        "fixed" => {
                            sm.spacer_fixed = v;
                            sm.spacer_down = None;
                        }
                        "none" => {
                            sm.spacer_up = None;
                            sm.spacer_down = None;
                            sm.spacer_fixed = None;
                        }
                        other => {
                            return Err(Error::Param(format!(
                                "type: up, down, fixed or none, not {other}"
                            )));
                        }
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "part.sound",
            label: "Choose sound",
            menu: &[],
            shortcut: None,
            params: "library (\"\" = automatic, \"ScoreArt\" = built-in, or a loaded library), preset?, part? (default: the selected part)",
            enabled: has_parts,
            run: |s, p| {
                let part = p_usize(p, "part").unwrap_or_else(|| s.focus().0);
                let library = clip(p_str(p, "library").unwrap_or(""));
                let preset = clip(p_str(p, "preset").unwrap_or(""));
                let known = library.is_empty()
                    || library == PartSound::BUILT_IN
                    || s.sounds.iter().any(|b| b.name == library);
                if !known {
                    return Err(Error::Param(format!("sound library \"{library}\" is not loaded")));
                }
                let pt = s
                    .score
                    .parts
                    .get_mut(part)
                    .ok_or_else(|| Error::Param("no such part".into()))?;
                pt.sound.library = library;
                pt.sound.preset = preset;
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "part.mix",
            label: "Instrument volume and pan",
            menu: &[],
            shortcut: None,
            params: "volume? 0…100, pan? -100 (left)…100 (right), part? (default: the selected part)",
            enabled: has_parts,
            run: |s, p| {
                let part = p_usize(p, "part").unwrap_or_else(|| s.focus().0);
                let pt = s
                    .score
                    .parts
                    .get_mut(part)
                    .ok_or_else(|| Error::Param("no such part".into()))?;
                if let Some(v) = p_i64(p, "volume") {
                    pt.sound.volume = v.clamp(0, 100) as u8;
                }
                if let Some(v) = p_i64(p, "pan") {
                    pt.sound.pan = v.clamp(-100, 100) as i8;
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "score.playback",
            label: "Playback style",
            menu: &[],
            shortcut: None,
            params: "reverb? 0…100 (the room), humanize? true|false (small natural variations), optimize? true|false (sound optimisation: shaped notes, matching samples, balanced sounds)",
            enabled: always,
            run: |s, p| {
                if let Some(v) = p_i64(p, "reverb") {
                    s.score.playback.reverb = v.clamp(0, 100) as u8;
                }
                if let Some(v) = p.get("humanize").and_then(Value::as_bool) {
                    s.score.playback.humanize = v;
                }
                if let Some(v) = p.get("optimize").and_then(Value::as_bool) {
                    s.score.playback.optimize = v;
                }
                let pb = s.score.playback;
                Ok(json!({ "reverb": pb.reverb, "humanize": pb.humanize, "optimize": pb.optimize }))
            },
            undoable: true,
        },
        CommandSpec {
            id: "sound.list",
            label: "List sound libraries",
            menu: &[],
            shortcut: None,
            params: "",
            enabled: always,
            run: |s, _| {
                let libs: Vec<Value> = s
                    .sounds
                    .iter()
                    .map(|b| {
                        json!({
                            "name": b.name,
                            "presets": b.presets().into_iter().map(|p| p.name).collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                Ok(Value::Array(libs))
            },
            undoable: false,
        },
        CommandSpec {
            id: "score.transpose",
            label: "Transpose",
            menu: &["Tools"],
            shortcut: None,
            params: "semitones: -24…24",
            enabled: always,
            run: |s, p| {
                let n = need_i64(p, "semitones")?.clamp(-24, 24) as i32;
                s.score.transpose(n);
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "style.spacing",
            label: "Note spacing",
            menu: &["Format"],
            shortcut: None,
            params: "factor: 0.5…2.0 (1.0 = default)",
            enabled: always,
            run: |s, p| {
                let f = p
                    .get("factor")
                    .and_then(Value::as_f64)
                    .filter(|f| f.is_finite())
                    .ok_or_else(|| Error::Param("factor".into()))?;
                s.score.page.spacing = f.clamp(0.5, 2.0) as f32;
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "page.format",
            label: "Page settings",
            menu: &["Format"],
            shortcut: None,
            params: "any of: size (A4, Letter, A3, A5, B4, Legal, Tabloid), orientation (portrait|landscape), width_mm, height_mm, margin_mm (all four), margin_top_mm, margin_bottom_mm, margin_left_mm, margin_right_mm (odd pages, or all pages when not two-sided), two_sided (true|false), even_margin_top_mm, even_margin_bottom_mm, even_margin_left_mm, even_margin_right_mm, staff_space_mm (staff size), staff_gap (staves of one instrument), part_gap (between instruments), system_gap (least system distance), max_system_gap, music_top, music_bottom (staff spaces from the margin), vertical_justify (true|false), max_system_spread, max_staff_spread, spread_factor, last_system_fill (0…1), first_page_number, first_indent (staff spaces, 0 = none), header_offset, footer_offset, spacing (0.5…2), reset: true (defaults)",
            enabled: always,
            run: |s, p| {
                let num = |k: &str| -> Result<Option<f32>> {
                    match p.get(k) {
                        None | Some(Value::Null) => Ok(None),
                        Some(v) => v
                            .as_f64()
                            .filter(|f| f.is_finite())
                            .map(|f| Some(f as f32))
                            .ok_or_else(|| Error::Param(format!("{k} must be a number"))),
                    }
                };
                let mut f = if p.get("reset").and_then(Value::as_bool) == Some(true) {
                    PageFormat::default()
                } else {
                    s.score.page
                };
                if let Some(name) = p_str(p, "size") {
                    let (_, w, h) = PageFormat::SIZES
                        .iter()
                        .find(|(n, ..)| n.eq_ignore_ascii_case(name))
                        .ok_or_else(|| Error::Param(format!("unknown size {name}")))?;
                    (f.width_mm, f.height_mm) = if f.landscape() { (*h, *w) } else { (*w, *h) };
                }
                if let Some(w) = num("width_mm")? {
                    f.width_mm = w;
                }
                if let Some(h) = num("height_mm")? {
                    f.height_mm = h;
                }
                match p_str(p, "orientation") {
                    None => {}
                    Some("portrait") if f.landscape() => {
                        (f.width_mm, f.height_mm) = (f.height_mm, f.width_mm)
                    }
                    Some("landscape") if !f.landscape() => {
                        (f.width_mm, f.height_mm) = (f.height_mm, f.width_mm)
                    }
                    Some("portrait" | "landscape") => {}
                    Some(o) => return Err(Error::Param(format!("unknown orientation {o}"))),
                }
                if let Some(m) = num("margin_mm")? {
                    f.margin_top_mm = m;
                    f.margin_bottom_mm = m;
                    f.margin_left_mm = m;
                    f.margin_right_mm = m;
                }
                for (k, slot) in [
                    ("margin_top_mm", &mut f.margin_top_mm),
                    ("margin_bottom_mm", &mut f.margin_bottom_mm),
                    ("margin_left_mm", &mut f.margin_left_mm),
                    ("margin_right_mm", &mut f.margin_right_mm),
                    ("staff_space_mm", &mut f.staff_space_mm),
                    ("staff_gap", &mut f.staff_gap),
                    ("part_gap", &mut f.part_gap),
                    ("system_gap", &mut f.system_gap),
                    ("spacing", &mut f.spacing),
                    ("even_margin_top_mm", &mut f.even_margin_top_mm),
                    ("even_margin_bottom_mm", &mut f.even_margin_bottom_mm),
                    ("even_margin_left_mm", &mut f.even_margin_left_mm),
                    ("even_margin_right_mm", &mut f.even_margin_right_mm),
                    ("max_system_gap", &mut f.max_system_gap),
                    ("music_top", &mut f.music_top),
                    ("music_bottom", &mut f.music_bottom),
                    ("max_system_spread", &mut f.max_system_spread),
                    ("max_staff_spread", &mut f.max_staff_spread),
                    ("spread_factor", &mut f.spread_factor),
                    ("last_system_fill", &mut f.last_system_fill),
                    ("first_indent", &mut f.first_indent),
                    ("header_offset", &mut f.header_offset),
                    ("footer_offset", &mut f.footer_offset),
                ] {
                    if let Some(v) = num(k)? {
                        *slot = v;
                    }
                }
                if let Some(v) = num("first_page_number")? {
                    f.first_page_number = v.round() as i32;
                }
                for (k, slot) in [
                    ("two_sided", &mut f.two_sided),
                    ("vertical_justify", &mut f.vertical_justify),
                ] {
                    match p.get(k) {
                        None | Some(Value::Null) => {}
                        Some(Value::Bool(b)) => *slot = *b,
                        Some(_) => return Err(Error::Param(format!("{k}: true or false"))),
                    }
                }
                // Turning two-sided on starts the even pages with the odd pages' margins,
                // mirrored (inside and outside).
                if p.get("two_sided").and_then(Value::as_bool) == Some(true) && !s.score.page.two_sided {
                    let even_given = [
                        "even_margin_top_mm",
                        "even_margin_bottom_mm",
                        "even_margin_left_mm",
                        "even_margin_right_mm",
                    ]
                    .iter()
                    .any(|k| p.get(*k).is_some());
                    if !even_given {
                        f.even_margin_top_mm = f.margin_top_mm;
                        f.even_margin_bottom_mm = f.margin_bottom_mm;
                        f.even_margin_left_mm = f.margin_right_mm;
                        f.even_margin_right_mm = f.margin_left_mm;
                    }
                }
                s.score.page = f.clamped();
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "view.mode",
            label: "View",
            menu: &["View"],
            shortcut: None,
            params: "mode: page | horizontal (one endless system) | vertical (systems without page breaks)",
            enabled: always,
            run: |s, p| {
                s.view = match need_str(p, "mode")? {
                    "page" => View::Page,
                    "horizontal" | "continuous" => View::Horizontal,
                    "vertical" => View::Vertical,
                    m => return Err(Error::Param(format!("unknown view {m}"))),
                };
                s.touch();
                Ok(Value::Null)
            },
            undoable: false,
        },
    ]
}
