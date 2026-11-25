//! The score canvas: pages, selection highlight, note-input cursor and mouse input.

use egui::{Color32, Mesh, Pos2, Rect, Sense, Shape as EShape, Stroke, Vec2};
use serde_json::json;

use scoreart_engine::Selection;
use scoreart_engrave::{Align, Shape, Target};
use scoreart_model::{Pitch, Step};

use crate::{ScoreApp, glyph, theme};

const PAGE_GAP: f32 = 28.0;
const PAD: f32 = 24.0;

/// Voice colours for the input cursor (voice 1–4).
pub fn voice_color(v: usize) -> Color32 {
    match v {
        0 => Color32::from_rgb(0x1c, 0x7e, 0xd6),
        1 => Color32::from_rgb(0x2f, 0x9e, 0x44),
        2 => Color32::from_rgb(0xe8, 0x59, 0x0c),
        _ => Color32::from_rgb(0xae, 0x3e, 0xc9),
    }
}

fn bezier(p: [(f32, f32); 4], t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    let a = u * u * u;
    let b = 3.0 * u * u * t;
    let c = 3.0 * u * t * t;
    let d = t * t * t;
    (
        a * p[0].0 + b * p[1].0 + c * p[2].0 + d * p[3].0,
        a * p[0].1 + b * p[1].1 + c * p[2].1 + d * p[3].1,
    )
}

/// A filled crescent between two Bézier curves as a triangle strip.
fn curve_mesh(p: [(f32, f32); 4], thick: f32, map: &dyn Fn(f32, f32) -> Pos2, color: Color32) -> Mesh {
    let c = scoreart_render::crescent(p, thick);
    let upper = [c[4], c[0], c[1], c[5]];
    let lower = [c[4], c[3], c[2], c[5]];
    let mut mesh = Mesh::default();
    const N: u32 = 24;
    for i in 0..=N {
        let t = i as f32 / N as f32;
        let a = bezier(upper, t);
        let b = bezier(lower, t);
        mesh.colored_vertex(map(a.0, a.1), color);
        mesh.colored_vertex(map(b.0, b.1), color);
    }
    for i in 0..N {
        let k = i * 2;
        mesh.add_triangle(k, k + 1, k + 2);
        mesh.add_triangle(k + 1, k + 3, k + 2);
    }
    mesh
}

/// A note picked up with the mouse: how high it was grabbed (staff spaces) and where it
/// would land now.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NoteDrag {
    ev: scoreart_model::edit::EventRef,
    y0: f32,
    /// Part, staff, measure, tick and pitch (diatonic) under the pointer.
    target: Option<(usize, usize, usize, u32, i32)>,
}

/// A staff text or dynamic picked up with the hand: where it was grabbed and how far the
/// pointer has moved since (staff spaces).
#[derive(Clone, Copy, Debug)]
pub(crate) struct MarkDrag {
    ev: scoreart_model::edit::EventRef,
    text: bool,
    page: usize,
    grab: (f32, f32),
    delta: (f32, f32),
    /// Its box when picked up.
    hit: scoreart_engrave::Hit,
}

/// A rehearsal mark, segno/coda sign or jump picked up with the hand.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MeasureMarkDrag {
    /// The mark's target (with its anchor and where it stands).
    target: Target,
    page: usize,
    grab: (f32, f32),
    delta: (f32, f32),
    hit: scoreart_engrave::Hit,
}

/// A line (hairpin, slur, 8va …) picked up with the hand: the whole line, or one end.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LineDrag {
    index: usize,
    /// Some(true): the start; Some(false): the end; None: the whole line.
    end: Option<bool>,
    /// The end grabbed is a hairpin's open end: pulled up or down it opens or closes.
    open: bool,
    page: usize,
    grab: (f32, f32),
    delta: (f32, f32),
    hit: scoreart_engrave::Hit,
}

/// A text block of the page (title, heading, header …) picked up with the hand.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BlockDrag {
    block: scoreart_model::BlockRef,
    page: usize,
    grab: (f32, f32),
    delta: (f32, f32),
    hit: scoreart_engrave::Hit,
}

/// The box of a block on a page.
fn block_box(
    layout: &scoreart_engrave::Layout,
    page: usize,
    block: scoreart_model::BlockRef,
) -> Option<scoreart_engrave::Hit> {
    layout
        .hits
        .iter()
        .copied()
        .find(|h| h.page == page && h.target == Target::Block { block })
}

impl LineDrag {
    /// Pulling a hairpin's open end more up or down than sideways: changing its opening.
    fn spreading(&self) -> bool {
        self.open && self.delta.1.abs() > self.delta.0.abs()
    }
}

/// Which end of line `index` is a hairpin's open end: Some(false) the end of a crescendo,
/// Some(true) the start of a diminuendo; None for other lines.
fn open_end(score: &scoreart_model::Score, index: usize) -> Option<bool> {
    match score.spanners.get(index).map(|s| s.kind) {
        Some(scoreart_model::SpannerKind::Crescendo) => Some(false),
        Some(scoreart_model::SpannerKind::Diminuendo) => Some(true),
        _ => None,
    }
}

/// The piece of line `index` under (x, y) on a page.
fn line_box(
    layout: &scoreart_engrave::Layout,
    page: usize,
    x: f32,
    y: f32,
    index: usize,
) -> Option<scoreart_engrave::Hit> {
    layout.hits.iter().copied().find(|h| {
        h.page == page
            && matches!(h.target, Target::Line { index: i, .. } if i == index)
            && x >= h.x0
            && x <= h.x1
            && y >= h.y0
            && y <= h.y1
    })
}

