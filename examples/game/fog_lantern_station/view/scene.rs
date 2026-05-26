use sky_engine::ui::neo::{Color, ImageRef, Ui};

use crate::model::Location;
use crate::theme::{self, AppTheme};

const ASSET_ROOT: &str = "examples/assets/fog_lantern_station";

pub fn draw_station_scene(
    ui: &mut Ui,
    id: &str,
    width: f32,
    height: f32,
    location: Location,
    app_theme: AppTheme,
) {
    ui.stack(id.to_string())
        .size(width, height)
        .clip()
        .z_index(1)
        .content(|ui| {
            ui.image(format!("{id}.image"))
                .size(width, height)
                .source(ImageRef::path(location_asset(location)))
                .cover()
                .radius(8.0)
                .z_index(0)
                .build();

            draw_color_grade(ui, id, width, height, app_theme);
            draw_frame(ui, id, width, height, app_theme);
        });
}

fn location_asset(location: Location) -> String {
    let name = match location {
        Location::WaitingHall => "waiting_hall",
        Location::TicketOffice => "ticket_office",
        Location::LostAndFound => "lost_and_found",
        Location::Underpass => "underpass",
        Location::ClockTower => "clock_tower",
        Location::Platform => "platform",
    };
    format!(
        "{}/{ASSET_ROOT}/{name}.png",
        env!("CARGO_MANIFEST_DIR").replace('\\', "/")
    )
}

fn draw_color_grade(ui: &mut Ui, id: &str, width: f32, height: f32, app_theme: AppTheme) {
    ui.rect(format!("{id}.grade.top"))
        .size(width, height)
        .z_index(1)
        .gradient(
            theme::alpha(app_theme.background_top, 0.10),
            theme::alpha(Color::BLACK, 0.24),
        )
        .radius(8.0)
        .build();

    ui.rect(format!("{id}.vignette"))
        .size(width, height)
        .z_index(1)
        .color(theme::alpha(Color::BLACK, 0.08))
        .border(1.0, theme::alpha(app_theme.border, 0.62))
        .radius(8.0)
        .build();
}

fn draw_frame(ui: &mut Ui, id: &str, width: f32, height: f32, app_theme: AppTheme) {
    ui.rect(format!("{id}.frame"))
        .size(width, height)
        .z_index(3)
        .color(Color::TRANSPARENT)
        .border(1.0, theme::alpha(app_theme.border, 0.74))
        .radius(8.0)
        .build();
}
