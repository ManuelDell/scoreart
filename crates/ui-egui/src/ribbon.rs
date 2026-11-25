//! The ribbon: tabs of grouped commands across the top, as in the ArtCraft office apps.
//!
//! The surface can be the ribbon, the side panel with menus and toolbar, both, or a focus mode
//! with only a floating input bar. Tabs are ordered by how often they are needed: Home holds
//! note input; Notation the everyday marks (articulation, dynamics, slurs and hairpins); More
//! symbols the rare ones; then measures, text, instruments, format and view.

use egui::{Color32, Painter, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, Vec2};
use scoreart_model::marks::{self, MARKS, MarkDef};
use scoreart_model::{Clef, Dynamic, Jump, KeySig, NoteHead, NoteValue, SpannerKind};
use serde_json::{Value, json};

use crate::i18n::{t, tf};
use crate::icons::{self, Ico};
use crate::panels::{self, NewScore};
use crate::side::SideTab;
use crate::theme::Tokens;
use crate::{Dialog, ScoreApp, glyph};

/// What surrounds the score.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Ribbon,
    Side,
    Both,
    Focus,
}

impl Surface {
    pub const ALL: [Surface; 4] = [Surface::Ribbon, Surface::Side, Surface::Both, Surface::Focus];

    pub fn label(self) -> &'static str {
        match self {
            Surface::Ribbon => t("Ribbon"),
            Surface::Side => t("Side panel"),
            Surface::Both => t("Both"),
            Surface::Focus => t("Focus"),
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Surface::Ribbon => "ribbon",
            Surface::Side => "side",
            Surface::Both => "both",
            Surface::Focus => "focus",
        }
    }

    pub fn from_code(s: &str) -> Option<Surface> {
        Surface::ALL.into_iter().find(|x| x.code() == s.trim())
    }

    pub fn has_ribbon(self) -> bool {
        matches!(self, Surface::Ribbon | Surface::Both)
    }

    pub fn has_side(self) -> bool {
        matches!(self, Surface::Side | Surface::Both)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    File,
    Home,
    Notation,
    More,
    Measures,
    Text,
    Instruments,
    Format,
    View,
}

impl Tab {
    pub const ALL: [Tab; 9] = [
        Tab::File,
        Tab::Home,
        Tab::Notation,
        Tab::More,
        Tab::Measures,
        Tab::Text,
        Tab::Instruments,
        Tab::Format,
        Tab::View,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::File => t("File"),
            Tab::Home => t("Home"),
            Tab::Notation => t("Notation"),
            Tab::More => t("More symbols"),
            Tab::Measures => t("Measures"),
            Tab::Text => t("Text"),
            Tab::Instruments => t("Instruments"),
            Tab::Format => t("Format"),
            Tab::View => t("View"),
        }
    }
}

pub struct RibbonState {
    pub tab: Tab,
    /// Only the tab names show; clicking one opens it again.
    pub minimized: bool,
}

impl Default for RibbonState {
    fn default() -> Self {
        RibbonState {
            tab: Tab::Home,
            minimized: false,
        }
    }
}

const SURFACE_SETTING: &str = "surface";

/// Switches the surface and remembers it.
pub fn set_surface(app: &mut ScoreApp, s: Surface) {
    app.surface = s;
    if s.has_side() && app.side.is_none() {
        app.side = Some(SideTab::Palettes);
    }
    app.host.save_setting(SURFACE_SETTING, s.code());
}

/// The saved surface (Both when nothing is saved).
pub fn saved_surface(host: &dyn crate::Host) -> Surface {
    host.load_setting(SURFACE_SETTING)
        .and_then(|s| Surface::from_code(&s))
        .unwrap_or(Surface::Both)
}

// ---------------------------------------------------------------- symbols in galleries

