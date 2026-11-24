//! Interface icons drawn in code from simple shapes (no fonts, no image files), on a 24 × 24
//! grid scaled into the rect they are painted in.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ico {
    New,
    Open,
    Save,
    SaveAs,
    Export,
    Sample,
    Undo,
    Redo,
    Delete,
    SelectAll,
    Pen,
    Hand,
    Keys,
    Play,
    Stop,
    Mixer,
    Speaker,
    Download,
    Page,
    Horizontal,
    Vertical,
    ZoomIn,
    ZoomOut,
    Zoom100,
    Text,
    Lyrics,
    Chord,
    Tempo,
    Rehearsal,
    Fingering,
    MeasurePlus,
    MeasureInsert,
    MeasureMinus,
    LineBreak,
    PageBreak,
    SectionBreak,
    Lock,
    Unlock,
    SpacerUp,
    SpacerDown,
    SpacerFixed,
    PageSetup,
    Spacing,
    Transpose,
    Up,
    Down,
    OctaveUp,
    OctaveDown,
    Tie,
    Slur,
    Cresc,
    Dim,
    Grid,
    Sliders,
    Instruments,
    Globe,
    Theme,
    Help,
    Info,
    Plus,
    Close,
    Collapse,
    Expand,
    Reset,
    Fonts,
    Heading,
    TextBlock,
    Header,
    PageNumber,
}

