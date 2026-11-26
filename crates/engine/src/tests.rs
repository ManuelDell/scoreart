use serde_json::json;

use scoreart_model::{EventKind, NoteValue, Score, Step, TPQ};

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

#[test]
fn opens_musescore_and_saves_musicxml() {
    let mscx = r#"<museScore version="4.40"><Score><Part><Staff id="1"/><Instrument><longName>Oboe</longName></Instrument></Part>
      <Staff id="1"><Measure><voice><TimeSig><sigN>2</sigN><sigD>4</sigD></TimeSig>
        <Chord><durationType>half</durationType><Note><pitch>72</pitch><tpc>14</tpc></Note></Chord></voice></Measure></Staff></Score></museScore>"#;
    let mut s = Session::default();
    s.open_bytes("Oboe Solo.mscx", mscx.as_bytes()).unwrap();
    assert_eq!(s.score.parts[0].name, "Oboe");
    assert!(s.status.contains("MuseScore"));
    assert!(s.save_bytes(&s.file_name).is_err(), "no MuseScore writer");
    let xml = s.save_bytes("Oboe Solo.musicxml").unwrap();
    assert!(
        String::from_utf8(xml)
            .unwrap()
            .contains("<part-name>Oboe</part-name>")
    );
}

#[test]
fn sound_and_mix_per_instrument() {
    let mut s = Session::default();
    s.execute("part.add", &json!({ "instrument": "violin" })).unwrap();
    assert!(
        s.execute("part.sound", &json!({ "part": 1, "library": "Not loaded" }))
            .is_err(),
        "only loaded libraries can be chosen"
    );
    s.execute("part.sound", &json!({ "part": 1, "library": "ScoreArt" }))
        .unwrap();
    s.execute("part.mix", &json!({ "part": 1, "volume": 55, "pan": -40 }))
        .unwrap();
    s.execute("part.mix", &json!({ "part": 0, "volume": 900, "pan": 900 }))
        .unwrap();
    assert_eq!(s.score.parts[1].sound.library, "ScoreArt");
    assert_eq!(
        (s.score.parts[1].sound.volume, s.score.parts[1].sound.pan),
        (55, -40)
    );
    assert_eq!(
        (s.score.parts[0].sound.volume, s.score.parts[0].sound.pan),
        (100, 100)
    );
    // Undoable like any edit.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.score.parts[0].sound.volume, 80);
    // Saved in MusicXML and read back.
    let back = scoreart_musicxml::from_str(&scoreart_musicxml::to_string(&s.score)).unwrap();
    assert_eq!(back.parts[1].sound, s.score.parts[1].sound);
    assert_eq!(back.parts[0].sound, s.score.parts[0].sound);
    // One WAV per instrument.
    let zip = s.save_bytes("Ode.zip").unwrap();
    let archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    let names: Vec<&str> = archive.file_names().collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"02 Violin.wav"), "{names:?}");
}

#[test]
fn instrument_changes() {
    let mut s = Session::default();
    // The sample is a piano (two staves): change the whole part to a violin.
    s.score.parts[0].sound.library = "ScoreArt".into();
    s.execute("part.change", &json!({ "part": 0, "instrument": "violin" }))
        .unwrap();
    let p = &s.score.parts[0];
    assert_eq!((p.name.as_str(), p.program, p.staff_count()), ("Violin", 40, 1));
    assert!(p.measures.iter().all(|m| m.staves.len() == 1));
    assert!(p.sound.library.is_empty(), "the sound goes back to automatic");
    assert!(
        s.execute("part.change", &json!({ "instrument": "kazoo" }))
            .is_err()
    );
    // From measure 3 the player switches to the flute.
    s.execute(
        "measure.instrument-change",
        &json!({ "part": 0, "measure": 2, "instrument": "flute" }),
    )
    .unwrap();
    let p = &s.score.parts[0];
    assert_eq!((p.program_at(1), p.program_at(2), p.program_at(7)), (40, 73, 73));
    assert_eq!(p.name_at(5), "Flute");
    // Played with the flute from there on.
    let seq = scoreart_midi::sequence(&s.score);
    let starts: std::collections::HashMap<usize, u64> = seq.measure_starts.iter().copied().collect();
    let m3 = starts[&2];
    assert!(seq.notes.iter().filter(|n| n.start < m3).all(|n| n.program == 40));
    assert!(
        seq.notes
            .iter()
            .filter(|n| n.start >= m3)
            .all(|n| n.program == 73)
    );
    // MIDI export switches the program; MusicXML keeps the change.
    let smf = scoreart_midi::to_smf(&s.score);
    assert!(smf.windows(2).any(|w| w[0] & 0xF0 == 0xC0 && w[1] == 73));
    let back = scoreart_musicxml::from_str(&scoreart_musicxml::to_string(&s.score)).unwrap();
    assert_eq!(
        back.parts[0].measures[2]
            .instrument_change
            .as_ref()
            .map(|c| (c.name.as_str(), c.program)),
        Some(("Flute", 73))
    );
    // Undo removes it again.
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.score.parts[0].measures[2].instrument_change.is_none());
    // Removing works through the same command.
    s.execute("edit.redo", &json!({})).unwrap();
    s.execute(
        "measure.instrument-change",
        &json!({ "part": 0, "measure": 2, "instrument": "" }),
    )
    .unwrap();
    assert!(s.score.parts[0].measures[2].instrument_change.is_none());
    // Audio still renders with both sounds.
    assert!(wav(&s.score).len() > 1000);
}

/// A "Women" part: soprano on staff 1, alto on staff 2 with a slur, and a flute below.
fn women_and_flute() -> Session {
    let mut s = blank();
    s.execute("part.change", &json!({ "part": 0, "instrument": "women" }))
        .unwrap();
    s.execute("part.add", &json!({ "instrument": "flute" })).unwrap();
    for (staff, steps) in [(0, ["g", "a"]), (1, ["c", "d"])] {
        s.execute(
            "cursor.set",
            &json!({ "part": 0, "staff": staff, "measure": 0, "tick": 0 }),
        )
        .unwrap();
        s.execute("input.duration", &json!({ "value": 6 })).unwrap();
        for st in steps {
            s.execute("note.enter", &json!({ "step": st })).unwrap();
        }
    }
    s.score.spanners.push(scoreart_model::Spanner {
        kind: scoreart_model::SpannerKind::Slur,
        part: 0,
        staff: 1,
        voice: 0,
        start: scoreart_model::Anchor { measure: 0, tick: 0 },
        end: scoreart_model::Anchor {
            measure: 0,
            tick: TPQ * 2,
        },
        dy: 0,
        spread: 0,
    });
    s.score.spanners.push(scoreart_model::Spanner {
        kind: scoreart_model::SpannerKind::Slur,
        part: 1,
        staff: 0,
        voice: 0,
        start: scoreart_model::Anchor { measure: 0, tick: 0 },
        end: scoreart_model::Anchor { measure: 1, tick: 0 },
        dy: 0,
        spread: 0,
    });
    s
}

