//! Chunked tilemap rendering demo.
//!
//! ```bash
//! cargo run --example tilemap_demo --features app --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::asset::{Assets, TextureAsset, TextureColorSpace};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SortingLayer,
    SpriteFeature, SpriteRenderer, Tile, TileFlags, TileId, TilemapDescriptor, TilemapFeature,
    TilemapHandle, TilemapRenderer, TilemapStorage, TilesetGrid, Transform, TransparentPhase,
};

const MAP_WIDTH: u32 = 160;
const MAP_HEIGHT: u32 = 96;
const TILE_WIDTH: f32 = 64.0;
const TILE_HEIGHT: f32 = 32.0;
const TILESET_TILE_WIDTH: u32 = 64;
const TILESET_TILE_HEIGHT: u32 = 32;
const TILESET_COLUMNS: u32 = 4;
const TILESET_ROWS: u32 = 4;
const MAP_LAYERS: u32 = 5;
const LAYER_GROUND: u32 = 0;
const LAYER_SHORE: u32 = 1;
const LAYER_DETAIL: u32 = 2;
const LAYER_RIDGE: u32 = 3;
const LAYER_CANOPY: u32 = 4;

struct TilemapDemo {
    map: Option<TilemapHandle>,
    time: f32,
}

impl TilemapDemo {
    fn new() -> Self {
        Self {
            map: None,
            time: 0.0,
        }
    }
}

impl AppState for TilemapDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        let asset_server = world
            .get_resource::<Assets>()
            .expect("App should install Assets before setup")
            .clone();
        let tileset = asset_server.insert_runtime(make_tileset_texture());

        let mut storage = TilemapStorage::new();
        let map = storage.create(TilemapDescriptor::new(MAP_WIDTH, MAP_HEIGHT, MAP_LAYERS));
        populate_map(storage.get_mut(map).expect("new tilemap should exist"));
        world.insert_resource(storage);
        self.map = Some(map);

        world.spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            CameraMarker::new(),
            Projection::orthographic(720.0),
            MainCamera,
        ));

        let [origin_x, origin_y] = centered_isometric_origin();
        let tileset_grid = TilesetGrid::new(
            tileset,
            [TILESET_TILE_WIDTH, TILESET_TILE_HEIGHT],
            TILESET_COLUMNS,
            TILESET_ROWS,
        );
        let layout = TilemapRenderer::new(map, tileset_grid.clone())
            .tile_size([TILE_WIDTH, TILE_HEIGHT])
            .isometric();

        spawn_tilemap_layer(
            world,
            map,
            tileset_grid.clone(),
            [origin_x, origin_y],
            LAYER_GROUND,
            0,
        );
        spawn_tilemap_layer(
            world,
            map,
            tileset_grid.clone(),
            [origin_x, origin_y],
            LAYER_SHORE,
            1,
        );
        spawn_tilemap_layer(
            world,
            map,
            tileset_grid.clone(),
            [origin_x, origin_y],
            LAYER_DETAIL,
            2,
        );

        spawn_character(
            world,
            &layout,
            [88, 56],
            [origin_x, origin_y],
            Color::rgb(0.95, 0.32, 0.28),
        );
        spawn_character(
            world,
            &layout,
            [62, 68],
            [origin_x, origin_y],
            Color::rgb(0.35, 0.65, 1.0),
        );

        spawn_tilemap_layer(
            world,
            map,
            tileset_grid.clone(),
            [origin_x, origin_y],
            LAYER_RIDGE,
            20,
        );
        spawn_tilemap_layer(
            world,
            map,
            tileset_grid,
            [origin_x, origin_y],
            LAYER_CANOPY,
            21,
        );

        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.035, 0.038, 0.045),
            ..Default::default()
        });
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        self.time += ctx.dt;
        animate_water(ctx.world, self.map, self.time);
        ctx.render();
    }
}

fn main() {
    let mut world = World::new();
    world
        .install(WindowPlugin::new("SkyEngine - Tilemap Demo", 1280, 720).with_vsync(false))
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

    App::new(world).run(TilemapDemo::new());
}

