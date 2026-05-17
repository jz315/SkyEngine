use super::grid::GridSpec;
use super::layer::{LayerId, LayerRole, TileLayer};
use super::object::TileObjectStore;
use super::palette::{PaletteId, PropertyBag, TilePaletteStore};

/// Runtime tile scene identifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileMapId(pub u64);

impl From<u64> for TileMapId {
    #[inline]
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// Finite scene dimensions in logical cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileMapSize {
    pub width: u32,
    pub height: u32,
}

impl TileMapSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

impl From<[u32; 2]> for TileMapSize {
    #[inline]
    fn from(value: [u32; 2]) -> Self {
        Self::new(value[0], value[1])
    }
}

impl From<(u32, u32)> for TileMapSize {
    #[inline]
    fn from(value: (u32, u32)) -> Self {
        Self::new(value.0, value.1)
    }
}

/// Runtime truth for one tile map/scene.
#[derive(Clone, Debug)]
pub struct TileMap {
    pub id: TileMapId,
    pub name: String,
    pub grid: GridSpec,
    pub size: TileMapSize,
    pub palettes: Vec<PaletteId>,
    pub layers: Vec<TileLayer>,
    pub objects: TileObjectStore,
    pub properties: PropertyBag,
}

impl TileMap {
    pub fn new(id: TileMapId, name: impl Into<String>, grid: GridSpec, size: TileMapSize) -> Self {
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

    pub fn layer(&self, id: LayerId) -> Option<&TileLayer> {
        self.layers.iter().find(|layer| layer.id == id)
    }

    pub fn layer_mut(&mut self, id: LayerId) -> Option<&mut TileLayer> {
        self.layers.iter_mut().find(|layer| layer.id == id)
    }

    pub fn add_layer(&mut self, layer: TileLayer) -> LayerId {
        let id = layer.id;
        if let Some(existing) = self.layer_mut(id) {
            *existing = layer;
        } else {
            self.layers.push(layer);
        }
        id
    }

    pub fn add_tile_layer(&mut self, name: impl Into<String>, role: LayerRole) -> LayerId {
        let id = self.next_layer_id();
        self.add_layer(TileLayer::tiles(id, name, role))
    }

    pub fn add_object_layer(&mut self, name: impl Into<String>, role: LayerRole) -> LayerId {
        let id = self.next_layer_id();
        self.add_layer(TileLayer::objects(id, name, role))
    }

    pub fn layer_by_name(&self, name: &str) -> Option<&TileLayer> {
        self.layers.iter().find(|layer| layer.name == name)
    }

    pub fn layer_id_by_name(&self, name: &str) -> Option<LayerId> {
        self.layer_by_name(name).map(|layer| layer.id)
    }

    pub fn first_layer_with_role(&self, role: &LayerRole) -> Option<&TileLayer> {
        self.layers.iter().find(|layer| &layer.role == role)
    }

    pub fn layers_with_role<'a>(
        &'a self,
        role: &'a LayerRole,
    ) -> impl Iterator<Item = &'a TileLayer> {
        self.layers.iter().filter(move |layer| &layer.role == role)
    }

    fn next_layer_id(&self) -> LayerId {
        LayerId(
            self.layers
                .iter()
                .map(|layer| layer.id.0)
                .max()
                .unwrap_or_default()
                .saturating_add(1),
        )
    }
}

/// Optional multi-scene tile project container.
#[derive(Clone, Debug, Default)]
pub struct TileWorld {
    pub palettes: TilePaletteStore,
    pub scenes: Vec<TileMap>,
}

impl TileWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn scene(&self, id: TileMapId) -> Option<&TileMap> {
        self.scenes.iter().find(|scene| scene.id == id)
    }
}