fn steps(s: &Session, part: usize, staff: usize, voice: usize) -> Vec<Step> {
    s.score
        .voice(part, staff, 0, voice)
        .map(|v| {
            v.events
                .iter()
                .flat_map(|e| e.notes().iter().map(|n| n.pitch.step))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn changing_a_two_line_part_keeps_every_line() {
    // Merge: the alto becomes voice 2 of the soprano staff, its slur goes along.
    let mut s = women_and_flute();
    assert_eq!(s.score.parts[0].lines().len(), 2);
    s.execute("part.change", &json!({ "part": 0, "instrument": "soprano" }))
        .unwrap();
    let p = &s.score.parts[0];
    assert_eq!((p.name.as_str(), p.staff_count()), ("Soprano", 1));
    assert_eq!(steps(&s, 0, 0, 0), [Step::G, Step::A]);
    assert_eq!(steps(&s, 0, 0, 1), [Step::C, Step::D]);
    let alto_slur = s.score.spanners.iter().find(|sp| sp.part == 0).unwrap();
    assert_eq!((alto_slur.staff, alto_slur.voice), (0, 1));
    assert!(
        s.score.spanners.iter().any(|sp| sp.part == 1),
        "the flute keeps its slur"
    );

    // Split, keeping the top line: a new Soprano part above, Women keeps the alto.
    let mut s = women_and_flute();
    let r = s
        .execute(
            "part.change",
            &json!({ "part": 0, "instrument": "soprano", "mode": "split", "staff": 1 }),
        )
        .unwrap();
    assert_eq!(r["part"], 0);
    let names: Vec<&str> = s.score.parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Soprano", "Women", "Flute"]);
    assert_eq!(s.score.parts[1].staff_count(), 1);
    assert_eq!(steps(&s, 0, 0, 0), [Step::G, Step::A]);
    assert_eq!(steps(&s, 1, 0, 0), [Step::C, Step::D]);
    let parts: Vec<(usize, usize)> = s.score.spanners.iter().map(|sp| (sp.part, sp.staff)).collect();
    assert_eq!(parts, [(1, 0), (2, 0)], "slurs follow the alto and the flute");

    // Split, giving the alto away: Women keeps the soprano, a new Alto part below.
    let mut s = women_and_flute();
    s.execute(
        "part.change",
        &json!({ "part": 0, "instrument": "alto", "mode": "split", "staff": 2 }),
    )
    .unwrap();
    let names: Vec<&str> = s.score.parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Women", "Alto", "Flute"]);
    assert_eq!(steps(&s, 0, 0, 0), [Step::G, Step::A]);
    assert_eq!(steps(&s, 1, 0, 0), [Step::C, Step::D]);

    // Discard: only the chosen line stays.
    let mut s = women_and_flute();
    s.execute(
        "part.change",
        &json!({ "part": 0, "instrument": "alto", "mode": "discard", "staff": 2 }),
    )
    .unwrap();
    assert_eq!(s.score.parts.len(), 2);
    assert_eq!(steps(&s, 0, 0, 0), [Step::C, Step::D]);
    assert_eq!(s.score.parts[0].lines().len(), 1);

    // Undo brings the two staves back.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.score.parts[0].staff_count(), 2);
}

#[test]
fn moving_and_removing_parts_keeps_lines_attached() {
    let mut s = women_and_flute();
    s.execute("part.move", &json!({ "part": 1, "to": 0 })).unwrap();
    assert_eq!(s.score.parts[0].name, "Flute");
    let flute_slur = s.score.spanners.iter().find(|sp| sp.end.measure == 1).unwrap();
    assert_eq!(flute_slur.part, 0);
    s.execute("part.remove", &json!({ "part": 0 })).unwrap();
    assert_eq!(s.score.spanners.len(), 1);
    assert_eq!((s.score.spanners[0].part, s.score.spanners[0].staff), (0, 1));
}

#[test]
fn page_format_and_view() {
    let mut s = Session::default();
    s.execute(
        "page.format",
        &json!({ "size": "A5", "orientation": "landscape", "margin_mm": 10, "staff_space_mm": 1.5 }),
    )
    .unwrap();
    let p = s.score.page;
    assert_eq!(
        (p.width_mm, p.height_mm, p.margin_left_mm, p.staff_space_mm),
        (210.0, 148.0, 10.0, 1.5)
    );
    assert_eq!(p.size_name(), Some("A5"));
    assert!(s.execute("page.format", &json!({ "size": "A9" })).is_err());
    assert!(s.execute("page.format", &json!({ "width_mm": "wide" })).is_err());
    // Saved with the score.
    let back = scoreart_musicxml::from_str(&scoreart_musicxml::to_string(&s.score)).unwrap();
    assert_eq!(back.page, s.score.page);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.score.page, scoreart_model::PageFormat::default());
    // The view is not part of the score and not undone.
    s.execute("view.mode", &json!({ "mode": "horizontal" })).unwrap();
    assert_eq!(s.layout().pages.len(), 1);
    assert!(s.execute("view.mode", &json!({ "mode": "sideways" })).is_err());
}

#[test]
fn midi_keys_enter_notes_and_chords() {
    let mut s = blank();
    assert!(
        s.execute("note.midi", &json!({ "key": 60 })).is_err(),
        "only in note input"
    );
    s.execute("input.toggle", &json!({})).unwrap();
    s.execute("note.midi", &json!({ "key": 61 })).unwrap();
    s.execute("note.midi", &json!({ "key": 64, "chord": true }))
        .unwrap();
    s.execute("note.midi", &json!({ "key": 67 })).unwrap();
    let v = s.score.voice(0, 0, 0, 0).unwrap();
    let names: Vec<Vec<String>> = v.events[..2]
        .iter()
        .map(|e| e.notes().iter().map(|n| n.pitch.name()).collect())
        .collect();
    assert_eq!(names, [vec!["C#4".to_string(), "E4".into()], vec!["G4".into()]]);
    assert!(s.execute("note.midi", &json!({ "key": 200 })).is_err());
}

#[test]
fn dragging_notes_moves_them_and_changes_pitch() {
    let mut s = blank();
    s.execute("input.toggle", &json!({})).unwrap();
    s.execute("input.duration", &json!({ "value": 5 })).unwrap();
    for st in ["c", "d"] {
        s.execute("note.enter", &json!({ "step": st })).unwrap();
    }
    s.execute("input.toggle", &json!({})).unwrap();
    // Select the C with a staccato, move it to beat 4 of measure 2.
    s.execute(
        "select.event",
        &json!({ "part": 0, "staff": 0, "voice": 0, "measure": 0, "index": 0 }),
    )
    .unwrap();
    s.execute("note.articulation", &json!({ "type": "staccato" }))
        .unwrap();
    s.execute(
        "note.move-to",
        &json!({ "part": 0, "staff": 0, "measure": 1, "tick": TPQ * 3 }),
    )
    .unwrap();
    let m1 = s.score.voice(0, 0, 0, 0).unwrap();
    assert!(m1.events[0].is_rest(), "a rest takes the old place");
    assert_eq!(steps(&s, 0, 0, 0), [Step::D]);
    let m2 = s.score.voice(0, 0, 1, 0).unwrap();
    let moved = m2.events.iter().find(|e| !e.is_rest()).unwrap();
    assert_eq!(moved.notes()[0].pitch.step, Step::C);
    assert_eq!(moved.articulations.len(), 1, "marks go along");
    assert_eq!(
        m2.onsets()[m2.events.iter().position(|e| !e.is_rest()).unwrap()],
        TPQ * 3
    );
    // Dragging up three staff positions: C → F.
    s.execute("note.step-up", &json!({ "steps": 3 })).unwrap();
    let ev = s.selected_event().unwrap();
    assert_eq!(s.score.event(ev).unwrap().notes()[0].pitch.step, Step::F);
    // One undo brings the move back.
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(!s.score.voice(0, 0, 0, 0).unwrap().events[0].is_rest());
}

#[test]
fn texts_and_dynamics_move_and_keep_their_place() {
    let mut s = blank();
    s.execute("input.toggle", &json!({})).unwrap();
    s.execute("note.enter", &json!({ "step": "c" })).unwrap();
    s.execute("input.toggle", &json!({})).unwrap();
    s.execute(
        "select.event",
        &json!({ "part": 0, "staff": 0, "voice": 0, "measure": 0, "index": 0 }),
    )
    .unwrap();
    s.execute("note.dynamic", &json!({ "type": "mf" })).unwrap();
    s.execute("note.text", &json!({ "text": "dolce" })).unwrap();
    s.execute("note.place", &json!({ "kind": "dynamic", "dx": 1.5, "dy": -0.5 }))
        .unwrap();
    s.execute("note.place", &json!({ "kind": "text", "dx": -1.0, "dy": -2.0 }))
        .unwrap();
    let e = s.score.voice(0, 0, 0, 0).unwrap().events[0].clone();
    assert_eq!(e.dynamic_place.map(|p| (p.dx, p.dy)), Some((15, -5)));
    assert_eq!(e.text_place.map(|p| (p.dx, p.dy)), Some((-10, -20)));
    // Saved and opened again: the places stay.
    let back = scoreart_musicxml::from_str(&scoreart_musicxml::to_string(&s.score)).unwrap();
    let b = &back.voice(0, 0, 0, 0).unwrap().events[0];
    assert_eq!(b.dynamic_place, e.dynamic_place);
    assert_eq!(b.text_place, e.text_place);
    // Reset puts the dynamic back; undo moves it again.
    s.execute("note.place", &json!({ "kind": "dynamic", "reset": true }))
        .unwrap();
    assert!(
        s.score.voice(0, 0, 0, 0).unwrap().events[0]
            .dynamic_place
            .is_none()
    );
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(
        s.score.voice(0, 0, 0, 0).unwrap().events[0]
            .dynamic_place
            .is_some()
    );
    assert!(s.execute("note.place", &json!({ "kind": "lyric" })).is_err());
}

/// Samples of a 16-bit stereo WAV file written by `wav_file`.
fn wav_samples(w: &[u8]) -> Vec<f32> {
    let (pairs, _) = w.get(44..).unwrap_or(&[]).as_chunks::<2>();
    pairs
        .iter()
        .map(|b| f32::from(i16::from_le_bytes(*b)) / 32767.0)
        .collect()
}

#[test]
fn a_loud_tutti_is_turned_down_not_squashed() {
    // Twelve instruments hammering fff chords: far more than full scale when summed.
    let mut s = Session::default();
    let insts = [
        "violin",
        "violin",
        "viola",
        "cello",
        "contrabass",
        "flute",
        "oboe",
        "bassoon",
        "trombone",
        "tuba",
        "piano",
        "organ",
    ];
    s.execute("file.new", &json!({ "instruments": insts, "measures": 2 }))
        .unwrap();
    for part in 0..insts.len() {
        s.execute(
            "cursor.set",
            &json!({ "part": part, "staff": 0, "measure": 0, "tick": 0 }),
        )
        .unwrap();
        s.execute("input.toggle", &json!({})).unwrap();
        for st in ["c", "e", "g", "c"] {
            s.execute("note.enter", &json!({ "step": st })).unwrap();
            s.execute("note.add", &json!({ "step": "e" })).unwrap();
            s.execute("note.add", &json!({ "step": "g" })).unwrap();
        }
        s.execute("input.toggle", &json!({})).unwrap();
        s.execute(
            "select.event",
            &json!({ "part": part, "staff": 0, "voice": 0, "measure": 0, "index": 0 }),
        )
        .unwrap();
        s.execute("note.dynamic", &json!({ "type": "fff" })).unwrap();
    }
    let pcm = wav_samples(&s.save_bytes("x.wav").unwrap());
    let peak = pcm.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak > 0.8 && peak <= 0.9, "peak {peak}");
    // Not held against the ceiling the way a saturating limiter would.
    let near = pcm.iter().filter(|v| v.abs() > 0.85).count();
    assert!(
        near * 1000 < pcm.len(),
        "{near} of {} samples near full scale",
        pcm.len()
    );
}

#[test]
fn the_playback_style_is_kept_in_the_file() {
    let mut s = blank();
    let r = s
        .execute(
            "score.playback",
            &json!({ "reverb": 60, "humanize": false, "optimize": false }),
        )
        .unwrap();
    assert_eq!(r, json!({ "reverb": 60, "humanize": false, "optimize": false }));
    let back = scoreart_musicxml::from_str(&scoreart_musicxml::to_string(&s.score)).unwrap();
    assert_eq!(back.playback.reverb, 60);
    assert!(!back.playback.humanize);
    assert!(!back.playback.optimize);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.score.playback, scoreart_model::PlaybackStyle::default());
    // The default is not written at all.
    assert!(!scoreart_musicxml::to_string(&s.score).contains("scoreart-reverb"));
}

