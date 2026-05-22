use std::collections::VecDeque;

use sky_engine::asset::{Handle, TextureAsset};
use sky_engine::ecs::EntityId;
use sky_engine::math::Vec2;
use sky_engine::render::Color;

pub const GRID_WIDTH: usize = 30;
pub const GRID_HEIGHT: usize = 18;
pub const TILE_SIZE: f32 = 28.0;
pub const CAMERA_SPEED: f32 = 520.0;
pub const ZOOM_MIN: f32 = 0.70;
pub const ZOOM_MAX: f32 = 2.30;
pub const PAWN_SPEED: f32 = 92.0;
pub const HARVEST_TIME: f32 = 1.75;
pub const BUILD_TIME: f32 = 2.60;
pub const SOW_TIME: f32 = 1.10;
pub const EAT_TIME: f32 = 1.20;
pub const SLEEP_RECOVERY_RATE: f32 = 0.34;
pub const HUNGER_DECAY_RATE: f32 = 0.018;
pub const REST_DECAY_RATE: f32 = 0.012;
pub const TITLE_UPDATE_FRAMES: u32 = 12;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToolMode {
    Inspect,
    Stockpile,
    Harvest,
    BuildWall,
    BuildBed,
    BuildFarm,
    Cancel,
}

impl ToolMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Inspect => "Inspect",
            Self::Stockpile => "Stockpile",
            Self::Harvest => "Harvest",
            Self::BuildWall => "Build Wall",
            Self::BuildBed => "Build Bed",
            Self::BuildFarm => "Farm Plot",
            Self::Cancel => "Cancel",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TerrainKind {
    Grass,
    RichSoil,
    Gravel,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Wood,
    Steel,
    Food,
}

impl ResourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Wood => "wood",
            Self::Steel => "steel",
            Self::Food => "food",
        }
    }
}

#[derive(Clone, Copy)]
pub struct ItemStack {
    pub kind: ResourceKind,
    pub amount: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Tree,
    Ore,
    BerryBush,
}

impl NodeKind {
    pub fn harvest_yield(self) -> ItemStack {
        match self {
            Self::Tree => ItemStack {
                kind: ResourceKind::Wood,
                amount: 12,
            },
            Self::Ore => ItemStack {
                kind: ResourceKind::Steel,
                amount: 10,
            },
            Self::BerryBush => ItemStack {
                kind: ResourceKind::Food,
                amount: 8,
            },
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Tree => "Tree",
            Self::Ore => "Steel ore",
            Self::BerryBush => "Berry bush",
        }
    }
}

