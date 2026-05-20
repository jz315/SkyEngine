# Tilemap Module And Runtime Refactor Plan

## Status

Current working plan for the SkyEngine tile module.

Phase 1 layout cleanup is implemented. The module now has `model`, `runtime`,
`edit`, `io`, and `render_bridge` boundaries while preserving the existing
public `sky_engine::tile::*` re-exports.

This replaces the older split plans:

- `tilemap_api_standard.md`
- `tilemap_ecs_truth_architecture_plan.md`

Those older documents described useful goals, but they now overlap and use
several names that no longer match the current direction. This document is the
single plan for naming, module layout, public API shape, ECS runtime ownership,
and render consumption.

## North Star

The user-facing tile API is:

```text
Tiles -> Map -> typed layers
```

The implementation flow is:

```text
Tiles / Map edit facade
  -> World-owned tile runtime truth
  -> derived render / physics / navigation caches
```

The public user should not need to understand documents, instances, sync,
refresh, storage layers, palette splits, render storage, or codec registries in
normal tilemap usage.

## Core Decisions

### One Runtime Truth

Runtime tilemap truth must live in `World`.

That does not mean one tile equals one ECS entity. Large tile payloads may live
in chunked data structures or ECS-owned storage resources. The rule is that
gameplay, editor tools, render extraction, physics extraction, and save/export
all derive from the same world-owned runtime state.

Do not reintroduce a long-lived `Document + mounted instance` workflow where
the document and runtime can diverge.

### Render May Consume Tile ECS Truth

Render should consume tile runtime truth directly through an internal bridge or
extractor.

Recommended flow:

```text
tile runtime dirty state
  -> tile render extractor / bridge
  -> render::TilemapStorage or prepared tile batches
  -> GPU cache / draw
```

Render must not depend on high-level edit facades, undo/redo, Tiled import
types, or save semantics. Render reads a stable runtime snapshot/change surface
and builds render caches from it.

### Public API Stays Small

Keep these public concepts:

- `Tiles`
- `Map`
- `MapBuilder`
- `MapEditor`
- `TileLayer`
- `ObjectLayer`
- `CollisionLayer`
- `MetadataLayer`
- `TileError`
- `MapId`
- `TileRef`
- `TilePalette`

Do not expose these as normal user concepts:

- `Document`
- `Instance`
- `refresh`
- `sync`
- `TilemapStorage`
- `storage_layer`
- `palette_split`
- `codec`
- generic format registries

Format support stays explicit in v1:

```rust
let mut map = tiles.open_tiled("maps/sewers.tmx")?;
map.save_as_tiled("maps/sewers_copy.tmj")?;
```

Future formats should follow the same shape, for example `open_ldtk` and
`save_as_ldtk`.

## Current Module Layout

The tile module is currently organized as:

```text
src/tile/
  mod.rs

  model/
    mod.rs
    map.rs
    grid.rs
    layer/
      mod.rs
      data.rs
      tile.rs
      types.rs
    object.rs
    palette/
      mod.rs
      property.rs
      store.rs
      types.rs
    color.rs

  runtime/
    mod.rs
    world.rs

  edit/
    mod.rs
    map.rs
    layers.rs
    error.rs

  io/
    mod.rs
    tiled/
      mod.rs
      import.rs
      export.rs
      palette.rs
      properties.rs
      scene.rs
      tests.rs

  render_bridge/
    mod.rs
    palette.rs
```

Future runtime dirty state, render extraction, and render cache modules should
be added only when Phase 3 implements automatic render consumption.

## Naming Standard

### User-Facing Names

These are the canonical names in examples, docs, and ordinary user code:

- `Tiles`: entry point bound to `World`.
- `Map`: short-lived live map facade.
- `MapId`: stable handle stored by systems/resources.
- `MapBuilder`: builder returned by `Tiles::create`.
- `MapEditor`: transactional edit facade.
- `TileLayer`, `ObjectLayer`, `CollisionLayer`, `MetadataLayer`: typed layer
  edit facades.

