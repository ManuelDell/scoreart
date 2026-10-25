use serde_json::json;

use scoreart_model::{EventKind, NoteValue, Step, TPQ};

use crate::*;

fn blank() -> Session {
    let mut s = Session::default();
    s.execute("file.new", &json!({ "instruments": ["flute"], "measures": 4 }))
        .unwrap();
    s
}

#[test]
fn note_input_flow() {
    let mut s = blank();
    s.execute("input.toggle", &json!({})).unwrap();
    assert!(s.input.active);
    s.execute("input.duration", &json!({ "value": 4 })).unwrap(); // eighth
    for st in ["c", "d", "e", "f"] {
        s.execute("note.enter", &json!({ "step": st })).unwrap();
    }
    s.execute("input.duration", &json!({ "value": "half" })).unwrap();
    s.execute("note.enter", &json!({ "step": "g" })).unwrap();
    let v = s.score.voice(0, 0, 0, 0).unwrap();
    assert_eq!(v.events.len(), 5);
    assert_eq!(v.events[0].duration.value, NoteValue::Eighth);
    // Octave nearest to the treble staff's middle line: C5 (B4 is the middle line).
    assert_eq!(v.events[0].notes()[0].pitch.octave, 5);
    assert_eq!(v.events[4].notes()[0].pitch.step, Step::G);
    assert_eq!(s.cursor.measure, 1);
    // Up arrow raises the last entered note by a semitone.
    s.execute("note.pitch-up", &json!({})).unwrap();
    assert_eq!(
        s.score.voice(0, 0, 0, 0).unwrap().events[4].notes()[0]
            .pitch
            .alter,
        1
    );
    // Undo restores it, redo re-applies.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(
        s.score.voice(0, 0, 0, 0).unwrap().events[4].notes()[0]
            .pitch
            .alter,
        0
    );
    s.execute("edit.redo", &json!({})).unwrap();
    assert_eq!(
        s.score.voice(0, 0, 0, 0).unwrap().events[4].notes()[0]
            .pitch
            .alter,
        1
    );
}

#[test]
fn chord_rest_tie_and_delete() {
    let mut s = blank();
    s.execute("note.enter", &json!({ "step": "e", "octave": 4 }))
        .unwrap();
    s.execute("note.add", &json!({ "step": "g" })).unwrap();
    s.execute("note.add", &json!({ "step": "c" })).unwrap();
    let e = &s.score.voice(0, 0, 0, 0).unwrap().events[0];
    let names: Vec<String> = e.notes().iter().map(|n| n.pitch.name()).collect();
    assert_eq!(names, ["E4", "G4", "C5"]);
    s.execute("note.tie", &json!({})).unwrap();
    let v = s.score.voice(0, 0, 0, 0).unwrap();
    assert!(v.events[0].notes().iter().all(|n| n.tie));
    assert_eq!(v.events[1].notes().len(), 3, "the tied chord continues");
    s.execute("note.rest", &json!({})).unwrap();
    s.execute("edit.escape", &json!({})).unwrap();
    s.execute(
        "select.event",
        &json!({ "part": 0, "staff": 0, "measure": 0, "index": 0 }),
    )
    .unwrap();
    s.execute("note.delete", &json!({})).unwrap();
    assert!(s.score.voice(0, 0, 0, 0).unwrap().events[0].is_rest());
}

#[test]
fn duration_change_on_selection() {
    let mut s = blank();
    s.execute("note.enter", &json!({ "step": "a" })).unwrap();
    s.execute("edit.escape", &json!({})).unwrap();
    s.execute(
        "select.event",
        &json!({ "part": 0, "staff": 0, "measure": 0, "index": 0 }),
    )
    .unwrap();
    s.execute("input.duration", &json!({ "value": "whole" })).unwrap();
    let v = s.score.voice(0, 0, 0, 0).unwrap();
    assert_eq!(v.events.len(), 1);
    assert_eq!(v.events[0].ticks(), TPQ * 4);
    assert!(matches!(v.events[0].kind, EventKind::Chord(_)));
}

