//! The side panel: a strip of tabs on the left edge (palettes, instruments, properties,
//! sounds) that switches what the panel next to it shows. Clicking the open tab again folds
//! the panel away.

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::ScoreApp;
use crate::i18n::t;
use crate::theme::Tokens;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideTab {
    Palettes,
    Instruments,
    Properties,
    Sounds,
}

impl SideTab {
    pub const ALL: [SideTab; 4] = [
        SideTab::Palettes,
        SideTab::Instruments,
        SideTab::Properties,
        SideTab::Sounds,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SideTab::Palettes => t("Palettes"),
            SideTab::Instruments => t("Instruments"),
            SideTab::Properties => t("Properties"),
            SideTab::Sounds => t("Sounds"),
        }
    }
}

/// Opens `tab`, or folds the panel when it is already open.
pub fn toggle(app: &mut ScoreApp, tab: SideTab) {
    app.side = if app.side == Some(tab) { None } else { Some(tab) };
}

fn paint_icon(p: &egui::Painter, c: Pos2, tab: SideTab, color: Color32) {
    let at = |x: f32, y: f32| Pos2::new(c.x + x, c.y + y);
    let stroke = Stroke::new(1.6, color);
    match tab {
        SideTab::Palettes => {
            for (x, y) in [(-6.0, -6.0), (2.0, -6.0), (-6.0, 2.0), (2.0, 2.0)] {
                p.rect_stroke(
                    Rect::from_min_size(at(x - 1.0, y - 1.0), Vec2::splat(6.0)),
                    1.5,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        SideTab::Instruments => {
            // A treble clef from the music font.
            crate::glyph::paint(p, '\u{E050}', at(-4.5, 5.0), 22.0, color);
        }
        SideTab::Properties => {
            for (y, knob) in [(-6.0, 3.0), (0.0, -3.0), (6.0, 1.0)] {
                p.line_segment([at(-8.0, y), at(8.0, y)], stroke);
                p.circle_filled(at(knob, y), 2.6, color);
            }
        }
        SideTab::Sounds => {
            p.add(egui::Shape::convex_polygon(
                vec![
                    at(-8.0, -3.0),
                    at(-4.0, -3.0),
                    at(1.0, -8.0),
                    at(1.0, 8.0),
                    at(-4.0, 3.0),
                    at(-8.0, 3.0),
                ],
                color,
                Stroke::NONE,
            ));
            for r in [5.0f32, 9.0] {
                let pts: Vec<Pos2> = (0..=10)
                    .map(|i| {
                        let a = -0.8 + 1.6 * i as f32 / 10.0;
                        at(1.5 + r * a.cos(), r * a.sin())
                    })
                    .collect();
                p.add(egui::Shape::line(pts, stroke));
            }
        }
    }
}

/// The tab strip on the far left.
pub fn strip(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let tk = Tokens::get(ui.ctx());
    egui::Panel::left("side-tabs")
        .exact_size(64.0)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(tk.toolbar)
                .inner_margin(egui::Margin::symmetric(4, 8)),
        )
        .show(ui, |ui| {
            for tab in SideTab::ALL {
                let on = app.side == Some(tab);
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(56.0, 52.0), Sense::click());
                let resp = resp.on_hover_text(tab.label());
                if on || resp.hovered() {
                    ui.painter().rect_filled(
                        rect,
                        6.0,
                        if on {
                            tk.accent_soft
                        } else {
                            ui.visuals().widgets.hovered.weak_bg_fill
                        },
                    );
                }
                if on {
                    let bar =
                        Rect::from_min_size(rect.left_top() + Vec2::new(-4.0, 8.0), Vec2::new(3.0, 36.0));
                    ui.painter().rect_filled(bar, 1.5, tk.accent);
                }
                let color = if on { tk.accent } else { ui.visuals().text_color() };
                paint_icon(ui.painter(), rect.center() - Vec2::new(0.0, 8.0), tab, color);
                ui.painter().text(
                    rect.center() + Vec2::new(0.0, 14.0),
                    egui::Align2::CENTER_CENTER,
                    tab.label(),
                    egui::FontId::proportional(10.0),
                    color,
                );
                if resp.clicked() {
                    toggle(app, tab);
                }
                ui.add_space(4.0);
            }
        });
}

/// The panel showing the open tab.
pub fn panel(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let Some(tab) = app.side else { return };
    let tk = Tokens::get(ui.ctx());
    egui::Panel::left("side-panel")
        .default_size(270.0)
        .size_range(200.0..=520.0)
        .resizable(true)
        .frame(egui::Frame::NONE.fill(tk.panel).inner_margin(10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tab.label()).size(15.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").on_hover_text(t("Close")).clicked() {
                        app.side = None;
                    }
                });
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .id_salt(("side-scroll", tab as u8))
                .auto_shrink([false, false])
                .show(ui, |ui| match tab {
                    SideTab::Palettes => crate::panels::palette_content(app, ui),
                    SideTab::Instruments => crate::instruments::content(app, ui),
                    SideTab::Properties => crate::panels::properties_content(app, ui),
                    SideTab::Sounds => sounds(app, ui),
                });
        });
}

/// Sound libraries, the mixer, MIDI input.
fn sounds(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let tk = Tokens::get(ui.ctx());
    ui.label(RichText::new(t("Loaded sound libraries")).strong());
    let names: Vec<String> = app.session.sounds.iter().map(|b| b.name.clone()).collect();
    if names.is_empty() {
        ui.label(RichText::new(t("built-in only")).color(tk.text_dim));
    }
    for name in names {
        ui.horizontal(|ui| {
            ui.label(&name);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("✕")
                    .on_hover_text(t("Remove this library"))
                    .clicked()
                {
                    app.remove_sounds(&name);
                }
            });
        });
    }
    ui.add_space(6.0);
    let full = Vec2::new(ui.available_width().min(240.0), 24.0);
    if ui
        .add_sized(full, egui::Button::new(t("Load sounds…")))
        .on_hover_text(t(
            "A SoundFont (.sf2), an SFZ instrument (.sfz) or a zip of SFZ instruments",
        ))
        .clicked()
    {
        app.host.request_sounds(false);
    }
    if app.host.can_load_folders()
        && ui
            .add_sized(full, egui::Button::new(t("Load SFZ folder…")))
            .on_hover_text(t("A folder with .sfz instruments and their samples"))
            .clicked()
    {
        app.host.request_sounds(true);
    }
    if ui
        .add_sized(full, egui::Button::new(t("Get free sounds…")))
        .on_hover_text(t("Free libraries (orchestra, piano …) with their licences"))
        .clicked()
    {
        app.open_catalog();
    }
    ui.add_space(10.0);
    ui.label(RichText::new(t("Playback")).strong());
    ui.checkbox(&mut app.show_mixer, t("Mixer    F10"));
    ui.checkbox(&mut app.sound_on_input, t("Play notes while editing"));
    ui.add_space(10.0);
    ui.label(RichText::new(t("MIDI input")).strong());
    let device = app.host.midi_connected();
    ui.label(RichText::new(device.unwrap_or_else(|| t("Not connected").to_string())).color(tk.text_dim));
    if ui
        .add_sized(full, egui::Button::new(t("MIDI input…")))
        .on_hover_text(t("Connect a MIDI keyboard and set what each key does"))
        .clicked()
    {
        app.open_midi();
    }
}
