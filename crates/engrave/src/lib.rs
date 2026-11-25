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

mod collide;
pub mod glyphs;
mod layout;
mod serif;
pub mod smufl;

pub use layout::{layout, text_accidental, text_width};
use scoreart_model::edit::EventRef;
use scoreart_model::{Clef, PageFormat};

/// How the score is laid out on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum View {
    /// Pages as printed.
    #[default]
    Page,
    /// One endless system from left to right.
    Horizontal,
    /// Systems one below the other on one endless page.
    Vertical,
}

/// Page and spacing settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub staff_space_mm: f32,
    pub page_width_mm: f32,
    pub page_height_mm: f32,
    pub margin_top_mm: f32,
    pub margin_bottom_mm: f32,
    pub margin_left_mm: f32,
    pub margin_right_mm: f32,
    /// Gap between the staves of one part (bottom line to top line), sp.
    pub staff_gap: f32,
    /// Gap between parts, sp.
    pub part_gap: f32,
    /// Gap between systems, sp.
    pub system_gap: f32,
    /// Horizontal spacing factor (1.0 = default).
    pub spacing: f32,
    pub view: View,
    /// Everything else of the page format: two-sided margins, music margins, vertical fill,
    /// last-system fill, first page number, first-system indent, header and footer offsets.
    pub page: PageFormat,
    /// File name, dates and part name for the macros of headers and footers.
    pub doc: scoreart_model::DocInfo,
}

impl Default for Style {
    fn default() -> Self {
        Style::new(&PageFormat::default(), View::Page)
    }
}

impl Style {
    /// The style for a score's page format, shown in `view`.
    pub fn new(page: &PageFormat, view: View) -> Style {
        let p = page.clamped();
        Style {
            staff_space_mm: p.staff_space_mm,
            page_width_mm: p.width_mm,
            page_height_mm: p.height_mm,
            margin_top_mm: p.margin_top_mm,
            margin_bottom_mm: p.margin_bottom_mm,
            margin_left_mm: p.margin_left_mm,
            margin_right_mm: p.margin_right_mm,
            staff_gap: p.staff_gap,
            part_gap: p.part_gap,
            system_gap: p.system_gap,
            spacing: p.spacing,
            view,
            page: p,
            doc: scoreart_model::DocInfo::default(),
        }
    }

    /// Margins of a page (by its printed number) in staff spaces: top, bottom, left, right.
    /// The odd-page margins are this style's own fields; even pages of a two-sided score have
    /// theirs from the page format.
    pub fn margins_of(&self, page_number: i32) -> [f32; 4] {
        if self.page.two_sided && page_number.rem_euclid(2) == 0 {
            let s = self.space();
            [
                self.page.even_margin_top_mm,
                self.page.even_margin_bottom_mm,
                self.page.even_margin_left_mm,
                self.page.even_margin_right_mm,
            ]
            .map(|m| fin(m, 15.0).clamp(0.0, 100.0) / s)
        } else {
            self.margins_sp()
        }
    }

    /// Page size in staff spaces.
    pub fn page_sp(&self) -> (f32, f32) {
        let s = self.space();
        (
            fin(self.page_width_mm, 210.0).clamp(50.0, 2000.0) / s,
            fin(self.page_height_mm, 297.0).clamp(50.0, 2000.0) / s,
        )
    }

    /// Margins in staff spaces: top, bottom, left, right.
    pub fn margins_sp(&self) -> [f32; 4] {
        let s = self.space();
        [
            self.margin_top_mm,
            self.margin_bottom_mm,
            self.margin_left_mm,
            self.margin_right_mm,
        ]
        .map(|m| fin(m, 15.0).clamp(0.0, 100.0) / s)
    }

    pub(crate) fn space(&self) -> f32 {
        fin(self.staff_space_mm, 1.75).clamp(0.5, 10.0)
    }
}

fn fin(v: f32, d: f32) -> f32 {
    if v.is_finite() { v } else { d }
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
        /// The serif text font (else sans-serif).
        serif: bool,
        /// Which kind of text it is (for styles and highlighting), if it has one.
        kind: Option<scoreart_model::TextKind>,
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
    /// The staff text of an event.
    Text {
        ev: EventRef,
    },
    /// The dynamic of an event.
    Dynamic {
        ev: EventRef,
    },
    /// A piece of a line (`score.spanners[index]`); whether it holds the line's start or end.
    Line {
        index: usize,
        first: bool,
        last: bool,
    },
    Measure {
        part: usize,
        staff: usize,
        measure: usize,
    },
    /// A text block of the page (title, heading, header …).
    Block {
        block: scoreart_model::BlockRef,
    },
    /// A rehearsal mark, segno/coda sign or jump of a measure, with what moving it needs:
    /// its anchor (`ax`: the measure's start, or end for a jump; `ay`: the top staff line), and
    /// where it stands now (`x`: the mark's centre, left or right end; `base`: its baseline).
    MeasureMark {
        measure: usize,
        kind: MeasureMark,
        ax: f32,
        ay: f32,
        x: f32,
        base: f32,
    },
}

/// The marks of a measure that can be moved by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasureMark {
    /// A boxed rehearsal mark (its centre).
    Rehearsal,
    /// Segno or coda at the start (its left end).
    Marker,
    /// D.S., D.C., Fine, To Coda at the end (its right end).
    Jump,
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
    /// Size of the staff (smaller for a small staff: its lines are this many spaces apart).
    pub mag: f32,
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
        let half_spaces = ((y - self.top) * 2.0 / self.mag.max(0.1)).round() as i32;
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
    /// Marks for the editor only (not printed): breaks and spacers.
    pub marks: Vec<Mark>,
    /// Places where things overlap (texts, lyrics, chord symbols, marks, notes), for the
    /// editor to point out; never printed.
    pub collisions: Vec<Collision>,
}

/// Two boxes that overlap on a page: `a` and `b` as (x0, y0, x1, y1) in staff spaces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collision {
    pub page: usize,
    pub a: [f32; 4],
    pub b: [f32; 4],
}

/// A mark the editor shows on the page but that is not printed (as MuseScore shows its layout
/// breaks and spacers).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub page: usize,
    /// For a break: where its sign sits (above the end of the measure); for a spacer: the
    /// staff line it starts from.
    pub x: f32,
    pub y: f32,
    pub kind: MarkKind,
    /// The measure (and staff of a spacer) it belongs to.
    pub measure: usize,
    pub part: usize,
    pub staff: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MarkKind {
    LineBreak,
    PageBreak,
    SectionBreak,
    /// Spacers with their height in staff spaces: up from the top line, down from the
    /// bottom line, fixed down from the bottom line.
    SpacerUp(f32),
    SpacerDown(f32),
    SpacerFixed(f32),
}

impl Layout {
    /// The most specific target under (x, y) on `page`: notes and rests before measures.
    pub fn hit(&self, page: usize, x: f32, y: f32) -> Option<Target> {
        let inside = |h: &&Hit| h.page == page && x >= h.x0 && x <= h.x1 && y >= h.y0 && y <= h.y1;
        let rank = |t: &Target| match t {
            Target::Note { .. } => 0,
            Target::Text { .. }
            | Target::Dynamic { .. }
            | Target::Block { .. }
            | Target::MeasureMark { .. } => 1,
            Target::Line { .. } => 2,
            Target::Rest { .. } => 3,
            Target::Measure { .. } => 4,
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
                Target::Text { .. }
                | Target::Dynamic { .. }
                | Target::Line { .. }
                | Target::Measure { .. }
                | Target::MeasureMark { .. }
                | Target::Block { .. } => false,
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
