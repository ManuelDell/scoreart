use scoreart_model::edit::Pos;
use scoreart_model::*;

use crate::*;

fn melody() -> Score {
    let piano = instrument("piano").unwrap();
    let mut s = Score::new("Melody", &[piano], TimeSig::default(), KeySig::new(1), 8);
    s.meta.composer = "ScoreArt".into();
    let mut p = Pos::default();
    let steps = [
        Step::G,
        Step::A,
        Step::B,
        Step::C,
        Step::D,
        Step::E,
        Step::F,
        Step::G,
    ];
    for (i, st) in steps.iter().cycle().take(40).enumerate() {
        let oct = if matches!(st, Step::G | Step::A | Step::B) {
            4
        } else {
            5
        };
        let alter = if *st == Step::F && i % 3 == 0 {
            0
        } else {
            KeySig::new(1).alter_for(*st)
        };
        let v = if i % 4 < 2 {
            NoteValue::Eighth
        } else {
            NoteValue::Quarter
        };
        let kind = EventKind::Chord(vec![Note::new(Pitch::new(*st, alter, oct))]);
        p = s.overwrite(p, kind, Duration::new(v, 0)).unwrap();
    }
    s
}

#[test]
fn lays_out_pages_and_boxes() {
    let s = melody();
    let l = layout(&s, &Style::default());
    assert!(!l.pages.is_empty());
    let p = &l.pages[0];
    assert!(p.items.len() > 100);
    // Every staff of every measure has a box.
    for m in 0..s.measures.len() {
        for st in 0..2 {
            let b = l.staff_box(0, st, m).unwrap();
            assert!(b.x1 > b.x0);
            assert_eq!(b.columns.last().unwrap().0, s.measure_ticks(m));
        }
    }
    // Shapes stay on the page.
    for it in &p.items {
        if let Shape::Glyph { x, y, .. } = it.shape {
            assert!(x > 0.0 && x < p.width && y > 0.0 && y < p.height, "{x},{y}");
        }
    }
    // Beams exist for the eighth pairs.
    assert!(p.items.iter().any(|i| matches!(i.shape, Shape::Quad { .. })));
    // Clicking a notehead finds it.
    let ev = scoreart_model::edit::EventRef {
        part: 0,
        staff: 0,
        voice: 0,
        measure: 0,
        index: 0,
    };
    let hb = l.event_box(ev).unwrap();
    let t = l
        .hit(hb.page, (hb.x0 + hb.x1) / 2.0, (hb.y0 + hb.y1) / 2.0)
        .unwrap();
    assert!(matches!(t, Target::Note { ev: e, .. } if e == ev));
}

#[test]
fn accidental_shown_once_per_measure() {
    let flute = instrument("flute").unwrap();
    let mut s = Score::new("A", &[flute], TimeSig::default(), KeySig::default(), 1);
    let fs = EventKind::Chord(vec![Note::new(Pitch::new(Step::F, 1, 5))]);
    let q = Duration::new(NoteValue::Quarter, 0);
    let mut p = Pos::default();
    p = s.overwrite(p, fs.clone(), q).unwrap();
    s.overwrite(p, fs, q).unwrap();
    let l = layout(&s, &Style::default());
    let sharps = l.pages[0]
        .items
        .iter()
        .filter(|i| matches!(i.shape, Shape::Glyph { c, .. } if c == glyphs::SHARP))
        .count();
    assert_eq!(sharps, 1);
}

#[test]
fn many_measures_make_many_pages() {
    let piano = instrument("piano").unwrap();
    let violin = instrument("violin").unwrap();
    let s = Score::new("Long", &[violin, piano], TimeSig::default(), KeySig::new(-3), 400);
    let l = layout(&s, &Style::default());
    assert!(l.pages.len() > 3);
    assert_eq!(l.staves.len(), 400 * 3);
}

#[test]
fn hostile_style_does_not_panic() {
    let s = melody();
    for st in [
        Style {
            staff_space_mm: 0.0,
            ..Style::default()
        },
        Style {
            page_width_mm: -5.0,
            page_height_mm: f32::NAN,
            ..Style::default()
        },
        Style {
            margin_left_mm: 1e9,
            margin_top_mm: f32::INFINITY,
            spacing: 0.0,
            ..Style::default()
        },
    ] {
        let _ = layout(&s, &st);
    }
    let _ = layout(&Score::default(), &Style::default());
}

/// Writes the test melody to `target/engrave-test.svg`-style files via the render crate in
/// `crates/render` (see its example); here we only check determinism.
#[test]
fn layout_is_deterministic() {
    let s = melody();
    assert_eq!(layout(&s, &Style::default()), layout(&s, &Style::default()));
}

#[test]
fn hidden_staves_and_breaks() {
    let mut s = Score::new(
        "T",
        &[instrument("flute").unwrap(), instrument("piano").unwrap()],
        TimeSig::default(),
        KeySig::default(),
        12,
    );
    s.parts[1].set_visible(false);
    s.measures[1].system_break = true;
    s.measures[3].page_break = true;
    let l = layout(&s, &Style::default());
    assert!(
        l.staves.iter().all(|b| b.part == 0),
        "the hidden piano is not drawn"
    );
    let first_system: Vec<usize> = l
        .staves
        .iter()
        .filter(|b| b.system == 0)
        .map(|b| b.measure)
        .collect();
    assert_eq!(first_system, [0, 1], "a line break after measure 2");
    let m4 = l.staves.iter().find(|b| b.measure == 4).unwrap();
    assert_eq!(m4.page, 1, "a page break after measure 4");
    // Every staff hidden: everything is shown rather than an empty page.
    s.parts[0].set_visible(false);
    let l = layout(&s, &Style::default());
    assert!(l.staves.iter().any(|b| b.part == 1));
}

#[test]
fn continuous_views_have_one_endless_page() {
    let s = melody();
    let page = layout(&s, &Style::default());
    let wide = layout(&s, &Style::new(&s.page, crate::View::Horizontal));
    assert_eq!(wide.pages.len(), 1);
    assert!(
        wide.pages[0].width > page.pages[0].width,
        "the page grows to the right"
    );
    assert!(wide.pages[0].height < page.pages[0].height);
    let tall = layout(&s, &Style::new(&s.page, crate::View::Vertical));
    assert_eq!(tall.pages.len(), 1);
    assert_eq!(tall.pages[0].width, page.pages[0].width);
    // Landscape A4 is wider than portrait.
    let mut fmt = s.page;
    (fmt.width_mm, fmt.height_mm) = (fmt.height_mm, fmt.width_mm);
    let land = layout(&s, &Style::new(&fmt, crate::View::Page));
    assert!(land.pages[0].width > page.pages[0].width);
}
