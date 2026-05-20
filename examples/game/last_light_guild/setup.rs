use sky_engine::asset::{Assets, TextureAsset, TextureColorSpace};
use sky_engine::ecs::{EntityId, World};
use sky_engine::render::{
    CameraMarker, MainCamera, Projection, RenderSettings, SortingLayer, SpriteRenderer, Tile,
    TileId, TilemapDescriptor, TilemapRenderer, TilemapStorage, TilesetGrid, Transform,
};

use crate::components::{
    Adventurer, Condition, Fixture, Follow, GridPos, Name, Personality, PixelVisual, Relationship,
    Role, RoomKind, Stats, StatusPip,
};
use crate::layout::{grid_to_world, tilemap_transform, GRID_H, GRID_W, TILE};
use crate::palette;
use crate::resources::{
    BoardCursor, Calendar, GameFlow, GuildStock, HudState, SimClock, TownState,
};

pub fn build_world() -> World {
    let mut world = World::new();

    world.insert_resource(Calendar { day: 0 });
    world.insert_resource(SimClock { paused: false });
    world.insert_resource(GuildStock {
        gold: 24,
        food: 22,
        medicine: 5,
        supplies: 9,
        reputation: 2,
    });
    world.insert_resource(TownState {
        danger: 4,
        unrest: 1,
    });
    world.insert_resource(BoardCursor { next: 0 });
    world.insert_resource(HudState::default());
    world.insert_resource(GameFlow::default());
    world.insert_resource(RenderSettings {
        clear_color: palette::CLEAR,
        ..Default::default()
    });

    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic(crate::layout::ORTHO_HEIGHT),
        MainCamera,
    ));

    spawn_fixtures(&mut world);

    let mira = spawn_adventurer(
        &mut world,
        "Mira",
        Role::Vanguard,
        Stats {
            might: 5,
            finesse: 2,
            wits: 2,
            spirit: 4,
        },
        Personality {
            courage: 3,
            caution: 0,
            empathy: 1,
        },
        GridPos { x: 5, y: 14 },
    );
    let elia = spawn_adventurer(
        &mut world,
        "Elia",
        Role::Medic,
        Stats {
            might: 1,
            finesse: 3,
            wits: 4,
            spirit: 5,
        },
        Personality {
            courage: 1,
            caution: 3,
            empathy: 4,
        },
        GridPos { x: 7, y: 14 },
    );
    let tovan = spawn_adventurer(
        &mut world,
        "Tovan",
        Role::Scout,
        Stats {
            might: 2,
            finesse: 5,
            wits: 4,
            spirit: 2,
        },
        Personality {
            courage: 2,
            caution: 2,
            empathy: 0,
        },
        GridPos { x: 9, y: 14 },
    );
    let nyx = spawn_adventurer(
        &mut world,
        "Nyx",
        Role::Occultist,
        Stats {
            might: 1,
            finesse: 2,
            wits: 5,
            spirit: 5,
        },
        Personality {
            courage: 2,
            caution: 1,
            empathy: 1,
        },
        GridPos { x: 11, y: 14 },
    );
    let bram = spawn_adventurer(
        &mut world,
        "Bram",
        Role::Broker,
        Stats {
            might: 3,
            finesse: 3,
            wits: 5,
            spirit: 2,
        },
        Personality {
            courage: 0,
            caution: 4,
            empathy: 2,
        },
        GridPos { x: 13, y: 14 },
    );

    spawn_relationships(&mut world, &[mira, elia, tovan, nyx, bram]);
    world
}

