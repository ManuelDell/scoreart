//! Menu bar, toolbar, palette, properties, status bar and dialogs.

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use serde_json::json;

use scoreart_engine::Selection;
use scoreart_engrave::glyphs as g;
use scoreart_model::{Clef, INSTRUMENTS, KeySig, NoteValue};

use crate::{Dialog, ScoreApp, canvas::voice_color, glyph, theme::Tokens};

// ---------- menu bar ----------

pub fn menu_bar(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("menu_bar")
        .frame(
            egui::Frame::NONE
                .fill(t.toolbar)
                .inner_margin(egui::Margin::symmetric(8, 3)),
        )
        .show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.label(RichText::new("ScoreArt").strong().color(t.accent));
                ui.add_space(6.0);
                ui.menu_button("File", |ui| {
                    if !app.host.embedded() {
                        if ui.button("New score…    Ctrl+N").clicked() {
                            app.dialog = Some(Dialog::NewScore(NewScore::default()));
                            ui.close();
                        }
                        if ui.button("Open…    Ctrl+O").clicked() {
                            app.host.request_open();
                            ui.close();
                        }
                        if ui.button("Open sample (Ode to Joy)").clicked() {
                            app.session = scoreart_engine::Session::default();
                            app.fields = Fields::default();
                            ui.close();
                        }
                        ui.separator();
                    }
                    if ui.button("Save    Ctrl+S").clicked() {
                        app.save(false);
                        ui.close();
                    }
                    if !app.host.embedded() && ui.button("Save as…    Ctrl+Shift+S").clicked() {
                        app.save(true);
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button("Export", |ui| {
                        let stem = app
                            .session
                            .file_name
                            .rsplit_once('.')
                            .map(|(a, _)| a.to_string())
                            .unwrap_or_else(|| app.session.file_name.clone());
                        for (label, ext) in [
                            ("MusicXML (.musicxml)", "musicxml"),
                            ("Compressed MusicXML (.mxl)", "mxl"),
                            ("MIDI (.mid)", "mid"),
                            ("SVG, first page (.svg)", "svg"),
                            ("Audio (.wav)", "wav"),
                            ("Audio per instrument (.zip of .wav)", "zip"),
                        ] {
                            if ui.button(label).clicked() {
                                let name = format!("{stem}.{ext}");
                                let bytes = app.session.save_bytes(&name);
                                match bytes {
                                    Ok(b) => {
                                        if let Some(n) = app.host.save(&name, b, true) {
                                            app.session.status = format!("Exported {n}");
                                        }
                                    }
                                    Err(e) => app.session.status = e.to_string(),
                                }
                                ui.close();
                            }
                        }
                    });
                });
                ui.menu_button("Edit", |ui| {
                    for (label, id) in [
                        ("Undo    Ctrl+Z", "edit.undo"),
                        ("Redo    Ctrl+Y", "edit.redo"),
                        ("Delete    Del", "note.delete"),
                        ("Select all    Ctrl+A", "select.all"),
                        ("Note input    N", "input.toggle"),
                    ] {
                        let enabled = scoreart_engine::cmd::find(id)
                            .map(|c| (c.enabled)(&app.session).is_ok())
                            .unwrap_or(false);
                        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                            app.run(id, json!({}));
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button("Transpose up a semitone").clicked() {
                        app.run("score.transpose", json!({ "semitones": 1 }));
                        ui.close();
                    }
                    if ui.button("Transpose down a semitone").clicked() {
                        app.run("score.transpose", json!({ "semitones": -1 }));
                        ui.close();
                    }
                });
                ui.menu_button("Add", |ui| {
                    ui.menu_button("Measures", |ui| {
                        if ui.button("Append one measure    Ctrl+B").clicked() {
                            app.run("measure.append", json!({}));
                            ui.close();
                        }
                        if ui.button("Append four measures").clicked() {
                            app.run("measure.append", json!({ "count": 4 }));
                            ui.close();
                        }
                        if ui.button("Insert before selection    Ins").clicked() {
                            app.run("measure.insert", json!({}));
                            ui.close();
                        }
                        if ui.button("Delete selected measures    Ctrl+Del").clicked() {
                            app.run("measure.delete", json!({}));
                            ui.close();
                        }
                    });
                    ui.menu_button("Instrument", |ui| {
                        for inst in INSTRUMENTS {
                            if ui.button(inst.name).clicked() {
                                app.run("part.add", json!({ "instrument": inst.id }));
                                ui.close();
                            }
                        }
                    });
                    ui.menu_button("Text", |ui| {
                        if ui.button("Lyric…    Ctrl+L").clicked() {
                            open_lyric_dialog(app);
                            ui.close();
                        }
                        if ui.button("System text…").clicked() {
                            app.dialog = Some(Dialog::Text {
                                title: "System text",
                                command: "measure.text",
                                key: "text",
                                value: String::new(),
                            });
                            ui.close();
                        }
                        if ui.button("Tempo…").clicked() {
                            app.dialog = Some(Dialog::Text {
                                title: "Tempo (quarter notes per minute)",
                                command: "measure.tempo",
                                key: "bpm",
                                value: "100".into(),
                            });
                            ui.close();
                        }
                    });
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Zoom in    Ctrl+=").clicked() {
                        app.set_zoom(app.zoom * 1.15);
                    }
                    if ui.button("Zoom out    Ctrl+-").clicked() {
                        app.set_zoom(app.zoom / 1.15);
                    }
                    if ui.button("Zoom 100%    Ctrl+0").clicked() {
                        app.set_zoom(7.5);
                    }
                    ui.separator();
                    ui.checkbox(&mut app.show_palette, "Palettes");
                    ui.checkbox(&mut app.show_properties, "Properties");
                    ui.checkbox(&mut app.show_mixer, "Mixer    F10");
                    if ui.button("Free sound libraries…").clicked() {
                        app.open_catalog();
                        ui.close();
                    }
                    ui.checkbox(&mut app.sound_on_input, "Play notes while editing");
                    ui.separator();
                    let dark = ui.ctx().global_style().visuals.dark_mode;
                    if ui
                        .button(if dark { "Light theme" } else { "Dark theme" })
                        .clicked()
                    {
                        ui.ctx().set_theme(if dark {
                            egui::Theme::Light
                        } else {
                            egui::Theme::Dark
                        });
                        ui.close();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard shortcuts").clicked() {
                        app.dialog = Some(Dialog::Shortcuts);
                        ui.close();
                    }
                    if ui.button("About ScoreArt").clicked() {
                        app.dialog = Some(Dialog::About);
                        ui.close();
                    }
                });
            });
        });
}