/// The note of a line's voice nearest to (x, y): (measure, tick).
fn note_near(
    layout: &scoreart_engrave::Layout,
    page: usize,
    x: f32,
    y: f32,
    sp: &scoreart_model::Spanner,
) -> Option<(usize, u32)> {
    // The staff of the line on that system (lines sit above or below it).
    let sb = layout
        .staves
        .iter()
        .filter(|s| s.page == page && s.part == sp.part && s.staff == sp.staff && x >= s.x0 && x <= s.x1)
        .min_by(|a, b| (a.top + 2.0 - y).abs().total_cmp(&(b.top + 2.0 - y).abs()))?;
    Some((sb.measure, sb.tick_near(x)))
}

/// Puts a dragged line where it was dropped.
fn drop_line(app: &mut ScoreApp, layout: &scoreart_engrave::Layout, d: LineDrag) {
    let Some(sp) = app.session.score.spanners.get(d.index).copied() else {
        return;
    };
    let (gx, gy) = d.grab;
    let x = gx + d.delta.0;
    match d.end {
        Some(_) if d.spreading() => {
            let mid = (d.hit.y0 + d.hit.y1) / 2.0;
            let half = (gy + d.delta.1 - mid).abs();
            let spread = (f64::from(half) * 2.0 * 10.0).round() / 10.0;
            app.run(
                "line.spread",
                json!({ "index": d.index, "spread": spread.max(0.3) }),
            );
        }
        Some(start) => {
            if d.delta.0.abs() < 0.3 {
                return;
            }
            if let Some((measure, tick)) = note_near(layout, d.page, x, gy, &sp) {
                let which = if start { "start" } else { "end" };
                app.run(
                    "line.end",
                    json!({ "index": d.index, "which": which, "measure": measure, "tick": tick }),
                );
            }
        }
        None => {
            // Sideways: by as many notes as the pointer moved over.
            let shift = match (
                note_near(layout, d.page, gx, gy, &sp),
                note_near(layout, d.page, x, gy, &sp),
            ) {
                (Some(a), Some(b)) if d.delta.0.abs() >= 0.8 => notes_between(&app.session.score, &sp, a, b),
                _ => 0,
            };
            let dy = if d.delta.1.abs() >= 0.15 {
                Some((f64::from(sp.dy) / 10.0 + f64::from(d.delta.1) * 10.0).round() / 10.0)
            } else {
                None
            };
            if shift == 0 && dy.is_none() {
                return;
            }
            let mut p = json!({ "index": d.index, "shift": shift });
            if let Some(dy) = dy {
                p["dy"] = json!(dy);
            }
            app.run("line.move", p);
        }
    }
}

/// How many notes of the line's voice lie from position `a` to `b` (negative: backwards).
fn notes_between(
    score: &scoreart_model::Score,
    sp: &scoreart_model::Spanner,
    a: (usize, u32),
    b: (usize, u32),
) -> i64 {
    let ev = |m: (usize, u32)| {
        scoreart_model::edit::Pos {
            part: sp.part,
            staff: sp.staff,
            voice: sp.voice,
            measure: m.0,
            tick: m.1,
        }
        .event(score)
    };
    let (Some(ea), Some(eb)) = (ev(a), ev(b)) else {
        return 0;
    };
    let forward = (b.0, b.1) > (a.0, a.1);
    let mut cur = ea;
    for n in 1..=4096i64 {
        let next = if forward {
            score.next_event(cur)
        } else {
            score.prev_event(cur)
        };
        match next {
            Some(x) if x == eb => return if forward { n } else { -n },
            Some(x) => cur = x,
            None => return 0,
        }
    }
    0
}

/// The hit box of an event's staff text or dynamic.
fn mark_box(
    layout: &scoreart_engrave::Layout,
    ev: scoreart_model::edit::EventRef,
    text: bool,
) -> Option<scoreart_engrave::Hit> {
    layout.hits.iter().copied().find(|h| match h.target {
        Target::Text { ev: e } => text && e == ev,
        Target::Dynamic { ev: e } => !text && e == ev,
        _ => false,
    })
}

/// The tick under `x` in a staff box: an existing note column or a multiple of `step`,
/// whichever lies nearest.
fn drop_tick(sb: &scoreart_engrave::StaffBox, x: f32, step: u32, len: u32) -> u32 {
    let mut best = (sb.tick_near(x), f32::MAX);
    best.1 = (sb.x_at(best.0) - x).abs();
    let step = step.max(1);
    let mut t = 0u32;
    while t < len && t / step < 256 {
        let d = (sb.x_at(t) - x).abs();
        if d < best.1 {
            best = (t, d);
        }
        t += step;
    }
    best.0
}