pub fn spawn_guild_tilemap(world: &mut World) {
    let asset_server = world
        .get_resource::<Assets>()
        .expect("App should install Assets before setup")
        .clone();
    let tileset = asset_server.insert_runtime(make_guild_tileset());
    let tileset_grid = TilesetGrid::new(tileset, [16, 16], 4, 4);

    let mut storage = TilemapStorage::new();
    let map = storage.create(TilemapDescriptor::new(GRID_W as u32, GRID_H as u32, 3));
    populate_guild_map(storage.get_mut(map).expect("new tilemap should exist"));
    world.insert_resource(storage);

    for (layer, sorting) in [(0, -20), (1, -10), (2, 0)] {
        world.spawn((
            tilemap_transform(-0.4 + layer as f32 * 0.02),
            TilemapRenderer::new(map, tileset_grid.clone())
                .layer(layer)
                .tile_size([TILE, TILE]),
            SortingLayer(sorting),
        ));
    }
}

fn populate_guild_map(map: &mut sky_engine::render::Tilemap) {
    let rooms = [
        (RoomKind::Bunks, 1, 1, 9, 7),
        (RoomKind::Infirmary, 11, 1, 10, 7),
        (RoomKind::Stores, 22, 1, 11, 7),
        (RoomKind::Training, 1, 9, 20, 10),
        (RoomKind::Common, 22, 9, 11, 10),
        (RoomKind::Board, 25, 11, 5, 6),
    ];

    for y in 0..GRID_H as u32 {
        for x in 0..GRID_W as u32 {
            let xi = x as i32;
            let yi = y as i32;
            let border = xi == 0 || yi == 0 || xi == GRID_W - 1 || yi == GRID_H - 1;
            let room = rooms
                .iter()
                .find(|(_, rx, ry, rw, rh)| {
                    xi >= *rx && xi < *rx + *rw && yi >= *ry && yi < *ry + *rh
                })
                .map(|(kind, _, _, _, _)| *kind);
            let Some(kind) = room else {
                let tile = if border { TileId(8) } else { TileId(0) };
                let _ = map.set_tile(0, x, y, Tile::new(tile));
                continue;
            };

            let room_border = rooms.iter().any(|(_, rx, ry, rw, rh)| {
                xi >= *rx
                    && xi < *rx + *rw
                    && yi >= *ry
                    && yi < *ry + *rh
                    && (xi == *rx || yi == *ry || xi == *rx + *rw - 1 || yi == *ry + *rh - 1)
            });
            let _ = map.set_tile(0, x, y, Tile::new(TileId(kind.tile_id())));
            if room_border {
                let _ = map.set_tile(1, x, y, Tile::new(TileId(9)));
            }
        }
    }

    let detail_tiles = [
        (3, 3, 10),
        (5, 3, 10),
        (7, 3, 10),
        (14, 3, 11),
        (17, 3, 11),
        (25, 3, 12),
        (29, 3, 12),
        (4, 14, 13),
        (7, 14, 13),
        (26, 14, 14),
        (29, 14, 15),
    ];
    for (x, y, id) in detail_tiles {
        let _ = map.set_tile(2, x, y, Tile::new(TileId(id)));
    }
}

fn spawn_fixtures(world: &mut World) {
    let markers = [
        (GridPos { x: 3, y: 4 }, 0.34, 0.20),
        (GridPos { x: 5, y: 4 }, 0.34, 0.20),
        (GridPos { x: 7, y: 4 }, 0.34, 0.20),
        (GridPos { x: 26, y: 12 }, 0.18, 0.18),
        (GridPos { x: 27, y: 13 }, 0.18, 0.18),
        (GridPos { x: 28, y: 14 }, 0.18, 0.18),
    ];

    for (pos, w, h) in markers {
        world.spawn((
            pos,
            Fixture,
            grid_to_world(pos, 0.35),
            SpriteRenderer::new(TILE * w, TILE * h).color(palette::FIXTURE),
            SortingLayer(3),
        ));
    }
}

