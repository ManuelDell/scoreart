//! Reading PDF files: the pages as pictures (to show a PDF next to the score it is being
//! copied into) and the recognition of the music in PDFs that notation programs wrote, where
//! notes are glyphs of a music font and staff lines, stems and beams are lines — see
//! [`recognize`].
//!
//! Built on hayro (MIT OR Apache-2.0), a PDF reader and rasterizer written in Rust.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

mod alternates;
mod collect;
mod fonts;
mod recognize;

pub use recognize::{Recognition, recognize};

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};

/// Most pages read from one file.
pub const MAX_PAGES: usize = 500;
/// Largest picture of a page, in pixels per side.
pub const MAX_PIXELS: u32 = 4000;

/// A PDF document, opened once and drawn page by page.
pub struct PdfDoc {
    pdf: Pdf,
    /// Page sizes in points (1/72 inch).
    sizes: Vec<(f32, f32)>,
}

/// One page as straight RGBA pixels.
pub struct PagePicture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl PdfDoc {
    /// Opens a PDF; fails for files that are not PDFs, encrypted or have no pages.
    pub fn open(bytes: Vec<u8>) -> Result<PdfDoc, String> {
        let pdf = Pdf::new(bytes).map_err(|e| format!("not a readable PDF ({e:?})"))?;
        let sizes: Vec<(f32, f32)> = pdf
            .pages()
            .iter()
            .take(MAX_PAGES)
            .map(|p| p.render_dimensions())
            .collect();
        if sizes.is_empty() {
            return Err("the PDF has no pages".into());
        }
        Ok(PdfDoc { pdf, sizes })
    }

    pub fn page_count(&self) -> usize {
        self.sizes.len()
    }

    /// Size of a page in points.
    pub fn page_size(&self, page: usize) -> Option<(f32, f32)> {
        self.sizes.get(page).copied()
    }

    /// Draws a page `width` pixels wide (white background).
    pub fn render(&self, page: usize, width: u32) -> Option<PagePicture> {
        let (w, h) = self.page_size(page)?;
        if !(w > 0.0 && h > 0.0) {
            return None;
        }
        let width = width.clamp(16, MAX_PIXELS);
        // A hair more than exact, so the rounded-down picture is `width` pixels wide.
        let scale = ((width as f32 + 0.25) / w).min(MAX_PIXELS as f32 / h);
        let pages = self.pdf.pages();
        let p = pages.get(page)?;
        let pixmap = render(
            p,
            &RenderCache::new(),
            &InterpreterSettings::default(),
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale: scale,
                y_scale: scale,
                bg_color: WHITE,
            },
        );
        Some(PagePicture {
            width: u32::from(pixmap.width()),
            height: u32::from(pixmap.height()),
            // Opaque white background: premultiplied and straight colours are the same.
            rgba: pixmap.data_as_u8_slice().to_vec(),
        })
    }

    pub(crate) fn pdf(&self) -> &Pdf {
        &self.pdf
    }
}
