//! The ScoreArt engraver: lays a [`Score`] out into pages of renderer-neutral shapes.
//!
//! All coordinates are in **staff spaces** (sp) from the top-left corner of a page; a glyph's
//! position is its SMuFL origin (left edge, baseline), drawn with a font whose em is 4 sp.
//! Engraving rules are implemented from general music-engraving knowledge (clean room; see
//! `AGENTS.md`).
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod glyphs;
mod layout;
pub mod smufl;

pub use layout::layout;
use scoreart_model::Clef;
use scoreart_model::edit::EventRef;

/// Page and spacing settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub staff_space_mm: f32,
    pub page_width_mm: f32,
    pub page_height_mm: f32,
    pub margin_mm: f32,
    /// Gap between the staves of one part (bottom line to top line), sp.
    pub staff_gap: f32,
    /// Gap between parts, sp.
    pub part_gap: f32,
    /// Gap between systems, sp.
    pub system_gap: f32,
    /// Horizontal spacing factor (1.0 = default).
    pub spacing: f32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            staff_space_mm: 1.75,
            page_width_mm: 210.0,
            page_height_mm: 297.0,
            margin_mm: 15.0,
            staff_gap: 6.0,
            part_gap: 7.0,
            system_gap: 9.0,
            spacing: 1.0,
        }
    }
}

impl Style {
    /// Page size in staff spaces.
    pub fn page_sp(&self) -> (f32, f32) {
        let s = self.staff_space_mm.clamp(0.5, 10.0);
        (
            self.page_width_mm.clamp(50.0, 2000.0) / s,
            self.page_height_mm.clamp(50.0, 2000.0) / s,
        )
    }

    pub fn margin_sp(&self) -> f32 {
        self.margin_mm.clamp(0.0, 100.0) / self.staff_space_mm.clamp(0.5, 10.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A SMuFL glyph at its origin; `scale` 1.0 = em of 4 sp.
    Glyph {
        c: char,
        x: f32,
        y: f32,
        scale: f32,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    /// A filled convex quadrilateral (beams).
    Quad {
        p: [(f32, f32); 4],
    },
    /// A tie or slur: a cubic Bézier centre line, filled as a crescent `thick` sp at its middle.
    Curve {
        p: [(f32, f32); 4],
        thick: f32,
    },
    /// Text; `y` is the baseline, `size` the font size in sp.
    Text {
        text: String,
        x: f32,
        y: f32,
        size: f32,
        align: Align,
        bold: bool,
        italic: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub shape: Shape,
    /// The event this shape belongs to (for selection highlighting).
    pub owner: Option<EventRef>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub items: Vec<Item>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Note {
        ev: EventRef,
        note: usize,
    },
    Rest {
        ev: EventRef,
    },
    Measure {
        part: usize,
        staff: usize,
        measure: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub page: usize,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub target: Target,
}

/// Where one staff of one measure sits, for cursors and mouse input.
#[derive(Clone, Debug, PartialEq)]
pub struct StaffBox {
    pub page: usize,
    pub system: usize,
    pub part: usize,
    pub staff: usize,
    pub measure: usize,
    pub x0: f32,
    pub x1: f32,
    /// y of the top staff line.
    pub top: f32,
    pub clef: Clef,
    /// Onset tick → notehead x, ascending; the last entry is the measure end.
    pub columns: Vec<(u32, f32)>,
}

impl StaffBox {
    /// Notehead x for a tick (exact column, or interpolated).
    pub fn x_at(&self, tick: u32) -> f32 {
        let mut prev: Option<(u32, f32)> = None;
        for &(t, x) in &self.columns {
            if t == tick {
                return x;
            }
            if t > tick {
                if let Some((pt, px)) = prev
                    && t > pt
                {
                    return px + (x - px) * (tick - pt) as f32 / (t - pt) as f32;
                }
                return x;
            }
            prev = Some((t, x));
        }
        prev.map(|(_, x)| x).unwrap_or(self.x0)
    }

    /// The diatonic step (see `Pitch::diatonic`) at page y.
    pub fn diatonic_at(&self, y: f32) -> i32 {
        let half_spaces = ((y - self.top) * 2.0).round() as i32;
        self.clef.top_line() - half_spaces.clamp(-40, 40)
    }

    /// The onset column nearest to x (excluding the measure-end entry).
    pub fn tick_near(&self, x: f32) -> u32 {
        let n = self.columns.len().saturating_sub(1).max(1);
        self.columns
            .iter()
            .take(n)
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map(|c| c.0)
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Layout {
    pub pages: Vec<Page>,
    pub hits: Vec<Hit>,
    pub staves: Vec<StaffBox>,
}

impl Layout {
    /// The most specific target under (x, y) on `page`: notes and rests before measures.
    pub fn hit(&self, page: usize, x: f32, y: f32) -> Option<Target> {
        let inside = |h: &&Hit| h.page == page && x >= h.x0 && x <= h.x1 && y >= h.y0 && y <= h.y1;
        let rank = |t: &Target| match t {
            Target::Note { .. } => 0,
            Target::Rest { .. } => 1,
            Target::Measure { .. } => 2,
        };
        self.hits
            .iter()
            .filter(inside)
            .min_by(|a, b| {
                rank(&a.target)
                    .cmp(&rank(&b.target))
                    .then(((a.x1 - a.x0) * (a.y1 - a.y0)).total_cmp(&((b.x1 - b.x0) * (b.y1 - b.y0))))
            })
            .map(|h| h.target)
    }

    pub fn staff_box(&self, part: usize, staff: usize, measure: usize) -> Option<&StaffBox> {
        self.staves
            .iter()
            .find(|s| s.part == part && s.staff == staff && s.measure == measure)
    }

    /// The staff box under (x, y), with a little vertical slack for ledger-line input.
    pub fn staff_box_at(&self, page: usize, x: f32, y: f32) -> Option<&StaffBox> {
        self.staves
            .iter()
            .filter(|s| s.page == page && x >= s.x0 && x <= s.x1 && y >= s.top - 4.0 && y <= s.top + 8.0)
            .min_by(|a, b| (a.top + 2.0 - y).abs().total_cmp(&(b.top + 2.0 - y).abs()))
    }

    /// The hit box of an event (union of its noteheads or its rest).
    pub fn event_box(&self, ev: EventRef) -> Option<Hit> {
        let mut out: Option<Hit> = None;
        for h in &self.hits {
            let matches = match h.target {
                Target::Note { ev: e, .. } | Target::Rest { ev: e } => e == ev,
                Target::Measure { .. } => false,
            };
            if matches {
                out = Some(match out {
                    None => *h,
                    Some(o) => Hit {
                        x0: o.x0.min(h.x0),
                        y0: o.y0.min(h.y0),
                        x1: o.x1.max(h.x1),
                        y1: o.y1.max(h.y1),
                        ..o
                    },
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;
