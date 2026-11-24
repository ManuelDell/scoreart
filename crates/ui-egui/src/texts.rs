//! Texts that are not notes: the "Fonts" window (every kind of text with its font, size, weight,
//! slant and alignment, next to a live preview of the sample song) and the dialog for adding and
//! editing the text blocks of the page.

use std::sync::Arc;

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use serde_json::json;

use scoreart_engrave::{Align, Layout, Shape, Style, View};
use scoreart_model::{BlockRef, PageFormat, Score, TextAlign, TextFont, TextKind, TextStyles};

use crate::i18n::{t, tf};
use crate::{Dialog, ScoreApp, canvas, glyph, theme};

/// The open "Fonts" window.
#[derive(Clone)]
pub(crate) struct FontsWindow {
    pub kind: TextKind,
    /// A size being dragged, shown at once and saved when let go.
    pending: Option<f32>,
    preview: Option<Preview>,
    /// Bring the selected kind into view in the preview.
    reveal: bool,
}

#[derive(Clone)]
struct Preview {
    styles: TextStyles,
    page: PageFormat,
    layout: Arc<Layout>,
}

impl FontsWindow {
    pub fn new(kind: TextKind) -> FontsWindow {
        FontsWindow {
            kind,
            pending: None,
            preview: None,
            reveal: true,
        }
    }
}

pub(crate) fn open(app: &mut ScoreApp, kind: TextKind) {
    app.fonts = Some(FontsWindow::new(kind));
}

/// Name of a kind, translated.
pub(crate) fn kind_label(k: TextKind) -> &'static str {
    t(k.label())
}

/// The sample song with every kind of text on it, in the given styles: two pages (a page
/// break after measure 4), so that headers, footers and page numbers show too.
fn sample(styles: &TextStyles, page: PageFormat) -> Score {
    let mut s = scoreart_engine::sample::ode_to_joy();
    s.text_styles = styles.clone();
    // Short pages (the sample is short), so the texts show large in the window.
    s.page = PageFormat {
        height_mm: (page.width_mm * 0.62).max(100.0),
        ..page
    };
    s.meta.copyright = t("Sample song · public domain").into();
    if let Some(m) = s.measures.first_mut() {
        m.tempo_text = Some("Allegro assai".into());
    }
    if let Some(m) = s.measures.get_mut(3) {
        m.page_break = true;
    }
    if let Some(m) = s.measures.get_mut(4) {
        m.rehearsal = Some("B".into());
        m.text = Some("Tutti".into());
    }
    let chords = ["D", "A7", "D", "A", "D", "A7", "D", "A7"];
    if let Some(part) = s.parts.first_mut() {
        for (mi, pm) in part.measures.iter_mut().enumerate() {
            let Some(e) = pm
                .staves
                .first_mut()
                .and_then(|sm| sm.voices.first_mut())
                .and_then(|v| v.events.first_mut())
            else {
                continue;
            };
            e.chord_symbol = chords.get(mi).map(|c| (*c).to_string());
            if mi == 0 {
                e.text = Some("dolce".into());
                if let Some(n) = e.notes_mut().first_mut() {
                    n.fingering = Some("3".into());
                }
            }
        }
    }
    let block = |kind, text: &str, measure| scoreart_model::TextBlock {
        kind,
        text: text.into(),
        measure,
        dx: 0,
        dy: 0,
    };
    s.blocks = vec![
        block(
            TextKind::Text,
            t("A text block: a few words before the music begins."),
            0,
        ),
        block(TextKind::Heading, t("Second verse"), 4),
    ];
    // A header on the pages after the first; the footer as in a new score (page number and
    // copyright).
    s.header_footer.show_header = true;
    for row in s.header_footer.header.iter_mut() {
        row[1] = t("Ode to Joy · header").into();
    }
    s
}

/// The preview's layout for the current styles (built again only when they change).
fn preview(app: &ScoreApp, w: &mut FontsWindow) -> Arc<Layout> {
    let mut styles = app.session.score.text_styles.clone();
    if let Some(size) = w.pending {
        let mut st = styles.get(w.kind);
        st.size = size;
        styles.set(w.kind, st);
    }
    let page = app.session.score.page;
    if let Some(p) = &w.preview
        && p.styles == styles
        && p.page == page
    {
        return p.layout.clone();
    }
    let score = sample(&styles, page);
    let layout = Arc::new(scoreart_engrave::layout(
        &score,
        &Style::new(&score.page, View::Page),
    ));
    w.preview = Some(Preview {
        styles,
        page,
        layout: layout.clone(),
    });
    layout
}