/// Something a gallery cell shows and does.
#[derive(Clone, Copy)]
enum Sym {
    Mark(&'static MarkDef),
    Dyn(Dynamic),
    Line(SpannerKind),
    Grace(&'static str, &'static str),
    Head(NoteHead),
    Tremolo(u8),
    Buzz,
    Arp(&'static str, &'static str),
    Clef(Clef),
    Time(&'static str),
    Barline(&'static str, &'static str),
    RepeatStart,
    RepeatEnd,
    Volta(u8),
    OneBar,
    Jump(Jump),
}

impl Sym {
    fn tip(self) -> String {
        match self {
            Sym::Mark(d) => t(d.label).to_string(),
            Sym::Dyn(d) => d.id().to_string(),
            Sym::Line(k) => t(k.label()).to_string(),
            Sym::Grace(_, tip) | Sym::Arp(_, tip) | Sym::Barline(_, tip) => t(tip).to_string(),
            Sym::Head(h) => t(h.id()).to_string(),
            Sym::Tremolo(n) => tf("Tremolo, {} stroke(s)", &[&n]),
            Sym::Buzz => t("Buzz roll").to_string(),
            Sym::Clef(c) => t(c.label()).to_string(),
            Sym::Time(ts) => ts.to_string(),
            Sym::RepeatStart => t("Start repeat").to_string(),
            Sym::RepeatEnd => t("End repeat").to_string(),
            Sym::Volta(0) => t("Remove volta").to_string(),
            Sym::Volta(n) => tf("Volta {}", &[&n]),
            Sym::OneBar => t("One-measure repeat").to_string(),
            Sym::Jump(j) => t(j.id()).to_string(),
        }
    }

    fn command(self) -> (&'static str, Value) {
        match self {
            Sym::Mark(d) => ("note.articulation", json!({ "type": d.id })),
            Sym::Dyn(d) => ("note.dynamic", json!({ "type": d.id() })),
            Sym::Line(k) => ("line.add", json!({ "type": k.id() })),
            Sym::Grace(id, _) => ("note.grace", json!({ "type": id })),
            Sym::Head(h) => ("note.head", json!({ "type": h.id() })),
            Sym::Tremolo(n) => ("note.tremolo", json!({ "strokes": n })),
            Sym::Buzz => ("note.tremolo", json!({ "strokes": "buzz" })),
            Sym::Arp(id, _) => ("note.arpeggio", json!({ "type": id })),
            Sym::Clef(c) => ("measure.clef", json!({ "clef": c.id() })),
            Sym::Time(ts) => ("measure.time", json!({ "time": ts })),
            Sym::Barline(id, _) => ("measure.barline", json!({ "type": id })),
            Sym::RepeatStart => ("measure.repeat-start", json!({})),
            Sym::RepeatEnd => ("measure.barline", json!({ "type": "repeat-end" })),
            Sym::Volta(n) => ("measure.volta", json!({ "number": n })),
            Sym::OneBar => ("measure.repeat-sign", json!({})),
            Sym::Jump(j) => ("measure.jump", json!({ "type": j.id() })),
        }
    }

    fn paint(self, p: &Painter, r: Rect, c: Color32) {
        let text = |s: &str, italic: bool| {
            let mut job = egui::text::LayoutJob::default();
            job.append(
                s,
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::proportional(13.0),
                    color: c,
                    italics: italic,
                    ..Default::default()
                },
            );
            let galley = p.layout_job(job);
            p.galley(r.center() - galley.size() / 2.0, galley, c);
        };
        match self {
            Sym::Mark(d) => panels::paint_smufl(p, r, d.glyph_above, c),
            Sym::Dyn(d) => panels::paint_dynamic(p, r, d.id(), c),
            Sym::Line(k) => match k {
                SpannerKind::Slur => icons::paint(p, r.shrink(5.0), Ico::Slur, c),
                SpannerKind::Crescendo => icons::paint(p, r.shrink(5.0), Ico::Cresc, c),
                SpannerKind::Diminuendo => icons::paint(p, r.shrink(5.0), Ico::Dim, c),
                SpannerKind::CrescendoLine => text("cresc.", true),
                SpannerKind::DiminuendoLine => text("dim.", true),
                SpannerKind::Ottava(o) => panels::paint_smufl(
                    p,
                    r,
                    match o {
                        1 => "ottavaAlta",
                        -1 => "ottavaBassaVb",
                        2 => "quindicesimaAlta",
                        -2 => "quindicesimaBassa",
                        3 => "ventiduesimaAlta",
                        _ => "ventiduesimaBassa",
                    },
                    c,
                ),
                SpannerKind::Pedal => panels::paint_smufl(p, r, "keyboardPedalPed", c),
                SpannerKind::TrillLine => text("tr~~", true),
                SpannerKind::Vibrato => panels::paint_smufl(p, r, "wiggleVibrato", c),
                SpannerKind::LetRing => text("l.r.", true),
                SpannerKind::PalmMute => text("P.M.", true),
                SpannerKind::Glissando => {
                    p.line_segment(
                        [
                            r.left_top() + Vec2::new(8.0, 9.0),
                            r.right_bottom() - Vec2::new(8.0, 9.0),
                        ],
                        Stroke::new(1.6, c),
                    );
                }
                SpannerKind::WavyGlissando => {
                    let (a, b) = (
                        r.left_top() + Vec2::new(7.0, 9.0),
                        r.right_bottom() - Vec2::new(7.0, 9.0),
                    );
                    let n = (b - a).normalized();
                    let pts: Vec<Pos2> = (0..=20)
                        .map(|i| {
                            let f = i as f32 / 20.0;
                            a + (b - a) * f + n.rot90() * 2.2 * (f * std::f32::consts::TAU * 3.0).sin()
                        })
                        .collect();
                    p.add(egui::Shape::line(pts, Stroke::new(1.4, c)));
                }
            },
            Sym::Grace(id, _) => {
                let name = match id {
                    "acciaccatura" => "graceNoteAcciaccaturaStemUp",
                    "appoggiatura" => "graceNoteAppoggiaturaStemUp",
                    "grace-16th" => "note16thUp",
                    _ => "note32ndUp",
                };
                panels::paint_smufl(p, r, name, c);
            }
            Sym::Head(h) => {
                let (g, _) = scoreart_engrave::glyphs::head_for(h, NoteValue::Quarter);
                let name = scoreart_engrave::smufl::TABLE
                    .iter()
                    .find(|x| x.1 == g)
                    .map_or("noteheadBlack", |x| x.0);
                panels::paint_smufl(p, r, name, c);
            }
            Sym::Tremolo(n) => panels::paint_smufl(p, r, &format!("tremolo{n}"), c),
            Sym::Buzz => panels::paint_smufl(p, r, "buzzRoll", c),
            Sym::Arp(id, _) => paint_arpeggio(p, r, id, c),
            Sym::Clef(cl) => {
                let name = match cl {
                    Clef::Treble => "gClef",
                    Clef::Treble8vb => "gClef8vb",
                    Clef::Bass => "fClef",
                    Clef::Alto | Clef::Tenor => "cClef",
                    Clef::Percussion => "unpitchedPercussionClef1",
                };
                panels::paint_smufl(p, r, name, c);
            }
            Sym::Time(ts) => match ts {
                "common" => panels::paint_smufl(p, r, "timeSigCommon", c),
                "cut" => panels::paint_smufl(p, r, "timeSigCutCommon", c),
                _ => {
                    let (n, d) = ts.split_once('/').unwrap_or((ts, ""));
                    let f = egui::FontId::proportional(11.5);
                    p.text(
                        r.center() - Vec2::new(0.0, 6.0),
                        egui::Align2::CENTER_CENTER,
                        n,
                        f.clone(),
                        c,
                    );
                    p.text(
                        r.center() + Vec2::new(0.0, 6.0),
                        egui::Align2::CENTER_CENTER,
                        d,
                        f,
                        c,
                    );
                }
            },
            Sym::Barline(id, _) => {
                let x = r.center().x;
                let (y0, y1) = (r.top() + 7.0, r.bottom() - 7.0);
                let thin = Stroke::new(1.2, c);
                match id {
                    "double" => {
                        p.line_segment([Pos2::new(x - 2.0, y0), Pos2::new(x - 2.0, y1)], thin);
                        p.line_segment([Pos2::new(x + 2.0, y0), Pos2::new(x + 2.0, y1)], thin);
                    }
                    "final" => {
                        p.line_segment([Pos2::new(x - 3.0, y0), Pos2::new(x - 3.0, y1)], thin);
                        p.rect_filled(
                            Rect::from_min_max(Pos2::new(x, y0), Pos2::new(x + 3.5, y1)),
                            0.0,
                            c,
                        );
                    }
                    _ => {
                        p.line_segment([Pos2::new(x, y0), Pos2::new(x, y1)], thin);
                    }
                }
            }
            Sym::RepeatStart => panels::paint_smufl(p, r, "repeatLeft", c),
            Sym::RepeatEnd => panels::paint_smufl(p, r, "repeatRight", c),
            Sym::Volta(0) => text("⌐ ×", false),
            Sym::Volta(n) => text(&format!("⌐{n}."), false),
            Sym::OneBar => panels::paint_smufl(p, r, "repeat1Bar", c),
            Sym::Jump(j) => match j {
                Jump::Segno => panels::paint_smufl(p, r, "segno", c),
                Jump::Coda => panels::paint_smufl(p, r, "coda", c),
                Jump::VariedCoda => panels::paint_smufl(p, r, "codaSquare", c),
                _ => text(j.text().unwrap_or(""), true),
            },
        }
    }

    /// Wide cells for words (D.S. al Coda …).
    fn wide(self) -> bool {
        matches!(self, Sym::Jump(j) if !matches!(j, Jump::Segno | Jump::Coda | Jump::VariedCoda))
    }
}

/// A vertical wavy line (with an arrow head), or the bracket of a non-arpeggio.
fn paint_arpeggio(p: &Painter, r: Rect, id: &str, c: Color32) {
    let (top, bottom, x) = (r.top() + 6.0, r.bottom() - 6.0, r.center().x);
    let stroke = Stroke::new(1.4, c);
    if id == "non-arpeggio" {
        let pts = [
            Pos2::new(x + 3.0, top),
            Pos2::new(x - 2.0, top),
            Pos2::new(x - 2.0, bottom),
            Pos2::new(x + 3.0, bottom),
        ];
        p.add(egui::Shape::line(pts.to_vec(), stroke));
        return;
    }
    let pts: Vec<Pos2> = (0..=24)
        .map(|i| {
            let f = i as f32 / 24.0;
            Pos2::new(
                x + 2.5 * (f * std::f32::consts::TAU * 3.0).sin(),
                top + 2.0 + f * (bottom - top - 4.0),
            )
        })
        .collect();
    p.add(egui::Shape::line(pts, stroke));
    let head = |tip: Pos2, dir: f32| {
        p.line_segment([tip, tip + Vec2::new(-3.5, -4.0 * dir)], stroke);
        p.line_segment([tip, tip + Vec2::new(3.5, -4.0 * dir)], stroke);
    };
    match id {
        "arpeggio-up" => head(Pos2::new(x, top), -1.0),
        "arpeggio-down" => head(Pos2::new(x, bottom), 1.0),
        _ => {}
    }
}

fn marks_of(group: &str) -> Vec<Sym> {
    MARKS.iter().filter(|d| d.group == group).map(Sym::Mark).collect()
}

fn mark(id: &str) -> Option<Sym> {
    MARKS.iter().find(|d| d.id == id).map(Sym::Mark)
}

fn dyns(ids: &[&str]) -> Vec<Sym> {
    Dynamic::all()
        .filter(|d| ids.contains(&d.id()))
        .map(Sym::Dyn)
        .collect()
}

// ---------------------------------------------------------------- widgets

/// Whether a command can run now, and why not.
fn can(app: &ScoreApp, id: &str) -> Result<(), String> {
    match scoreart_engine::cmd::find(id) {
        Some(c) => (c.enabled)(&app.session),
        None => Ok(()),
    }
}

fn frame_for(ui: &Ui, resp: &Response, on: bool) -> (Color32, Color32) {
    let tk = Tokens::get(ui.ctx());
    let fg = if !resp.enabled() {
        ui.visuals().weak_text_color()
    } else if on {
        tk.accent
    } else {
        ui.visuals().text_color()
    };
    let bg = if on {
        tk.accent_soft
    } else if resp.hovered() && resp.enabled() {
        ui.visuals().widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    (fg, bg)
}

/// A large button: icon above, label below (two lines at most).
fn big(ui: &mut Ui, ico: Ico, label: &str, on: bool, enabled: bool, tip: &str) -> Response {
    let tk = Tokens::get(ui.ctx());
    // Wide enough for the longest word, so words never break in the middle.
    let font = egui::FontId::proportional(11.5);
    let longest = label
        .split_whitespace()
        .map(|w| {
            ui.painter()
                .layout_no_wrap(w.to_string(), font.clone(), Color32::WHITE)
                .size()
                .x
        })
        .fold(0.0f32, f32::max);
    let w = (longest + 12.0).clamp(58.0, 110.0);
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(w, 70.0),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    let resp = resp.on_hover_text(tip);
    let resp = if enabled { resp } else { disabled(resp) };
    let (fg, bg) = frame_for(ui, &resp, on);
    ui.painter().rect_filled(rect, 6.0, bg);
    if on {
        ui.painter().rect_stroke(
            rect,
            6.0,
            Stroke::new(1.0, tk.accent.linear_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
    }
    icons::paint(
        ui.painter(),
        Rect::from_center_size(rect.center_top() + Vec2::new(0.0, 22.0), Vec2::splat(28.0)),
        ico,
        fg,
    );
    let mut job = egui::text::LayoutJob::simple(label.to_string(), font, fg, rect.width() - 4.0);
    job.halign = egui::Align::Center;
    let galley = ui.painter().layout_job(job);
    ui.painter()
        .galley(Pos2::new(rect.center().x, rect.top() + 40.0), galley, fg);
    resp
}

fn disabled(resp: Response) -> Response {
    // Disabled widgets keep their tooltip but take no clicks.
    let mut r = resp;
    r.flags.remove(egui::response::Flags::ENABLED);
    r
}

/// A small button: icon (or none) and label in one row.
fn small(ui: &mut Ui, ico: Option<Ico>, label: &str, on: bool, enabled: bool, tip: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(12.5),
        Color32::WHITE,
    );
    let w = galley.size().x + if ico.is_some() { 30.0 } else { 12.0 };
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(w.max(24.0), 23.0),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    let resp = resp.on_hover_text(tip);
    let resp = if enabled { resp } else { disabled(resp) };
    let (fg, bg) = frame_for(ui, &resp, on);
    ui.painter().rect_filled(rect, 5.0, bg);
    let mut x = rect.left() + 6.0;
    if let Some(i) = ico {
        icons::paint(
            ui.painter(),
            Rect::from_min_size(Pos2::new(x, rect.center().y - 8.0), Vec2::splat(16.0)),
            i,
            fg,
        );
        x += 22.0;
    }
    ui.painter().text(
        Pos2::new(x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.5),
        fg,
    );
    resp
}

/// A square cell painted by `paint`.
fn cell(
    ui: &mut Ui,
    w: f32,
    on: bool,
    enabled: bool,
    tip: &str,
    paint: impl FnOnce(&Painter, Rect, Color32),
) -> Response {
    cell_sized(ui, Vec2::new(w, 32.0), on, enabled, tip, paint)
}

fn cell_sized(
    ui: &mut Ui,
    size: Vec2,
    on: bool,
    enabled: bool,
    tip: &str,
    paint: impl FnOnce(&Painter, Rect, Color32),
) -> Response {
    let tk = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let resp = resp.on_hover_text(tip);
    let resp = if enabled { resp } else { disabled(resp) };
    let (fg, bg) = frame_for(ui, &resp, on);
    ui.painter().rect_filled(rect, 5.0, bg);
    if on {
        ui.painter().rect_stroke(
            rect,
            5.0,
            Stroke::new(1.0, tk.accent.linear_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
    }
    paint(ui.painter(), rect, fg);
    resp
}

/// A group: its content above, its name below, a divider on the right.
fn group(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    let tk = Tokens::get(ui.ctx());
    ui.vertical(|ui| {
        ui.set_height(92.0);
        ui.horizontal(|ui| {
            ui.set_height(72.0);
            ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
            add(ui);
        });
        ui.add_space(2.0);
        let w = ui.min_rect().width();
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 14.0), Sense::hover());
        ui.painter().text(
            r.center(),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(11.0),
            tk.text_dim,
        );
    });
    let (r, _) = ui.allocate_exact_size(Vec2::new(9.0, 88.0), Sense::hover());
    ui.painter().line_segment(
        [
            Pos2::new(r.center().x, r.top() + 4.0),
            Pos2::new(r.center().x, r.bottom() - 4.0),
        ],
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
}

/// Runs a symbol's command.
fn run_sym(app: &mut ScoreApp, s: Sym) {
    let (id, p) = s.command();
    app.run(id, p);
}

/// A gallery: `rows` rows of `cols` cells, with ▾ opening all of `all`.
fn gallery(app: &mut ScoreApp, ui: &mut Ui, id: &str, shown: &[Sym], all: &[Sym], cols: usize) {
    let enabled_for = |app: &ScoreApp, s: Sym| can(app, s.command().0).is_ok();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
        for row in shown.chunks(cols.max(1)) {
            ui.horizontal(|ui| {
                for s in row {
                    let w = if s.wide() { 76.0 } else { 34.0 };
                    let en = enabled_for(app, *s);
                    let tip = match can(app, s.command().0) {
                        Ok(()) => s.tip(),
                        Err(why) => format!("{}\n{}", s.tip(), crate::i18n::tr(&why)),
                    };
                    if cell(ui, w, false, en, &tip, |p, r, c| s.paint(p, r, c)).clicked() {
                        run_sym(app, *s);
                    }
                }
            });
        }
    });
    if all.len() > shown.len() {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(16.0, 66.0), Sense::click());
        let resp = resp.on_hover_text(t("More…"));
        let (fg, bg) = frame_for(ui, &resp, false);
        ui.painter().rect_filled(rect, 4.0, bg);
        icons::paint(
            ui.painter(),
            Rect::from_center_size(rect.center(), Vec2::splat(14.0)),
            Ico::Expand,
            fg,
        );
        egui::Popup::menu(&resp)
            .id(egui::Id::unique(("gallery", id)))
            .show(|ui| {
                ui.set_max_width(9.0 * 38.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
                    for s in all {
                        let w = if s.wide() { 76.0 } else { 34.0 };
                        let en = enabled_for(app, *s);
                        if cell(ui, w, false, en, &s.tip(), |p, r, c| s.paint(p, r, c)).clicked() {
                            run_sym(app, *s);
                        }
                    }
                });
            });
    }
}

