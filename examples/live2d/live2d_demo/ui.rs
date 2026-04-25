use sky_engine::app::egui;

use super::benchmark::BenchmarkConfig;
use super::model::ModelSlot;

#[derive(Default)]
pub struct PendingUiActions {
    pub clicked_motion: Option<(usize, usize)>,
    pub clicked_expression: Option<usize>,
}

pub fn draw_live2d_panel(
    egui_ctx: &egui::Context,
    slots: &[ModelSlot],
    active: &mut usize,
    fps_display: f32,
    benchmark: Option<BenchmarkConfig>,
) -> PendingUiActions {
    let mut actions = PendingUiActions::default();

    egui::SidePanel::left("live2d_panel")
        .default_width(200.0)
        .resizable(true)
        .show(egui_ctx, |ui| {
            ui.heading("🎭 Live2D");
            ui.separator();

            ui.label(format!("FPS: {fps_display:.0}"));
            ui.label("U: toggle UI / pure render");
            if let Some(benchmark) = benchmark {
                ui.label(format!(
                    "Benchmark: warmup {} / sample {}",
                    benchmark.warmup_frames, benchmark.sample_frames
                ));
            }
            ui.separator();

            ui.label(format!("Loaded: {}", slots.len()));
            if slots.len() > 1 {
                ui.small("Only the selected model is rendered.");
                ui.add_space(4.0);
                ui.label("Focus");
                for (index, slot) in slots.iter().enumerate() {
                    ui.radio_value(active, index, &slot.name);
                }
                ui.separator();
            }

            let active_slot = &slots[*active];

            ui.strong(&active_slot.name);
            ui.add_space(4.0);

            if !active_slot.motion_groups.is_empty() {
                ui.label("Motions");
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for (group_index, group) in active_slot.motion_groups.iter().enumerate() {
                            ui.collapsing(group.name.as_str(), |ui| {
                                for (motion_index, motion) in group.motions.iter().enumerate() {
                                    if ui.button(&motion.name).clicked() {
                                        actions.clicked_motion = Some((group_index, motion_index));
                                    }
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
                ui.separator();
            } else {
                ui.weak("(no motions)");
                ui.add_space(6.0);
            }

            if !active_slot.expression_names.is_empty() {
                ui.label("Expressions");
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for (index, name) in active_slot.expression_names.iter().enumerate() {
                            if ui.button(name).clicked() {
                                actions.clicked_expression = Some(index);
                            }
                        }
                    });
            } else {
                ui.weak("(no expressions)");
            }
        });

    actions
}
