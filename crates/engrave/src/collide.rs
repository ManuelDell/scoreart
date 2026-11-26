//! Where things overlap on the laid-out pages: texts (lyrics, chord symbols, staff and system
//! texts, titles), rehearsal marks, dynamics, noteheads and rests. The editor shows them so
//! layout problems do not go unnoticed; they are never printed.

use crate::{Align, Collision, Layout, MeasureMark, Shape, Target};
use scoreart_model::TextKind;

/// What a box is, for deciding which pairs count.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    /// A rehearsal mark's frame.
    Frame,
    Dynamic,
    Note,
}

/// Boxes shrink by this much before they are compared (touching is no overlap), staff spaces.
const SLACK: f32 = 0.08;
/// Overlaps smaller than this (in square staff spaces) are ignored.
const LEAST_AREA: f32 = 0.06;
/// Most collisions reported (a page full of them says enough).
const MAX: usize = 2000;

fn text_box(text: &str, x: f32, y: f32, size: f32, align: Align) -> Option<[f32; 4]> {
    if text.trim().is_empty() || !(size.is_finite() && size > 0.0) {
        return None;
    }
    let w = crate::text_width(text.trim_end(), size);
    let x0 = match align {
        Align::Left => x,
        Align::Center => x - w / 2.0,
        Align::Right => x - w,
    };
    Some([x0, y - 0.7 * size, x0 + w, y + 0.2 * size])
}

fn overlap(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let w = (a[2].min(b[2]) - a[0].max(b[0]) - 2.0 * SLACK).max(0.0);
    let h = (a[3].min(b[3]) - a[1].max(b[1]) - 2.0 * SLACK).max(0.0);
    w * h
}

/// `inner` lies within `outer` (a rehearsal mark's text in its frame).
fn inside(inner: &[f32; 4], outer: &[f32; 4]) -> bool {
    inner[0] >= outer[0] - 0.3
        && inner[2] <= outer[2] + 0.3
        && inner[1] >= outer[1] - 0.6
        && inner[3] <= outer[3] + 0.6
}

/// Every overlap that matters: a text with anything else (another text, a frame, a dynamic, a
/// note or rest), and frames or dynamics with notes. Notes among themselves (chords, seconds)
/// are left alone.
pub(crate) fn collisions(l: &Layout) -> Vec<Collision> {
    let mut out = Vec::new();
    for (pi, page) in l.pages.iter().enumerate() {
        let mut boxes: Vec<([f32; 4], Kind)> = page
            .items
            .iter()
            .filter_map(|it| match &it.shape {
                Shape::Text {
                    text,
                    x,
                    y,
                    size,
                    align,
                    ..
                } => text_box(text, *x, *y, *size, *align).map(|b| (b, Kind::Text)),
                _ => None,
            })
            .collect();
        boxes.extend(beams(page).into_iter().map(|b| (b, Kind::Note)));
        for h in l.hits.iter().filter(|h| h.page == pi) {
            let kind = match h.target {
                Target::Note { .. } | Target::Rest { .. } => Kind::Note,
                Target::Dynamic { .. } => Kind::Dynamic,
                Target::MeasureMark {
                    kind: MeasureMark::Rehearsal,
                    ..
                } => Kind::Frame,
                _ => continue,
            };
            boxes.push(([h.x0, h.y0, h.x1, h.y1], kind));
        }
        boxes.retain(|(b, _)| b.iter().all(|v| v.is_finite()) && b[2] > b[0] && b[3] > b[1]);
        boxes.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
        for i in 0..boxes.len() {
            let (a, ka) = boxes[i];
            for &(b, kb) in boxes.iter().skip(i + 1) {
                if b[0] >= a[2] {
                    break;
                }
                let counts = match (ka, kb) {
                    (Kind::Note, Kind::Note) => false,
                    (Kind::Text, Kind::Frame) => !inside(&a, &b),
                    (Kind::Frame, Kind::Text) => !inside(&b, &a),
                    (Kind::Dynamic, Kind::Dynamic) | (Kind::Frame, Kind::Frame) => true,
                    _ => true,
                };
                if counts && overlap(&a, &b) > LEAST_AREA {
                    out.push(Collision { page: pi, a, b });
                    if out.len() >= MAX {
                        return out;
                    }
                }
            }
        }
    }
    out
}