#[test]
fn lines_move_up_down_sideways_and_by_their_ends() {
    let mut s = blank();
    s.execute("input.toggle", &json!({})).unwrap();
    for st in ["c", "d", "e", "f", "g", "a", "b", "c"] {
        s.execute("note.enter", &json!({ "step": st })).unwrap();
    }
    s.execute("input.toggle", &json!({})).unwrap();
    s.execute(
        "select.event",
        &json!({ "part": 0, "staff": 0, "voice": 0, "measure": 0, "index": 0 }),
    )
    .unwrap();
    s.execute("select.extend-next", &json!({})).unwrap();
    s.execute("line.add", &json!({ "type": "crescendo" })).unwrap();
    let q = scoreart_model::TPQ;
    let sp = s.score.spanners[0];
    assert_eq!((sp.start.tick, sp.end.tick), (0, q));
    // Down by 1.5 staff spaces and two notes later.
    s.execute("line.move", &json!({ "index": 0, "dy": 1.5, "shift": 2 }))
        .unwrap();
    let sp = s.score.spanners[0];
    assert_eq!(sp.dy, 15);
    assert_eq!((sp.start.tick, sp.end.tick), (2 * q, 3 * q));
    // The end onto the next measure's second note.
    s.execute(
        "line.end",
        &json!({ "index": 0, "which": "end", "measure": 1, "tick": q }),
    )
    .unwrap();
    assert_eq!(
        (s.score.spanners[0].end.measure, s.score.spanners[0].end.tick),
        (1, q)
    );
    // A start after the end is refused.
    assert!(
        s.execute(
            "line.end",
            &json!({ "index": 0, "which": "start", "measure": 1, "tick": 3 * q })
        )
        .is_err()
    );
    assert!(
        s.execute("line.move", &json!({ "index": 0, "shift": 100 }))
            .is_err()
    );
    // Opened wider.
    s.execute("line.spread", &json!({ "index": 0, "spread": 2.4 }))
        .unwrap();
    assert_eq!(s.score.spanners[0].spread, 24);
    // Kept in MusicXML.
    let back = scoreart_musicxml::from_str(&scoreart_musicxml::to_string(&s.score)).unwrap();
    assert_eq!(back.spanners[0].dy, 15);
    assert_eq!(back.spanners[0].spread, 24);
    assert_eq!(back.spanners[0].end, s.score.spanners[0].end);
    // Undo brings it back.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.score.spanners[0].spread, 0);
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.score.spanners[0].dy, 0);
    // And it is drawn moved, with a box to pick it up.
    s.execute("edit.redo", &json!({})).unwrap();
    assert!(
        s.execute("line.spread", &json!({ "index": 0, "spread": "wide" }))
            .is_err()
    );
    let layout = s.layout();
    assert!(
        layout
            .hits
            .iter()
            .any(|h| matches!(h.target, scoreart_engrave::Target::Line { index: 0, .. }))
    );
}

#[test]
fn text_styles_and_blocks() {
    use scoreart_engrave::Target;
    use scoreart_model::{BlockRef, TextFont, TextKind};
    let mut s = Session::default();
    // Styles: set, read back, reset; "all" changes every font.
    s.execute(
        "text.style",
        &json!({ "kind": "title", "size": 30, "bold": true, "font": "sans" }),
    )
    .unwrap();
    let st = s.score.text_styles.get(TextKind::Title);
    assert_eq!((st.size, st.bold, st.font), (30.0, true, TextFont::Sans));
    assert!(s.execute("text.style", &json!({ "kind": "nope" })).is_err());
    assert!(
        s.execute("text.style", &json!({ "kind": "title", "align": "up" }))
            .is_err()
    );
    let big_title = |s: &mut Session| {
        s.layout().pages[0]
            .items
            .iter()
            .find_map(|it| match &it.shape {
                scoreart_engrave::Shape::Text {
                    kind: Some(TextKind::Title),
                    size,
                    bold,
                    ..
                } => Some((*size, *bold)),
                _ => None,
            })
            .unwrap()
    };
    let (size, bold) = big_title(&mut s);
    assert!(
        bold && size > 5.0,
        "30 pt title is larger than the default ({size})"
    );
    s.execute("text.style", &json!({ "kind": "all", "font": "sans" }))
        .unwrap();
    assert!(
        TextKind::ALL
            .iter()
            .all(|k| s.score.text_styles.get(*k).font == TextFont::Sans)
    );
    s.execute("text.style", &json!({ "kind": "all", "reset": true }))
        .unwrap();
    assert!(s.score.text_styles.0.is_empty());

    // Blocks: a heading above measure 5, a header; moved by hand; edited; removed.
    s.execute(
        "block.add",
        &json!({ "kind": "heading", "text": "Part two", "measure": 4 }),
    )
    .unwrap();
    s.execute("block.add", &json!({ "kind": "footer", "text": "Footer" }))
        .unwrap();
    // A footer text goes into the middle of the footer (as in MuseScore), not into a block.
    assert!(s.score.header_footer.show_footer);
    assert_eq!(s.score.header_footer.footer[0][1], "Footer");
    assert_eq!(s.score.blocks.len(), 1);
    assert!(
        s.execute("block.add", &json!({ "kind": "title", "text": "x" }))
            .is_err()
    );
    assert!(
        s.execute("block.add", &json!({ "kind": "text", "text": "  " }))
            .is_err()
    );
    let layout = s.layout();
    let heading = layout
        .hits
        .iter()
        .find(|h| {
            h.target
                == Target::Block {
                    block: BlockRef::Added(0),
                }
        })
        .copied()
        .expect("the heading can be picked");
    // It sits above the system that holds measure 5.
    let sys = layout.staves.iter().find(|b| b.measure == 4).unwrap();
    assert!(heading.y1 <= sys.top && heading.page == sys.page);
    s.execute("block.move", &json!({ "block": 0, "dx": 2.5, "dy": -1 }))
        .unwrap();
    assert_eq!((s.score.blocks[0].dx, s.score.blocks[0].dy), (25, -10));
    let moved = s
        .layout()
        .hits
        .iter()
        .find(|h| {
            h.target
                == Target::Block {
                    block: BlockRef::Added(0),
                }
        })
        .copied()
        .unwrap();
    assert!((moved.x0 - heading.x0 - 2.5).abs() < 0.01 && (moved.y0 - heading.y0 + 1.0).abs() < 0.01);
    s.execute("block.move", &json!({ "block": "title", "dx": 0, "dy": 3 }))
        .unwrap();
    assert_eq!(s.score.block_offsets.get(&TextKind::Title), Some(&(0, 30)));
    s.execute(
        "block.edit",
        &json!({ "block": "subtitle", "text": "New subtitle" }),
    )
    .unwrap();
    assert_eq!(s.score.meta.subtitle, "New subtitle");
    assert!(
        s.execute("block.edit", &json!({ "block": 9, "text": "x" }))
            .is_err()
    );
    s.execute("block.remove", &json!({ "block": "page-number" }))
        .unwrap();
    assert!(s.score.hide_page_numbers);
    s.execute("page.numbers", &json!({ "on": true })).unwrap();
    assert!(!s.score.hide_page_numbers);
    // Inserting measures before it keeps the heading with its measure.
    s.execute("select.measure", &json!({ "part": 0, "staff": 0, "measure": 0 }))
        .unwrap();
    s.execute("measure.insert", &json!({ "count": 2 })).unwrap();
    assert_eq!(s.score.blocks[0].measure, 6);
    // Everything survives saving.
    let bytes = s.save_bytes("x.musicxml").unwrap();
    let mut t = Session::default();
    t.open_bytes("x.musicxml", &bytes).unwrap();
    assert_eq!(t.score.blocks, s.score.blocks);
    assert_eq!(t.score.block_offsets, s.score.block_offsets);
    assert_eq!(t.score.header_footer, s.score.header_footer);
    s.execute("block.remove", &json!({ "block": 0 })).unwrap();
    assert_eq!(s.score.blocks.len(), 0);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.score.blocks.len(), 1);
}

#[test]
fn page_furniture_from_page_two() {
    use scoreart_model::TextKind;
    let mut s = Session::default();
    s.execute("measure.append", &json!({ "count": 120 })).unwrap();
    s.execute("block.add", &json!({ "kind": "header", "text": "My header" }))
        .unwrap();
    s.execute("score.meta", &json!({ "copyright": "© me" })).unwrap();
    let l = s.layout();
    assert!(l.pages.len() >= 2);
    let kinds = |p: usize| -> Vec<TextKind> {
        l.pages[p]
            .items
            .iter()
            .filter_map(|it| match &it.shape {
                scoreart_engrave::Shape::Text { kind: Some(k), .. } => Some(*k),
                _ => None,
            })
            .collect()
    };
    assert!(kinds(0).contains(&TextKind::Copyright) && !kinds(0).contains(&TextKind::Header));
    assert!(kinds(1).contains(&TextKind::Header) && kinds(1).contains(&TextKind::PageNumber));
    s.execute("page.numbers", &json!({ "on": false })).unwrap();
    assert!(!s.layout().pages[1].items.iter().any(|it| matches!(
        &it.shape,
        scoreart_engrave::Shape::Text {
            kind: Some(TextKind::PageNumber),
            ..
        }
    )));
}

