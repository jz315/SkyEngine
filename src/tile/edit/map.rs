use std::path::Path;

use crate::ecs::World;

use super::super::io::tiled::{TiledExporter, TiledImporter};
use super::super::model::{
    GridOrientation, GridSpec, LayerId, LayerKind, LayerRole, MapData, MapLayer, MapSize,
    TilePalette, TilePaletteStore,
};
use super::super::runtime::{
    find_map_layer, find_world_layer, insert_map, runtime, runtime_ref, MapSourceBinding,
};
use super::layers::{CollisionLayer, EditTarget, MetadataLayer, ObjectLayer, TileLayer};
use super::{MapId, TileError};

pub struct Tiles<'w> {
    world: &'w mut World,
}

impl<'w> Tiles<'w> {
    pub fn new(world: &'w mut World) -> Self {
        super::super::runtime::ensure_runtime(world);
        Self { world }
    }

    pub fn open_tiled(&mut self, path: impl AsRef<Path>) -> Result<Map<'_>, TileError> {
        let path = path.as_ref();
        let (data, palettes) = TiledImporter::load(path)?;
        let id = insert_map(
            self.world,
            data,
            palettes,
            Some(MapSourceBinding::Tiled(path.to_path_buf())),
        )?;
        self.map(id)
    }

    pub fn create(&mut self, name: impl Into<String>) -> MapBuilder<'_> {
        MapBuilder::new(self.world, name)
    }

    pub fn map(&mut self, id: MapId) -> Result<Map<'_>, TileError> {
        if runtime(self.world)?.maps.contains_key(&id) {
            Ok(Map {
                world: self.world,
                id,
            })
        } else {
            Err(TileError::NotFound(format!("map {:?}", id)))
        }
    }
}

pub struct MapBuilder<'w> {
    world: &'w mut World,
    name: String,
    grid: GridSpec,
    size: MapSize,
    palettes: TilePaletteStore,
    layers: Vec<MapLayer>,
}

impl<'w> MapBuilder<'w> {
    fn new(world: &'w mut World, name: impl Into<String>) -> Self {
        Self {
            world,
            name: name.into(),
            grid: GridSpec::default(),
            size: MapSize::new(1, 1),
            palettes: TilePaletteStore::new(),
            layers: Vec::new(),
        }
    }

    pub fn orthogonal(mut self, cell_size: [u32; 2]) -> Self {
        self.grid = GridSpec::orthogonal(cell_size);
        self
    }

    pub fn isometric(mut self, cell_size: [u32; 2]) -> Self {
        self.grid = GridSpec::isometric(cell_size);
        self
    }

    pub fn staggered(mut self, cell_size: [u32; 2]) -> Self {
        self.grid = GridSpec::new(GridOrientation::Staggered, cell_size);
        self
    }

    pub fn hexagonal(mut self, cell_size: [u32; 2]) -> Self {
        self.grid = GridSpec::new(GridOrientation::Hexagonal, cell_size);
        self
    }

    pub fn size(mut self, size: [u32; 2]) -> Self {
        self.size = MapSize::new(size[0], size[1]);
        self
    }

    pub fn palette(mut self, palette: TilePalette) -> Self {
        self.palettes.insert(palette);
        self
    }

    pub fn palettes(mut self, palettes: impl IntoIterator<Item = TilePalette>) -> Self {
        self.palettes.extend(palettes);
        self
    }

    pub fn tiles(mut self, name: impl Into<String>) -> Self {
        let id = self.next_layer_id();
        self.layers
            .push(MapLayer::tiles(id, name, LayerRole::Ground));
        self
    }

    pub fn objects(mut self, name: impl Into<String>) -> Self {
        let id = self.next_layer_id();
        self.layers
            .push(MapLayer::objects(id, name, LayerRole::Props));
        self
    }

    pub fn collision(mut self, name: impl Into<String>) -> Self {
        let id = self.next_layer_id();
        self.layers
            .push(MapLayer::collision(id, name, LayerRole::Collision));
        self
    }

    pub fn meta(mut self, name: impl Into<String>) -> Self {
        let id = self.next_layer_id();
        self.layers
            .push(MapLayer::metadata(id, name, LayerRole::Gameplay));
        self
    }

    pub fn build(self) -> Result<Map<'w>, TileError> {
        let mut data = MapData::new(MapId(0), self.name, self.grid, self.size);
        data.palettes = self.palettes.iter().map(|palette| palette.id).collect();
        data.layers = self.layers;
        let id = insert_map(
            self.world,
            data,
            self.palettes.iter().cloned().collect::<Vec<_>>(),
            None,
        )?;
        Ok(Map {
            world: self.world,
            id,
        })
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

pub struct Map<'w> {
    world: &'w mut World,
    id: MapId,
}

impl<'w> Map<'w> {
    pub fn id(&self) -> MapId {
        self.id
    }

