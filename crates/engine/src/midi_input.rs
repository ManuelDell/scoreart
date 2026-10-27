//! Note input from a MIDI keyboard or controller, with a binding for every key and controller.
//!
//! By default every key enters its note (keys pressed while others are held add to the chord)
//! and controllers do nothing. Any key or controller (pad, button, pedal, knob) can instead run
//! an action: a note value, rest, tie, undo, play …

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What a key or controller does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MidiAction {
    /// Enter (or, outside note input, sound) the key's note.
    Note,
    /// Nothing.
    Ignore,
    /// One of [`ACTIONS`] by id.
    Action(String),
}

/// A key (by note number) or a controller (by CC number).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Trigger {
    Key(u8),
    Control(u8),
}

impl Trigger {
    pub fn label(self) -> String {
        match self {
            Trigger::Key(k) => format!("Key {} ({k})", key_name(k)),
            Trigger::Control(c) => format!("Controller {c}{}", cc_name(c)),
        }
    }
}

/// Note name with octave, middle C = C4.
pub fn key_name(k: u8) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let name = NAMES.get(usize::from(k % 12)).copied().unwrap_or("?");
    format!("{name}{}", i32::from(k / 12) - 1)
}

fn cc_name(c: u8) -> &'static str {
    match c {
        1 => " (modulation)",
        7 => " (volume)",
        10 => " (pan)",
        11 => " (expression)",
        64 => " (sustain pedal)",
        66 => " (sostenuto pedal)",
        67 => " (soft pedal)",
        _ => "",
    }
}

/// An action a key or controller can run: id, label, and the command it runs. Ids starting
/// with `ui.` are handled by the user interface (playback).
pub struct ActionSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub command: &'static str,
    pub params: fn() -> Value,
}

macro_rules! act {
    ($id:expr, $label:expr, $cmd:expr) => {
        ActionSpec {
            id: $id,
            label: $label,
            command: $cmd,
            params: || json!({}),
        }
    };
    ($id:expr, $label:expr, $cmd:expr, $params:tt) => {
        ActionSpec {
            id: $id,
            label: $label,
            command: $cmd,
            params: || json!($params),
        }
    };
}

/// Every action that can be bound.
pub const ACTIONS: &[ActionSpec] = &[
    act!("input.toggle", "Note input on/off", "input.toggle"),
    act!("dur.64", "64th note", "input.duration", { "value": 1 }),
    act!("dur.32", "32nd note", "input.duration", { "value": 2 }),
    act!("dur.16", "16th note", "input.duration", { "value": 3 }),
    act!("dur.8", "Eighth note", "input.duration", { "value": 4 }),
    act!("dur.4", "Quarter note", "input.duration", { "value": 5 }),
    act!("dur.2", "Half note", "input.duration", { "value": 6 }),
    act!("dur.1", "Whole note", "input.duration", { "value": 7 }),
    act!("dur.breve", "Breve", "input.duration", { "value": 8 }),
    act!("dot", "Dot", "input.dot"),
    act!("rest", "Rest", "note.rest"),
    act!("tie", "Tie", "note.tie"),
    act!("voice.1", "Voice 1", "input.voice", { "voice": 1 }),
    act!("voice.2", "Voice 2", "input.voice", { "voice": 2 }),
    act!("voice.3", "Voice 3", "input.voice", { "voice": 3 }),
    act!("voice.4", "Voice 4", "input.voice", { "voice": 4 }),
    act!("undo", "Undo", "edit.undo"),
    act!("redo", "Redo", "edit.redo"),
    act!("delete", "Delete", "note.delete"),
    act!("next", "Next note", "select.next"),
    act!("prev", "Previous note", "select.prev"),
    act!("next-measure", "Next measure", "select.next-measure"),
    act!("prev-measure", "Previous measure", "select.prev-measure"),
    act!("up", "Pitch up", "note.pitch-up"),
    act!("down", "Pitch down", "note.pitch-down"),
    act!("octave-up", "Octave up", "note.octave-up"),
    act!("octave-down", "Octave down", "note.octave-down"),
    act!("staccato", "Staccato", "note.articulation", { "type": "staccato" }),
    act!("accent", "Accent", "note.articulation", { "type": "accent" }),
    act!("slur", "Slur", "line.add", { "type": "slur" }),
    act!("append", "Append a measure", "measure.append"),
    act!("ui.play", "Play / stop", ""),
];

