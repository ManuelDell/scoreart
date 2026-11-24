//! PDF export: every page of a layout, with Bravura and Crimson Text embedded as real fonts
//! (subset by krilla), so the music stays text — searchable, sharp at any size, and readable
//! again by ScoreArt's PDF import.

use krilla::Document;
use krilla::geom::{PathBuilder, Point, Size, Transform};
use krilla::metadata::Metadata;
use krilla::page::PageSettings;
use krilla::paint::{Fill, LineCap, Stroke};
use krilla::text::{Font, TextDirection};

use scoreart_engrave::{Align, Layout, Shape, Style};

use crate::{BRAVURA, SERIF, crescent};

/// Writes the layout as a PDF document; `title` goes into the document's metadata.
pub fn pdf(layout: &Layout, style: &Style, title: &str) -> Result<Vec<u8>, String> {
    let mm = if style.staff_space_mm.is_finite() {
        style.staff_space_mm.clamp(0.5, 10.0)
    } else {
        1.75
    };
    // Points per staff space: the page is drawn in staff spaces, as the engraver lays it out.
    let k = mm * 72.0 / 25.4;
    let music = Font::new(BRAVURA.into(), 0).ok_or("the music font cannot be read")?;
    let text = Font::new(SERIF.into(), 0).ok_or("the text font cannot be read")?;
    let mut doc = Document::new();
    doc.set_metadata(
        Metadata::new()
            .title(title.to_string())
            .creator("ScoreArt".to_string()),
    );
    for page in &layout.pages {
        let (w, h) = ((page.width * k).max(1.0), (page.height * k).max(1.0));
        let Some(size) = Size::from_wh(w, h) else {
            continue;
        };
        let mut pg = doc.start_page_with(PageSettings::new(size));
        let mut s = pg.surface();
        s.push_transform(&Transform::from_scale(k, k));
        s.set_fill(Some(Fill::default()));
        for it in &page.items {
            match &it.shape {
                Shape::Glyph { c, x, y, scale } => {
                    let em = 4.0 * scale;
                    if em.is_finite() && em > 0.0 {
                        s.draw_text(
                            Point::from_xy(*x, *y),
                            music.clone(),
                            em,
                            &c.to_string(),
                            false,
                            TextDirection::Auto,
                        );
                    }
                }
                Shape::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    width,
                } => {
                    let mut b = PathBuilder::new();
                    b.move_to(*x1, *y1);
                    b.line_to(*x2, *y2);
                    if let Some(path) = b.finish() {
                        s.set_fill(None);
                        s.set_stroke(Some(Stroke {
                            width: width.max(0.01),
                            line_cap: LineCap::Butt,
                            ..Stroke::default()
                        }));
                        s.draw_path(&path);
                        s.set_stroke(None);
                        s.set_fill(Some(Fill::default()));
                    }
                }
                Shape::Rect { x, y, w, h } => {
                    if let Some(r) = krilla::geom::Rect::from_xywh(*x, *y, w.max(0.001), h.max(0.001)) {
                        let mut b = PathBuilder::new();
                        b.push_rect(r);
                        if let Some(path) = b.finish() {
                            s.draw_path(&path);
                        }
                    }
                }
                Shape::Quad { p } => {
                    let mut b = PathBuilder::new();
                    b.move_to(p[0].0, p[0].1);
                    for q in &p[1..] {
                        b.line_to(q.0, q.1);
                    }
                    b.close();
                    if let Some(path) = b.finish() {
                        s.draw_path(&path);
                    }
                }
                Shape::Curve { p, thick } => {
                    let c = crescent(*p, *thick);
                    let mut b = PathBuilder::new();
                    b.move_to(c[4].0, c[4].1);
                    b.cubic_to(c[0].0, c[0].1, c[1].0, c[1].1, c[5].0, c[5].1);
                    b.cubic_to(c[2].0, c[2].1, c[3].0, c[3].1, c[4].0, c[4].1);
                    b.close();
                    if let Some(path) = b.finish() {
                        s.draw_path(&path);
                    }
                }
                Shape::Text {
                    text: t,
                    x,
                    y,
                    size,
                    align,
                    bold,
                    italic,
                    ..
                } => {
                    if t.trim().is_empty() || !(size.is_finite() && *size > 0.0) {
                        continue;
                    }
                    let w = scoreart_engrave::text_width(t, *size);
                    let x0 = match align {
                        Align::Left => *x,
                        Align::Center => x - w / 2.0,
                        Align::Right => x - w,
                    };
                    // One text font is bundled: italic is slanted, bold drawn with an outline.
                    let slant = if *italic {
                        // x' = x + 0.2·(baseline − y): leaning right, upwards (y points down).
                        Transform::from_row(1.0, 0.0, -0.2, 1.0, 0.2 * *y, 0.0)
                    } else {
                        Transform::default()
                    };
                    s.push_transform(&slant);
                    if *bold {
                        s.set_stroke(Some(Stroke {
                            width: size * 0.035,
                            ..Stroke::default()
                        }));
                    }
                    s.draw_text(
                        Point::from_xy(x0, *y),
                        text.clone(),
                        *size,
                        t,
                        false,
                        TextDirection::Auto,
                    );
                    s.set_stroke(None);
                    s.pop();
                }
            }
        }
        s.pop();
        s.finish();
        pg.finish();
    }
    doc.finish().map_err(|e| format!("{e:?}"))
}
