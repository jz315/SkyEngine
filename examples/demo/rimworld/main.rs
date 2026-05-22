mod model;
mod systems;

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, RunnerPlugin,
    SetupContext, WindowPlugin,
};
use sky_engine::asset::{cook, AssetConfig, Assets, TextureAsset};
use sky_engine::ecs::World;
use sky_engine::math::{Projection, Transform, Vec2};
use sky_engine::render::{
    CameraMarker, Color, MainCamera, RenderPipelineAsset, SortingLayer, SpriteFeature,
    SpriteRenderer, TransparentPhase,
};

use crate::model::{
    cell_world, CellVisual, GameState, MapState, PawnJob, PawnState, PawnVisual, RimworldTextures,
    SurfaceInfo, WorldVisuals, GRID_HEIGHT, GRID_WIDTH, TILE_SIZE,
};

struct RimworldApp;

impl AppState for RimworldApp {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        let asset_dir = rimworld_asset_dir();
        let asset_server = world
            .get_resource::<Assets>()
            .expect("Rimworld should install its Assets before setup")
            .clone();
        let grass = asset_server
            .load::<TextureAsset>(asset_dir.join("grass.png"))
            .expect("grass texture should be in the cooked manifest");
        let tree = asset_server
            .load::<TextureAsset>(asset_dir.join("tree.png"))
            .expect("tree texture should be in the cooked manifest");
        let pawn = asset_server
            .load::<TextureAsset>(asset_dir.join("person.png"))
            .expect("pawn texture should be in the cooked manifest");

        let ground_entities = world
            .get_resource::<WorldVisuals>()
            .map(|visuals| {
                visuals
                    .cells
                    .iter()
                    .map(|visual| visual.ground)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for entity in ground_entities {
            if let Some(sprite) = world.get_mut::<SpriteRenderer>(entity) {
                sprite.texture = Some(grass.clone());
                sprite.color = Color::WHITE;
            }
        }

        let pawn_entities = world
            .get_resource::<GameState>()
            .map(|game| {
                game.pawns
                    .iter()
                    .map(|pawn_state| (pawn_state.visual.body, pawn_state.tint))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (entity, tint) in pawn_entities {
            if let Some(sprite) = world.get_mut::<SpriteRenderer>(entity) {
                sprite.texture = Some(pawn.clone());
                sprite.color = tint;
            }
        }

        world.insert_resource(RimworldTextures { tree });
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        let size = ctx.logical_view_size().to_array();
        if let Some(surface) = ctx.world.get_resource_mut::<SurfaceInfo>() {
            surface.size = size;
        }
        ctx.world.tick_with_delta(ctx.dt);
        ctx.render();
        if let Some(title) = ctx
            .world
            .get_resource::<GameState>()
            .map(|game| game.title.clone())
        {
            ctx.set_title(&title);
        }
    }
}

fn main() {
    let mut world = build_world();
    world.insert_resource(create_rimworld_assets());
    systems::install_systems(&mut world);
    world
        .install(WindowPlugin::new("SkyEngine — Rimworld Prototype", 1280, 720).with_vsync(false))
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
                .add_phase(TransparentPhase::new())
                .build(),
        ))
        .unwrap();

    App::new(world).run(RimworldApp);
}

fn rimworld_asset_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("demo")
        .join("rimworld")
        .join("asset")
}

fn create_rimworld_assets() -> Assets {
    let config = AssetConfig::new(rimworld_asset_dir(), AssetConfig::default_target());
    cook::cook_all(&config).expect("rimworld texture assets should cook");
    Assets::new(config).expect("rimworld assets should load cooked manifest")
}

