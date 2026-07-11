mod components;
mod layout;
mod palette;
mod resources;
mod setup;
mod systems;
mod ui;

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, RunnerPlugin,
    SetupContext, WindowPlugin,
};
use sky_engine::asset::Assets;
use sky_engine::input::KeyCode;
use sky_engine::render::{RenderPipelineAsset, SpriteFeature, TilemapFeature, TransparentPhase};

struct LastLightGuild;

impl AppState for LastLightGuild {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        setup::spawn_guild_tilemap(ctx.world);
        systems::install_systems(ctx.world);
        let ui = ui::spawn_ui(ctx.world);
        ctx.world.insert_resource(ui);
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        ctx.update_ui();

        if ctx.input.key_pressed(KeyCode::Escape) {
            ctx.request_exit();
            return;
        }
        if ctx.input.key_pressed(KeyCode::KeyR) {
            let asset_server = ctx.world.get_resource::<Assets>().cloned();
            *ctx.world = setup::build_world();
            if let Some(asset_server) = asset_server {
                ctx.world.insert_resource(asset_server);
            }
            ctx.world.insert_resource(*ctx.input);
            setup::spawn_guild_tilemap(ctx.world);
            systems::install_systems(ctx.world);
            let ui = ui::spawn_ui(ctx.world);
            ctx.world.insert_resource(ui);
        }

        ctx.world.tick_with_delta(ctx.dt).unwrap();

        ctx.set_title(&systems::title(ctx.world));
        ctx.render();
        ctx.render_ui();
    }
}

fn main() {
    let mut world = setup::build_world();
    world
        .install(
            WindowPlugin::new("Last Light Guild", layout::WINDOW_W, layout::WINDOW_H)
                .with_vsync(true),
        )
        .unwrap();
    world
        .install(RunnerPlugin::game().with_auto_tick(false))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world
        .install(RenderPlugin::pipeline(
            RenderPipelineAsset::builder()
                .add_feature(SpriteFeature::unlit())
                .add_feature(TilemapFeature::unlit())
                .add_phase(TransparentPhase::new())
                .build(),
        ))
        .unwrap();

    App::new(world).run(LastLightGuild);
}
