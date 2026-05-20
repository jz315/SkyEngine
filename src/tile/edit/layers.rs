use crate::ecs::World;

use super::super::model::{
    CellCoord, CellRect, Footprint, LayerData, LayerId, LayerKind, MapData, ObjectVisual,
    PropertyBag, TileCell, TileObject, TileObjectId,
};
use super::super::runtime::{mutate_world_map, runtime_ref};
use super::{Cell, MapId, ObjectId, Rect, TileError, TileRef};

#[derive(Clone, Debug)]
pub struct Brush {
    tile: TileCell,
}

impl Brush {
    pub fn solid(tile: impl Into<TileCell>) -> Self {
        Self { tile: tile.into() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CollisionValue {
    Solid,
}

pub struct TileLayer<'a> {
    pub(crate) target: EditTarget<'a>,
    pub(crate) layer: LayerId,
}

impl TileLayer<'_> {
    pub fn get(&self, cell: impl Into<Cell>) -> Option<TileCell> {
        let cell = cell.into();
        self.target
            .map_data()
            .and_then(|scene| scene.layer(self.layer))
            .and_then(|layer| layer.tile(cell))
    }

    pub fn set(
        &mut self,
        cell: impl Into<Cell>,
        tile: impl Into<TileCell>,
    ) -> Result<(), TileError> {
        let cell = cell.into();
        let tile = tile.into();
        self.target.mutate(|scene| {
            check_bounds(scene, cell)?;
            let layer = scene
                .layer_mut(self.layer)
                .ok_or_else(|| TileError::NotFound(format!("layer {:?}", self.layer)))?;
            let _ = layer.set_tile(cell, Some(tile));
            Ok(())
        })
    }

    pub fn erase(&mut self, cell: impl Into<Cell>) -> Result<(), TileError> {
        let cell = cell.into();
        self.target.mutate(|scene| {
            check_bounds(scene, cell)?;
            let layer = scene
                .layer_mut(self.layer)
                .ok_or_else(|| TileError::NotFound(format!("layer {:?}", self.layer)))?;
            let _ = layer.set_tile(cell, None);
            Ok(())
        })
    }

    pub fn fill(
        &mut self,
        rect: impl Into<Rect>,
        tile: impl Into<TileCell>,
    ) -> Result<(), TileError> {
        let rect = rect.into();
        let tile = tile.into();
        self.target.mutate(|scene| {
            check_rect(scene, rect)?;
            let layer = scene
                .layer_mut(self.layer)
                .ok_or_else(|| TileError::NotFound(format!("layer {:?}", self.layer)))?;
            for cell in rect.cells() {
                let _ = layer.set_tile(cell, Some(tile));
            }
            Ok(())
        })
    }

    pub fn clear(&mut self, rect: impl Into<Rect>) -> Result<(), TileError> {
        let rect = rect.into();
        self.target.mutate(|scene| {
            check_rect(scene, rect)?;
            let layer = scene
                .layer_mut(self.layer)
                .ok_or_else(|| TileError::NotFound(format!("layer {:?}", self.layer)))?;
            for cell in rect.cells() {
                let _ = layer.set_tile(cell, None);
            }
            Ok(())
        })
    }

    pub fn paint(&mut self, rect: impl Into<Rect>, brush: &Brush) -> Result<(), TileError> {
        self.fill(rect, brush.tile)
    }
}

pub struct ObjectSpec {
    kind: String,
    cell: CellCoord,
    footprint: Footprint,
    visual: ObjectVisual,
}

pub fn object(kind: impl Into<String>) -> ObjectSpec {
    ObjectSpec {
        kind: kind.into(),
        cell: CellCoord::new(0, 0),
        footprint: Footprint::one_cell(),
        visual: ObjectVisual::None,
    }
}

impl ObjectSpec {
    pub fn at(mut self, cell: impl Into<Cell>) -> Self {
        self.cell = cell.into();
        self
    }

    pub fn footprint(mut self, size: [u32; 2]) -> Self {
        self.footprint = Footprint { size };
        self
    }

