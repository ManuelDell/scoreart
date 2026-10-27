//! MIDI input: connecting a keyboard or controller, entering notes from it, and the "MIDI
//! input" window where every key and controller can be given its own action.

use egui::{Color32, Rect, RichText, Sense, Stroke, Vec2};
use scoreart_engine::midi_input::{
    ACTIONS, MidiAction, MidiEvent, MidiMap, MidiState, Trigger, action, key_name,
};
use serde_json::json;

use crate::ScoreApp;
use crate::theme::Tokens;

const MAP_SETTING: &str = "midi-map";
const PORT_SETTING: &str = "midi-port";

#[derive(Default)]
pub(crate) struct MidiUi {
    pub open: bool,
    map: MidiMap,
    state: MidiState,
    /// Settings were read from the host.
    loaded: bool,
    ports: Vec<String>,
    /// The next key or controller pressed is selected.
    learn: bool,
    selected: Option<Trigger>,
    /// The last key or controller pressed, for the display.
    last: Option<Trigger>,
    error: String,
    /// When the device list is looked up again while it is empty (seconds of app time).
    next_scan: f64,
}

/// While no device is known, the list is looked up again every second (the browser fills it in
/// after the user allowed access; devices get plugged in).
fn scan_due(ui: &egui::Ui, next: &mut f64) -> bool {
    let now = ui.input(|i| i.time);
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    if now >= *next {
        *next = now + 1.0;
        true
    } else {
        false
    }
}

/// Reads saved bindings, reconnects the last device, and handles the messages that arrived.
pub(crate) fn poll(app: &mut ScoreApp) {
    if !app.midi.loaded {
        app.midi.loaded = true;
        if let Some(t) = app.host.load_setting(MAP_SETTING) {
            app.midi.map = MidiMap::from_json(&t);
        }
        if let Some(port) = app
            .host
            .load_setting(PORT_SETTING)
            .filter(|p| !p.trim().is_empty())
            && app.host.midi_ports().iter().any(|p| p == port.trim())
        {
            let _ = app.host.midi_open(Some(port.trim()));
        }
    }
    let msgs = app.host.take_midi();
    for msg in msgs {
        if let Some(t) = app.midi.map.pressed(&msg) {
            app.midi.last = Some(t);
            if app.midi.learn {
                app.midi.learn = false;
                app.midi.selected = Some(t);
                continue;
            }
        }
        let ev = app.midi.map.interpret(&msg, &mut app.midi.state);
        match ev {
            Some(MidiEvent::Note { key, chord, .. }) => {
                if app.session.input.active {
                    app.run("note.midi", json!({ "key": key, "chord": chord }));
                } else {
                    app.preview_keys(&[key]);
                }
            }
            Some(MidiEvent::Command { id, params }) => app.run(id, params),
            Some(MidiEvent::Ui(_)) => app.toggle_playback(),
            None => {}
        }
    }
}

fn save(app: &mut ScoreApp) {
    let text = app.midi.map.to_json();
    app.host.save_setting(MAP_SETTING, &text);
}

fn action_label(a: &MidiAction) -> String {
    match a {
        MidiAction::Note => "Enter the note".into(),
        MidiAction::Ignore => "Nothing".into(),
        MidiAction::Action(id) => action(id).map_or_else(|| id.clone(), |s| s.label.to_string()),
    }
}

