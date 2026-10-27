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
    });
    s.score.spanners.push(scoreart_model::Spanner {
        kind: scoreart_model::SpannerKind::Slur,
        part: 1,
        staff: 0,
        voice: 0,
        start: scoreart_model::Anchor { measure: 0, tick: 0 },
        end: scoreart_model::Anchor { measure: 1, tick: 0 },
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