/// A command button: big or small, greyed out (with the reason) when it cannot run.
fn cmd_big(app: &mut ScoreApp, ui: &mut Ui, ico: Ico, label: &str, tip: &str, id: &str, p: Value) {
    let (en, tip) = match can(app, id) {
        Ok(()) => (true, tip.to_string()),
        Err(why) => (false, format!("{tip}\n{}", crate::i18n::tr(&why))),
    };
    if big(ui, ico, label, false, en, &tip).clicked() {
        app.run(id, p);
    }
}

fn cmd_small(app: &mut ScoreApp, ui: &mut Ui, ico: Option<Ico>, label: &str, tip: &str, id: &str, p: Value) {
    let (en, tip) = match can(app, id) {
        Ok(()) => (true, tip.to_string()),
        Err(why) => (false, format!("{tip}\n{}", crate::i18n::tr(&why))),
    };
    if small(ui, ico, label, false, en, &tip).clicked() {
        app.run(id, p);
    }
}

fn text_dialog(
    app: &mut ScoreApp,
    title: &'static str,
    command: &'static str,
    key: &'static str,
    value: &str,
) {
    app.dialog = Some(Dialog::Text {
        title,
        command,
        key,
        value: value.to_string(),
    });
}

// ---------------------------------------------------------------- title bar

/// The thin bar above the ribbon: quick commands, the file name and the surface switch.
pub fn title_bar(app: &mut ScoreApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    egui::Panel::top("title_bar")
        .exact_size(36.0)
        .frame(
            egui::Frame::NONE
                .fill(tk.toolbar)
                .inner_margin(egui::Margin::symmetric(10, 4)),
        )
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new("ScoreArt").strong().color(tk.accent));
                ui.add_space(8.0);
                let quick: [(Ico, &str, &str); 3] = [
                    (Ico::Save, "file.save", "Save    Ctrl+S"),
                    (Ico::Undo, "edit.undo", "Undo    Ctrl+Z"),
                    (Ico::Redo, "edit.redo", "Redo    Ctrl+Y"),
                ];
                for (ico, id, tip) in quick {
                    let en = id == "file.save" || can(app, id).is_ok();
                    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                    let resp = resp.on_hover_text(crate::i18n::keys(t(tip)));
                    let resp = if en { resp } else { disabled(resp) };
                    let (fg, bg) = frame_for(ui, &resp, false);
                    ui.painter().rect_filled(rect, 5.0, bg);
                    icons::paint(ui.painter(), rect.shrink(5.0), ico, fg);
                    if resp.clicked() {
                        if id == "file.save" {
                            app.save(false);
                        } else {
                            app.run(id, json!({}));
                        }
                    }
                }
                let playing = app.is_playing();
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                let resp = resp.on_hover_text(if playing {
                    t("Stop (Space)")
                } else {
                    t("Play from the selection (Space)")
                });
                let (fg, bg) = frame_for(ui, &resp, playing);
                ui.painter().rect_filled(rect, 5.0, bg);
                icons::paint(
                    ui.painter(),
                    rect.shrink(6.0),
                    if playing { Ico::Stop } else { Ico::Play },
                    fg,
                );
                if resp.clicked() {
                    app.toggle_playback();
                }
                let name = format!(
                    "{}{}",
                    app.session.file_name,
                    if app.session.dirty { " •" } else { "" }
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    surface_switch(app, ui);
                    if app.surface.has_ribbon() {
                        let min = app.ribbon.minimized;
                        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
                        let resp = resp.on_hover_text(crate::i18n::keys(if min {
                            t("Show the ribbon    Ctrl+F1")
                        } else {
                            t("Hide the ribbon    Ctrl+F1")
                        }));
                        let (fg, bg) = frame_for(ui, &resp, false);
                        ui.painter().rect_filled(rect, 5.0, bg);
                        icons::paint(
                            ui.painter(),
                            rect.shrink(5.0),
                            if min { Ico::Expand } else { Ico::Collapse },
                            fg,
                        );
                        if resp.clicked() {
                            app.ribbon.minimized = !min;
                        }
                    }
                    ui.add_space(ui.available_width() / 2.0 - 120.0);
                    ui.label(RichText::new(name).strong());
                });
            });
        });
}

/// Ribbon | Side panel | Both | Focus.
pub fn surface_switch(app: &mut ScoreApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    // Right to left: the last one first.
    for s in Surface::ALL.into_iter().rev() {
        let on = app.surface == s;
        let galley = ui.painter().layout_no_wrap(
            s.label().to_string(),
            egui::FontId::proportional(12.0),
            Color32::WHITE,
        );
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(galley.size().x + 34.0, 26.0), Sense::click());
        let resp = resp.on_hover_text(tf("Interface: {}", &[&s.label()]));
        let (fg, bg) = frame_for(ui, &resp, on);
        ui.painter().rect_filled(rect, 5.0, bg);
        mini_surface(
            ui.painter(),
            Rect::from_min_size(rect.left_center() + Vec2::new(6.0, -6.0), Vec2::new(17.0, 12.0)),
            s,
            fg,
        );
        ui.painter().text(
            rect.left_center() + Vec2::new(28.0, 0.0),
            egui::Align2::LEFT_CENTER,
            s.label(),
            egui::FontId::proportional(12.0),
            fg,
        );
        if resp.clicked() {
            set_surface(app, s);
        }
    }
    let _ = tk;
}

/// A little picture of a surface.
fn mini_surface(p: &Painter, r: Rect, s: Surface, c: Color32) {
    p.rect_stroke(r, 2.0, Stroke::new(1.2, c), egui::StrokeKind::Inside);
    let soft = c.linear_multiply(0.6);
    let top = Rect::from_min_size(r.min, Vec2::new(r.width(), r.height() * 0.3));
    let left = Rect::from_min_max(
        Pos2::new(
            r.left(),
            r.top() + if s == Surface::Both { r.height() * 0.3 } else { 0.0 },
        ),
        Pos2::new(r.left() + r.width() * 0.3, r.bottom()),
    );
    match s {
        Surface::Ribbon => {
            p.rect_filled(top, 1.0, soft);
        }
        Surface::Side => {
            p.rect_filled(left, 1.0, soft);
        }
        Surface::Both => {
            p.rect_filled(top, 1.0, soft);
            p.rect_filled(left, 1.0, soft);
        }
        Surface::Focus => {
            let bar = Rect::from_center_size(
                Pos2::new(r.center().x, r.bottom() - r.height() * 0.22),
                Vec2::new(r.width() * 0.5, r.height() * 0.18),
            );
            p.rect_filled(bar, 1.0, soft);
        }
    }
}

// ---------------------------------------------------------------- the ribbon

