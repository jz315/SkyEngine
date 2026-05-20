use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::ecs::{EntityId, World};

use super::super::edit::{MapId, TileError};
use super::super::model::{GridSpec, LayerId, LayerKind, MapData, TilePalette, TilePaletteStore};

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct RuntimeMapRoot {
    pub id: MapId,
    pub name: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct RuntimeMapGrid {
    pub grid: GridSpec,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub(crate) struct RuntimeMapBounds {
    pub size: [u32; 2],
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct RuntimeLayerNode {
    pub map: MapId,
    pub id: LayerId,
    pub name: String,
    pub kind: LayerKind,
}

#[derive(Default)]
pub(crate) struct TileRuntime {
    pub next_map_id: u64,
    pub maps: BTreeMap<MapId, RuntimeMapRecord>,
}

pub(crate) struct RuntimeMapRecord {
    pub data: MapData,
    pub palettes: TilePaletteStore,
    pub binding: Option<MapSourceBinding>,
    _root: EntityId,
    _layers: Vec<EntityId>,
    pub undo: Vec<MapData>,
    pub redo: Vec<MapData>,
}

#[derive(Clone, Debug)]
pub(crate) enum MapSourceBinding {
    Tiled(PathBuf),
}

pub(crate) fn ensure_runtime(world: &mut World) {
    if world.get_resource::<TileRuntime>().is_none() {
        world.insert_resource(TileRuntime {
            next_map_id: 1,
            maps: BTreeMap::new(),
        });
    }
}

pub(crate) fn runtime(world: &mut World) -> Result<&mut TileRuntime, TileError> {
    ensure_runtime(world);
    world
        .get_resource_mut::<TileRuntime>()
        .ok_or_else(|| TileError::InvalidOperation("tile runtime resource is missing".to_string()))
}

pub(crate) fn runtime_ref(world: &World) -> Option<&TileRuntime> {
    world.get_resource::<TileRuntime>()
}

pub(crate) fn insert_map(
    world: &mut World,
    mut data: MapData,
    palettes: impl IntoIterator<Item = TilePalette>,
    binding: Option<MapSourceBinding>,
) -> Result<MapId, TileError> {
    ensure_runtime(world);
    let palettes = palettes.into_iter().collect::<Vec<_>>();
    let id = {
        let runtime = runtime(world)?;
        let id = MapId(runtime.next_map_id.max(1));
        runtime.next_map_id = id.0.saturating_add(1);
        id
    };
    data.id = id;
    if data.palettes.is_empty() {
        data.palettes = palettes.iter().map(|palette| palette.id).collect();
    }
    let mut store = TilePaletteStore::new();
    store.extend(palettes);
    insert_map_with_store(world, data, store, binding)
}

fn insert_map_with_store(
    world: &mut World,
    data: MapData,
    palettes: TilePaletteStore,
    binding: Option<MapSourceBinding>,
) -> Result<MapId, TileError> {
    let root = world.spawn((
        RuntimeMapRoot {
            id: data.id,
            name: data.name.clone(),
        },
        RuntimeMapGrid {
            grid: data.grid.clone(),
        },
        RuntimeMapBounds {
            size: [data.size.width, data.size.height],
        },
    ));
    let layers = data
        .layers
        .iter()
        .map(|layer| {
            world.spawn((RuntimeLayerNode {
                map: data.id,
                id: layer.id,
                name: layer.name.clone(),
                kind: layer.kind,
            },))
        })
        .collect::<Vec<_>>();
    let id = data.id;
    runtime(world)?.maps.insert(
        id,
        RuntimeMapRecord {
            data,
            palettes,
            binding,
            _root: root,
            _layers: layers,
            undo: Vec::new(),
            redo: Vec::new(),
        },
    );
    Ok(id)
}

pub(crate) fn mutate_world_map<R>(
    world: &mut World,
    id: MapId,
    edit: impl FnOnce(&mut MapData) -> Result<R, TileError>,
) -> Result<R, TileError> {
    let record = runtime(world)?
        .maps
        .get_mut(&id)
        .ok_or_else(|| TileError::NotFound(format!("map {:?}", id)))?;
    let before = record.data.clone();
    match edit(&mut record.data) {
        Ok(value) => {
            record.undo.push(before);
            record.redo.clear();
            Ok(value)
        }
        Err(error) => {
            record.data = before;
            Err(error)
        }
    }
}

pub(crate) fn find_world_layer(
    world: &World,
    map: MapId,
    name: &str,
    kind: LayerKind,
) -> Result<LayerId, TileError> {
    let runtime = runtime_ref(world).ok_or_else(|| {
        TileError::InvalidOperation("tile runtime resource is missing".to_string())
    })?;
    let record = runtime
        .maps
        .get(&map)
        .ok_or_else(|| TileError::NotFound(format!("map {:?}", map)))?;
    find_map_layer(&record.data, name, kind)
}

pub(crate) fn find_map_layer(
    data: &MapData,
    name: &str,
    kind: LayerKind,
) -> Result<LayerId, TileError> {
    let layer = data
        .layer_by_name(name)
        .ok_or_else(|| TileError::NotFound(format!("layer {name}")))?;
    if layer.kind != kind {
        return Err(TileError::WrongLayerKind {
            name: name.to_string(),
            expected: kind,
            found: layer.kind,
        });
    }
    Ok(layer.id)
}