// ---------- toolbar ----------

/// A square button showing a SMuFL glyph (or a text label), with a baseline at `base` (0–1 of
/// the button height) and the glyph size in px.
fn glyph_button(ui: &mut egui::Ui, label: GlyphLabel, selected: bool, tip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let size = Vec2::new(34.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = resp.on_hover_text(tip);
    let bg = if selected {
        t.accent_soft
    } else if resp.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 6.0, bg);
    if selected {
        ui.painter()
            .rect_stroke(rect, 6.0, Stroke::new(1.2, t.accent), egui::StrokeKind::Inside);
    }
    let fg = ui.visuals().text_color();
    match label {
        GlyphLabel::Glyph(c, em, base, dx) => {
            glyph::paint(
                ui.painter(),
                c,
                Pos2::new(rect.center().x + dx, rect.top() + rect.height() * base),
                em,
                fg,
            );
        }
        GlyphLabel::Text(s) => {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                s,
                egui::FontId::proportional(15.0),
                fg,
            );
        }
    }
    resp
}

#[derive(Clone, Copy)]
enum GlyphLabel {
    /// glyph, em px, baseline (fraction of height), x offset from centre
    Glyph(char, f32, f32, f32),
    Text(&'static str),
}

fn duration_glyph(v: NoteValue) -> char {
    match v {
        NoteValue::Breve => '\u{ECA0}',
        NoteValue::Whole => '\u{ECA2}',
        NoteValue::Half => '\u{ECA3}',
        NoteValue::Quarter => '\u{ECA5}',
        NoteValue::Eighth => '\u{ECA7}',
        NoteValue::N16th => '\u{ECA9}',
        NoteValue::N32nd => '\u{ECAB}',
        NoteValue::N64th => '\u{ECAD}',
    }
}

pub fn toolbar(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("toolbar")
        .frame(
            egui::Frame::NONE
                .fill(t.toolbar)
                .inner_margin(egui::Margin::symmetric(8, 4)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let input = app.session.input;
                // Playback.
                let playing = app.is_playing();
                let label = if playing {
                    GlyphLabel::Text("■")
                } else {
                    GlyphLabel::Text("▶")
                };
                let tip = if playing {
                    "Stop (Space)"
                } else {
                    "Play from the selection (Space)"
                };
                if glyph_button(ui, label, playing, tip).clicked() {
                    app.toggle_playback();
                }
                ui.separator();
                // Note input toggle: a pen.
                if glyph_button(ui, GlyphLabel::Text("✏"), input.active, "Note input (N)").clicked() {
                    app.run("input.toggle", json!({}));
                }
                ui.separator();
                for v in [
                    NoteValue::N64th,
                    NoteValue::N32nd,
                    NoteValue::N16th,
                    NoteValue::Eighth,
                    NoteValue::Quarter,
                    NoteValue::Half,
                    NoteValue::Whole,
                ] {
                    let selected = input.value == v && (input.active || app.session.selection.is_none());
                    let tip = format!("{} ({})", v.xml_name(), v.digit());
                    let label = GlyphLabel::Glyph(duration_glyph(v), 26.0, 0.72, -3.0);
                    if glyph_button(ui, label, selected, &tip).clicked() {
                        app.run("input.duration", json!({ "value": v.digit() }));
                    }
                }
                if glyph_button(
                    ui,
                    GlyphLabel::Glyph('\u{ECB7}', 30.0, 0.62, -1.0),
                    input.dots > 0,
                    "Augmentation dot (.)",
                )
                .clicked()
                {
                    app.run("input.dot", json!({}));
                }
                ui.separator();
                if glyph_button(
                    ui,
                    GlyphLabel::Glyph(g::REST_QUARTER, 26.0, 0.5, -3.0),
                    false,
                    "Rest (0)",
                )
                .clicked()
                {
                    app.run("note.rest", json!({}));
                }
                for (c, id, tip) in [
                    (g::DOUBLE_FLAT, "double-flat", "Double flat"),
                    (g::FLAT, "flat", "Flat"),
                    (g::NATURAL, "natural", "Natural"),
                    (g::SHARP, "sharp", "Sharp"),
                    (g::DOUBLE_SHARP, "double-sharp", "Double sharp"),
                ] {
                    if glyph_button(ui, GlyphLabel::Glyph(c, 26.0, 0.55, -4.0), false, tip).clicked() {
                        app.run("note.accidental", json!({ "type": id }));
                    }
                }
                if glyph_button(ui, GlyphLabel::Text("⌒"), false, "Tie (T)").clicked() {
                    app.run("note.tie", json!({}));
                }
                ui.separator();
                for v in 0..4usize {
                    let selected = input.voice == v;
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(26.0, 34.0), Sense::click());
                    let resp = resp.on_hover_text(format!("Voice {} (Ctrl+Alt+{})", v + 1, v + 1));
                    let c = voice_color(v);
                    let r = Rect::from_center_size(rect.center(), Vec2::new(22.0, 22.0));
                    ui.painter()
                        .rect_filled(r, 5.0, if selected { c } else { c.linear_multiply(0.18) });
                    ui.painter().text(
                        r.center(),
                        egui::Align2::CENTER_CENTER,
                        (v + 1).to_string(),
                        egui::FontId::proportional(13.0),
                        if selected { Color32::WHITE } else { c },
                    );
                    if resp.clicked() {
                        if !input.active && app.session.selected_event().is_some() {
                            app.run("note.voice", json!({ "voice": v + 1 }));
                        } else {
                            app.run("input.voice", json!({ "voice": v + 1 }));
                        }
                    }
                }
                ui.separator();
                if glyph_button(ui, GlyphLabel::Text("↶"), false, "Undo (Ctrl+Z)").clicked() {
                    app.run("edit.undo", json!({}));
                }
                if glyph_button(ui, GlyphLabel::Text("↷"), false, "Redo (Ctrl+Y)").clicked() {
                    app.run("edit.redo", json!({}));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if glyph_button(ui, GlyphLabel::Text("+"), false, "Zoom in").clicked() {
                        app.set_zoom(app.zoom * 1.15);
                    }
                    ui.label(format!("{:.0}%", app.zoom / 7.5 * 100.0));
                    if glyph_button(ui, GlyphLabel::Text("−"), false, "Zoom out").clicked() {
                        app.set_zoom(app.zoom / 1.15);
                    }
                });
            });
        });
}