/// Bugs found writing a 6/8 hymn with five verses, voltas "1.+3." / "2.4.5." and a refrain.
#[test]
fn hymn_in_six_eight() {
    use scoreart_engrave::Shape;
    let mut s = Session::default();
    s.execute(
        "file.new",
        &json!({ "instruments": ["piano"], "time": "6/8", "measures": 6, "tempo": 55 }),
    )
    .unwrap();
    // A new 6/8 score counts dotted quarters.
    let t = s.score.measures[0].tempo.unwrap();
    assert_eq!((t.bpm, t.unit), (55, scoreart_model::BeatUnit::DottedQuarter));
    assert!((s.score.tempo_at(0) - 82.5).abs() < 1e-9);
    // Pickup: a one-eighth first measure, not counted.
    s.execute("select.measure", &json!({ "part": 0, "staff": 0, "measure": 0 }))
        .unwrap();
    s.execute("measure.length", &json!({ "length": "1/8" })).unwrap();
    assert_eq!(s.score.measure_ticks(0), TPQ / 2);
    assert_eq!(s.score.measure_number(1), 1);
    assert!(s.execute("measure.length", &json!({ "length": "1/7" })).is_err());
    // A short first ending, labelled "1.+3.".
    s.execute("select.measure", &json!({ "part": 0, "staff": 0, "measure": 2 }))
        .unwrap();
    s.execute("measure.length", &json!({ "length": "5/8" })).unwrap();
    s.execute("measure.volta", &json!({ "number": 1, "text": "1.+3." }))
        .unwrap();
    s.execute("measure.barline", &json!({ "type": "repeat-end" }))
        .unwrap();
    assert_eq!(s.score.measures[2].volta_text.as_deref(), Some("1.+3."));
    // Tempo word and a dotted-quarter metronome mark in one place.
    s.execute("select.measure", &json!({ "part": 0, "staff": 0, "measure": 3 }))
        .unwrap();
    s.execute("measure.tempo", &json!({ "bpm": 70, "unit": "dotted-quarter" }))
        .unwrap();
    s.execute("measure.tempo-text", &json!({ "text": "schneller" }))
        .unwrap();
    assert!(
        s.execute("measure.tempo", &json!({ "bpm": 70, "unit": "crotchet" }))
            .is_err()
    );
    // A verse number, a dynamic with a hairpin running into it, a long staff text at the end.
    s.execute(
        "cursor.set",
        &json!({ "part": 0, "staff": 0, "measure": 4, "tick": 0 }),
    )
    .unwrap();
    s.execute("input.toggle", &json!({})).unwrap();
    s.execute("input.duration", &json!({ "value": "quarter" }))
        .unwrap();
    s.execute("input.dot", &json!({})).unwrap();
    for st in ["a", "b", "c", "d"] {
        s.execute("note.enter", &json!({ "step": st, "octave": 4 }))
            .unwrap();
    }
    s.execute("input.toggle", &json!({})).unwrap();
    s.execute(
        "select.event",
        &json!({ "part": 0, "staff": 0, "measure": 4, "index": 0 }),
    )
    .unwrap();
    s.execute("note.lyric", &json!({ "text": "1. Mit" })).unwrap();
    s.execute(
        "select.event",
        &json!({ "part": 0, "staff": 0, "measure": 4, "index": 1, "extend": true }),
    )
    .unwrap();
    s.execute("line.add", &json!({ "type": "crescendo" })).unwrap();
    s.execute(
        "select.event",
        &json!({ "part": 0, "staff": 0, "measure": 4, "index": 1 }),
    )
    .unwrap();
    s.execute("note.dynamic", &json!({ "type": "f" })).unwrap();
    s.execute(
        "select.event",
        &json!({ "part": 0, "staff": 0, "measure": 5, "index": 1 }),
    )
    .unwrap();
    s.execute(
        "note.text",
        &json!({ "text": "2., 4.: zu Refrain 1 · 5.: zu Refrain 2" }),
    )
    .unwrap();

    // Saved and opened again, everything is still there (the metronome keeps its unit).
    let bytes = s.save_bytes("x.musicxml").unwrap();
    let mut back = Session::default();
    back.open_bytes("x.musicxml", &bytes).unwrap();
    assert_eq!(back.score.measures[3].tempo, s.score.measures[3].tempo);
    assert_eq!(back.score.measures[0].tempo, s.score.measures[0].tempo);
    assert_eq!(back.score.measures[2].volta_text, s.score.measures[2].volta_text);
    assert_eq!(back.score.measure_ticks(0), TPQ / 2);

    let l = s.layout();
    let items: Vec<&Shape> = l
        .pages
        .iter()
        .flat_map(|p| p.items.iter().map(|i| &i.shape))
        .collect();
    // The dotted metronome mark: a dot glyph and "= 70".
    assert!(
        items
            .iter()
            .any(|sh| matches!(sh, Shape::Glyph { c, .. } if *c == '\u{ECB7}'))
    );
    assert!(
        items
            .iter()
            .any(|sh| matches!(sh, Shape::Text { text, .. } if text == "= 70"))
    );
    // The verse number stands apart, left of its syllable.
    let num = items.iter().find_map(|sh| match sh {
        Shape::Text { text, x, .. } if text == "1." => Some(*x),
        _ => None,
    });
    let syl = items.iter().find_map(|sh| match sh {
        Shape::Text { text, x, .. } if text == "Mit" => Some(*x),
        _ => None,
    });
    assert!(num.unwrap() < syl.unwrap());
    // The hairpin stops before the f it leads to.
    let f_x = s.layout().hits.iter().find_map(|h| match h.target {
        scoreart_engrave::Target::Dynamic { .. } => Some(h.x0),
        _ => None,
    });
    let hairpin_end = items
        .iter()
        .filter_map(|sh| match sh {
            // Hairpin strokes: long and slightly slanted.
            Shape::Line {
                x1,
                x2,
                y1,
                y2,
                width,
            } if *width == 0.16
                && (x2 - x1).abs() > 1.0
                && (y1 - y2).abs() > 0.05
                && (y1 - y2).abs() < 1.5 =>
            {
                Some(x1.max(*x2))
            }
            _ => None,
        })
        .fold(f32::MIN, f32::max);
    assert!(hairpin_end < f_x.unwrap(), "{hairpin_end} < {f_x:?}");
    // No text runs past the right page margin.
    let page = &l.pages[0];
    let right = page.width - s.score.page.margin_right_mm / s.score.page.staff_space_mm + 0.5;
    for sh in items.iter() {
        if let Shape::Text {
            text,
            x,
            size,
            align: scoreart_engrave::Align::Left,
            ..
        } = sh
        {
            let w = scoreart_engrave::text_width(text, *size);
            assert!(
                x + w <= right + 1.0,
                "{text} runs off the page: x {x} size {size} w {w} right {right}"
            );
        }
    }
    // The empty bass staff of the short ending shows one measure rest, not a split one.
    let rests_in = |m: usize| {
        l.hits
            .iter()
            .filter(|h| matches!(h.target, scoreart_engrave::Target::Rest { ev } if ev.measure == m && ev.staff == 1))
            .count()
    };
    assert_eq!(rests_in(2), 1);
    // The last system is short: with the fill threshold above its share of the line, it ends
    // where its music ends (MuseScore's "last system fill threshold").
    s.execute("page.format", &json!({ "last_system_fill": 0.9 }))
        .unwrap();
    let l = s.layout();
    let page = &l.pages[0];
    let last = l
        .staves
        .iter()
        .filter(|b| b.page == l.pages.len() - 1)
        .map(|b| b.x1)
        .fold(0.0f32, f32::max);
    assert!(last < page.width * 0.8, "{last}");
}

/// Legato is the norm; slurred notes join even more; staccato only where it is written.
#[test]
fn legato_by_default_slurs_join_staccato_when_written() {
    let mut s = Session::default();
    s.execute(
        "file.new",
        &json!({ "instruments": ["piano"], "names": ["Chor"], "measures": 3 }),
    )
    .unwrap();
    s.execute(
        "cursor.set",
        &json!({ "part": 0, "staff": 0, "measure": 0, "tick": 0 }),
    )
    .unwrap();
    s.execute("input.toggle", &json!({})).unwrap();
    s.execute("input.duration", &json!({ "value": "quarter" }))
        .unwrap();
    // Measure 1: plain C D E F; measure 2: slurred G A G A; measure 3: staccato C C C C.
    for st in ["c", "d", "e", "f", "g", "a", "g", "a", "c", "c", "c", "c"] {
        s.execute("note.enter", &json!({ "step": st, "octave": 5 }))
            .unwrap();
    }
    s.execute("input.toggle", &json!({})).unwrap();
    s.execute(
        "select.event",
        &json!({ "part": 0, "staff": 0, "measure": 1, "index": 0 }),
    )
    .unwrap();
    s.execute(
        "select.event",
        &json!({ "part": 0, "staff": 0, "measure": 1, "index": 3, "extend": true }),
    )
    .unwrap();
    s.execute("line.add", &json!({ "type": "slur" })).unwrap();
    for i in 0..4 {
        s.execute(
            "select.event",
            &json!({ "part": 0, "staff": 0, "measure": 2, "index": i }),
        )
        .unwrap();
        s.execute("note.articulation", &json!({ "type": "staccato" }))
            .unwrap();
    }
    // Saved and opened again: a piano called "Chor" stays a piano.
    let bytes = s.save_bytes("x.musicxml").unwrap();
    let mut t = Session::default();
    t.open_bytes("x.musicxml", &bytes).unwrap();
    assert_eq!(t.score.parts[0].program, 0);

    let seq = scoreart_midi::sequence(&t.score);
    let q = u64::from(TPQ);
    let n = &seq.notes;
    assert_eq!(n.len(), 12);
    for p in &n[0..4] {
        assert!(
            p.ticks * 100 >= q * 97,
            "plain notes are legato: {} of {q}",
            p.ticks
        );
    }
    for p in &n[4..7] {
        assert!(p.ticks > q, "slurred notes run into the next: {}", p.ticks);
    }
    for p in &n[8..12] {
        assert!(p.ticks * 100 <= q * 55, "staccato is short: {}", p.ticks);
    }
    // No two notes of one key overlap (a repeated key is struck again cleanly).
    for (a, b) in n.iter().zip(n.iter().skip(1)) {
        if a.key == b.key && a.part == b.part {
            assert!(a.start + a.ticks < b.start || b.start < a.start);
        }
    }
}