/// Paints `ico` centred in `rect`, scaled to the smaller side.
pub fn paint(p: &Painter, rect: Rect, ico: Ico, color: Color32) {
    let s = rect.width().min(rect.height()) / 24.0;
    let o = rect.center() - Vec2::splat(12.0 * s);
    let at = |x: f32, y: f32| Pos2::new(o.x + x * s, o.y + y * s);
    let w = (1.7 * s).max(1.2);
    let st = Stroke::new(w, color);
    let line = |pts: &[(f32, f32)]| {
        p.add(Shape::line(pts.iter().map(|(x, y)| at(*x, *y)).collect(), st));
    };
    let fill = |pts: &[(f32, f32)]| {
        p.add(Shape::convex_polygon(
            pts.iter().map(|(x, y)| at(*x, *y)).collect(),
            color,
            Stroke::NONE,
        ));
    };
    let rect_s = |x: f32, y: f32, w2: f32, h: f32| {
        p.rect_stroke(
            Rect::from_min_size(at(x, y), Vec2::new(w2 * s, h * s)),
            1.5 * s,
            st,
            egui::StrokeKind::Middle,
        );
    };
    let rect_f = |x: f32, y: f32, w2: f32, h: f32| {
        p.rect_filled(
            Rect::from_min_size(at(x, y), Vec2::new(w2 * s, h * s)),
            1.0 * s,
            color,
        );
    };
    let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32| {
        let pts: Vec<Pos2> = (0..=20)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / 20.0;
                at(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        p.add(Shape::line(pts, st));
    };
    let circle = |cx: f32, cy: f32, r: f32| {
        p.circle_stroke(at(cx, cy), r * s, st);
    };
    let dot = |cx: f32, cy: f32, r: f32| {
        p.circle_filled(at(cx, cy), r * s, color);
    };
    let text = |x: f32, y: f32, t: &str, size: f32| {
        p.text(
            at(x, y),
            egui::Align2::CENTER_CENTER,
            t,
            egui::FontId::proportional(size * s),
            color,
        );
    };
    let staff = || {
        for y in [8.0, 11.0, 14.0, 17.0] {
            line(&[(3.0, y), (21.0, y)]);
        }
    };
    use std::f32::consts::PI;
    match ico {
        Ico::New => {
            line(&[
                (6.0, 3.0),
                (14.0, 3.0),
                (18.0, 7.0),
                (18.0, 21.0),
                (6.0, 21.0),
                (6.0, 3.0),
            ]);
            line(&[(14.0, 3.0), (14.0, 7.0), (18.0, 7.0)]);
            line(&[(12.0, 11.0), (12.0, 17.0)]);
            line(&[(9.0, 14.0), (15.0, 14.0)]);
        }
        Ico::Open => {
            line(&[
                (3.0, 6.0),
                (9.0, 6.0),
                (11.0, 8.0),
                (20.0, 8.0),
                (20.0, 18.0),
                (3.0, 18.0),
                (3.0, 6.0),
            ]);
            line(&[(3.0, 11.0), (20.0, 11.0)]);
        }
        Ico::Save | Ico::SaveAs => {
            line(&[
                (4.0, 4.0),
                (16.0, 4.0),
                (20.0, 8.0),
                (20.0, 20.0),
                (4.0, 20.0),
                (4.0, 4.0),
            ]);
            line(&[(8.0, 4.0), (8.0, 9.0), (15.0, 9.0), (15.0, 4.0)]);
            rect_s(8.0, 13.0, 8.0, 5.0);
            if ico == Ico::SaveAs {
                p.circle_filled(at(19.0, 19.0), 4.2 * s, color);
                p.line_segment([at(17.5, 20.5), at(20.5, 17.5)], Stroke::new(w, Color32::WHITE));
            }
        }
        Ico::Export | Ico::Download => {
            line(&[(12.0, 3.0), (12.0, 15.0)]);
            line(&[(7.0, 10.0), (12.0, 15.0), (17.0, 10.0)]);
            line(&[(4.0, 15.0), (4.0, 20.0), (20.0, 20.0), (20.0, 15.0)]);
        }
        Ico::Sample => {
            dot(8.0, 17.0, 3.0);
            line(&[(10.6, 17.0), (10.6, 5.0), (18.0, 3.5), (18.0, 15.0)]);
            dot(15.5, 15.0, 3.0);
        }
        Ico::Undo | Ico::Redo => {
            let f = if ico == Ico::Undo { 1.0 } else { -1.0 };
            let x = |v: f32| 12.0 + f * (v - 12.0);
            arc(12.0, 14.0, 7.0, PI, 2.0 * PI);
            fill(&[(x(5.0), 17.5), (x(1.5), 11.0), (x(9.0), 12.0)]);
        }
        Ico::Delete => {
            line(&[(4.0, 6.0), (20.0, 6.0)]);
            line(&[(9.0, 6.0), (9.0, 3.5), (15.0, 3.5), (15.0, 6.0)]);
            line(&[(6.0, 6.0), (7.0, 21.0), (17.0, 21.0), (18.0, 6.0)]);
            line(&[(10.0, 10.0), (10.0, 17.0)]);
            line(&[(14.0, 10.0), (14.0, 17.0)]);
        }
        Ico::SelectAll => {
            for (x, y) in [(3.0, 3.0), (3.0, 21.0), (21.0, 3.0), (21.0, 21.0)] {
                dot(x, y, 1.2);
            }
            for i in 0..5 {
                let v = 6.0 + i as f32 * 3.0;
                dot(v, 3.0, 0.9);
                dot(v, 21.0, 0.9);
                dot(3.0, v, 0.9);
                dot(21.0, v, 0.9);
            }
            rect_f(8.0, 8.0, 8.0, 8.0);
        }
        Ico::Pen => {
            p.line_segment([at(17.0, 4.0), at(6.5, 14.5)], Stroke::new(4.0 * s, color));
            fill(&[(4.8, 12.6), (8.4, 16.2), (3.0, 19.0)]);
            line(&[(3.0, 21.0), (21.0, 21.0)]);
        }
        Ico::Hand => {
            // An open hand: four fingers, the thumb and the palm, filled.
            let finger = |x: f32, top: f32, bottom: f32| {
                p.line_segment([at(x, top), at(x, bottom)], Stroke::new(3.0 * s, color));
                p.circle_filled(at(x, top), 1.5 * s, color);
            };
            finger(8.5, 6.5, 13.0);
            finger(11.5, 4.0, 13.0);
            finger(14.5, 4.5, 13.0);
            finger(17.5, 7.0, 13.0);
            p.line_segment([at(8.0, 16.5), at(4.2, 11.8)], Stroke::new(3.0 * s, color));
            p.circle_filled(at(4.2, 11.8), 1.5 * s, color);
            fill(&[
                (7.0, 12.0),
                (19.0, 12.0),
                (19.0, 17.0),
                (16.5, 21.0),
                (9.5, 21.0),
                (7.0, 17.0),
            ]);
        }
        Ico::Keys => {
            rect_s(2.5, 5.0, 19.0, 14.0);
            for x in [7.25, 12.0, 16.75] {
                line(&[(x, 5.0), (x, 19.0)]);
            }
            for x in [5.6, 10.4, 15.1] {
                rect_f(x, 5.0, 3.2, 8.0);
            }
        }
        Ico::Play => fill(&[(6.0, 4.0), (20.0, 12.0), (6.0, 20.0)]),
        Ico::Stop => rect_f(5.0, 5.0, 14.0, 14.0),
        Ico::Mixer => {
            for (x, k) in [(6.0, 15.0), (12.0, 8.0), (18.0, 12.0)] {
                line(&[(x, 3.0), (x, 21.0)]);
                rect_f(x - 2.5, k - 1.5, 5.0, 3.0);
            }
        }
        Ico::Speaker => {
            fill(&[
                (3.0, 9.0),
                (8.0, 9.0),
                (13.0, 4.0),
                (13.0, 20.0),
                (8.0, 15.0),
                (3.0, 15.0),
            ]);
            arc(13.0, 12.0, 4.0, -0.9, 0.9);
            arc(13.0, 12.0, 8.0, -0.9, 0.9);
        }
        Ico::Page => {
            line(&[
                (6.0, 3.0),
                (14.0, 3.0),
                (18.0, 7.0),
                (18.0, 21.0),
                (6.0, 21.0),
                (6.0, 3.0),
            ]);
            for y in [10.0, 13.0, 16.0] {
                line(&[(9.0, y), (15.0, y)]);
            }
        }
        Ico::Horizontal => {
            rect_s(1.5, 7.0, 21.0, 10.0);
            for y in [10.0, 12.0, 14.0] {
                line(&[(3.5, y), (20.5, y)]);
            }
        }
        Ico::Vertical => {
            rect_s(6.0, 1.5, 12.0, 21.0);
            for y in [5.0, 7.0, 11.0, 13.0, 17.0, 19.0] {
                line(&[(8.0, y), (16.0, y)]);
            }
        }
        Ico::ZoomIn | Ico::ZoomOut | Ico::Zoom100 => {
            circle(10.0, 10.0, 6.5);
            line(&[(15.0, 15.0), (21.0, 21.0)]);
            match ico {
                Ico::ZoomIn => {
                    line(&[(7.0, 10.0), (13.0, 10.0)]);
                    line(&[(10.0, 7.0), (10.0, 13.0)]);
                }
                Ico::ZoomOut => line(&[(7.0, 10.0), (13.0, 10.0)]),
                _ => text(10.0, 10.0, "1:1", 6.5),
            }
        }
        Ico::Text => {
            line(&[(5.0, 5.0), (19.0, 5.0)]);
            line(&[(12.0, 5.0), (12.0, 20.0)]);
            line(&[(9.0, 20.0), (15.0, 20.0)]);
        }
        Ico::Lyrics => {
            staff();
            dot(8.0, 15.5, 2.0);
            line(&[(9.8, 15.5), (9.8, 6.0)]);
            text(12.0, 21.0, "la-la", 7.0);
        }
        Ico::Chord => text(12.0, 12.0, "C7", 14.0),
        Ico::Tempo => {
            dot(8.0, 16.0, 2.6);
            line(&[(10.3, 16.0), (10.3, 5.0)]);
            text(17.0, 14.0, "=", 12.0);
        }
        Ico::Rehearsal => {
            rect_s(4.0, 4.0, 16.0, 16.0);
            text(12.0, 12.5, "A", 12.0);
        }
        Ico::Fingering => text(12.0, 12.0, "1 2 3", 9.0),
        Ico::MeasurePlus | Ico::MeasureInsert | Ico::MeasureMinus => {
            for y in [6.0, 9.0, 12.0, 15.0, 18.0] {
                line(&[(2.0, y), (14.0, y)]);
            }
            line(&[(2.0, 6.0), (2.0, 18.0)]);
            line(&[(14.0, 6.0), (14.0, 18.0)]);
            match ico {
                Ico::MeasurePlus => {
                    line(&[(19.0, 8.0), (19.0, 16.0)]);
                    line(&[(15.5, 12.0), (22.5, 12.0)]);
                }
                Ico::MeasureInsert => {
                    line(&[(21.0, 12.0), (16.0, 12.0)]);
                    line(&[(18.5, 9.5), (16.0, 12.0), (18.5, 14.5)]);
                }
                _ => line(&[(15.5, 12.0), (22.5, 12.0)]),
            }
        }
        Ico::LineBreak => {
            line(&[(20.0, 5.0), (20.0, 14.0), (5.0, 14.0)]);
            line(&[(9.0, 10.0), (5.0, 14.0), (9.0, 18.0)]);
        }
        Ico::PageBreak => {
            line(&[(5.0, 3.0), (5.0, 9.0), (19.0, 9.0), (19.0, 3.0)]);
            line(&[(5.0, 21.0), (5.0, 15.0), (19.0, 15.0), (19.0, 21.0)]);
            for x in [3.0, 7.0, 11.0, 15.0, 19.0] {
                line(&[(x, 12.0), (x + 2.0, 12.0)]);
            }
        }
        Ico::Lock | Ico::Unlock => {
            rect_s(5.0, 11.0, 14.0, 10.0);
            if ico == Ico::Lock {
                line(&[
                    (8.0, 11.0),
                    (8.0, 7.5),
                    (10.0, 4.5),
                    (14.0, 4.5),
                    (16.0, 7.5),
                    (16.0, 11.0),
                ]);
            } else {
                line(&[
                    (8.0, 11.0),
                    (8.0, 7.5),
                    (10.0, 4.5),
                    (14.0, 4.5),
                    (16.0, 7.5),
                    (16.0, 8.5),
                ]);
            }
            dot(12.0, 16.0, 1.4);
        }
        Ico::SectionBreak => {
            line(&[(20.0, 4.0), (20.0, 12.0), (6.0, 12.0)]);
            line(&[(10.0, 8.0), (6.0, 12.0), (10.0, 16.0)]);
            line(&[(4.0, 20.0), (20.0, 20.0)]);
            line(&[(4.0, 4.0), (4.0, 20.0)]);
        }
        Ico::SpacerUp | Ico::SpacerDown | Ico::SpacerFixed => {
            line(&[(4.0, 3.0), (20.0, 3.0)]);
            line(&[(4.0, 21.0), (20.0, 21.0)]);
            line(&[(12.0, 5.0), (12.0, 19.0)]);
            match ico {
                Ico::SpacerUp => line(&[(8.0, 9.0), (12.0, 5.0), (16.0, 9.0)]),
                Ico::SpacerDown => line(&[(8.0, 15.0), (12.0, 19.0), (16.0, 15.0)]),
                _ => {
                    line(&[(8.0, 9.0), (12.0, 5.0), (16.0, 9.0)]);
                    line(&[(8.0, 15.0), (12.0, 19.0), (16.0, 15.0)]);
                }
            }
        }
        Ico::PageSetup => {
            rect_s(5.0, 3.0, 14.0, 18.0);
            p.rect_stroke(
                Rect::from_min_size(at(8.0, 6.0), Vec2::new(8.0 * s, 12.0 * s)),
                0.0,
                Stroke::new(w * 0.7, color.linear_multiply(0.6)),
                egui::StrokeKind::Middle,
            );
        }
        Ico::Spacing => {
            line(&[(3.0, 12.0), (21.0, 12.0)]);
            line(&[(6.0, 9.0), (3.0, 12.0), (6.0, 15.0)]);
            line(&[(18.0, 9.0), (21.0, 12.0), (18.0, 15.0)]);
            dot(9.0, 6.0, 1.8);
            dot(15.0, 6.0, 1.8);
        }
        Ico::Transpose => {
            line(&[(8.0, 20.0), (8.0, 4.0)]);
            line(&[(4.0, 8.0), (8.0, 4.0), (12.0, 8.0)]);
            line(&[(16.0, 4.0), (16.0, 20.0)]);
            line(&[(12.0, 16.0), (16.0, 20.0), (20.0, 16.0)]);
        }
        Ico::Up | Ico::OctaveUp => {
            line(&[(12.0, 20.0), (12.0, 5.0)]);
            line(&[(6.0, 11.0), (12.0, 5.0), (18.0, 11.0)]);
            if ico == Ico::OctaveUp {
                text(19.0, 18.0, "8", 8.0);
            }
        }
        Ico::Down | Ico::OctaveDown => {
            line(&[(12.0, 4.0), (12.0, 19.0)]);
            line(&[(6.0, 13.0), (12.0, 19.0), (18.0, 13.0)]);
            if ico == Ico::OctaveDown {
                text(19.0, 6.0, "8", 8.0);
            }
        }
        Ico::Tie => arc(12.0, 17.0, 9.0, PI * 1.1, PI * 1.9),
        Ico::Slur => {
            arc(12.0, 19.0, 10.0, PI * 1.1, PI * 1.9);
            dot(3.5, 16.0, 1.6);
            dot(20.5, 16.0, 1.6);
        }
        Ico::Cresc => line(&[(20.0, 7.0), (4.0, 12.0), (20.0, 17.0)]),
        Ico::Dim => line(&[(4.0, 7.0), (20.0, 12.0), (4.0, 17.0)]),
        Ico::Grid => {
            for (x, y) in [(4.0, 4.0), (13.0, 4.0), (4.0, 13.0), (13.0, 13.0)] {
                rect_s(x, y, 7.0, 7.0);
            }
        }
        Ico::Sliders => {
            for (y, k) in [(6.0, 15.0), (12.0, 8.0), (18.0, 13.0)] {
                line(&[(3.0, y), (21.0, y)]);
                dot(k, y, 2.3);
            }
        }
        Ico::Instruments => {
            // A treble clef from the music font.
            crate::glyph::paint(p, '\u{E050}', at(8.0, 17.0), 20.0 * s, color);
        }
        Ico::Globe => {
            circle(12.0, 12.0, 9.0);
            line(&[(3.0, 12.0), (21.0, 12.0)]);
            arc(12.0, 12.0, 9.0, -PI / 2.0, PI / 2.0);
            p.add(Shape::ellipse_stroke(
                at(12.0, 12.0),
                Vec2::new(4.0 * s, 9.0 * s),
                st,
            ));
        }
        Ico::Theme => {
            circle(12.0, 12.0, 4.0);
            for i in 0..8 {
                let a = i as f32 * PI / 4.0;
                line(&[
                    (12.0 + 6.5 * a.cos(), 12.0 + 6.5 * a.sin()),
                    (12.0 + 9.0 * a.cos(), 12.0 + 9.0 * a.sin()),
                ]);
            }
        }
        Ico::Help => {
            circle(12.0, 12.0, 9.0);
            text(12.0, 12.5, "?", 12.0);
        }
        Ico::Info => {
            circle(12.0, 12.0, 9.0);
            text(12.0, 12.5, "i", 12.0);
        }
        Ico::Plus => {
            line(&[(12.0, 5.0), (12.0, 19.0)]);
            line(&[(5.0, 12.0), (19.0, 12.0)]);
        }
        Ico::Close => {
            line(&[(6.0, 6.0), (18.0, 18.0)]);
            line(&[(18.0, 6.0), (6.0, 18.0)]);
        }
        Ico::Collapse => line(&[(6.0, 15.0), (12.0, 9.0), (18.0, 15.0)]),
        Ico::Expand => line(&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]),
        Ico::Reset => {
            arc(12.0, 12.0, 7.5, -PI * 0.2, PI * 1.5);
            fill(&[(12.0, 1.5), (16.5, 4.5), (12.0, 7.5)]);
        }
        Ico::Fonts => {
            // A big serif A and a small a.
            line(&[(2.5, 20.0), (8.5, 4.0), (14.5, 20.0)]);
            line(&[(5.0, 14.5), (12.0, 14.5)]);
            circle(18.5, 16.8, 3.2);
            line(&[(21.7, 13.0), (21.7, 20.0)]);
        }
        Ico::Heading => {
            line(&[(4.0, 3.0), (4.0, 13.0)]);
            line(&[(12.0, 3.0), (12.0, 13.0)]);
            line(&[(4.0, 8.0), (12.0, 8.0)]);
            line(&[(4.0, 17.0), (20.0, 17.0)]);
            line(&[(4.0, 21.0), (16.0, 21.0)]);
        }
        Ico::TextBlock => {
            for (y, x1) in [(5.0, 20.0), (9.5, 20.0), (14.0, 20.0), (18.5, 13.0)] {
                line(&[(4.0, y), (x1, y)]);
            }
        }
        Ico::Header | Ico::PageNumber => {
            rect_s(5.0, 2.5, 14.0, 19.0);
            if ico == Ico::Header {
                // A page with its header and footer.
                rect_f(7.5, 5.0, 9.0, 2.6);
                rect_f(7.5, 16.4, 9.0, 2.6);
            } else {
                text(14.5, 7.0, "2", 7.5);
            }
            for y in [10.0, 12.5] {
                line(&[(8.0, y), (16.0, y)]);
            }
        }
    }
}
