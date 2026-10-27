//! The mixer: one channel strip per instrument with its sound, volume, pan, mute, solo and a
//! level meter, plus the list of loaded sound libraries.

use std::sync::Arc;

use egui::{RichText, Vec2};
use scoreart_model::PartSound;
use scoreart_synth::{MixControls, SoundBank};
use serde_json::json;

use crate::ScoreApp;
use crate::theme::Tokens;

/// Mixer state kept by the app between frames.
#[derive(Default)]
pub struct Mixer {
    /// Shared with the playing synth.
    pub controls: Arc<MixControls>,
    /// Score revision the volumes/pans were last copied from.
    synced: u64,
    /// Meter levels shown (decaying).
    meters: Vec<f32>,
    /// Preset filter text per part.
    filter: Vec<String>,
}

impl Mixer {
    /// Keeps the controls matching the score: one track per part, volume and pan from the
    /// score after any change (undo included); mute and solo survive while parts stay.
    pub fn sync(&mut self, score: &scoreart_model::Score, revision: u64) {
        if self.controls.len() != score.parts.len() {
            let old = self.controls.clone();
            let fresh = MixControls::from_score(score);
            for i in 0..fresh.len().min(old.len()) {
                fresh.set_mute(i, old.mute(i));
                fresh.set_solo(i, old.solo(i));
            }
            self.controls = Arc::new(fresh);
            self.synced = revision;
        } else if self.synced != revision {
            for (i, p) in score.parts.iter().enumerate() {
                self.controls.set_volume(i, f32::from(p.sound.volume) / 100.0);
                self.controls.set_pan(i, f32::from(p.sound.pan) / 100.0);
            }
            self.synced = revision;
        }
        self.meters.resize(score.parts.len(), 0.0);
        self.filter.resize(score.parts.len(), String::new());
    }
}

impl ScoreApp {
    /// A newly loaded sound library: replaces one with the same name.
    pub fn add_sounds(&mut self, bank: SoundBank) {
        let n = bank.presets().len();
        let name = bank.name.clone();
        self.session.sounds.retain(|b| b.name != name);
        self.session.sounds.push(Arc::new(bank));
        self.session.status = format!("Loaded sounds \"{name}\" ({n} presets)");
    }

    fn remove_sounds(&mut self, name: &str) {
        self.session.sounds.retain(|b| b.name != name);
        self.host.forget_sounds(name);
        self.session.status = format!("Removed sounds \"{name}\"");
    }
}

fn sound_label(sound: &PartSound) -> String {
    if sound.library.is_empty() {
        "Automatic".into()
    } else if sound.library == PartSound::BUILT_IN {
        "Built-in".into()
    } else {
        // The preset has its own list below.
        sound.library.clone()
    }
}

pub fn panel(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    app.mixer.sync(&app.session.score, app.session.revision);
    let playing = app.is_playing();
    egui::Panel::bottom("mixer")
        .resizable(true)
        .default_size(250.0)
        .frame(egui::Frame::NONE.fill(t.panel).inner_margin(10))
        .show(ui, |ui| {
            // Libraries.
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Mixer").strong());
                ui.separator();
                ui.label(RichText::new("Sounds:").color(t.text_dim));
                let names: Vec<String> = app.session.sounds.iter().map(|b| b.name.clone()).collect();
                if names.is_empty() {
                    ui.label(RichText::new("built-in only").color(t.text_dim));
                }
                for name in names {
                    ui.label(&name);
                    if ui
                        .small_button("✕")
                        .on_hover_text("Remove this library")
                        .clicked()
                    {
                        app.remove_sounds(&name);
                    }
                }
                if ui
                    .button("Load sounds…")
                    .on_hover_text("A SoundFont (.sf2), an SFZ instrument (.sfz) or a zip of SFZ instruments")
                    .clicked()
                {
                    app.host.request_sounds(false);
                }
                if app.host.can_load_folders()
                    && ui
                        .button("Load SFZ folder…")
                        .on_hover_text("A folder with .sfz instruments and their samples")
                        .clicked()
                {
                    app.host.request_sounds(true);
                }
            });
            ui.add_space(6.0);
            let parts: Vec<(String, PartSound)> = app
                .session
                .score
                .parts
                .iter()
                .map(|p| (p.name.clone(), p.sound.clone()))
                .collect();
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    for (pi, (name, sound)) in parts.iter().enumerate() {
                        strip(app, ui, pi, name, sound, playing, &t);
                        ui.separator();
                    }
                });
            });
        });
    if playing {
        ui.ctx().request_repaint();
    }
}