fn build_world() -> World {
    let mut world = World::new();
    world.insert_resource(MapState::new());
    world.insert_resource(SurfaceInfo {
        size: [1280.0, 720.0],
    });

    let mut game = GameState::default();

    let camera_entity = world.spawn((
        Transform::from_xyz(game.camera_center.x(), game.camera_center.y(), 0.0),
        CameraMarker::new(),
        Projection::orthographic(720.0),
        MainCamera,
    ));

    world.spawn((
        Transform::from_xyz(
            GRID_WIDTH as f32 * TILE_SIZE * 0.5,
            GRID_HEIGHT as f32 * TILE_SIZE * 0.5,
            -0.05,
        ),
        SpriteRenderer::new(
            GRID_WIDTH as f32 * TILE_SIZE + 400.0,
            GRID_HEIGHT as f32 * TILE_SIZE + 400.0,
        )
        .color(Color::rgb(0.075, 0.076, 0.088)),
        SortingLayer(-10),
    ));

    let mut cell_visuals = Vec::with_capacity(GRID_WIDTH * GRID_HEIGHT);
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            let world_pos = cell_world(x as i32, y as i32);
            let ground = world.spawn((
                Transform::from_xyz(world_pos[0], world_pos[1], 0.02),
                SpriteRenderer::new(TILE_SIZE - 1.0, TILE_SIZE - 1.0),
                SortingLayer(0),
            ));
            let zone = world.spawn((
                Transform::from_xyz(world_pos[0], world_pos[1], 0.08),
                SpriteRenderer::new(TILE_SIZE - 7.0, TILE_SIZE - 7.0),
                SortingLayer(1),
            ));
            let content = world.spawn((
                Transform::from_xyz(world_pos[0], world_pos[1], 0.14),
                SpriteRenderer::new(TILE_SIZE - 8.0, TILE_SIZE - 8.0),
                SortingLayer(2),
            ));
            let overlay = world.spawn((
                Transform::from_xyz(world_pos[0], world_pos[1], 0.20),
                SpriteRenderer::new(TILE_SIZE - 4.0, TILE_SIZE - 4.0),
                SortingLayer(3),
            ));
            cell_visuals.push(CellVisual {
                ground,
                zone,
                content,
                overlay,
            });
        }
    }

    let hover_pos = cell_world(0, 0);
    let hover_entity = world.spawn((
        Transform::from_xyz(hover_pos[0], hover_pos[1], 0.26),
        SpriteRenderer::new(TILE_SIZE - 2.0, TILE_SIZE - 2.0)
            .color(Color::new(1.0, 1.0, 1.0, 0.14))
            .visible(false),
        SortingLayer(4),
    ));
    let selection_entity = world.spawn((
        Transform::from_xyz(hover_pos[0], hover_pos[1], 0.30),
        SpriteRenderer::new(TILE_SIZE - 6.0, TILE_SIZE - 6.0)
            .color(Color::new(1.0, 0.92, 0.28, 0.30))
            .visible(false),
        SortingLayer(5),
    ));

    spawn_pawn(
        &mut world,
        &mut game,
        "Nova",
        [8, 8],
        Color::rgb(0.92, 0.76, 0.60),
    );
    spawn_pawn(
        &mut world,
        &mut game,
        "Ari",
        [11, 7],
        Color::rgb(0.66, 0.86, 1.0),
    );
    spawn_pawn(
        &mut world,
        &mut game,
        "Milo",
        [9, 10],
        Color::rgb(1.0, 0.78, 0.44),
    );

    world.insert_resource(game);
    world.insert_resource(WorldVisuals {
        cells: cell_visuals,
        hover_entity,
        selection_entity,
        camera_entity,
    });

    world
}

fn spawn_pawn(
    world: &mut World,
    game: &mut GameState,
    name: &'static str,
    cell: [i32; 2],
    tint: Color,
) {
    let pos = Vec2::from_array(cell_world(cell[0], cell[1]));
    let shadow = world.spawn((
        Transform::from_xyz(pos.x(), pos.y() - 5.0, 0.30),
        SpriteRenderer::new(TILE_SIZE * 0.52, TILE_SIZE * 0.26)
            .color(Color::new(0.0, 0.0, 0.0, 0.25)),
        SortingLayer(4),
    ));
    let body = world.spawn((
        Transform::from_xyz(pos.x(), pos.y(), 0.40),
        SpriteRenderer::new(TILE_SIZE * 0.82, TILE_SIZE * 0.82).color(tint),
        SortingLayer(6),
    ));
    game.pawns.push(PawnState {
        name,
        pos,
        visual: PawnVisual { body, shadow },
        tint,
        carrying: None,
        hunger: 0.82,
        rest: 0.78,
        job: PawnJob::Idle,
    });
}
