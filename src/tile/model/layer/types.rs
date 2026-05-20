use super::super::grid::CellCoord;
use super::super::object::TileObjectId;
use super::super::palette::PropertyBag;
use super::data::{
    CollisionLayerData, LayerData, MetadataLayerData, ObjectLayerData, TileLayerData,
};
use super::tile::TileCell;

/// Runtime layer identifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId(pub u32);

impl From<u32> for LayerId {
    #[inline]
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// High-level layer kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LayerKind {
    #[default]
    Tiles,
    Objects,
    Collision,
    Metadata,
}

/// Gameplay/editor layer role.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LayerRole {
    Ground,
    Detail,
    Props,
    Walls,
    Upper,
    Collision,
    Gameplay,
    Preview,
    Custom(String),
}

/// One structured tile scene layer.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct MapLayer {
    pub id: LayerId,
    pub name: String,
    pub role: LayerRole,
    pub kind: LayerKind,
    pub visible: bool,
    pub editable: bool,
    pub opacity: f32,
    pub offset: [f32; 2],
    pub parallax: [f32; 2],
    pub data: LayerData,
    pub properties: PropertyBag,
}

impl MapLayer {
    pub fn tiles(id: LayerId, name: impl Into<String>, role: LayerRole) -> Self {
        Self {
            id,
            name: name.into(),
            role,
            kind: LayerKind::Tiles,
            visible: true,
            editable: true,
            opacity: 1.0,
            offset: [0.0, 0.0],
            parallax: [1.0, 1.0],
            data: LayerData::Tiles(TileLayerData::default()),
            properties: PropertyBag::new(),
        }
    }

    pub fn objects(id: LayerId, name: impl Into<String>, role: LayerRole) -> Self {
        Self {
            kind: LayerKind::Objects,
            data: LayerData::Objects(ObjectLayerData::default()),
            ..Self::tiles(id, name, role)
        }
    }

    pub fn collision(id: LayerId, name: impl Into<String>, role: LayerRole) -> Self {
        Self {
            kind: LayerKind::Collision,
            data: LayerData::Collision(CollisionLayerData::default()),
            ..Self::tiles(id, name, role)
        }
    }

    pub fn metadata(id: LayerId, name: impl Into<String>, role: LayerRole) -> Self {
        Self {
            kind: LayerKind::Metadata,
            data: LayerData::Metadata(MetadataLayerData::default()),
            ..Self::tiles(id, name, role)
        }
    }

    pub fn set_tile(&mut self, cell: CellCoord, tile: Option<TileCell>) -> Option<TileCell> {
        self.data.as_tiles_mut()?.tiles.set(cell, tile)
    }

    pub fn tile(&self, cell: CellCoord) -> Option<TileCell> {
        self.data.as_tiles()?.tiles.get(cell)
    }

    pub fn add_object_id(&mut self, id: TileObjectId) {
        if let Some(data) = self.data.as_objects_mut() {
            if !data.objects.contains(&id) {
                data.objects.push(id);
            }
        }
    }

    pub fn remove_object_id(&mut self, id: TileObjectId) -> bool {
        self.data
            .as_objects_mut()
            .is_some_and(|data| data.remove_object(id))
    }
}