fn text_box(sh: &Shape) -> Option<(TextKind, f32, f32, f32, f32)> {
    let Shape::Text {
        text,
        x,
        y,
        size,
        align,
        kind: Some(k),
        ..
    } = sh
    else {
        return None;
    };
    let w = text.chars().count() as f32 * size * 0.52;
    let x0 = match align {
        Align::Left => *x,
        Align::Center => x - w / 2.0,
        Align::Right => x - w,
    };
    Some((*k, x0, y - size, x0 + w, y + size * 0.25))
}

pub(crate) fn window(app: &mut ScoreApp, ctx: &egui::Context) {
    let Some(mut w) = app.fonts.take() else { return };
    let mut open = true;
    let tk = theme::Tokens::get(ctx);
    egui::Window::new(t("Fonts"))
        .id(egui::Id::unique("fonts-window"))
        .open(&mut open)
        .default_size([1080.0, 640.0])
        .min_width(760.0)
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(190.0);
                    kinds_list(ui, &mut w);
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(270.0);
                    controls(app, ui, &mut w);
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.label(RichText::new(t("Preview")).strong());
                    ui.label(
                        RichText::new(t("Changes show at once. Click a text to choose its kind."))
                            .small()
                            .color(tk.text_dim),
                    );
                    let layout = preview(app, &mut w);
                    preview_pages(ui, &layout, &mut w, tk.accent);
                });
            });
        });
    if open {
        app.fonts = Some(w);
    }
}

fn kinds_list(ui: &mut egui::Ui, w: &mut FontsWindow) {
    egui::ScrollArea::vertical()
        .id_salt("fonts-kinds")
        .max_height(560.0)
        .show(ui, |ui| {
            let mut group = "";
            for k in TextKind::ALL {
                if k.group() != group {
                    group = k.group();
                    ui.add_space(6.0);
                    ui.label(RichText::new(t(group)).small().strong());
                }
                if ui.selectable_label(w.kind == k, kind_label(k)).clicked() && w.kind != k {
                    w.kind = k;
                    w.pending = None;
                    w.reveal = true;
                }
            }
        });
}