    pub fn name(&self) -> &str {
        let record = runtime_ref(self.world)
            .and_then(|runtime| runtime.maps.get(&self.id))
            .expect("Map facade should point at an existing runtime map");
        &record.data.name
    }

    pub fn set_name(&mut self, name: impl Into<String>) -> Result<(), TileError> {
        super::super::runtime::mutate_world_map(self.world, self.id, |data| {
            data.name = name.into();
            Ok(())
        })
    }

    pub fn size(&self) -> [u32; 2] {
        let record = runtime_ref(self.world)
            .and_then(|runtime| runtime.maps.get(&self.id))
            .expect("Map facade should point at an existing runtime map");
        [record.data.size.width, record.data.size.height]
    }

    pub fn resize(&mut self, size: [u32; 2]) -> Result<(), TileError> {
        super::super::runtime::mutate_world_map(self.world, self.id, |data| {
            data.size = MapSize::new(size[0], size[1]);
            Ok(())
        })
    }

    pub fn grid(&self) -> GridSpec {
        let record = runtime_ref(self.world)
            .and_then(|runtime| runtime.maps.get(&self.id))
            .expect("Map facade should point at an existing runtime map");
        record.data.grid.clone()
    }

    pub fn set_grid(&mut self, grid: GridSpec) -> Result<(), TileError> {
        super::super::runtime::mutate_world_map(self.world, self.id, |data| {
            data.grid = grid;
            Ok(())
        })
    }

    pub fn tiles(&mut self, name: &str) -> Result<TileLayer<'_>, TileError> {
        let layer = find_world_layer(self.world, self.id, name, LayerKind::Tiles)?;
        Ok(TileLayer {
            target: EditTarget::World {
                world: &mut *self.world,
                map: self.id,
            },
            layer,
        })
    }

    pub fn objects(&mut self, name: &str) -> Result<ObjectLayer<'_>, TileError> {
        let layer = find_world_layer(self.world, self.id, name, LayerKind::Objects)?;
        Ok(ObjectLayer {
            target: EditTarget::World {
                world: &mut *self.world,
                map: self.id,
            },
            layer,
        })
    }

    pub fn collision(&mut self, name: &str) -> Result<CollisionLayer<'_>, TileError> {
        let layer = find_world_layer(self.world, self.id, name, LayerKind::Collision)?;
        Ok(CollisionLayer {
            target: EditTarget::World {
                world: &mut *self.world,
                map: self.id,
            },
            layer,
        })
    }

    pub fn meta(&mut self, name: &str) -> Result<MetadataLayer<'_>, TileError> {
        let layer = find_world_layer(self.world, self.id, name, LayerKind::Metadata)?;
        Ok(MetadataLayer {
            target: EditTarget::World {
                world: &mut *self.world,
                map: self.id,
            },
            layer,
        })
    }

    pub fn edit<F>(&mut self, edit: F) -> Result<(), TileError>
    where
        F: FnOnce(&mut MapEditor<'_>) -> Result<(), TileError>,
    {
        let runtime = runtime(self.world)?;
        let record = runtime
            .maps
            .get_mut(&self.id)
            .ok_or_else(|| TileError::NotFound(format!("map {:?}", self.id)))?;
        let before = record.data.clone();
        let mut editor = MapEditor {
            data: &mut record.data,
        };
        match edit(&mut editor) {
            Ok(()) => {
                record.undo.push(before);
                record.redo.clear();
                Ok(())
            }
            Err(error) => {
                record.data = before;
                Err(error)
            }
        }
    }

    pub fn undo(&mut self) -> Result<(), TileError> {
        let record = runtime(self.world)?
            .maps
            .get_mut(&self.id)
            .ok_or_else(|| TileError::NotFound(format!("map {:?}", self.id)))?;
        let previous = record.undo.pop().ok_or(TileError::UndoUnavailable)?;
        let current = std::mem::replace(&mut record.data, previous);
        record.redo.push(current);
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), TileError> {
        let record = runtime(self.world)?
            .maps
            .get_mut(&self.id)
            .ok_or_else(|| TileError::NotFound(format!("map {:?}", self.id)))?;
        let next = record.redo.pop().ok_or(TileError::UndoUnavailable)?;
        let current = std::mem::replace(&mut record.data, next);
        record.undo.push(current);
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), TileError> {
        let binding = runtime_ref(self.world)
            .and_then(|runtime| runtime.maps.get(&self.id))
            .and_then(|record| record.binding.clone())
            .ok_or(TileError::UnboundMap)?;
        match binding {
            MapSourceBinding::Tiled(path) => self.save_tiled_to(path),
        }
    }

    pub fn save_as(&mut self, path: impl AsRef<Path>) -> Result<(), TileError> {
        let path = path.as_ref().to_path_buf();
        let binding = runtime_ref(self.world)
            .and_then(|runtime| runtime.maps.get(&self.id))
            .and_then(|record| record.binding.clone())
            .ok_or(TileError::UnboundMap)?;
        match binding {
            MapSourceBinding::Tiled(_) => {
                self.save_tiled_to(&path)?;
                runtime(self.world)?
                    .maps
                    .get_mut(&self.id)
                    .expect("saved map should still exist")
                    .binding = Some(MapSourceBinding::Tiled(path));
                Ok(())
            }
        }
    }

    pub fn save_as_tiled(&mut self, path: impl AsRef<Path>) -> Result<(), TileError> {
        let path = path.as_ref().to_path_buf();
        self.save_tiled_to(&path)?;
        runtime(self.world)?
            .maps
            .get_mut(&self.id)
            .expect("saved map should still exist")
            .binding = Some(MapSourceBinding::Tiled(path));
        Ok(())
    }

    fn save_tiled_to(&mut self, path: impl AsRef<Path>) -> Result<(), TileError> {
        let record = runtime_ref(self.world)
            .and_then(|runtime| runtime.maps.get(&self.id))
            .ok_or_else(|| TileError::NotFound(format!("map {:?}", self.id)))?;
        let palettes = record.palettes.iter().cloned().collect::<Vec<_>>();
        TiledExporter::write_tmj(path, &record.data, &palettes)?;
        Ok(())
    }
}