pub(crate) fn window(app: &mut ScoreApp, ctx: &egui::Context) {
    if !app.midi.open {
        return;
    }
    let t = Tokens::get(ctx);
    let mut open = true;
    egui::Window::new("MIDI input")
        .open(&mut open)
        .default_width(720.0)
        .pivot(egui::Align2::CENTER_TOP)
        .default_pos(egui::pos2(ctx.content_rect().center().x, 60.0))
        .show(ctx, |ui| {
            device_row(app, ui, &t);
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Channel");
                let mut ch = app.midi.map.channel;
                egui::ComboBox::from_id_salt("midi-channel")
                    .selected_text(ch.map_or("All".to_string(), |c| c.to_string()))
                    .width(60.0)
                    .height(320.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut ch, None, "All");
                        for c in 1..=16u8 {
                            ui.selectable_value(&mut ch, Some(c), c.to_string());
                        }
                    });
                let mut tr = app.midi.map.transpose;
                ui.label("Transpose");
                ui.add(egui::DragValue::new(&mut tr).range(-36..=36).suffix(" semitones"));
                if ch != app.midi.map.channel || tr != app.midi.map.transpose {
                    app.midi.map.channel = ch;
                    app.midi.map.transpose = tr;
                    save(app);
                }
                ui.separator();
                ui.label(RichText::new("Last pressed:").color(t.text_dim));
                ui.label(app.midi.last.map_or("—".to_string(), Trigger::label));
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Click a key below, or press Learn and then a key, pad, button or pedal on the device, to choose what it does.",
                )
                .color(t.text_dim),
            );
            keyboard(app, ui, &t);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let learn = app.midi.learn;
                if ui
                    .add(egui::Button::new(if learn { "Press a key or control…" } else { "Learn" }).selected(learn))
                    .clicked()
                {
                    app.midi.learn = !learn;
                }
                ui.label("or controller");
                let mut cc = match app.midi.selected {
                    Some(Trigger::Control(c)) => i32::from(c),
                    _ => -1,
                };
                if ui
                    .add(egui::DragValue::new(&mut cc).range(-1..=127).custom_formatter(|v, _| {
                        if v < 0.0 { "—".into() } else { format!("CC {v:.0}") }
                    }))
                    .changed()
                    && cc >= 0
                {
                    app.midi.selected = Some(Trigger::Control(cc as u8));
                }
            });
            binding_row(app, ui, &t);
            ui.separator();
            bindings_list(app, ui, &t);
        });
    if !open {
        app.midi.open = false;
        app.midi.learn = false;
    }
}

fn device_row(app: &mut ScoreApp, ui: &mut egui::Ui, t: &Tokens) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Device").strong());
        let current = app.host.midi_connected();
        let mut pick: Option<Option<String>> = None;
        egui::ComboBox::from_id_salt("midi-port")
            .selected_text(current.clone().unwrap_or_else(|| "Not connected".into()))
            .width(260.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), "Not connected").clicked() {
                    pick = Some(None);
                }
                for p in &app.midi.ports {
                    if ui.selectable_label(current.as_deref() == Some(p), p).clicked() {
                        pick = Some(Some(p.clone()));
                    }
                }
            });
        if ui
            .button("Refresh")
            .on_hover_text("Look for devices again (plug the device in first)")
            .clicked()
            || app.midi.ports.is_empty() && scan_due(ui, &mut app.midi.next_scan)
        {
            app.midi.ports = app.host.midi_ports();
            app.midi.error = if app.midi.ports.is_empty() {
                "No MIDI devices found.".into()
            } else {
                String::new()
            };
        }
        if let Some(port) = pick {
            match app.host.midi_open(port.as_deref()) {
                Ok(()) => {
                    app.midi.error.clear();
                    app.host.save_setting(PORT_SETTING, port.as_deref().unwrap_or(""));
                }
                Err(e) => app.midi.error = e,
            }
        }
        if !app.midi.error.is_empty() {
            ui.label(RichText::new(&app.midi.error).color(t.text_dim));
        }
    });
    ui.label(
        RichText::new("Played keys enter notes in note input (N); keys pressed together make a chord. Outside note input they only sound.")
            .small()
            .color(t.text_dim),
    );
}

/// An 88-key keyboard (A0–C8): bound keys are coloured, the selected one outlined.
fn keyboard(app: &mut ScoreApp, ui: &mut egui::Ui, t: &Tokens) {
    const LOW: u8 = 21;
    const HIGH: u8 = 108;
    let black = |k: u8| matches!(k % 12, 1 | 3 | 6 | 8 | 10);
    let whites = (LOW..=HIGH).filter(|k| !black(*k)).count() as f32;
    let width = ui.available_width().min(1000.0);
    let ww = width / whites;
    let h = 70.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, h), Sense::click());
    let painter = ui.painter_at(rect);
    let mut rects: Vec<(u8, Rect)> = Vec::new();
    let mut x = rect.left();
    for k in LOW..=HIGH {
        if !black(k) {
            rects.push((
                k,
                Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(ww, h)),
            ));
            x += ww;
        }
    }
    let mut blacks: Vec<(u8, Rect)> = Vec::new();
    for k in LOW..=HIGH {
        if black(k)
            && let Some((_, r)) = rects.iter().find(|(w, _)| *w == k - 1)
        {
            blacks.push((
                k,
                Rect::from_min_size(
                    egui::pos2(r.right() - ww * 0.3, rect.top()),
                    Vec2::new(ww * 0.6, h * 0.62),
                ),
            ));
        }
    }
    let selected = app.midi.selected;
    let fill = |k: u8, base: Color32| -> Color32 {
        match app.midi.map.binding(Trigger::Key(k)) {
            MidiAction::Note => base,
            MidiAction::Ignore => Color32::from_gray(150),
            MidiAction::Action(_) => t.accent,
        }
    };
    for (k, r) in &rects {
        painter.rect_filled(r.shrink(0.5), 1.0, fill(*k, Color32::WHITE));
        painter.rect_stroke(
            r.shrink(0.5),
            1.0,
            Stroke::new(0.6, Color32::from_gray(90)),
            egui::StrokeKind::Inside,
        );
        if k % 12 == 0 {
            painter.text(
                egui::pos2(r.center().x, r.bottom() - 3.0),
                egui::Align2::CENTER_BOTTOM,
                key_name(*k),
                egui::FontId::proportional(8.0),
                Color32::from_gray(80),
            );
        }
    }
    for (k, r) in &blacks {
        painter.rect_filled(*r, 1.0, fill(*k, Color32::from_gray(25)));
    }
    if let Some(Trigger::Key(sk)) = selected
        && let Some((_, r)) = blacks.iter().chain(rects.iter()).find(|(k, _)| *k == sk)
    {
        painter.rect_stroke(
            *r,
            1.0,
            Stroke::new(2.5, Color32::from_rgb(230, 120, 30)),
            egui::StrokeKind::Inside,
        );
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        // Black keys lie on top.
        let hit = blacks
            .iter()
            .chain(rects.iter())
            .find(|(_, r)| r.contains(p))
            .map(|(k, _)| *k);
        if let Some(k) = hit {
            app.midi.selected = Some(Trigger::Key(k));
            app.midi.learn = false;
        }
    }
}

