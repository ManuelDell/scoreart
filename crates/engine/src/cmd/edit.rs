//! Undo/redo, selection, cursor and note-input mode.

use serde_json::{Value, json};

use scoreart_model::edit::{EventRef, Pos};

use super::{CommandSpec, always, need_usize, p_usize};
use crate::{Error, Result, Selection, Session};

pub fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "edit.undo",
            label: "Undo",
            menu: &["Edit"],
            shortcut: Some("Ctrl+Z"),
            params: "",
            enabled: |s| {
                if s.can_undo() {
                    Ok(())
                } else {
                    Err("Nothing to undo".into())
                }
            },
            run: |s, _| {
                s.undo();
                Ok(Value::Null)
            },
            undoable: false,
        },
        CommandSpec {
            id: "edit.redo",
            label: "Redo",
            menu: &["Edit"],
            shortcut: Some("Ctrl+Y"),
            params: "",
            enabled: |s| {
                if s.can_redo() {
                    Ok(())
                } else {
                    Err("Nothing to redo".into())
                }
            },
            run: |s, _| {
                s.redo();
                Ok(Value::Null)
            },
            undoable: false,
        },
        CommandSpec {
            id: "edit.escape",
            label: "Leave note input / clear selection",
            menu: &[],
            shortcut: Some("Esc"),
            params: "",
            enabled: always,
            run: |s, _| {
                if s.input.active {
                    s.input.active = false;
                } else {
                    s.selection = None;
                }
                Ok(Value::Null)
            },
            undoable: false,
        },
        CommandSpec {
            id: "input.toggle",
            label: "Note input",
            menu: &["Edit"],
            shortcut: Some("N"),
            params: "",
            enabled: |s| {
                if s.score.parts.is_empty() {
                    Err("Add an instrument first".into())
                } else {
                    Ok(())
                }
            },
            run: |s, _| {
                if s.input.active {
                    s.input.active = false;
                } else {
                    start_input(s);
                }
                Ok(json!({ "active": s.input.active }))
            },
            undoable: false,
        },
        CommandSpec {
            id: "input.voice",
            label: "Voice",
            menu: &[],
            shortcut: Some("Ctrl+Alt+1…4"),
            params: "voice: 1–4",
            enabled: always,
            run: |s, p| {
                let v = need_usize(p, "voice")?;
                if !(1..=4).contains(&v) {
                    return Err(Error::Param("voice must be 1–4".into()));
                }
                s.input.voice = v - 1;
                s.cursor.voice = v - 1;
                Ok(Value::Null)
            },
            undoable: false,
        },
        CommandSpec {
            id: "select.event",
            label: "Select note or rest",
            menu: &[],
            shortcut: None,
            params: "part, staff, measure, index, voice? (0), note? (index in chord), extend? (bool: select a range from the current note)",
            enabled: always,
            run: |s, p| {
                let ev = EventRef {
                    part: need_usize(p, "part")?,
                    staff: need_usize(p, "staff")?,
                    voice: p_usize(p, "voice").unwrap_or(0),
                    measure: need_usize(p, "measure")?,
                    index: need_usize(p, "index")?,
                };
                let extend = p.get("extend").and_then(Value::as_bool).unwrap_or(false);
                if extend
                    && let Some(Selection::Event { ev: cur, .. }) = s.selection
                    && cur.part == ev.part
                    && cur.staff == ev.staff
                    && cur.voice == ev.voice
                    && s.score.event(ev).is_some()
                {
                    s.range_end = Some(ev);
                    return Ok(Value::Null);
                }
                select_event(s, ev, p_usize(p, "note"))?;
                Ok(Value::Null)
            },
            undoable: false,
        },
        CommandSpec {
            id: "select.extend-next",
            label: "Extend selection to the next note",
            menu: &[],
            shortcut: Some("Shift+Right"),
            params: "",
            enabled: always,
            run: |s, _| extend(s, true),
            undoable: false,
        },
        CommandSpec {
            id: "select.extend-prev",
            label: "Extend selection to the previous note",
            menu: &[],
            shortcut: Some("Shift+Left"),
            params: "",
            enabled: always,
            run: |s, _| extend(s, false),
            undoable: false,
        },
        CommandSpec {
            id: "select.measure",
            label: "Select measure",
            menu: &[],
            shortcut: None,
            params: "part, staff, measure, extend? (bool: extend the current measure range)",
            enabled: always,
            run: |s, p| {
                let part = need_usize(p, "part")?;
                let staff = need_usize(p, "staff")?;
                let m = need_usize(p, "measure")?;
                if s.score.staff_measure(part, staff, m).is_none() {
                    return Err(Error::Param("no such measure".into()));
                }
                let extend = p.get("extend").and_then(Value::as_bool).unwrap_or(false);
                s.selection = match (extend, s.selection) {
                    (
                        true,
                        Some(Selection::Measures {
                            part: pp,
                            staff: ps,
                            first,
                            last,
                        }),
                    ) if pp == part && ps == staff => Some(Selection::Measures {
                        part,
                        staff,
                        first: first.min(m),
                        last: last.max(m),
                    }),
                    _ => Some(Selection::Measures {
                        part,
                        staff,
                        first: m,
                        last: m,
                    }),
                };
                s.cursor = Pos {
                    part,
                    staff,
                    voice: s.input.voice,
                    measure: m,
                    tick: 0,
                };
                Ok(Value::Null)
            },
            undoable: false,
        },
        CommandSpec {
            id: "select.all",
            label: "Select all",
            menu: &["Edit"],
            shortcut: Some("Ctrl+A"),
            params: "",
            enabled: always,
            run: |s, _| {
                let last = s.score.measures.len().saturating_sub(1);
                s.selection = Some(Selection::Measures {
                    part: s.cursor.part,
                    staff: s.cursor.staff,
                    first: 0,
                    last,
                });
                Ok(Value::Null)
            },
            undoable: false,
        },
        CommandSpec {
            id: "select.next",
            label: "Next note",
            menu: &[],
            shortcut: Some("Right"),
            params: "",
            enabled: always,
            run: |s, _| step(s, true),
            undoable: false,
        },
        CommandSpec {
            id: "select.prev",
            label: "Previous note",
            menu: &[],
            shortcut: Some("Left"),
            params: "",
            enabled: always,
            run: |s, _| step(s, false),
            undoable: false,
        },
        CommandSpec {
            id: "select.next-measure",
            label: "Next measure",
            menu: &[],
            shortcut: Some("Ctrl+Right"),
            params: "",
            enabled: always,
            run: |s, _| step_measure(s, true),
            undoable: false,
        },
        CommandSpec {
            id: "select.prev-measure",
            label: "Previous measure",
            menu: &[],
            shortcut: Some("Ctrl+Left"),
            params: "",
            enabled: always,
            run: |s, _| step_measure(s, false),
            undoable: false,
        },
        CommandSpec {
            id: "select.staff-down",
            label: "Staff below",
            menu: &[],
            shortcut: Some("Ctrl+Alt+Down"),
            params: "",
            enabled: always,
            run: |s, _| step_staff(s, true),
            undoable: false,
        },
        CommandSpec {
            id: "select.staff-up",
            label: "Staff above",
            menu: &[],
            shortcut: Some("Ctrl+Alt+Up"),
            params: "",
            enabled: always,
            run: |s, _| step_staff(s, false),
            undoable: false,
        },
        CommandSpec {
            id: "cursor.set",
            label: "Place the input cursor",
            menu: &[],
            shortcut: None,
            params: "part, staff, measure, tick (0 = measure start), voice?",
            enabled: always,
            run: |s, p| {
                let pos = Pos {
                    part: need_usize(p, "part")?,
                    staff: need_usize(p, "staff")?,
                    voice: p_usize(p, "voice").unwrap_or(s.input.voice),
                    measure: need_usize(p, "measure")?,
                    tick: p_usize(p, "tick").unwrap_or(0).min(u32::MAX as usize) as u32,
                };
                if s.score.staff_measure(pos.part, pos.staff, pos.measure).is_none() {
                    return Err(Error::Param("no such staff or measure".into()));
                }
                if pos.tick >= s.score.measure_ticks(pos.measure) {
                    return Err(Error::Param("tick past the end of the measure".into()));
                }
                s.cursor = pos;
                // The cursor placed by hand wins over an older selection (note input starts here).
                s.selection = None;
                s.range_end = None;
                Ok(Value::Null)
            },
            undoable: false,
        },
    ]
}