fn strip(
    app: &mut ScoreApp,
    ui: &mut egui::Ui,
    pi: usize,
    name: &str,
    sound: &PartSound,
    playing: bool,
    t: &Tokens,
) {
    let controls = app.mixer.controls.clone();
    ui.vertical(|ui| {
        ui.set_width(150.0);
        ui.label(RichText::new(name).strong());
        // Sound: library, then preset.
        let mut choice: Option<(String, String)> = None;
        egui::ComboBox::from_id_salt(("mix-lib", pi))
            .width(140.0)
            .selected_text(sound_label(sound))
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(sound.library.is_empty(), "Automatic")
                    .clicked()
                {
                    choice = Some((String::new(), String::new()));
                }
                if ui
                    .selectable_label(sound.library == PartSound::BUILT_IN, "Built-in (ScoreArt)")
                    .clicked()
                {
                    choice = Some((PartSound::BUILT_IN.into(), String::new()));
                }
                for b in &app.session.sounds {
                    if ui.selectable_label(sound.library == b.name, &b.name).clicked() {
                        choice = Some((b.name.clone(), String::new()));
                    }
                }
            });
        let bank = app
            .session
            .sounds
            .iter()
            .find(|b| b.name == sound.library)
            .cloned();
        if let Some(bank) = bank {
            let presets = bank.presets();
            let filter = app
                .mixer
                .filter
                .get(pi)
                .cloned()
                .unwrap_or_default()
                .to_lowercase();
            egui::ComboBox::from_id_salt(("mix-preset", pi))
                .width(140.0)
                .height(320.0)
                .selected_text(if sound.preset.is_empty() {
                    "(by instrument)".to_string()
                } else {
                    sound
                        .preset
                        .rsplit('/')
                        .next()
                        .unwrap_or(&sound.preset)
                        .to_string()
                })
                .show_ui(ui, |ui| {
                    if presets.len() > 12
                        && let Some(f) = app.mixer.filter.get_mut(pi)
                    {
                        // Typing goes to the search, never to the score.
                        let r = ui.add(egui::TextEdit::singleline(f).hint_text("Search…"));
                        if !r.has_focus() {
                            r.request_focus();
                        }
                    }
                    for p in presets
                        .iter()
                        .filter(|p| filter.is_empty() || p.name.to_lowercase().contains(&filter))
                    {
                        let label = if bank.is_soundfont() {
                            format!("{:03}:{:03} {}", p.bank, p.program, p.name)
                        } else {
                            p.name.clone()
                        };
                        if ui.selectable_label(sound.preset == p.name, label).clicked() {
                            choice = Some((bank.name.clone(), p.name.clone()));
                        }
                    }
                });
        }
        if let Some((library, preset)) = choice {
            if playing {
                app.stop_playback();
            }
            app.run(
                "part.sound",
                json!({ "part": pi, "library": library, "preset": preset }),
            );
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            // Volume fader and meter.
            ui.spacing_mut().slider_width = 110.0;
            let mut vol = (controls.volume(pi) * 100.0).round();
            let r = ui.add(
                egui::Slider::new(&mut vol, 0.0..=100.0)
                    .vertical()
                    .show_value(false)
                    .text(""),
            );
            if r.changed() {
                controls.set_volume(pi, vol / 100.0);
            }
            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                app.run("part.mix", json!({ "part": pi, "volume": vol as i64 }));
            }
            let peak = controls.take_peak(pi);
            if let Some(m) = app.mixer.meters.get_mut(pi) {
                *m = if playing {
                    m.mul_add(0.85, 0.0).max(peak)
                } else {
                    0.0
                };
            }
            let level = app.mixer.meters.get(pi).copied().unwrap_or(0.0).min(1.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(8.0, 110.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, t.canvas);
            let mut fill = rect;
            fill.set_top(rect.bottom() - rect.height() * level.sqrt());
            let color = if level > 0.9 {
                egui::Color32::from_rgb(220, 70, 60)
            } else {
                t.accent
            };
            ui.painter().rect_filled(fill, 2.0, color);
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("{vol:.0}")).color(t.text_dim));
                let mute = controls.mute(pi);
                if ui
                    .add(egui::Button::new("M").selected(mute))
                    .on_hover_text("Mute")
                    .clicked()
                {
                    controls.set_mute(pi, !mute);
                }
                let solo = controls.solo(pi);
                if ui
                    .add(egui::Button::new("S").selected(solo))
                    .on_hover_text("Solo")
                    .clicked()
                {
                    controls.set_solo(pi, !solo);
                }
            });
        });
        // Pan.
        ui.spacing_mut().slider_width = 100.0;
        let mut pan = (controls.pan(pi) * 100.0).round();
        let r = ui
            .add(
                egui::Slider::new(&mut pan, -100.0..=100.0)
                    .show_value(false)
                    .text("Pan"),
            )
            .on_hover_text("Left … right (double-click: centre)");
        if r.double_clicked() {
            controls.set_pan(pi, 0.0);
            app.run("part.mix", json!({ "part": pi, "pan": 0 }));
        } else {
            if r.changed() {
                controls.set_pan(pi, pan / 100.0);
            }
            if r.drag_stopped() || (r.changed() && !r.dragged()) {
                app.run("part.mix", json!({ "part": pi, "pan": pan as i64 }));
            }
        }
    });
}