#[test]
fn measures_signatures_parts() {
    let mut s = blank();
    s.execute("measure.append", &json!({ "count": 3 })).unwrap();
    assert_eq!(s.score.measures.len(), 7);
    s.execute("select.measure", &json!({ "part": 0, "staff": 0, "measure": 2 }))
        .unwrap();
    s.execute("measure.time", &json!({ "time": "3/4" })).unwrap();
    assert_eq!(s.score.measure_ticks(2), TPQ * 3);
    s.execute("measure.key", &json!({ "fifths": -3 })).unwrap();
    assert_eq!(s.score.key_at(5).fifths, -3);
    s.execute("measure.delete", &json!({})).unwrap();
    assert_eq!(s.score.measures.len(), 6);
    s.execute("part.add", &json!({ "instrument": "cello" })).unwrap();
    assert_eq!(s.score.parts.len(), 2);
    assert_eq!(s.score.parts[1].clefs[0], scoreart_model::Clef::Bass);
    assert!(s.execute("part.add", &json!({ "instrument": "kazoo" })).is_err());
    s.execute("score.meta", &json!({ "title": "Duo", "composer": "Me" }))
        .unwrap();
    assert_eq!(s.score.meta.title, "Duo");
}

#[test]
fn save_and_reopen() {
    let mut s = Session::default();
    for name in ["x.musicxml", "x.mxl"] {
        let bytes = s.save_bytes(name).unwrap();
        let mut t = Session::default();
        t.open_bytes(name, &bytes).unwrap();
        assert_eq!(t.score, s.score);
        assert_eq!(t.file_name, name);
    }
    assert!(!s.save_bytes("x.mid").unwrap().is_empty());
    assert!(
        String::from_utf8(s.save_bytes("x.svg").unwrap())
            .unwrap()
            .contains("<svg")
    );
    assert!(s.save_bytes("x.doc").is_err());
    let w = s.save_bytes("x.wav").unwrap();
    assert!(
        w.starts_with(b"RIFF") && w.len() > 44 + 44_100 * 4 * 10,
        "the sample lasts > 10 s"
    );
    assert!(!s.layout().pages.is_empty());
}

/// Every command, with every kind of junk parameter, in several states: no panic, and a
/// failing command leaves the score unchanged.
#[test]
fn hostile_params_never_panic() {
    let junk = [
        json!(null),
        json!({}),
        json!([]),
        json!("x"),
        json!({ "step": "zz", "value": -1, "voice": 99, "part": 1e30, "staff": -5, "measure": "NaN", "index": 1e9, "note": 77,
                "type": 5, "time": "0/0", "fifths": 1e9, "clef": [], "count": 1e12, "bpm": -1, "text": "x".repeat(5000),
                "semitones": i64::MAX, "instruments": [1, 2], "measures": -3, "factor": "big", "octave": 1e9, "alter": -1e9,
                "tick": u64::MAX, "instrument": "", "name": "" }),
        json!({ "step": "c", "value": 9, "voice": 0, "part": 0, "staff": 0, "measure": 0, "index": 0, "note": 0, "type": "sharp",
                "time": "7/8", "fifths": 7, "clef": "alto", "count": 2, "bpm": 60, "text": "ok", "semitones": 3,
                "instrument": "violin", "name": "V", "tick": 960, "octave": 9, "alter": 2, "factor": 1.5 }),
    ];
    let states: Vec<Box<dyn Fn() -> Session>> = vec![
        Box::new(Session::default),
        Box::new(|| {
            let mut s = Session::default();
            let _ = s.execute(
                "select.event",
                &json!({ "part": 0, "staff": 0, "measure": 0, "index": 0 }),
            );
            s
        }),
        Box::new(|| {
            let mut s = Session::default();
            let _ = s.execute("input.toggle", &json!({}));
            s
        }),
        Box::new(|| {
            let mut s = Session::default();
            let _ = s.execute("select.measure", &json!({ "part": 0, "staff": 1, "measure": 7 }));
            s
        }),
    ];
    for make in &states {
        for spec in registry() {
            for p in &junk {
                let mut s = make();
                let before = s.score.clone();
                if s.execute(spec.id, p).is_err() {
                    assert_eq!(s.score, before, "{} changed the score and failed", spec.id);
                }
                // The result must still lay out.
                let _ = s.layout();
            }
        }
    }
}