These names should remain stable.

### Internal Model Names

The current internal `scene.rs` file should be renamed because it is not an app
scene, render scene, or scene graph. It is the internal data model for one map.

Target names:

```text
scene.rs       -> model/map.rs
TileMap        -> MapData
TileMapSize    -> MapSize
SceneTile      -> TileCell
TileLayer      -> MapLayer
```

Do not keep compatibility aliases for the old internal names. Code should move
directly to the new names so the module boundary remains explicit.

### Runtime Names

Runtime names should say what owns the data:

```text
TileRuntime     -> TileRuntime
MapRecord       -> RuntimeMapRecord
MapBinding      -> MapSourceBinding
TileMapRoot     -> RuntimeMapRoot
TileLayerNode   -> RuntimeLayerNode
```

The word `runtime` is useful here because this layer is explicitly about
`World` ownership and live state.

### IO Names

The current `adapters` module should become `io`.

Target names:

```text
adapters/       -> io/
adapters/tiled  -> io/tiled
TiledImportData -> TiledMapSnapshot
TiledImporter   -> TiledImporter
TiledExporter   -> TiledExporter
```

`TiledImporter` and `TiledExporter` are acceptable because they are explicit
format entry points. Avoid generic `Adapter`, `Codec`, `Endpoint`, or
`Registry` names until more than one real format needs shared public
abstractions.

### Render Bridge Names

The current `sync` module should not be public API. The word `sync` is useful
internally, but it violates the user-facing standard if exposed.

Target names:

```text
render_bridge/palette.rs    -> palette/render conversion helpers
palette_to_tileset_grid     -> palette/render grid conversion
future map render bridge    -> render_bridge/extract.rs
```

Keep `render_bridge` internal unless there is a clear expert API reason to
expose it.

## Current Code State

Completed refactor work:

- `src/tile/api.rs` was split into `edit` and `runtime`.
- `Tiles`, `Map`, `MapBuilder`, and typed layer facades live under `edit`.
- World-owned runtime state lives under `runtime`.
- Core model files live under `model`.
- Tiled import/export lives under `io/tiled`.
- Palette-to-render-grid helpers live under internal `render_bridge`.
- Tile-owned `Color`, `TileFlags`, and `TileRenderOrder` exist.
- Palette to render-grid conversion moved out of palette model and into a
  render bridge.
- Tiled import now converts render parser output into a tile-owned snapshot
  before scene/palette conversion.

Remaining design debt:

- `TiledImporter` still uses `render::TiledImport` as the parser entry point,
  though the dependency is now contained.
- Render does not yet consume live tile runtime truth automatically.

Additional standard cleanup now completed:

- `io/tiled/scene.rs` was renamed to `io/tiled/map.rs`.
- `TiledImportData` was renamed to `TiledMapSnapshot`.
- Runtime names now use `RuntimeMapRecord`, `RuntimeMapRoot`,
  `RuntimeLayerNode`, and `MapSourceBinding`.
- Runtime helpers now talk about maps/data instead of scenes.

## Desired Runtime Data Shape

### Runtime Resource

The first version may keep a single resource:

```rust
pub(crate) struct TileRuntime {
    next_map_id: u64,
    maps: BTreeMap<MapId, RuntimeMapRecord>,
}
```

Each map record should contain:

```rust
pub(crate) struct RuntimeMapRecord {
    data: MapData,
    palettes: TilePaletteStore,
    binding: Option<MapSourceBinding>,
    root: EntityId,
    layers: Vec<EntityId>,
    dirty: MapDirtyState,
    undo: Vec<MapData>,
    redo: Vec<MapData>,
    render: Option<RenderMapBinding>,
}
```

This is acceptable as an intermediate ECS-owned resource. It still lives in
`World`, so it is a single runtime truth. Later, if query access and system
composition demand it, map/layer/chunk payloads can move further into typed ECS
components/entities.

### ECS Marker Components

