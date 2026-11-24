//! What a page draws, in points with y down: glyphs (with their font and character), straight
//! strokes, filled polygons and curves. This is everything the recognition looks at.

use hayro::hayro_interpret::font::Glyph as HGlyph;
use hayro::hayro_interpret::kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape};
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps, InterpreterCache,
    InterpreterSettings, SoftMask, font::GlyphRun, interpret_page,
};

use crate::PdfDoc;

/// Most drawing operations kept per page (hostile files).
const MAX_ITEMS: usize = 200_000;

/// A glyph of a font at its place.
#[derive(Clone, Debug)]
pub(crate) struct Glyph {
    /// PostScript name of the font (subset prefix removed).
    pub font: String,
    /// The character, when the PDF says which one it is.
    pub ch: Option<char>,
    /// Origin (baseline point).
    pub x: f32,
    pub y: f32,
    /// Bounding box of the drawn outline.
    pub bx0: f32,
    pub by0: f32,
    pub bx1: f32,
    pub by1: f32,
    /// Font size in points.
    pub size: f32,
}

impl Glyph {
    pub fn cx(&self) -> f32 {
        (self.bx0 + self.bx1) / 2.0
    }
    pub fn cy(&self) -> f32 {
        (self.by0 + self.by1) / 2.0
    }
}

/// A straight stroke from (x0, y0) to (x1, y1), `w` thick (also thin filled rectangles).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Seg {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub w: f32,
}

impl Seg {
    pub fn is_vertical(&self) -> bool {
        (self.x1 - self.x0).abs() < 0.35 && (self.y1 - self.y0).abs() > 1.0
    }
    pub fn is_horizontal(&self) -> bool {
        (self.y1 - self.y0).abs() < 0.35 && (self.x1 - self.x0).abs() > 1.0
    }
    pub fn top(&self) -> f32 {
        self.y0.min(self.y1)
    }
    pub fn bottom(&self) -> f32 {
        self.y0.max(self.y1)
    }
    pub fn left(&self) -> f32 {
        self.x0.min(self.x1)
    }
    pub fn right(&self) -> f32 {
        self.x0.max(self.x1)
    }
    pub fn xm(&self) -> f32 {
        (self.x0 + self.x1) / 2.0
    }
}

/// A filled polygon without curves (beams, thick barlines, wedges drawn as areas).
#[derive(Clone, Debug)]
pub(crate) struct Poly {
    pub pts: Vec<(f32, f32)>,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Poly {
    /// The vertical middle of the polygon at `x` (beams), if `x` is within it.
    pub fn mid_at(&self, x: f32) -> Option<f32> {
        if x < self.x0 - 0.5 || x > self.x1 + 0.5 {
            return None;
        }
        // Crossings of the vertical line at x with the edges.
        let mut ys: Vec<f32> = Vec::new();
        let n = self.pts.len();
        for i in 0..n {
            let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
            let (lo, hi) = if a.0 <= b.0 { (a, b) } else { (b, a) };
            let xx = x.clamp(self.x0, self.x1);
            if hi.0 - lo.0 > 1e-3 && xx >= lo.0 && xx <= hi.0 {
                let t = (xx - lo.0) / (hi.0 - lo.0);
                ys.push(lo.1 + t * (hi.1 - lo.1));
            }
        }
        if ys.is_empty() {
            return Some((self.y0 + self.y1) / 2.0);
        }
        let (mn, mx) = ys
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), y| (a.min(*y), b.max(*y)));
        Some((mn + mx) / 2.0)
    }
    pub fn w(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn h(&self) -> f32 {
        self.y1 - self.y0
    }
}

/// A curved shape (ties, slurs): its ends and bounding box.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Curve {
    pub a: (f32, f32),
    pub b: (f32, f32),
    pub y0: f32,
    pub y1: f32,
}

#[derive(Default)]
pub(crate) struct PageContent {
    pub width: f32,
    pub height: f32,
    pub glyphs: Vec<Glyph>,
    pub segs: Vec<Seg>,
    pub polys: Vec<Poly>,
    pub curves: Vec<Curve>,
}

struct Recorder {
    page: PageContent,
    count: usize,
}

fn subpaths(path: &BezPath) -> Vec<(Vec<Point>, bool, bool)> {
    // (points, has curves, closed)
    let mut out = Vec::new();
    let mut pts: Vec<Point> = Vec::new();
    let mut curved = false;
    let mut closed = false;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                if pts.len() > 1 {
                    out.push((std::mem::take(&mut pts), curved, closed));
                }
                pts.clear();
                pts.push(p);
                curved = false;
                closed = false;
            }
            PathEl::LineTo(p) => pts.push(p),
            PathEl::QuadTo(_, p) => {
                curved = true;
                pts.push(p);
            }
            PathEl::CurveTo(_, _, p) => {
                curved = true;
                pts.push(p);
            }
            PathEl::ClosePath => closed = true,
        }
    }
    if pts.len() > 1 {
        out.push((pts, curved, closed));
    }
    out
}

fn bbox(pts: &[Point]) -> (f32, f32, f32, f32) {
    pts.iter()
        .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |(a, b, c, d), p| {
            (
                a.min(p.x as f32),
                b.min(p.y as f32),
                c.max(p.x as f32),
                d.max(p.y as f32),
            )
        })
}

impl Recorder {
    fn full(&mut self) -> bool {
        self.count += 1;
        self.count > MAX_ITEMS
    }