fn make_guild_tileset() -> TextureAsset {
    const COLUMNS: u32 = 4;
    const ROWS: u32 = 4;
    const TILE_PX: u32 = 16;
    let width = COLUMNS * TILE_PX;
    let height = ROWS * TILE_PX;
    let mut pixels = vec![0u8; (width * height * 4) as usize];

    for tile_y in 0..ROWS {
        for tile_x in 0..COLUMNS {
            let id = tile_y * COLUMNS + tile_x;
            let base = tile_rgba(id);
            for py in 0..TILE_PX {
                for px in 0..TILE_PX {
                    let x = tile_x * TILE_PX + px;
                    let y = tile_y * TILE_PX + py;
                    let edge = px == 0 || py == 0 || px + 1 == TILE_PX || py + 1 == TILE_PX;
                    let grain = ((px * 3 + py * 5 + id * 11) % 17) as f32 / 17.0;
                    let shade = if edge { 0.58 } else { 0.86 + grain * 0.20 };
                    let alpha = if id == 0 { 230 } else { base[3] };
                    let i = ((y * width + x) * 4) as usize;
                    pixels[i] = scale_channel(base[0], shade);
                    pixels[i + 1] = scale_channel(base[1], shade);
                    pixels[i + 2] = scale_channel(base[2], shade);
                    pixels[i + 3] = alpha;
                }
            }
        }
    }

    TextureAsset::new(width, height, TextureColorSpace::Srgb, pixels)
}

fn tile_rgba(id: u32) -> [u8; 4] {
    match id {
        0 => [22, 22, 28, 255],
        1 => [46, 40, 86, 255],
        2 => [35, 72, 78, 255],
        3 => [78, 57, 29, 255],
        4 => [54, 54, 44, 255],
        5 => [82, 38, 36, 255],
        6 => [42, 28, 20, 255],
        8 => [18, 20, 25, 255],
        9 => [54, 50, 42, 255],
        10 => [91, 60, 37, 255],
        11 => [84, 105, 98, 255],
        12 => [110, 83, 41, 255],
        13 => [104, 93, 70, 255],
        14 => [120, 72, 48, 255],
        15 => [216, 132, 42, 255],
        _ => [165, 91, 74, 255],
    }
}

fn scale_channel(value: u8, scale: f32) -> u8 {
    (value as f32 * scale).round().clamp(0.0, 255.0) as u8
}

fn spawn_adventurer(
    world: &mut World,
    name: &'static str,
    role: Role,
    stats: Stats,
    personality: Personality,
    pos: GridPos,
) -> EntityId {
    let entity = world.spawn((
        Name(name),
        role,
        pos,
        Adventurer,
        stats,
        Condition {
            health: 10,
            stress: 1,
            fatigue: 0,
        },
        personality,
        PixelVisual {
            base: role.color(),
            hurt: palette::HURT,
            pulse: 0.0,
        },
    ));
    world.insert(entity, grid_to_world(pos, 0.5));
    world.insert(
        entity,
        SpriteRenderer::new(TILE * 0.78, TILE * 0.95).color(role.color()),
    );
    world.insert(entity, SortingLayer(6));

    world.spawn((
        StatusPip,
        Follow {
            target: entity,
            offset_x: 0.0,
            offset_y: -TILE * 0.68,
        },
        Transform::from_xyz(0.0, 0.0, 0.65),
        SpriteRenderer::new(TILE * 0.70, TILE * 0.13).color(palette::HEALTHY),
        SortingLayer(7),
    ));

    world.spawn((
        Follow {
            target: entity,
            offset_x: 0.0,
            offset_y: -TILE * 0.43,
        },
        Transform::from_xyz(0.0, 0.0, 0.45),
        SpriteRenderer::new(TILE * 0.85, TILE * 0.18).color(palette::SHADOW),
        SortingLayer(5),
    ));

    entity
}

fn spawn_relationships(world: &mut World, adventurers: &[EntityId]) {
    for i in 0..adventurers.len() {
        for j in (i + 1)..adventurers.len() {
            world.spawn((Relationship {
                a: adventurers[i],
                b: adventurers[j],
                trust: 0,
                tension: 0,
            },));
        }
    }
}