// ---------- status bar ----------

pub fn status_bar(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("status_bar")
        .exact_size(26.0)
        .frame(
            egui::Frame::NONE
                .fill(t.status)
                .inner_margin(egui::Margin::symmetric(10, 0)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let s = &app.session;
                let mode = if s.input.active {
                    let dots = ".".repeat(usize::from(s.input.dots));
                    RichText::new(format!(
                        "● Note input · {}{dots} · Voice {}",
                        s.input.value.xml_name(),
                        s.input.voice + 1
                    ))
                    .color(voice_color(s.input.voice))
                } else {
                    RichText::new("Normal mode").color(t.text_dim)
                };
                ui.label(mode);
                ui.separator();
                let msg = if s.status.is_empty() {
                    s.describe_selection()
                } else {
                    s.status.clone()
                };
                ui.label(msg);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{}{}", s.file_name, if s.dirty { " •" } else { "" }))
                            .color(t.text_dim),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "{} measures · {} instruments",
                            s.score.measures.len(),
                            s.score.parts.len()
                        ))
                        .color(t.text_dim),
                    );
                });
            });
        });
}

// ---------- palette ----------

fn section(ui: &mut egui::Ui, title: &str, open: bool, add: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(RichText::new(title).strong())
        .default_open(open)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
                add(ui);
            });
        });
}

/// A palette cell showing a SMuFL glyph scaled and centred to fit.
fn smufl_button(ui: &mut egui::Ui, name: &str, tip: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 34.0), Sense::click());
    let resp = resp.on_hover_text(tip);
    let bg = if resp.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        t.toolbar
    };
    ui.painter().rect_filled(rect, 5.0, bg);
    if let Some((c, w, lo, hi)) = scoreart_engrave::smufl::get(name) {
        let h = (hi - lo).max(0.3);
        let span = w.max(h).max(0.6);
        let em = (4.0 * 24.0 / span).min(30.0);
        let sp = em / 4.0;
        let x = rect.center().x - w * sp / 2.0;
        let y = rect.center().y + (hi + lo) / 2.0 * sp;
        glyph::paint(ui.painter(), c, Pos2::new(x, y), em, ui.visuals().text_color());
    }
    resp
}

