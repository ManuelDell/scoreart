//! Page settings (paper size, orientation, margins, staff size, distances) and the view mode
//! (page, continuous horizontal or vertical).

use egui::RichText;
use scoreart_engine::View;
use scoreart_model::PageFormat;
use serde_json::json;

use crate::ScoreApp;

use crate::i18n::{t, tf};

/// Name shown for a view mode.
pub(crate) fn view_label(v: View) -> &'static str {
    match v {
        View::Page => t("Page view"),
        View::Horizontal => t("Continuous view (horizontal)"),
        View::Vertical => t("Continuous view (vertical)"),
    }
}

pub(crate) fn set_view(app: &mut ScoreApp, v: View) {
    let mode = match v {
        View::Page => "page",
        View::Horizontal => "horizontal",
        View::Vertical => "vertical",
    };
    app.run("view.mode", json!({ "mode": mode }));
}

/// The three views as selectable entries.
pub(crate) fn view_choices(app: &mut ScoreApp, ui: &mut egui::Ui) {
    for v in [View::Page, View::Horizontal, View::Vertical] {
        if ui
            .selectable_label(app.session.view == v, view_label(v))
            .clicked()
        {
            set_view(app, v);
            ui.close();
        }
    }
}

pub(crate) fn open(app: &mut ScoreApp) {
    app.page_settings = Some(app.session.score.page);
}

fn apply(app: &mut ScoreApp, f: &PageFormat) {
    app.run(
        "page.format",
        json!({
            "width_mm": f.width_mm,
            "height_mm": f.height_mm,
            "margin_top_mm": f.margin_top_mm,
            "margin_bottom_mm": f.margin_bottom_mm,
            "margin_left_mm": f.margin_left_mm,
            "margin_right_mm": f.margin_right_mm,
            "two_sided": f.two_sided,
            "even_margin_top_mm": f.even_margin_top_mm,
            "even_margin_bottom_mm": f.even_margin_bottom_mm,
            "even_margin_left_mm": f.even_margin_left_mm,
            "even_margin_right_mm": f.even_margin_right_mm,
            "staff_space_mm": f.staff_space_mm,
            "staff_gap": f.staff_gap,
            "part_gap": f.part_gap,
            "system_gap": f.system_gap,
            "max_system_gap": f.max_system_gap,
            "music_top": f.music_top,
            "music_bottom": f.music_bottom,
            "vertical_justify": f.vertical_justify,
            "max_system_spread": f.max_system_spread,
            "max_staff_spread": f.max_staff_spread,
            "spread_factor": f.spread_factor,
            "last_system_fill": f.last_system_fill,
            "first_page_number": f.first_page_number,
            "first_indent": f.first_indent,
            "spacing": f.spacing,
        }),
    );
}

/// A length in millimetres, shown in millimetres or inches.
fn length(ui: &mut egui::Ui, v: &mut f32, lo: f32, hi: f32, inch: bool) {
    let k = if inch { 25.4 } else { 1.0 };
    let mut shown = *v / k;
    let r = ui.add(
        egui::DragValue::new(&mut shown)
            .range(lo / k..=hi / k)
            .speed(if inch { 0.01 } else { 0.5 })
            .suffix(if inch { " in" } else { " mm" })
            .max_decimals(if inch { 3 } else { 1 }),
    );
    if r.changed() {
        *v = shown * k;
    }
}

fn sp(ui: &mut egui::Ui, v: &mut f32, lo: f32, hi: f32) {
    ui.add(
        egui::DragValue::new(v)
            .range(lo..=hi)
            .speed(0.1)
            .suffix(" sp")
            .max_decimals(2),
    );
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong());
    ui.separator();
}