pub fn ribbon(app: &mut ScoreApp, ui: &mut Ui) {
    let tk = Tokens::get(ui.ctx());
    egui::Panel::top("ribbon")
        .frame(egui::Frame::NONE.fill(tk.panel).inner_margin(egui::Margin {
            left: 8,
            right: 8,
            top: 0,
            bottom: 4,
        }))
        .show(ui, |ui| {
            // Tabs.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for tab in Tab::ALL {
                    let on = app.ribbon.tab == tab && !app.ribbon.minimized;
                    let galley = ui.painter().layout_no_wrap(
                        tab.label().to_string(),
                        egui::FontId::proportional(13.5),
                        Color32::WHITE,
                    );
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(galley.size().x + 22.0, 30.0), Sense::click());
                    let fg = if app.ribbon.tab == tab {
                        tk.accent
                    } else {
                        ui.visuals().text_color()
                    };
                    if resp.hovered() {
                        ui.painter().rect_filled(
                            rect.shrink2(Vec2::new(0.0, 3.0)),
                            5.0,
                            ui.visuals().widgets.hovered.weak_bg_fill,
                        );
                    }
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        tab.label(),
                        egui::FontId::proportional(13.5),
                        fg,
                    );
                    if on {
                        ui.painter().rect_filled(
                            Rect::from_min_max(
                                Pos2::new(rect.left() + 8.0, rect.bottom() - 3.0),
                                Pos2::new(rect.right() - 8.0, rect.bottom()),
                            ),
                            1.5,
                            tk.accent,
                        );
                    }
                    if resp.double_clicked() {
                        app.ribbon.minimized = !app.ribbon.minimized;
                    } else if resp.clicked() {
                        app.ribbon.tab = tab;
                        app.ribbon.minimized = false;
                    }
                }
            });
            if app.ribbon.minimized {
                return;
            }
            ui.separator();
            egui::ScrollArea::horizontal()
                .id_salt("ribbon-scroll")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        match app.ribbon.tab {
                            Tab::File => tab_file(app, ui),
                            Tab::Home => tab_home(app, ui),
                            Tab::Notation => tab_notation(app, ui),
                            Tab::More => tab_more(app, ui),
                            Tab::Measures => tab_measures(app, ui),
                            Tab::Text => tab_text(app, ui),
                            Tab::Instruments => tab_instruments(app, ui),
                            Tab::Format => tab_format(app, ui),
                            Tab::View => tab_view(app, ui),
                        }
                    });
                });
        });
}

fn column(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(2.0, 1.0);
        add(ui);
    });
}

fn tab_file(app: &mut ScoreApp, ui: &mut Ui) {
    let embedded = app.host.embedded();
    group(ui, t("Score"), |ui| {
        if !embedded {
            if big(
                ui,
                Ico::New,
                t("New"),
                false,
                true,
                &crate::i18n::keys(t("New score…    Ctrl+N")),
            )
            .clicked()
            {
                app.dialog = Some(Dialog::NewScore(NewScore::default()));
            }
            if big(
                ui,
                Ico::Open,
                t("Open"),
                false,
                true,
                &crate::i18n::keys(t("Open…    Ctrl+O")),
            )
            .clicked()
            {
                app.host.request_open();
            }
        }
        if big(
            ui,
            Ico::Save,
            t("Save"),
            false,
            true,
            &crate::i18n::keys(t("Save    Ctrl+S")),
        )
        .clicked()
        {
            app.save(false);
        }
        if !embedded
            && big(
                ui,
                Ico::SaveAs,
                t("Save as"),
                false,
                true,
                &crate::i18n::keys(t("Save as…    Ctrl+Shift+S")),
            )
            .clicked()
        {
            app.save(true);
        }
    });
    group(ui, t("Export"), |ui| {
        let formats = panels::export_formats();
        for chunk in formats.chunks(3) {
            column(ui, |ui| {
                for (label, ext) in chunk {
                    if small(ui, Some(Ico::Export), label, false, true, label).clicked() {
                        panels::export(app, ext);
                    }
                }
            });
        }
    });
    if !embedded {
        group(ui, t("Examples"), |ui| {
            if big(
                ui,
                Ico::Sample,
                t("Ode to Joy"),
                false,
                true,
                t("Open sample (Ode to Joy)"),
            )
            .clicked()
            {
                app.session = scoreart_engine::Session::default();
                app.fields = panels::Fields::default();
            }
        });
    }
}

fn tab_home(app: &mut ScoreApp, ui: &mut Ui) {
    let input = app.session.input;
    group(ui, t("Input"), |ui| {
        if big(
            ui,
            Ico::Pen,
            t("Pen"),
            input.active,
            true,
            &crate::i18n::keys(t("Pen: click to write notes (N)")),
        )
        .clicked()
        {
            app.set_pen(true);
        }
        if big(
            ui,
            Ico::Hand,
            t("Hand"),
            !input.active,
            true,
            &crate::i18n::keys(t("Hand: select, edit and drag notes and marks (Esc)")),
        )
        .clicked()
        {
            app.set_pen(false);
        }
        column(ui, |ui| {
            if small(
                ui,
                Some(Ico::Keys),
                t("MIDI keyboard…"),
                app.host.midi_connected().is_some(),
                true,
                t("Connect a MIDI keyboard and set what each key does"),
            )
            .clicked()
            {
                app.open_midi();
            }
            if small(
                ui,
                Some(Ico::Speaker),
                t("Hear notes"),
                app.sound_on_input,
                true,
                t("Play notes while editing"),
            )
            .clicked()
            {
                app.sound_on_input = !app.sound_on_input;
            }
        });
    });
    group(ui, t("Duration"), |ui| {
        column(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for v in [
                    NoteValue::N64th,
                    NoteValue::N32nd,
                    NoteValue::N16th,
                    NoteValue::Eighth,
                    NoteValue::Quarter,
                    NoteValue::Half,
                    NoteValue::Whole,
                    NoteValue::Breve,
                ] {
                    let on = input.value == v && (input.active || app.session.selection.is_none());
                    let tip = format!("{} ({})", t(v.xml_name()), v.digit());
                    let g = panels::duration_glyph(v);
                    if cell(ui, 30.0, on, true, &tip, |p, r, c| {
                        glyph::paint(p, g, Pos2::new(r.center().x - 4.0, r.bottom() - 9.0), 24.0, c);
                    })
                    .clicked()
                    {
                        app.run("input.duration", json!({ "value": v.digit() }));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if cell(
                    ui,
                    30.0,
                    input.dots > 0,
                    true,
                    t("Augmentation dot (.)"),
                    |p, r, c| {
                        p.circle_filled(r.center(), 2.6, c);
                    },
                )
                .clicked()
                {
                    app.run("input.dot", json!({}));
                }
                if cell(ui, 30.0, false, true, t("Rest (0)"), |p, r, c| {
                    glyph::paint(
                        p,
                        scoreart_engrave::glyphs::REST_QUARTER,
                        Pos2::new(r.center().x - 4.0, r.center().y + 1.0),
                        24.0,
                        c,
                    );
                })
                .clicked()
                {
                    app.run("note.rest", json!({}));
                }
                cmd_small(
                    app,
                    ui,
                    Some(Ico::Tie),
                    t("Tie"),
                    &crate::i18n::keys(t("Tie (T)")),
                    "note.tie",
                    json!({}),
                );
            });
        });
    });
    group(ui, t("Pitch"), |ui| {
        column(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (name, id, tip) in [
                    ("accidentalDoubleFlat", "double-flat", "Double flat"),
                    ("accidentalFlat", "flat", "Flat"),
                    ("accidentalNatural", "natural", "Natural"),
                    ("accidentalSharp", "sharp", "Sharp"),
                    ("accidentalDoubleSharp", "double-sharp", "Double sharp"),
                ] {
                    let en = can(app, "note.accidental").is_ok();
                    if cell(ui, 30.0, false, en, t(tip), |p, r, c| {
                        panels::paint_smufl(p, r, name, c)
                    })
                    .clicked()
                    {
                        app.run("note.accidental", json!({ "type": id }));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (ico, id, tip) in [
                    (Ico::Up, "note.pitch-up", "Pitch up    ↑"),
                    (Ico::Down, "note.pitch-down", "Pitch down    ↓"),
                    (Ico::OctaveUp, "note.octave-up", "Octave up    Ctrl+↑"),
                    (Ico::OctaveDown, "note.octave-down", "Octave down    Ctrl+↓"),
                ] {
                    let en = can(app, id).is_ok();
                    if cell(ui, 30.0, false, en, &crate::i18n::keys(t(tip)), |p, r, c| {
                        icons::paint(p, r.shrink(7.0), ico, c)
                    })
                    .clicked()
                    {
                        app.run(id, json!({}));
                    }
                }
            });
        });
    });
    group(ui, t("Voices"), |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
            for row in [[0usize, 1], [2, 3]] {
                ui.horizontal(|ui| {
                    for v in row {
                        let c = crate::canvas::voice_color(v);
                        let on = input.voice == v;
                        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
                        let resp = resp.on_hover_text(tf("Voice {} (Ctrl+Alt+{})", &[&(v + 1), &(v + 1)]));
                        ui.painter().rect_filled(
                            rect.shrink(2.0),
                            5.0,
                            if on { c } else { c.linear_multiply(0.18) },
                        );
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            (v + 1).to_string(),
                            egui::FontId::proportional(13.0),
                            if on { Color32::WHITE } else { c },
                        );
                        if resp.clicked() {
                            if !input.active && app.session.selected_event().is_some() {
                                app.run("note.voice", json!({ "voice": v + 1 }));
                            } else {
                                app.run("input.voice", json!({ "voice": v + 1 }));
                            }
                        }
                    }
                });
            }
        });
    });
    group(ui, t("Edit"), |ui| {
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                Some(Ico::Undo),
                t("Undo"),
                &crate::i18n::keys(t("Undo    Ctrl+Z")),
                "edit.undo",
                json!({}),
            );
            cmd_small(
                app,
                ui,
                Some(Ico::Redo),
                t("Redo"),
                &crate::i18n::keys(t("Redo    Ctrl+Y")),
                "edit.redo",
                json!({}),
            );
            cmd_small(
                app,
                ui,
                Some(Ico::Delete),
                t("Delete"),
                &crate::i18n::keys(t("Delete    Del")),
                "note.delete",
                json!({}),
            );
        });
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                Some(Ico::SelectAll),
                t("Select all"),
                &crate::i18n::keys(t("Select all    Ctrl+A")),
                "select.all",
                json!({}),
            );
        });
    });
    group(ui, t("Playback"), |ui| {
        let playing = app.is_playing();
        if big(
            ui,
            if playing { Ico::Stop } else { Ico::Play },
            if playing { t("Stop") } else { t("Play") },
            playing,
            true,
            &crate::i18n::keys(t("Play from the selection (Space)")),
        )
        .clicked()
        {
            app.toggle_playback();
        }
        if big(ui, Ico::Mixer, t("Mixer"), app.show_mixer, true, "F10").clicked() {
            app.show_mixer = !app.show_mixer;
        }
    });
}

