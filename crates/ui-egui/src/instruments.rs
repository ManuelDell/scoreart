//! "Instruments and sounds": every instrument of the score with its sound, order and
//! visibility; adding, changing and removing instruments; giving a line of an instrument (the
//! alto of "Women") an instrument of its own.
//!
//! Changing an instrument that holds several lines asks first whether to merge them into the
//! new instrument, split one off, or keep only one, so nothing is lost by accident.

use egui::RichText;
use scoreart_model::{INSTRUMENTS, Line};
use serde_json::json;

use crate::ScoreApp;
use crate::theme::Tokens;

use crate::i18n::{t, tf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Merge,
    Split,
    Discard,
}

/// An instrument change waiting for the user to say what happens to the lines.
#[derive(Clone, Debug)]
pub(crate) struct PendingChange {
    part: usize,
    instrument: &'static str,
    mode: Mode,
    keep: Line,
}

#[derive(Default)]
pub(crate) struct InstrumentsState {
    pending: Option<PendingChange>,
}

impl InstrumentsState {
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

/// Changes `part` to `instrument`, asking first when the part holds more than one line of notes.
pub(crate) fn request_change(app: &mut ScoreApp, part: usize, instrument: &'static str) {
    let Some(p) = app.session.score.parts.get(part) else {
        return;
    };
    let filled: Vec<Line> = p.lines().into_iter().filter(|l| p.line_has_notes(*l)).collect();
    if filled.len() < 2 {
        app.run(
            "part.change",
            json!({ "part": part, "instrument": instrument, "name": instrument_name(instrument) }),
        );
        return;
    }
    app.instruments.pending = Some(PendingChange {
        part,
        instrument,
        mode: Mode::Merge,
        keep: filled.first().copied().unwrap_or_default(),
    });
}

/// Gives one line of `part` its own new instrument; the part keeps its other lines.
fn split_line(app: &mut ScoreApp, part: usize, line: Line, instrument: &'static str) {
    app.run(
        "part.change",
        json!({
            "part": part,
            "instrument": instrument,
            "mode": "split",
            "name": instrument_name(instrument),
            "staff": line.staff + 1,
            "voice": line.voice + 1,
        }),
    );
}

fn instrument_name(id: &str) -> &'static str {
    INSTRUMENTS.iter().find(|i| i.id == id).map_or("", |i| t(i.name))
}

/// The question asked when an instrument change needs a decision (shown over everything).
pub(crate) fn dialogs(app: &mut ScoreApp, ctx: &egui::Context) {
    confirm(app, ctx);
}

/// The "Instruments" tab of the side panel.
pub(crate) fn content(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let tk = Tokens::get(ui.ctx());
    let playing = app.is_playing();
    let count = app.session.score.parts.len();
    for pi in 0..count {
        egui::Frame::NONE
            .fill(tk.canvas.linear_multiply(0.35))
            .corner_radius(6)
            .inner_margin(6)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                part_row(app, ui, pi, count, playing, &tk);
            });
        ui.add_space(4.0);
    }
    ui.add_space(4.0);
    ui.label(RichText::new(t("Add instrument")).strong());
    let mut add: Option<&'static str> = None;
    egui::ComboBox::from_id_salt("instruments-add")
        .selected_text(t("Choose …"))
        .width(ui.available_width().min(220.0))
        .height(420.0)
        .show_ui(ui, |ui| {
            for i in INSTRUMENTS {
                if ui.selectable_label(false, t(i.name)).clicked() {
                    add = Some(i.id);
                }
            }
        });
    if let Some(id) = add {
        app.run(
            "part.add",
            json!({ "instrument": id, "name": instrument_name(id) }),
        );
    }
}