fn text_button(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>, tip: &str) -> egui::Response {
    ui.add_sized(Vec2::new(40.0, 26.0), egui::Button::new(label))
        .on_hover_text(tip)
}

/// A palette cell showing a dynamic in Bravura letters, sized to fit.
fn dynamic_button(ui: &mut egui::Ui, id: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(40.0, 30.0), Sense::click());
    let resp = resp.on_hover_text(id);
    let bg = if resp.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        t.toolbar
    };
    ui.painter().rect_filled(rect, 5.0, bg);
    let letters: Vec<(char, f32)> = id.chars().filter_map(g::dynamic_letter).collect();
    let width: f32 = letters.iter().map(|l| l.1 - 0.1).sum::<f32>().max(0.5);
    // Fit the word into the cell: at most 30 px em, at most 34 px wide.
    let em = (4.0 * 34.0 / width).min(30.0);
    let sp = em / 4.0;
    let mut x = rect.center().x - width * sp / 2.0;
    let y = rect.center().y + 0.35 * sp;
    for (c, w) in letters {
        glyph::paint(ui.painter(), c, Pos2::new(x, y), em, ui.visuals().text_color());
        x += (w - 0.1) * sp;
    }
    resp
}

pub fn palette(app: &mut ScoreApp, ui: &mut egui::Ui) {
    use scoreart_model::marks::{self, MARKS};
    let t = Tokens::get(ui.ctx());
    egui::Panel::left("palette")
        .default_size(232.0)
        .resizable(true)
        .frame(egui::Frame::NONE.fill(t.panel).inner_margin(8))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(RichText::new("Palettes").size(15.0).strong());
                ui.label(
                    RichText::new(
                        "Select notes or measures, then click. Shift+click or Shift+→ selects a range.",
                    )
                    .small()
                    .color(t.text_dim),
                );
                ui.add_space(4.0);
                section(ui, "Clefs", false, |ui| {
                    for c in Clef::ALL {
                        let name = match c {
                            Clef::Treble => "gClef",
                            Clef::Treble8vb => "gClef8vb",
                            Clef::Bass => "fClef",
                            Clef::Alto | Clef::Tenor => "cClef",
                            Clef::Percussion => "unpitchedPercussionClef1",
                        };
                        let (gc, _, _) = g::clef(c);
                        let resp = if scoreart_engrave::smufl::get(name).is_some() {
                            smufl_button(ui, name, c.label())
                        } else {
                            let base = match c {
                                Clef::Bass => 0.45,
                                Clef::Alto | Clef::Tenor | Clef::Percussion => 0.5,
                                _ => 0.62,
                            };
                            glyph_button(ui, GlyphLabel::Glyph(gc, 15.0, base, -5.0), false, c.label())
                        };
                        if resp.clicked() {
                            app.run("measure.clef", json!({ "clef": c.id() }));
                        }
                    }
                });
                section(ui, "Key signatures", false, |ui| {
                    for f in -7..=7i32 {
                        let k = KeySig::new(f);
                        let tip = format!("{} major / {} accidentals", k.name(), f.abs());
                        if text_button(ui, RichText::new(key_label(f)).size(12.0), &tip).clicked() {
                            app.run("measure.key", json!({ "fifths": f }));
                        }
                    }
                });
                section(ui, "Time signatures", false, |ui| {
                    for ts in [
                        "2/4", "3/4", "4/4", "5/4", "6/8", "3/8", "9/8", "12/8", "2/2", "7/8", "common",
                        "cut",
                    ] {
                        let label = match ts {
                            "common" => "C".to_string(),
                            "cut" => "¢".to_string(),
                            x => x.to_string(),
                        };
                        if text_button(ui, label, ts).clicked() {
                            app.run("measure.time", json!({ "time": ts }));
                        }
                    }
                });
                for (group, open) in [
                    (marks::ARTICULATIONS, true),
                    (marks::FERMATAS, true),
                    (marks::BREATHS, false),
                    (marks::ORNAMENTS, true),
                    (marks::TECHNIQUES, false),
                    (marks::SLIDES, false),
                ] {
                    section(ui, group, open, |ui| {
                        for d in MARKS.iter().filter(|d| d.group == group) {
                            if smufl_button(ui, d.glyph_above, d.label).clicked() {
                                app.run("note.articulation", json!({ "type": d.id }));
                            }
                        }
                    });
                }
                section(ui, "Dynamics", true, |ui| {
                    for d in scoreart_model::Dynamic::all() {
                        if dynamic_button(ui, d.id()).clicked() {
                            app.run("note.dynamic", json!({ "type": d.id() }));
                        }
                    }
                    if text_button(ui, "✕", "Remove dynamic").clicked() {
                        app.run("note.dynamic", json!({ "type": "none" }));
                    }
                });
                section(ui, "Lines", true, |ui| {
                    for k in scoreart_model::SpannerKind::ALL {
                        let (resp, _) = match k {
                            scoreart_model::SpannerKind::Slur => (text_button(ui, "⌒", k.label()), ()),
                            scoreart_model::SpannerKind::Crescendo => (text_button(ui, "<", k.label()), ()),
                            scoreart_model::SpannerKind::Diminuendo => (text_button(ui, ">", k.label()), ()),
                            scoreart_model::SpannerKind::CrescendoLine => {
                                (text_button(ui, RichText::new("cresc.").italics(), k.label()), ())
                            }
                            scoreart_model::SpannerKind::DiminuendoLine => {
                                (text_button(ui, RichText::new("dim.").italics(), k.label()), ())
                            }
                            scoreart_model::SpannerKind::Ottava(o) => {
                                let name = match o {
                                    1 => "ottavaAlta",
                                    -1 => "ottavaBassaVb",
                                    2 => "quindicesimaAlta",
                                    -2 => "quindicesimaBassa",
                                    3 => "ventiduesimaAlta",
                                    _ => "ventiduesimaBassa",
                                };
                                (smufl_button(ui, name, k.label()), ())
                            }
                            scoreart_model::SpannerKind::Pedal => {
                                (smufl_button(ui, "keyboardPedalPed", k.label()), ())
                            }
                            scoreart_model::SpannerKind::TrillLine => (
                                text_button(ui, RichText::new("tr~~").italics().strong(), k.label()),
                                (),
                            ),
                            scoreart_model::SpannerKind::Vibrato => {
                                (smufl_button(ui, "wiggleVibrato", k.label()), ())
                            }
                            scoreart_model::SpannerKind::LetRing => {
                                (text_button(ui, RichText::new("l.r.").italics(), k.label()), ())
                            }
                            scoreart_model::SpannerKind::PalmMute => {
                                (text_button(ui, RichText::new("P.M.").italics(), k.label()), ())
                            }
                            scoreart_model::SpannerKind::Glissando => (text_button(ui, "╲", k.label()), ()),
                            scoreart_model::SpannerKind::WavyGlissando => {
                                (text_button(ui, "≀", k.label()), ())
                            }
                        };
                        if resp.clicked() {
                            app.run("line.add", json!({ "type": k.id() }));
                        }
                    }
                    if text_button(ui, "✕", "Remove lines starting or ending at the selection").clicked() {
                        app.run("line.remove", json!({}));
                    }
                });
                section(ui, "Arpeggios & tremolos", false, |ui| {
                    for (label, id, tip) in [
                        ("≀", "arpeggio", "Arpeggio"),
                        ("≀↑", "arpeggio-up", "Arpeggio up"),
                        ("≀↓", "arpeggio-down", "Arpeggio down"),
                        ("[", "non-arpeggio", "Non-arpeggio bracket"),
                    ] {
                        if text_button(ui, label, tip).clicked() {
                            app.run("note.arpeggio", json!({ "type": id }));
                        }
                    }
                    for n in 1..=4 {
                        if smufl_button(ui, &format!("tremolo{n}"), &format!("Tremolo, {n} stroke(s)"))
                            .clicked()
                        {
                            app.run("note.tremolo", json!({ "strokes": n }));
                        }
                    }
                    if smufl_button(ui, "buzzRoll", "Buzz roll").clicked() {
                        app.run("note.tremolo", json!({ "strokes": "buzz" }));
                    }
                });
                section(ui, "Grace notes", false, |ui| {
                    for (label, id) in [
                        ("♪̸", "acciaccatura"),
                        ("♪", "appoggiatura"),
                        ("𝅘𝅥𝅯", "grace-16th"),
                        ("𝅘𝅥𝅰", "grace-32nd"),
                    ] {
                        if text_button(ui, label, id).clicked() {
                            app.run("note.grace", json!({ "type": id }));
                        }
                    }
                    if text_button(ui, "↑", "Move grace note up").clicked() {
                        app.run("note.grace-pitch", json!({ "delta": 1 }));
                    }
                    if text_button(ui, "↓", "Move grace note down").clicked() {
                        app.run("note.grace-pitch", json!({ "delta": -1 }));
                    }
                    if text_button(ui, "✕", "Remove grace notes").clicked() {
                        app.run("note.grace-remove", json!({}));
                    }
                });
                section(ui, "Noteheads", false, |ui| {
                    for h in scoreart_model::NoteHead::ALL {
                        let (c, _) = g::head_for(h, NoteValue::Quarter);
                        let name = scoreart_engrave::smufl::TABLE
                            .iter()
                            .find(|x| x.1 == c)
                            .map(|x| x.0)
                            .unwrap_or("noteheadBlack");
                        let resp = if name == "noteheadBlack" {
                            glyph_button(ui, GlyphLabel::Glyph(c, 30.0, 0.55, -4.0), false, h.id())
                        } else {
                            smufl_button(ui, name, h.id())
                        };
                        if resp.clicked() {
                            app.run("note.head", json!({ "type": h.id() }));
                        }
                    }
                    if text_button(ui, "( )", "Parentheses (ghost note)").clicked() {
                        app.run("note.paren", json!({}));
                    }
                });
                section(ui, "Repeats & jumps", false, |ui| {
                    if text_button(ui, "𝄆", "Start repeat").clicked() {
                        app.run("measure.repeat-start", json!({}));
                    }
                    if text_button(ui, "𝄇", "End repeat").clicked() {
                        app.run("measure.barline", json!({ "type": "repeat-end" }));
                    }
                    for n in 1..=3 {
                        if text_button(ui, format!("⌐{n}."), &format!("Volta {n}")).clicked() {
                            app.run("measure.volta", json!({ "number": n }));
                        }
                    }
                    if text_button(ui, "⌐✕", "Remove volta").clicked() {
                        app.run("measure.volta", json!({ "number": 0 }));
                    }
                    if smufl_button(ui, "repeat1Bar", "One-measure repeat").clicked() {
                        app.run("measure.repeat-sign", json!({}));
                    }
                    for j in scoreart_model::Jump::ALL {
                        let resp = match j {
                            scoreart_model::Jump::Segno => smufl_button(ui, "segno", "Segno"),
                            scoreart_model::Jump::Coda => smufl_button(ui, "coda", "Coda"),
                            scoreart_model::Jump::VariedCoda => smufl_button(ui, "codaSquare", "Varied coda"),
                            _ => ui
                                .add_sized(
                                    Vec2::new(84.0, 26.0),
                                    egui::Button::new(RichText::new(j.text().unwrap_or("")).italics()),
                                )
                                .on_hover_text(j.id()),
                        };
                        if resp.clicked() {
                            app.run("measure.jump", json!({ "type": j.id() }));
                        }
                    }
                });
                section(ui, "Barlines", false, |ui| {
                    for (label, id) in [("‖", "double"), ("𝄂", "final"), ("|", "regular")] {
                        if text_button(ui, label, id).clicked() {
                            app.run("measure.barline", json!({ "type": id }));
                        }
                    }
                });
                section(ui, "Text", false, |ui| {
                    let mut dlg = |ui: &mut egui::Ui,
                                   label: &str,
                                   title: &'static str,
                                   command: &'static str,
                                   key: &'static str,
                                   value: String| {
                        if ui
                            .add_sized(Vec2::new(98.0, 26.0), egui::Button::new(label))
                            .clicked()
                        {
                            app.dialog = Some(Dialog::Text {
                                title,
                                command,
                                key,
                                value,
                            });
                        }
                    };
                    dlg(
                        ui,
                        "Staff text…",
                        "Staff text",
                        "note.text",
                        "text",
                        String::new(),
                    );
                    dlg(
                        ui,
                        "Chord symbol…",
                        "Chord symbol (e.g. Cmaj7, F#m7/C#)",
                        "note.chord-symbol",
                        "text",
                        String::new(),
                    );
                    dlg(
                        ui,
                        "Tempo marking…",
                        "Tempo marking (e.g. Allegro)",
                        "measure.tempo-text",
                        "text",
                        "Allegro".into(),
                    );
                    dlg(
                        ui,
                        "System text…",
                        "System text",
                        "measure.text",
                        "text",
                        String::new(),
                    );
                    dlg(
                        ui,
                        "Fingering…",
                        "Fingering",
                        "note.fingering",
                        "text",
                        String::new(),
                    );
                    if ui
                        .add_sized(Vec2::new(98.0, 26.0), egui::Button::new("Rehearsal mark"))
                        .clicked()
                    {
                        app.run("measure.rehearsal", json!({}));
                    }
                    for f in ["1", "2", "3", "4", "5"] {
                        if ui
                            .add_sized(Vec2::new(26.0, 26.0), egui::Button::new(f))
                            .on_hover_text("Fingering")
                            .clicked()
                        {
                            app.run("note.fingering", json!({ "text": f }));
                        }
                    }
                });
                section(ui, "Tempo", false, |ui| {
                    for bpm in [60, 72, 80, 92, 100, 108, 120, 132, 144] {
                        if text_button(ui, bpm.to_string(), &format!("♩ = {bpm}")).clicked() {
                            app.run("measure.tempo", json!({ "bpm": bpm }));
                        }
                    }
                });
            });
        });
}