fn controls(app: &mut ScoreApp, ui: &mut egui::Ui, w: &mut FontsWindow) {
    let k = w.kind;
    let st = app.session.score.text_styles.get(k);
    let id = k.id();
    ui.heading(kind_label(k));
    ui.label(RichText::new(t(k.group())).small());
    ui.add_space(8.0);
    egui::Grid::new("fonts-grid")
        .num_columns(2)
        .spacing([10.0, 10.0])
        .show(ui, |ui| {
            ui.label(t("Font"));
            ui.horizontal(|ui| {
                for (f, name, fam) in [
                    (
                        TextFont::Serif,
                        t("Serif"),
                        egui::FontFamily::Name(glyph::SERIF.into()),
                    ),
                    (TextFont::Sans, t("Sans-serif"), egui::FontFamily::Proportional),
                ] {
                    let label = RichText::new(name).font(egui::FontId::new(15.0, fam));
                    if ui.selectable_label(st.font == f, label).clicked() && st.font != f {
                        let font = if f == TextFont::Serif { "serif" } else { "sans" };
                        app.run("text.style", json!({ "kind": id, "font": font }));
                    }
                }
            });
            ui.end_row();

            ui.label(t("Size"));
            ui.horizontal(|ui| {
                let mut size = w.pending.unwrap_or(st.size);
                if ui.small_button("−").on_hover_text(t("Smaller")).clicked() {
                    app.run(
                        "text.style",
                        json!({ "kind": id, "size": (st.size - 0.5).max(4.0) }),
                    );
                }
                let r = ui.add(
                    egui::DragValue::new(&mut size)
                        .range(4.0..=96.0)
                        .speed(0.1)
                        .fixed_decimals(1)
                        .suffix(" pt"),
                );
                if r.changed() {
                    w.pending = Some(size);
                }
                if (r.drag_stopped() || r.lost_focus() || (r.changed() && !r.dragged() && !r.has_focus()))
                    && let Some(v) = w.pending.take()
                {
                    app.run("text.style", json!({ "kind": id, "size": v }));
                }
                if ui.small_button("+").on_hover_text(t("Larger")).clicked() {
                    app.run(
                        "text.style",
                        json!({ "kind": id, "size": (st.size + 0.5).min(96.0) }),
                    );
                }
            });
            ui.end_row();

            ui.label(t("Style"));
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(st.bold, RichText::new("B").strong())
                    .on_hover_text(t("Bold"))
                    .clicked()
                {
                    app.run("text.style", json!({ "kind": id, "bold": !st.bold }));
                }
                if ui
                    .selectable_label(st.italic, RichText::new("I").italics())
                    .on_hover_text(t("Italic"))
                    .clicked()
                {
                    app.run("text.style", json!({ "kind": id, "italic": !st.italic }));
                }
            });
            ui.end_row();

            if k.is_block() {
                ui.label(t("Alignment"));
                ui.horizontal(|ui| {
                    for (a, name, v) in [
                        (TextAlign::Left, t("Left"), "left"),
                        (TextAlign::Center, t("Centre"), "center"),
                        (TextAlign::Right, t("Right"), "right"),
                    ] {
                        if ui.selectable_label(st.align == a, name).clicked() && st.align != a {
                            app.run("text.style", json!({ "kind": id, "align": v }));
                        }
                    }
                });
                ui.end_row();
            }
        });
    ui.add_space(8.0);
    // The words of the title page are edited right here.
    if let Some(text) = meta_text(&app.session.score, k) {
        ui.label(t("Text"));
        let key = egui::Id::unique(("fonts-text", id));
        let mut v = ui
            .data_mut(|d| d.get_temp::<String>(key))
            .unwrap_or_else(|| text.to_string());
        let r = ui.add(egui::TextEdit::singleline(&mut v).desired_width(250.0));
        if r.lost_focus() && v != text {
            app.run("block.edit", json!({ "block": id, "text": v }));
        }
        if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(key, v));
        } else {
            ui.data_mut(|d| d.remove::<String>(key));
        }
        ui.add_space(4.0);
    }
    if k == TextKind::PageNumber {
        let mut on = !app.session.score.hide_page_numbers;
        if ui
            .checkbox(&mut on, t("Show page numbers (from page 2)"))
            .changed()
        {
            app.run("page.numbers", json!({ "on": on }));
        }
    }
    if app.session.score.block_offsets.contains_key(&k)
        && ui
            .button(t("Back to its place"))
            .on_hover_text(t("Undo moving it by hand"))
            .clicked()
    {
        app.run("block.move", json!({ "block": id, "dx": 0, "dy": 0 }));
    }
    ui.add_space(4.0);
    let changed = st != k.default_style();
    if ui
        .add_enabled(changed, egui::Button::new(t("Reset this text")))
        .clicked()
    {
        app.run("text.style", json!({ "kind": id, "reset": true }));
    }
    ui.add_space(14.0);
    ui.separator();
    ui.label(RichText::new(t("All texts")).strong());
    ui.horizontal(|ui| {
        if ui.button(t("All serif")).clicked() {
            app.run("text.style", json!({ "kind": "all", "font": "serif" }));
        }
        if ui.button(t("All sans-serif")).clicked() {
            app.run("text.style", json!({ "kind": "all", "font": "sans" }));
        }
    });
    if ui
        .add_enabled(
            !app.session.score.text_styles.0.is_empty(),
            egui::Button::new(t("Reset all texts")),
        )
        .clicked()
    {
        app.run("text.style", json!({ "kind": "all", "reset": true }));
    }
}

fn meta_text(score: &Score, k: TextKind) -> Option<&str> {
    let m = &score.meta;
    Some(match k {
        TextKind::Title => &m.title,
        TextKind::Subtitle => &m.subtitle,
        TextKind::Composer => &m.composer,
        TextKind::Lyricist => &m.lyricist,
        TextKind::Copyright => &m.copyright,
        _ => return None,
    })
}

