use super::grid::GridSpec;
use super::layer::{LayerId, MapLayer};
use super::object::TileObjectStore;
use super::palette::{PaletteId, PropertyBag};

/// Stable runtime tile map identifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MapId(pub u64);

impl From<u64> for MapId {
    #[inline]
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// Finite map dimensions in logical cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapSize {
    pub width: u32,
    pub height: u32,
}

impl MapSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

impl From<[u32; 2]> for MapSize {
    #[inline]
    fn from(value: [u32; 2]) -> Self {
        Self::new(value[0], value[1])
    }
}

impl From<(u32, u32)> for MapSize {
    #[inline]
    fn from(value: (u32, u32)) -> Self {
        Self::new(value.0, value.1)
    }
}

/// Runtime truth for one tile map.
#[derive(Clone, Debug)]
pub struct MapData {
    pub id: MapId,
    pub name: String,
    pub grid: GridSpec,
    pub size: MapSize,
    pub palettes: Vec<PaletteId>,
    pub layers: Vec<MapLayer>,
    pub objects: TileObjectStore,
    pub properties: PropertyBag,
}

impl MapData {
    pub fn new(id: MapId, name: impl Into<String>, grid: GridSpec, size: MapSize) -> Self {
        Self {
            id,
            name: name.into(),
            grid,
            size,
            palettes: Vec::new(),
            layers: Vec::new(),
            objects: TileObjectStore::new(),
            properties: PropertyBag::new(),
        }
    }

    pub fn layer(&self, id: LayerId) -> Option<&MapLayer> {
        self.layers.iter().find(|layer| layer.id == id)
    }

    pub fn layer_mut(&mut self, id: LayerId) -> Option<&mut MapLayer> {
        self.layers.iter_mut().find(|layer| layer.id == id)
    }

    pub fn layer_by_name(&self, name: &str) -> Option<&MapLayer> {
        self.layers.iter().find(|layer| layer.name == name)
    }
}