fn tab_notation(app: &mut ScoreApp, ui: &mut Ui) {
    let common: Vec<Sym> = [
        "accent",
        "staccato",
        "tenuto",
        "marcato",
        "staccatissimo",
        "portato",
        "fermata",
    ]
    .iter()
    .filter_map(|id| mark(id))
    .collect();
    let rest_art: Vec<Sym> = marks_of(marks::ARTICULATIONS);
    group(ui, t("Articulation"), |ui| {
        let mut all = common.clone();
        all.extend(
            rest_art
                .iter()
                .copied()
                .filter(|s| !common.iter().any(|c| c.tip() == s.tip())),
        );
        gallery(app, ui, "art", &common, &all, 4);
    });
    group(ui, t("Dynamics"), |ui| {
        let shown = dyns(&["pp", "p", "mp", "mf", "f", "ff", "sf", "sfz", "fp", "fz"]);
        let all: Vec<Sym> = Dynamic::all().map(Sym::Dyn).collect();
        gallery(app, ui, "dyn", &shown, &all, 5);
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                Some(Ico::Close),
                t("Remove"),
                t("Remove dynamic"),
                "note.dynamic",
                json!({ "type": "none" }),
            );
        });
    });
    group(ui, t("Slurs & hairpins"), |ui| {
        let shown = [
            Sym::Line(SpannerKind::Slur),
            Sym::Line(SpannerKind::Crescendo),
            Sym::Line(SpannerKind::Diminuendo),
            Sym::Line(SpannerKind::CrescendoLine),
            Sym::Line(SpannerKind::DiminuendoLine),
        ];
        gallery(app, ui, "hair", &shown[..3], &shown[..3], 3);
        column(ui, |ui| {
            for s in &shown[3..] {
                let en = can(app, "line.add").is_ok();
                if cell(ui, 52.0, false, en, &s.tip(), |p, r, c| s.paint(p, r, c)).clicked() {
                    run_sym(app, *s);
                }
            }
        });
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                Some(Ico::Close),
                t("Remove"),
                t("Remove lines starting or ending at the selection"),
                "line.remove",
                json!({}),
            );
        });
    });
    group(ui, t("Breath"), |ui| {
        let shown: Vec<Sym> = ["breath", "caesura"].iter().filter_map(|id| mark(id)).collect();
        gallery(app, ui, "breath", &shown, &shown, 1);
    });
    group(ui, t("Grace notes"), |ui| {
        let shown = [
            Sym::Grace("acciaccatura", "Grace note"),
            Sym::Grace("appoggiatura", "Grace note"),
        ];
        gallery(app, ui, "grace", &shown, &shown, 1);
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                Some(Ico::Close),
                t("Remove"),
                t("Remove grace notes"),
                "note.grace-remove",
                json!({}),
            );
        });
    });
}

fn tab_more(app: &mut ScoreApp, ui: &mut Ui) {
    let common = [
        "accent",
        "staccato",
        "tenuto",
        "marcato",
        "staccatissimo",
        "portato",
    ];
    let art: Vec<Sym> = MARKS
        .iter()
        .filter(|d| d.group == marks::ARTICULATIONS && !common.contains(&d.id))
        .map(Sym::Mark)
        .collect();
    let ferm: Vec<Sym> = MARKS
        .iter()
        .filter(|d| d.group == marks::FERMATAS && d.id != "fermata")
        .map(Sym::Mark)
        .collect();
    group(ui, t("Articulations"), |ui| {
        gallery(app, ui, "more-art", &art[..art.len().min(6)], &art, 3);
    });
    group(ui, t("Fermatas"), |ui| {
        gallery(app, ui, "more-ferm", &ferm[..ferm.len().min(6)], &ferm, 3);
    });
    let orn = marks_of(marks::ORNAMENTS);
    group(ui, t("Ornaments"), |ui| {
        gallery(app, ui, "orn", &orn[..orn.len().min(8)], &orn, 4);
    });
    let tech = marks_of(marks::TECHNIQUES);
    group(ui, t("Techniques"), |ui| {
        gallery(app, ui, "tech", &tech[..tech.len().min(6)], &tech, 3);
    });
    let br: Vec<Sym> = MARKS
        .iter()
        .filter(|d| d.group == marks::BREATHS)
        .map(Sym::Mark)
        .collect();
    group(ui, t("Breaths & pauses"), |ui| {
        gallery(app, ui, "br", &br[..br.len().min(4)], &br, 2);
    });
    let sl = marks_of(marks::SLIDES);
    group(ui, t("Slides"), |ui| {
        gallery(app, ui, "sl", &sl, &sl, 2);
    });
    let lines: Vec<Sym> = SpannerKind::ALL
        .into_iter()
        .filter(|k| {
            !matches!(
                k,
                SpannerKind::Slur
                    | SpannerKind::Crescendo
                    | SpannerKind::Diminuendo
                    | SpannerKind::CrescendoLine
                    | SpannerKind::DiminuendoLine
            )
        })
        .map(Sym::Line)
        .collect();
    group(ui, t("Lines"), |ui| {
        gallery(app, ui, "lines", &lines[..lines.len().min(8)], &lines, 4);
    });
    let rare_dyn: Vec<Sym> = Dynamic::all()
        .filter(|d| !["pp", "p", "mp", "mf", "f", "ff", "sf", "sfz", "fp", "fz"].contains(&d.id()))
        .map(Sym::Dyn)
        .collect();
    group(ui, t("Dynamics"), |ui| {
        gallery(
            app,
            ui,
            "rare-dyn",
            &rare_dyn[..rare_dyn.len().min(6)],
            &rare_dyn,
            3,
        );
    });
    let trem = [
        Sym::Tremolo(1),
        Sym::Tremolo(2),
        Sym::Tremolo(3),
        Sym::Tremolo(4),
        Sym::Buzz,
        Sym::Arp("arpeggio", "Arpeggio"),
        Sym::Arp("arpeggio-up", "Arpeggio up"),
        Sym::Arp("arpeggio-down", "Arpeggio down"),
        Sym::Arp("non-arpeggio", "Non-arpeggio bracket"),
    ];
    group(ui, t("Tremolo & arpeggio"), |ui| {
        gallery(app, ui, "trem", &trem[..6], &trem, 3);
    });
    let heads: Vec<Sym> = NoteHead::ALL.into_iter().map(Sym::Head).collect();
    group(ui, t("Noteheads"), |ui| {
        gallery(app, ui, "heads", &heads[..heads.len().min(6)], &heads, 3);
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                None,
                "( )",
                t("Parentheses (ghost note)"),
                "note.paren",
                json!({}),
            );
        });
    });
    let grace = [
        Sym::Grace("grace-16th", "Grace note"),
        Sym::Grace("grace-32nd", "Grace note"),
    ];
    group(ui, t("Grace notes"), |ui| {
        gallery(app, ui, "grace2", &grace, &grace, 1);
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                Some(Ico::Up),
                t("Up"),
                t("Move grace note up"),
                "note.grace-pitch",
                json!({ "delta": 1 }),
            );
            cmd_small(
                app,
                ui,
                Some(Ico::Down),
                t("Down"),
                t("Move grace note down"),
                "note.grace-pitch",
                json!({ "delta": -1 }),
            );
        });
    });
}