/// Pages laid out as MuseScore lays them out: margins of odd and even pages, the music top
/// margin, systems spread to fill a page, headers and footers with their macros.
#[test]
fn pages_like_musescore() {
    use scoreart_engrave::{Align, Shape};
    use scoreart_model::TextKind;
    let mut s = Session::default();
    s.execute(
        "file.new",
        &json!({ "instruments": ["piano"], "measures": 220, "title": "" }),
    )
    .unwrap();
    s.execute("score.meta", &json!({ "title": "", "copyright": "© Test" }))
        .unwrap();
    let texts =
        |l: &scoreart_engrave::Layout, page: usize| -> Vec<(String, Option<TextKind>, f32, f32, Align)> {
            l.pages[page]
                .items
                .iter()
                .filter_map(|it| match &it.shape {
                    Shape::Text {
                        text,
                        kind,
                        x,
                        y,
                        align,
                        ..
                    } => Some((text.clone(), *kind, *x, *y, *align)),
                    _ => None,
                })
                .collect()
        };
    let l = s.layout();
    assert!(l.pages.len() >= 3, "{} pages", l.pages.len());
    let sp = s.score.page.staff_space_mm;
    let (pw, ph) = (s.score.page.width_mm / sp, s.score.page.height_mm / sp);
    // As in a new MuseScore 4 score: no page number on page 1, "2" bottom left on page 2,
    // "3" bottom right on page 3; the copyright in the middle of every footer.
    let has = |p: usize, t: &str| texts(&l, p).into_iter().find(|x| x.0 == t);
    assert!(has(0, "1").is_none());
    let two = has(1, "2").expect("page number 2");
    assert_eq!((two.1, two.4), (Some(TextKind::PageNumber), Align::Left));
    assert!(two.3 > ph * 0.9, "in the footer");
    let three = has(2, "3").expect("page number 3");
    assert_eq!(three.4, Align::Right);
    for p in 0..3 {
        let c = has(p, "© Test").expect("copyright");
        assert!((c.2 - pw / 2.0).abs() < 0.01 && c.3 > ph * 0.9);
    }
    // The first staff of a page keeps the music top margin below the top margin.
    let top_of = |l: &scoreart_engrave::Layout, page: usize| {
        l.staves
            .iter()
            .filter(|b| b.page == page)
            .map(|b| b.top)
            .fold(f32::MAX, f32::min)
    };
    let mt = s.score.page.margin_top_mm / sp;
    assert!(top_of(&l, 1) >= mt + s.score.page.music_top - 0.01);
    // Pages before the last are filled: their last staff ends near the music bottom margin,
    // systems no further apart than the largest system distance.
    let bottom = ph - s.score.page.margin_bottom_mm / sp - s.score.page.music_bottom;
    let last_staff = l
        .staves
        .iter()
        .filter(|b| b.page == 0)
        .map(|b| b.top + 4.0)
        .fold(0.0f32, f32::max);
    let tops: Vec<f32> = {
        let mut t: Vec<f32> = l
            .staves
            .iter()
            .filter(|b| b.page == 0 && b.staff == 0)
            .map(|b| b.top)
            .collect();
        t.sort_by(f32::total_cmp);
        t.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        t
    };
    let spread_full = bottom - last_staff < 1.0;
    let gaps_at_max = tops
        .windows(2)
        .all(|w| w[1] - w[0] <= 4.0 + 6.5 + 4.0 + 15.0 + 0.01);
    assert!(
        spread_full || gaps_at_max,
        "page 1 is filled ({last_staff} of {bottom})"
    );

    // Two-sided with other even margins, page numbers from 5, a header with macros.
    s.execute(
        "page.format",
        &json!({ "two_sided": true, "even_margin_left_mm": 30, "first_page_number": 5 }),
    )
    .unwrap();
    s.execute(
        "page.header-footer",
        &json!({ "show_header": true, "header_first_page": true, "header_odd_even": false, "odd_header": ["$:workTitle:", "Seite $P von $n", "$$"] }),
    )
    .unwrap();
    s.execute("score.meta", &json!({ "title": "Mein Stück" }))
        .unwrap();
    let l = s.layout();
    let left = |l: &scoreart_engrave::Layout, page: usize| {
        l.staves
            .iter()
            .filter(|b| b.page == page)
            .map(|b| b.x0)
            .fold(f32::MAX, f32::min)
    };
    // Page 1 is number 5 (odd margins), page 2 number 6 (even: 30 mm on the left).
    assert!(left(&l, 1) - left(&l, 0) > (30.0 - 10.0) / sp - 0.5);
    let pages = l.pages.len();
    assert!(has_text(&l, 0, &format!("Seite 5 von {pages}")));
    assert!(has_text(&l, 1, "Mein Stück") && has_text(&l, 1, "$"));
    assert!(has_text(&l, 1, "6"), "the footer's $p on page 6");
    // No page numbers at all.
    s.execute("page.numbers", &json!({ "on": false })).unwrap();
    let l = s.layout();
    assert!(!has_text(&l, 1, "6"));
    // Saved and opened again, all of it is still there.
    let bytes = s.save_bytes("x.musicxml").unwrap();
    let mut t = Session::default();
    t.open_bytes("x.musicxml", &bytes).unwrap();
    assert_eq!(t.score.page, s.score.page);
    assert_eq!(t.score.header_footer, s.score.header_footer);
    assert_eq!(t.score.hide_page_numbers, s.score.hide_page_numbers);
}

fn has_text(l: &scoreart_engrave::Layout, page: usize, t: &str) -> bool {
    l.pages[page]
        .items
        .iter()
        .any(|it| matches!(&it.shape, scoreart_engrave::Shape::Text { text, .. } if text == t))
}

#[test]
fn spacers_set_the_distance_to_the_next_staff_and_system() {
    let mut s = Session::default();
    s.execute(
        "file.new",
        &json!({ "instruments": ["piano"], "measures": 24, "title": "" }),
    )
    .unwrap();
    let gap = |s: &mut Session, a: (usize, usize), b: (usize, usize)| {
        let l = s.layout();
        l.staff_box(0, b.1, b.0).unwrap().top - l.staff_box(0, a.1, a.0).unwrap().top
    };
    let natural = gap(&mut s, (0, 0), (0, 1));
    // Spacer down below the right hand of measure 1: more room to the left hand.
    s.execute("select.measure", &json!({ "part": 0, "staff": 0, "measure": 0 }))
        .unwrap();
    s.execute("staff.spacer", &json!({ "type": "down", "height": 12 }))
        .unwrap();
    assert_eq!(s.score.staff_measure(0, 0, 0).unwrap().spacer_down, Some(12.0));
    let wide = gap(&mut s, (0, 0), (0, 1));
    assert!(
        (wide - 16.0).abs() < 0.01,
        "4 lines + 12 spaces: {wide} (natural {natural})"
    );
    // A fixed spacer replaces it and is exact, even when smaller than the natural gap.
    s.execute("staff.spacer", &json!({ "type": "fixed", "height": 3 }))
        .unwrap();
    let sm = s.score.staff_measure(0, 0, 0).unwrap();
    assert_eq!((sm.spacer_down, sm.spacer_fixed), (None, Some(3.0)));
    assert!((gap(&mut s, (0, 0), (0, 1)) - 7.0).abs() < 0.01);
    // Spacer up above the left hand: room above it.
    s.execute("staff.spacer", &json!({ "type": "none" })).unwrap();
    s.execute("select.measure", &json!({ "part": 0, "staff": 1, "measure": 0 }))
        .unwrap();
    s.execute("staff.spacer", &json!({ "type": "up", "height": 14 }))
        .unwrap();
    assert!((gap(&mut s, (0, 0), (0, 1)) - 18.0).abs() < 0.01);
    s.execute("staff.spacer", &json!({ "type": "none" })).unwrap();
    assert!((gap(&mut s, (0, 0), (0, 1)) - natural).abs() < 0.01);
    // Below the last staff of a system: the next system on the page keeps that distance.
    let l = s.layout();
    let first = l.staff_box(0, 1, 0).unwrap().clone();
    let next = l
        .staves
        .iter()
        .find(|b| b.system == first.system + 1 && b.part == 0 && b.staff == 0)
        .unwrap()
        .clone();
    assert_eq!(next.page, first.page);
    s.execute("select.measure", &json!({ "part": 0, "staff": 1, "measure": 0 }))
        .unwrap();
    s.execute("staff.spacer", &json!({ "type": "fixed", "height": 20 }))
        .unwrap();
    let l = s.layout();
    let a = l.staff_box(0, 1, 0).unwrap();
    let b = l.staff_box(0, 0, next.measure).unwrap();
    assert!((b.top - a.top - 24.0).abs() < 0.01, "{} → {}", a.top, b.top);
    // The editor sees the spacer as a mark; nothing of it is printed.
    assert!(
        l.marks
            .iter()
            .any(|m| matches!(m.kind, scoreart_engrave::MarkKind::SpacerFixed(h) if h == 20.0))
    );
    let undo_to = s.score.clone();
    s.execute("staff.spacer", &json!({ "type": "fixed", "height": 0 }))
        .unwrap();
    assert_eq!(s.score.staff_measure(0, 1, 0).unwrap().spacer_fixed, None);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.score, undo_to);
}

#[test]
fn a_section_break_starts_a_new_section() {
    let mut s = Session::default();
    s.execute(
        "file.new",
        &json!({ "instruments": ["piano"], "measures": 8, "title": "" }),
    )
    .unwrap();
    s.execute("input.toggle", &json!({})).unwrap();
    s.execute("input.duration", &json!({ "value": "whole" })).unwrap();
    for _ in 0..6 {
        s.execute("note.enter", &json!({ "step": "c" })).unwrap();
    }
    s.execute("input.toggle", &json!({})).unwrap();
    let before = scoreart_midi::sequence(&s.score);
    s.execute("select.measure", &json!({ "part": 0, "staff": 0, "measure": 3 }))
        .unwrap();
    s.execute("measure.break", &json!({ "kind": "section" })).unwrap();
    let sec = s.score.measures[3].section.unwrap();
    assert_eq!(
        (sec.pause, sec.long_names, sec.restart_numbers),
        (3.0, true, true)
    );
    // Measure numbers count from 1 again after it.
    let numbers: Vec<usize> = (0..8).map(|m| s.score.measure_number(m)).collect();
    assert_eq!(numbers, [1, 2, 3, 4, 1, 2, 3, 4]);
    // A new system starts after it, with the long instrument names.
    let l = s.layout();
    let (a, b) = (l.staff_box(0, 0, 3).unwrap(), l.staff_box(0, 0, 4).unwrap());
    assert_ne!(a.system, b.system);
    assert!(s.score.section_starts_with_long_names(4));
    assert!(
        l.marks
            .iter()
            .any(|m| m.measure == 3 && m.kind == scoreart_engrave::MarkKind::SectionBreak)
    );
    // Playback waits three seconds before the next section.
    let tick = |m: u64| m * 4 * scoreart_model::TPQ as u64;
    let after = scoreart_midi::sequence(&s.score);
    let extra = after.seconds_at(tick(4)) - before.seconds_at(tick(4));
    // (Three seconds of silence; the last 32nd of the measure is part of them.)
    assert!((2.85..=3.0).contains(&extra), "pause {extra}");
    let last = after
        .notes
        .iter()
        .filter(|n| n.start < tick(4))
        .map(|n| n.start + n.ticks)
        .max()
        .unwrap();
    assert!(last < tick(4), "the last notes stop before the pause");
    // Settings: no pause, keep counting, short names.
    s.execute(
        "measure.break",
        &json!({ "kind": "section", "pause": 0, "long_names": false, "restart_numbers": false }),
    )
    .unwrap();
    assert_eq!(s.score.measure_number(4), 5);
    assert!(!s.score.section_starts_with_long_names(4));
    let after = scoreart_midi::sequence(&s.score);
    assert!((after.seconds_at(tick(4)) - before.seconds_at(tick(4))).abs() < 0.01);
    // Toggling it off, or a line break in its place, removes it.
    s.execute("measure.break", &json!({ "kind": "line" })).unwrap();
    assert!(s.score.measures[3].section.is_none());
    s.execute("measure.break", &json!({ "kind": "section" })).unwrap();
    s.execute("measure.break", &json!({ "kind": "section" })).unwrap();
    assert!(s.score.measures[3].section.is_none());
}