    pub fn visual_tile(mut self, tile: impl Into<TileRef>) -> Self {
        self.visual = ObjectVisual::Tile(tile.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectHandle {
    id: ObjectId,
}

impl ObjectHandle {
    pub fn id(self) -> ObjectId {
        self.id
    }
}

#[derive(Clone, Debug)]
pub struct ObjectView {
    pub id: ObjectId,
    pub cell: CellCoord,
    pub footprint: Footprint,
    pub visual: ObjectVisual,
}

pub struct ObjectLayer<'a> {
    pub(crate) target: EditTarget<'a>,
    pub(crate) layer: LayerId,
}

impl ObjectLayer<'_> {
    pub fn place(&mut self, object: ObjectSpec) -> Result<ObjectHandle, TileError> {
        self.target.mutate(|scene| {
            check_bounds(scene, object.cell)?;
            let mut tile_object = TileObject::new(TileObjectId(0), self.layer, object.cell);
            tile_object.footprint = object.footprint;
            tile_object.visual = object.visual;
            if !object.kind.is_empty() {
                tile_object.properties.insert("kind", object.kind);
            }
            let id = scene.objects.insert(tile_object);
            scene
                .layer_mut(self.layer)
                .ok_or_else(|| TileError::NotFound(format!("layer {:?}", self.layer)))?
                .add_object_id(id);
            Ok(ObjectHandle { id })
        })
    }

    pub fn get(&self, id: ObjectId) -> Result<ObjectView, TileError> {
        let object = self
            .target
            .map_data()
            .and_then(|scene| scene.objects.get(id))
            .filter(|object| object.layer == self.layer)
            .ok_or_else(|| TileError::NotFound(format!("object {:?}", id)))?;
        Ok(ObjectView {
            id,
            cell: object.cell,
            footprint: object.footprint.clone(),
            visual: object.visual.clone(),
        })
    }

    pub fn remove(&mut self, id: ObjectId) -> Result<(), TileError> {
        self.target.mutate(|scene| {
            let object = scene
                .objects
                .remove(id)
                .filter(|object| object.layer == self.layer)
                .ok_or_else(|| TileError::NotFound(format!("object {:?}", id)))?;
            if let Some(layer) = scene.layer_mut(object.layer) {
                let _ = layer.remove_object_id(id);
            }
            Ok(())
        })
    }

    pub fn move_to(&mut self, id: ObjectId, cell: impl Into<Cell>) -> Result<(), TileError> {
        let cell = cell.into();
        self.target.mutate(|scene| {
            check_bounds(scene, cell)?;
            let object = scene
                .objects
                .get_mut(id)
                .filter(|object| object.layer == self.layer)
                .ok_or_else(|| TileError::NotFound(format!("object {:?}", id)))?;
            object.cell = cell;
            Ok(())
        })
    }
}

pub struct CollisionLayer<'a> {
    pub(crate) target: EditTarget<'a>,
    pub(crate) layer: LayerId,
}

impl CollisionLayer<'_> {
    pub fn set(&mut self, cell: impl Into<Cell>, value: CollisionValue) -> Result<(), TileError> {
        match value {
            CollisionValue::Solid => self.fill((cell.into(), [1, 1]), value),
        }
    }

    pub fn erase(&mut self, cell: impl Into<Cell>) -> Result<(), TileError> {
        self.clear((cell.into(), [1, 1]))
    }

    pub fn fill(&mut self, rect: impl Into<Rect>, _value: CollisionValue) -> Result<(), TileError> {
        let rect = rect.into();
        self.target.mutate(|scene| {
            check_rect(scene, rect)?;
            let layer = scene
                .layer_mut(self.layer)
                .ok_or_else(|| TileError::NotFound(format!("layer {:?}", self.layer)))?;
            let LayerData::Collision(data) = &mut layer.data else {
                return Err(TileError::WrongLayerKind {
                    name: layer.name.clone(),
                    expected: LayerKind::Collision,
                    found: layer.kind,
                });
            };
            data.occupied.push(rect);
            Ok(())
        })
    }

