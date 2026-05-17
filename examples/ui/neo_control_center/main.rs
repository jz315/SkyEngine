mod actions;
mod app;
mod locale;
mod model;
mod theme;
mod view;

use sky_engine::app::{App, AppConfig};
use sky_engine::ecs::World;
use sky_engine::render::RenderPipelineAsset;

fn main() {
    App::new(
        AppConfig::new("Neo Control Center", 1440, 920)
            .with_vsync(true)
            .with_resizable(true)
            .with_frame_rate_limit(60.0),
        World::new(),
    )
    .with_render_pipeline(RenderPipelineAsset::forward_2d())
    .run(app::NeoControlCenter::default());
}