/// Moves dynamics and staff texts that sit on notes, rests or other texts up or down, in
/// small steps, to the nearest free place (at most `MAX_NUDGE` away) — as MuseScore's
/// automatic placement keeps them clear. Away from the middle of their staff first.
pub(crate) fn settle(l: &mut Layout) {
    for pi in 0..l.pages.len() {
        // What may not be covered: noteheads, rests, and the texts that stay (lyrics, chord
        // symbols …).
        let Some(page) = l.pages.get(pi) else { continue };
        let mut notes: Vec<[f32; 4]> = l
            .hits
            .iter()
            .filter(|h| h.page == pi && matches!(h.target, Target::Note { .. } | Target::Rest { .. }))
            .map(|h| [h.x0, h.y0, h.x1, h.y1])
            .collect();
        notes.extend(beams(page));
        let fixed_texts: Vec<[f32; 4]> = page
            .items
            .iter()
            .filter_map(|it| match &it.shape {
                Shape::Text {
                    text,
                    x,
                    y,
                    size,
                    align,
                    kind,
                    ..
                } if !matches!(kind, Some(TextKind::StaffText)) => text_box(text, *x, *y, *size, *align),
                _ => None,
            })
            .collect();
        let staff_mids: Vec<(f32, f32, f32)> = l
            .staves
            .iter()
            .filter(|s| s.page == pi)
            .map(|s| (s.x0, s.x1, s.top + 2.0 * s.mag))
            .collect();
        let mut placed: Vec<[f32; 4]> = Vec::new();
        let blocked = |b: &[f32; 4], placed: &[[f32; 4]]| {
            notes
                .iter()
                .chain(fixed_texts.iter())
                .chain(placed.iter())
                .any(|o| overlap(b, o) > LEAST_AREA)
        };
        let first_dir = |b: &[f32; 4]| {
            let cx = (b[0] + b[2]) / 2.0;
            let cy = (b[1] + b[3]) / 2.0;
            let mid = staff_mids
                .iter()
                .filter(|s| cx >= s.0 && cx <= s.1)
                .min_by(|a, c| (a.2 - cy).abs().total_cmp(&(c.2 - cy).abs()))
                .map_or(cy, |s| s.2);
            if cy < mid { -1.0 } else { 1.0 }
        };
        let nudge = |b: &[f32; 4], placed: &[[f32; 4]]| -> f32 {
            if !blocked(b, placed) {
                return 0.0;
            }
            let dir = first_dir(b);
            let mut step = 0.25f32;
            while step <= MAX_NUDGE {
                for d in [dir * step, -dir * step] {
                    let moved = [b[0], b[1] + d, b[2], b[3] + d];
                    if !blocked(&moved, placed) {
                        return d;
                    }
                }
                step += 0.25;
            }
            0.0
        };
        // Dynamics: their hit box and the glyphs inside it.
        let dyn_hits: Vec<usize> = (0..l.hits.len())
            .filter(|i| l.hits[*i].page == pi && matches!(l.hits[*i].target, Target::Dynamic { .. }))
            .collect();
        for hi in dyn_hits {
            let h = l.hits[hi];
            let b = [h.x0, h.y0, h.x1, h.y1];
            let d = nudge(&b, &placed);
            placed.push([b[0], b[1] + d, b[2], b[3] + d]);
            if d == 0.0 {
                continue;
            }
            l.hits[hi].y0 += d;
            l.hits[hi].y1 += d;
            if let Some(page) = l.pages.get_mut(pi) {
                for it in &mut page.items {
                    if let Shape::Glyph { x, y, .. } = &mut it.shape
                        && *x >= h.x0 - 0.01
                        && *x <= h.x1
                        && *y >= h.y0
                        && *y <= h.y1
                        && it.owner.is_some()
                    {
                        *y += d;
                    }
                }
            }
        }
        // Staff texts.
        let Some(page) = l.pages.get(pi) else { continue };
        let texts: Vec<(usize, [f32; 4])> = page
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, it)| match &it.shape {
                Shape::Text {
                    text,
                    x,
                    y,
                    size,
                    align,
                    kind: Some(TextKind::StaffText),
                    ..
                } => text_box(text, *x, *y, *size, *align).map(|b| (i, b)),
                _ => None,
            })
            .collect();
        for (i, b) in texts {
            let d = nudge(&b, &placed);
            placed.push([b[0], b[1] + d, b[2], b[3] + d]);
            if d == 0.0 {
                continue;
            }
            if let Some(Shape::Text { y, .. }) = l
                .pages
                .get_mut(pi)
                .and_then(|p| p.items.get_mut(i))
                .map(|it| &mut it.shape)
            {
                *y += d;
            }
            for h in l.hits.iter_mut().filter(|h| {
                h.page == pi
                    && matches!(h.target, Target::Text { .. })
                    && h.x0 <= b[0] + 0.3
                    && h.x1 >= b[2] - 0.3
                    && h.y1 >= b[1]
                    && h.y0 <= b[3]
            }) {
                h.y0 += d;
                h.y1 += d;
            }
        }
    }
}

/// Farthest a dynamic or staff text is moved to keep clear, staff spaces.
const MAX_NUDGE: f32 = 4.0;

/// The beams of a page (filled quadrilaterals of notes) as boxes; a slanted beam's box is
/// a little more than the beam.
fn beams(page: &crate::Page) -> Vec<[f32; 4]> {
    page.items
        .iter()
        .filter(|it| it.owner.is_some())
        .filter_map(|it| match &it.shape {
            Shape::Quad { p } => Some([
                p.iter().map(|q| q.0).fold(f32::MAX, f32::min),
                p.iter().map(|q| q.1).fold(f32::MAX, f32::min),
                p.iter().map(|q| q.0).fold(f32::MIN, f32::max),
                p.iter().map(|q| q.1).fold(f32::MIN, f32::max),
            ]),
            _ => None,
        })
        .collect()
}
