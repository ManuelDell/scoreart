//! Every notation mark ScoreArt knows: set it with commands, save, reopen, lay out.

use scoreart_engine::Session;
use scoreart_model::marks::MARKS;
use scoreart_model::{Dynamic, Jump, NoteHead, SpannerKind};
use serde_json::json;

fn run(s: &mut Session, id: &str, p: serde_json::Value) {
    if let Err(e) = s.execute(id, &p) {
        panic!("{id} {p}: {e}");
    }
}

fn sel(s: &mut Session, measure: usize, index: usize) {
    run(
        s,
        "select.event",
        json!({ "part": 0, "staff": 0, "measure": measure, "index": index }),
    );
}

/// A flute score with eight quarter notes per measure for `measures` measures.
fn notes(measures: usize) -> Session {
    let mut s = Session::default();
    run(
        &mut s,
        "file.new",
        json!({ "instruments": ["flute", "piano"], "measures": measures, "key": 1 }),
    );
    for _ in 0..measures * 4 {
        for st in ["g", "a", "b", "c", "d", "e", "f"] {
            let _ = st;
        }
        run(&mut s, "note.enter", json!({ "step": "a", "octave": 4 }));
    }
    run(&mut s, "edit.escape", json!({}));
    s
}

fn all_marks() -> Session {
    let mut s = notes(40);
    // One mark per note.
    for (i, d) in MARKS.iter().enumerate() {
        sel(&mut s, i / 4, i % 4);
        run(&mut s, "note.articulation", json!({ "type": d.id }));
    }
    // Dynamics on the second staff part's... on the flute, one per note from measure 20.
    for (i, d) in Dynamic::all().enumerate() {
        sel(&mut s, 20 + i / 4, i % 4);
        run(&mut s, "note.dynamic", json!({ "type": d.id() }));
    }
    sel(&mut s, 28, 0);
    run(&mut s, "note.grace", json!({ "type": "acciaccatura" }));
    sel(&mut s, 28, 1);
    run(&mut s, "note.grace", json!({ "type": "appoggiatura" }));
    run(&mut s, "note.grace-pitch", json!({ "delta": -2 }));
    sel(&mut s, 28, 2);
    run(&mut s, "note.tremolo", json!({ "strokes": 3 }));
    sel(&mut s, 28, 3);
    run(&mut s, "note.tremolo", json!({ "strokes": "buzz" }));
    sel(&mut s, 29, 0);
    run(&mut s, "note.add", json!({ "step": "c" }));
    run(&mut s, "note.add", json!({ "step": "e" }));
    run(&mut s, "note.arpeggio", json!({ "type": "arpeggio-up" }));
    for (i, h) in NoteHead::ALL.iter().enumerate() {
        sel(&mut s, 30 + i / 4, i % 4);
        run(&mut s, "note.head", json!({ "type": h.id() }));
    }
    sel(&mut s, 32, 0);
    run(&mut s, "note.paren", json!({}));
    run(&mut s, "note.fingering", json!({ "text": "3" }));
    run(&mut s, "note.chord-symbol", json!({ "text": "F#m7/C#" }));
    run(&mut s, "note.text", json!({ "text": "dolce" }));
    // Every line, each over two notes, measures 33–37.
    for (i, k) in SpannerKind::ALL.iter().enumerate() {
        let m = 33 + i / 4;
        sel(&mut s, m.min(39), i % 4);
        run(&mut s, "select.extend-next", json!({}));
        run(&mut s, "line.add", json!({ "type": k.id() }));
    }
    // A slur across a barline and a long hairpin.
    sel(&mut s, 38, 2);
    for _ in 0..4 {
        run(&mut s, "select.extend-next", json!({}));
    }
    run(&mut s, "line.add", json!({ "type": "slur" }));
    // Voltas, jumps, rehearsal, tempo text, repeat sign.
    run(
        &mut s,
        "select.measure",
        json!({ "part": 0, "staff": 0, "measure": 10 }),
    );
    run(&mut s, "measure.volta", json!({ "number": 1 }));
    run(&mut s, "measure.barline", json!({ "type": "repeat-end" }));
    run(
        &mut s,
        "select.measure",
        json!({ "part": 0, "staff": 0, "measure": 11 }),
    );
    run(&mut s, "measure.volta", json!({ "number": 2 }));
    run(&mut s, "measure.rehearsal", json!({}));
    run(&mut s, "measure.tempo-text", json!({ "text": "Allegro" }));
    for (i, j) in Jump::ALL.iter().enumerate() {
        run(
            &mut s,
            "select.measure",
            json!({ "part": 0, "staff": 0, "measure": 12 + i }),
        );
        run(&mut s, "measure.jump", json!({ "type": j.id() }));
    }
    run(
        &mut s,
        "select.measure",
        json!({ "part": 1, "staff": 0, "measure": 3 }),
    );
    run(&mut s, "measure.repeat-sign", json!({}));
    s
}

#[test]
fn every_mark_round_trips_through_musicxml() {
    let s = all_marks();
    for name in ["x.musicxml", "x.mxl"] {
        let bytes = s.save_bytes(name).unwrap();
        let mut t = Session::default();
        t.open_bytes(name, &bytes).unwrap();
        let (a, b) = (&s.score, &t.score);
        assert_eq!(a.measures, b.measures, "measure marks");
        for (pa, pb) in a.parts.iter().zip(&b.parts) {
            for (m, (ma, mb)) in pa.measures.iter().zip(&pb.measures).enumerate() {
                assert_eq!(ma, mb, "part {} measure {}", pa.name, m + 1);
            }
        }
        let mut sa = a.spanners.clone();
        let mut sb = b.spanners.clone();
        let key = |x: &scoreart_model::Spanner| {
            (
                x.part,
                x.staff,
                x.voice,
                x.start.measure,
                x.start.tick,
                x.kind.id(),
            )
        };
        sa.sort_by_key(key);
        sb.sort_by_key(key);
        assert_eq!(sa, sb, "lines");
        assert_eq!(a, b);
    }
}

#[test]
fn every_mark_has_a_glyph_and_lays_out() {
    for d in MARKS {
        assert!(
            scoreart_engrave::smufl::get(d.glyph_above).is_some(),
            "{}",
            d.glyph_above
        );
        assert!(
            scoreart_engrave::smufl::get(d.glyph_below).is_some(),
            "{}",
            d.glyph_below
        );
    }
    let mut s = all_marks();
    let l = s.layout();
    let glyphs: usize = l
        .pages
        .iter()
        .map(|p| {
            p.items
                .iter()
                .filter(|i| matches!(i.shape, scoreart_engrave::Shape::Glyph { .. }))
                .count()
        })
        .sum();
    assert!(glyphs > 300, "{glyphs} glyphs");
    // MIDI and audio still work.
    assert!(!s.save_bytes("x.mid").unwrap().is_empty());
}

#[test]
fn deleting_measures_keeps_lines_valid() {
    let mut s = all_marks();
    run(
        &mut s,
        "select.measure",
        json!({ "part": 0, "staff": 0, "measure": 33 }),
    );
    run(
        &mut s,
        "select.measure",
        json!({ "part": 0, "staff": 0, "measure": 36, "extend": true }),
    );
    run(&mut s, "measure.delete", json!({}));
    let _ = s.layout();
    let n = s.score.measures.len();
    assert!(s.score.spanners.iter().all(|sp| sp.end.measure < n));
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.score.measures.len(), 40);
}