/// The sample's pages one below the other, the selected kind of text in the accent colour.
fn preview_pages(ui: &mut egui::Ui, layout: &Layout, w: &mut FontsWindow, accent: Color32) {
    egui::ScrollArea::vertical()
        .id_salt("fonts-preview")
        .auto_shrink(false)
        .show(ui, |ui| {
            let n = layout.pages.len().clamp(1, 2) as f32;
            let gap = 14.0;
            let avail = (ui.available_width() - 8.0).max(200.0);
            let pw = layout.pages.first().map_or(1.0, |p| p.width.max(1.0));
            let ph = layout.pages.first().map_or(1.0, |p| p.height.max(1.0));
            let zoom = (avail / pw).max(0.5);
            let (rect, resp) =
                ui.allocate_exact_size(Vec2::new(avail, (ph * zoom + gap) * n), Sense::click());
            let painter = ui.painter_at(rect);
            let mut reveal_at: Option<Rect> = None;
            let mut picked: Option<TextKind> = None;
            for (pi, page) in layout.pages.iter().take(2).enumerate() {
                let origin = Pos2::new(rect.left(), rect.top() + pi as f32 * (ph * zoom + gap));
                let prect = Rect::from_min_size(origin, Vec2::new(page.width * zoom, page.height * zoom));
                painter.rect_filled(
                    prect.translate(Vec2::new(2.0, 3.0)),
                    2.0,
                    Color32::from_black_alpha(40),
                );
                painter.rect_filled(prect, 0.0, Color32::WHITE);
                let map = |x: f32, y: f32| Pos2::new(origin.x + x * zoom, origin.y + y * zoom);
                let click = resp
                    .clicked()
                    .then(|| resp.interact_pointer_pos())
                    .flatten()
                    .filter(|p| prect.contains(*p))
                    .map(|p| ((p.x - origin.x) / zoom, (p.y - origin.y) / zoom));
                for it in &page.items {
                    let b = text_box(&it.shape);
                    let on = b.is_some_and(|b| b.0 == w.kind);
                    if let Some((k, x0, y0, x1, y1)) = b {
                        if on {
                            let r = Rect::from_min_max(map(x0 - 0.3, y0 - 0.2), map(x1 + 0.3, y1));
                            painter.rect_filled(r, 2.0, accent.linear_multiply(0.14));
                            if reveal_at.is_none() {
                                reveal_at = Some(r);
                            }
                        }
                        if let Some((cx, cy)) = click
                            && cx >= x0 - 0.3
                            && cx <= x1 + 0.3
                            && cy >= y0 - 0.3
                            && cy <= y1 + 0.3
                        {
                            picked = Some(k);
                        }
                    }
                    let color = if on {
                        accent
                    } else {
                        Color32::from_rgb(0x11, 0x11, 0x11)
                    };
                    canvas::paint_shape(&painter, &it.shape, &map, zoom, color);
                }
            }
            if let Some(k) = picked {
                w.kind = k;
                w.pending = None;
            }
            if w.reveal {
                if let Some(r) = reveal_at {
                    ui.scroll_to_rect(r.expand(40.0), None);
                }
                w.reveal = false;
            }
        });
}

// ---------------------------------------------------------------- text blocks

/// Adding or editing one text block of the page.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BlockDialog {
    /// None: a new block of `kind`.
    block: Option<BlockRef>,
    kind: TextKind,
    value: String,
}

/// Opens the dialog for a new heading or text block (a header or footer opens the header and
/// footer dialog).
pub(crate) fn add_block(app: &mut ScoreApp, kind: TextKind) {
    if matches!(kind, TextKind::Header | TextKind::Footer) {
        open_header_footer(app);
        return;
    }
    app.dialog = Some(Dialog::Block(BlockDialog {
        block: None,
        kind,
        value: String::new(),
    }));
}

/// Opens the dialog for an existing block (the page number opens the Fonts window instead).
pub(crate) fn edit_block(app: &mut ScoreApp, block: BlockRef) {
    let score = &app.session.score;
    let (kind, value) = match block {
        BlockRef::Fixed(TextKind::PageNumber | TextKind::Header | TextKind::Footer) => {
            open_header_footer(app);
            return;
        }
        BlockRef::Fixed(k) => (k, meta_text(score, k).unwrap_or_default().to_string()),
        BlockRef::Added(i) => match score.blocks.get(i) {
            Some(b) => (b.kind, b.text.clone()),
            None => return,
        },
    };
    app.dialog = Some(Dialog::Block(BlockDialog {
        block: Some(block),
        kind,
        value,
    }));
}

fn block_param(b: BlockRef) -> serde_json::Value {
    match b {
        BlockRef::Fixed(k) => json!(k.id()),
        BlockRef::Added(i) => json!(i),
    }
}

/// Removes a block (a title-page text is cleared; the page number is hidden).
pub(crate) fn remove_block(app: &mut ScoreApp, b: BlockRef) {
    app.run("block.remove", json!({ "block": block_param(b) }));
}