/// Enters note-input mode at the selection (or keeps the cursor).
pub(crate) fn start_input(s: &mut Session) {
    match s.selection {
        Some(Selection::Event { ev, .. }) => {
            if let Some(p) = ev.pos(&s.score) {
                s.cursor = p;
                s.input.voice = ev.voice;
            }
        }
        Some(Selection::Measures {
            part, staff, first, ..
        }) => {
            s.cursor = Pos {
                part,
                staff,
                voice: s.input.voice,
                measure: first,
                tick: 0,
            };
        }
        None => {}
    }
    s.cursor.voice = s.input.voice;
    s.input.active = true;
}

fn extend(s: &mut Session, forward: bool) -> Result<Value> {
    let Some(Selection::Event { ev, .. }) = s.selection else {
        return Ok(Value::Null);
    };
    let end = s.range_end.unwrap_or(ev);
    let next = if forward {
        s.score.next_event(end)
    } else {
        s.score.prev_event(end)
    };
    if let Some(n) = next {
        s.range_end = Some(n);
    }
    Ok(Value::Null)
}

pub(crate) fn select_event(s: &mut Session, ev: EventRef, note: Option<usize>) -> Result<()> {
    let e = s
        .score
        .event(ev)
        .ok_or_else(|| Error::Param("no such event".into()))?;
    let note = note.filter(|n| *n < e.notes().len());
    if let Some(p) = note.and_then(|n| e.notes().get(n)).or_else(|| e.notes().last()) {
        s.last_pitch = Some(p.pitch);
    }
    s.selection = Some(Selection::Event { ev, note });
    s.range_end = None;
    if let Some(p) = ev.pos(&s.score) {
        s.cursor = p;
    }
    Ok(())
}