pub fn action(id: &str) -> Option<&'static ActionSpec> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// The bindings of a device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MidiMap {
    /// Only this channel (1–16) is listened to; `None` = all.
    pub channel: Option<u8>,
    /// Semitones added to every played note.
    pub transpose: i8,
    /// Keys that do something other than enter their note.
    pub keys: BTreeMap<u8, MidiAction>,
    /// Controllers that do something (all others are ignored).
    pub controls: BTreeMap<u8, MidiAction>,
}

/// What a MIDI message asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum MidiEvent {
    /// Enter `key` (a MIDI note number); `chord`: add it to the chord just entered.
    Note { key: u8, velocity: u8, chord: bool },
    /// Run an engine command.
    Command { id: &'static str, params: Value },
    /// A user-interface action (`ui.play`).
    Ui(&'static str),
}

/// Keys held down and controller values, to find chords and button presses.
#[derive(Clone, Debug, Default)]
pub struct MidiState {
    held: BTreeSet<u8>,
    controls: BTreeMap<u8, u8>,
}

impl MidiMap {
    pub fn binding(&self, t: Trigger) -> MidiAction {
        match t {
            Trigger::Key(k) => self.keys.get(&k).cloned().unwrap_or(MidiAction::Note),
            Trigger::Control(c) => self.controls.get(&c).cloned().unwrap_or(MidiAction::Ignore),
        }
    }

    /// Binds `t`; the default binding is stored as no entry.
    pub fn bind(&mut self, t: Trigger, a: MidiAction) {
        match t {
            Trigger::Key(k) if a == MidiAction::Note => {
                self.keys.remove(&k);
            }
            Trigger::Key(k) => {
                self.keys.insert(k, a);
            }
            Trigger::Control(c) if a == MidiAction::Ignore => {
                self.controls.remove(&c);
            }
            Trigger::Control(c) => {
                self.controls.insert(c, a);
            }
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Reads saved bindings; anything unreadable gives the defaults.
    pub fn from_json(text: &str) -> MidiMap {
        serde_json::from_str(text).unwrap_or_default()
    }

    /// Whether a channel message passes the channel filter.
    fn listens(&self, status: u8) -> bool {
        self.channel.is_none_or(|c| c == (status & 0x0F) + 1)
    }

    /// The key or controller a message comes from, when it is pressed (for learning).
    pub fn pressed(&self, msg: &[u8]) -> Option<Trigger> {
        match *msg {
            [s, k, v, ..] if s & 0xF0 == 0x90 && v > 0 && self.listens(s) => Some(Trigger::Key(k & 0x7F)),
            [s, c, v, ..] if s & 0xF0 == 0xB0 && v >= 64 && self.listens(s) => {
                Some(Trigger::Control(c & 0x7F))
            }
            _ => None,
        }
    }

    /// Turns one MIDI message into what it should do.
    pub fn interpret(&self, msg: &[u8], st: &mut MidiState) -> Option<MidiEvent> {
        let (&status, rest) = msg.split_first()?;
        if status >= 0xF0 || !self.listens(status) {
            return None;
        }
        let a = rest.first().copied().unwrap_or(0) & 0x7F;
        let b = rest.get(1).copied().unwrap_or(0) & 0x7F;
        match status & 0xF0 {
            0x90 if b > 0 => {
                let chord = !st.held.is_empty();
                st.held.insert(a);
                match self.binding(Trigger::Key(a)) {
                    MidiAction::Note => {
                        let key = (i16::from(a) + i16::from(self.transpose)).clamp(0, 127) as u8;
                        Some(MidiEvent::Note {
                            key,
                            velocity: b,
                            chord,
                        })
                    }
                    MidiAction::Ignore => None,
                    MidiAction::Action(id) => {
                        // A key used as a button does not start a chord.
                        st.held.remove(&a);
                        event_for(&id)
                    }
                }
            }
            0x80 | 0x90 => {
                st.held.remove(&a);
                None
            }
            0xB0 => {
                let before = st.controls.insert(a, b).unwrap_or(0);
                // A button or pedal fires once when pressed (crossing the middle).
                if before < 64 && b >= 64 {
                    match self.binding(Trigger::Control(a)) {
                        MidiAction::Action(id) => event_for(&id),
                        MidiAction::Note | MidiAction::Ignore => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

fn event_for(id: &str) -> Option<MidiEvent> {
    let a = action(id)?;
    if a.id.starts_with("ui.") {
        Some(MidiEvent::Ui(a.id))
    } else {
        Some(MidiEvent::Command {
            id: a.command,
            params: (a.params)(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_enter_notes_and_bound_keys_run_actions() {
        let mut map = MidiMap::default();
        map.bind(Trigger::Key(21), MidiAction::Action("rest".into()));
        map.bind(Trigger::Control(64), MidiAction::Action("undo".into()));
        let mut st = MidiState::default();
        assert_eq!(
            map.interpret(&[0x90, 60, 100], &mut st),
            Some(MidiEvent::Note {
                key: 60,
                velocity: 100,
                chord: false
            })
        );
        // Held C, then E: a chord.
        assert_eq!(
            map.interpret(&[0x90, 64, 90], &mut st),
            Some(MidiEvent::Note {
                key: 64,
                velocity: 90,
                chord: true
            })
        );
        map.interpret(&[0x80, 60, 0], &mut st);
        map.interpret(&[0x90, 64, 0], &mut st);
        assert_eq!(
            map.interpret(&[0x90, 21, 80], &mut st),
            Some(MidiEvent::Command {
                id: "note.rest",
                params: json!({})
            })
        );
        // The pedal fires once per press.
        let undo = Some(MidiEvent::Command {
            id: "edit.undo",
            params: json!({}),
        });
        assert_eq!(map.interpret(&[0xB0, 64, 127], &mut st), undo);
        assert_eq!(map.interpret(&[0xB0, 64, 120], &mut st), None);
        assert_eq!(map.interpret(&[0xB0, 64, 0], &mut st), None);
        assert_eq!(map.interpret(&[0xB0, 64, 127], &mut st), undo);
        // Channel filter and transposition.
        map.channel = Some(2);
        map.transpose = -12;
        assert_eq!(map.interpret(&[0x90, 60, 100], &mut st), None);
        assert_eq!(
            map.interpret(&[0x91, 60, 100], &mut st),
            Some(MidiEvent::Note {
                key: 48,
                velocity: 100,
                chord: false
            })
        );
        assert_eq!(map.pressed(&[0x91, 72, 1]), Some(Trigger::Key(72)));
        // Saved and read back; junk gives the defaults.
        assert_eq!(MidiMap::from_json(&map.to_json()), map);
        assert_eq!(MidiMap::from_json("{nope"), MidiMap::default());
        // Every action names a real command.
        let ids: Vec<&str> = crate::registry().iter().map(|c| c.id).collect();
        for a in ACTIONS.iter().filter(|a| !a.id.starts_with("ui.")) {
            assert!(ids.contains(&a.command), "{}", a.command);
        }
        for junk in [&[][..], &[0x90], &[0xF0, 1, 2], &[0xE0, 1, 2]] {
            let _ = map.interpret(junk, &mut st);
        }
    }
}