#[test]
fn header_macros_for_dates_files_and_parts() {
    // 2026-11-24 13:05 (local clock of the host).
    let mut s = Session {
        clock: Some(1_795_525_500),
        ..Session::default()
    };
    s.execute(
        "file.new",
        &json!({ "instruments": ["piano"], "measures": 4, "title": "" }),
    )
    .unwrap();
    assert_eq!(s.score.meta.created, "2026-11-24");
    // A change stamps the time of the last change.
    s.execute("score.meta", &json!({ "composer": "N. N." })).unwrap();
    s.file_name = "Lied.musicxml".into();
    s.file_path = "/home/a/Lied.musicxml".into();
    let mut hf = s.score.header_footer.clone();
    hf.header[0][0] = "$d|$D|$M|$m|$f|$F|[$i][$I][$:partName:]|$:creationDate:".into();
    hf.show_header = true;
    hf.header_first_page = true;
    s.score.header_footer = hf;
    let header = |s: &mut Session| -> Option<String> {
        s.layout().pages[0].items.iter().find_map(|it| match &it.shape {
            scoreart_engrave::Shape::Text { text, .. } if text.contains('|') => Some(text.clone()),
            _ => None,
        })
    };
    assert_eq!(
        header(&mut s).unwrap(),
        "2026-11-24|2026-11-24|2026-11-24|13:05|Lied.musicxml|/home/a/Lied.musicxml|[][][]|2026-11-24"
    );
    s.date_style = scoreart_model::DateStyle::German;
    s.score.meta.created = "2025-03-01".into();
    s.modified = None;
    let h = header(&mut s).unwrap();
    assert!(h.starts_with("24.11.2026|01.03.2025|01.03.2025||"), "{h}");
}

#[test]
fn locked_text_blocks_stay_put_until_unlocked() {
    let mut s = Session::default();
    s.execute(
        "file.new",
        &json!({ "instruments": ["piano"], "measures": 4, "title": "Lied", "composer": "N. N." }),
    )
    .unwrap();
    s.execute(
        "block.add",
        &json!({ "kind": "text", "text": "Strophe", "measure": 0 }),
    )
    .unwrap();
    // Free: it moves.
    s.execute("block.move", &json!({ "block": 0, "dx": 2, "dy": 1 }))
        .unwrap();
    assert_eq!((s.score.blocks[0].dx, s.score.blocks[0].dy), (20, 10));
    // Locked: it stays where it is, also "back to its place" is refused.
    let r = s.execute("block.lock", &json!({ "block": 0 })).unwrap();
    assert_eq!(r, json!({ "locked": true }));
    let err = s
        .execute("block.move", &json!({ "block": 0, "dx": 0, "dy": 0 }))
        .unwrap_err();
    assert_eq!(err.to_string(), crate::TEXT_LOCKED);
    assert_eq!((s.score.blocks[0].dx, s.score.blocks[0].dy), (20, 10));
    // The title page's texts lock alike.
    s.execute("block.lock", &json!({ "block": "title", "locked": true }))
        .unwrap();
    assert!(
        s.execute("block.move", &json!({ "block": "title", "dx": 1, "dy": 0 }))
            .is_err()
    );
    assert!(
        s.execute("block.move", &json!({ "block": "composer", "dx": 1, "dy": 0 }))
            .is_ok()
    );
    // Unlocked again: free to move.
    s.execute("block.lock", &json!({ "block": 0, "locked": false }))
        .unwrap();
    s.execute("block.move", &json!({ "block": 0, "dx": -1, "dy": 0 }))
        .unwrap();
    assert_eq!(s.score.blocks[0].dx, -10);
    // All at once; undo brings the previous state back.
    s.execute("block.lock", &json!({ "block": "all", "locked": true }))
        .unwrap();
    use scoreart_model::{BlockRef, TextKind};
    for b in [
        BlockRef::Added(0),
        BlockRef::Fixed(TextKind::Title),
        BlockRef::Fixed(TextKind::Composer),
        BlockRef::Fixed(TextKind::Footer),
    ] {
        assert!(s.score.block_locked(b), "{b:?}");
    }
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(!s.score.block_locked(BlockRef::Fixed(TextKind::Composer)));
    assert!(s.score.block_locked(BlockRef::Fixed(TextKind::Title)));
    s.execute("block.lock", &json!({ "block": "all", "locked": false }))
        .unwrap();
    assert!(s.score.locked_blocks.is_empty() && !s.score.blocks[0].locked);
    // The lock is saved with the score.
    s.execute("block.lock", &json!({ "block": "subtitle" })).ok();
    s.execute("block.lock", &json!({ "block": 0 })).unwrap();
    s.execute("block.lock", &json!({ "block": "composer" })).unwrap();
    let xml = scoreart_musicxml::to_string(&s.score);
    let back = scoreart_musicxml::from_str(&xml).unwrap();
    assert!(back.block_locked(BlockRef::Added(0)));
    assert!(back.block_locked(BlockRef::Fixed(TextKind::Composer)));
    assert!(!back.block_locked(BlockRef::Fixed(TextKind::Title)));
}

/// Filling a page moves systems and staves apart, but never a distance a fixed spacer sets.
#[test]
fn filling_a_page_keeps_fixed_spacers() {
    for justify in [false, true] {
        let mut s = Session::default();
        s.execute(
            "file.new",
            &json!({ "instruments": ["voice", "piano"], "measures": 60, "title": "" }),
        )
        .unwrap();
        s.score.page.vertical_justify = justify;
        // Below the voice in the first system, and below the left hand (to the next system).
        s.execute("select.measure", &json!({ "part": 0, "staff": 0, "measure": 0 }))
            .unwrap();
        s.execute("staff.spacer", &json!({ "type": "fixed", "height": 5 }))
            .unwrap();
        s.execute("select.measure", &json!({ "part": 1, "staff": 1, "measure": 0 }))
            .unwrap();
        s.execute("staff.spacer", &json!({ "type": "fixed", "height": 6 }))
            .unwrap();
        let l = s.layout();
        assert!(l.pages.len() > 1, "a full first page");
        let first = l.staff_box(0, 0, 0).unwrap().clone();
        let rh = l.staff_box(1, 0, 0).unwrap();
        assert!(
            (rh.top - first.top - 9.0).abs() < 0.01,
            "justify {justify}: {}",
            rh.top - first.top
        );
        let lh = l.staff_box(1, 1, 0).unwrap();
        let next = l
            .staves
            .iter()
            .find(|b| b.system == first.system + 1 && b.part == 0)
            .unwrap();
        assert_eq!(next.page, first.page);
        assert!(
            (next.top - lh.top - 10.0).abs() < 0.01,
            "justify {justify}: {}",
            next.top - lh.top
        );
        // The other systems on the page did move apart (the page is filled).
        let later: Vec<f32> = l
            .staves
            .iter()
            .filter(|b| b.page == 0 && b.part == 0 && b.system > first.system + 1)
            .map(|b| b.top)
            .collect();
        assert!(!later.is_empty());
    }
}

/// Graces after a note (a trill's turn) are played at its end, those before at its start.
#[test]
fn graces_after_a_note_play_at_its_end() {
    let mut s = Session::default();
    s.execute(
        "file.new",
        &json!({ "instruments": ["flute"], "measures": 1, "title": "" }),
    )
    .unwrap();
    s.execute("input.toggle", &json!({})).unwrap();
    s.execute("input.duration", &json!({ "value": "whole" })).unwrap();
    s.execute("note.enter", &json!({ "step": "c" })).unwrap();
    s.execute("input.toggle", &json!({})).unwrap();
    let e = s.score.parts[0].measures[0].staves[0].voices[0]
        .events
        .first_mut()
        .unwrap();
    let p = |m: u8| scoreart_model::Pitch::from_midi(i32::from(m), scoreart_model::KeySig::new(0));
    e.grace = vec![
        scoreart_model::Grace {
            pitches: vec![p(74)],
            value: scoreart_model::NoteValue::N16th,
            slash: true,
            after: false,
        },
        scoreart_model::Grace {
            pitches: vec![p(71)],
            value: scoreart_model::NoteValue::N16th,
            slash: false,
            after: true,
        },
    ];
    let seq = scoreart_midi::sequence(&s.score);
    let at = |k: u8| seq.notes.iter().find(|n| n.key == k).map(|n| n.start).unwrap();
    assert!(
        at(74) < u64::from(scoreart_model::TPQ / 4),
        "before: at the start"
    );
    assert!(at(71) >= u64::from(3 * scoreart_model::TPQ), "after: at the end");
}