fn tab_measures(app: &mut ScoreApp, ui: &mut Ui) {
    group(ui, t("Measures"), |ui| {
        cmd_big(
            app,
            ui,
            Ico::MeasurePlus,
            t("Append"),
            &crate::i18n::keys(t("Append one measure    Ctrl+B")),
            "measure.append",
            json!({}),
        );
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                Some(Ico::MeasurePlus),
                t("Append four"),
                t("Append four measures"),
                "measure.append",
                json!({ "count": 4 }),
            );
            cmd_small(
                app,
                ui,
                Some(Ico::MeasureInsert),
                t("Insert before"),
                &crate::i18n::keys(t("Insert before selection    Ins")),
                "measure.insert",
                json!({}),
            );
            cmd_small(
                app,
                ui,
                Some(Ico::MeasureMinus),
                t("Delete"),
                &crate::i18n::keys(t("Delete selected measures    Ctrl+Del")),
                "measure.delete",
                json!({}),
            );
        });
        column(ui, |ui| {
            let en = can(app, "measure.length").is_ok();
            if small(
                ui,
                Some(Ico::Spacing),
                t("Length / pickup…"),
                false,
                en,
                t("A pickup (upbeat) or a measure shorter or longer than its time signature"),
            )
            .clicked()
            {
                panels::open_measure_length(app);
            }
        });
    });
    let clefs: Vec<Sym> = Clef::ALL.into_iter().map(Sym::Clef).collect();
    group(ui, t("Clef"), |ui| {
        gallery(app, ui, "clefs", &clefs[..4], &clefs, 2)
    });
    group(ui, t("Key"), |ui| {
        let en = can(app, "measure.key").is_ok();
        let r = big(
            ui,
            Ico::Transpose,
            t("Key signature"),
            false,
            en,
            t("Key signatures"),
        );
        egui::Popup::menu(&r)
            .id(egui::Id::unique("key-popup"))
            .show(|ui| {
                egui::Grid::new("keys")
                    .num_columns(3)
                    .spacing([4.0, 4.0])
                    .show(ui, |ui| {
                        for (i, f) in (-7..=7i32).enumerate() {
                            let k = KeySig::new(f);
                            let label = format!(
                                "{}  ({})",
                                tf("{} major", &[&crate::i18n::note_name(k.name())]),
                                panels::key_label(f)
                            );
                            if ui.button(label).clicked() {
                                app.run("measure.key", json!({ "fifths": f }));
                                ui.close();
                            }
                            if i % 3 == 2 {
                                ui.end_row();
                            }
                        }
                    });
            });
    });
    let times = ["4/4", "3/4", "2/4", "6/8", "common", "cut"].map(Sym::Time);
    let all_times = [
        "4/4", "3/4", "2/4", "6/8", "common", "cut", "5/4", "3/8", "9/8", "12/8", "2/2", "7/8",
    ]
    .map(Sym::Time);
    group(ui, t("Time"), |ui| {
        gallery(app, ui, "times", &times, &all_times, 3)
    });
    let bars = [
        Sym::Barline("double", "Double barline"),
        Sym::Barline("final", "Final barline"),
        Sym::Barline("regular", "Regular barline"),
    ];
    group(ui, t("Barlines"), |ui| gallery(app, ui, "bars", &bars, &bars, 2));
    let reps = [
        Sym::RepeatStart,
        Sym::RepeatEnd,
        Sym::OneBar,
        Sym::Volta(1),
        Sym::Volta(2),
        Sym::Volta(0),
    ];
    let all_reps = [
        Sym::RepeatStart,
        Sym::RepeatEnd,
        Sym::OneBar,
        Sym::Volta(1),
        Sym::Volta(2),
        Sym::Volta(0),
        Sym::Volta(3),
    ];
    group(ui, t("Repeats"), |ui| {
        gallery(app, ui, "reps", &reps, &all_reps, 3);
        column(ui, |ui| {
            let en = can(app, "measure.volta").is_ok();
            if small(
                ui,
                Some(Ico::Text),
                t("Volta label…"),
                false,
                en,
                t("The label of the volta, e.g. \"1.+3.\" or \"2.4.5.\" — it is played on those passes"),
            )
            .clicked()
            {
                text_dialog(app, t("Volta label (e.g. 1.+3.)"), "measure.volta", "text", "");
            }
        });
    });
    let jumps: Vec<Sym> = Jump::ALL.into_iter().map(Sym::Jump).collect();
    group(ui, t("Jumps"), |ui| {
        gallery(app, ui, "jumps", &jumps[..3], &jumps, 3)
    });
    group(ui, t("Breaks"), |ui| {
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                Some(Ico::LineBreak),
                t("Line break"),
                t("Line break after the measure (toggle)"),
                "measure.break",
                json!({ "kind": "line" }),
            );
            cmd_small(
                app,
                ui,
                Some(Ico::PageBreak),
                t("Page break"),
                t("Page break after the measure (toggle)"),
                "measure.break",
                json!({ "kind": "page" }),
            );
            cmd_small(
                app,
                ui,
                Some(Ico::SectionBreak),
                t("Section break"),
                t(
                    "Section break after the measure: new system, pause in playback, measure numbers start again (toggle)",
                ),
                "measure.break",
                json!({ "kind": "section" }),
            );
        });
        column(ui, |ui| {
            for (ico, label, tip, kind) in [
                (
                    Ico::SpacerUp,
                    t("Spacer up"),
                    t("More room above the staff in the selected measures"),
                    "up",
                ),
                (
                    Ico::SpacerDown,
                    t("Spacer down"),
                    t("More room below the staff in the selected measures"),
                    "down",
                ),
                (
                    Ico::SpacerFixed,
                    t("Fixed spacer"),
                    t("Exact distance below the staff in the selected measures"),
                    "fixed",
                ),
            ] {
                cmd_small(
                    app,
                    ui,
                    Some(ico),
                    label,
                    tip,
                    "staff.spacer",
                    json!({ "type": kind }),
                );
            }
        });
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                None,
                t("Remove spacers"),
                t("Remove all spacers of the selected measures"),
                "staff.spacer",
                json!({ "type": "none" }),
            );
            let sec = crate::panels::section_at_focus(app);
            if small(
                ui,
                None,
                t("Section…"),
                false,
                sec.is_some(),
                t("Settings of the section break: pause, instrument names, measure numbers"),
            )
            .clicked()
            {
                app.dialog = sec.map(Dialog::Section);
            }
            let m = app.session.focus().2;
            let frame = app.session.score.measures.get(m).and_then(|i| i.frame_after);
            if small(
                ui,
                None,
                t("Blank space…"),
                frame.is_some(),
                true,
                t("Free room after the system of the measure: pushes what follows down or onto the next page"),
            )
            .clicked()
            {
                app.dialog = Some(Dialog::Frame {
                    height: frame.unwrap_or(10.0),
                    existing: frame.is_some(),
                });
            }
        });
    });
}

fn tab_text(app: &mut ScoreApp, ui: &mut Ui) {
    group(ui, t("Notes"), |ui| {
        let en = can(app, "note.lyric").is_ok();
        if big(
            ui,
            Ico::Lyrics,
            t("Lyrics"),
            false,
            en,
            &crate::i18n::keys(t("Lyric…    Ctrl+L")),
        )
        .clicked()
        {
            panels::open_lyric_dialog(app);
        }
        let en = can(app, "note.chord-symbol").is_ok();
        if big(
            ui,
            Ico::Chord,
            t("Chord symbol"),
            false,
            en,
            &crate::i18n::keys(t("Chord symbol (e.g. Cmaj7, F#m7/C#)")),
        )
        .clicked()
        {
            text_dialog(
                app,
                t("Chord symbol (e.g. Cmaj7, F#m7/C#)"),
                "note.chord-symbol",
                "text",
                "",
            );
        }
        column(ui, |ui| {
            let en = can(app, "note.text").is_ok();
            if small(
                ui,
                Some(Ico::Text),
                t("Staff text…"),
                false,
                en,
                &crate::i18n::keys(t("Staff text")),
            )
            .clicked()
            {
                text_dialog(app, t("Staff text"), "note.text", "text", "");
            }
            let en = can(app, "note.fingering").is_ok();
            if small(
                ui,
                Some(Ico::Fingering),
                t("Fingering…"),
                false,
                en,
                t("Fingering"),
            )
            .clicked()
            {
                text_dialog(app, t("Fingering"), "note.fingering", "text", "");
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for f in ["1", "2", "3", "4", "5"] {
                    if cell_sized(ui, Vec2::splat(21.0), false, en, t("Fingering"), |p, r, c| {
                        p.text(
                            r.center(),
                            egui::Align2::CENTER_CENTER,
                            f,
                            egui::FontId::proportional(12.0),
                            c,
                        );
                    })
                    .clicked()
                    {
                        app.run("note.fingering", json!({ "text": f }));
                    }
                }
            });
        });
    });
    group(ui, t("Measures"), |ui| {
        let en = can(app, "measure.rehearsal").is_ok();
        if big(
            ui,
            Ico::Rehearsal,
            t("Rehearsal mark"),
            false,
            en,
            &crate::i18n::keys("Ctrl+M"),
        )
        .clicked()
        {
            app.run("measure.rehearsal", json!({}));
        }
        column(ui, |ui| {
            let en = can(app, "measure.text").is_ok();
            if small(
                ui,
                Some(Ico::Text),
                t("System text…"),
                false,
                en,
                t("System text"),
            )
            .clicked()
            {
                text_dialog(app, t("System text"), "measure.text", "text", "");
            }
        });
    });
    group(ui, t("Tempo"), |ui| {
        let en = can(app, "measure.tempo").is_ok();
        let r = big(
            ui,
            Ico::Tempo,
            t("Tempo"),
            false,
            en,
            t("Tempo (beats per minute)"),
        );
        egui::Popup::menu(&r)
            .id(egui::Id::unique("tempo-popup"))
            .show(|ui| {
                let unit = panels::beat_unit_here(app);
                let presets: &[u16] = if unit == scoreart_model::BeatUnit::DottedQuarter {
                    &[40, 46, 52, 56, 60, 66, 72, 80, 88, 96, 104, 112]
                } else {
                    &[40, 60, 72, 80, 92, 100, 108, 120, 132, 144, 160, 176]
                };
                for bpm in presets {
                    if ui
                        .button(format!("{} = {bpm}", panels::unit_symbol(unit)))
                        .clicked()
                    {
                        app.run("measure.tempo", json!({ "bpm": bpm, "unit": unit.id() }));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(t("Tempo…")).clicked() {
                    panels::open_tempo(app);
                    ui.close();
                }
            });
        column(ui, |ui| {
            let en = can(app, "measure.tempo-text").is_ok();
            if small(
                ui,
                Some(Ico::Text),
                t("Tempo marking…"),
                false,
                en,
                t("Tempo marking (e.g. Allegro)"),
            )
            .clicked()
            {
                text_dialog(
                    app,
                    t("Tempo marking (e.g. Allegro)"),
                    "measure.tempo-text",
                    "text",
                    "Allegro",
                );
            }
        });
    });
    group(ui, t("Score"), |ui| {
        if big(
            ui,
            Ico::Info,
            t("Title & credits"),
            app.side == Some(SideTab::Properties),
            true,
            t("Title, subtitle, composer, lyricist"),
        )
        .clicked()
        {
            show_side(app, SideTab::Properties);
        }
    });
}

/// Opens a side-panel tab, showing the side panel if the surface hides it.
fn show_side(app: &mut ScoreApp, tab: SideTab) {
    if !app.surface.has_side() {
        set_surface(app, Surface::Both);
    }
    app.side = Some(tab);
}

fn tab_instruments(app: &mut ScoreApp, ui: &mut Ui) {
    group(ui, t("Instruments"), |ui| {
        if big(
            ui,
            Ico::Instruments,
            t("Instruments"),
            app.side == Some(SideTab::Instruments) && app.surface.has_side(),
            true,
            t("Add, change, order and split instruments (I)"),
        )
        .clicked()
        {
            show_side(app, SideTab::Instruments);
        }
        let r = big(
            ui,
            Ico::Plus,
            t("Add"),
            false,
            can(app, "part.add").is_ok(),
            t("Add instrument"),
        );
        egui::Popup::menu(&r)
            .id(egui::Id::unique("add-instrument"))
            .show(|ui| {
                for inst in scoreart_model::INSTRUMENTS {
                    if ui.button(t(inst.name)).clicked() {
                        app.run("part.add", json!({ "instrument": inst.id, "name": t(inst.name) }));
                        ui.close();
                    }
                }
            });
    });
    group(ui, t("Transpose"), |ui| {
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                Some(Ico::Up),
                t("Up a semitone"),
                t("Transpose up a semitone"),
                "score.transpose",
                json!({ "semitones": 1 }),
            );
            cmd_small(
                app,
                ui,
                Some(Ico::Down),
                t("Down a semitone"),
                t("Transpose down a semitone"),
                "score.transpose",
                json!({ "semitones": -1 }),
            );
        });
    });
    group(ui, t("Sounds"), |ui| {
        if big(ui, Ico::Mixer, t("Mixer"), app.show_mixer, true, "F10").clicked() {
            app.show_mixer = !app.show_mixer;
        }
        let on = app.session.score.playback.optimize;
        if big(
            ui,
            Ico::Sliders,
            t("Sound optimization"),
            on,
            true,
            t("On: every note grows, rounds and lets go; staccato, pizz. and tremolo play the library's matching samples; all sounds balanced. Off: the notes as written with the chosen sound."),
        )
        .clicked()
        {
            app.run("score.playback", json!({ "optimize": !on }));
            app.session.status = if on { t("Sound optimization off").into() } else { t("Sound optimization on").into() };
        }
        column(ui, |ui| {
            if small(
                ui,
                Some(Ico::Open),
                t("Load sounds…"),
                false,
                true,
                t("A SoundFont (.sf2), an SFZ instrument (.sfz) or a zip of SFZ instruments"),
            )
            .clicked()
            {
                app.host.request_sounds(false);
            }
            if app.host.can_load_folders()
                && small(
                    ui,
                    Some(Ico::Open),
                    t("Load SFZ folder…"),
                    false,
                    true,
                    t("A folder with .sfz instruments and their samples"),
                )
                .clicked()
            {
                app.host.request_sounds(true);
            }
            if small(
                ui,
                Some(Ico::Download),
                t("Get free sounds…"),
                false,
                true,
                t("Free libraries (orchestra, piano …) with their licences"),
            )
            .clicked()
            {
                app.open_catalog();
            }
        });
    });
    group(ui, t("Input device"), |ui| {
        if big(
            ui,
            Ico::Keys,
            t("MIDI input"),
            app.host.midi_connected().is_some(),
            true,
            t("Connect a MIDI keyboard and set what each key does"),
        )
        .clicked()
        {
            app.open_midi();
        }
    });
}

