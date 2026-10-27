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

    egui::ScrollArea::both()
        .auto_shrink(false)
        .id_salt("score_canvas")
        .show(ui, |ui| {
            let avail = ui.available_width();
            let size = Vec2::new((max_w + 2.0 * PAD).max(avail), total_h.max(ui.available_height()));
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
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
                if let Some(pos) = resp.hover_pos()
                    && prect.contains(pos)
                {
                    hover = Some((pi, (pos.x - origin.x) / zoom, (pos.y - origin.y) / zoom));
                }
                if !prect.intersects(clip) {
                    continue;
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

    if let Some((page, x, y, shift, _cmd)) = click {
        handle_click(app, &layout, page, x, y, shift);
    }
}

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