pub(crate) fn dialog(app: &mut ScoreApp, ctx: &egui::Context) {
    let Some(mut f) = app.page_settings else { return };
    let mut close = false;
    let mut apply_now = false;
    let inch_id = egui::Id::unique("page-settings-inch");
    let mut inch = ctx.data(|d| d.get_temp::<bool>(inch_id)).unwrap_or(false);
    let resp = egui::Modal::new(egui::Id::unique("page-settings")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.heading(t("Page settings"));
        let max_h = (ctx.content_rect().height() - 160.0).clamp(240.0, 640.0);
        egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
            section(ui, t("Page"));
            egui::Grid::new("page-grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(t("Paper size"));
                    let current = f.size_name().unwrap_or(t("Custom"));
                    egui::ComboBox::from_id_salt("page-size")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for (name, w, h) in PageFormat::SIZES {
                                let label = format!("{name}  ({w:.0} × {h:.0} mm)");
                                if ui.selectable_label(current == *name, label).clicked() {
                                    (f.width_mm, f.height_mm) =
                                        if f.landscape() { (*h, *w) } else { (*w, *h) };
                                }
                            }
                        });
                    ui.end_row();
                    ui.label(t("Width × height"));
                    ui.horizontal(|ui| {
                        length(ui, &mut f.width_mm, 50.0, 2000.0, inch);
                        ui.label("×");
                        length(ui, &mut f.height_mm, 50.0, 2000.0, inch);
                    });
                    ui.end_row();
                    ui.label(t("Orientation"));
                    ui.horizontal(|ui| {
                        let landscape = f.landscape();
                        if ui.selectable_label(!landscape, t("Portrait")).clicked() && landscape {
                            (f.width_mm, f.height_mm) = (f.height_mm, f.width_mm);
                        }
                        if ui.selectable_label(landscape, t("Landscape")).clicked() && !landscape {
                            (f.width_mm, f.height_mm) = (f.height_mm, f.width_mm);
                        }
                    });
                    ui.end_row();
                    ui.label(t("Unit"));
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut inch, false, t("Millimetres"));
                        ui.selectable_value(&mut inch, true, t("Inches"));
                    });
                    ui.end_row();
                    ui.label(t("Two-sided"));
                    let was = f.two_sided;
                    ui.checkbox(
                        &mut f.two_sided,
                        t("Odd and even pages with margins of their own"),
                    );
                    if f.two_sided && !was {
                        // The even pages start mirrored: inside and outside swap.
                        f.even_margin_top_mm = f.margin_top_mm;
                        f.even_margin_bottom_mm = f.margin_bottom_mm;
                        f.even_margin_left_mm = f.margin_right_mm;
                        f.even_margin_right_mm = f.margin_left_mm;
                    }
                    ui.end_row();
                });
            ui.add_space(4.0);
            egui::Grid::new("margins")
                .num_columns(5)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("");
                    for h in [t("Top"), t("Bottom"), t("Left"), t("Right")] {
                        ui.label(RichText::new(h).small());
                    }
                    ui.end_row();
                    ui.label(if f.two_sided { t("Odd pages") } else { t("Margins") });
                    length(ui, &mut f.margin_top_mm, 0.0, 100.0, inch);
                    length(ui, &mut f.margin_bottom_mm, 0.0, 100.0, inch);
                    length(ui, &mut f.margin_left_mm, 0.0, 100.0, inch);
                    length(ui, &mut f.margin_right_mm, 0.0, 100.0, inch);
                    ui.end_row();
                    ui.add_enabled_ui(f.two_sided, |ui| ui.label(t("Even pages")));
                    for v in [
                        &mut f.even_margin_top_mm,
                        &mut f.even_margin_bottom_mm,
                        &mut f.even_margin_left_mm,
                        &mut f.even_margin_right_mm,
                    ] {
                        ui.add_enabled_ui(f.two_sided, |ui| length(ui, v, 0.0, 100.0, inch));
                    }
                    ui.end_row();
                });
            ui.add_space(4.0);
            egui::Grid::new("page-grid-2")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(t("Staff size"));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut f.staff_space_mm)
                                .range(0.5..=10.0)
                                .speed(0.01)
                                .suffix(" mm")
                                .max_decimals(3),
                        );
                        ui.label(
                            RichText::new(tf(
                                "between lines (staff height {} mm)",
                                &[&format!("{:.1}", f.staff_space_mm * 4.0)],
                            ))
                            .weak(),
                        );
                    });
                    ui.end_row();
                    ui.label(t("First page number"));
                    ui.add(egui::DragValue::new(&mut f.first_page_number).range(-999..=9999));
                    ui.end_row();
                });

            section(ui, t("Distances"));
            egui::Grid::new("dist-grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(t("Music top margin"))
                        .on_hover_text(t("From the top margin to the first staff of a page"));
                    sp(ui, &mut f.music_top, 0.0, 60.0);
                    ui.end_row();
                    ui.label(t("Music bottom margin"))
                        .on_hover_text(t("From the last staff of a page to the bottom margin"));
                    sp(ui, &mut f.music_bottom, 0.0, 60.0);
                    ui.end_row();
                    ui.label(t("Between staves of an instrument"));
                    sp(ui, &mut f.staff_gap, 1.0, 40.0);
                    ui.end_row();
                    ui.label(t("Between instruments"));
                    sp(ui, &mut f.part_gap, 1.0, 40.0);
                    ui.end_row();
                    ui.label(t("Min. system distance"));
                    sp(ui, &mut f.system_gap, 1.0, 60.0);
                    ui.end_row();
                    ui.label(t("Max. system distance"))
                        .on_hover_text(t("Systems move apart up to this distance to fill a page"));
                    sp(ui, &mut f.max_system_gap, 1.0, 100.0);
                    ui.end_row();
                    ui.label(t("Vertical justification"));
                    ui.checkbox(
                        &mut f.vertical_justify,
                        t("Spread the staves inside the systems too"),
                    );
                    ui.end_row();
                    if f.vertical_justify {
                        ui.label(t("Max. system distance (justified)"));
                        sp(ui, &mut f.max_system_spread, 1.0, 100.0);
                        ui.end_row();
                        ui.label(t("Max. staff distance (justified)"));
                        sp(ui, &mut f.max_staff_spread, 1.0, 100.0);
                        ui.end_row();
                        ui.label(t("Factor for distance between systems"));
                        ui.add(
                            egui::DragValue::new(&mut f.spread_factor)
                                .range(0.1..=20.0)
                                .speed(0.05)
                                .max_decimals(2),
                        );
                        ui.end_row();
                    }
                });

            section(ui, t("Systems"));
            egui::Grid::new("sys-grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(t("Last system fill threshold")).on_hover_text(t(
                        "A last system shorter than this share of the line is not stretched",
                    ));
                    let mut pct = f.last_system_fill * 100.0;
                    if ui
                        .add(egui::Slider::new(&mut pct, 0.0..=100.0).step_by(5.0).suffix(" %"))
                        .changed()
                    {
                        f.last_system_fill = pct / 100.0;
                    }
                    ui.end_row();
                    ui.label(t("Indent first system"));
                    ui.horizontal(|ui| {
                        let mut on = f.first_indent > 0.0;
                        if ui.checkbox(&mut on, "").changed() {
                            f.first_indent = if on { 5.0 } else { 0.0 };
                        }
                        if on {
                            sp(ui, &mut f.first_indent, 0.5, 60.0);
                        }
                    });
                    ui.end_row();
                    ui.label(t("Note spacing"));
                    ui.add(egui::Slider::new(&mut f.spacing, 0.5..=2.0).step_by(0.05));
                    ui.end_row();
                });
            ui.label(
                RichText::new(t("sp = staff spaces (the distance between two staff lines)"))
                    .small()
                    .weak(),
            );
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
                .on_hover_text(t("As a new MuseScore 4 score: A4, margins 10 mm (bottom 20 mm)"))
                .clicked()
            {
                f = PageFormat::default();
            }
        });
    });
    ctx.data_mut(|d| d.insert_temp(inch_id, inch));
    if resp.should_close() {
        close = true;
    }
    if apply_now {
        apply(app, &f);
    }
    app.page_settings = if close { None } else { Some(f) };
}