/// Shows the block dialog; returns whether it is closed.
pub(crate) fn block_dialog(app: &mut ScoreApp, ctx: &egui::Context, mut d: BlockDialog) -> bool {
    let measure = app.session.focus().2;
    let resp = egui::Modal::new(egui::Id::unique("block-dialog")).show(ctx, |ui| {
        ui.set_width(400.0);
        let title = match d.block {
            None => tf("New: {}", &[&kind_label(d.kind)]),
            Some(_) => kind_label(d.kind).to_string(),
        };
        ui.heading(title);
        let note = match d.kind {
            TextKind::Heading | TextKind::Text if d.block.is_none() => Some(tf(
                "Placed above the system with measure {}. Drag it to move it.",
                &[&app.session.score.measure_number(measure).to_string()],
            )),
            _ => None,
        };
        if let Some(n) = note {
            ui.label(RichText::new(n).small());
        }
        ui.add_space(4.0);
        let multi = d.kind == TextKind::Text;
        let r = if multi {
            ui.add(
                egui::TextEdit::multiline(&mut d.value)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            )
        } else {
            ui.add(egui::TextEdit::singleline(&mut d.value).desired_width(f32::INFINITY))
        };
        r.request_focus();
        let enter = if multi {
            ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command)
        } else {
            r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
        };
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let ok = ui.button(t("OK")).clicked() || enter;
            let cancel = ui.button(t("Cancel")).clicked();
            let remove = d.block.is_some() && ui.button(t("Remove")).clicked();
            let fonts = ui.button(t("Fonts…")).clicked();
            (ok, cancel, remove, fonts)
        })
        .inner
    });
    let (ok, cancel, remove, fonts) = resp.inner;
    if ok {
        let text = d.value.trim_end().to_string();
        match d.block {
            None if text.trim().is_empty() => {}
            None => app.run(
                "block.add",
                json!({ "kind": d.kind.id(), "text": text, "measure": measure }),
            ),
            Some(b @ BlockRef::Added(_)) if text.trim().is_empty() => remove_block(app, b),
            Some(b) => app.run("block.edit", json!({ "block": block_param(b), "text": text })),
        }
        return true;
    }
    if remove && let Some(b) = d.block {
        remove_block(app, b);
        return true;
    }
    if fonts {
        open(app, d.kind);
        return true;
    }
    if cancel || resp.should_close() {
        return true;
    }
    app.dialog = Some(Dialog::Block(d));
    false
}

/// The offset of a block now, in staff spaces.
pub(crate) fn block_offset(score: &Score, b: BlockRef) -> (f32, f32) {
    let (dx, dy) = match b {
        BlockRef::Fixed(k) => score.block_offsets.get(&k).copied().unwrap_or_default(),
        BlockRef::Added(i) => score.blocks.get(i).map_or((0, 0), |x| (x.dx, x.dy)),
    };
    (f32::from(dx) / 10.0, f32::from(dy) / 10.0)
}

/// Moves a block by a dragged distance.
pub(crate) fn move_block(app: &mut ScoreApp, b: BlockRef, delta: (f32, f32)) {
    if delta.0.hypot(delta.1) < 0.15 {
        return;
    }
    let (x, y) = block_offset(&app.session.score, b);
    let round = |v: f32| (f64::from(v) * 10.0).round() / 10.0;
    app.run(
        "block.move",
        json!({ "block": block_param(b), "dx": round(x + delta.0), "dy": round(y + delta.1) }),
    );
}

/// A frame around the selected block.
pub(crate) fn frame(painter: &egui::Painter, r: Rect, accent: Color32) {
    painter.rect_filled(r, 3.0, accent.linear_multiply(0.08));
    painter.rect_stroke(r, 3.0, Stroke::new(1.2, accent), egui::StrokeKind::Outside);
}

// ---------------------------------------------------------------- header and footer

/// Opens the header and footer dialog with the score's settings.
pub(crate) fn open_header_footer(app: &mut ScoreApp) {
    app.header_footer = Some(HeaderFooterEdit {
        hf: app.session.score.header_footer.clone(),
        page_numbers: !app.session.score.hide_page_numbers,
    });
}

/// The header and footer being edited.
#[derive(Clone)]
pub(crate) struct HeaderFooterEdit {
    hf: scoreart_model::HeaderFooter,
    page_numbers: bool,
}

