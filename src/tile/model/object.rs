use std::collections::BTreeMap;

use super::grid::{CellCoord, TileDirection};
use super::layer::{LayerId, TileRef};
use super::palette::PropertyBag;

/// Runtime tile object identifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileObjectId(pub u64);

/// Optional object prototype identifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectPrototypeId(pub u64);

/// Object footprint in cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Footprint {
    pub size: [u32; 2],
}

impl Footprint {
    pub const fn one_cell() -> Self {
        Self { size: [1, 1] }
    }
}

/// One visual tile in a multi-tile object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectVisualTile {
    pub offset: [i32; 2],
    pub tile_ref: TileRef,
}

/// Backend-neutral sprite visual reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpriteVisualRef {
    pub key: String,
}

/// Visual payload for an object-like tile scene item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectVisual {
    Tile(TileRef),
    MultiTile(Vec<ObjectVisualTile>),
    Sprite(SpriteVisualRef),
    None,
}

/// Addressable object in a tile scene.
#[derive(Clone, Debug, PartialEq)]
pub struct TileObject {
    pub id: TileObjectId,
    pub prototype: Option<ObjectPrototypeId>,
    pub layer: LayerId,
    pub cell: CellCoord,
    pub orientation: TileDirection,
    pub footprint: Footprint,
    pub visual: ObjectVisual,
    pub properties: PropertyBag,
}

impl TileObject {
    pub fn new(id: TileObjectId, layer: LayerId, cell: CellCoord) -> Self {
        Self {
            id,
            prototype: None,
            layer,
            cell,
            orientation: TileDirection::North,
            footprint: Footprint::one_cell(),
            visual: ObjectVisual::None,
            properties: PropertyBag::new(),
        }
    }
}

/// Object storage for one scene.
#[derive(Clone, Debug, Default)]
pub struct TileObjectStore {
    objects: BTreeMap<TileObjectId, TileObject>,
    next_id: u64,
}

impl TileObjectStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reserve_id(&mut self) -> TileObjectId {
        let id = TileObjectId(self.next_id.max(1));
        self.next_id = id.0.saturating_add(1);
        id
    }

    pub fn insert(&mut self, mut object: TileObject) -> TileObjectId {
        if object.id.0 == 0 {
            object.id = self.reserve_id();
        } else {
            self.next_id = self.next_id.max(object.id.0.saturating_add(1));
        }
        let id = object.id;
        self.objects.insert(id, object);
        id
    }

    pub fn remove(&mut self, id: TileObjectId) -> Option<TileObject> {
        self.objects.remove(&id)
    }

    pub fn get(&self, id: TileObjectId) -> Option<&TileObject> {
        self.objects.get(&id)
    }

    pub fn get_mut(&mut self, id: TileObjectId) -> Option<&mut TileObject> {
        self.objects.get_mut(&id)
    }
}