/// Paints one engraved shape; `map` turns page staff spaces into screen points.
pub(crate) fn paint_shape(
    painter: &egui::Painter,
    shape: &Shape,
    map: &dyn Fn(f32, f32) -> Pos2,
    zoom: f32,
    color: Color32,
) {
    match shape {
        Shape::Glyph { c, x, y, scale } => glyph::paint(painter, *c, map(*x, *y), 4.0 * zoom * scale, color),
        Shape::Line {
            x1,
            y1,
            x2,
            y2,
            width,
        } => {
            painter.line_segment(
                [map(*x1, *y1), map(*x2, *y2)],
                Stroke::new((width * zoom).max(0.6), color),
            );
        }
        Shape::Rect { x, y, w, h } => {
            painter.rect_filled(
                Rect::from_min_size(map(*x, *y), Vec2::new(w * zoom, h * zoom)),
                0.0,
                color,
            );
        }
        Shape::Quad { p } => {
            painter.add(EShape::convex_polygon(
                p.iter().map(|(a, b)| map(*a, *b)).collect(),
                color,
                Stroke::NONE,
            ));
        }
        Shape::Curve { p, thick } => {
            painter.add(EShape::mesh(curve_mesh(*p, *thick, &map, color)));
        }
        Shape::Text {
            text,
            x,
            y,
            size,
            align,
            bold,
            italic,
            serif,
            ..
        } => {
            let family = if *serif {
                egui::FontFamily::Name(glyph::SERIF.into())
            } else {
                egui::FontFamily::Proportional
            };
            let font = egui::FontId::new((size * zoom).max(1.0), family);
            let mut job = egui::text::LayoutJob::default();
            job.append(
                text,
                0.0,
                egui::TextFormat {
                    font_id: font,
                    color,
                    italics: *italic,
                    ..Default::default()
                },
            );
            let galley = painter.layout_job(job);
            let w = galley.size().x;
            let baseline = galley
                .rows
                .first()
                .and_then(|r| r.row.glyphs.first().map(|g| r.pos.y + g.pos.y))
                .unwrap_or(0.0);
            let pos = map(*x, *y);
            let gx = match align {
                Align::Left => pos.x,
                Align::Center => pos.x - w / 2.0,
                Align::Right => pos.x - w,
            };
            let gp = Pos2::new(gx, pos.y - baseline);
            painter.galley(gp, galley.clone(), color);
            if *bold {
                // Fake bold: the text again, shifted a little (the fonts have no bold face).
                let d = (size * zoom * 0.045).clamp(0.5, 2.5);
                painter.galley(gp + Vec2::new(d * 0.5, 0.0), galley.clone(), color);
                painter.galley(gp + Vec2::new(d, 0.0), galley, color);
            }
        }
    }
}

/// Paints a break or spacer sign of the layout in the accent colour.
fn paint_layout_mark(
    painter: &egui::Painter,
    m: &scoreart_engrave::Mark,
    map: &dyn Fn(f32, f32) -> Pos2,
    zoom: f32,
    accent: Color32,
) {
    use scoreart_engrave::MarkKind;
    let stroke = Stroke::new((zoom * 0.12).clamp(1.0, 2.0), accent);
    let arrow = |from: f32, to: f32| {
        let (x, d) = (m.x, if to < from { -0.5 } else { 0.5 });
        painter.line_segment([map(x, from), map(x, to)], stroke);
        painter.line_segment([map(x - 0.5, to - d), map(x, to)], stroke);
        painter.line_segment([map(x + 0.5, to - d), map(x, to)], stroke);
    };
    let bar = |y: f32| painter.line_segment([map(m.x - 0.8, y), map(m.x + 0.8, y)], stroke);
    let size = (zoom * 2.0).clamp(13.0, 26.0);
    match m.kind {
        MarkKind::SpacerUp(h) => {
            arrow(m.y, m.y - h);
            bar(m.y - h);
        }
        MarkKind::SpacerDown(h) => {
            arrow(m.y, m.y + h);
            bar(m.y + h);
        }
        MarkKind::SpacerFixed(h) => {
            painter.line_segment([map(m.x, m.y), map(m.x, m.y + h)], stroke);
            bar(m.y);
            bar(m.y + h);
        }
        MarkKind::LineBreak | MarkKind::PageBreak | MarkKind::SectionBreak => {
            let sign = match m.kind {
                MarkKind::LineBreak => "↵",
                MarkKind::PageBreak => "⤓",
                _ => "§",
            };
            let r = Rect::from_center_size(map(m.x, m.y), Vec2::splat(size * 1.1));
            painter.rect_filled(r, 2.0, accent.linear_multiply(0.15));
            painter.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                sign,
                egui::FontId::proportional(size * 0.8),
                accent,
            );
        }
    }
}

