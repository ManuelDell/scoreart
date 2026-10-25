//! ScoreArt's own colours (violet brand accent; light and dark).

use egui::Color32;

#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub accent: Color32,
    pub accent_soft: Color32,
    pub canvas: Color32,
    pub page_shadow: Color32,
    pub toolbar: Color32,
    pub panel: Color32,
    pub status: Color32,
    pub text_dim: Color32,
}

impl Tokens {
    pub fn get(ctx: &egui::Context) -> Tokens {
        if ctx.global_style().visuals.dark_mode {
            Tokens {
                accent: Color32::from_rgb(0x91, 0x71, 0xf8),
                accent_soft: Color32::from_rgb(0x3b, 0x2f, 0x6b),
                canvas: Color32::from_rgb(0x2a, 0x2a, 0x30),
                page_shadow: Color32::from_black_alpha(140),
                toolbar: Color32::from_rgb(0x24, 0x24, 0x2a),
                panel: Color32::from_rgb(0x20, 0x20, 0x26),
                status: Color32::from_rgb(0x1b, 0x1b, 0x20),
                text_dim: Color32::from_gray(150),
            }
        } else {
            Tokens {
                accent: Color32::from_rgb(0x70, 0x48, 0xe8),
                accent_soft: Color32::from_rgb(0xe5, 0xdb, 0xff),
                canvas: Color32::from_rgb(0xdf, 0xe2, 0xe8),
                page_shadow: Color32::from_black_alpha(45),
                toolbar: Color32::from_rgb(0xf8, 0xf8, 0xfa),
                panel: Color32::from_rgb(0xf3, 0xf3, 0xf6),
                status: Color32::from_rgb(0xea, 0xea, 0xef),
                text_dim: Color32::from_gray(110),
            }
        }
    }
}