Runtime marker components should represent identities and relationships, not
duplicate the full map truth:

```rust
pub(crate) struct RuntimeMapRoot {
    pub id: MapId,
    pub name: String,
}

pub(crate) struct RuntimeLayerNode {
    pub map: MapId,
    pub id: LayerId,
    pub name: String,
    pub kind: LayerKind,
}
```

If these markers become observable by systems, updates to name, size, layer
structure, and visibility must keep them synchronized with `RuntimeMapRecord`.

## Render Consumption Plan

### Goal

Render should update from tile runtime truth without user code calling
`sync_*`, `refresh_*`, or manually creating `TilemapStorage`/`TilemapRenderer`
for ordinary maps.

### Proposed Flow

```text
Map edit commits
  -> mark MapDirtyState
  -> render bridge consumes dirty maps
  -> update or create TilemapStorage entries
  -> ensure TilemapRenderer layer entities/components
  -> render::tilemap extraction draws normal tilemap data
```

### Render Binding

Add a per-map binding that records render-side handles:

```rust
pub(crate) struct RenderMapBinding {
    pub map_handle: crate::render::TilemapHandle,
    pub layer_entities: Vec<EntityId>,
    pub layer_bindings: Vec<RenderLayerBinding>,
    pub version: u64,
}

pub(crate) struct RenderLayerBinding {
    pub source_layer: LayerId,
    pub palette: PaletteId,
    pub storage_layer: u32,
    pub entity: EntityId,
}
```

This binding is derived state. It can be rebuilt from tile runtime truth.

### Dirty State

Add explicit dirty tracking:

```rust
pub(crate) struct MapDirtyState {
    pub structure: bool,
    pub palettes: bool,
    pub layers: BTreeMap<LayerId, LayerDirtyState>,
    pub version: u64,
}

pub(crate) struct LayerDirtyState {
    pub full: bool,
    pub cells: Vec<CellCoord>,
    pub rects: Vec<CellRect>,
}
```

Simple first implementation:

- mark full layer dirty for all tile edits
- mark structure dirty for layer add/remove or resize
- optimize later to cells/rects/chunks after behavior is stable

### Bridge Responsibilities

The render bridge may know both `tile` and `render` types.

It may:

- convert `TilePalette` to `TilesetGrid`
- convert `TileCell` to `render::Tile`
- split layers by palette when required by render storage
- maintain render layer entities
- update `TilemapStorage`

It must not:

- parse Tiled files
- own save semantics
- expose normal user APIs
- mutate edit history directly
- become the semantic source of tile truth

## IO Plan

### Tiled Import

Current transitional flow:

```text
render::TiledImport parser
  -> tile::io/tiled snapshot
  -> MapData + TilePalette list
  -> TileRuntime insert
```

Target flow:

```text
tile::io::tiled parser
  -> TiledMapSnapshot
  -> MapData + TilePalette list
  -> TileRuntime insert
```

Short-term acceptable state:

- keep using `render::TiledImport` as the parser backend
- keep all direct use of render parser types inside `io/tiled/import.rs`
- do not let `scene`/`model`, `palette`, `runtime`, or `edit` consume render
  parser types

### Tiled Export

Export should write from runtime truth:

```text
Map facade save
  -> RuntimeMapRecord data + palettes
  -> TiledExporter
```

No separate public `export` API is needed in v1. Saving to another path is
`save_as` or `save_as_tiled`.

## Public API Contract

Keep:

```rust
let mut tiles = sky_engine::tile::Tiles::new(world);
let mut map = tiles.open_tiled("maps/sewers.tmx")?;
map.tiles("Ground")?.set([12, 4], grass)?;
map.objects("Props")?.place(object("crate").at([5, 6]))?;
map.save()?;
```

New maps:

```rust
let mut map = tiles
    .create("Overworld")
    .orthogonal([32, 32])
    .size([128, 64])
    .tiles("Ground")
    .objects("Props")
    .build()?;

map.save_as_tiled("maps/overworld.tmj")?;
```