/// A line break with a little too much before it: the notes move closer instead of a new
/// system starting (other programs' system starts come out alike).
#[test]
fn a_line_break_squeezes_its_system_before_breaking_it() {
    let mut s = Session::default();
    s.execute(
        "file.new",
        &json!({ "instruments": ["piano"], "measures": 24, "title": "" }),
    )
    .unwrap();
    s.execute("input.toggle", &json!({})).unwrap();
    s.execute("input.duration", &json!({ "value": "16th" })).unwrap();
    for _ in 0..16 * 24 {
        s.execute("note.enter", &json!({ "step": "c" })).unwrap();
    }
    s.execute("input.toggle", &json!({})).unwrap();
    let per_system = |s: &mut Session| -> Vec<usize> {
        let l = s.layout();
        let mut n = std::collections::BTreeMap::new();
        for b in l.staves.iter().filter(|b| b.part == 0 && b.staff == 0) {
            *n.entry(b.system).or_insert(0) += 1;
        }
        n.into_values().collect()
    };
    let natural = per_system(&mut s)[0];
    // One measure more than fits, then a break.
    let m = natural;
    s.execute("select.measure", &json!({ "part": 0, "staff": 0, "measure": m }))
        .unwrap();
    s.execute("measure.break", &json!({ "kind": "line" })).unwrap();
    assert_eq!(per_system(&mut s)[0], natural + 1, "squeezed into one system");
    // Far too much: broken as usual.
    s.execute("measure.break", &json!({ "kind": "line" })).unwrap();
    s.execute(
        "select.measure",
        &json!({ "part": 0, "staff": 0, "measure": 2 * natural + 1 }),
    )
    .unwrap();
    s.execute("measure.break", &json!({ "kind": "line" })).unwrap();
    assert_eq!(per_system(&mut s)[0], natural);
}

/// (part, staff, measure) → sorted (onset, MIDI pitch, length) of every note.
fn note_map(s: &Score) -> std::collections::BTreeMap<(usize, usize, usize), Vec<(u32, u8, u32)>> {
    let mut out: std::collections::BTreeMap<_, Vec<_>> = std::collections::BTreeMap::new();
    for (pi, p) in s.parts.iter().enumerate() {
        for (m, pm) in p.measures.iter().enumerate() {
            for (si, st) in pm.staves.iter().enumerate() {
                for v in &st.voices {
                    let mut t = 0;
                    for e in &v.events {
                        if let EventKind::Chord(ns) = &e.kind {
                            for n in ns {
                                out.entry((pi, si, m))
                                    .or_default()
                                    .push((t, n.pitch.midi(), e.ticks()));
                            }
                        }
                        t += e.ticks();
                    }
                }
            }
        }
    }
    for v in out.values_mut() {
        v.sort();
    }
    out
}

/// Exports `s` as PDF, converts the PDF back and checks every note came back.
fn pdf_round_trip(s: &Session) -> Session {
    let pdf = s.save_bytes("x.pdf").unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let mut back = Session::default();
    back.open_bytes("x.pdf", &pdf).unwrap();
    assert_eq!(back.score.parts.len(), s.score.parts.len());
    assert_eq!(back.score.measures.len(), s.score.measures.len());
    assert_eq!(note_map(&back.score), note_map(&s.score));
    let r = back.reference.as_ref().unwrap();
    assert!(r.uncertain.is_empty(), "uncertain {:?}", r.uncertain);
    assert_eq!(back.file_name, "x.musicxml");
    back
}

#[test]
fn pdf_export_and_conversion_of_the_sample() {
    let s = Session::default();
    let back = pdf_round_trip(&s);
    assert_eq!(back.score.meta.title, s.score.meta.title);
    assert_eq!(back.score.meta.composer, s.score.meta.composer);
}

#[test]
fn pdf_conversion_reads_keys_accidentals_and_chords() {
    let xml = r#"<?xml version="1.0"?>
<score-partwise version="4.0"><part-list><score-part id="P1"><part-name>Flute</part-name></score-part></part-list>
<part id="P1">
<measure number="1"><attributes><divisions>2</divisions><key><fifths>-3</fifths></key><time><beats>3</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>
<note><pitch><step>E</step><alter>-1</alter><octave>4</octave></pitch><duration>2</duration><type>quarter</type></note>
<note><pitch><step>A</step><octave>4</octave></pitch><duration>2</duration><type>quarter</type><accidental>natural</accidental></note>
<note><pitch><step>A</step><octave>4</octave></pitch><duration>1</duration><type>eighth</type></note>
<note><pitch><step>B</step><alter>-1</alter><octave>4</octave></pitch><duration>1</duration><type>eighth</type></note>
</measure>
<measure number="2"><attributes><key><fifths>2</fifths></key></attributes>
<note><pitch><step>D</step><octave>5</octave></pitch><duration>3</duration><type>quarter</type><dot/></note>
<note><pitch><step>C</step><alter>1</alter><octave>5</octave></pitch><duration>1</duration><type>eighth</type></note>
<note><rest/><duration>2</duration><type>quarter</type></note>
</measure>
<measure number="3">
<note><pitch><step>D</step><octave>4</octave></pitch><duration>4</duration><type>half</type></note>
<note><chord/><pitch><step>F</step><alter>1</alter><octave>4</octave></pitch><duration>4</duration><type>half</type></note>
<note><chord/><pitch><step>A</step><octave>4</octave></pitch><duration>4</duration><type>half</type></note>
<note><pitch><step>C</step><octave>5</octave></pitch><duration>2</duration><type>quarter</type><accidental>natural</accidental></note>
<barline location="right"><bar-style>light-heavy</bar-style></barline>
</measure>
</part></score-partwise>"#;
    let mut s = Session::default();
    s.open_bytes("k.musicxml", xml.as_bytes()).unwrap();
    let back = pdf_round_trip(&s);
    assert_eq!(back.score.measures[0].key.map(|k| k.fifths), Some(-3));
    assert_eq!(back.score.measures[1].key.map(|k| k.fifths), Some(2));
}

#[test]
fn a_pdf_can_be_shown_without_converting() {
    let mut s = Session::default();
    let pdf = s.save_bytes("x.pdf").unwrap();
    let before = note_map(&s.score);
    s.open_pdf("Original.pdf", &pdf, false).unwrap();
    assert_eq!(note_map(&s.score), before);
    let r = s.reference.as_ref().unwrap();
    assert_eq!(r.name, "Original.pdf");
    assert!(r.doc.page_count() >= 1);
    let pic = r.doc.render(0, 300).unwrap();
    assert_eq!(pic.width, 300);
    assert_eq!(pic.rgba.len(), (pic.width * pic.height * 4) as usize);
    // Ink on the page: not every pixel is white.
    assert!(pic.rgba.chunks(4).any(|p| p[0] < 128));
}

#[test]
fn broken_pdfs_fail_cleanly() {
    let mut s = Session::default();
    let title = s.score.meta.title.clone();
    for bytes in [
        &b"%PDF-1.7\n garbage"[..],
        b"%PDF",
        b"%PDF-1.4\n1 0 obj << /Type /Catalog >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF",
    ] {
        assert!(s.open_bytes("bad.pdf", bytes).is_err());
        assert!(s.open_pdf("bad.pdf", bytes, false).is_err());
    }
    assert_eq!(s.score.meta.title, title);
    assert!(s.reference.is_none());
}

/// A lead sheet line: a melody with chord symbols.
fn lead_sheet() -> Session {
    let xml = r#"<?xml version="1.0"?>
<score-partwise version="4.0"><part-list><score-part id="P1"><part-name>Voice</part-name></score-part></part-list>
<part id="P1">
<measure number="1"><attributes><divisions>1</divisions><key><fifths>0</fifths></key><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>
<harmony><root><root-step>C</root-step></root><kind>major</kind></harmony>
<note><pitch><step>E</step><octave>5</octave></pitch><duration>2</duration><type>half</type></note>
<harmony><root><root-step>A</root-step></root><kind>minor</kind></harmony>
<note><pitch><step>C</step><octave>5</octave></pitch><duration>2</duration><type>half</type></note>
</measure>
<measure number="2">
<note><pitch><step>D</step><octave>5</octave></pitch><duration>2</duration><type>half</type></note>
<harmony><root><root-step>G</root-step></root><kind>dominant</kind><bass><bass-step>B</bass-step></bass></harmony>
<note><pitch><step>B</step><octave>4</octave></pitch><duration>2</duration><type>half</type></note>
</measure>
</part></score-partwise>"#;
    let mut s = Session::default();
    s.open_bytes("lead.musicxml", xml.as_bytes()).unwrap();
    s
}

#[test]
fn chord_symbols_sound_until_the_next_one() {
    let mut s = lead_sheet();
    let seq = scoreart_midi::sequence(&s.score);
    let chords: Vec<&scoreart_midi::PlayNote> = seq.notes.iter().filter(|n| n.tech.harmony).collect();
    let q = u64::from(TPQ);
    // C: bass C3, then C E G; Am from beat 3 to the G7/B in measure 2, beat 3.
    let at = |t: u64| -> Vec<u8> {
        let mut k: Vec<u8> = chords.iter().filter(|n| n.start == t).map(|n| n.key).collect();
        k.sort_unstable();
        k
    };
    assert_eq!(at(0), vec![48, 60, 64, 67]);
    assert_eq!(at(2 * q), vec![45, 57, 60, 64]);
    assert_eq!(at(6 * q), vec![47, 55, 59, 62, 65]);
    assert!(
        chords
            .iter()
            .filter(|n| n.start == 2 * q)
            .all(|n| n.ticks == 4 * q)
    );
    assert!(
        chords
            .iter()
            .filter(|n| n.start == 6 * q)
            .all(|n| n.ticks == 2 * q)
    );
    // The melody plays as before.
    assert_eq!(seq.notes.iter().filter(|n| !n.tech.harmony).count(), 4);
    // Their own instrument; off when switched off.
    s.execute("part.harmony", &json!({ "part": 0, "instrument": "guitar" }))
        .unwrap();
    let guitar = scoreart_model::INSTRUMENTS
        .iter()
        .find(|i| i.id == "guitar")
        .unwrap()
        .program;
    let seq = scoreart_midi::sequence(&s.score);
    assert!(
        seq.notes
            .iter()
            .filter(|n| n.tech.harmony)
            .all(|n| n.program == guitar)
    );
    // A MIDI file gets a track of its own for them.
    let smf = scoreart_midi::to_smf(&s.score);
    assert!(smf.starts_with(b"MThd"));
    s.execute("part.harmony", &json!({ "part": 0, "play": false }))
        .unwrap();
    assert!(
        !scoreart_midi::sequence(&s.score)
            .notes
            .iter()
            .any(|n| n.tech.harmony)
    );
    // Saved with the score.
    let back = from_saved(&s);
    assert_eq!(back.score.parts[0].harmony, s.score.parts[0].harmony);
}