fn part_row(app: &mut ScoreApp, ui: &mut egui::Ui, pi: usize, count: usize, playing: bool, tk: &Tokens) {
    let Some(part) = app.session.score.parts.get(pi).cloned() else {
        return;
    };
    let wide = ui.available_width().min(220.0);
    ui.horizontal(|ui| {
        let mut shown = part.visible();
        if ui
            .checkbox(&mut shown, "")
            .on_hover_text(t("Show in the score (hidden instruments still play)"))
            .changed()
        {
            app.run("part.visible", json!({ "part": pi, "visible": shown }));
        }
        ui.label(RichText::new(&part.name).strong());
        if part.staff_count() > 1 {
            ui.label(RichText::new(tf("{} staves", &[&part.staff_count()])).color(tk.text_dim));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(count > 1, egui::Button::new("✕").small())
                .on_hover_text(t("Remove this instrument and its music"))
                .clicked()
            {
                app.run("part.remove", json!({ "part": pi }));
            }
            if ui
                .add_enabled(pi + 1 < count, egui::Button::new("▼").small())
                .on_hover_text(t("Move down"))
                .clicked()
            {
                app.run("part.move", json!({ "part": pi, "to": pi + 1 }));
            }
            if ui
                .add_enabled(pi > 0, egui::Button::new("▲").small())
                .on_hover_text(t("Move up"))
                .clicked()
            {
                app.run("part.move", json!({ "part": pi, "to": pi - 1 }));
            }
        });
    });
    let mut pick: Option<&'static str> = None;
    egui::ComboBox::from_id_salt(("instruments-change", pi))
        .selected_text(t("Change instrument …"))
        .width(wide)
        .height(420.0)
        .show_ui(ui, |ui| {
            for i in INSTRUMENTS {
                if ui
                    .selectable_label(i.name == part.name || t(i.name) == part.name, t(i.name))
                    .clicked()
                {
                    pick = Some(i.id);
                }
            }
        });
    if let Some(id) = pick {
        request_change(app, pi, id);
    }
    ui.label(RichText::new(t("Sound")).color(tk.text_dim));
    crate::mixer::sound_picker(app, ui, pi, &part.sound, playing, wide);
    // Lines that could become instruments of their own.
    let lines: Vec<Line> = part
        .lines()
        .into_iter()
        .filter(|l| part.line_has_notes(*l))
        .collect();
    if lines.len() > 1 {
        for line in lines {
            ui.label(RichText::new(crate::i18n::line_label(&part, line)).color(tk.text_dim));
            let mut pick: Option<&'static str> = None;
            egui::ComboBox::from_id_salt(("instruments-line", pi, line.staff, line.voice))
                .selected_text(t("Own instrument …"))
                .width(wide)
                .height(420.0)
                .show_ui(ui, |ui| {
                    ui.label(RichText::new(t("Moves this line into a new instrument")).color(tk.text_dim));
                    for i in INSTRUMENTS {
                        if ui.selectable_label(false, t(i.name)).clicked() {
                            pick = Some(i.id);
                        }
                    }
                });
            if let Some(id) = pick {
                split_line(app, pi, line, id);
            }
        }
    }
}

/// The question asked before changing an instrument with several lines.
fn confirm(app: &mut ScoreApp, ctx: &egui::Context) {
    let Some(mut pc) = app.instruments.pending.clone() else {
        return;
    };
    let Some(part) = app.session.score.parts.get(pc.part).cloned() else {
        app.instruments.pending = None;
        return;
    };
    let lines: Vec<Line> = part
        .lines()
        .into_iter()
        .filter(|l| part.line_has_notes(*l))
        .collect();
    let new = instrument_name(pc.instrument);
    let mut done: Option<bool> = None;
    egui::Modal::new(egui::Id::unique("instrument-change-confirm")).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.heading(tf("Change {} to {}", &[&part.name, &new]));
        ui.label(tf(
            "{} holds {} lines. What should happen to them?",
            &[&part.name, &lines.len()],
        ));
        ui.add_space(6.0);
        ui.radio_value(
            &mut pc.mode,
            Mode::Merge,
            tf("Merge: all lines go into {}", &[&new]),
        );
        ui.label(
            RichText::new(t(
                "    Staves the new instrument does not have become extra voices.",
            ))
            .small()
            .weak(),
        );
        ui.radio_value(
            &mut pc.mode,
            Mode::Split,
            tf(
                "Split: one line becomes {}, {} keeps the rest",
                &[&new, &part.name],
            ),
        );
        ui.radio_value(
            &mut pc.mode,
            Mode::Discard,
            tf("Keep one line as {}, discard the others", &[&new]),
        );
        if pc.mode != Mode::Merge {
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                ui.label(t("Line"));
                egui::ComboBox::from_id_salt("instrument-change-line")
                    .selected_text(crate::i18n::line_label(&part, pc.keep))
                    .show_ui(ui, |ui| {
                        for l in &lines {
                            ui.selectable_value(&mut pc.keep, *l, crate::i18n::line_label(&part, *l));
                        }
                    });
            });
        }
        if pc.mode == Mode::Discard {
            ui.label(
                RichText::new(t("The other lines are deleted (Undo brings them back)."))
                    .color(egui::Color32::from_rgb(200, 90, 60)),
            );
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(t("Change")).clicked() {
                done = Some(true);
            }
            if ui.button(t("Cancel")).clicked() {
                done = Some(false);
            }
        });
    });
    match done {
        Some(true) => {
            app.instruments.pending = None;
            let mode = match pc.mode {
                Mode::Merge => "merge",
                Mode::Split => "split",
                Mode::Discard => "discard",
            };
            app.run(
                "part.change",
                json!({
                    "part": pc.part,
                    "instrument": pc.instrument,
                    "mode": mode,
                    "name": instrument_name(pc.instrument),
                    "staff": pc.keep.staff + 1,
                    "voice": pc.keep.voice + 1,
                }),
            );
        }
        Some(false) => app.instruments.pending = None,
        None => app.instruments.pending = Some(pc),
    }
}
