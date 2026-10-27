//! Score-wide commands: new score, properties, measures, signatures, parts.

use serde_json::{Value, json};

use scoreart_model::{
    Barline, Clef, Instrument, InstrumentChange, KeySig, PartSound, Score, Tempo, TimeSig, TimeSymbol, clip,
    instrument, limits,
};

use super::{CommandSpec, always, need_i64, need_str, p_i64, p_str, p_usize};
use crate::{Error, Result, Selection, Session};

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
            params: "title?, composer?, instruments? ([ids], default [\"piano\"]), time? (\"4/4\"), key? (fifths -7…7), measures? (16), tempo? (bpm)",
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
                if let Some(t) = p_i64(p, "tempo")
                    && let Some(m) = score.measures.first_mut()
                {
                    m.tempo = Some(Tempo {
                        bpm: t.clamp(10, 400) as u16,
                    });
                }
                let style = s.style.clone();
                let sounds = std::mem::take(&mut s.sounds);
                *s = Session {
                    style,
                    sounds,
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
            params: "bpm: quarter notes per minute (0 removes)",
            enabled: always,
            run: |s, p| {
                let bpm = need_i64(p, "bpm")?;
                let (m, _) = target_measures(s);
                if let Some(info) = s.score.measures.get_mut(m) {
                    info.tempo = if bpm <= 0 {
                        None
                    } else {
                        Some(Tempo {
                            bpm: bpm.clamp(10, 400) as u16,
                        })
                    };
                }
                Ok(Value::Null)
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
            params: "instrument: piano|organ|voice|soprano|alto|tenor|bass-voice|flute|oboe|bassoon|violin|viola|cello|contrabass|guitar|trombone|tuba",
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
                s.score.parts.remove(part);
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
            params: "instrument (id: piano, violin, flute …), part? (default: the selected part). Replaces name, sound and staves of the whole part",
            enabled: has_parts,
            run: |s, p| {
                let part = p_usize(p, "part").unwrap_or_else(|| s.focus().0);
                let id = need_str(p, "instrument")?;
                let inst = instrument(id).ok_or_else(|| Error::Param(format!("unknown instrument {id}")))?;
                let pt = s
                    .score
                    .parts
                    .get_mut(part)
                    .ok_or_else(|| Error::Param("no such part".into()))?;
                pt.change_instrument(inst);
                s.score.normalize();
                s.selection = None;
                s.cursor = Default::default();
                Ok(Value::Null)
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
            id: "measure.break",
            label: "Line or page break",
            menu: &[],
            shortcut: None,
            params: "kind: line|page|none, measure? (default: the selection) — a new system or page starts after the measure",
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
                    }
                    "page" => {
                        info.page_break = !info.page_break;
                        info.system_break = false;
                    }
                    "none" => {
                        info.system_break = false;
                        info.page_break = false;
                    }
                    other => return Err(Error::Param(format!("kind: line, page or none, not {other}"))),
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
                s.style.spacing = f.clamp(0.5, 2.0) as f32;
                s.touch();
                Ok(Value::Null)
            },
            undoable: false,
        },
    ]
}