fn centered_isometric_origin() -> [f32; 2] {
    let min_x = -((MAP_HEIGHT - 1) as f32) * TILE_WIDTH * 0.5 - TILE_WIDTH * 0.5;
    let max_x = ((MAP_WIDTH - 1) as f32) * TILE_WIDTH * 0.5 + TILE_WIDTH * 0.5;
    let min_y = -TILE_HEIGHT * 0.5;
    let max_y = ((MAP_WIDTH + MAP_HEIGHT - 2) as f32) * TILE_HEIGHT * 0.5 + TILE_HEIGHT * 0.5;
    [-(min_x + max_x) * 0.5, -(min_y + max_y) * 0.5]
}

fn spawn_tilemap_layer(
    world: &mut World,
    map: TilemapHandle,
    tileset: TilesetGrid,
    origin: [f32; 2],
    layer: u32,
    sorting_layer: i32,
) {
    world.spawn((
        Transform::from_xyz(origin[0], origin[1], 0.0),
        TilemapRenderer::new(map, tileset)
            .layer(layer)
            .tile_size([TILE_WIDTH, TILE_HEIGHT])
            .isometric(),
        SortingLayer(sorting_layer),
    ));
}

fn spawn_character(
    world: &mut World,
    layout: &TilemapRenderer,
    cell: [i32; 2],
    map_origin: [f32; 2],
    color: Color,
) {
    let local = layout.cell_to_local_center(cell);
    world.spawn((
        Transform::from_xyz(
            map_origin[0] + local[0],
            map_origin[1] + local[1] + 22.0,
            0.0,
        ),
        SpriteRenderer::new(28.0, 54.0).color(color),
        SortingLayer(12),
    ));
}

fn populate_map(map: &mut sky_engine::render::Tilemap) {
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let terrain = if y < 18 {
                Tile::new(TileId(1))
            } else if y < 24 {
                Tile::new(TileId(0))
            } else {
                let checker = ((x / 5) + (y / 5)) % 2;
                Tile::new(TileId(if checker == 0 { 0 } else { 4 }))
            };
            let _ = map.set_tile(LAYER_GROUND, x, y, terrain);
        }
    }

    for y in 17..27 {
        for x in 0..MAP_WIDTH {
            let band = (y as i32 - 22).abs() as u32;
            if band <= 4 && (x + y + band) % 3 != 0 {
                let tint = if band < 2 {
                    Color::new(1.0, 0.95, 0.72, 0.95)
                } else {
                    Color::new(0.88, 0.92, 0.72, 0.78)
                };
                let _ = map.set_tile(LAYER_SHORE, x, y, Tile::tinted(TileId(2), tint));
            }
        }
    }

    for y in 28..MAP_HEIGHT - 12 {
        for x in 12..MAP_WIDTH - 12 {
            let path_center = MAP_HEIGHT as f32 * 0.54 + (x as f32 * 0.13).sin() * 8.0;
            if (y as f32 - path_center).abs() < 1.8 {
                let _ = map.set_tile(LAYER_DETAIL, x, y, Tile::new(TileId(7)));
            } else if (x * 17 + y * 11) % 97 == 0 {
                let _ = map.set_tile(LAYER_DETAIL, x, y, Tile::new(TileId(8)));
            } else if (x * 19 + y * 5) % 131 == 0 {
                let _ = map.set_tile(LAYER_DETAIL, x, y, Tile::new(TileId(9)));
            }
        }
    }

    for x in 12..MAP_WIDTH - 12 {
        let ridge = 52 + ((x as f32 * 0.18).sin() * 6.0) as i32;
        for y in ridge..ridge + 3 {
            if (0..MAP_HEIGHT as i32).contains(&y) {
                let _ = map.set_tile(LAYER_RIDGE, x, y as u32, Tile::new(TileId(5)));
            }
        }
    }

    for y in 30..MAP_HEIGHT - 8 {
        for x in 20..MAP_WIDTH - 20 {
            let dx = x as f32 - MAP_WIDTH as f32 * 0.62;
            let dy = y as f32 - MAP_HEIGHT as f32 * 0.58;
            let distance = (dx * dx * 0.5 + dy * dy).sqrt();
            if distance < 16.0 && (x + y) % 3 != 0 {
                let flags = if (x + y) % 2 == 0 {
                    TileFlags::FLIP_X
                } else {
                    TileFlags::empty()
                };
                let _ = map.set_tile(LAYER_CANOPY, x, y, Tile::new(TileId(6)).with_flags(flags));
            }
        }
    }

    for y in 52..MAP_HEIGHT - 14 {
        for x in 28..MAP_WIDTH - 28 {
            let dx = x as f32 - MAP_WIDTH as f32 * 0.32;
            let dy = y as f32 - MAP_HEIGHT as f32 * 0.7;
            let distance = (dx * dx * 0.85 + dy * dy).sqrt();
            if distance < 12.0 && (x * 3 + y) % 4 != 0 {
                let flags = if (x + y) % 3 == 0 {
                    TileFlags::FLIP_Y
                } else {
                    TileFlags::empty()
                };
                let _ = map.set_tile(
                    LAYER_CANOPY,
                    x,
                    y,
                    Tile::tinted(TileId(10), Color::new(0.82, 1.0, 0.86, 0.92)).with_flags(flags),
                );
            }
        }
    }
}

