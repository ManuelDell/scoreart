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
    /// The ticked instruments of the open library.
    chosen: std::collections::BTreeSet<&'static str>,
    /// The licence confirmation is open for these instruments.
    confirm: Option<Vec<&'static str>>,
    accepted: bool,
}

/// Instruments waiting to be downloaded, one after the other.
#[derive(Default)]
pub struct DownloadQueue {
    pending: std::collections::VecDeque<(&'static Library, &'static str)>,
    /// How many the current batch has in all.
    total: usize,
}

/// Starts the next queued download when the host is free; keeps the frames coming meanwhile.
pub fn pump(app: &mut ScoreApp, ctx: &egui::Context) {
    let busy = app.host.download_status().is_some();
    if !busy {
        if let Some((lib, inst)) = app.downloads.pending.pop_front() {
            app.host.download_sounds(lib, inst);
            let n = app.downloads.total - app.downloads.pending.len();
            app.session.status = if app.downloads.total > 1 {
                tf(
                    "Downloading {} ({} of {}) …",
                    &[&catalog::instrument_name(inst), &n, &app.downloads.total],
                )
            } else {
                tf("Downloading {} …", &[&catalog::instrument_name(inst)])
            };
        } else {
            app.downloads.total = 0;
        }
    }
    if busy || !app.downloads.pending.is_empty() {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
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
                ui.add(egui::Hyperlink::from_label_and_url(l.name, l.url).open_in_new_tab(true));
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
                let total = app.downloads.total;
                let batch = if total > 1 {
                    tf("Instrument {} of {}", &[&(total - app.downloads.pending.len()), &total])
                } else {
                    String::new()
                };
                ui.horizontal(|ui| {
                    if !batch.is_empty() {
                        ui.label(batch);
                    }
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
                        app.downloads.pending.clear();
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
                            st.chosen.clear();
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
                    ui.add_space(4.0);
                    match lib.source {
                        Source::Website { url } => {
                            licence_box(ui, lib, &tk);
                            ui.add_space(6.0);
                            ui.add(egui::Hyperlink::from_label_and_url(t("Open the publisher's website to download"), url).open_in_new_tab(true));
                            ui.label(
                                RichText::new(t("After downloading, load the file with Mixer → Load sounds…")).color(tk.text_dim),
                            );
                        }
                        Source::Repository { page, .. } => {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(t("Source:"));
                                ui.add(egui::Hyperlink::from_label_and_url(page, page).open_in_new_tab(true));
                            });
                            instruments(app, ui, lib, &mut st, &tk, status.is_some());
                            ui.add_space(6.0);
                            // Shown again in full before anything is downloaded.
                            licence_box(ui, lib, &tk);
                        }
                    }
                    });
                    });
            });
        });
    if let (Some(insts), Some(lib)) = (st.confirm.clone(), libs.get(st.library)) {
        confirm(app, ctx, lib, &insts, &mut st, &tk);
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
    // What a download would actually fetch: obtainable and not loaded yet.
    let missing: Vec<&'static str> = lib
        .instruments
        .iter()
        .copied()
        .filter(|i| obtainable(app, lib, i) && !installed(app, lib, i))
        .collect();
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
    let shown: Vec<&'static str> = lib
        .instruments
        .iter()
        .copied()
        .filter(|i| f.is_empty() || i.to_lowercase().contains(&f))
        .collect();
    ui.horizontal(|ui| {
        let label = if f.is_empty() {
            t("Select all")
        } else {
            t("Select all found")
        };
        if ui
            .button(label)
            .on_hover_text(t("Ticks every instrument that is not loaded yet"))
            .clicked()
        {
            st.chosen
                .extend(shown.iter().copied().filter(|i| missing.contains(i)));
        }
        if ui
            .add_enabled(!st.chosen.is_empty(), egui::Button::new(t("Select none")))
            .clicked()
        {
            st.chosen.clear();
        }
    });
    let chosen: Vec<&'static str> = lib
        .instruments
        .iter()
        .copied()
        .filter(|i| st.chosen.contains(i) && obtainable(app, lib, i))
        .collect();
    ui.label(
        RichText::new(t(
            "Download one instrument with its ⤓ button, or tick several and download the selection.",
        ))
        .small()
        .color(tk.text_dim),
    );
    if !chosen.is_empty() {
        let label = tf("Download selected ({})…", &[&chosen.len()]);
        if ui
            .add_enabled(!busy, egui::Button::new(RichText::new(label).strong()))
            .clicked()
        {
            st.confirm = Some(chosen.clone());
            st.accepted = false;
        }
    }
    ui.add_space(2.0);
    egui::ScrollArea::vertical()
        .id_salt("catalog-instruments")
        .max_height(260.0)
        .show(ui, |ui| {
            ui.set_min_width(320.0);
            for inst in &shown {
                let done = installed(app, lib, inst);
                let mut label = if done {
                    format!("✓ {}", catalog::instrument_name(inst))
                } else {
                    catalog::instrument_name(inst).to_string()
                };
                if on_server(app, lib, inst) {
                    label.push_str(t(" (on the server)"));
                }
                let mut on = st.chosen.contains(inst);
                let r = ui
                    .horizontal(|ui| {
                        let get = ui
                            .add_enabled(
                                !busy && obtainable(app, lib, inst),
                                egui::Button::new("⤓").small(),
                            )
                            .on_hover_text(tf("Download only \"{}\"…", &[&catalog::instrument_name(inst)]));
                        if get.clicked() {
                            st.confirm = Some(vec![*inst]);
                            st.accepted = false;
                        }
                        ui.checkbox(&mut on, label)
                    })
                    .inner;
                let r = if inst.contains('/') {
                    r.on_hover_text(*inst)
                } else {
                    r
                };
                if r.changed() {
                    if on {
                        st.chosen.insert(inst);
                    } else {
                        st.chosen.remove(inst);
                    }
                }
            }
        });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        let label = tf("Download the whole library ({} instruments)…", &[&missing.len()]);
        if ui
            .add_enabled(
                !missing.is_empty() && !busy,
                egui::Button::new(RichText::new(label).small()),
            )
            .on_hover_text(tf("Everything not loaded yet · {}", &[&t(lib.size)]))
            .clicked()
        {
            st.confirm = Some(missing.clone());
            st.accepted = false;
        }
    });
    if chosen.iter().any(|i| installed(app, lib, i)) {
        ui.label(
            RichText::new(t(
                "Already loaded — downloading again only fetches missing files.",
            ))
            .color(tk.text_dim),
        );
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
    insts: &[&'static str],
    st: &mut CatalogState,
    tk: &Tokens,
) {
    let mut close = false;
    egui::Modal::new(egui::Id::unique("licence-confirm")).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.heading(tf("Licence of {}", &[&lib.name]));
        match insts {
            [one] => ui.label(tf(
                "You are about to download \"{}\" ({}).",
                &[&catalog::instrument_name(one), &lib.name],
            )),
            _ => {
                let r = ui.label(
                    RichText::new(tf(
                        "You are about to download {} instruments of {}, one after the other.",
                        &[&insts.len(), &lib.name],
                    ))
                    .strong(),
                );
                let names: Vec<&str> = insts
                    .iter()
                    .take(6)
                    .map(|i| catalog::instrument_name(i))
                    .collect();
                let more = insts.len().saturating_sub(names.len());
                let mut list = names.join(", ");
                if more > 0 {
                    list.push_str(&tf(" and {} more", &[&more]));
                }
                ui.label(RichText::new(list).color(tk.text_dim));
                if insts.len() == lib.instruments.len() || insts.len() > 20 {
                    ui.label(
                        RichText::new(tf("That is the whole library: {}.", &[&t(lib.size)]))
                            .color(ui.visuals().warn_fg_color),
                    );
                }
                r
            }
        };
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            licence_box(ui, lib, tk);
        });
        if let Source::Repository { page, .. } = lib.source {
            ui.horizontal_wrapped(|ui| {
                ui.label(t("Files come from:"));
                ui.add(egui::Hyperlink::from_label_and_url(page, page).open_in_new_tab(true));
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
                app.downloads.pending.extend(insts.iter().map(|i| (lib, *i)));
                app.downloads.total = app.downloads.pending.len();
                st.chosen.clear();
                pump(app, ctx);
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