Rules:

- `open_tiled` is explicit and does not guess from extension.
- `save_as` changes path only after a binding exists.
- unbound `save` / `save_as` returns `UnboundMap`.
- grouped edits through `map.edit` are one undoable action.
- failed grouped edits roll back.

## Migration Phases

### Phase 1: Finish Naming Layout

Status: done.

Moved files without changing behavior:

```text
scene.rs                -> model/map.rs
grid.rs                 -> model/grid.rs
layer.rs + layer/*      -> model/layer/*
object.rs               -> model/object.rs
palette.rs + palette/*  -> model/palette/*
color.rs                -> model/color.rs
adapters/tiled/*        -> io/tiled/*
sync/render.rs          -> render_bridge/palette.rs
```

Maintain public re-exports during the move.

Validation:

```bash
cargo test --features app tile::
cargo check --examples --features app
```

### Phase 2: Rename Internal Model Types

Status: done.

Renamed internal model types without compatibility aliases:

```text
TileMap      -> MapData
TileMapSize  -> MapSize
SceneTile    -> TileCell
TileLayer    -> MapLayer
```

No temporary aliases are kept for those old names.

Validation:

```bash
cargo test --features app tile::
cargo test --features app render::tilemap::tiled
cargo check --examples --features app
```

### Phase 3: Make Render Bridge Automatic

Add render binding and dirty state to runtime records.

Implement an internal update path that can:

- mount a runtime map into render storage
- update dirty tile layers after edits
- rebuild render layer fan-out after structure/palette changes

Candidate user-facing installation shape:

```rust
// Preferred: automatic through app renderer / TilemapFeature once a map exists.
let mut map = tiles.open_tiled(path)?;

// Optional expert/internal API if explicit mount is necessary.
tile::render_bridge::mount(world, map.id())?;
```

Do not expose this as `sync_*` in the ordinary public API.

Validation:

```bash
cargo test --features app tile::
cargo test --features app render::tilemap
cargo check --examples --features app
```

### Phase 4: Move Tiled Parser Ownership

After render bridge behavior is stable, decide whether to:

- keep render's Tiled parser as a shared internal parser backend, or
- move parser implementation into `tile::io::tiled`.

The important constraint is that model/runtime/edit modules must not depend on
render parser types.

If moving the parser:

- copy parser data types into `tile::io::tiled`
- update render-only `TiledMapInstance` to consume tile IO snapshots or keep a
  render-only fast path clearly marked as such
- keep official Tiled sample tests passing

Validation:

```bash
cargo test --features app tile::
cargo test --features app render::tilemap::tiled
cargo test --features app render::tilemap::instance
cargo check --examples --features app
```

### Phase 5: ECS Component Deepening

Only do this after the simpler resource-backed runtime is stable.

Evaluate moving more truth into ECS components/entities:

- map root metadata
- layer metadata
- object entities
- chunk payload components or chunk-keyed storage resources

Do not split every cell into an entity.

This phase is worthwhile only if systems need direct typed ECS query access to
tile structures or if dirty propagation becomes cleaner through ECS components.

## Documentation Rules

After this plan lands:

- Do not create new tile architecture plan files for the same topic.
- Update this file instead.
- Keep `AGENTS.md` factual and current; future proposals stay here.
- Remove references to old document/instance runtime workflows from examples
  and docs as they are migrated.
- Normal user docs should show only `Tiles -> Map -> typed layers`.

## Acceptance Criteria

This refactor is done when:

- `src/tile` has clear `model`, `runtime`, `edit`, `io`, and `render_bridge`
  layers.
- public examples use `Tiles -> Map -> typed layers`.
- no ordinary user API exposes document, instance, sync, refresh, render
  storage, storage layers, or palette splits.
- render updates from world-owned tile runtime truth.
- Tiled import/export still round-trips the supported subset.
- app/render examples compile with `cargo check --examples --features app`.
- tile tests pass with `cargo test --features app tile::`.
