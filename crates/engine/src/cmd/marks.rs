//! Marks on notes (noteheads, tremolos, arpeggios, grace notes, fingerings, texts), lines between
//! notes (slurs, hairpins, octave lines …) and measure marks (voltas, jumps, rehearsal marks).

use serde_json::{Value, json};

use scoreart_model::edit::EventRef;
use scoreart_model::{
    Anchor, Arpeggio, Grace, Jump, NoteHead, NoteValue, Pitch, Placement, Spanner, SpannerKind, Step,
    Tremolo, clip, limits,
};

use super::{CommandSpec, need_i64, need_str, need_usize, p_i64, p_str};
use crate::{Error, Selection, Session};

fn has_note(s: &Session) -> std::result::Result<(), String> {
    match s.selection {
        Some(Selection::Event { ev, .. }) if s.score.event(ev).is_some_and(|e| !e.is_rest()) => Ok(()),
        Some(Selection::Measures { .. }) => Ok(()),
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

/// The selected notes as (event, note index or all).
fn note_targets(s: &Session) -> Vec<(EventRef, Option<usize>)> {
    match s.selection {
        Some(Selection::Event { ev, note }) if s.range_end.is_none() => vec![(ev, note)],
        _ => s.selected_events().into_iter().map(|e| (e, None)).collect(),
    }
}

fn has_lines(s: &Session) -> std::result::Result<(), String> {
    if s.score.spanners.is_empty() {
        Err("no lines".into())
    } else {
        Ok(())
    }
}

/// The anchor `n` notes after (or before) `a` in the line's voice.
fn step_anchor(s: &Session, sp: &Spanner, a: Anchor, n: i64) -> Option<Anchor> {
    let pos = scoreart_model::edit::Pos {
        part: sp.part,
        staff: sp.staff,
        voice: sp.voice,
        measure: a.measure,
        tick: a.tick,
    };
    let mut ev = pos.event(&s.score)?;
    for _ in 0..n.unsigned_abs().min(4096) {
        ev = if n > 0 {
            s.score.next_event(ev)?
        } else {
            s.score.prev_event(ev)?
        };
    }
    anchor(s, ev)
}

fn anchor(s: &Session, r: EventRef) -> Option<Anchor> {
    r.pos(&s.score).map(|p| Anchor {
        measure: p.measure,
        tick: p.tick,
    })
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "note.head",
            label: "Notehead",
            menu: &[],
            shortcut: None,
            params: "type: normal|x|diamond|slash|triangle|circle-x|cross|circled",
            enabled: has_note,
            run: |s, p| {
                let t = need_str(p, "type")?;
                let head =
                    NoteHead::from_id(t).ok_or_else(|| Error::Param(format!("unknown notehead {t}")))?;
                for (ev, note) in note_targets(s) {
                    if let Some(e) = s.score.event_mut(ev) {
                        for (i, n) in e.notes_mut().iter_mut().enumerate() {
                            if note.is_none() || note == Some(i) {
                                n.head = head;
                            }
                        }
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.paren",
            label: "Parentheses (ghost note)",
            menu: &[],
            shortcut: None,
            params: "",
            enabled: has_note,
            run: |s, _| {
                for (ev, note) in note_targets(s) {
                    if let Some(e) = s.score.event_mut(ev) {
                        for (i, n) in e.notes_mut().iter_mut().enumerate() {
                            if note.is_none() || note == Some(i) {
                                n.paren = !n.paren;
                            }
                        }
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.fingering",
            label: "Fingering",
            menu: &["Add", "Text"],
            shortcut: None,
            params: "text (\"1\"…\"5\", \"p i m a\"; empty removes) — on the selected note",
            enabled: has_note,
            run: |s, p| {
                let t = p_str(p, "text")
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(12)
                    .collect::<String>();
                for (ev, note) in note_targets(s) {
                    if let Some(e) = s.score.event_mut(ev) {
                        let n = e.notes().len();
                        // Without a picked note, the fingering goes on the top note.
                        let idx = note.unwrap_or(n.saturating_sub(1));
                        if let Some(nn) = e.notes_mut().get_mut(idx) {
                            nn.fingering = if t.is_empty() { None } else { Some(t.clone()) };
                        }
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.tremolo",
            label: "Tremolo",
            menu: &[],
            shortcut: None,
            params: "strokes: 0 (none), 1–4, or \"buzz\"",
            enabled: has_note,
            run: |s, p| {
                let t = match p.get("strokes") {
                    Some(Value::String(b)) if b == "buzz" => Tremolo::Buzz,
                    _ => match need_i64(p, "strokes")? {
                        0 => Tremolo::None,
                        n @ 1..=4 => Tremolo::Strokes(n as u8),
                        _ => return Err(Error::Param("strokes 0–4 or \"buzz\"".into())),
                    },
                };
                for (ev, _) in note_targets(s) {
                    if let Some(e) = s.score.event_mut(ev)
                        && !e.is_rest()
                    {
                        e.tremolo = if e.tremolo == t { Tremolo::None } else { t };
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.arpeggio",
            label: "Arpeggio",
            menu: &[],
            shortcut: None,
            params: "type: arpeggio|arpeggio-up|arpeggio-down|non-arpeggio|none",
            enabled: has_note,
            run: |s, p| {
                let t = need_str(p, "type")?;
                let a = Arpeggio::from_id(t).ok_or_else(|| Error::Param(format!("unknown arpeggio {t}")))?;
                for (ev, _) in note_targets(s) {
                    if let Some(e) = s.score.event_mut(ev)
                        && !e.is_rest()
                    {
                        e.arpeggio = if e.arpeggio == a { Arpeggio::None } else { a };
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.grace",
            label: "Grace note",
            menu: &["Add", "Notes"],
            shortcut: Some("/"),
            params: "type: acciaccatura|appoggiatura|grace-16th|grace-32nd; step? and octave? (default: a step above the note)",
            enabled: has_note,
            run: |s, p| {
                let t = p_str(p, "type").unwrap_or("acciaccatura");
                let (value, slash) = match t {
                    "acciaccatura" => (NoteValue::Eighth, true),
                    "appoggiatura" => (NoteValue::Eighth, false),
                    "grace-16th" => (NoteValue::N16th, false),
                    "grace-32nd" => (NoteValue::N32nd, false),
                    other => return Err(Error::Param(format!("unknown grace type {other}"))),
                };
                let targets = note_targets(s);
                let Some((ev, _)) = targets.first().copied() else {
                    return Ok(Value::Null);
                };
                let key = s.score.key_at(ev.measure);
                let top = s
                    .score
                    .event(ev)
                    .and_then(|e| e.notes().last().map(|n| n.pitch))
                    .ok_or(Error::Param("no note".into()))?;
                let pitch = match (
                    p_str(p, "step")
                        .and_then(|x| x.chars().next())
                        .and_then(Step::from_letter),
                    p_i64(p, "octave"),
                ) {
                    (Some(st), Some(o)) => Pitch::new(st, key.alter_for(st), o.clamp(0, 9) as i32),
                    (Some(st), None) => {
                        let mut d = top.diatonic() + 1;
                        while d.rem_euclid(7) != st.index() {
                            d += 1;
                        }
                        Pitch::from_diatonic(d, key.alter_for(st))
                    }
                    _ => top.transpose_diatonic(1, key),
                };
                let e = s.score.event_mut(ev).ok_or(Error::Param("no event".into()))?;
                if e.grace.len() >= limits::MAX_GRACE {
                    return Err(Error::Param("too many grace notes".into()));
                }
                e.grace.push(Grace {
                    pitches: vec![pitch],
                    value,
                    slash,
                    after: false,
                });
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.grace-pitch",
            label: "Move grace note",
            menu: &[],
            shortcut: None,
            params: "delta: diatonic steps (±1 …) for the last grace note of the selection",
            enabled: has_note,
            run: |s, p| {
                let delta = need_i64(p, "delta")?.clamp(-14, 14) as i32;
                let Some((ev, _)) = note_targets(s).first().copied() else {
                    return Ok(Value::Null);
                };
                let key = s.score.key_at(ev.measure);
                if let Some(e) = s.score.event_mut(ev)
                    && let Some(g) = e.grace.last_mut()
                {
                    for p in &mut g.pitches {
                        *p = p.transpose_diatonic(delta, key);
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.grace-remove",
            label: "Remove grace notes",
            menu: &[],
            shortcut: None,
            params: "",
            enabled: has_note,
            run: |s, _| {
                for (ev, _) in note_targets(s) {
                    if let Some(e) = s.score.event_mut(ev) {
                        e.grace.clear();
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.chord-symbol",
            label: "Chord symbol",
            menu: &["Add", "Text"],
            shortcut: Some("Ctrl+K"),
            params: "text (\"C\", \"F#m7\", \"Bbmaj7/D\"; empty removes)",
            enabled: has_selection,
            run: |s, p| {
                let t = p_str(p, "text").unwrap_or("").trim().to_string();
                if let Some((ev, _)) = note_targets(s).first().copied()
                    && let Some(e) = s.score.event_mut(ev)
                {
                    e.chord_symbol = if t.is_empty() { None } else { Some(clip(&t)) };
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.text",
            label: "Staff text",
            menu: &["Add", "Text"],
            shortcut: Some("Ctrl+T"),
            params: "text (\"dolce\", \"pizz.\" …; empty removes)",
            enabled: has_selection,
            run: |s, p| {
                let t = p_str(p, "text").unwrap_or("").trim().to_string();
                if let Some((ev, _)) = note_targets(s).first().copied()
                    && let Some(e) = s.score.event_mut(ev)
                {
                    e.text = if t.is_empty() { None } else { Some(clip(&t)) };
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "note.place",
            label: "Move text or dynamic",
            menu: &["Format"],
            shortcut: None,
            params: "kind: text|dynamic, dx and dy (staff spaces, as MuseScore stores them: dx from the note, dy from the top staff line — or with below: true from the bottom line — to the baseline), below? (default: as it was), reset: true puts it back",
            enabled: has_selection,
            run: |s, p| {
                let kind = need_str(p, "kind")?;
                let reset = p.get("reset").and_then(Value::as_bool).unwrap_or(false);
                let num = |k: &str| {
                    p.get(k)
                        .and_then(Value::as_f64)
                        .filter(|v| v.is_finite())
                        .unwrap_or(0.0)
                };
                let Some((ev, _)) = note_targets(s).first().copied() else {
                    return Ok(Value::Null);
                };
                let Some(e) = s.score.event_mut(ev) else {
                    return Ok(Value::Null);
                };
                match kind {
                    "text" => {
                        let below = p
                            .get("below")
                            .and_then(Value::as_bool)
                            .unwrap_or(e.text_place.is_some_and(|p| p.below));
                        e.text_place = (!reset).then(|| Placement::from_sp(num("dx"), num("dy"), below));
                    }
                    "dynamic" => {
                        let below = p
                            .get("below")
                            .and_then(Value::as_bool)
                            .unwrap_or(e.dynamic_place.is_none_or(|p| p.below));
                        e.dynamic_place = (!reset).then(|| Placement::from_sp(num("dx"), num("dy"), below));
                    }
                    other => return Err(Error::Param(format!("unknown kind {other}"))),
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "line.add",
            label: "Add line",
            menu: &["Add", "Lines"],
            shortcut: None,
            params: "type: slur|crescendo|diminuendo|cresc-line|dim-line|8va|8vb|15ma|15mb|22ma|22mb|pedal|trill-line|vibrato|let-ring|palm-mute|glissando|wavy-glissando — over the selected range (a single note: to the next note)",
            enabled: has_selection,
            run: |s, p| {
                let t = need_str(p, "type")?;
                let kind =
                    SpannerKind::from_id(t).ok_or_else(|| Error::Param(format!("unknown line {t}")))?;
                let (a, mut b) = s
                    .selected_range()
                    .ok_or(Error::Param("nothing selected".into()))?;
                if a == b {
                    b = s.score.next_event(a).unwrap_or(a);
                }
                let (Some(start), Some(end)) = (anchor(s, a), anchor(s, b)) else {
                    return Ok(Value::Null);
                };
                if s.score.spanners.len() >= limits::MAX_SPANNERS {
                    return Err(Error::Param("too many lines".into()));
                }
                let sp = Spanner {
                    kind,
                    part: a.part,
                    staff: a.staff,
                    voice: a.voice,
                    start,
                    end,
                    dy: 0,
                    spread: 0,
                };
                if !s.score.spanners.contains(&sp) {
                    s.score.spanners.push(sp);
                }
                s.score.normalize();
                Ok(json!({ "lines": s.score.spanners.len() }))
            },
            undoable: true,
        },
        CommandSpec {
            id: "line.remove",
            label: "Remove lines",
            menu: &["Edit"],
            shortcut: None,
            params: "type? (only lines of this type) — removes lines that start or end in the selection",
            enabled: has_selection,
            run: |s, p| {
                let only = p_str(p, "type").and_then(SpannerKind::from_id);
                let anchors: Vec<(usize, usize, usize, Anchor)> = s
                    .selected_events()
                    .into_iter()
                    .filter_map(|e| anchor(s, e).map(|a| (e.part, e.staff, e.voice, a)))
                    .collect();
                let before = s.score.spanners.len();
                s.score.spanners.retain(|sp| {
                    let hit = anchors.iter().any(|(p, st, v, a)| {
                        sp.part == *p && sp.staff == *st && sp.voice == *v && (sp.start == *a || sp.end == *a)
                    });
                    !(hit && only.is_none_or(|k| k == sp.kind))
                });
                Ok(json!({ "removed": before - s.score.spanners.len() }))
            },
            undoable: true,
        },
        CommandSpec {
            id: "line.move",
            label: "Move line",
            menu: &[],
            shortcut: None,
            params: "index (of the line), dy? (staff spaces from its place, down; sets it), shift? (notes to move both ends by, ±)",
            enabled: has_lines,
            run: |s, p| {
                let i = need_usize(p, "index")?;
                let sp = *s
                    .score
                    .spanners
                    .get(i)
                    .ok_or_else(|| Error::Param("no such line".into()))?;
                let mut moved = sp;
                if let Some(dy) = p.get("dy").and_then(Value::as_f64).filter(|v| v.is_finite()) {
                    moved.dy = (dy * 10.0).round().clamp(-400.0, 400.0) as i16;
                }
                if let Some(n) = p_i64(p, "shift").filter(|n| *n != 0) {
                    let (Some(a), Some(b)) =
                        (step_anchor(s, &sp, sp.start, n), step_anchor(s, &sp, sp.end, n))
                    else {
                        return Err(Error::Param("the line cannot move that far".into()));
                    };
                    moved.start = a;
                    moved.end = b;
                }
                if let Some(x) = s.score.spanners.get_mut(i) {
                    *x = moved;
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "line.end",
            label: "Move the end of a line",
            menu: &[],
            shortcut: None,
            params: "index (of the line), which: start|end, measure, tick (a note in the line's voice)",
            enabled: has_lines,
            run: |s, p| {
                let i = need_usize(p, "index")?;
                let which = need_str(p, "which")?;
                let sp = *s
                    .score
                    .spanners
                    .get(i)
                    .ok_or_else(|| Error::Param("no such line".into()))?;
                let at = Anchor {
                    measure: need_usize(p, "measure")?,
                    tick: need_i64(p, "tick")?.clamp(0, i64::from(u32::MAX)) as u32,
                };
                let pos = scoreart_model::edit::Pos {
                    part: sp.part,
                    staff: sp.staff,
                    voice: sp.voice,
                    measure: at.measure,
                    tick: at.tick,
                };
                let ev = pos
                    .event(&s.score)
                    .ok_or_else(|| Error::Param("no note there".into()))?;
                let at = anchor(s, ev).ok_or_else(|| Error::Param("no note there".into()))?;
                let mut moved = sp;
                match which {
                    "start" => moved.start = at,
                    "end" => moved.end = at,
                    other => return Err(Error::Param(format!("which: start or end, not {other}"))),
                }
                if (moved.start.measure, moved.start.tick) > (moved.end.measure, moved.end.tick) {
                    return Err(Error::Param("a line cannot end before it starts".into()));
                }
                if let Some(x) = s.score.spanners.get_mut(i) {
                    *x = moved;
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "line.spread",
            label: "Hairpin opening",
            menu: &[],
            shortcut: None,
            params: "index (of a hairpin), spread (staff spaces, 0.3…6; 0 = standard)",
            enabled: has_lines,
            run: |s, p| {
                let i = need_usize(p, "index")?;
                let v = p
                    .get("spread")
                    .and_then(Value::as_f64)
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| Error::Param("spread: a number".into()))?;
                let sp = s
                    .score
                    .spanners
                    .get_mut(i)
                    .ok_or_else(|| Error::Param("no such line".into()))?;
                if !matches!(sp.kind, SpannerKind::Crescendo | SpannerKind::Diminuendo) {
                    return Err(Error::Param("only hairpins open".into()));
                }
                sp.spread = if v <= 0.0 {
                    0
                } else {
                    (v * 10.0).round().clamp(3.0, 60.0) as u8
                };
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.volta",
            label: "Volta (ending)",
            menu: &[],
            shortcut: None,
            params: "number: 1–9 (0 removes; optional with a text: the volta there, else the text's first number), text? (the printed label when it is not \"n.\", e.g. \"1.+3.\" or \"2.4.5.\"; empty: \"n.\") — on the selected measures",
            enabled: has_selection,
            run: |s, p| {
                let text = p_str(p, "text")
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(clip);
                let (a, b) = measure_range(s);
                let n = match (p_i64(p, "number"), &text) {
                    (Some(n), _) => n,
                    (None, Some(t)) => s
                        .score
                        .measures
                        .get(a)
                        .and_then(|i| i.volta)
                        .map(i64::from)
                        .or_else(|| t.chars().find_map(|c| c.to_digit(10)).map(i64::from))
                        .filter(|n| *n > 0)
                        .unwrap_or(1),
                    (None, None) => return Err(Error::Param("number".into())),
                };
                if !(0..=9).contains(&n) {
                    return Err(Error::Param("number 0–9".into()));
                }
                for m in a..=b {
                    if let Some(info) = s.score.measures.get_mut(m) {
                        info.volta = (n > 0).then_some(n as u8);
                        info.volta_text = if n > 0 { text.clone() } else { None };
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.jump",
            label: "Repeat sign or jump",
            menu: &[],
            shortcut: None,
            params: "type: segno|coda|varied-coda|fine|to-coda|da-capo|da-capo-al-fine|da-capo-al-coda|dal-segno|dal-segno-al-fine|dal-segno-al-coda (toggles)",
            enabled: has_selection,
            run: |s, p| {
                let t = need_str(p, "type")?;
                let j = Jump::from_id(t).ok_or_else(|| Error::Param(format!("unknown jump {t}")))?;
                let (a, b) = measure_range(s);
                let m = if j.at_end() { b } else { a };
                if let Some(info) = s.score.measures.get_mut(m) {
                    if let Some(i) = info.jumps.iter().position(|x| *x == j) {
                        info.jumps.remove(i);
                    } else if info.jumps.len() < 4 {
                        info.jumps.push(j);
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.rehearsal",
            label: "Rehearsal mark",
            menu: &["Add", "Text"],
            shortcut: Some("Ctrl+M"),
            params: "text? (default: the next letter A, B, C …; empty removes when given)",
            enabled: has_selection,
            run: |s, p| {
                let (a, _) = measure_range(s);
                let text = match p_str(p, "text") {
                    Some(t) => t.trim().chars().take(8).collect::<String>(),
                    None => {
                        let used = s
                            .score
                            .measures
                            .iter()
                            .take(a)
                            .filter(|m| m.rehearsal.is_some())
                            .count();
                        char::from_u32(u32::from(b'A') + (used % 26) as u32)
                            .map(String::from)
                            .unwrap_or_default()
                    }
                };
                if let Some(info) = s.score.measures.get_mut(a) {
                    info.rehearsal = if text.is_empty() { None } else { Some(text) };
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.tempo-text",
            label: "Tempo marking",
            menu: &["Add", "Text"],
            shortcut: None,
            params: "text (\"Allegro\", \"Andante\" …; empty removes)",
            enabled: has_selection,
            run: |s, p| {
                let t = p_str(p, "text").unwrap_or("").trim().to_string();
                let (a, _) = measure_range(s);
                if let Some(info) = s.score.measures.get_mut(a) {
                    info.tempo_text = if t.is_empty() { None } else { Some(clip(&t)) };
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
        CommandSpec {
            id: "measure.repeat-sign",
            label: "One-measure repeat (%)",
            menu: &[],
            shortcut: None,
            params: "— toggles on the selected measures of the selected staff",
            enabled: has_selection,
            run: |s, _| {
                let (part, staff, _) = s.focus();
                let (a, b) = measure_range(s);
                for m in a..=b {
                    if m == 0 {
                        continue;
                    }
                    if let Some(sm) = s.score.staff_measure_mut(part, staff, m) {
                        sm.measure_repeat = !sm.measure_repeat;
                    }
                }
                Ok(Value::Null)
            },
            undoable: true,
        },
    ]
}

/// First and last measure of the selection.
fn measure_range(s: &Session) -> (usize, usize) {
    match (s.selection, s.selected_range()) {
        (Some(Selection::Measures { first, last, .. }), _) => (first, last),
        (_, Some((a, b))) => (a.measure, b.measure),
        _ => (s.cursor.measure, s.cursor.measure),
    }
}