fn from_saved(s: &Session) -> Session {
    let bytes = s.save_bytes("x.musicxml").unwrap();
    let mut back = Session::default();
    back.open_bytes("x.musicxml", &bytes).unwrap();
    back
}

#[test]
fn layout_settings_from_musescore_survive_musicxml() {
    let mut s = lead_sheet();
    s.execute("part.hide_empty", &json!({ "part": 0, "hide": true }))
        .unwrap();
    s.score.measures[0].frame_after = Some(30.0);
    s.score.measures[0].marker_place = Some(scoreart_model::Placement::from_sp(8.4, -4.0, false));
    s.score.measures[0].jumps.push(scoreart_model::Jump::Segno);
    s.score.page.german_chords = true;
    s.score.page.first_names = 2;
    let back = from_saved(&s);
    assert_eq!(back.score.parts[0].hide_empty, vec![true]);
    assert_eq!(back.score.measures[0].frame_after, Some(30.0));
    assert_eq!(
        back.score.measures[0].marker_place,
        s.score.measures[0].marker_place
    );
    assert!(back.score.page.german_chords);
    assert_eq!(back.score.page.first_names, 2);
}

#[test]
fn measure_marks_move_where_they_are_dropped() {
    let mut s = lead_sheet();
    s.execute("select.measure", &json!({ "part": 0, "staff": 0, "measure": 1 }))
        .unwrap();
    s.execute("measure.rehearsal", &json!({ "text": "Vers" }))
        .unwrap();
    let mark = |s: &mut Session| {
        let l = s.layout();
        l.hits.iter().find_map(|h| match h.target {
            scoreart_engrave::Target::MeasureMark {
                kind: scoreart_engrave::MeasureMark::Rehearsal,
                ax,
                ay,
                x,
                base,
                ..
            } => Some((ax, ay, x, base)),
            _ => None,
        })
    };
    let (ax, ay, x, base) = mark(&mut s).unwrap();
    // Dropped 2 spaces right and 1 up: it stands exactly there.
    s.execute(
        "measure.place",
        &json!({ "kind": "rehearsal", "measure": 1, "dx": x + 2.0 - ax, "dy": base - 1.0 - ay }),
    )
    .unwrap();
    let (_, _, x2, base2) = mark(&mut s).unwrap();
    assert!(
        (x2 - (x + 2.0)).abs() < 0.06 && (base2 - (base - 1.0)).abs() < 0.06,
        "{x}→{x2}, {base}→{base2}"
    );
    // Back to its default place.
    s.execute(
        "measure.place",
        &json!({ "kind": "rehearsal", "measure": 1, "reset": true }),
    )
    .unwrap();
    assert!(s.score.measures[1].rehearsal_place.is_none());
    assert!(
        s.execute(
            "measure.place",
            &json!({ "kind": "jump", "measure": 1, "dx": 1, "dy": -3 })
        )
        .is_err()
    );
}

#[test]
fn frames_and_name_settings_by_command() {
    let mut s = lead_sheet();
    s.execute("measure.frame", &json!({ "measure": 0, "height": 25 }))
        .unwrap();
    assert_eq!(s.score.measures[0].frame_after, Some(25.0));
    assert!(s.score.measures[0].system_break);
    s.execute("measure.frame", &json!({ "measure": 0, "height": 0 }))
        .unwrap();
    assert_eq!(s.score.measures[0].frame_after, None);
    s.execute(
        "page.format",
        &json!({ "first_names": "none", "later_names": "short", "single_name": false, "german_chords": true }),
    )
    .unwrap();
    let f = s.score.page;
    assert_eq!(
        (f.first_names, f.later_names, f.single_name, f.german_chords),
        (2, 1, false, true)
    );
    assert!(
        s.execute("page.format", &json!({ "first_names": "loud" }))
            .is_err()
    );
}

#[test]
fn worksheet_text_fields() {
    use scoreart_engrave::{Shape, Target};
    use scoreart_model::BlockRef;
    let mut s = Session::default();
    let hit = |s: &mut Session, i: usize| {
        s.layout()
            .hits
            .iter()
            .find(|h| {
                h.target
                    == Target::Block {
                        block: BlockRef::Added(i),
                    }
            })
            .copied()
    };
    // A long task text above the second system, framed, with its own font.
    let task = "Aufgabe 1: Bestimme die Intervalle zwischen den Noten und schreibe ihre Namen \
                darunter. Achte auf die Vorzeichen der Tonart und benenne jede Terz und Quinte."
        .repeat(3);
    s.execute(
        "block.add",
        &json!({ "kind": "text", "text": task, "measure": 4, "border": true, "font": "sans", "size": 12, "bold": true }),
    )
    .unwrap();
    let b = &s.score.blocks[0];
    assert!(b.look.border && b.look.style.is_some_and(|st| st.bold && st.size == 12.0));
    // Name and date side by side, and an empty answer field with lines to write on.
    s.execute(
        "block.add",
        &json!({ "kind": "text", "text": "Name:", "measure": 4, "width": 60, "lines": 1 }),
    )
    .unwrap();
    s.execute(
        "block.add",
        &json!({ "kind": "text", "text": "Datum:", "measure": 4, "width": 35, "lines": 1, "beside": true }),
    )
    .unwrap();
    s.execute(
        "block.add",
        &json!({ "kind": "text", "measure": 4, "lines": 4, "shade": true }),
    )
    .unwrap();
    assert!(
        s.execute("block.add", &json!({ "kind": "text", "measure": 4 }))
            .is_err()
    );
    let l = s.layout();
    let (Some(a), Some(name), Some(date), Some(answer)) =
        (hit(&mut s, 0), hit(&mut s, 1), hit(&mut s, 2), hit(&mut s, 3))
    else {
        panic!("every field can be picked");
    };
    // Side by side: same top, the date right of the name.
    assert!((name.y0 - date.y0).abs() < 0.01 && date.x0 > name.x1);
    // One below the other, nothing overlapping: task, name row, answer, then the music.
    assert!(a.y1 <= name.y0 && name.y1 <= answer.y0);
    let sys = l.staves.iter().find(|b| b.measure == 4).unwrap();
    let above = l
        .staves
        .iter()
        .filter(|b| b.page == sys.page && b.top < sys.top)
        .map(|b| b.top + 4.0)
        .fold(0.0f32, f32::max);
    assert!(a.y0 >= above, "the task stays below the system before");
    assert!(answer.y1 <= sys.top, "the answer field ends above its system");
    assert!(
        l.pages[sys.page]
            .items
            .iter()
            .any(|it| matches!(it.shape, Shape::Shade { .. }))
    );
    // Moving the answer field down moves the music below it too.
    let top_before = sys.top;
    s.execute("block.move", &json!({ "block": 3, "dx": 0, "dy": 6 }))
        .unwrap();
    let l2 = s.layout();
    let sys2 = l2.staves.iter().find(|b| b.measure == 4).unwrap();
    let moved = hit(&mut s, 3).unwrap();
    assert!(sys2.page != sys.page || sys2.top >= top_before + 5.9);
    assert!(sys2.page != moved.page || moved.y1 <= sys2.top);
    // Moving it far up stops below the system before.
    s.execute("block.move", &json!({ "block": 0, "dx": 0, "dy": -40 }))
        .unwrap();
    assert!(hit(&mut s, 0).unwrap().y0 >= above);
    // The look: read back, back to the style of its kind, bad values refused.
    let r = s
        .execute(
            "block.look",
            &json!({ "block": 0, "style": "kind", "underline": true }),
        )
        .unwrap();
    assert_eq!(r["style"], serde_json::Value::Null);
    assert_eq!(r["underline"], true);
    assert!(
        s.execute("block.look", &json!({ "block": 0, "align": "up" }))
            .is_err()
    );
    assert!(
        s.execute("block.look", &json!({ "block": "title", "border": true }))
            .is_err()
    );
    let r = s
        .execute("block.look", &json!({ "block": 0, "lines": 500, "width": 1 }))
        .unwrap();
    assert_eq!((r["lines"].as_u64(), r["width"].as_u64()), (Some(30), Some(10)));
    // Saved and read back with the score.
    let back = scoreart_musicxml::from_str(&scoreart_musicxml::to_string(&s.score)).unwrap();
    assert_eq!(back.blocks, s.score.blocks);
}

#[test]
fn chord_symbols_move_by_hand() {
    use scoreart_engrave::Target;
    let mut s = Session::default();
    s.execute(
        "select.event",
        &json!({ "part": 0, "staff": 0, "voice": 0, "measure": 1, "index": 0 }),
    )
    .unwrap();
    s.execute("note.chord-symbol", &json!({ "text": "Am7" })).unwrap();
    let ev = |s: &Session| match s.selection {
        Some(Selection::Event { ev, .. }) => ev,
        _ => panic!("a note is selected"),
    };
    let e = ev(&s);
    let chord_hit = |s: &mut Session| {
        s.layout()
            .hits
            .iter()
            .find(|h| h.target == Target::Chord { ev: e })
            .copied()
    };
    // Measured from the top line of its staff (moved up, the system makes room for it).
    let top = |s: &mut Session| s.layout().staff_box(e.part, e.staff, e.measure).unwrap().top;
    let before = chord_hit(&mut s).expect("the chord symbol can be picked up");
    let top0 = top(&mut s);
    s.execute("note.place", &json!({ "kind": "chord", "dx": 1.5, "dy": -2 }))
        .unwrap();
    assert_eq!(
        s.score.event(e).unwrap().chord_place.map(|p| (p.dx, p.dy)),
        Some((15, -20))
    );
    let after = chord_hit(&mut s).unwrap();
    let top1 = top(&mut s);
    assert!((after.x0 - before.x0 - 1.5).abs() < 0.01);
    assert!(((after.y0 - top1) - (before.y0 - top0) + 2.0).abs() < 0.01);
    // Saved and read back.
    let back = scoreart_musicxml::from_str(&scoreart_musicxml::to_string(&s.score)).unwrap();
    assert_eq!(
        back.event(e).unwrap().chord_place,
        s.score.event(e).unwrap().chord_place
    );
    // Back to its own place.
    s.execute("note.place", &json!({ "kind": "chord", "reset": true }))
        .unwrap();
    assert_eq!(s.score.event(e).unwrap().chord_place, None);
}