fn apply_header_footer(app: &mut ScoreApp, e: &HeaderFooterEdit) {
    let h = &e.hf;
    app.run(
        "page.header-footer",
        json!({
            "show_header": h.show_header, "header_first_page": h.header_first_page, "header_odd_even": h.header_odd_even,
            "odd_header": h.header[0], "even_header": h.header[1],
            "show_footer": h.show_footer, "footer_first_page": h.footer_first_page, "footer_odd_even": h.footer_odd_even,
            "odd_footer": h.footer[0], "even_footer": h.footer[1],
            "page_number_first_page": h.page_number_first_page,
        }),
    );
    if e.page_numbers == app.session.score.hide_page_numbers {
        app.run("page.numbers", json!({ "on": e.page_numbers }));
    }
}

/// The macros a field can hold, and what they show.
const MACROS: &[(&str, &str)] = &[
    ("$p", "Page number, except on the first page"),
    ("$P", "Page number, on every page"),
    ("$n", "Number of pages"),
    ("$C", "Copyright, on the first page only"),
    ("$c", "Copyright, on every page"),
    ("$:workTitle:", "Title"),
    ("$:subtitle:", "Subtitle"),
    ("$:composer:", "Composer"),
    ("$:lyricist:", "Lyricist"),
    ("$:copyright:", "Copyright"),
    ("$$", "A dollar sign"),
];

pub(crate) fn header_footer_dialog(app: &mut ScoreApp, ctx: &egui::Context) {
    let Some(mut e) = app.header_footer.take() else {
        return;
    };
    let mut close = false;
    let mut apply_now = false;
    let resp = egui::Modal::new(egui::Id::unique("header-footer")).show(ctx, |ui| {
        ui.set_width(620.0);
        ui.heading(t("Header and footer"));
        ui.label(
            RichText::new(t("Three fields each, left, middle and right; macros such as $p (page number) are filled in on every page."))
                .small(),
        );
        for footer in [false, true] {
            ui.add_space(8.0);
            let h = &mut e.hf;
            let (show, first, odd_even, rows, title) = if footer {
                (&mut h.show_footer, &mut h.footer_first_page, &mut h.footer_odd_even, &mut h.footer, t("Footer"))
            } else {
                (&mut h.show_header, &mut h.header_first_page, &mut h.header_odd_even, &mut h.header, t("Header"))
            };
            ui.horizontal(|ui| {
                ui.checkbox(show, RichText::new(title).strong());
                ui.add_enabled_ui(*show, |ui| {
                    ui.checkbox(first, t("Show on first page"));
                    ui.checkbox(odd_even, t("Different odd/even pages"));
                });
            });
            let shown = *show;
            let both = *odd_even;
            ui.add_enabled_ui(shown, |ui| {
                egui::Grid::new(("hf-grid", footer)).num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label("");
                    for c in [t("Left"), t("Middle"), t("Right")] {
                        ui.label(RichText::new(c).small());
                    }
                    ui.end_row();
                    let [odd, even] = rows;
                    for (label, row, on) in [
                        (if both { t("Odd pages") } else { t("All pages") }, odd, true),
                        (t("Even pages"), even, both),
                    ] {
                        if !on {
                            continue;
                        }
                        ui.label(label);
                        for f in row.iter_mut() {
                            ui.add_sized([165.0, 22.0], egui::TextEdit::singleline(f));
                        }
                        ui.end_row();
                    }
                });
            });
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.checkbox(&mut e.page_numbers, t("Show page numbers"));
            ui.add_enabled_ui(e.page_numbers, |ui| {
                ui.checkbox(&mut e.hf.page_number_first_page, t("also on the first page"));
            });
        });
        egui::CollapsingHeader::new(t("Macros"))
            .id_salt("hf-macros")
            .show(ui, |ui| {
                egui::Grid::new("hf-macros-grid").num_columns(2).spacing([16.0, 2.0]).show(ui, |ui| {
                    for (m, what) in MACROS {
                        ui.label(RichText::new(*m).monospace());
                        ui.label(t(what));
                        ui.end_row();
                    }
                });
            });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(t("OK")).clicked() {
                apply_now = true;
                close = true;
            }
            if ui.button(t("Apply")).clicked() {
                apply_now = true;
            }
            if ui.button(t("Cancel")).clicked() {
                close = true;
            }
            if ui
                .button(t("Defaults"))
                .on_hover_text(t("As a new MuseScore 4 score: page number and copyright in the footer"))
                .clicked()
            {
                e.hf = scoreart_model::HeaderFooter::default();
                e.page_numbers = true;
            }
        });
    });
    if resp.should_close() {
        close = true;
    }
    if apply_now {
        apply_header_footer(app, &e);
    }
    if !close {
        app.header_footer = Some(e);
    }
}