pub struct MapEditor<'a> {
    data: &'a mut MapData,
}

impl MapEditor<'_> {
    pub fn tiles(&mut self, name: &str) -> Result<TileLayer<'_>, TileError> {
        let layer = find_map_layer(self.data, name, LayerKind::Tiles)?;
        Ok(TileLayer {
            target: EditTarget::MapData(&mut *self.data),
            layer,
        })
    }

    pub fn objects(&mut self, name: &str) -> Result<ObjectLayer<'_>, TileError> {
        let layer = find_map_layer(self.data, name, LayerKind::Objects)?;
        Ok(ObjectLayer {
            target: EditTarget::MapData(&mut *self.data),
            layer,
        })
    }

    pub fn collision(&mut self, name: &str) -> Result<CollisionLayer<'_>, TileError> {
        let layer = find_map_layer(self.data, name, LayerKind::Collision)?;
        Ok(CollisionLayer {
            target: EditTarget::MapData(&mut *self.data),
            layer,
        })
    }

    pub fn meta(&mut self, name: &str) -> Result<MetadataLayer<'_>, TileError> {
        let layer = find_map_layer(self.data, name, LayerKind::Metadata)?;
        Ok(MetadataLayer {
            target: EditTarget::MapData(&mut *self.data),
            layer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::{TileCell, TileRef};

    #[test]
    fn create_edit_and_undo_live_map() {
        let mut world = World::new();
        let mut tiles = Tiles::new(&mut world);
        let mut map = tiles
            .create("test")
            .orthogonal([16, 16])
            .size([2, 2])
            .tiles("Ground")
            .build()
            .expect("map should build");

        map.tiles("Ground")
            .expect("tile layer")
            .set([0, 0], (1, 2))
            .expect("set tile");
        assert_eq!(
            map.tiles("Ground").expect("tile layer").get([0, 0]),
            Some(TileCell::new(TileRef::new(
                crate::tile::PaletteId(1),
                crate::tile::TileDefId(2)
            )))
        );

        map.undo().expect("undo should restore previous scene");
        assert_eq!(map.tiles("Ground").expect("tile layer").get([0, 0]), None);
    }

    #[test]
    fn grouped_edit_rolls_back_on_error() {
        let mut world = World::new();
        let mut tiles = Tiles::new(&mut world);
        let mut map = tiles
            .create("test")
            .orthogonal([16, 16])
            .size([1, 1])
            .tiles("Ground")
            .build()
            .expect("map should build");

        let error = map
            .edit(|edit| {
                edit.tiles("Ground")?.set([0, 0], (1, 1))?;
                edit.tiles("Ground")?.set([2, 2], (1, 2))?;
                Ok(())
            })
            .expect_err("out-of-bounds edit should fail");
        assert!(matches!(error, TileError::OutOfBounds { .. }));
        assert_eq!(map.tiles("Ground").expect("tile layer").get([0, 0]), None);
    }

    #[test]
    fn save_as_requires_existing_format_binding() {
        let mut world = World::new();
        let mut tiles = Tiles::new(&mut world);
        let mut map = tiles.create("test").size([1, 1]).build().expect("map");
        let temp = tempfile::tempdir().expect("tempdir");
        let first = temp.path().join("first.tmj");
        let second = temp.path().join("second.tmj");

        assert!(matches!(map.save_as(&first), Err(TileError::UnboundMap)));
        map.save_as_tiled(&first)
            .expect("first explicit Tiled save");
        map.save_as(&second)
            .expect("save_as should reuse the Tiled binding");
    }
}
