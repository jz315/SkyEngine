use sky_engine::ui::serein::Ui;

use crate::theme::{self, AppTheme};

pub fn draw(ui: &mut Ui, width: f32, height: f32, app_theme: AppTheme) {
    ui.rect("fog-station.background")
        .size(width, height)
        .z_index(-100)
        .gradient(app_theme.background_top, app_theme.background_bottom)
        .build();

    ui.rect("fog-station.background.floor-glow")
        .x(0.0)
        .y((height - 140.0).max(0.0))
        .size(width, 140.0_f32.min(height))
        .z_index(-99)
        .gradient(
            theme::color(0.02, 0.02, 0.02, 0.0),
            theme::color(0.16, 0.11, 0.08, 0.22),
        )
        .build();
}