fn step(s: &mut Session, forward: bool) -> Result<Value> {
    if s.input.active {
        // Move the input cursor event by event.
        let ev = s.cursor.event(&s.score);
        let next = ev.and_then(|e| {
            if forward {
                s.score.next_event(e)
            } else {
                s.score.prev_event(e)
            }
        });
        if let Some(n) = next
            && let Some(p) = n.pos(&s.score)
        {
            s.cursor = p;
            if let Some(Selection::Event { .. }) = s.selection {
                s.selection = Some(Selection::Event { ev: n, note: None });
            }
        }
        return Ok(Value::Null);
    }
    let current = match s.selection {
        Some(Selection::Event { ev, .. }) => Some(ev),
        Some(Selection::Measures {
            part, staff, first, ..
        }) => Some(EventRef {
            part,
            staff,
            voice: 0,
            measure: first,
            index: 0,
        }),
        None => s.cursor.event(&s.score),
    };
    let Some(cur) = current else {
        return Ok(Value::Null);
    };
    let target = if matches!(s.selection, Some(Selection::Event { .. })) {
        if forward {
            s.score.next_event(cur)
        } else {
            s.score.prev_event(cur)
        }
    } else {
        Some(cur)
    };
    if let Some(t) = target {
        select_event(s, t, None)?;
    }
    Ok(Value::Null)
}

fn step_measure(s: &mut Session, forward: bool) -> Result<Value> {
    let (part, staff, m) = s.focus();
    let n = s.score.measures.len();
    let target = if forward {
        (m + 1).min(n.saturating_sub(1))
    } else {
        m.saturating_sub(1)
    };
    if s.input.active {
        s.cursor = Pos {
            part,
            staff,
            voice: s.input.voice,
            measure: target,
            tick: 0,
        };
        return Ok(Value::Null);
    }
    let voice = s.selected_event().map(|e| e.voice).unwrap_or(0);
    select_event(
        s,
        EventRef {
            part,
            staff,
            voice,
            measure: target,
            index: 0,
        },
        None,
    )
    .or_else(|_| {
        select_event(
            s,
            EventRef {
                part,
                staff,
                voice: 0,
                measure: target,
                index: 0,
            },
            None,
        )
    })?;
    Ok(Value::Null)
}

fn step_staff(s: &mut Session, down: bool) -> Result<Value> {
    let (part, staff, m) = s.focus();
    // Flatten (part, staff) pairs.
    let all: Vec<(usize, usize)> = s
        .score
        .parts
        .iter()
        .enumerate()
        .flat_map(|(p, pt)| (0..pt.staff_count()).map(move |st| (p, st)))
        .collect();
    let Some(i) = all.iter().position(|x| *x == (part, staff)) else {
        return Ok(Value::Null);
    };
    let j = if down {
        (i + 1).min(all.len().saturating_sub(1))
    } else {
        i.saturating_sub(1)
    };
    let Some(&(p, st)) = all.get(j) else {
        return Ok(Value::Null);
    };
    let tick = s.cursor.tick;
    if s.input.active {
        s.cursor = Pos {
            part: p,
            staff: st,
            voice: 0,
            measure: m,
            tick,
        };
        s.input.voice = 0;
        return Ok(Value::Null);
    }
    let pos = Pos {
        part: p,
        staff: st,
        voice: 0,
        measure: m,
        tick,
    };
    if let Some(ev) = pos.event(&s.score) {
        select_event(s, ev, None)?;
    }
    Ok(Value::Null)
}
