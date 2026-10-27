//! "Free sound libraries": the catalogue of libraries ScoreArt can fetch. Every download first
//! shows the library's licence in full and needs the user's explicit acceptance.

use egui::RichText;
use scoreart_synth::catalog::{self, Library, Source};

use crate::ScoreApp;
use crate::theme::Tokens;

use crate::i18n::{t, tf};

/// A download in progress, as the host reports it.
#[derive(Clone, Debug, Default)]
pub struct DownloadStatus {
    pub library: String,
    pub instrument: String,
    pub progress: catalog::Progress,
}

#[derive(Default)]
pub struct CatalogState {
    library: usize,
    filter: String,
    instrument: Option<&'static str>,
    /// The licence confirmation is open for this instrument.
    confirm: Option<&'static str>,
    accepted: bool,
}

/// Whether `instrument` of `lib` is already loaded.
fn installed(app: &ScoreApp, lib: &Library, instrument: &str) -> bool {
    let preset = instrument.trim_end_matches(".sfz").trim_end_matches(".SFZ");
    app.session
        .sounds
        .iter()
        .filter(|b| b.name == lib.name)
        .any(|b| b.presets().iter().any(|p| p.name == preset))
}

fn licence_box(ui: &mut egui::Ui, lib: &Library, tk: &Tokens) {
    let l = &lib.licence;
    egui::Frame::NONE
        .fill(tk.canvas)
        .inner_margin(8)
        .corner_radius(4)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(t("Licence:")).strong());
                ui.hyperlink_to(l.name, l.url);
            });
            ui.label(RichText::new(t("The licence allows you to")).strong());
            for a in l.allows {
                ui.label(format!("• {}", t(a)));
            }
            ui.label(RichText::new(t("The licence requires you to")).strong());
            for r in l.requires {
                ui.label(format!("• {}", t(r)));
            }
            ui.label(RichText::new(t("Please note")).strong());
            for n in l.notes {
                ui.label(RichText::new(format!("• {}", t(n))).color(tk.text_dim));
            }
            ui.label(
                RichText::new(t(
                    "• ScoreArt does not host, sell or change these files. They come directly from the publisher's source; ScoreArt's own licence does not cover them.",
                ))
                .color(tk.text_dim),
            );
        });
}

pub fn window(app: &mut ScoreApp, ctx: &egui::Context) {
    let Some(mut st) = app.catalog.take() else { return };
    let tk = Tokens::get(ctx);
    let mut open = true;
    let status = app.host.download_status();
    let libs: Vec<&'static Library> = catalog::libraries()
        .iter()
        .filter(|l| app.host.library_allowed(l.name))
        .collect();
    let offered = app.host.host_sounds();
    egui::Window::new(t("Free sound libraries"))
        .open(&mut open)
        .default_width(780.0)
        .pivot(egui::Align2::CENTER_TOP)
        .default_pos(egui::pos2(ctx.content_rect().center().x, 40.0))
        .collapsible(false)
        .show(ctx, |ui| {
            ui.label(t(
                "ScoreArt can fetch these free libraries for you. None of them is part of ScoreArt: an instrument is downloaded only after you have read and accepted its licence, directly from the publisher's public source. The licence and a record of your acceptance are saved next to the files.",
            ));
            if !offered.is_empty() {
                ui.separator();
                ui.label(RichText::new(t("Provided by your administrator")).strong());
                ui.horizontal_wrapped(|ui| {
                    for name in &offered {
                        let loaded = app.session.sounds.iter().any(|b| &b.name == name);
                        let label = if loaded {
                            format!("✓ {name}")
                        } else {
                            tf("Load \"{}\"", &[name])
                        };
                        if ui.add_enabled(!loaded, egui::Button::new(label)).clicked() {
                            app.host.load_host_sounds(name);
                            app.session.status = tf("Loading {} …", &[name]);
                        }
                    }
                });
                ui.separator();
            }
            if !app.host.internet_downloads() {
                ui.label(
                    RichText::new(t(
                        "Your administrator has turned off downloads from the internet; only instruments kept on the server can be loaded.",
                    ))
                    .color(tk.text_dim),
                );
            } else if !app.host.can_download() {
                ui.label(
                    RichText::new(t("Downloading is not available here; use the source links and \"Load sounds…\"."))
                        .color(tk.text_dim),
                );
            }
            if let Some(s) = &status {
                let p = &s.progress;
                let frac = if p.files_total > 0 {
                    p.files_done as f32 / p.files_total as f32
                } else {
                    0.0
                };
                ui.horizontal(|ui| {
                    ui.add(
                        egui::ProgressBar::new(frac)
                            .desired_width(380.0)
                            .text(tf(
                                "{} · {}: {}/{} files, {} MB",
                                &[
                                    &s.library,
                                    &catalog::instrument_name(&s.instrument),
                                    &p.files_done,
                                    &p.files_total,
                                    &format!("{:.1}", p.bytes as f64 / 1e6),
                                ],
                            )),
                    );
                    if ui.button(t("Cancel")).clicked() {
                        app.host.cancel_download();
                    }
                });
                ctx.request_repaint();
            }
            ui.separator();
            ui.horizontal_top(|ui| {
                // Libraries.
                ui.vertical(|ui| {
                    ui.set_width(190.0);
                    for (i, l) in libs.iter().enumerate() {
                        let text = format!("{}\n{}", l.name, t(short_licence(l)));
                        if ui.selectable_label(st.library == i, text).clicked() {
                            st.library = i;
                            st.instrument = None;
                            st.confirm = None;
                            st.filter.clear();
                        }
                    }
                });
                ui.separator();
                let Some(lib) = libs.get(st.library) else { return };
                let height = (ctx.content_rect().height() - 280.0).clamp(220.0, 640.0);
                egui::ScrollArea::vertical()
                    .id_salt("catalog-details")
                    .max_height(height)
                    .min_scrolled_height(height)
                    .show(ui, |ui| {
                    ui.vertical(|ui| {
                    ui.heading(lib.name);
                    ui.label(RichText::new(lib.publisher).color(tk.text_dim));
                    ui.label(t(lib.description));
                    ui.label(tf("Size: {}", &[&t(lib.size)]));
                    licence_box(ui, lib, &tk);
                    ui.add_space(6.0);
                    match lib.source {
                        Source::Website { url } => {
                            ui.hyperlink_to(t("Open the publisher's website to download"), url);
                            ui.label(
                                RichText::new(t("After downloading, load the file with Mixer → Load sounds…")).color(tk.text_dim),
                            );
                        }
                        Source::Repository { page, .. } => {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(t("Source:"));
                                ui.hyperlink_to(page, page);
                            });
                            instruments(app, ui, lib, &mut st, &tk, status.is_some());
                        }
                    }
                    });
                    });
            });
        });
    if let (Some(inst), Some(lib)) = (st.confirm, libs.get(st.library)) {
        confirm(app, ctx, lib, inst, &mut st, &tk);
    }
    if open {
        app.catalog = Some(st);
    }
}