fn tab_format(app: &mut ScoreApp, ui: &mut Ui) {
    group(ui, t("Page"), |ui| {
        if big(
            ui,
            Ico::PageSetup,
            t("Page settings"),
            app.page_settings.is_some(),
            true,
            t("Paper, margins, two-sided, staff size, distances"),
        )
        .clicked()
        {
            crate::page::open(app);
        }
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                Some(Ico::Reset),
                t("Reset"),
                t("Reset page settings"),
                "page.format",
                json!({ "reset": true }),
            );
        });
    });
    group(ui, t("Note spacing"), |ui| {
        let now = app.session.score.page.spacing;
        column(ui, |ui| {
            for (label, f) in [("Tight", 0.8f32), ("Normal", 1.0)] {
                if small(
                    ui,
                    Some(Ico::Spacing),
                    t(label),
                    (now - f).abs() < 0.01,
                    true,
                    t("Note spacing"),
                )
                .clicked()
                {
                    app.run("style.spacing", json!({ "factor": f }));
                }
            }
        });
        column(ui, |ui| {
            for (label, f) in [("Wide", 1.25f32), ("Very wide", 1.5)] {
                if small(
                    ui,
                    Some(Ico::Spacing),
                    t(label),
                    (now - f).abs() < 0.01,
                    true,
                    t("Note spacing"),
                )
                .clicked()
                {
                    app.run("style.spacing", json!({ "factor": f }));
                }
            }
        });
    });
    group(ui, t("Texts"), |ui| {
        if big(
            ui,
            Ico::Fonts,
            t("Fonts"),
            app.fonts.is_some(),
            true,
            t("Fonts of all texts (not the notes), with a live preview…"),
        )
        .clicked()
        {
            if app.fonts.is_some() {
                app.fonts = None;
            } else {
                crate::texts::open(app, scoreart_model::TextKind::Title);
            }
        }
        column(ui, |ui| {
            for (k, ico, tip) in [
                (
                    scoreart_model::TextKind::Heading,
                    Ico::Heading,
                    t("Add a heading above the system of the selected measure"),
                ),
                (
                    scoreart_model::TextKind::Text,
                    Ico::TextBlock,
                    t("Add a text block above the system of the selected measure"),
                ),
            ] {
                if small(ui, Some(ico), crate::texts::kind_label(k), false, true, tip).clicked() {
                    crate::texts::add_block(app, k);
                }
            }
        });
        column(ui, |ui| {
            if small(
                ui,
                Some(Ico::Header),
                t("Header & footer…"),
                app.header_footer.is_some(),
                true,
                t("Header and footer: left, middle and right, odd and even pages, page numbers and copyright"),
            )
            .clicked()
            {
                crate::texts::open_header_footer(app);
            }
        });
        column(ui, |ui| {
            let on = !app.session.score.hide_page_numbers;
            if small(
                ui,
                Some(Ico::PageNumber),
                t("Page numbers"),
                on,
                true,
                t("Page numbers from page 2 on (toggle)"),
            )
            .clicked()
            {
                app.run("page.numbers", json!({ "on": !on }));
            }
            if small(
                ui,
                Some(Ico::Text),
                t("Copyright"),
                false,
                true,
                t("The copyright line at the foot of the first page"),
            )
            .clicked()
            {
                crate::texts::edit_block(
                    app,
                    scoreart_model::BlockRef::Fixed(scoreart_model::TextKind::Copyright),
                );
            }
        });
        column(ui, |ui| {
            let sel = app.block_sel;
            let locked = sel.is_some_and(|b| app.session.score.block_locked(b));
            let (ico, label) = if locked {
                (Ico::Lock, t("Position locked"))
            } else {
                (Ico::Unlock, t("Lock position"))
            };
            let tip = if sel.is_none() {
                t("Pick a text block first (click it with the hand)")
            } else if locked {
                t("The picked text stays at its place; click to let it move again")
            } else {
                t("Hold the picked text at its place so that it cannot be dragged by accident")
            };
            if small(ui, Some(ico), label, locked, sel.is_some(), tip).clicked()
                && let Some(b) = sel
            {
                crate::texts::lock_block(app, b, !locked);
            }
            if small(
                ui,
                Some(Ico::Lock),
                t("Lock all texts"),
                false,
                true,
                t("Hold every text block at its place"),
            )
            .clicked()
            {
                app.run("block.lock", json!({ "block": "all", "locked": true }));
            }
            if small(
                ui,
                Some(Ico::Unlock),
                t("Unlock all texts"),
                false,
                true,
                t("Let every text block move freely again"),
            )
            .clicked()
            {
                app.run("block.lock", json!({ "block": "all", "locked": false }));
            }
        });
    });
    group(ui, t("Breaks"), |ui| {
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                Some(Ico::LineBreak),
                t("Line break"),
                t("Line break after the measure (toggle)"),
                "measure.break",
                json!({ "kind": "line" }),
            );
            cmd_small(
                app,
                ui,
                Some(Ico::PageBreak),
                t("Page break"),
                t("Page break after the measure (toggle)"),
                "measure.break",
                json!({ "kind": "page" }),
            );
            cmd_small(
                app,
                ui,
                Some(Ico::SectionBreak),
                t("Section break"),
                t(
                    "Section break after the measure: new system, pause in playback, measure numbers start again (toggle)",
                ),
                "measure.break",
                json!({ "kind": "section" }),
            );
        });
        column(ui, |ui| {
            for (ico, label, tip, kind) in [
                (
                    Ico::SpacerUp,
                    t("Spacer up"),
                    t("More room above the staff in the selected measures"),
                    "up",
                ),
                (
                    Ico::SpacerDown,
                    t("Spacer down"),
                    t("More room below the staff in the selected measures"),
                    "down",
                ),
                (
                    Ico::SpacerFixed,
                    t("Fixed spacer"),
                    t("Exact distance below the staff in the selected measures"),
                    "fixed",
                ),
            ] {
                cmd_small(
                    app,
                    ui,
                    Some(ico),
                    label,
                    tip,
                    "staff.spacer",
                    json!({ "type": kind }),
                );
            }
        });
        column(ui, |ui| {
            cmd_small(
                app,
                ui,
                None,
                t("Remove spacers"),
                t("Remove all spacers of the selected measures"),
                "staff.spacer",
                json!({ "type": "none" }),
            );
            let sec = crate::panels::section_at_focus(app);
            if small(
                ui,
                None,
                t("Section…"),
                false,
                sec.is_some(),
                t("Settings of the section break: pause, instrument names, measure numbers"),
            )
            .clicked()
            {
                app.dialog = sec.map(Dialog::Section);
            }
            let m = app.session.focus().2;
            let frame = app.session.score.measures.get(m).and_then(|i| i.frame_after);
            if small(
                ui,
                None,
                t("Blank space…"),
                frame.is_some(),
                true,
                t("Free room after the system of the measure: pushes what follows down or onto the next page"),
            )
            .clicked()
            {
                app.dialog = Some(Dialog::Frame {
                    height: frame.unwrap_or(10.0),
                    existing: frame.is_some(),
                });
            }
        });
    });
}