pub fn show(app: &mut ScoreApp, ui: &mut egui::Ui) {
    let zoom = app.zoom;
    let selected = app.session.selection;
    let selected_ev = app.session.selected_event();
    let range: std::collections::HashSet<scoreart_model::edit::EventRef> = if app.session.range_end.is_some()
    {
        app.session.selected_events().into_iter().collect()
    } else {
        Default::default()
    };
    let input = app.session.input;
    let playhead = app.playback.as_ref().and_then(|p| p.at);
    let cursor = app.session.cursor;
    let tk = theme::Tokens::get(ui.ctx());
    let layout = app.session.layout();

    let max_w = layout.pages.iter().map(|p| p.width).fold(0.0f32, f32::max) * zoom;
    let total_h: f32 = layout
        .pages
        .iter()
        .map(|p| p.height * zoom + PAGE_GAP)
        .sum::<f32>()
        + PAD;
    let mut scroll_target: Option<Rect> = None;
    let mut click: Option<(usize, f32, f32, bool, bool)> = None;
    let mut hover: Option<(usize, f32, f32)> = None;
    let mut double: Option<(usize, f32, f32)> = None;

    egui::ScrollArea::both()
        .auto_shrink(false)
        .id_salt("score_canvas")
        .show(ui, |ui| {
            let avail = ui.available_width();
            let size = Vec2::new((max_w + 2.0 * PAD).max(avail), total_h.max(ui.available_height()));
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, tk.canvas);
            let clip = ui.clip_rect();
            let mut y = rect.top() + PAD;
            for (pi, page) in layout.pages.iter().enumerate() {
                let pw = page.width * zoom;
                let ph = page.height * zoom;
                let x = rect.left() + ((rect.width() - pw) / 2.0).max(PAD);
                let prect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(pw, ph));
                y += ph + PAGE_GAP;
                let origin = prect.min;
                let map = |sx: f32, sy: f32| Pos2::new(origin.x + sx * zoom, origin.y + sy * zoom);

                // Mouse.
                if let Some(pos) = resp.interact_pointer_pos()
                    && resp.clicked()
                    && prect.contains(pos)
                {
                    let mods = ui.input(|i| i.modifiers);
                    click = Some((
                        pi,
                        (pos.x - origin.x) / zoom,
                        (pos.y - origin.y) / zoom,
                        mods.shift,
                        mods.command,
                    ));
                }
                if let Some(pos) = resp.interact_pointer_pos()
                    && resp.double_clicked()
                    && prect.contains(pos)
                {
                    double = Some((pi, (pos.x - origin.x) / zoom, (pos.y - origin.y) / zoom));
                }
                if let Some(pos) = resp.hover_pos()
                    && prect.contains(pos)
                {
                    hover = Some((pi, (pos.x - origin.x) / zoom, (pos.y - origin.y) / zoom));
                }
                // Dragging a note: pick it up where the press began.
                if resp.drag_started()
                    && !input.active
                    && let Some(pos) = ui.input(|i| i.pointer.press_origin())
                    && prect.contains(pos)
                {
                    let (sx, sy) = ((pos.x - origin.x) / zoom, (pos.y - origin.y) / zoom);
                    match layout.hit(pi, sx, sy) {
                        Some(Target::Note { ev, note }) => {
                            let _ = app.session.execute(
                                "select.event",
                                &json!({ "part": ev.part, "staff": ev.staff, "voice": ev.voice, "measure": ev.measure, "index": ev.index, "note": note }),
                            );
                            app.note_drag = Some(NoteDrag { ev, y0: sy, target: None });
                        }
                        Some(Target::Text { ev }) | Some(Target::Dynamic { ev }) => {
                            let text = matches!(layout.hit(pi, sx, sy), Some(Target::Text { .. }));
                            let _ = app.session.execute(
                                "select.event",
                                &json!({ "part": ev.part, "staff": ev.staff, "voice": ev.voice, "measure": ev.measure, "index": ev.index }),
                            );
                            if let Some(hit) = mark_box(&layout, ev, text) {
                                app.mark_drag = Some(MarkDrag { ev, text, page: pi, grab: (sx, sy), delta: (0.0, 0.0), hit });
                            }
                        }
                        Some(t @ Target::MeasureMark { .. }) => {
                            if let Some(hit) = layout.hits.iter().copied().find(|h| h.page == pi && h.target == t) {
                                app.measure_mark_drag =
                                    Some(MeasureMarkDrag { target: t, page: pi, grab: (sx, sy), delta: (0.0, 0.0), hit });
                            }
                        }
                        Some(Target::Block { block }) => {
                            app.block_sel = Some(block);
                            if app.session.score.block_locked(block) {
                                app.session.status = scoreart_engine::TEXT_LOCKED.into();
                            } else if let Some(hit) = block_box(&layout, pi, block) {
                                app.block_drag = Some(BlockDrag { block, page: pi, grab: (sx, sy), delta: (0.0, 0.0), hit });
                            }
                        }
                        Some(Target::Line { index, first, last }) => {
                            if let Some(hit) = line_box(&layout, pi, sx, sy, index) {
                                // Near an end of the line: that end moves; elsewhere the line.
                                let grip = 1.5;
                                let end = if first && sx - hit.x0 < grip {
                                    Some(true)
                                } else if last && hit.x1 - sx < grip {
                                    Some(false)
                                } else {
                                    None
                                };
                                let open = open_end(&app.session.score, index) == end && end.is_some();
                                app.line_drag = Some(LineDrag {
                                    index,
                                    end,
                                    open,
                                    page: pi,
                                    grab: (sx, sy),
                                    delta: (0.0, 0.0),
                                    hit,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(d) = app.line_drag.as_mut()
                    && d.page == pi
                    && resp.dragged()
                    && let Some(pos) = ui.input(|i| i.pointer.latest_pos())
                {
                    let (sx, sy) = ((pos.x - origin.x) / zoom, (pos.y - origin.y) / zoom);
                    d.delta = (sx - d.grab.0, sy - d.grab.1);
                }
                if let Some(d) = app.block_drag.as_mut()
                    && d.page == pi
                    && resp.dragged()
                    && let Some(pos) = ui.input(|i| i.pointer.latest_pos())
                {
                    let (sx, sy) = ((pos.x - origin.x) / zoom, (pos.y - origin.y) / zoom);
                    d.delta = (sx - d.grab.0, sy - d.grab.1);
                }
                if let Some(d) = app.measure_mark_drag.as_mut()
                    && d.page == pi
                    && resp.dragged()
                    && let Some(pos) = ui.input(|i| i.pointer.latest_pos())
                {
                    let (sx, sy) = ((pos.x - origin.x) / zoom, (pos.y - origin.y) / zoom);
                    d.delta = (sx - d.grab.0, sy - d.grab.1);
                }
                if let Some(d) = app.mark_drag.as_mut()
                    && d.page == pi
                    && resp.dragged()
                    && let Some(pos) = ui.input(|i| i.pointer.latest_pos())
                {
                    let (sx, sy) = ((pos.x - origin.x) / zoom, (pos.y - origin.y) / zoom);
                    d.delta = (sx - d.grab.0, sy - d.grab.1);
                }
                if let Some(d) = app.note_drag.as_mut()
                    && resp.dragged()
                    && let Some(pos) = ui.input(|i| i.pointer.latest_pos())
                    && prect.contains(pos)
                {
                    let (sx, sy) = ((pos.x - origin.x) / zoom, (pos.y - origin.y) / zoom);
                    d.target = layout.staff_box_at(pi, sx, sy).map(|sb| {
                        let step = app.session.score.event(d.ev).map_or(TPQ_GRID, |e| e.ticks().min(TPQ_GRID * 4));
                        let len = app.session.score.measure_ticks(sb.measure);
                        (sb.part, sb.staff, sb.measure, drop_tick(sb, sx, step, len), sb.diatonic_at(sy))
                    });
                }
                if !prect.intersects(clip) {
                    continue;
                }
                // Hand cursor over things that can be picked up.
                if !input.active
                    && let Some((hp, hx, hy)) = hover
                    && hp == pi
                {
                    let target = layout.hit(pi, hx, hy);
                    let movable = match target {
                        Some(Target::Block { block }) => !app.session.score.block_locked(block),
                        t => matches!(
                            t,
                            Some(
                                Target::Note { .. }
                                    | Target::Text { .. }
                                    | Target::Dynamic { .. }
                                    | Target::Line { .. }
                                    | Target::MeasureMark { .. }
                            )
                        ),
                    };
                    // At the end of a line: the end can be pulled sideways.
                    // Which end is under the pointer: Some(true) start, Some(false) end.
                    let end_here = match target {
                        Some(Target::Line { index, first, last }) => line_box(&layout, pi, hx, hy, index).and_then(|h| {
                            if first && hx - h.x0 < 1.5 {
                                Some((index, true))
                            } else if last && h.x1 - hx < 1.5 {
                                Some((index, false))
                            } else {
                                None
                            }
                        }),
                        _ => None,
                    };
                    let at_end = end_here.is_some();
                    let at_open = end_here.is_some_and(|(i, e)| open_end(&app.session.score, i) == Some(e));
                    if app.line_drag.is_some_and(|d| d.spreading()) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                    } else if app.line_drag.is_some_and(|d| d.end.is_some()) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    } else if app.mark_drag.is_some()
                        || app.measure_mark_drag.is_some()
                        || app.note_drag.is_some()
                        || app.line_drag.is_some()
                        || app.block_drag.is_some()
                    {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    } else if at_open {
                        // Sideways: the end moves; up or down: the hairpin opens or closes.
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
                    } else if at_end {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    } else if movable {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    }
                } else if input.active && hover.is_some_and(|h| h.0 == pi) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
                painter.rect_filled(prect.translate(Vec2::new(2.0, 3.0)), 2.0, tk.page_shadow);
                painter.rect_filled(prect, 0.0, Color32::WHITE);
                let ink = Color32::from_rgb(0x11, 0x11, 0x11);
                for it in &page.items {
                    let sel = match (it.owner, selected_ev) {
                        (Some(o), Some(s)) => o == s || range.contains(&o),
                        _ => false,
                    } || match (it.owner, selected) {
                        (
                            Some(o),
                            Some(Selection::Measures {
                                part,
                                staff,
                                first,
                                last,
                            }),
                        ) => o.part == part && o.staff == staff && o.measure >= first && o.measure <= last,
                        _ => false,
                    };
                    let color = if sel { tk.accent } else { ink };
                    paint_shape(&painter, &it.shape, &map, zoom, color);
                }
                // Measure-range selection frame.
                if let Some(Selection::Measures {
                    part,
                    staff,
                    first,
                    last,
                }) = selected
                {
                    for sb in layout.staves.iter().filter(|s| {
                        s.page == pi
                            && s.part == part
                            && s.staff == staff
                            && s.measure >= first
                            && s.measure <= last
                    }) {
                        let r = Rect::from_min_max(map(sb.x0, sb.top - 0.6), map(sb.x1, sb.top + 4.6));
                        painter.rect_filled(r, 2.0, tk.accent.linear_multiply(0.12));
                        painter.rect_stroke(r, 2.0, Stroke::new(1.5, tk.accent), egui::StrokeKind::Inside);
                    }
                }
                // Playhead: a line through every staff of the sounding measure.
                if let Some((pm, ptick)) = playhead {
                    for sb in layout.staves.iter().filter(|s| s.page == pi && s.measure == pm) {
                        let x = sb.x_at(ptick) + 0.5;
                        let r = Rect::from_min_max(map(x - 0.25, sb.top - 1.5), map(x + 0.25, sb.top + 5.5));
                        painter.rect_filled(r, 1.0, tk.accent.linear_multiply(0.85));
                        let band = Rect::from_min_max(map(sb.x0, sb.top - 1.5), map(sb.x1, sb.top + 5.5));
                        painter.rect_filled(band, 0.0, tk.accent.linear_multiply(0.06));
                        if sb.part == 0 && sb.staff == 0 {
                            scroll_target = Some(r.expand(160.0));
                        }
                    }
                }
                // Where the dragged note would land.
                if let Some(d) = app.note_drag
                    && let Some((part, staff, measure, tick, dia)) = d.target
                    && let Some(sb) = layout.staff_box(part, staff, measure).filter(|s| s.page == pi)
                {
                    let x = sb.x_at(tick);
                    let yy = sb.top + (sb.clef.top_line() - dia) as f32 * 0.5;
                    glyph::paint(
                        &painter,
                        scoreart_engrave::glyphs::HEAD_BLACK,
                        map(x, yy),
                        4.0 * zoom,
                        tk.accent,
                    );
                }
                // Where a dragged line would go.
                if let Some(d) = app.line_drag.filter(|d| d.page == pi) {
                    let h = d.hit;
                    match d.end {
                        None => {
                            let r = Rect::from_min_max(
                                map(h.x0 + d.delta.0, h.y0 + d.delta.1),
                                map(h.x1 + d.delta.0, h.y1 + d.delta.1),
                            );
                            painter.rect_filled(r, 3.0, tk.accent.linear_multiply(0.12));
                            painter.rect_stroke(r, 3.0, Stroke::new(1.2, tk.accent), egui::StrokeKind::Outside);
                        }
                        Some(start) if d.spreading() => {
                            // The new opening at the open end.
                            let x = if start { h.x0 } else { h.x1 };
                            let mid = (h.y0 + h.y1) / 2.0;
                            let half = (d.grab.1 + d.delta.1 - mid).abs().max(0.15);
                            let tip = if start { h.x1 } else { h.x0 };
                            for sign in [-1.0, 1.0] {
                                painter.line_segment(
                                    [map(tip, mid), map(x, mid + sign * half)],
                                    Stroke::new(2.0, tk.accent),
                                );
                            }
                        }
                        Some(start) => {
                            let x = if start { h.x0 } else { h.x1 } + d.delta.0;
                            let a = map(x, h.y0 - 1.0);
                            let b = map(x, h.y1 + 1.0);
                            painter.line_segment([a, b], Stroke::new(2.0, tk.accent));
                        }
                    }
                }
                // The picked text block, and where a dragged one would go.
                if let Some(b) = app.block_sel
                    && let Some(h) = block_box(&layout, pi, b)
                {
                    let r = Rect::from_min_max(map(h.x0, h.y0), map(h.x1, h.y1));
                    crate::texts::frame(&painter, r, tk.accent);
                    if app.session.score.block_locked(b) {
                        let size = (zoom * 1.8).clamp(12.0, 22.0);
                        let at = Rect::from_min_size(
                            Pos2::new(r.right() + 3.0, r.top() - size * 0.3),
                            Vec2::splat(size),
                        );
                        painter.rect_filled(at.expand(1.0), 3.0, tk.accent);
                        crate::icons::paint(&painter, at, crate::icons::Ico::Lock, Color32::WHITE);
                    }
                }
                if let Some(d) = app.block_drag.filter(|d| d.page == pi) {
                    let h = d.hit;
                    let r = Rect::from_min_max(map(h.x0 + d.delta.0, h.y0 + d.delta.1), map(h.x1 + d.delta.0, h.y1 + d.delta.1));
                    painter.rect_filled(r, 3.0, tk.accent.linear_multiply(0.12));
                    painter.rect_stroke(r, 3.0, Stroke::new(1.2, tk.accent), egui::StrokeKind::Outside);
                }
                // Where a dragged rehearsal mark, segno/coda or jump would go.
                if let Some(d) = app.measure_mark_drag.filter(|d| d.page == pi) {
                    let h = d.hit;
                    let r = Rect::from_min_max(map(h.x0 + d.delta.0, h.y0 + d.delta.1), map(h.x1 + d.delta.0, h.y1 + d.delta.1));
                    painter.rect_filled(r, 3.0, tk.accent.linear_multiply(0.12));
                    painter.rect_stroke(r, 3.0, Stroke::new(1.2, tk.accent), egui::StrokeKind::Outside);
                }
                // Where a dragged text or dynamic would go.
                if let Some(d) = app.mark_drag.filter(|d| d.page == pi) {
                    let h = d.hit;
                    let r = Rect::from_min_max(map(h.x0 + d.delta.0, h.y0 + d.delta.1), map(h.x1 + d.delta.0, h.y1 + d.delta.1));
                    painter.rect_filled(r, 3.0, tk.accent.linear_multiply(0.12));
                    painter.rect_stroke(r, 3.0, Stroke::new(1.2, tk.accent), egui::StrokeKind::Outside);
                }
                // Overlaps: both boxes tinted, where they meet stronger (never printed).
                if app.show_collisions {
                    let red = crate::panels::COLLISION_RED;
                    for c in layout.collisions.iter().filter(|c| c.page == pi) {
                        for b in [c.a, c.b] {
                            let r = Rect::from_min_max(map(b[0], b[1]), map(b[2], b[3]));
                            painter.rect_filled(r, 1.0, red.linear_multiply(0.12));
                            painter.rect_stroke(r, 1.0, Stroke::new(1.0, red.linear_multiply(0.8)), egui::StrokeKind::Outside);
                        }
                        let (x0, y0) = (c.a[0].max(c.b[0]), c.a[1].max(c.b[1]));
                        let (x1, y1) = (c.a[2].min(c.b[2]), c.a[3].min(c.b[3]));
                        if x1 > x0 && y1 > y0 {
                            painter.rect_filled(Rect::from_min_max(map(x0, y0), map(x1, y1)), 0.0, red.linear_multiply(0.35));
                        }
                    }
                }
                // Breaks and spacers: editor-only signs, never printed.
                for m in layout.marks.iter().filter(|m| m.page == pi) {
                    paint_layout_mark(&painter, m, &map, zoom, tk.accent);
                }
                // Note-input cursor.
                if input.active
                    && let Some(sb) = layout
                        .staff_box(cursor.part, cursor.staff, cursor.measure)
                        .filter(|s| s.page == pi)
                {
                    let x = sb.x_at(cursor.tick);
                    let r = Rect::from_min_max(map(x - 0.5, sb.top - 1.2), map(x + 1.7, sb.top + 5.2));
                    let c = voice_color(cursor.voice);
                    painter.rect_filled(r, 3.0, c.linear_multiply(0.18));
                    painter.rect_stroke(r, 3.0, Stroke::new(1.5, c), egui::StrokeKind::Inside);
                    if app.scroll_to_cursor {
                        scroll_target = Some(r.expand(80.0));
                    }
                }
                // Ghost note under the mouse in note input.
                if input.active
                    && let Some((hp, hx, hy)) = hover
                    && hp == pi
                    && let Some(sb) = layout.staff_box_at(pi, hx, hy)
                {
                    let tick = sb.tick_near(hx);
                    let x = sb.x_at(tick);
                    let yy = sb.top + (sb.top_line_diff(hy) as f32) * 0.5 * sb.mag;
                    let c = voice_color(input.voice).linear_multiply(0.55);
                    glyph::paint(
                        &painter,
                        scoreart_engrave::glyphs::HEAD_BLACK,
                        map(x, yy),
                        4.0 * zoom,
                        c,
                    );
                }
                if !input.active
                    && app.scroll_to_cursor
                    && let Some(ev) = selected_ev
                    && let Some(h) = layout.event_box(ev).filter(|h| h.page == pi)
                {
                    scroll_target = Some(Rect::from_min_max(map(h.x0, h.y0), map(h.x1, h.y1)).expand(120.0));
                }
            }
            if let Some(r) = scroll_target {
                ui.scroll_to_rect(r, None);
            }
        });
    app.scroll_to_cursor = false;
    if resp_drag_stopped(ui)
        && let Some(d) = app.note_drag.take()
    {
        drop_note(app, &layout, d);
    }
    if resp_drag_stopped(ui)
        && let Some(d) = app.mark_drag.take()
    {
        drop_mark(app, &layout, d);
    }
    if resp_drag_stopped(ui)
        && let Some(d) = app.measure_mark_drag.take()
    {
        drop_measure_mark(app, d);
    }
    if resp_drag_stopped(ui)
        && let Some(d) = app.line_drag.take()
    {
        drop_line(app, &layout, d);
    }
    if resp_drag_stopped(ui)
        && let Some(d) = app.block_drag.take()
    {
        crate::texts::move_block(app, d.block, d.delta);
    }

    if let Some((page, x, y, shift, _cmd)) = click {
        handle_click(app, &layout, page, x, y, shift);
    }
    // Double-clicking a text block edits it.
    if let Some((page, x, y)) = double
        && !app.session.input.active
        && let Some(Target::Block { block }) = layout.hit(page, x, y)
    {
        crate::texts::edit_block(app, block);
    }
    // Double-clicking a staff text edits it.
    if let Some((page, x, y)) = double
        && !app.session.input.active
        && let Some(Target::Text { ev }) = layout.hit(page, x, y)
    {
        let value = app
            .session
            .score
            .event(ev)
            .and_then(|e| e.text.clone())
            .unwrap_or_default();
        app.dialog = Some(crate::Dialog::Text {
            title: crate::i18n::t("Staff text"),
            command: "note.text",
            key: "text",
            value,
        });
    }
}

/// Whether the mouse button was let go this frame.
fn resp_drag_stopped(ui: &egui::Ui) -> bool {
    ui.input(|i| i.pointer.any_released())
}

/// Puts a dragged note where it was dropped: higher or lower on the same beat, or at another
/// beat, measure or staff (with the pitch it was dropped at).
fn drop_note(app: &mut ScoreApp, layout: &scoreart_engrave::Layout, d: NoteDrag) {
    let Some((part, staff, measure, tick, dia)) = d.target else {
        return;
    };
    let Some(from) = d.ev.pos(&app.session.score) else {
        return;
    };
    // The pitch where the note was grabbed, on its own staff.
    let Some(dia0) = layout
        .staff_box(from.part, from.staff, from.measure)
        .map(|sb| sb.diatonic_at(d.y0))
    else {
        return;
    };
    let steps = dia - dia0;
    if (part, staff, measure, tick) == (from.part, from.staff, from.measure, from.tick) {
        match steps {
            0 => {}
            s if s > 0 => app.run("note.step-up", json!({ "steps": s })),
            s => app.run("note.step-down", json!({ "steps": -s })),
        }
    } else {
        app.run(
            "note.move-to",
            json!({ "part": part, "staff": staff, "measure": measure, "tick": tick, "steps": steps }),
        );
    }
}

/// Places a rehearsal mark, segno/coda or jump where it was dragged (as MuseScore stores it:
/// from its anchor, the measure's start or end and the top staff line).
fn drop_measure_mark(app: &mut ScoreApp, d: MeasureMarkDrag) {
    if d.delta.0.hypot(d.delta.1) < 0.15 {
        return;
    }
    let Target::MeasureMark {
        measure,
        kind,
        ax,
        ay,
        x,
        base,
    } = d.target
    else {
        return;
    };
    let round = |v: f32| (f64::from(v) * 10.0).round() / 10.0;
    let kind = match kind {
        scoreart_engrave::MeasureMark::Rehearsal => "rehearsal",
        scoreart_engrave::MeasureMark::Marker => "marker",
        scoreart_engrave::MeasureMark::Jump => "jump",
    };
    app.run(
        "measure.place",
        json!({
            "measure": measure,
            "kind": kind,
            "dx": round(x + d.delta.0 - ax),
            "dy": round(base + d.delta.1 - ay),
        }),
    );
}

/// Moves a staff text or dynamic by the distance it was dragged.
fn drop_mark(app: &mut ScoreApp, layout: &scoreart_engrave::Layout, d: MarkDrag) {
    if d.delta.0.hypot(d.delta.1) < 0.15 {
        return;
    }
    let Some(e) = app.session.score.event(d.ev) else {
        return;
    };
    let round = |v: f32| (f64::from(v) * 10.0).round() / 10.0;
    let (dx, dy) = if d.text {
        // A moved text is measured from just above (or below) the staff.
        let place = e.text_place.unwrap_or_default();
        let Some(sb) = layout.staff_box(d.ev.part, d.ev.staff, d.ev.measure) else {
            return;
        };
        let base = if place.below { sb.top + 7.0 } else { sb.top - 1.5 };
        let baseline = d.hit.y1 - 0.5;
        (place.dx_sp() + d.delta.0, baseline + d.delta.1 - base)
    } else {
        let place = e.dynamic_place.unwrap_or_default();
        (place.dx_sp() + d.delta.0, place.dy_sp() + d.delta.1)
    };
    let kind = if d.text { "text" } else { "dynamic" };
    app.run(
        "note.place",
        json!({ "kind": kind, "dx": round(dx), "dy": round(dy) }),
    );
}

/// Snap grid for dropped notes when the note is longer: a quarter.
const TPQ_GRID: u32 = scoreart_model::TPQ;

trait TopLineDiff {
    fn top_line_diff(&self, y: f32) -> i32;
}

impl TopLineDiff for scoreart_engrave::StaffBox {
    /// Half spaces below the top line, snapped.
    fn top_line_diff(&self, y: f32) -> i32 {
        (((y - self.top) * 2.0 / self.mag.max(0.1)).round() as i32).clamp(-24, 32)
    }
}

fn handle_click(
    app: &mut ScoreApp,
    layout: &scoreart_engrave::Layout,
    page: usize,
    x: f32,
    y: f32,
    shift: bool,
) {
    app.block_sel = None;
    let s = &mut app.session;
    if s.input.active {
        if let Some(sb) = layout.staff_box_at(page, x, y) {
            let tick = sb.tick_near(x);
            let d = sb.diatonic_at(y);
            let p = Pitch::from_diatonic(d, 0);
            let step = match p.step {
                Step::C => "c",
                Step::D => "d",
                Step::E => "e",
                Step::F => "f",
                Step::G => "g",
                Step::A => "a",
                Step::B => "b",
            };
            let pos = json!({ "part": sb.part, "staff": sb.staff, "measure": sb.measure, "tick": tick });
            if s.execute("cursor.set", &pos).is_ok() {
                app.run("note.enter", json!({ "step": step, "octave": p.octave }));
            }
        }
        return;
    }
    let r = match layout.hit(page, x, y) {
        Some(Target::Note { ev, note }) => s.execute(
            "select.event",
            &json!({ "part": ev.part, "staff": ev.staff, "voice": ev.voice, "measure": ev.measure, "index": ev.index, "note": note, "extend": shift }),
        ),
        Some(Target::Line { index, .. }) => match s.score.spanners.get(index).copied() {
            Some(sp) => s.execute(
                "select.event",
                &json!({ "part": sp.part, "staff": sp.staff, "voice": sp.voice, "measure": sp.start.measure, "index": scoreart_model::edit::Pos { part: sp.part, staff: sp.staff, voice: sp.voice, measure: sp.start.measure, tick: sp.start.tick }.event(&s.score).map_or(0, |e| e.index) }),
            ),
            None => Ok(serde_json::Value::Null),
        },
        Some(Target::Text { ev } | Target::Dynamic { ev }) => s.execute(
            "select.event",
            &json!({ "part": ev.part, "staff": ev.staff, "voice": ev.voice, "measure": ev.measure, "index": ev.index }),
        ),
        Some(Target::Rest { ev }) => s.execute(
            "select.event",
            &json!({ "part": ev.part, "staff": ev.staff, "voice": ev.voice, "measure": ev.measure, "index": ev.index, "extend": shift }),
        ),
        Some(Target::Block { block }) => {
            app.block_sel = Some(block);
            return;
        }
        Some(Target::Measure { part, staff, measure }) => {
            s.execute("select.measure", &json!({ "part": part, "staff": staff, "measure": measure, "extend": shift }))
        }
        // A rehearsal mark, segno/coda or jump: its measure (in the top staff).
        Some(Target::MeasureMark { measure, .. }) => {
            s.execute("select.measure", &json!({ "part": 0, "staff": 0, "measure": measure, "extend": shift }))
        }
        None => s.execute("edit.escape", &json!({})),
    };
    if let Err(e) = r {
        s.status = e.to_string();
    }
}
