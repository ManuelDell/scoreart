//! A PDF next to the score: the question what to do with an opened PDF (show it, or convert its
//! music into a score), and the panel on the right that shows its pages.

use std::sync::Arc;

use egui::{Color32, RichText, Vec2};

use crate::ScoreApp;
use crate::i18n::{t, tf};
use crate::theme::Tokens;

/// The PDF panel's state: a PDF waiting for the user's choice, and the pages drawn so far.
#[derive(Default)]
pub(crate) struct RefView {
    /// A PDF the user opened, until they pick "show" or "convert".
    pub pending: Option<(String, Vec<u8>)>,
    /// Whether the panel is open (when there is a PDF).
    pub show: bool,
    /// Drawn pages of the PDF they were drawn from (its address), and their width in pixels.
    pages: Vec<Option<egui::TextureHandle>>,
    of: usize,
    width: u32,
}

/// Remembers an opened PDF and asks what to do with it.
pub(crate) fn opened(app: &mut ScoreApp, name: &str, bytes: &[u8]) {
    app.reference.pending = Some((name.to_string(), bytes.to_vec()));
}

/// The question "show or convert" for a just-opened PDF.
pub(crate) fn dialog(app: &mut ScoreApp, ctx: &egui::Context) {
    let Some((name, _)) = &app.reference.pending else {
        return;
    };
    let name = name.clone();
    let mut choice: Option<Option<bool>> = None; // Some(None) = cancel, Some(Some(convert))
    let resp = egui::Modal::new(egui::Id::unique("pdf-open")).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.heading(t("Open PDF"));
        ui.add_space(4.0);
        ui.label(RichText::new(&name).strong());
        ui.add_space(6.0);
        ui.label(t(
            "Show the PDF next to your score, for example to copy it, or let ScoreArt read its music and turn it into a score you can edit.",
        ));
        ui.add_space(4.0);
        let tk = Tokens::get(ui.ctx());
        ui.label(
            RichText::new(t(
                "Converting works with PDFs that notation programs wrote (MuseScore, Sibelius, Finale, Dorico, LilyPond, ScoreArt). Scanned pages cannot be converted yet. Always check the result against the PDF.",
            ))
            .color(tk.text_dim)
            .size(12.0),
        );
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(t("Show next to the score")).clicked() {
                choice = Some(Some(false));
            }
            if ui.button(t("Convert to a score")).clicked() {
                choice = Some(Some(true));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t("Cancel")).clicked() {
                    choice = Some(None);
                }
            });
        });
    });
    if resp.should_close() && choice.is_none() {
        choice = Some(None);
    }
    let Some(choice) = choice else { return };
    let Some((name, bytes)) = app.reference.pending.take() else {
        return;
    };
    let Some(convert) = choice else { return };
    if convert {
        app.stop_playback();
    }
    match app.session.open_pdf(&name, &bytes, convert) {
        Ok(()) => {
            app.reference.show = true;
            if convert {
                app.fields = crate::panels::Fields::default();
                app.scroll_to_cursor = true;
            }
        }
        Err(e) => app.session.status = e.to_string(),
    }
}

/// Toggles the PDF panel (when a PDF is open).
pub(crate) fn toggle(app: &mut ScoreApp) {
    if app.session.reference.is_some() {
        app.reference.show = !app.reference.show;
    }
}

/// The panel on the right with the PDF's pages.
pub(crate) fn panel(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let Some(r) = app.session.reference.clone() else {
        app.reference.pages.clear();
        return;
    };
    if !app.reference.show {
        return;
    }
    let tk = Tokens::get(ui.ctx());
    egui::Panel::right("pdf-panel")
        .default_size(420.0)
        .size_range(220.0..=1000.0)
        .resizable(true)
        .frame(egui::Frame::NONE.fill(tk.panel).inner_margin(8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&r.name).size(14.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").on_hover_text(t("Close the PDF")).clicked() {
                        app.session.reference = None;
                        app.reference.show = false;
                    }
                    if ui.small_button("–").on_hover_text(t("Hide")).clicked() {
                        app.reference.show = false;
                    }
                });
            });
            if !r.uncertain.is_empty() {
                let list: Vec<String> = r.uncertain.iter().take(20).map(|m| (m + 1).to_string()).collect();
                let more = if r.uncertain.len() > 20 { " …" } else { "" };
                ui.label(
                    RichText::new(tf("Check measure {}", &[&format!("{}{more}", list.join(", "))]))
                        .color(Color32::from_rgb(0xD9, 0x7A, 0x1A))
                        .size(12.0),
                );
            }
            ui.add_space(4.0);
            pages(app, ui, &r);
        });
}

fn pages(app: &mut ScoreApp, ui: &mut egui::Ui, r: &Arc<scoreart_engine::Reference>) {
    let view = &mut app.reference;
    let id = Arc::as_ptr(r) as usize;
    let n = r.doc.page_count();
    let ppp = ui.ctx().pixels_per_point();
    let avail = (ui.available_width() - 12.0).max(100.0);
    let want = ((avail * ppp) as u32).clamp(200, 2400);
    // Redraw all pages for another PDF, or when the panel became much wider or narrower.
    if view.of != id
        || view.pages.len() != n
        || (want as f32 - view.width as f32).abs() > 0.3 * view.width as f32
    {
        view.pages = vec![None; n];
        view.of = id;
        view.width = want;
    }
    let mut drawn_one = false;
    egui::ScrollArea::vertical()
        .id_salt("pdf-pages")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for i in 0..n {
                let Some((w, h)) = r.doc.page_size(i) else {
                    continue;
                };
                let size = Vec2::new(avail, avail * h / w.max(1.0));
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                let visible = ui.is_rect_visible(rect);
                if visible
                    && !drawn_one
                    && view.pages.get(i).is_some_and(|p| p.is_none())
                    && let Some(pic) = r.doc.render(i, view.width)
                {
                    drawn_one = true;
                    let img = egui::ColorImage::from_rgba_unmultiplied(
                        [pic.width as usize, pic.height as usize],
                        &pic.rgba,
                    );
                    let tex =
                        ui.ctx()
                            .load_texture(format!("pdf-page-{i}"), img, egui::TextureOptions::LINEAR);
                    if let Some(slot) = view.pages.get_mut(i) {
                        *slot = Some(tex);
                    }
                }
                let p = ui.painter();
                match view.pages.get(i).and_then(|t| t.as_ref()) {
                    Some(tex) => {
                        p.image(
                            tex.id(),
                            rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    None => {
                        p.rect_filled(rect, 0.0, Color32::WHITE);
                        if visible {
                            ui.ctx().request_repaint();
                        }
                    }
                }
                p.rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(1.0, Tokens::get(ui.ctx()).page_shadow),
                    egui::StrokeKind::Outside,
                );
                ui.add_space(8.0);
            }
        });
}
