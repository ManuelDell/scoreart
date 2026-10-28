//! Renders an engraved [`Layout`] page to SVG. Music glyphs are emitted as real outlines from the
//! bundled Bravura font (OFL-1.1), so the SVG needs no font installed; text uses Crimson Text (serif) or a sans-serif font, by style.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use scoreart_engrave::{Align, Layout, Page, Shape, Style};

/// Bravura, the SMuFL reference font by Steinberg Media Technologies GmbH (SIL OFL 1.1),
/// bundled unmodified. See `assets/fonts/OFL-Bravura.txt`.
pub static BRAVURA: &[u8] = include_bytes!("../../../assets/fonts/Bravura.otf");

/// Crimson Text by Sebastian Kosch (SIL OFL 1.1), the serif text font, bundled unmodified.
/// See `assets/fonts/OFL-CrimsonText.txt`.
pub static SERIF: &[u8] = include_bytes!("../../../assets/fonts/CrimsonText-Regular.ttf");

struct PathBuilder {
    d: String,
}

impl ttf_parser::OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.d, "M{x} {y}");
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let _ = write!(self.d, "L{x} {y}");
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let _ = write!(self.d, "Q{x1} {y1} {x} {y}");
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let _ = write!(self.d, "C{x1} {y1} {x2} {y2} {x} {y}");
    }
    fn close(&mut self) {
        self.d.push('Z');
    }
}

/// Outline of a glyph in font units (y up), as SVG path data.
pub fn glyph_path(c: char) -> Option<(String, f32)> {
    let face = ttf_parser::Face::parse(BRAVURA, 0).ok()?;
    let id = face.glyph_index(c)?;
    let mut b = PathBuilder { d: String::new() };
    face.outline_glyph(id, &mut b)?;
    Some((b.d, f32::from(face.units_per_em())))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn f(v: f32) -> String {
    if v.is_finite() {
        format!("{:.3}", v)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    } else {
        "0".into()
    }
}

/// Fills a tie/slur centre line as a crescent: the curve's control points are pushed apart by
/// `thick` along the normal of its chord.
pub fn crescent(p: [(f32, f32); 4], thick: f32) -> [(f32, f32); 6] {
    let (dx, dy) = (p[3].0 - p[0].0, p[3].1 - p[0].1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (nx, ny) = (-dy / len * thick * 0.66, dx / len * thick * 0.66);
    [
        (p[1].0 + nx, p[1].1 + ny),
        (p[2].0 + nx, p[2].1 + ny),
        (p[2].0 - nx, p[2].1 - ny),
        (p[1].0 - nx, p[1].1 - ny),
        p[0],
        p[3],
    ]
}

/// One page as a standalone SVG document sized in millimetres.
pub fn page_svg(page: &Page, style: &Style) -> String {
    let mm = if style.staff_space_mm.is_finite() {
        style.staff_space_mm.clamp(0.5, 10.0)
    } else {
        1.75
    };
    let mut defs: BTreeMap<char, String> = BTreeMap::new();
    let mut body = String::new();
    for it in &page.items {
        match &it.shape {
            Shape::Glyph { c, x, y, scale } => {
                if !defs.contains_key(c)
                    && let Some((d, upm)) = glyph_path(*c)
                {
                    // Normalise to an em of 4 staff spaces, y down.
                    let s = 4.0 / upm.max(1.0);
                    defs.insert(
                        *c,
                        format!(
                            "<path id=\"g{:X}\" transform=\"scale({} {})\" d=\"{}\"/>",
                            *c as u32,
                            f(s),
                            f(-s),
                            d
                        ),
                    );
                }
                if defs.contains_key(c) {
                    let _ = writeln!(
                        body,
                        "<use href=\"#g{:X}\" transform=\"translate({} {}) scale({})\"/>",
                        *c as u32,
                        f(*x),
                        f(*y),
                        f(*scale)
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
                let _ = writeln!(
                    body,
                    "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#000\" stroke-width=\"{}\"/>",
                    f(*x1),
                    f(*y1),
                    f(*x2),
                    f(*y2),
                    f(*width)
                );
            }
            Shape::Rect { x, y, w, h } => {
                let _ = writeln!(
                    body,
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                    f(*x),
                    f(*y),
                    f(*w),
                    f(*h)
                );
            }
            Shape::Quad { p } => {
                let pts: Vec<String> = p.iter().map(|(x, y)| format!("{},{}", f(*x), f(*y))).collect();
                let _ = writeln!(body, "<polygon points=\"{}\"/>", pts.join(" "));
            }
            Shape::Curve { p, thick } => {
                let c = crescent(*p, *thick);
                let _ = writeln!(
                    body,
                    "<path d=\"M{} {}C{} {} {} {} {} {}C{} {} {} {} {} {}Z\"/>",
                    f(c[4].0),
                    f(c[4].1),
                    f(c[0].0),
                    f(c[0].1),
                    f(c[1].0),
                    f(c[1].1),
                    f(c[5].0),
                    f(c[5].1),
                    f(c[2].0),
                    f(c[2].1),
                    f(c[3].0),
                    f(c[3].1),
                    f(c[4].0),
                    f(c[4].1)
                );
            }
            Shape::Text {
                text,
                x,
                y,
                size,
                align,
                bold,
                italic,
                serif,
                ..
            } => {
                let anchor = match align {
                    Align::Left => "start",
                    Align::Center => "middle",
                    Align::Right => "end",
                };
                let _ = writeln!(
                    body,
                    "<text x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"{anchor}\" font-family=\"{}\"{}{}>{}</text>",
                    f(*x),
                    f(*y),
                    f(*size),
                    if *serif {
                        "Crimson Text, Georgia, serif"
                    } else {
                        "Helvetica, Arial, sans-serif"
                    },
                    if *bold { " font-weight=\"bold\"" } else { "" },
                    if *italic { " font-style=\"italic\"" } else { "" },
                    esc(text)
                );
            }
        }
    }
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}mm\" height=\"{}mm\" viewBox=\"0 0 {} {}\">",
        f(page.width * mm),
        f(page.height * mm),
        f(page.width),
        f(page.height)
    );
    svg.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#fff\"/>\n");
    svg.push_str("<defs>\n");
    for d in defs.values() {
        svg.push_str(d);
        svg.push('\n');
    }
    svg.push_str("</defs>\n<g fill=\"#000\" font-family=\"'Times New Roman', 'Liberation Serif', 'DejaVu Serif', serif\">\n");
    svg.push_str(&body);
    svg.push_str("</g>\n</svg>\n");
    svg
}

/// Every page of a layout as SVG documents.
pub fn svg_pages(layout: &Layout, style: &Style) -> Vec<String> {
    layout.pages.iter().map(|p| page_svg(p, style)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bravura_has_noteheads() {
        let (d, upm) = glyph_path(scoreart_engrave::glyphs::HEAD_BLACK).unwrap();
        assert!(d.starts_with('M'));
        assert_eq!(upm, 1000.0);
    }
}
