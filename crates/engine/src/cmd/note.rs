//! Note input and note editing. Keyboard behaviour follows the common scorewriter convention
//! (letters enter pitches, digits pick durations), which is functional and not specific to any
//! one product.

use serde_json::{Value, json};

use scoreart_model::edit::{EventRef, Pos};
use scoreart_model::{
    Accidental, Articulation, Duration, Dynamic, EventKind, Note, NoteValue, Pitch, Step, clip, limits,
};

use super::edit::{select_event, start_input};
use super::{CommandSpec, always, need_str, p_i64, p_str, p_usize};
use crate::{Error, Result, Selection, Session};

fn has_event_selection(s: &Session) -> std::result::Result<(), String> {
    match s.selection {
        Some(Selection::Event { .. }) => Ok(()),
        _ => Err("Select a note or rest first".into()),
    }
}

fn has_note_selection(s: &Session) -> std::result::Result<(), String> {
    match s.selection {
        Some(Selection::Event { ev, .. }) if s.score.event(ev).is_some_and(|e| !e.is_rest()) => Ok(()),
        _ => Err("Select a note first".into()),
    }
}

fn has_selection(s: &Session) -> std::result::Result<(), String> {
    if s.selection.is_some() {
        Ok(())
    } else {
        Err("Select something first".into())
    }
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "note.enter",
            label: "Enter note",
            menu: &[],
            shortcut: Some("A–G"),
            params: "step: c|d|e|f|g|a|b; octave? (scientific, C4 = middle C; default nearest to the previous note); alter? (-2…2; default from the key)",
            enabled: |s| {
                if s.score.parts.is_empty() {
                    Err("Add an instrument first".into())
                } else {
                    Ok(())
                }
            },
            run: |s, p| {
                let step = parse_step(need_str(p, "step")?)?;
                if !s.input.active {
                    start_input(s);
                }
                let pitch = choose_pitch(s, step, p_i64(p, "octave"), p_i64(p, "alter"));
                enter(s, EventKind::Chord(vec![Note::new(pitch)]))
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.add",
            label: "Add note to chord",
            menu: &[],
            shortcut: Some("Shift+A–G"),
            params: "step; octave?; alter? — adds the pitch above the chord's top note",
            enabled: has_note_selection,
            run: |s, p| {
                let step = parse_step(need_str(p, "step")?)?;
                let Some(Selection::Event { ev, .. }) = s.selection else {
                    return Err(Error::Disabled("no selection".into()));
                };
                let key = s.score.key_at(ev.measure);
                let top = s.score.event(ev).and_then(|e| e.notes().last().map(|n| n.pitch));
                let pitch = match (p_i64(p, "octave"), top) {
                    (Some(o), _) => Pitch::new(step, alter_or_key(p_i64(p, "alter"), key, step), o as i32),
                    (None, Some(t)) => {
                        let mut d = t.diatonic() + 1;
                        while d.rem_euclid(7) != step.index() {
                            d += 1;
                        }
                        Pitch::from_diatonic(d, alter_or_key(p_i64(p, "alter"), key, step))
                    }
                    (None, None) => choose_pitch(s, step, None, p_i64(p, "alter")),
                };
                s.score.add_to_chord(ev, pitch)?;
                let idx = s
                    .score
                    .event(ev)
                    .and_then(|e| e.notes().iter().position(|n| n.pitch == pitch));
                s.selection = Some(Selection::Event { ev, note: idx });
                s.last_pitch = Some(pitch);
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.midi",
            label: "Enter a played note",
            menu: &[],
            shortcut: None,
            params: "key: 0…127 (MIDI note number, 60 = middle C), chord?: true adds it to the chord just entered. Spelled in the key signature",
            enabled: |s| {
                if s.input.active {
                    Ok(())
                } else {
                    Err("Note input is off (N)".into())
                }
            },
            run: |s, p| {
                let key = p_i64(p, "key")
                    .filter(|k| (0..=127).contains(k))
                    .ok_or_else(|| Error::Param("key: 0…127".into()))?;
                let pitch = Pitch::from_midi(key as i32, s.score.key_at(s.cursor.measure));
                if p.get("chord").and_then(Value::as_bool) == Some(true)
                    && let Some(Selection::Event { ev, .. }) = s.selection
                    && s.score.event(ev).is_some_and(|e| !e.is_rest())
                {
                    s.score.add_to_chord(ev, pitch)?;
                    s.last_pitch = Some(pitch);
                    return Ok(Value::Null);
                }
                enter(s, EventKind::Chord(vec![Note::new(pitch)]))
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.rest",
            label: "Enter rest",
            menu: &[],
            shortcut: Some("0"),
            params: "",
            enabled: |s| {
                if s.score.parts.is_empty() {
                    Err("Add an instrument first".into())
                } else {
                    Ok(())
                }
            },
            run: |s, _| {
                if !s.input.active {
                    if let Some(Selection::Event { ev, .. }) = s.selection {
                        s.score.make_rest(ev)?;
                        return Ok(Value::Null);
                    }
                    start_input(s);
                }
                enter(s, EventKind::Rest)
            },
            undoable: true,
        },
        CommandSpec {
            id: "input.duration",
            label: "Duration",
            menu: &[],
            shortcut: Some("1–8"),
            params: "value: 64th|32nd|16th|eighth|quarter|half|whole|breve, or digit 1–8 (5 = quarter)",
            enabled: always,
            run: |s, p| {
                let value = match p.get("value") {
                    Some(Value::Number(n)) => n
                        .as_u64()
                        .and_then(|d| u8::try_from(d).ok())
                        .and_then(NoteValue::from_digit),
                    Some(Value::String(v)) => {
                        NoteValue::from_id(v).or_else(|| v.parse::<u8>().ok().and_then(NoteValue::from_digit))
                    }
                    _ => None,
                }
                .ok_or_else(|| Error::Param("value: 64th…breve or 1–8".into()))?;
                s.input.value = value;
                s.input.dots = 0;
                if !s.input.active
                    && let Some(ev) = s.selected_event()
                {
                    rewrite_duration(s, ev, Duration::new(value, 0))?;
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "input.dot",
            label: "Augmentation dot",
            menu: &[],
            shortcut: Some("."),
            params: "",
            enabled: always,
            run: |s, _| {
                if !s.input.active
                    && let Some(ev) = s.selected_event()
                    && let Some(e) = s.score.event(ev)
                {
                    let d = Duration::new(e.duration.value, if e.duration.dots == 0 { 1 } else { 0 });
                    rewrite_duration(s, ev, d)?;
                    return Ok(Value::Null);
                }
                s.input.dots = if s.input.dots == 0 { 1 } else { 0 };
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.pitch-up",
            label: "Pitch up",
            menu: &[],
            shortcut: Some("Up"),
            params: "",
            enabled: has_note_selection,
            run: |s, _| transpose_selection(s, |p, k| p.transpose_chromatic(1, k)),
            undoable: true,
        },
        CommandSpec {
            id: "note.pitch-down",
            label: "Pitch down",
            menu: &[],
            shortcut: Some("Down"),
            params: "",
            enabled: has_note_selection,
            run: |s, _| transpose_selection(s, |p, k| p.transpose_chromatic(-1, k)),
            undoable: true,
        },
        CommandSpec {
            id: "note.step-up",
            label: "Diatonic step up",
            menu: &[],
            shortcut: Some("Alt+Shift+Up"),
            params: "",
            enabled: has_note_selection,
            run: |s, _| transpose_selection(s, |p, k| p.transpose_diatonic(1, k)),
            undoable: true,
        },
        CommandSpec {
            id: "note.step-down",
            label: "Diatonic step down",
            menu: &[],
            shortcut: Some("Alt+Shift+Down"),
            params: "",
            enabled: has_note_selection,
            run: |s, _| transpose_selection(s, |p, k| p.transpose_diatonic(-1, k)),
            undoable: true,
        },
        CommandSpec {
            id: "note.octave-up",
            label: "Octave up",
            menu: &[],
            shortcut: Some("Ctrl+Up"),
            params: "",
            enabled: has_note_selection,
            run: |s, _| transpose_selection(s, |p, _| Pitch::new(p.step, p.alter, i32::from(p.octave) + 1)),
            undoable: true,
        },
        CommandSpec {
            id: "note.octave-down",
            label: "Octave down",
            menu: &[],
            shortcut: Some("Ctrl+Down"),
            params: "",
            enabled: has_note_selection,
            run: |s, _| transpose_selection(s, |p, _| Pitch::new(p.step, p.alter, i32::from(p.octave) - 1)),
            undoable: true,
        },
        CommandSpec {
            id: "note.accidental",
            label: "Accidental",
            menu: &[],
            shortcut: None,
            params: "type: double-flat|flat|natural|sharp|double-sharp|auto",
            enabled: has_note_selection,
            run: |s, p| {
                let t = need_str(p, "type")?;
                let acc = if t == "auto" {
                    None
                } else {
                    Some(
                        Accidental::from_id(t)
                            .ok_or_else(|| Error::Param(format!("unknown accidental {t}")))?,
                    )
                };
                transpose_selection(s, |pi, key| match acc {
                    Some(a) => Pitch::new(pi.step, a.alter(), i32::from(pi.octave)),
                    None => Pitch::new(pi.step, key.alter_for(pi.step), i32::from(pi.octave)),
                })
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.tie",
            label: "Tie",
            menu: &["Add", "Notes"],
            shortcut: Some("T"),
            params: "",
            enabled: has_note_selection,
            run: |s, _| {
                let Some(Selection::Event { ev, note }) = s.selection else {
                    return Ok(Value::Null);
                };
                let Some(e) = s.score.event(ev).cloned() else {
                    return Ok(Value::Null);
                };
                // In note input the whole chord is tied; otherwise the selected note.
                let one = if s.input.active {
                    None
                } else {
                    note.and_then(|n| e.notes().get(n))
                };
                let picked: Vec<Pitch> = match one {
                    Some(n) => vec![n.pitch],
                    None => e.notes().iter().map(|n| n.pitch).collect(),
                };
                let next = s.score.next_event(ev);
                let next_has = next
                    .and_then(|n| s.score.event(n))
                    .map(|n| picked.iter().all(|p| n.notes().iter().any(|x| x.pitch == *p)))
                    .unwrap_or(false);
                if !next_has && !s.input.active {
                    return Err(Error::Param("no following note with the same pitch".into()));
                }
                if let Some(evm) = s.score.event_mut(ev) {
                    for n in evm.notes_mut() {
                        if picked.contains(&n.pitch) {
                            n.tie = true;
                        }
                    }
                }
                if s.input.active && !next_has {
                    let kind = EventKind::Chord(picked.iter().map(|p| Note::new(*p)).collect());
                    return enter(s, kind);
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.delete",
            label: "Delete",
            menu: &["Edit"],
            shortcut: Some("Del"),
            params: "",
            enabled: has_selection,
            run: |s, _| {
                match s.selection {
                    Some(Selection::Event { ev, note }) => {
                        let chord = s.score.event(ev).map(|e| e.notes().len()).unwrap_or(0);
                        match note {
                            Some(n) if chord > 1 => {
                                if let Some(e) = s.score.event_mut(ev)
                                    && let EventKind::Chord(notes) = &mut e.kind
                                    && n < notes.len()
                                {
                                    notes.remove(n);
                                }
                                s.selection = Some(Selection::Event { ev, note: None });
                            }
                            _ => {
                                s.score.make_rest(ev)?;
                                s.selection = None;
                                if let Some(p) = ev.pos(&s.score).or(Some(s.cursor)) {
                                    s.cursor = p;
                                }
                            }
                        }
                    }
                    Some(Selection::Measures {
                        part,
                        staff,
                        first,
                        last,
                    }) => {
                        for m in first..=last.min(s.score.measures.len().saturating_sub(1)) {
                            if let Some(sm) = s.score.staff_measure_mut(part, staff, m) {
                                sm.voices.clear();
                            }
                        }
                        s.score.normalize();
                    }
                    None => {}
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.articulation",
            label: "Articulation",
            menu: &[],
            shortcut: None,
            params: "type: staccato|staccatissimo|accent|tenuto|marcato|fermata (toggles)",
            enabled: has_event_selection,
            run: |s, p| {
                let t = need_str(p, "type")?;
                let a = Articulation::from_id(t)
                    .ok_or_else(|| Error::Param(format!("unknown articulation {t}")))?;
                for ev in selected_events(s) {
                    if let Some(e) = s.score.event_mut(ev) {
                        if let Some(i) = e.articulations.iter().position(|x| *x == a) {
                            e.articulations.remove(i);
                        } else if !e.is_rest()
                            || matches!(
                                a.def().place,
                                scoreart_model::marks::Place::Above | scoreart_model::marks::Place::Breath
                            )
                        {
                            e.articulations.push(a);
                        }
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.dynamic",
            label: "Dynamic",
            menu: &[],
            shortcut: None,
            params: "type: ppp|pp|p|mp|mf|f|ff|fff|sf|sfz|fp|none",
            enabled: has_event_selection,
            run: |s, p| {
                let t = need_str(p, "type")?;
                let d = if t == "none" {
                    None
                } else {
                    Some(Dynamic::from_id(t).ok_or_else(|| Error::Param(format!("unknown dynamic {t}")))?)
                };
                for ev in selected_events(s).into_iter().take(1) {
                    if let Some(e) = s.score.event_mut(ev) {
                        e.dynamic = d;
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.lyric",
            label: "Lyric",
            menu: &["Add", "Text"],
            shortcut: Some("Ctrl+L"),
            params: "text (empty removes; a trailing - joins it to the next syllable), verse? 1… (default 1)",
            enabled: has_note_selection,
            run: |s, p| {
                let text = p_str(p, "text").unwrap_or("").trim();
                let verse = p_usize(p, "verse")
                    .unwrap_or(1)
                    .clamp(1, scoreart_model::MAX_VERSES)
                    - 1;
                if let Some(ev) = s.selected_event()
                    && let Some(e) = s.score.event_mut(ev)
                {
                    e.set_lyric(verse, (!text.is_empty()).then(|| clip(text)));
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.voice",
            label: "Move to voice",
            menu: &[],
            shortcut: None,
            params: "voice: 1–4 — moves the selected chord into another voice",
            enabled: has_note_selection,
            run: |s, p| {
                let v = super::need_usize(p, "voice")?;
                if !(1..=limits::MAX_VOICES).contains(&v) {
                    return Err(Error::Param("voice must be 1–4".into()));
                }
                let Some(ev) = s.selected_event() else {
                    return Ok(Value::Null);
                };
                if ev.voice == v - 1 {
                    return Ok(Value::Null);
                }
                let Some(e) = s.score.event(ev).cloned() else {
                    return Ok(Value::Null);
                };
                let pos = ev.pos(&s.score).ok_or(Error::Param("no position".into()))?;
                let target = Pos { voice: v - 1, ..pos };
                s.score.overwrite(target, e.kind.clone(), e.duration)?;
                s.score.make_rest(ev)?;
                if let Some(nev) = target.event(&s.score) {
                    select_event(s, nev, None)?;
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
    ]
}

fn parse_step(s: &str) -> Result<Step> {
    let mut chars = s.trim().chars();
    match (chars.next().and_then(Step::from_letter), chars.next()) {
        (Some(st), None) => Ok(st),
        _ => Err(Error::Param(format!(
            "step must be one of c d e f g a b, not {s:?}"
        ))),
    }
}

fn alter_or_key(alter: Option<i64>, key: scoreart_model::KeySig, step: Step) -> i8 {
    alter
        .map(|a| a.clamp(-2, 2) as i8)
        .unwrap_or_else(|| key.alter_for(step))
}

/// The pitch for a letter: the given octave, or the octave nearest the previous note
/// (or the middle of the staff).
fn choose_pitch(s: &Session, step: Step, octave: Option<i64>, alter: Option<i64>) -> Pitch {
    let c = s.cursor;
    let key = s.score.key_at(c.measure);
    let alter = alter_or_key(alter, key, step);
    if let Some(o) = octave {
        return Pitch::new(step, alter, o.clamp(0, 9) as i32);
    }
    let reference = s
        .last_pitch
        .map(|p| p.diatonic())
        .unwrap_or_else(|| s.score.clef_at(c.part, c.staff, c.measure).top_line() - 4);
    let mut best = Pitch::new(step, alter, 4);
    let mut best_d = i32::MAX;
    for o in 0..=9 {
        let cand = Pitch::new(step, alter, o);
        let d = (cand.diatonic() - reference).abs();
        if d < best_d {
            best_d = d;
            best = cand;
        }
    }
    best
}

/// Writes `kind` at the input cursor with the input duration and selects it.
fn enter(s: &mut Session, kind: EventKind) -> Result<Value> {
    let pos = Pos {
        voice: s.input.voice,
        ..s.cursor
    };
    let d = Duration::new(s.input.value, s.input.dots);
    let pitch = match &kind {
        EventKind::Chord(n) => n.last().map(|n| n.pitch),
        EventKind::Rest => None,
    };
    let after = s.score.overwrite(pos, kind, d)?;
    if let Some(ev) = pos.event(&s.score) {
        s.selection = Some(Selection::Event { ev, note: None });
    }
    if let Some(p) = pitch {
        s.last_pitch = Some(p);
    }
    s.cursor = after;
    Ok(json!({ "measure": after.measure, "tick": after.tick }))
}

fn rewrite_duration(s: &mut Session, ev: EventRef, d: Duration) -> Result<()> {
    let e = s
        .score
        .event(ev)
        .cloned()
        .ok_or(Error::Param("no such event".into()))?;
    let pos = ev.pos(&s.score).ok_or(Error::Param("no position".into()))?;
    s.score.overwrite(pos, e.kind.clone(), d)?;
    if let Some(n) = pos.event(&s.score)
        && let Some(ne) = s.score.event_mut(n)
    {
        // Keep every mark of the event; only the length changed.
        let (kind, duration) = (ne.kind.clone(), ne.duration);
        *ne = e;
        ne.kind = kind;
        ne.duration = duration;
        s.selection = Some(Selection::Event { ev: n, note: None });
    }
    Ok(())
}

fn selected_events(s: &Session) -> Vec<EventRef> {
    match s.selection {
        Some(Selection::Event { ev, .. }) => vec![ev],
        Some(Selection::Measures {
            part,
            staff,
            first,
            last,
        }) => {
            let mut out = Vec::new();
            for m in first..=last {
                if let Some(v) = s.score.voice(part, staff, m, 0) {
                    for i in 0..v.events.len() {
                        out.push(EventRef {
                            part,
                            staff,
                            voice: 0,
                            measure: m,
                            index: i,
                        });
                    }
                }
            }
            out
        }
        None => Vec::new(),
    }
}

fn transpose_selection(s: &mut Session, f: impl Fn(Pitch, scoreart_model::KeySig) -> Pitch) -> Result<Value> {
    let Some(Selection::Event { ev, note }) = s.selection else {
        return Ok(Value::Null);
    };
    let key = s.score.key_at(ev.measure);
    let mut last = None;
    if let Some(e) = s.score.event_mut(ev) {
        for (i, n) in e.notes_mut().iter_mut().enumerate() {
            if note.is_none() || note == Some(i) {
                n.pitch = f(n.pitch, key);
                last = Some(n.pitch);
            }
        }
        if let EventKind::Chord(notes) = &mut e.kind {
            notes.sort_by_key(|n| (n.pitch.diatonic(), n.pitch.alter));
            notes.dedup_by_key(|n| n.pitch);
        }
    }
    // Keep the moved note selected after re-sorting.
    if let (Some(p), Some(_)) = (last, note) {
        let idx = s
            .score
            .event(ev)
            .and_then(|e| e.notes().iter().position(|n| n.pitch == p));
        s.selection = Some(Selection::Event { ev, note: idx });
    }
    s.last_pitch = last.or(s.last_pitch);
    Ok(Value::Null)
}