fn tab_view(app: &mut ScoreApp, ui: &mut Ui) {
    group(ui, t("Interface"), |ui| {
        for s in Surface::ALL {
            let on = app.surface == s;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(74.0, 70.0), Sense::click());
            let resp = resp.on_hover_text(tf("Interface: {}", &[&s.label()]));
            let (fg, bg) = frame_for(ui, &resp, on);
            let tk = Tokens::get(ui.ctx());
            ui.painter().rect_filled(rect, 6.0, bg);
            if on {
                ui.painter()
                    .rect_stroke(rect, 6.0, Stroke::new(1.2, tk.accent), egui::StrokeKind::Inside);
            }
            mini_surface(
                ui.painter(),
                Rect::from_center_size(rect.center_top() + Vec2::new(0.0, 24.0), Vec2::new(40.0, 28.0)),
                s,
                fg,
            );
            ui.painter().text(
                Pos2::new(rect.center().x, rect.bottom() - 13.0),
                egui::Align2::CENTER_CENTER,
                s.label(),
                egui::FontId::proportional(11.5),
                fg,
            );
            if resp.clicked() {
                set_surface(app, s);
            }
        }
    });
    group(ui, t("Side panel"), |ui| {
        let tabs = SideTab::ALL;
        for pair in tabs.chunks(2) {
            column(ui, |ui| {
                for tab in pair {
                    let ico = match tab {
                        SideTab::Palettes => Ico::Grid,
                        SideTab::Instruments => Ico::Instruments,
                        SideTab::Properties => Ico::Sliders,
                        SideTab::Sounds => Ico::Speaker,
                    };
                    let on = app.surface.has_side() && app.side == Some(*tab);
                    if small(ui, Some(ico), tab.label(), on, true, tab.label()).clicked() {
                        if on {
                            app.side = None;
                        } else {
                            show_side(app, *tab);
                        }
                    }
                }
            });
        }
    });
    group(ui, t("View"), |ui| {
        for (v, ico, label) in [
            (scoreart_engine::View::Page, Ico::Page, t("Page")),
            (
                scoreart_engine::View::Horizontal,
                Ico::Horizontal,
                t("Continuous"),
            ),
            (scoreart_engine::View::Vertical, Ico::Vertical, t("Vertical")),
        ] {
            if big(
                ui,
                ico,
                label,
                app.session.view == v,
                true,
                crate::page::view_label(v),
            )
            .clicked()
            {
                crate::page::set_view(app, v);
            }
        }
        if big(
            ui,
            Ico::Original,
            t("PDF"),
            app.reference.show && app.session.reference.is_some(),
            app.session.reference.is_some(),
            t("Show the opened PDF beside the score (open a PDF with File → Open)"),
        )
        .clicked()
        {
            crate::reference::toggle(app);
        }
    });
    group(ui, t("Zoom"), |ui| {
        column(ui, |ui| {
            if small(
                ui,
                Some(Ico::ZoomIn),
                t("Zoom in"),
                false,
                true,
                &crate::i18n::keys("Ctrl+="),
            )
            .clicked()
            {
                app.set_zoom(app.zoom * 1.15);
            }
            if small(
                ui,
                Some(Ico::ZoomOut),
                t("Zoom out"),
                false,
                true,
                &crate::i18n::keys("Ctrl+-"),
            )
            .clicked()
            {
                app.set_zoom(app.zoom / 1.15);
            }
            if small(
                ui,
                Some(Ico::Zoom100),
                "100 %",
                false,
                true,
                &crate::i18n::keys("Ctrl+0"),
            )
            .clicked()
            {
                app.set_zoom(7.5);
            }
        });
    });
    group(ui, t("Language & theme"), |ui| {
        column(ui, |ui| {
            for l in crate::i18n::Lang::ALL {
                if small(
                    ui,
                    Some(Ico::Globe),
                    l.name(),
                    crate::i18n::lang() == l,
                    true,
                    t("Language"),
                )
                .clicked()
                {
                    app.set_language(l);
                }
            }
            let dark = ui.ctx().global_style().visuals.dark_mode;
            if small(
                ui,
                Some(Ico::Theme),
                if dark { t("Light theme") } else { t("Dark theme") },
                false,
                true,
                t("Theme"),
            )
            .clicked()
            {
                ui.ctx().set_theme(if dark {
                    egui::Theme::Light
                } else {
                    egui::Theme::Dark
                });
            }
        });
    });
    group(ui, t("Help"), |ui| {
        column(ui, |ui| {
            if small(
                ui,
                Some(Ico::Help),
                t("Keyboard shortcuts"),
                false,
                true,
                t("Keyboard shortcuts"),
            )
            .clicked()
            {
                app.dialog = Some(Dialog::Shortcuts);
            }
            if small(
                ui,
                Some(Ico::Info),
                t("About ScoreArt"),
                false,
                true,
                t("About ScoreArt"),
            )
            .clicked()
            {
                app.dialog = Some(Dialog::About);
            }
        });
    });
}

// ---------------------------------------------------------------- focus mode

/// The floating input bar of the focus surface.
pub fn focus_bar(app: &mut ScoreApp, ctx: &egui::Context) {
    let tk = Tokens::get(ctx);
    egui::Area::new(egui::Id::unique("focus-bar"))
        .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, -40.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(tk.panel)
                .corner_radius(12)
                .inner_margin(6)
                .stroke(Stroke::new(
                    1.0,
                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                ))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 6],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_black_alpha(70),
                })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let input = app.session.input;
                        if cell(
                            ui,
                            32.0,
                            input.active,
                            true,
                            &crate::i18n::keys(t("Pen: click to write notes (N)")),
                            |p, r, c| icons::paint(p, r.shrink(7.0), Ico::Pen, c),
                        )
                        .clicked()
                        {
                            app.set_pen(true);
                        }
                        if cell(
                            ui,
                            32.0,
                            !input.active,
                            true,
                            &crate::i18n::keys(t("Hand: select, edit and drag notes and marks (Esc)")),
                            |p, r, c| icons::paint(p, r.shrink(6.0), Ico::Hand, c),
                        )
                        .clicked()
                        {
                            app.set_pen(false);
                        }
                        ui.separator();
                        for v in [
                            NoteValue::N32nd,
                            NoteValue::N16th,
                            NoteValue::Eighth,
                            NoteValue::Quarter,
                            NoteValue::Half,
                            NoteValue::Whole,
                        ] {
                            let on = input.value == v && (input.active || app.session.selection.is_none());
                            let g = panels::duration_glyph(v);
                            if cell(
                                ui,
                                30.0,
                                on,
                                true,
                                &format!("{} ({})", t(v.xml_name()), v.digit()),
                                |p, r, c| {
                                    glyph::paint(
                                        p,
                                        g,
                                        Pos2::new(r.center().x - 4.0, r.bottom() - 9.0),
                                        24.0,
                                        c,
                                    );
                                },
                            )
                            .clicked()
                            {
                                app.run("input.duration", json!({ "value": v.digit() }));
                            }
                        }
                        if cell(
                            ui,
                            30.0,
                            input.dots > 0,
                            true,
                            t("Augmentation dot (.)"),
                            |p, r, c| {
                                p.circle_filled(r.center(), 2.6, c);
                            },
                        )
                        .clicked()
                        {
                            app.run("input.dot", json!({}));
                        }
                        if cell(ui, 30.0, false, true, t("Rest (0)"), |p, r, c| {
                            glyph::paint(
                                p,
                                scoreart_engrave::glyphs::REST_QUARTER,
                                Pos2::new(r.center().x - 4.0, r.center().y + 1.0),
                                24.0,
                                c,
                            );
                        })
                        .clicked()
                        {
                            app.run("note.rest", json!({}));
                        }
                        ui.separator();
                        for (name, id, tip) in [
                            ("accidentalFlat", "flat", "Flat"),
                            ("accidentalNatural", "natural", "Natural"),
                            ("accidentalSharp", "sharp", "Sharp"),
                        ] {
                            let en = can(app, "note.accidental").is_ok();
                            if cell(ui, 30.0, false, en, t(tip), |p, r, c| {
                                panels::paint_smufl(p, r, name, c)
                            })
                            .clicked()
                            {
                                app.run("note.accidental", json!({ "type": id }));
                            }
                        }
                        let en = can(app, "note.tie").is_ok();
                        if cell(
                            ui,
                            30.0,
                            false,
                            en,
                            &crate::i18n::keys(t("Tie (T)")),
                            |p, r, c| icons::paint(p, r.shrink(6.0), Ico::Tie, c),
                        )
                        .clicked()
                        {
                            app.run("note.tie", json!({}));
                        }
                        ui.separator();
                        let playing = app.is_playing();
                        if cell(
                            ui,
                            32.0,
                            playing,
                            true,
                            t("Play from the selection (Space)"),
                            |p, r, c| {
                                icons::paint(p, r.shrink(8.0), if playing { Ico::Stop } else { Ico::Play }, c)
                            },
                        )
                        .clicked()
                        {
                            app.toggle_playback();
                        }
                    });
                });
        });
}
