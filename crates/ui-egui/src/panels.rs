//! Menu bar, toolbar, palette, properties, status bar and dialogs.

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use serde_json::json;

use scoreart_engine::Selection;
use scoreart_engrave::glyphs as g;
use scoreart_model::{Clef, INSTRUMENTS, KeySig, NoteValue};

use crate::{Dialog, ScoreApp, canvas::voice_color, glyph, theme::Tokens};

use crate::i18n::{t, tf};

// ---------- menu bar ----------

pub fn menu_bar(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let tk = Tokens::get(ui.ctx());
    egui::Panel::top("menu_bar")
        .frame(
            egui::Frame::NONE
                .fill(tk.toolbar)
                .inner_margin(egui::Margin::symmetric(8, 3)),
        )
        .show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.label(RichText::new("ScoreArt").strong().color(tk.accent));
                ui.add_space(6.0);
                ui.menu_button(t("File"), |ui| {
                    if !app.host.embedded() {
                        if ui.button(t("New score…    Ctrl+N")).clicked() {
                            app.dialog = Some(Dialog::NewScore(NewScore::default()));
                            ui.close();
                        }
                        if ui.button(t("Open…    Ctrl+O")).clicked() {
                            app.host.request_open();
                            ui.close();
                        }
                        if ui.button(t("Open sample (Ode to Joy)")).clicked() {
                            app.session = scoreart_engine::Session::default();
                            app.fields = Fields::default();
                            ui.close();
                        }
                        ui.separator();
                    }
                    if ui.button(t("Save    Ctrl+S")).clicked() {
                        app.save(false);
                        ui.close();
                    }
                    if !app.host.embedded() && ui.button(t("Save as…    Ctrl+Shift+S")).clicked() {
                        app.save(true);
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button(t("Export"), |ui| {
                        for (label, ext) in export_formats() {
                            if ui.button(label).clicked() {
                                export(app, ext);
                                ui.close();
                            }
                        }
                    });
                });
                ui.menu_button(t("Edit"), |ui| {
                    for (label, id) in [
                        (t("Undo    Ctrl+Z"), "edit.undo"),
                        (t("Redo    Ctrl+Y"), "edit.redo"),
                        (t("Delete    Del"), "note.delete"),
                        (t("Select all    Ctrl+A"), "select.all"),
                        (t("Note input    N"), "input.toggle"),
                    ] {
                        let enabled = scoreart_engine::cmd::find(id)
                            .map(|c| (c.enabled)(&app.session).is_ok())
                            .unwrap_or(false);
                        if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                            app.run(id, json!({}));
                            ui.close();
                        }
                    }
                    if ui.button(t("MIDI input device…")).clicked() {
                        app.open_midi();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(t("Transpose up a semitone")).clicked() {
                        app.run("score.transpose", json!({ "semitones": 1 }));
                        ui.close();
                    }
                    if ui.button(t("Transpose down a semitone")).clicked() {
                        app.run("score.transpose", json!({ "semitones": -1 }));
                        ui.close();
                    }
                });
                ui.menu_button(t("Add"), |ui| {
                    ui.menu_button(t("Measures"), |ui| {
                        if ui.button(t("Append one measure    Ctrl+B")).clicked() {
                            app.run("measure.append", json!({}));
                            ui.close();
                        }
                        if ui.button(t("Append four measures")).clicked() {
                            app.run("measure.append", json!({ "count": 4 }));
                            ui.close();
                        }
                        if ui.button(t("Insert before selection    Ins")).clicked() {
                            app.run("measure.insert", json!({}));
                            ui.close();
                        }
                        if ui.button(t("Delete selected measures    Ctrl+Del")).clicked() {
                            app.run("measure.delete", json!({}));
                            ui.close();
                        }
                        ui.separator();
                        if ui.button(t("Line break after the measure (toggle)")).clicked() {
                            app.run("measure.break", json!({ "kind": "line" }));
                            ui.close();
                        }
                        if ui.button(t("Page break after the measure (toggle)")).clicked() {
                            app.run("measure.break", json!({ "kind": "page" }));
                            ui.close();
                        }
                    });
                    ui.menu_button(t("Instrument"), |ui| {
                        for inst in INSTRUMENTS {
                            if ui.button(t(inst.name)).clicked() {
                                app.run("part.add", json!({ "instrument": inst.id, "name": t(inst.name) }));
                                ui.close();
                            }
                        }
                    });
                    ui.menu_button(t("Text"), |ui| {
                        if ui.button(t("Lyric…    Ctrl+L")).clicked() {
                            open_lyric_dialog(app);
                            ui.close();
                        }
                        if ui.button(t("System text…")).clicked() {
                            app.dialog = Some(Dialog::Text {
                                title: t("System text"),
                                command: "measure.text",
                                key: "text",
                                value: String::new(),
                            });
                            ui.close();
                        }
                        if ui.button(t("Tempo…")).clicked() {
                            open_tempo(app);
                            ui.close();
                        }
                    });
                });
                ui.menu_button(t("Instruments"), |ui| {
                    if ui.button(t("Instruments and sounds…    I")).clicked() {
                        app.open_instruments();
                        ui.close();
                    }
                    ui.menu_button(t("Add instrument"), |ui| {
                        for inst in INSTRUMENTS {
                            if ui.button(t(inst.name)).clicked() {
                                app.run("part.add", json!({ "instrument": inst.id, "name": t(inst.name) }));
                                ui.close();
                            }
                        }
                    });
                    ui.checkbox(&mut app.show_mixer, t("Mixer    F10"));
                    if ui.button(t("MIDI input device…")).clicked() {
                        app.open_midi();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(t("Load sounds…")).clicked() {
                        app.host.request_sounds(false);
                        ui.close();
                    }
                    if ui.button(t("Free sound libraries…")).clicked() {
                        app.open_catalog();
                        ui.close();
                    }
                });
                ui.menu_button(t("Format"), |ui| {
                    if ui.button(t("Page settings…")).clicked() {
                        crate::page::open(app);
                        ui.close();
                    }
                    if ui.button(t("Fonts…")).clicked() {
                        crate::texts::open(app, scoreart_model::TextKind::Title);
                        ui.close();
                    }
                    if ui.button(t("Header & footer…")).clicked() {
                        crate::texts::open_header_footer(app);
                        ui.close();
                    }
                    ui.menu_button(t("Add text"), |ui| {
                        for k in [
                            scoreart_model::TextKind::Heading,
                            scoreart_model::TextKind::Text,
                            scoreart_model::TextKind::Header,
                            scoreart_model::TextKind::Footer,
                        ] {
                            if ui.button(crate::texts::kind_label(k)).clicked() {
                                crate::texts::add_block(app, k);
                                ui.close();
                            }
                        }
                    });
                    ui.menu_button(t("Note spacing"), |ui| {
                        for (label, f) in [
                            (t("Tight"), 0.8),
                            (t("Normal"), 1.0),
                            (t("Wide"), 1.25),
                            (t("Very wide"), 1.5),
                        ] {
                            if ui.button(label).clicked() {
                                app.run("style.spacing", json!({ "factor": f }));
                                ui.close();
                            }
                        }
                    });
                    if ui.button(t("Reset page settings")).clicked() {
                        app.run("page.format", json!({ "reset": true }));
                        ui.close();
                    }
                });
                ui.menu_button(t("View"), |ui| {
                    ui.menu_button(t("Interface"), |ui| {
                        for s in crate::ribbon::Surface::ALL {
                            if ui.selectable_label(app.surface == s, s.label()).clicked() {
                                crate::ribbon::set_surface(app, s);
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    crate::page::view_choices(app, ui);
                    ui.separator();
                    ui.menu_button(t("Language"), |ui| {
                        for l in crate::i18n::Lang::ALL {
                            if ui.selectable_label(crate::i18n::lang() == l, l.name()).clicked() {
                                app.set_language(l);
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    if ui.button(t("Zoom in    Ctrl+=")).clicked() {
                        app.set_zoom(app.zoom * 1.15);
                    }
                    if ui.button(t("Zoom out    Ctrl+-")).clicked() {
                        app.set_zoom(app.zoom / 1.15);
                    }
                    if ui.button(t("Zoom 100%    Ctrl+0")).clicked() {
                        app.set_zoom(7.5);
                    }
                    ui.separator();
                    let mut pdf = app.reference.show && app.session.reference.is_some();
                    if ui
                        .add_enabled(
                            app.session.reference.is_some(),
                            egui::Checkbox::new(&mut pdf, t("PDF beside the score")),
                        )
                        .clicked()
                    {
                        crate::reference::toggle(app);
                    }
                    ui.separator();
                    for tab in crate::side::SideTab::ALL {
                        if ui.selectable_label(app.side == Some(tab), tab.label()).clicked() {
                            crate::side::toggle(app, tab);
                            ui.close();
                        }
                    }
                    ui.checkbox(&mut app.show_mixer, t("Mixer    F10"));
                    if ui.button(t("Free sound libraries…")).clicked() {
                        app.open_catalog();
                        ui.close();
                    }
                    ui.checkbox(&mut app.sound_on_input, t("Play notes while editing"));
                    ui.separator();
                    let dark = ui.ctx().global_style().visuals.dark_mode;
                    if ui
                        .button(if dark { t("Light theme") } else { t("Dark theme") })
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
                ui.menu_button(t("Help"), |ui| {
                    if ui.button(t("Keyboard shortcuts")).clicked() {
                        app.dialog = Some(Dialog::Shortcuts);
                        ui.close();
                    }
                    if ui.button(t("About ScoreArt")).clicked() {
                        app.dialog = Some(Dialog::About);
                        ui.close();
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    crate::ribbon::surface_switch(app, ui);
                });
            });
        });
}

// ---------- toolbar ----------

/// A square button showing a SMuFL glyph (or a text label), with a baseline at `base` (0–1 of
/// the button height) and the glyph size in px.
pub(crate) fn glyph_button(
    ui: &mut egui::Ui,
    label: GlyphLabel,
    selected: bool,
    tip: &str,
) -> egui::Response {
    let tk = Tokens::get(ui.ctx());
    let size = Vec2::new(34.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = resp.on_hover_text(tip);
    let bg = if selected {
        tk.accent_soft
    } else if resp.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 6.0, bg);
    if selected {
        ui.painter()
            .rect_stroke(rect, 6.0, Stroke::new(1.2, tk.accent), egui::StrokeKind::Inside);
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
        GlyphLabel::Icon(i) => paint_icon(ui.painter(), rect, i, fg),
    }
    resp
}

#[derive(Clone, Copy)]
pub(crate) enum GlyphLabel {
    /// glyph, em px, baseline (fraction of height), x offset from centre
    Glyph(char, f32, f32, f32),
    Icon(Icon),
}

/// Toolbar icons drawn in code, so they look the same whatever fonts the system has.
#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Play,
    Stop,
    Pen,
    Hand,
    Tie,
    Undo,
    Redo,
    Plus,
    Minus,
}

fn paint_icon(p: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let c = rect.center();
    let at = |x: f32, y: f32| Pos2::new(c.x + x, c.y + y);
    let stroke = Stroke::new(1.8, color);
    match icon {
        Icon::Play => {
            p.add(egui::Shape::convex_polygon(
                vec![at(-5.0, -7.0), at(7.0, 0.0), at(-5.0, 7.0)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, Vec2::splat(12.0)), 1.5, color);
        }
        Icon::Pen => {
            // A pencil leaning right: body, then the tip.
            p.line_segment([at(5.5, -7.5), at(-4.0, 2.0)], Stroke::new(4.0, color));
            p.add(egui::Shape::convex_polygon(
                vec![at(-5.6, 0.4), at(-2.4, 3.6), at(-7.5, 5.5)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Hand => crate::icons::paint(
            p,
            Rect::from_center_size(c, Vec2::splat(18.0)),
            crate::icons::Ico::Hand,
            color,
        ),
        Icon::Tie => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let a = std::f32::consts::PI * (i as f32 / 16.0);
                    at(-9.0 * a.cos(), 3.0 - 6.0 * a.sin())
                })
                .collect();
            p.add(egui::Shape::line(pts, Stroke::new(2.2, color)));
        }
        Icon::Undo | Icon::Redo => {
            let flip = if matches!(icon, Icon::Undo) { 1.0 } else { -1.0 };
            // An arc over the top, ending in an arrowhead on the left (undo) or right (redo).
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let a = std::f32::consts::PI * (i as f32 / 16.0);
                    at(flip * 7.0 * a.cos(), 4.0 - 7.0 * a.sin())
                })
                .collect();
            p.add(egui::Shape::line(pts, stroke));
            let tip = at(-7.0 * flip, 5.5);
            p.add(egui::Shape::convex_polygon(
                vec![tip, at(-11.0 * flip, 0.5), at(-2.5 * flip, 1.5)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Plus | Icon::Minus => {
            p.line_segment([at(-6.0, 0.0), at(6.0, 0.0)], stroke);
            if matches!(icon, Icon::Plus) {
                p.line_segment([at(0.0, -6.0), at(0.0, 6.0)], stroke);
            }
        }
    }
}

pub(crate) fn duration_glyph(v: NoteValue) -> char {
    match v {
        NoteValue::Long => '\u{ECA1}',
        NoteValue::Breve => '\u{ECA0}',
        NoteValue::Whole => '\u{ECA2}',
        NoteValue::Half => '\u{ECA3}',
        NoteValue::Quarter => '\u{ECA5}',
        NoteValue::Eighth => '\u{ECA7}',
        NoteValue::N16th => '\u{ECA9}',
        NoteValue::N32nd => '\u{ECAB}',
        NoteValue::N64th => '\u{ECAD}',
        NoteValue::N128th => '\u{ECAF}',
        NoteValue::N256th => '\u{ECB1}',
    }
}

pub fn toolbar(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let tk = Tokens::get(ui.ctx());
    egui::Panel::top("toolbar")
        .frame(
            egui::Frame::NONE
                .fill(tk.toolbar)
                .inner_margin(egui::Margin::symmetric(8, 4)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let input = app.session.input;
                // Playback.
                let playing = app.is_playing();
                let label = if playing {
                    GlyphLabel::Icon(Icon::Stop)
                } else {
                    GlyphLabel::Icon(Icon::Play)
                };
                let tip = if playing {
                    t("Stop (Space)")
                } else {
                    t("Play from the selection (Space)")
                };
                if glyph_button(ui, label, playing, tip).clicked() {
                    app.toggle_playback();
                }
                ui.separator();
                // The tools: the pen writes notes, the hand selects and moves.
                if glyph_button(
                    ui,
                    GlyphLabel::Icon(Icon::Pen),
                    input.active,
                    t("Pen: click to write notes (N)"),
                )
                .clicked()
                {
                    app.set_pen(true);
                }
                if glyph_button(
                    ui,
                    GlyphLabel::Icon(Icon::Hand),
                    !input.active,
                    t("Hand: select, edit and drag notes and marks (Esc)"),
                )
                .clicked()
                {
                    app.set_pen(false);
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
                    let tip = format!("{} ({})", t(v.xml_name()), v.digit());
                    let label = GlyphLabel::Glyph(duration_glyph(v), 26.0, 0.72, -3.0);
                    if glyph_button(ui, label, selected, &tip).clicked() {
                        app.run("input.duration", json!({ "value": v.digit() }));
                    }
                }
                if glyph_button(
                    ui,
                    GlyphLabel::Glyph('\u{ECB7}', 30.0, 0.62, -1.0),
                    input.dots > 0,
                    t("Augmentation dot (.)"),
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
                    t("Rest (0)"),
                )
                .clicked()
                {
                    app.run("note.rest", json!({}));
                }
                for (c, id, tip) in [
                    (g::DOUBLE_FLAT, "double-flat", t("Double flat")),
                    (g::FLAT, "flat", t("Flat")),
                    (g::NATURAL, "natural", t("Natural")),
                    (g::SHARP, "sharp", t("Sharp")),
                    (g::DOUBLE_SHARP, "double-sharp", t("Double sharp")),
                ] {
                    if glyph_button(ui, GlyphLabel::Glyph(c, 26.0, 0.55, -4.0), false, tip).clicked() {
                        app.run("note.accidental", json!({ "type": id }));
                    }
                }
                if glyph_button(ui, GlyphLabel::Icon(Icon::Tie), false, t("Tie (T)")).clicked() {
                    app.run("note.tie", json!({}));
                }
                ui.separator();
                for v in 0..4usize {
                    let selected = input.voice == v;
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(26.0, 34.0), Sense::click());
                    let resp = resp.on_hover_text(tf("Voice {} (Ctrl+Alt+{})", &[&(v + 1), &(v + 1)]));
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
                if glyph_button(ui, GlyphLabel::Icon(Icon::Undo), false, t("Undo (Ctrl+Z)")).clicked() {
                    app.run("edit.undo", json!({}));
                }
                if glyph_button(ui, GlyphLabel::Icon(Icon::Redo), false, t("Redo (Ctrl+Y)")).clicked() {
                    app.run("edit.redo", json!({}));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if glyph_button(ui, GlyphLabel::Icon(Icon::Plus), false, t("Zoom in")).clicked() {
                        app.set_zoom(app.zoom * 1.15);
                    }
                    ui.label(format!("{:.0}%", app.zoom / 7.5 * 100.0));
                    if glyph_button(ui, GlyphLabel::Icon(Icon::Minus), false, t("Zoom out")).clicked() {
                        app.set_zoom(app.zoom / 1.15);
                    }
                    egui::ComboBox::from_id_salt("view-mode")
                        .selected_text(crate::page::view_label(app.session.view))
                        .width(190.0)
                        .show_ui(ui, |ui| crate::page::view_choices(app, ui));
                });
            });
        });
}

/// The export formats: label and file extension.
pub(crate) fn export_formats() -> [(&'static str, &'static str); 7] {
    [
        (t("PDF (.pdf)"), "pdf"),
        (t("MusicXML (.musicxml)"), "musicxml"),
        (t("Compressed MusicXML (.mxl)"), "mxl"),
        (t("MIDI (.mid)"), "mid"),
        (t("SVG, first page (.svg)"), "svg"),
        (t("Audio (.wav)"), "wav"),
        (t("Audio per instrument (.zip of .wav)"), "zip"),
    ]
}

/// Writes the score in the format of `ext` and hands it to the host to save.
pub(crate) fn export(app: &mut ScoreApp, ext: &str) {
    let stem = app
        .session
        .file_name
        .rsplit_once('.')
        .map(|(a, _)| a.to_string())
        .unwrap_or_else(|| app.session.file_name.clone());
    let name = format!("{stem}.{ext}");
    match app.session.save_bytes(&name) {
        Ok(b) => {
            if let Some(n) = app.host.save(&name, b, true) {
                app.session.status = tf("Exported {}", &[&n]);
            }
        }
        Err(e) => app.session.status = e.to_string(),
    }
}

// ---------- status bar ----------

pub fn status_bar(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let tk = Tokens::get(ui.ctx());
    egui::Panel::bottom("status_bar")
        .exact_size(26.0)
        .frame(
            egui::Frame::NONE
                .fill(tk.status)
                .inner_margin(egui::Margin::symmetric(10, 0)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let s = &app.session;
                let mode = if s.input.active {
                    let dots = ".".repeat(usize::from(s.input.dots));
                    RichText::new(tf(
                        "● Note input · {}{} · Voice {}",
                        &[&t(s.input.value.xml_name()), &dots, &(s.input.voice + 1)],
                    ))
                    .color(voice_color(s.input.voice))
                } else {
                    RichText::new(t("Normal mode")).color(tk.text_dim)
                };
                ui.label(mode);
                ui.separator();
                let msg = if s.status.is_empty() {
                    s.describe_selection()
                } else {
                    s.status.clone()
                };
                // One line, cut short; the whole message on hover.
                let full = crate::i18n::tr(&msg);
                let line = full.split_whitespace().collect::<Vec<_>>().join(" ");
                let right = 330.0_f32.min(ui.available_width() / 2.0);
                ui.scope(|ui| {
                    ui.set_max_width((ui.available_width() - right).max(40.0));
                    ui.add(egui::Label::new(line).truncate()).on_hover_text(full);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{}{}", s.file_name, if s.dirty { " •" } else { "" }))
                            .color(tk.text_dim),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new({
                            let (m, p) = (s.score.measures.len(), s.score.parts.len());
                            match (m == 1, p == 1) {
                                (true, true) => tf("{} measure · {} instrument", &[&m, &p]),
                                (true, false) => tf("{} measure · {} instruments", &[&m, &p]),
                                (false, true) => tf("{} measures · {} instrument", &[&m, &p]),
                                (false, false) => tf("{} measures · {} instruments", &[&m, &p]),
                            }
                        })
                        .color(tk.text_dim),
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
    let tk = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 34.0), Sense::click());
    let resp = resp.on_hover_text(tip);
    let bg = if resp.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        tk.toolbar
    };
    ui.painter().rect_filled(rect, 5.0, bg);
    paint_smufl(ui.painter(), rect, name, ui.visuals().text_color());
    resp
}

/// A SMuFL glyph by name, scaled and centred to fit `rect`.
pub(crate) fn paint_smufl(p: &egui::Painter, rect: Rect, name: &str, color: Color32) {
    if let Some((c, w, lo, hi)) = scoreart_engrave::smufl::get(name) {
        let h = (hi - lo).max(0.3);
        let span = w.max(h).max(0.6);
        let fit = rect.width().min(rect.height()) - 10.0;
        let em = (4.0 * fit / span).min(fit * 1.25);
        let sp = em / 4.0;
        let x = rect.center().x - w * sp / 2.0;
        let y = rect.center().y + (hi + lo) / 2.0 * sp;
        glyph::paint(p, c, Pos2::new(x, y), em, color);
    }
}

/// A dynamic ("mf", "sfz" …) in Bravura letters, sized to fit `rect`.
pub(crate) fn paint_dynamic(p: &egui::Painter, rect: Rect, id: &str, color: Color32) {
    let letters: Vec<(char, f32)> = id.chars().filter_map(g::dynamic_letter).collect();
    let width: f32 = letters.iter().map(|l| l.1 - 0.1).sum::<f32>().max(0.5);
    let em = (4.0 * (rect.width() - 6.0) / width).min(rect.height());
    let sp = em / 4.0;
    let mut x = rect.center().x - width * sp / 2.0;
    let y = rect.center().y + 0.35 * sp;
    for (c, w) in letters {
        glyph::paint(p, c, Pos2::new(x, y), em, color);
        x += (w - 0.1) * sp;
    }
}

fn text_button(ui: &mut egui::Ui, label: impl Into<egui::WidgetText>, tip: &str) -> egui::Response {
    ui.add_sized(Vec2::new(40.0, 26.0), egui::Button::new(label))
        .on_hover_text(tip)
}

/// A palette cell showing a dynamic in Bravura letters, sized to fit.
fn dynamic_button(ui: &mut egui::Ui, id: &str) -> egui::Response {
    let tk = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(40.0, 30.0), Sense::click());
    let resp = resp.on_hover_text(id);
    let bg = if resp.hovered() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        tk.toolbar
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

pub fn palette_content(app: &mut ScoreApp, ui: &mut egui::Ui) {
    use scoreart_model::marks::{self, MARKS};
    let tk = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label(
            RichText::new(t(
                "Select notes or measures, then click. Shift+click or Shift+→ selects a range.",
            ))
            .small()
            .color(tk.text_dim),
        );
        ui.add_space(4.0);
        section(ui, t("Clefs"), false, |ui| {
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
                    smufl_button(ui, name, t(c.label()))
                } else {
                    let base = match c {
                        Clef::Bass => 0.45,
                        Clef::Alto | Clef::Tenor | Clef::Percussion => 0.5,
                        _ => 0.62,
                    };
                    glyph_button(ui, GlyphLabel::Glyph(gc, 15.0, base, -5.0), false, t(c.label()))
                };
                if resp.clicked() {
                    app.run("measure.clef", json!({ "clef": c.id() }));
                }
            }
        });
        section(ui, t("Key signatures"), false, |ui| {
            for f in -7..=7i32 {
                let k = KeySig::new(f);
                let tip = tf(
                    "{} major / {} accidentals",
                    &[&crate::i18n::note_name(k.name()), &f.abs()],
                );
                if text_button(ui, RichText::new(key_label(f)).size(12.0), &tip).clicked() {
                    app.run("measure.key", json!({ "fifths": f }));
                }
            }
        });
        section(ui, t("Time signatures"), false, |ui| {
            for ts in [
                "2/4", "3/4", "4/4", "5/4", "6/8", "3/8", "9/8", "12/8", "2/2", "7/8", "common", "cut",
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
            section(ui, t(group), open, |ui| {
                for d in MARKS.iter().filter(|d| d.group == group) {
                    if smufl_button(ui, d.glyph_above, t(d.label)).clicked() {
                        app.run("note.articulation", json!({ "type": d.id }));
                    }
                }
            });
        }
        section(ui, t("Dynamics"), true, |ui| {
            for d in scoreart_model::Dynamic::all() {
                if dynamic_button(ui, d.id()).clicked() {
                    app.run("note.dynamic", json!({ "type": d.id() }));
                }
            }
            if text_button(ui, "✕", t("Remove dynamic")).clicked() {
                app.run("note.dynamic", json!({ "type": "none" }));
            }
        });
        section(ui, t("Lines"), true, |ui| {
            for k in scoreart_model::SpannerKind::ALL {
                let (resp, _) = match k {
                    scoreart_model::SpannerKind::Slur => (text_button(ui, "⌒", t(k.label())), ()),
                    scoreart_model::SpannerKind::Crescendo => (text_button(ui, "<", t(k.label())), ()),
                    scoreart_model::SpannerKind::Diminuendo => (text_button(ui, ">", t(k.label())), ()),
                    scoreart_model::SpannerKind::CrescendoLine => (
                        text_button(ui, RichText::new("cresc.").italics(), t(k.label())),
                        (),
                    ),
                    scoreart_model::SpannerKind::DiminuendoLine => {
                        (text_button(ui, RichText::new("dim.").italics(), t(k.label())), ())
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
                        (smufl_button(ui, name, t(k.label())), ())
                    }
                    scoreart_model::SpannerKind::Pedal => {
                        (smufl_button(ui, "keyboardPedalPed", t(k.label())), ())
                    }
                    scoreart_model::SpannerKind::TrillLine => (
                        text_button(ui, RichText::new("tr~~").italics().strong(), t(k.label())),
                        (),
                    ),
                    scoreart_model::SpannerKind::Vibrato => {
                        (smufl_button(ui, "wiggleVibrato", t(k.label())), ())
                    }
                    scoreart_model::SpannerKind::LetRing => {
                        (text_button(ui, RichText::new("l.r.").italics(), t(k.label())), ())
                    }
                    scoreart_model::SpannerKind::PalmMute => {
                        (text_button(ui, RichText::new("P.M.").italics(), t(k.label())), ())
                    }
                    scoreart_model::SpannerKind::Glissando => (text_button(ui, "╲", t(k.label())), ()),
                    scoreart_model::SpannerKind::WavyGlissando => (text_button(ui, "≀", t(k.label())), ()),
                };
                if resp.clicked() {
                    app.run("line.add", json!({ "type": k.id() }));
                }
            }
            if text_button(ui, "✕", t("Remove lines starting or ending at the selection")).clicked() {
                app.run("line.remove", json!({}));
            }
        });
        section(ui, t("Arpeggios & tremolos"), false, |ui| {
            for (label, id, tip) in [
                ("≀", "arpeggio", t("Arpeggio")),
                ("≀↑", "arpeggio-up", t("Arpeggio up")),
                ("≀↓", "arpeggio-down", t("Arpeggio down")),
                ("[", "non-arpeggio", t("Non-arpeggio bracket")),
            ] {
                if text_button(ui, label, tip).clicked() {
                    app.run("note.arpeggio", json!({ "type": id }));
                }
            }
            for n in 1..=4 {
                if smufl_button(ui, &format!("tremolo{n}"), &tf("Tremolo, {} stroke(s)", &[&n])).clicked() {
                    app.run("note.tremolo", json!({ "strokes": n }));
                }
            }
            if smufl_button(ui, "buzzRoll", t("Buzz roll")).clicked() {
                app.run("note.tremolo", json!({ "strokes": "buzz" }));
            }
        });
        section(ui, t("Grace notes"), false, |ui| {
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
            if text_button(ui, "↑", t("Move grace note up")).clicked() {
                app.run("note.grace-pitch", json!({ "delta": 1 }));
            }
            if text_button(ui, "↓", t("Move grace note down")).clicked() {
                app.run("note.grace-pitch", json!({ "delta": -1 }));
            }
            if text_button(ui, "✕", t("Remove grace notes")).clicked() {
                app.run("note.grace-remove", json!({}));
            }
        });
        section(ui, t("Noteheads"), false, |ui| {
            for h in scoreart_model::NoteHead::ALL {
                let (c, _) = g::head_for(h, NoteValue::Quarter);
                let name = scoreart_engrave::smufl::TABLE
                    .iter()
                    .find(|x| x.1 == c)
                    .map(|x| x.0)
                    .unwrap_or("noteheadBlack");
                let resp = if name == "noteheadBlack" {
                    glyph_button(ui, GlyphLabel::Glyph(c, 30.0, 0.55, -4.0), false, t(h.id()))
                } else {
                    smufl_button(ui, name, t(h.id()))
                };
                if resp.clicked() {
                    app.run("note.head", json!({ "type": h.id() }));
                }
            }
            if text_button(ui, "( )", t("Parentheses (ghost note)")).clicked() {
                app.run("note.paren", json!({}));
            }
        });
        section(ui, t("Repeats & jumps"), false, |ui| {
            if text_button(ui, "𝄆", t("Start repeat")).clicked() {
                app.run("measure.repeat-start", json!({}));
            }
            if text_button(ui, "𝄇", t("End repeat")).clicked() {
                app.run("measure.barline", json!({ "type": "repeat-end" }));
            }
            for n in 1..=3 {
                if text_button(ui, format!("⌐{n}."), &tf("Volta {}", &[&n])).clicked() {
                    app.run("measure.volta", json!({ "number": n }));
                }
            }
            if text_button(ui, "⌐✕", t("Remove volta")).clicked() {
                app.run("measure.volta", json!({ "number": 0 }));
            }
            if smufl_button(ui, "repeat1Bar", t("One-measure repeat")).clicked() {
                app.run("measure.repeat-sign", json!({}));
            }
            for j in scoreart_model::Jump::ALL {
                let resp = match j {
                    scoreart_model::Jump::Segno => smufl_button(ui, "segno", t("Segno")),
                    scoreart_model::Jump::Coda => smufl_button(ui, "coda", t("Coda")),
                    scoreart_model::Jump::VariedCoda => smufl_button(ui, "codaSquare", t("Varied coda")),
                    _ => ui
                        .add_sized(
                            Vec2::new(84.0, 26.0),
                            egui::Button::new(RichText::new(j.text().unwrap_or("")).italics()),
                        )
                        .on_hover_text(t(j.id())),
                };
                if resp.clicked() {
                    app.run("measure.jump", json!({ "type": j.id() }));
                }
            }
        });
        section(ui, t("Barlines"), false, |ui| {
            for (label, id) in [("‖", "double"), ("𝄂", "final"), ("|", "regular")] {
                if text_button(ui, label, id).clicked() {
                    app.run("measure.barline", json!({ "type": id }));
                }
            }
        });
        section(ui, t("Text"), false, |ui| {
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
                t("Staff text…"),
                t("Staff text"),
                "note.text",
                "text",
                String::new(),
            );
            dlg(
                ui,
                t("Chord symbol…"),
                t("Chord symbol (e.g. Cmaj7, F#m7/C#)"),
                "note.chord-symbol",
                "text",
                String::new(),
            );
            dlg(
                ui,
                t("Tempo marking…"),
                t("Tempo marking (e.g. Allegro)"),
                "measure.tempo-text",
                "text",
                "Allegro".into(),
            );
            dlg(
                ui,
                t("System text…"),
                t("System text"),
                "measure.text",
                "text",
                String::new(),
            );
            dlg(
                ui,
                t("Fingering…"),
                t("Fingering"),
                "note.fingering",
                "text",
                String::new(),
            );
            if ui
                .add_sized(Vec2::new(98.0, 26.0), egui::Button::new(t("Rehearsal mark")))
                .clicked()
            {
                app.run("measure.rehearsal", json!({}));
            }
            for f in ["1", "2", "3", "4", "5"] {
                if ui
                    .add_sized(Vec2::new(26.0, 26.0), egui::Button::new(f))
                    .on_hover_text(t("Fingering"))
                    .clicked()
                {
                    app.run("note.fingering", json!({ "text": f }));
                }
            }
        });
        section(ui, t("Tempo"), false, |ui| {
            let unit = beat_unit_here(app);
            for bpm in [60, 72, 80, 92, 100, 108, 120, 132, 144] {
                if text_button(ui, bpm.to_string(), &format!("{} = {bpm}", unit_symbol(unit))).clicked() {
                    app.run("measure.tempo", json!({ "bpm": bpm, "unit": unit.id() }));
                }
            }
        });
    });
}

/// The beat a tempo is counted in at the selection: dotted quarters in 6/8, 9/8 and 12/8.
pub(crate) fn beat_unit_here(app: &ScoreApp) -> scoreart_model::BeatUnit {
    let ts = app.session.score.time_at(app.session.focus().2);
    if ts.beat_type == 8 && ts.beats.is_multiple_of(3) && ts.beats > 3 {
        scoreart_model::BeatUnit::DottedQuarter
    } else {
        scoreart_model::BeatUnit::Quarter
    }
}

pub(crate) fn unit_symbol(u: scoreart_model::BeatUnit) -> &'static str {
    use scoreart_model::BeatUnit::*;
    match u {
        Eighth => "♪",
        DottedEighth => "♪.",
        Quarter => "♩",
        DottedQuarter => "♩.",
        Half => "𝅗𝅥",
        DottedHalf => "𝅗𝅥.",
    }
}

fn unit_name(u: scoreart_model::BeatUnit) -> &'static str {
    use scoreart_model::BeatUnit::*;
    match u {
        Eighth => "Eighth",
        DottedEighth => "Dotted eighth",
        Quarter => "Quarter",
        DottedQuarter => "Dotted quarter",
        Half => "Half",
        DottedHalf => "Dotted half",
    }
}

/// Opens the tempo dialog with the beat of the selection.
pub(crate) fn open_tempo(app: &mut ScoreApp) {
    let unit = beat_unit_here(app);
    let bpm = if unit == scoreart_model::BeatUnit::DottedQuarter {
        "60"
    } else {
        "100"
    };
    app.dialog = Some(Dialog::Tempo {
        bpm: bpm.into(),
        unit,
    });
}

/// Opens the measure-length dialog for the selection (a first measure is a pickup).
pub(crate) fn open_measure_length(app: &mut ScoreApp) {
    let m = app.session.focus().2;
    let length = match app.session.score.measures.get(m).and_then(|i| i.actual) {
        Some(t) => {
            let eighths = t / (scoreart_model::TPQ / 2);
            format!("{eighths}/8")
        }
        None if m == 0 => "1/8".into(),
        None => String::new(),
    };
    app.dialog = Some(Dialog::MeasureLength {
        length,
        counted: m != 0,
    });
}

/// The section break after the focused measure, if it has one.
pub(crate) fn section_at_focus(app: &ScoreApp) -> Option<scoreart_model::SectionBreak> {
    let m = app.session.focus().2;
    app.session.score.measures.get(m).and_then(|i| i.section)
}

pub(crate) fn key_label(f: i32) -> String {
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
    /// The verse the lyric field edits (0-based).
    pub verse: usize,
    part_name: String,
}

pub fn properties_content(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let tk = Tokens::get(ui.ctx());
    // Refresh buffers when the score or selection changed.
    if app.fields.revision != app.session.revision || app.fields.sel != app.session.selection {
        let m = &app.session.score.meta;
        let (part, _, _) = app.session.focus();
        let lyric = app
            .session
            .selected_event()
            .and_then(|e| app.session.score.event(e))
            .and_then(|e| e.lyric(app.fields.verse).map(str::to_string))
            .unwrap_or_default();
        let verse = app.fields.verse;
        app.fields = Fields {
            verse,
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
    egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(6.0);
            egui::CollapsingHeader::new(RichText::new(t("Score")).strong()).default_open(true).show(ui, |ui| {
                let mut changed = false;
                egui::Grid::new("meta").num_columns(2).spacing([6.0, 6.0]).show(ui, |ui| {
                    for (label, field) in [
                        (t("Title"), &mut app.fields.title),
                        (t("Subtitle"), &mut app.fields.subtitle),
                        (t("Composer"), &mut app.fields.composer),
                        (t("Lyricist"), &mut app.fields.lyricist),
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
            egui::CollapsingHeader::new(RichText::new(t("Selection")).strong()).default_open(true).show(ui, |ui| {
                match sel {
                    None => {
                        ui.label(RichText::new(t("Nothing selected. Click a note, a rest or an empty part of a measure.")).color(tk.text_dim));
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
                        ui.label(t("Lyric"));
                        egui::ComboBox::from_id_salt("verse")
                            .width(54.0)
                            .selected_text(tf("v. {}", &[&(app.fields.verse + 1)]))
                            .show_ui(ui, |ui| {
                                for v in 0..6 {
                                    if ui.selectable_label(app.fields.verse == v, tf("Verse {}", &[&(v + 1)])).clicked() {
                                        app.fields.verse = v;
                                        app.fields.revision = 0;
                                    }
                                }
                            });
                        let r = ui.add(egui::TextEdit::singleline(&mut app.fields.lyric).desired_width(120.0));
                        if r.lost_focus() {
                            let text = app.fields.lyric.clone();
                            let verse = app.fields.verse + 1;
                            app.run("note.lyric", json!({ "text": text, "verse": verse }));
                            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                app.run("select.next", json!({}));
                                app.fields.revision = 0;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t("Voice"));
                        for v in 1..=4 {
                            if ui.selectable_label(ev.voice + 1 == v, v.to_string()).clicked() {
                                app.run("note.voice", json!({ "voice": v }));
                            }
                        }
                    });
                }
            });
            egui::CollapsingHeader::new(RichText::new(t("Instrument")).strong()).default_open(false).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t("Name"));
                    let r = ui.add(egui::TextEdit::singleline(&mut app.fields.part_name).desired_width(130.0));
                    if r.lost_focus() {
                        let n = app.fields.part_name.clone();
                        app.run("part.rename", json!({ "name": n }));
                    }
                });
                let (part, _, measure) = app.session.focus();
                let current = app.session.score.parts.get(part).map(|p| p.name.clone()).unwrap_or_default();
                // Swap the whole part to another instrument.
                let mut pick: Option<&'static str> = None;
                egui::ComboBox::from_id_salt("change-instrument")
                    .selected_text(tf("Change to … ({})", &[&crate::i18n::tr(&current)]))
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        for i in scoreart_model::INSTRUMENTS {
                            if ui.selectable_label(i.name == current || t(i.name) == current, t(i.name)).clicked() {
                                pick = Some(i.id);
                            }
                        }
                    });
                if let Some(id) = pick {
                    crate::instruments::request_change(app, part, id);
                }
                // Switch instrument from the selected measure on ("To Flute").
                let here = app
                    .session
                    .score
                    .parts
                    .get(part)
                    .and_then(|p| p.measures.get(measure))
                    .and_then(|m| m.instrument_change.as_ref())
                    .map(|c| c.name.clone());
                let mut change: Option<&'static str> = None;
                egui::ComboBox::from_id_salt("instrument-change")
                    .selected_text(match &here {
                        Some(n) => tf("From m. {}: {}", &[&(measure + 1), &crate::i18n::tr(n)]),
                        None => tf("Instrument change at m. {} …", &[&(measure + 1)]),
                    })
                    .width(200.0)
                    .show_ui(ui, |ui| {
                        if here.is_some() && ui.selectable_label(false, t("(remove the change)")).clicked() {
                            change = Some("");
                        }
                        for i in scoreart_model::INSTRUMENTS {
                            if ui.selectable_label(here.as_deref() == Some(i.name), i.name).clicked() {
                                change = Some(i.id);
                            }
                        }
                    });
                if let Some(id) = change {
                    app.run(
                        "measure.instrument-change",
                        json!({ "part": part, "measure": measure, "instrument": id }),
                    );
                }
                if ui.button(t("Remove this instrument")).clicked() {
                    app.run("part.remove", json!({}));
                }
            });
        });
}

pub fn open_lyric_dialog(app: &mut ScoreApp) {
    let current = app
        .session
        .selected_event()
        .and_then(|e| app.session.score.event(e))
        .and_then(|e| e.lyric(app.fields.verse).map(str::to_string))
        .unwrap_or_default();
    if app.session.selected_event().is_some() {
        app.dialog = Some(Dialog::Text {
            title: t("Lyric (a trailing - joins it to the next syllable)"),
            command: "note.lyric",
            key: "text",
            value: current,
        });
    } else {
        app.session.status = t("Select a note first").into();
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
            title: t("Untitled score").into(),
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
                ui.heading(t("New score"));
                ui.add_space(6.0);
                egui::Grid::new("ns")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        ui.label(t("Title"));
                        ui.text_edit_singleline(&mut ns.title);
                        ui.end_row();
                        ui.label(t("Composer"));
                        ui.text_edit_singleline(&mut ns.composer);
                        ui.end_row();
                        ui.label(t("Time"));
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
                        ui.label(t("Key"));
                        egui::ComboBox::from_id_salt("ns_key")
                            .selected_text(tf(
                                "{} major",
                                &[&crate::i18n::note_name(KeySig::new(ns.key).name())],
                            ))
                            .show_ui(ui, |ui| {
                                for f in -7..=7 {
                                    ui.selectable_value(
                                        &mut ns.key,
                                        f,
                                        tf(
                                            "{} major ({})",
                                            &[&crate::i18n::note_name(KeySig::new(f).name()), &key_label(f)],
                                        ),
                                    );
                                }
                            });
                        ui.end_row();
                        ui.label(t("Measures"));
                        ui.add(egui::DragValue::new(&mut ns.measures).range(1..=999));
                        ui.end_row();
                        ui.label(t("Tempo ♩="));
                        ui.add(egui::DragValue::new(&mut ns.tempo).range(20..=300));
                        ui.end_row();
                    });
                ui.add_space(6.0);
                ui.label(RichText::new(t("Instruments")).strong());
                ui.horizontal_wrapped(|ui| {
                    for inst in INSTRUMENTS {
                        let mut on = ns.instruments.contains(&inst.id);
                        if ui.checkbox(&mut on, t(inst.name)).changed() {
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
                        .add_enabled(!ns.instruments.is_empty(), egui::Button::new(t("Create")))
                        .clicked();
                    let cancel = ui.button(t("Cancel")).clicked();
                    (ok, cancel)
                })
                .inner
            });
            let (ok, cancel) = resp.inner;
            if ok {
                app.run(
                    "file.new",
                    json!({ "title": ns.title, "composer": ns.composer, "instruments": ns.instruments,
                            "names": ns.instruments.iter().map(|id| scoreart_model::instrument(id).map_or("", |i| t(i.name))).collect::<Vec<_>>(),
                            "time": ns.time,
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
                ui.horizontal(|ui| {
                    (
                        ui.button(t("OK")).clicked() || enter,
                        ui.button(t("Cancel")).clicked(),
                    )
                })
                .inner
            });
            let (ok, cancel) = resp.inner;
            if ok {
                let v = if key == "bpm" {
                    json!({ key: value.trim().parse::<i64>().unwrap_or(0) })
                } else if command == "note.lyric" {
                    json!({ key: value, "verse": app.fields.verse + 1 })
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
        Dialog::Block(d) => {
            close = crate::texts::block_dialog(app, ctx, d);
            if !close {
                return;
            }
        }
        Dialog::Tempo { mut bpm, mut unit } => {
            let resp = egui::Modal::new(egui::Id::unique("tempo_dialog")).show(ctx, |ui| {
                ui.set_width(340.0);
                ui.heading(t("Tempo"));
                ui.label(
                    RichText::new(t("Beats per minute of a note value (in 6/8: dotted quarters).")).small(),
                );
                ui.add_space(4.0);
                let mut enter = false;
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("tempo-unit")
                        .selected_text(unit_symbol(unit))
                        .width(60.0)
                        .show_ui(ui, |ui| {
                            for u in scoreart_model::BeatUnit::ALL {
                                ui.selectable_value(
                                    &mut unit,
                                    u,
                                    format!("{}  {}", unit_symbol(u), t(unit_name(u))),
                                );
                            }
                        });
                    ui.label("=");
                    let r = ui.add(egui::TextEdit::singleline(&mut bpm).desired_width(60.0));
                    r.request_focus();
                    enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    (
                        ui.button(t("OK")).clicked() || enter,
                        ui.button(t("Cancel")).clicked(),
                    )
                })
                .inner
            });
            let (ok, cancel) = resp.inner;
            if ok {
                let n = bpm.trim().parse::<i64>().unwrap_or(0);
                app.run("measure.tempo", json!({ "bpm": n, "unit": unit.id() }));
                close = true;
            } else if cancel || resp.should_close() {
                close = true;
            } else {
                app.dialog = Some(Dialog::Tempo { bpm, unit });
            }
        }
        Dialog::MeasureLength {
            mut length,
            mut counted,
        } => {
            let resp = egui::Modal::new(egui::Id::unique("length_dialog")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading(t("Measure length"));
                ui.label(
                    RichText::new(t(
                        "For a pickup (upbeat) or a short measure: its length as a fraction of a whole note, e.g. 1/8 for one eighth.",
                    ))
                    .small(),
                );
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for l in ["1/8", "1/4", "3/8", "1/2", "5/8", "3/4"] {
                        if ui.selectable_label(length == l, l).clicked() {
                            length = l.to_string();
                        }
                    }
                    if ui.selectable_label(length.trim().is_empty(), t("As the time signature")).clicked() {
                        length.clear();
                    }
                });
                let r = ui.add(
                    egui::TextEdit::singleline(&mut length)
                        .hint_text(t("As the time signature"))
                        .desired_width(160.0),
                );
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.checkbox(&mut counted, t("Count it in the measure numbers"));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    (
                        ui.button(t("OK")).clicked() || enter,
                        ui.button(t("Cancel")).clicked(),
                    )
                })
                .inner
            });
            let (ok, cancel) = resp.inner;
            if ok {
                app.run(
                    "measure.length",
                    json!({ "length": length.trim(), "counted": counted }),
                );
                close = true;
            } else if cancel || resp.should_close() {
                close = true;
            } else {
                app.dialog = Some(Dialog::MeasureLength { length, counted });
            }
        }
        Dialog::Section(mut sec) => {
            let resp = egui::Modal::new(egui::Id::unique("section_dialog")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading(t("Section break"));
                ui.horizontal(|ui| {
                    ui.label(t("Pause in playback"));
                    ui.add(
                        egui::DragValue::new(&mut sec.pause)
                            .range(0.0..=60.0)
                            .speed(0.1)
                            .suffix(" s"),
                    );
                });
                ui.checkbox(&mut sec.long_names, t("Next system shows long instrument names"));
                ui.checkbox(&mut sec.restart_numbers, t("Measure numbers start again at 1"));
                ui.add_space(6.0);
                ui.horizontal(|ui| (ui.button(t("OK")).clicked(), ui.button(t("Cancel")).clicked()))
                    .inner
            });
            let (ok, cancel) = resp.inner;
            if ok {
                app.run(
                    "measure.break",
                    json!({
                        "kind": "section",
                        "pause": sec.pause,
                        "long_names": sec.long_names,
                        "restart_numbers": sec.restart_numbers,
                    }),
                );
                close = true;
            } else if cancel || resp.should_close() {
                close = true;
            } else {
                app.dialog = Some(Dialog::Section(sec));
            }
        }
        Dialog::Shortcuts => {
            let resp = egui::Modal::new(egui::Id::unique("shortcuts")).show(ctx, |ui| {
                ui.set_width(460.0);
                ui.heading(t("Keyboard shortcuts"));
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    egui::Grid::new("sc").striped(true).num_columns(2).show(ui, |ui| {
                        for c in scoreart_engine::registry() {
                            if let Some(s) = c.shortcut {
                                ui.label(RichText::new(crate::i18n::keys(s)).monospace());
                                ui.label(t(c.label));
                                ui.end_row();
                            }
                        }
                    });
                });
                ui.button(t("Close")).clicked()
            });
            close = resp.inner || resp.should_close();
        }
        Dialog::About => {
            let resp = egui::Modal::new(egui::Id::unique("about")).show(ctx, |ui| {
                ui.set_width(440.0);
                ui.heading("ScoreArt");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label(t("An open-source music notation editor written in Rust."));
                ui.add_space(6.0);
                ui.label(t(
                    "Licensed MIT OR Apache-2.0. Music font: Bravura by Steinberg Media Technologies GmbH; text font: Crimson Text by Sebastian Kosch; both SIL Open Font License 1.1.",
                ));
                ui.label(t("MusicXML and SMuFL are specifications of the W3C Music Notation Community Group."));
                ui.add_space(6.0);
                ui.label(RichText::new(t("ScoreArt is independent and not affiliated with MuseScore / Muse Group or ArtCraft.")).small());
                ui.button(t("Close")).clicked()
            });
            close = resp.inner || resp.should_close();
        }
    }
    if close {
        app.dialog = None;
    }
}