    fn add_path(&mut self, path: &BezPath, stroke_w: Option<f32>) {
        for (pts, curved, closed) in subpaths(path) {
            if self.full() {
                return;
            }
            let (x0, y0, x1, y1) = bbox(&pts);
            if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
                continue;
            }
            let (w, h) = (x1 - x0, y1 - y0);
            match stroke_w {
                Some(sw) if !curved => {
                    for p in pts.windows(2) {
                        self.page.segs.push(Seg {
                            x0: p[0].x as f32,
                            y0: p[0].y as f32,
                            x1: p[1].x as f32,
                            y1: p[1].y as f32,
                            w: sw,
                        });
                    }
                    if closed && pts.len() > 2 {
                        let (a, b) = (pts[pts.len() - 1], pts[0]);
                        self.page.segs.push(Seg {
                            x0: a.x as f32,
                            y0: a.y as f32,
                            x1: b.x as f32,
                            y1: b.y as f32,
                            w: sw,
                        });
                    }
                }
                _ => {
                    let thin = w.min(h);
                    let long = w.max(h);
                    // A thin filled shape is a stroke (stems, barlines, staff lines drawn as
                    // rectangles, also with rounded ends).
                    if thin < 1.3 && long > 2.0 * thin.max(0.1) {
                        if h >= w {
                            let xm = (x0 + x1) / 2.0;
                            self.page.segs.push(Seg {
                                x0: xm,
                                y0,
                                x1: xm,
                                y1,
                                w,
                            });
                        } else {
                            let ym = (y0 + y1) / 2.0;
                            self.page.segs.push(Seg {
                                x0,
                                y0: ym,
                                x1,
                                y1: ym,
                                w: h,
                            });
                        }
                    } else if curved {
                        // A filled crescent runs there and back: its far end is half way round;
                        // a stroked curve ends at its last point.
                        let far = if stroke_w.is_some() {
                            pts.len() - 1
                        } else {
                            pts.len() / 2
                        };
                        let (a, b) = (pts[0], pts[far]);
                        self.page.curves.push(Curve {
                            a: (a.x as f32, a.y as f32),
                            b: (b.x as f32, b.y as f32),
                            y0,
                            y1,
                        });
                    } else {
                        self.page.polys.push(Poly {
                            pts: pts.iter().map(|p| (p.x as f32, p.y as f32)).collect(),
                            x0,
                            y0,
                            x1,
                            y1,
                        });
                    }
                }
            }
        }
    }
}

fn scale_of(t: &Affine) -> f32 {
    let [a, b, c, d, _, _] = t.as_coeffs();
    ((a * d - b * c).abs().sqrt()) as f32
}

impl<'a> Device<'a> for Recorder {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, mode: &DrawMode) {
        let p = props.transform * path.clone();
        let sw = match mode {
            DrawMode::Stroke(s) => Some(s.line_width * scale_of(&props.transform)),
            _ => None,
        };
        self.add_path(&p, sw);
    }
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        for g in run.glyphs() {
            if self.full() {
                return;
            }
            let t = props.transform * g.transform();
            let (font, rect) = match &**g {
                HGlyph::Outline(o) => {
                    let name = o.font_data().and_then(|f| f.postscript_name).unwrap_or_default();
                    (name, (t * o.outline()).bounding_box())
                }
                HGlyph::Type3(_) => (String::new(), Rect::ZERO),
            };
            let ch = g.as_unicode().and_then(|u| match u {
                hayro::hayro_interpret::hayro_cmap::BfString::Char(c) => Some(c),
                _ => None,
            });
            let origin = t * Point::ZERO;
            // Outlines are in thousandths of an em.
            let size = scale_of(&t) * 1000.0;
            let font = match font.split_once('+') {
                Some((tag, rest)) if tag.len() == 6 => rest.to_string(),
                _ => font,
            };
            self.page.glyphs.push(Glyph {
                font,
                ch,
                x: origin.x as f32,
                y: origin.y as f32,
                bx0: rect.x0 as f32,
                by0: rect.y0 as f32,
                bx1: rect.x1 as f32,
                by1: rect.y1 as f32,
                size,
            });
        }
    }
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

/// Everything drawn on every page.
pub(crate) fn collect(doc: &PdfDoc) -> Vec<PageContent> {
    let pdf = doc.pdf();
    let pages = pdf.pages();
    let mut out = Vec::new();
    for page in pages.iter().take(crate::MAX_PAGES) {
        let (w, h) = page.render_dimensions();
        let cache = InterpreterCache::new();
        let mut ctx = Context::new(
            page.initial_transform(true).to_kurbo(),
            Rect::new(0.0, 0.0, f64::from(w.max(1.0)) * 4.0, f64::from(h.max(1.0)) * 4.0),
            &cache,
            page.xref(),
            InterpreterSettings::default(),
        );
        let mut rec = Recorder {
            page: PageContent {
                width: w,
                height: h,
                ..PageContent::default()
            },
            count: 0,
        };
        interpret_page(page, &mut ctx, &mut rec);
        // Text drawn twice at the same place (bold made by overprinting) counts once.
        let mut page = rec.page;
        page.glyphs
            .sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
        page.glyphs
            .dedup_by(|a, b| a.ch == b.ch && (a.x - b.x).abs() < 0.05 && (a.y - b.y).abs() < 0.05);
        out.push(page);
    }
    out
}