fn animate_water(world: &mut World, map: Option<TilemapHandle>, time: f32) {
    let Some(map_handle) = map else {
        return;
    };
    let Some(storage) = world.get_resource_mut::<TilemapStorage>() else {
        return;
    };
    let Some(map) = storage.get_mut(map_handle) else {
        return;
    };

    let wave = (time * 2.4).sin() * 0.08 + 0.12;
    let tint = Color::new(0.75 + wave, 0.9 + wave * 0.4, 1.0, 1.0);
    for y in 0..18 {
        for x in 0..MAP_WIDTH {
            if (x + y) % 11 == 0 {
                let _ = map.set_tile(LAYER_GROUND, x, y, Tile::tinted(TileId(1), tint));
            }
        }
    }
}

fn make_tileset_texture() -> TextureAsset {
    let width = TILESET_COLUMNS * TILESET_TILE_WIDTH;
    let height = TILESET_ROWS * TILESET_TILE_HEIGHT;
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for tile_y in 0..TILESET_ROWS {
        for tile_x in 0..TILESET_COLUMNS {
            let tile_id = tile_y * TILESET_COLUMNS + tile_x;
            let base = tile_color(tile_id);
            for py in 0..TILESET_TILE_HEIGHT {
                for px in 0..TILESET_TILE_WIDTH {
                    let x = tile_x * TILESET_TILE_WIDTH + px;
                    let y = tile_y * TILESET_TILE_HEIGHT + py;
                    let local_x = (px as f32 + 0.5) / TILESET_TILE_WIDTH as f32 * 2.0 - 1.0;
                    let local_y = (py as f32 + 0.5) / TILESET_TILE_HEIGHT as f32 * 2.0 - 1.0;
                    let diamond = local_x.abs() + local_y.abs();
                    let shade = if diamond > 1.0 {
                        1.0
                    } else if diamond > 0.9 {
                        0.78
                    } else if (px + py + tile_id) % 7 == 0 {
                        1.12
                    } else {
                        1.0
                    };
                    let i = ((y * width + x) * 4) as usize;
                    pixels[i] = scale_channel(base[0], shade);
                    pixels[i + 1] = scale_channel(base[1], shade);
                    pixels[i + 2] = scale_channel(base[2], shade);
                    pixels[i + 3] = if diamond <= 1.0 { base[3] } else { 0 };
                }
            }
        }
    }
    TextureAsset::new(width, height, TextureColorSpace::Srgb, pixels)
}

fn tile_color(tile_id: u32) -> [u8; 4] {
    match tile_id {
        0 => [86, 151, 84, 255],
        1 => [64, 132, 196, 255],
        2 => [222, 196, 118, 255],
        4 => [104, 171, 93, 255],
        5 => [127, 121, 107, 255],
        6 => [62, 117, 76, 235],
        7 => [156, 118, 72, 245],
        8 => [132, 187, 82, 230],
        9 => [178, 181, 155, 238],
        10 => [42, 139, 91, 220],
        _ => [190, 102, 73, 255],
    }
}

fn scale_channel(value: u8, scale: f32) -> u8 {
    (value as f32 * scale).round().clamp(0.0, 255.0) as u8
}