/// The selected key or controller and the choice of what it does.
fn binding_row(app: &mut ScoreApp, ui: &mut egui::Ui, t: &Tokens) {
    let Some(sel) = app.midi.selected else {
        ui.label(RichText::new("No key selected.").color(t.text_dim));
        return;
    };
    let current = app.midi.map.binding(sel);
    let mut choice: Option<MidiAction> = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(sel.label()).strong());
        ui.label("does");
        egui::ComboBox::from_id_salt("midi-action")
            .selected_text(action_label(&current))
            .width(220.0)
            .height(420.0)
            .show_ui(ui, |ui| {
                if matches!(sel, Trigger::Key(_))
                    && ui
                        .selectable_label(current == MidiAction::Note, "Enter the note")
                        .clicked()
                {
                    choice = Some(MidiAction::Note);
                }
                if ui
                    .selectable_label(current == MidiAction::Ignore, "Nothing")
                    .clicked()
                {
                    choice = Some(MidiAction::Ignore);
                }
                ui.separator();
                for a in ACTIONS {
                    let this = MidiAction::Action(a.id.to_string());
                    if ui.selectable_label(current == this, a.label).clicked() {
                        choice = Some(this);
                    }
                }
            });
    });
    if let Some(c) = choice {
        app.midi.map.bind(sel, c);
        save(app);
    }
}

fn bindings_list(app: &mut ScoreApp, ui: &mut egui::Ui, t: &Tokens) {
    let mut rows: Vec<(Trigger, MidiAction)> = app
        .midi
        .map
        .keys
        .iter()
        .map(|(k, a)| (Trigger::Key(*k), a.clone()))
        .chain(
            app.midi
                .map
                .controls
                .iter()
                .map(|(c, a)| (Trigger::Control(*c), a.clone())),
        )
        .collect();
    rows.sort_by_key(|(t, _)| *t);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Assigned").strong());
        if !rows.is_empty() && ui.button("Reset all").clicked() {
            app.midi.map.keys.clear();
            app.midi.map.controls.clear();
            save(app);
        }
    });
    if rows.is_empty() {
        ui.label(RichText::new("Every key enters its note; controllers do nothing.").color(t.text_dim));
        return;
    }
    let mut remove: Option<Trigger> = None;
    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
        egui::Grid::new("midi-bindings")
            .striped(true)
            .num_columns(3)
            .show(ui, |ui| {
                for (trig, a) in &rows {
                    if ui.link(trig.label()).clicked() {
                        app.midi.selected = Some(*trig);
                    }
                    ui.label(action_label(a));
                    if ui
                        .small_button("✕")
                        .on_hover_text("Back to the default")
                        .clicked()
                    {
                        remove = Some(*trig);
                    }
                    ui.end_row();
                }
            });
    });
    if let Some(trig) = remove {
        let default = match trig {
            Trigger::Key(_) => MidiAction::Note,
            Trigger::Control(_) => MidiAction::Ignore,
        };
        app.midi.map.bind(trig, default);
        save(app);
    }
}