fn short_licence(l: &Library) -> &'static str {
    match l.id {
        "vsco2ce" | "vcsl" => t("CC0 · public domain"),
        "salamander" => t("CC BY 3.0 · credit the author"),
        _ => t("free licence · website download"),
    }
}

fn instruments(
    app: &mut ScoreApp,
    ui: &mut egui::Ui,
    lib: &'static Library,
    st: &mut CatalogState,
    tk: &Tokens,
    busy: bool,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(tf("Instruments ({})", &[&lib.instruments.len()])).strong());
        if lib.instruments.len() > 8 {
            ui.add(
                egui::TextEdit::singleline(&mut st.filter)
                    .hint_text(t("Search…"))
                    .desired_width(180.0),
            );
        }
    });
    let f = st.filter.to_lowercase();
    egui::ScrollArea::vertical()
        .id_salt("catalog-instruments")
        .max_height(170.0)
        .show(ui, |ui| {
            ui.set_min_width(320.0);
            for inst in lib
                .instruments
                .iter()
                .filter(|i| f.is_empty() || i.to_lowercase().contains(&f))
            {
                let done = installed(app, lib, inst);
                let mut label = if done {
                    format!("✓ {}", catalog::instrument_name(inst))
                } else {
                    catalog::instrument_name(inst).to_string()
                };
                if on_server(app, lib, inst) {
                    label.push_str(t(" (on the server)"));
                }
                let r = ui.selectable_label(st.instrument == Some(*inst), label);
                let r = if inst.contains('/') {
                    r.on_hover_text(*inst)
                } else {
                    r
                };
                if r.clicked() {
                    st.instrument = Some(inst);
                }
            }
        });
    ui.add_space(4.0);
    if let Some(inst) = st.instrument {
        let can = obtainable(app, lib, inst) && !busy;
        let label = tf("Download \"{}\"…", &[&catalog::instrument_name(inst)]);
        if ui.add_enabled(can, egui::Button::new(label)).clicked() {
            st.confirm = Some(inst);
            st.accepted = false;
        }
        if installed(app, lib, inst) {
            ui.label(
                RichText::new(t(
                    "Already loaded — downloading again only fetches missing files.",
                ))
                .color(tk.text_dim),
            );
        }
    }
}

/// Whether the host keeps a copy of this instrument.
fn on_server(app: &ScoreApp, lib: &Library, inst: &str) -> bool {
    app.host
        .provided_instruments(lib.name)
        .is_some_and(|l| l.iter().any(|i| i == inst))
}

/// Whether this instrument can be fetched: from the host's copy, or from the internet when that
/// is allowed.
fn obtainable(app: &ScoreApp, lib: &Library, inst: &str) -> bool {
    app.host.can_download() && (on_server(app, lib, inst) || app.host.internet_downloads())
}

/// The licence confirmation: the whole licence statement and an explicit tick before anything
/// is fetched.
fn confirm(
    app: &mut ScoreApp,
    ctx: &egui::Context,
    lib: &'static Library,
    inst: &'static str,
    st: &mut CatalogState,
    tk: &Tokens,
) {
    let mut close = false;
    egui::Modal::new(egui::Id::unique("licence-confirm")).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.heading(tf("Licence of {}", &[&lib.name]));
        ui.label(tf(
            "You are about to download \"{}\" ({}).",
            &[&catalog::instrument_name(inst), &lib.name],
        ));
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            licence_box(ui, lib, tk);
        });
        if let Source::Repository { page, .. } = lib.source {
            ui.horizontal_wrapped(|ui| {
                ui.label(t("Files come from:"));
                ui.hyperlink_to(page, page);
            });
        }
        ui.checkbox(
            &mut st.accepted,
            tf(
                "I have read the licence ({}) and accept its terms for the files I download.",
                &[&lib.licence.name],
            ),
        );
        ui.horizontal(|ui| {
            if ui
                .add_enabled(st.accepted, egui::Button::new(t("Accept and download")))
                .clicked()
            {
                app.host.download_sounds(lib, inst);
                app.session.status = tf("Downloading {} …", &[&catalog::instrument_name(inst)]);
                close = true;
            }
            if ui.button(t("Cancel")).clicked() {
                close = true;
            }
        });
    });
    if close {
        st.confirm = None;
        st.accepted = false;
    }
}
