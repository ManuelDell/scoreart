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

/// A line (hairpin, slur, 8va …) picked up with the hand: the whole line, or one end.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LineDrag {
    index: usize,
    /// Some(true): the start; Some(false): the end; None: the whole line.
    end: Option<bool>,
    page: usize,
    grab: (f32, f32),
    delta: (f32, f32),
    hit: scoreart_engrave::Hit,
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
                                app.line_drag = Some(LineDrag {
                                    index,
                                    end,
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
                    let movable = matches!(
                        target,
                        Some(Target::Note { .. } | Target::Text { .. } | Target::Dynamic { .. } | Target::Line { .. })
                    );
                    // At the end of a line: the end can be pulled sideways.
                    let at_end = match target {
                        Some(Target::Line { index, first, last }) => line_box(&layout, pi, hx, hy, index)
                            .is_some_and(|h| (first && hx - h.x0 < 1.5) || (last && h.x1 - hx < 1.5)),
                        _ => false,
                    };
                    if app.line_drag.is_some_and(|d| d.end.is_some()) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    } else if app.mark_drag.is_some() || app.note_drag.is_some() || app.line_drag.is_some() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
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
                    match &it.shape {
                        Shape::Glyph { c, x, y, scale } => {
                            glyph::paint(&painter, *c, map(*x, *y), 4.0 * zoom * scale, color)
                        }
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
                        } => {
                            let font = egui::FontId::proportional((size * zoom).max(1.0));
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
                                painter.galley(gp + Vec2::new(0.6, 0.0), galley, color);
                            }
                        }
                    }
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
                        Some(start) => {
                            let x = if start { h.x0 } else { h.x1 } + d.delta.0;
                            let a = map(x, h.y0 - 1.0);
                            let b = map(x, h.y1 + 1.0);
                            painter.line_segment([a, b], Stroke::new(2.0, tk.accent));
                        }
                    }
                }
                // Where a dragged text or dynamic would go.
                if let Some(d) = app.mark_drag.filter(|d| d.page == pi) {
                    let h = d.hit;
                    let r = Rect::from_min_max(map(h.x0 + d.delta.0, h.y0 + d.delta.1), map(h.x1 + d.delta.0, h.y1 + d.delta.1));
                    painter.rect_filled(r, 3.0, tk.accent.linear_multiply(0.12));
                    painter.rect_stroke(r, 3.0, Stroke::new(1.2, tk.accent), egui::StrokeKind::Outside);
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
                    let yy = sb.top + (sb.top_line_diff(hy) as f32) * 0.5;
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
        && let Some(d) = app.line_drag.take()
    {
        drop_line(app, &layout, d);
    }

    if let Some((page, x, y, shift, _cmd)) = click {
        handle_click(app, &layout, page, x, y, shift);
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
        (((y - self.top) * 2.0).round() as i32).clamp(-24, 32)
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
        Some(Target::Measure { part, staff, measure }) => {
            s.execute("select.measure", &json!({ "part": part, "staff": staff, "measure": measure, "extend": shift }))
        }
        None => s.execute("edit.escape", &json!({})),
    };
    if let Err(e) = r {
        s.status = e.to_string();
    }
}