fn key_label(f: i32) -> String {
    match f {
        0 => "♮".into(),
        f if f > 0 => format!("{f}♯"),
        f => format!("{}♭", -f),
    }
}

// ---------- properties ----------

/// Editable text buffers for the properties panel, refreshed when the score changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fields {
    revision: u64,
    sel: Option<Selection>,
    title: String,
    subtitle: String,
    composer: String,
    lyricist: String,
    lyric: String,
    part_name: String,
}

pub fn properties(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    // Refresh buffers when the score or selection changed.
    if app.fields.revision != app.session.revision || app.fields.sel != app.session.selection {
        let m = &app.session.score.meta;
        let (part, _, _) = app.session.focus();
        let lyric = app
            .session
            .selected_event()
            .and_then(|e| app.session.score.event(e))
            .and_then(|e| e.lyric.clone())
            .unwrap_or_default();
        app.fields = Fields {
            revision: app.session.revision,
            sel: app.session.selection,
            title: m.title.clone(),
            subtitle: m.subtitle.clone(),
            composer: m.composer.clone(),
            lyricist: m.lyricist.clone(),
            lyric,
            part_name: app
                .session
                .score
                .parts
                .get(part)
                .map(|p| p.name.clone())
                .unwrap_or_default(),
        };
    }
    egui::Panel::right("properties").default_size(240.0).resizable(true).frame(egui::Frame::NONE.fill(t.panel).inner_margin(10)).show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(RichText::new("Properties").size(15.0).strong());
            ui.add_space(6.0);
            egui::CollapsingHeader::new(RichText::new("Score").strong()).default_open(true).show(ui, |ui| {
                let mut changed = false;
                egui::Grid::new("meta").num_columns(2).spacing([6.0, 6.0]).show(ui, |ui| {
                    for (label, field) in [
                        ("Title", &mut app.fields.title),
                        ("Subtitle", &mut app.fields.subtitle),
                        ("Composer", &mut app.fields.composer),
                        ("Lyricist", &mut app.fields.lyricist),
                    ] {
                        ui.label(label);
                        let r = ui.add(egui::TextEdit::singleline(field).desired_width(150.0));
                        changed |= r.lost_focus();
                        ui.end_row();
                    }
                });
                if changed {
                    let f = app.fields.clone();
                    app.run("score.meta", json!({ "title": f.title, "subtitle": f.subtitle, "composer": f.composer, "lyricist": f.lyricist }));
                }
            });
            let sel = app.session.selection;
            egui::CollapsingHeader::new(RichText::new("Selection").strong()).default_open(true).show(ui, |ui| {
                match sel {
                    None => {
                        ui.label(RichText::new("Nothing selected. Click a note, a rest or an empty part of a measure.").color(t.text_dim));
                    }
                    Some(_) => {
                        ui.label(app.session.describe_selection());
                    }
                }
                if let Some(ev) = app.session.selected_event()
                    && app.session.score.event(ev).is_some_and(|e| !e.is_rest())
                {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Lyric");
                        let r = ui.add(egui::TextEdit::singleline(&mut app.fields.lyric).desired_width(120.0));
                        if r.lost_focus() {
                            let text = app.fields.lyric.clone();
                            app.run("note.lyric", json!({ "text": text }));
                            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                app.run("select.next", json!({}));
                                app.fields.revision = 0;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Voice");
                        for v in 1..=4 {
                            if ui.selectable_label(ev.voice + 1 == v, v.to_string()).clicked() {
                                app.run("note.voice", json!({ "voice": v }));
                            }
                        }
                    });
                }
            });
            egui::CollapsingHeader::new(RichText::new("Instrument").strong()).default_open(false).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    let r = ui.add(egui::TextEdit::singleline(&mut app.fields.part_name).desired_width(130.0));
                    if r.lost_focus() {
                        let n = app.fields.part_name.clone();
                        app.run("part.rename", json!({ "name": n }));
                    }
                });
                if ui.button("Remove this instrument").clicked() {
                    app.run("part.remove", json!({}));
                }
            });
        });
    });
}