#[derive(Clone, Copy)]
pub struct ResourceNode {
    pub kind: NodeKind,
    pub designated: bool,
    pub reserved: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StructureKind {
    Wall,
    Bed,
}

impl StructureKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Wall => "Wall",
            Self::Bed => "Bed",
        }
    }

    pub fn cost(self) -> ItemStack {
        match self {
            Self::Wall => ItemStack {
                kind: ResourceKind::Wood,
                amount: 6,
            },
            Self::Bed => ItemStack {
                kind: ResourceKind::Wood,
                amount: 10,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct Blueprint {
    pub kind: StructureKind,
    pub reserved: bool,
}

#[derive(Clone, Copy)]
pub struct FarmPlot {
    pub designated: bool,
    pub reserved: bool,
    pub planted: bool,
    pub growth: f32,
    pub ready: bool,
}

#[derive(Clone, Copy)]
pub struct MapCell {
    pub terrain: TerrainKind,
    pub stockpile: bool,
    pub grass_density: f32,
    pub node: Option<ResourceNode>,
    pub item: Option<ItemStack>,
    pub item_reserved: bool,
    pub blueprint: Option<Blueprint>,
    pub structure: Option<StructureKind>,
    pub farm: Option<FarmPlot>,
}

impl MapCell {
    pub fn passable(self) -> bool {
        !matches!(self.structure, Some(StructureKind::Wall))
    }
}

pub struct MapState {
    pub cells: Vec<MapCell>,
}

impl MapState {
    pub fn new() -> Self {
        Self {
            cells: create_map(),
        }
    }

    pub fn cell(&self, x: i32, y: i32) -> Option<&MapCell> {
        if !in_bounds(x, y) {
            return None;
        }
        self.cells.get(cell_index(x, y))
    }

    pub fn cell_mut(&mut self, x: i32, y: i32) -> Option<&mut MapCell> {
        if !in_bounds(x, y) {
            return None;
        }
        self.cells.get_mut(cell_index(x, y))
    }
}

#[derive(Clone, Copy)]
pub struct CellVisual {
    pub ground: EntityId,
    pub zone: EntityId,
    pub content: EntityId,
    pub overlay: EntityId,
}

#[derive(Clone)]
pub struct PawnVisual {
    pub body: EntityId,
    pub shadow: EntityId,
}

pub struct PawnState {
    pub name: &'static str,
    pub pos: Vec2,
    pub visual: PawnVisual,
    pub tint: Color,
    pub carrying: Option<ItemStack>,
    pub hunger: f32,
    pub rest: f32,
    pub job: PawnJob,
}

pub enum PawnJob {
    Idle,
    Moving {
        purpose: MovePurpose,
        path: Vec<[i32; 2]>,
        step_index: usize,
    },
    Harvesting {
        cell: [i32; 2],
        progress: f32,
    },
    Building {
        cell: [i32; 2],
        progress: f32,
    },
    Sowing {
        cell: [i32; 2],
        progress: f32,
    },
    Eating {
        progress: f32,
    },
    Sleeping {
        cell: [i32; 2],
    },
}

pub enum MovePurpose {
    Harvest([i32; 2]),
    Build([i32; 2]),
    Sow([i32; 2]),
    EatAt([i32; 2]),
    SleepAt([i32; 2]),
    HaulPickup([i32; 2], [i32; 2]),
    HaulDeliver([i32; 2]),
    PlayerMove([i32; 2]),
}

pub struct ResourceLedger {
    pub wood: u32,
    pub steel: u32,
    pub food: u32,
}

impl ResourceLedger {
    pub fn add(&mut self, kind: ResourceKind, amount: u32) {
        match kind {
            ResourceKind::Wood => self.wood += amount,
            ResourceKind::Steel => self.steel += amount,
            ResourceKind::Food => self.food += amount,
        }
    }

    pub fn amount(&self, kind: ResourceKind) -> u32 {
        match kind {
            ResourceKind::Wood => self.wood,
            ResourceKind::Steel => self.steel,
            ResourceKind::Food => self.food,
        }
    }

    pub fn consume(&mut self, kind: ResourceKind, amount: u32) -> bool {
        let slot = match kind {
            ResourceKind::Wood => &mut self.wood,
            ResourceKind::Steel => &mut self.steel,
            ResourceKind::Food => &mut self.food,
        };
        if *slot < amount {
            return false;
        }
        *slot -= amount;
        true
    }
}

pub struct SurfaceInfo {
    pub size: [f32; 2],
}

pub struct WorldVisuals {
    pub cells: Vec<CellVisual>,
    pub hover_entity: EntityId,
    pub selection_entity: EntityId,
    pub camera_entity: EntityId,
}

pub struct RimworldTextures {
    pub tree: Handle<TextureAsset>,
}

pub struct GameState {
    pub pawns: Vec<PawnState>,
    pub camera_center: Vec2,
    pub zoom: f32,
    pub hovered_cell: Option<[i32; 2]>,
    pub selected_cell: Option<[i32; 2]>,
    pub selected_pawn: Option<usize>,
    pub selected_pawns: Vec<usize>,
    pub drag_select_start: Option<[f32; 2]>,
    pub tool: ToolMode,
    pub paused: bool,
    pub resources: ResourceLedger,
    pub frame_count: u32,
    pub title: String,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            pawns: Vec::new(),
            camera_center: Vec2::new(
                GRID_WIDTH as f32 * TILE_SIZE * 0.5 - TILE_SIZE * 0.5,
                GRID_HEIGHT as f32 * TILE_SIZE * 0.5 - TILE_SIZE * 0.5,
            ),
            zoom: 1.0,
            hovered_cell: None,
            selected_cell: None,
            selected_pawn: None,
            selected_pawns: Vec::new(),
            drag_select_start: None,
            tool: ToolMode::Inspect,
            paused: false,
            resources: ResourceLedger {
                wood: 22,
                steel: 18,
                food: 14,
            },
            frame_count: 0,
            title: "SkyEngine — Rimworld Prototype".to_string(),
        }
    }
}

pub fn in_bounds(x: i32, y: i32) -> bool {
    x >= 0 && x < GRID_WIDTH as i32 && y >= 0 && y < GRID_HEIGHT as i32
}

pub fn cell_index(x: i32, y: i32) -> usize {
    y as usize * GRID_WIDTH + x as usize
}

pub fn index_to_cell(index: usize) -> [i32; 2] {
    [(index % GRID_WIDTH) as i32, (index / GRID_WIDTH) as i32]
}

pub fn cell_world(x: i32, y: i32) -> [f32; 2] {
    [
        x as f32 * TILE_SIZE + TILE_SIZE * 0.5,
        y as f32 * TILE_SIZE + TILE_SIZE * 0.5,
    ]
}

pub fn world_to_cell(world: Vec2) -> [i32; 2] {
    [
        (world.x() / TILE_SIZE).floor() as i32,
        (world.y() / TILE_SIZE).floor() as i32,
    ]
}

pub fn manhattan(a: [i32; 2], b: [i32; 2]) -> i32 {
    (a[0] - b[0]).abs() + (a[1] - b[1]).abs()
}

pub fn structure_cost(kind: StructureKind) -> ItemStack {
    kind.cost()
}

pub fn find_path(
    map: &MapState,
    start: [i32; 2],
    goal: [i32; 2],
    allow_goal_blocked: bool,
) -> Vec<[i32; 2]> {
    if start == goal {
        return vec![goal];
    }
    if !in_bounds(goal[0], goal[1]) {
        return Vec::new();
    }

    let mut queue = VecDeque::new();
    let mut visited = [false; GRID_WIDTH * GRID_HEIGHT];
    let mut previous = [usize::MAX; GRID_WIDTH * GRID_HEIGHT];

    let start_index = cell_index(start[0], start[1]);
    let goal_index = cell_index(goal[0], goal[1]);
    queue.push_back(start);
    visited[start_index] = true;

    while let Some([x, y]) = queue.pop_front() {
        for neighbor in [[x + 1, y], [x - 1, y], [x, y + 1], [x, y - 1]] {
            if !in_bounds(neighbor[0], neighbor[1]) {
                continue;
            }
            let index = cell_index(neighbor[0], neighbor[1]);
            if visited[index] {
                continue;
            }
            let passable = map
                .cell(neighbor[0], neighbor[1])
                .is_some_and(|cell| cell.passable() || (allow_goal_blocked && neighbor == goal));
            if !passable {
                continue;
            }
            visited[index] = true;
            previous[index] = cell_index(x, y);
            if index == goal_index {
                let mut path = vec![goal];
                let mut cursor = previous[index];
                while cursor != start_index {
                    path.push(index_to_cell(cursor));
                    cursor = previous[cursor];
                    if cursor == usize::MAX {
                        return Vec::new();
                    }
                }
                path.push(start);
                path.reverse();
                path.remove(0);
                return path;
            }
            queue.push_back(neighbor);
        }
    }

    Vec::new()
}

pub fn create_map() -> Vec<MapCell> {
    let mut cells = Vec::with_capacity(GRID_WIDTH * GRID_HEIGHT);
    for y in 0..GRID_HEIGHT as i32 {
        for x in 0..GRID_WIDTH as i32 {
            let terrain = if x >= 18 && y <= 5 {
                TerrainKind::Gravel
            } else if x >= 7 && x <= 15 && y >= 10 {
                TerrainKind::RichSoil
            } else {
                TerrainKind::Grass
            };
            let stockpile = x >= 3 && x <= 6 && y >= 4 && y <= 7;
            let grass_density = match terrain {
                TerrainKind::Grass => 0.58 + ((x * 17 + y * 11).rem_euclid(23) as f32) / 40.0,
                TerrainKind::RichSoil => 0.36,
                TerrainKind::Gravel => 0.08,
            };
            let node = match (x, y) {
                (22, 4)
                | (24, 5)
                | (25, 3)
                | (23, 7)
                | (19, 6)
                | (21, 8)
                | (26, 7)
                | (20, 3)
                | (11, 8)
                | (8, 9)
                | (15, 8) => Some(ResourceNode {
                    kind: NodeKind::Tree,
                    designated: matches!((x, y), (22, 4) | (24, 5) | (19, 6) | (11, 8)),
                    reserved: false,
                }),
                (7, 13) | (9, 14) | (12, 13) | (10, 15) | (14, 14) | (8, 11) => {
                    Some(ResourceNode {
                        kind: NodeKind::BerryBush,
                        designated: matches!((x, y), (9, 14)),
                        reserved: false,
                    })
                }
                (4, 13) | (5, 15) | (15, 3) | (16, 5) | (17, 4) => Some(ResourceNode {
                    kind: NodeKind::Ore,
                    designated: matches!((x, y), (15, 3)),
                    reserved: false,
                }),
                _ => None,
            };

            let item = match (x, y) {
                (14, 9) => Some(ItemStack {
                    kind: ResourceKind::Wood,
                    amount: 8,
                }),
                (16, 10) => Some(ItemStack {
                    kind: ResourceKind::Steel,
                    amount: 5,
                }),
                (11, 12) => Some(ItemStack {
                    kind: ResourceKind::Food,
                    amount: 4,
                }),
                _ => None,
            };

            let blueprint = match (x, y) {
                (12, 8)
                | (12, 9)
                | (12, 10)
                | (13, 10)
                | (14, 10)
                | (15, 10)
                | (16, 10)
                | (16, 9)
                | (16, 8)
                | (15, 8)
                | (14, 8) => Some(Blueprint {
                    kind: StructureKind::Wall,
                    reserved: false,
                }),
                (13, 8) | (14, 9) => Some(Blueprint {
                    kind: StructureKind::Bed,
                    reserved: false,
                }),
                _ => None,
            };

            let farm = match (x, y) {
                (7, 12)
                | (8, 12)
                | (9, 12)
                | (10, 12)
                | (11, 12)
                | (7, 13)
                | (8, 13)
                | (9, 13)
                | (10, 13)
                | (11, 13)
                | (7, 14)
                | (8, 14)
                | (9, 14)
                | (10, 14)
                | (11, 14) => Some(FarmPlot {
                    designated: true,
                    reserved: false,
                    planted: false,
                    growth: 0.0,
                    ready: false,
                }),
                _ => None,
            };

            cells.push(MapCell {
                terrain,
                stockpile,
                grass_density,
                node,
                item,
                item_reserved: false,
                blueprint,
                structure: None,
                farm,
            });
        }
    }
    cells
}
