//! Where things overlap on the laid-out pages: texts (lyrics, chord symbols, staff and system
//! texts, titles), rehearsal marks, dynamics, noteheads and rests. The editor shows them so
//! layout problems do not go unnoticed; they are never printed.

use crate::{Align, Collision, Layout, MeasureMark, Shape, Target};

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
