//! A tiny isometric building game using Kenney's miniature asset packs.
//!
//! ```bash
//! cargo run --example miniature_builder_game --features app --release
//! ```

mod actions;
mod app_bridge;
mod assets;
mod board;
mod camera;
mod geometry;
mod hud;
mod model;
mod preview;
mod projection;
mod scene_view;
mod screenshot;
mod selection;
mod title;

use assets::GameAssets;
use board::{BoardDeltaQueue, BoardIntentQueue, BoardState};
use geometry::{WINDOW_H, WINDOW_W};
use hud::HudState;
use scene_view::{mount_ground_scene, spawn_backdrop};
use screenshot::ScreenshotProbe;
use selection::{BuildSelection, HoverState};
use sky_engine::app::{App, AppConfig, AppState, FrameContext, SetupContext};
use sky_engine::ecs::World;
use sky_engine::render::{
    Color, RenderPipelineAsset, RenderSettings, SpriteFeature, TilemapFeature, TransparentPhase,
};
use title::TitleState;

struct MiniatureBuilder;

impl MiniatureBuilder {
    fn new() -> Self {
        Self
    }
}

impl AppState for MiniatureBuilder {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        let assets = GameAssets::load(world);
        let mut board = BoardState::new();

        spawn_backdrop(world);
        actions::install_actions(world);
        camera::spawn_camera(world);
        board.seed_starter_layout();
        mount_ground_scene(world, &assets, &board);
        preview::spawn_preview_entities(world, &assets);
        projection::setup_structure_projection(world, &assets, &board);
        world.insert_resource(board);
        world.insert_resource(BuildSelection::default());
        world.insert_resource(HoverState::default());
        world.insert_resource(HudState::default());
        world.insert_resource(BoardIntentQueue::default());
        world.insert_resource(BoardDeltaQueue::default());
        world.insert_resource(assets);
        world.insert_resource(ScreenshotProbe::from_env());
        world.insert_resource(TitleState::default());
        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.025, 0.031, 0.032),
            ..Default::default()
        });

        app_bridge::install_app_bridge(world);
        world.group("camera").add(camera::update_camera);
        world
            .group("input")
            .add(selection::update_selection)
            .add(selection::update_hover)
            .add(board::collect_board_intents)
            .add(app_bridge::request_exit_from_actions);
        world.group("simulation").add(board::apply_board_intents);
        world
            .group("projection")
            .add(projection::apply_board_deltas)
            .add(projection::sync_structure_instance);
        world
            .group("presentation")
            .add(preview::update_preview)
            .add(title::update_window_title)
            .add(screenshot::update_screenshot_probe);
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        app_bridge::sync_frame_state(ctx);
        ctx.tick();
        if app_bridge::apply_app_requests(ctx) {
            return;
        }
        ctx.render();
    }
}

fn main() {
    App::new(
        AppConfig::new("SkyEngine - Miniature Builder", WINDOW_W, WINDOW_H)
            .with_vsync(false)
            .with_resizable(true)
            .with_auto_tick(false),
        World::new(),
    )
    .with_render_pipeline(
        RenderPipelineAsset::builder()
            .add_feature(SpriteFeature::unlit())
            .add_feature(TilemapFeature::unlit())
            .add_phase(TransparentPhase::new())
            .build(),
    )
    .run(MiniatureBuilder::new());
}
