//! Page settings (paper size, orientation, margins, staff size, distances) and the view mode
//! (page, continuous horizontal or vertical).

use egui::RichText;
use scoreart_engine::View;
use scoreart_model::PageFormat;
use serde_json::json;

use crate::ScoreApp;

/// Name shown for a view mode.
pub(crate) fn view_label(v: View) -> &'static str {
    match v {
        View::Page => "Page view",
        View::Horizontal => "Continuous view (horizontal)",
        View::Vertical => "Continuous view (vertical)",
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
            "staff_space_mm": f.staff_space_mm,
            "staff_gap": f.staff_gap,
            "part_gap": f.part_gap,
            "system_gap": f.system_gap,
            "spacing": f.spacing,
        }),
    );
}

fn mm(ui: &mut egui::Ui, v: &mut f32, lo: f32, hi: f32) {
    ui.add(
        egui::DragValue::new(v)
            .range(lo..=hi)
            .speed(0.5)
            .suffix(" mm")
            .max_decimals(1),
    );
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

pub(crate) fn dialog(app: &mut ScoreApp, ctx: &egui::Context) {
    let Some(mut f) = app.page_settings else { return };
    let mut close = false;
    let mut apply_now = false;
    let resp = egui::Modal::new(egui::Id::unique("page-settings")).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.heading("Page settings");
        ui.add_space(6.0);
        egui::Grid::new("page-grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Paper size");
                let current = f.size_name().unwrap_or("Custom");
                egui::ComboBox::from_id_salt("page-size")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (name, w, h) in PageFormat::SIZES {
                            let label = format!("{name}  ({w:.0} × {h:.0} mm)");
                            if ui.selectable_label(current == *name, label).clicked() {
                                (f.width_mm, f.height_mm) = if f.landscape() { (*h, *w) } else { (*w, *h) };
                            }
                        }
                    });
                ui.end_row();
                ui.label("Width × height");
                ui.horizontal(|ui| {
                    mm(ui, &mut f.width_mm, 50.0, 2000.0);
                    ui.label("×");
                    mm(ui, &mut f.height_mm, 50.0, 2000.0);
                });
                ui.end_row();
                ui.label("Orientation");
                ui.horizontal(|ui| {
                    let landscape = f.landscape();
                    if ui.selectable_label(!landscape, "Portrait").clicked() && landscape {
                        (f.width_mm, f.height_mm) = (f.height_mm, f.width_mm);
                    }
                    if ui.selectable_label(landscape, "Landscape").clicked() && !landscape {
                        (f.width_mm, f.height_mm) = (f.height_mm, f.width_mm);
                    }
                });
                ui.end_row();
                ui.label("Margins");
                egui::Grid::new("margins").num_columns(4).show(ui, |ui| {
                    ui.label("Top");
                    mm(ui, &mut f.margin_top_mm, 0.0, 100.0);
                    ui.label("Bottom");
                    mm(ui, &mut f.margin_bottom_mm, 0.0, 100.0);
                    ui.end_row();
                    ui.label("Left");
                    mm(ui, &mut f.margin_left_mm, 0.0, 100.0);
                    ui.label("Right");
                    mm(ui, &mut f.margin_right_mm, 0.0, 100.0);
                    ui.end_row();
                });
                ui.end_row();
                ui.label("Staff size");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut f.staff_space_mm)
                            .range(0.5..=10.0)
                            .speed(0.01)
                            .suffix(" mm")
                            .max_decimals(2),
                    );
                    ui.label(
                        RichText::new(format!(
                            "between lines (staff height {:.1} mm)",
                            f.staff_space_mm * 4.0
                        ))
                        .weak(),
                    );
                });
                ui.end_row();
                ui.label("Between staves of an instrument");
                sp(ui, &mut f.staff_gap, 1.0, 40.0);
                ui.end_row();
                ui.label("Between instruments");
                sp(ui, &mut f.part_gap, 1.0, 40.0);
                ui.end_row();
                ui.label("Between systems");
                sp(ui, &mut f.system_gap, 1.0, 60.0);
                ui.end_row();
                ui.label("Note spacing");
                ui.add(egui::Slider::new(&mut f.spacing, 0.5..=2.0).step_by(0.05));
                ui.end_row();
            });
        ui.label(
            RichText::new("sp = staff spaces (the distance between two staff lines)")
                .small()
                .weak(),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("OK").clicked() {
                apply_now = true;
                close = true;
            }
            if ui.button("Apply").clicked() {
                apply_now = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
            if ui
                .button("Defaults")
                .on_hover_text("A4 portrait, 15 mm margins")
                .clicked()
            {
                f = PageFormat::default();
            }
        });
    });
    if resp.should_close() {
        close = true;
    }
    if apply_now {
        apply(app, &f);
    }
    app.page_settings = if close { None } else { Some(f) };
}