pub fn open_lyric_dialog(app: &mut ScoreApp) {
    let current = app
        .session
        .selected_event()
        .and_then(|e| app.session.score.event(e))
        .and_then(|e| e.lyric.clone())
        .unwrap_or_default();
    if app.session.selected_event().is_some() {
        app.dialog = Some(Dialog::Text {
            title: "Lyric",
            command: "note.lyric",
            key: "text",
            value: current,
        });
    } else {
        app.session.status = "Select a note first".into();
    }
}

// ---------- dialogs ----------

#[derive(Clone, Debug, PartialEq)]
pub struct NewScore {
    title: String,
    composer: String,
    instruments: Vec<&'static str>,
    time: &'static str,
    key: i32,
    measures: u32,
    tempo: u32,
}

impl Default for NewScore {
    fn default() -> Self {
        NewScore {
            title: "Untitled score".into(),
            composer: String::new(),
            instruments: vec!["piano"],
            time: "4/4",
            key: 0,
            measures: 16,
            tempo: 100,
        }
    }
}

pub fn dialogs(app: &mut ScoreApp, ctx: &egui::Context) {
    let Some(dialog) = app.dialog.clone() else { return };
    let mut close = false;
    match dialog {
        Dialog::NewScore(mut ns) => {
            let resp = egui::Modal::new(egui::Id::unique("new_score")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.heading("New score");
                ui.add_space(6.0);
                egui::Grid::new("ns")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Title");
                        ui.text_edit_singleline(&mut ns.title);
                        ui.end_row();
                        ui.label("Composer");
                        ui.text_edit_singleline(&mut ns.composer);
                        ui.end_row();
                        ui.label("Time");
                        egui::ComboBox::from_id_salt("ns_time")
                            .selected_text(ns.time)
                            .show_ui(ui, |ui| {
                                for t in [
                                    "2/4", "3/4", "4/4", "5/4", "6/8", "3/8", "9/8", "12/8", "2/2", "common",
                                    "cut",
                                ] {
                                    ui.selectable_value(&mut ns.time, t, t);
                                }
                            });
                        ui.end_row();
                        ui.label("Key");
                        egui::ComboBox::from_id_salt("ns_key")
                            .selected_text(format!("{} major", KeySig::new(ns.key).name()))
                            .show_ui(ui, |ui| {
                                for f in -7..=7 {
                                    ui.selectable_value(
                                        &mut ns.key,
                                        f,
                                        format!("{} major ({})", KeySig::new(f).name(), key_label(f)),
                                    );
                                }
                            });
                        ui.end_row();
                        ui.label("Measures");
                        ui.add(egui::DragValue::new(&mut ns.measures).range(1..=999));
                        ui.end_row();
                        ui.label("Tempo ♩=");
                        ui.add(egui::DragValue::new(&mut ns.tempo).range(20..=300));
                        ui.end_row();
                    });
                ui.add_space(6.0);
                ui.label(RichText::new("Instruments").strong());
                ui.horizontal_wrapped(|ui| {
                    for inst in INSTRUMENTS {
                        let mut on = ns.instruments.contains(&inst.id);
                        if ui.checkbox(&mut on, inst.name).changed() {
                            if on {
                                ns.instruments.push(inst.id);
                            } else {
                                ns.instruments.retain(|i| *i != inst.id);
                            }
                        }
                    }
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let ok = ui
                        .add_enabled(!ns.instruments.is_empty(), egui::Button::new("Create"))
                        .clicked();
                    let cancel = ui.button("Cancel").clicked();
                    (ok, cancel)
                })
                .inner
            });
            let (ok, cancel) = resp.inner;
            if ok {
                app.run(
                    "file.new",
                    json!({ "title": ns.title, "composer": ns.composer, "instruments": ns.instruments, "time": ns.time,
                            "key": ns.key, "measures": ns.measures, "tempo": ns.tempo }),
                );
                app.fields = Fields::default();
                close = true;
            } else if cancel || resp.should_close() {
                close = true;
            } else {
                app.dialog = Some(Dialog::NewScore(ns));
            }
        }
        Dialog::Text {
            title,
            command,
            key,
            mut value,
        } => {
            let resp = egui::Modal::new(egui::Id::unique("text_dialog")).show(ctx, |ui| {
                ui.set_width(320.0);
                ui.heading(title);
                let r = ui.text_edit_singleline(&mut value);
                r.request_focus();
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| (ui.button("OK").clicked() || enter, ui.button("Cancel").clicked()))
                    .inner
            });
            let (ok, cancel) = resp.inner;
            if ok {
                let v = if key == "bpm" {
                    json!({ key: value.trim().parse::<i64>().unwrap_or(0) })
                } else {
                    json!({ key: value })
                };
                app.run(command, v);
                close = true;
            } else if cancel || resp.should_close() {
                close = true;
            } else {
                app.dialog = Some(Dialog::Text {
                    title,
                    command,
                    key,
                    value,
                });
            }
        }
        Dialog::Shortcuts => {
            let resp = egui::Modal::new(egui::Id::unique("shortcuts")).show(ctx, |ui| {
                ui.set_width(460.0);
                ui.heading("Keyboard shortcuts");
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    egui::Grid::new("sc").striped(true).num_columns(2).show(ui, |ui| {
                        for c in scoreart_engine::registry() {
                            if let Some(s) = c.shortcut {
                                ui.label(RichText::new(s).monospace());
                                ui.label(c.label);
                                ui.end_row();
                            }
                        }
                    });
                });
                ui.button("Close").clicked()
            });
            close = resp.inner || resp.should_close();
        }
        Dialog::About => {
            let resp = egui::Modal::new(egui::Id::unique("about")).show(ctx, |ui| {
                ui.set_width(440.0);
                ui.heading("ScoreArt");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label("An open-source music notation editor written in Rust.");
                ui.add_space(6.0);
                ui.label("Licensed MIT OR Apache-2.0. Music font: Bravura by Steinberg Media Technologies GmbH, SIL Open Font License 1.1.");
                ui.label("MusicXML and SMuFL are specifications of the W3C Music Notation Community Group.");
                ui.add_space(6.0);
                ui.label(RichText::new("ScoreArt is independent and not affiliated with MuseScore / Muse Group or ArtCraft.").small());
                ui.button("Close").clicked()
            });
            close = resp.inner || resp.should_close();
        }
    }
    if close {
        app.dialog = None;
    }
}
