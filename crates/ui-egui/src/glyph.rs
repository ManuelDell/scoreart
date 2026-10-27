//! Drawing SMuFL glyphs with egui: the Bravura font is registered as the `bravura` family and
//! glyphs are placed by their baseline, as the engraver expects.

use std::sync::Arc;

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Painter, Pos2};

pub const FAMILY: &str = "bravura";

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        FAMILY.into(),
        Arc::new(FontData::from_static(scoreart_render::BRAVURA)),
    );
    fonts
        .families
        .insert(FontFamily::Name(FAMILY.into()), vec![FAMILY.into()]);
    // Bravura also has the Unicode music symbols (♭ ♯ 𝄆 𝅘𝅥 …) used in labels and palettes:
    // last in line, after egui's own fonts, so text never shows empty boxes for them.
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push(FAMILY.into());
    }
    ctx.set_fonts(fonts);
}

pub fn font(size_px: f32) -> FontId {
    FontId::new(size_px.clamp(1.0, 2000.0), FontFamily::Name(FAMILY.into()))
}

/// Paints `c` with its baseline-left origin at `origin`; `em_px` is the font size (4 sp).
pub fn paint(painter: &Painter, c: char, origin: Pos2, em_px: f32, color: Color32) {
    if !(em_px.is_finite() && em_px > 0.5) {
        return;
    }
    let galley = painter.layout_no_wrap(c.to_string(), font(em_px), color);
    let (dx, baseline) = galley
        .rows
        .first()
        .and_then(|r| {
            r.row
                .glyphs
                .first()
                .map(|g| (r.pos.x + g.pos.x, r.pos.y + g.pos.y))
        })
        .unwrap_or((0.0, em_px * 0.8));
    painter.galley(Pos2::new(origin.x - dx, origin.y - baseline), galley, color);
}
