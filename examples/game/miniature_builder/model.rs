use sky_engine::render::{Color, TileId};

use crate::geometry::{COLS, ROWS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Meadow,
    Vault,
    Archive,
    Commons,
}

impl Zone {
    pub fn name(self) -> &'static str {
        match self {
            Self::Meadow => "meadow",
            Self::Vault => "vault",
            Self::Archive => "archive",
            Self::Commons => "commons",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Farm,
    Dungeon,
    Library,
}

impl Family {
    pub fn name(self) -> &'static str {
        match self {
            Self::Farm => "farm",
            Self::Dungeon => "dungeon",
            Self::Library => "library",
        }
    }

    pub fn preferred_zone(self) -> Zone {
        match self {
            Self::Farm => Zone::Meadow,
            Self::Dungeon => Zone::Vault,
            Self::Library => Zone::Archive,
        }
    }
}

#[derive(Clone, Copy)]
pub struct BlueprintDef {
    pub name: &'static str,
    pub stem: &'static str,
    pub family: Family,
    pub cost: i32,
    pub appeal: i32,
}

pub const BLUEPRINTS: [BlueprintDef; 9] = [
    BlueprintDef {
        name: "Corn rows",
        stem: "corn",
        family: Family::Farm,
        cost: 9,
        appeal: 7,
    },
    BlueprintDef {
        name: "Hay stack",
        stem: "hayBales",
        family: Family::Farm,
        cost: 7,
        appeal: 5,
    },
    BlueprintDef {
        name: "Low fence",
        stem: "fenceHigh",
        family: Family::Farm,
        cost: 4,
        appeal: 3,
    },
    BlueprintDef {
        name: "Treasure chest",
        stem: "chestClosed",
        family: Family::Dungeon,
        cost: 8,
        appeal: 6,
    },
    BlueprintDef {
        name: "Barrel stack",
        stem: "barrelsStacked",
        family: Family::Dungeon,
        cost: 7,
        appeal: 5,
    },
    BlueprintDef {
        name: "Aged stairs",
        stem: "stairsAged",
        family: Family::Dungeon,
        cost: 12,
        appeal: 9,
    },
    BlueprintDef {
        name: "Bookcase",
        stem: "bookcaseBooks",
        family: Family::Library,
        cost: 10,
        appeal: 8,
    },
    BlueprintDef {
        name: "Reading table",
        stem: "longTableDecoratedChairsBooks",
        family: Family::Library,
        cost: 11,
        appeal: 9,
    },
    BlueprintDef {
        name: "Candles",
        stem: "candleStandDouble",
        family: Family::Library,
        cost: 5,
        appeal: 4,
    },
];

pub const ORIENTATIONS: [&str; 4] = ["E", "S", "W", "N"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacedStructure {
    pub blueprint: usize,
    pub orientation: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub zone: Zone,
    pub structure: Option<PlacedStructure>,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counts {
    pub farm: usize,
    pub dungeon: usize,
    pub library: usize,
}

pub const FLOOR_PLANK_W: TileId = TileId(0);
pub const FLOOR_PLANK_E: TileId = TileId(1);
pub const FLOOR_PLANK_N: TileId = TileId(2);
pub const FLOOR_PLANK_S: TileId = TileId(3);

pub fn zone_for(row: usize, col: usize) -> Zone {
    if col <= 3 && row <= 7 {
        Zone::Meadow
    } else if row >= 6 && col >= 3 {
        Zone::Vault
    } else if row <= 4 && col >= 6 {
        Zone::Archive
    } else {
        Zone::Commons
    }
}

pub fn zone_tint(zone: Zone) -> Color {
    match zone {
        Zone::Meadow => Color::rgba8(232, 255, 222, 255),
        Zone::Vault => Color::rgba8(230, 235, 240, 255),
        Zone::Archive => Color::rgba8(255, 238, 218, 255),
        Zone::Commons => Color::rgba8(232, 248, 225, 255),
    }
}

pub fn ground_tile_for(row: usize, col: usize, zone: Zone) -> TileId {
    match zone {
        Zone::Meadow => {
            if (row + col) % 2 == 0 {
                FLOOR_PLANK_W
            } else {
                FLOOR_PLANK_E
            }
        }
        Zone::Vault => FLOOR_PLANK_N,
        Zone::Archive => FLOOR_PLANK_S,
        Zone::Commons => {
            if row % 2 == 0 {
                FLOOR_PLANK_E
            } else {
                FLOOR_PLANK_W
            }
        }
    }
}

pub fn make_cells() -> Vec<Cell> {
    let mut cells = Vec::with_capacity(ROWS * COLS);
    for row in 0..ROWS {
        for col in 0..COLS {
            cells.push(Cell {
                zone: zone_for(row, col),
                structure: None,
            });
        }
    }
    cells
}