    pub fn clear(&mut self, rect: impl Into<Rect>) -> Result<(), TileError> {
        let rect = rect.into();
        self.target.mutate(|scene| {
            check_rect(scene, rect)?;
            let layer = scene
                .layer_mut(self.layer)
                .ok_or_else(|| TileError::NotFound(format!("layer {:?}", self.layer)))?;
            let LayerData::Collision(data) = &mut layer.data else {
                return Err(TileError::WrongLayerKind {
                    name: layer.name.clone(),
                    expected: LayerKind::Collision,
                    found: layer.kind,
                });
            };
            data.occupied.retain(|occupied| *occupied != rect);
            Ok(())
        })
    }
}

pub struct MetadataLayer<'a> {
    pub(crate) target: EditTarget<'a>,
    pub(crate) layer: LayerId,
}

impl MetadataLayer<'_> {
    pub fn get(&self, cell: impl Into<Cell>) -> Option<&PropertyBag> {
        let cell = cell.into();
        let layer = self
            .target
            .map_data()
            .and_then(|scene| scene.layer(self.layer))?;
        match &layer.data {
            LayerData::Metadata(data) => data.cells.get(&cell),
            _ => None,
        }
    }

    pub fn set(
        &mut self,
        cell: impl Into<Cell>,
        data: impl Into<PropertyBag>,
    ) -> Result<(), TileError> {
        let cell = cell.into();
        let properties = data.into();
        self.target.mutate(|scene| {
            check_bounds(scene, cell)?;
            let layer = scene
                .layer_mut(self.layer)
                .ok_or_else(|| TileError::NotFound(format!("layer {:?}", self.layer)))?;
            let LayerData::Metadata(data) = &mut layer.data else {
                return Err(TileError::WrongLayerKind {
                    name: layer.name.clone(),
                    expected: LayerKind::Metadata,
                    found: layer.kind,
                });
            };
            data.cells.insert(cell, properties);
            Ok(())
        })
    }

    pub fn erase(&mut self, cell: impl Into<Cell>) -> Result<(), TileError> {
        let cell = cell.into();
        self.target.mutate(|scene| {
            check_bounds(scene, cell)?;
            let layer = scene
                .layer_mut(self.layer)
                .ok_or_else(|| TileError::NotFound(format!("layer {:?}", self.layer)))?;
            let LayerData::Metadata(data) = &mut layer.data else {
                return Err(TileError::WrongLayerKind {
                    name: layer.name.clone(),
                    expected: LayerKind::Metadata,
                    found: layer.kind,
                });
            };
            data.cells.remove(&cell);
            Ok(())
        })
    }
}

pub(crate) enum EditTarget<'a> {
    World { world: &'a mut World, map: MapId },
    MapData(&'a mut MapData),
}

impl EditTarget<'_> {
    fn map_data(&self) -> Option<&MapData> {
        match self {
            Self::World { world, map } => runtime_ref(world)
                .and_then(|runtime| runtime.maps.get(map).map(|record| &record.data)),
            Self::MapData(data) => Some(data),
        }
    }

    fn mutate<R>(
        &mut self,
        edit: impl FnOnce(&mut MapData) -> Result<R, TileError>,
    ) -> Result<R, TileError> {
        match self {
            Self::World { world, map } => mutate_world_map(world, *map, edit),
            Self::MapData(data) => edit(data),
        }
    }
}

fn check_bounds(scene: &MapData, cell: CellCoord) -> Result<(), TileError> {
    if cell.x < 0
        || cell.y < 0
        || cell.x as u32 >= scene.size.width
        || cell.y as u32 >= scene.size.height
    {
        return Err(TileError::OutOfBounds {
            cell,
            size: [scene.size.width, scene.size.height],
        });
    }
    Ok(())
}

fn check_rect(scene: &MapData, rect: CellRect) -> Result<(), TileError> {
    if rect.is_empty() {
        return Ok(());
    }
    check_bounds(scene, rect.min)?;
    let max = CellCoord::new(
        rect.min
            .x
            .saturating_add(rect.size[0] as i32)
            .saturating_sub(1),
        rect.min
            .y
            .saturating_add(rect.size[1] as i32)
            .saturating_sub(1),
    );
    check_bounds(scene, max)
}
