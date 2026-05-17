# Tile Scene Runtime Architecture Plan

## Purpose

SkyEngine currently has a capable chunked tilemap renderer and a Tiled import
bridge, but the runtime model is still too close to "renderable tile layers".
Games need a higher-level tile scene model that can represent many tile
workflows:

- orthogonal tiles
- isometric tiles
- staggered isometric tiles
- hex tiles
- large overhanging tiles
- animated tiles
- object-like tiles
- collision and metadata layers
- runtime buildable/editable maps
- authoring round trips through external editors such as Tiled

The goal is not to make Tiled the runtime source of truth. The goal is to make
SkyEngine's own tile scene runtime the source of truth, with Tiled and future
tools acting as authoring/import/export adapters.

## Current Architecture

The current `src/render/tilemap` module owns the chunked tilemap renderer and
the Tiled import bridge.

```text
Tiled file
  -> TiledImport
  -> Tilemap + TiledLayer/TiledObjectLayer metadata
  -> TiledMapInstance::spawn(...)
  -> TilemapStorage resource + TilemapRenderer/SpriteRenderer entities
  -> ExtractTilemaps
  -> TilemapFrameCache
  -> DrawTilemap
```

Current important types:

- `TilemapStorage`
  - World resource storing large tile data outside ECS component storage.
- `TilemapHandle`
  - Generation-checked handle into `TilemapStorage`.
- `Tilemap`
  - Layered, chunked storage for `Tile` values.
  - Chunks track dirty versions and non-empty tile counts.
- `TilemapRenderer`
  - ECS component selecting one `Tilemap` layer to draw.
  - Stores tileset grid, logical tile size, draw size, tile offset,
    orientation, render order, depth sort, color, visibility, and layer mask.
- `TilesetGrid`
  - Atlas/rect/UV metadata and per-tile draw sizes.
- `TiledImport`
  - Parses `.tmx`, `.tmj`, and `.json` into engine tilemap data and Tiled
    metadata.
- `TiledMapInstance`
  - Runtime spawn/despawn helper that loads textures, inserts `TilemapStorage`,
    spawns tile layer renderers, and spawns some Tiled tile objects as sprites.

This is a good low-level rendering foundation. It should remain.

## Current Limitations

The current model becomes awkward when a game wants a full tile scene workflow.

1. `Tilemap` is mostly render/storage oriented, not a semantic scene model.
2. Layer roles are implicit names or numeric layer indices, not structured data.
3. Tiled tile objects may become `SpriteRenderer` entities, which creates a
   second placement/rendering rule beside tilemap rendering.
4. There is no editor/runtime edit session API that tracks changes in a format
   suitable for saving, replaying, undoing, or exporting.
5. `TiledImport` is the main import surface, but there is no format-neutral
   `TileMap` target that future Tiled/LDtk/custom importers can share.
6. Image collection tilesets and large multi-image tile palettes need stronger
   runtime atlas support.
7. Collision, gameplay metadata, build rules, and authoring properties are
   present only as importer metadata, not as first-class runtime concepts.
8. Example game code may manually assemble layers, sprites, atlases, and
   placement rules, which makes it easy to diverge from editor behavior.

## Target Principle

Separate the system into three layers:

```text
Authoring Format Layer
  Tiled, LDtk, custom editor files, generated documents

Engine Runtime Layer
  TileMap, TileWorld, TilePalette, TileLayer, TileObject

Persistence / Exchange Layer
  save deltas, snapshots, export to Tiled, export to custom editor formats
```

The runtime source of truth should be SkyEngine's `TileMap`, not a Tiled
document.

Tiled should be:

- an importer into `TileMap`
- optionally an exporter from `TileMap`
- optionally a source of preserved authoring metadata

It should not be the shape that all gameplay systems are forced to mutate.

## Target Architecture

```text
Authoring source
  -> Adapter import
  -> TileMapAsset / TileMapDocument
  -> TileMap runtime
  -> TilemapStorage + TilemapRenderer sync
  -> ExtractTilemaps
  -> TilemapFrameCache
  -> DrawTilemap

Runtime edits
  -> TileMapEditSession
  -> TileMap runtime mutation
  -> dirty render sync
  -> save delta / snapshot / export
```

Suggested module split:

```text
src/tile/
  mod.rs
  grid.rs
  scene.rs
  palette.rs
  layer.rs
  object.rs
  edit.rs
  edit/
    summary.rs
    session.rs
    history.rs
  persistence.rs

src/render/tilemap/
  storage.rs
  component/tilemap.rs
  extract.rs
  draw.rs
  cache.rs
  sync.rs

src/tile/adapters/tiled/
  import.rs
  export.rs
  document.rs
  tileset.rs
  properties.rs

src/tile/adapters/ldtk/
  import.rs
```

This exact module layout can change, but the responsibilities should stay
separate:

- `src/tile`: format-neutral runtime model.
- `src/render/tilemap`: render storage/backend and GPU extraction.
- `adapters`: external format conversion.

## Runtime Data Model

### TileWorld

`TileWorld` is an optional high-level container for projects with multiple
loaded maps/scenes.

```rust
pub struct TileWorld {
    pub palettes: TilePaletteStore,
    pub scenes: Vec<TileMap>,
}
```

### TileMap

`TileMap` is the runtime truth for a single map.

```rust
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
```

It should be usable without any Tiled dependency.

### GridSpec

`GridSpec` defines coordinate semantics.

```rust
pub struct GridSpec {
    pub orientation: GridOrientation,
    pub cell_size: [u32; 2],
    pub origin: GridOrigin,
    pub render_order: TileRenderOrder,
    pub stagger_axis: Option<StaggerAxis>,
    pub stagger_index: Option<StaggerIndex>,
    pub hex_side_length: Option<u32>,
}

pub enum GridOrientation {
    Orthogonal,
    Isometric,
    Staggered,
    Hexagonal,
}
```

The important invariant is that logical cell size and drawn tile image size are
different concepts.

Examples:

```text
Top-down 32x32:
  cell_size = 32x32
  draw_size = 32x32
  draw_offset = 0,0

Kenney isometric miniature:
  cell_size = 256x128
  draw_size = 256x512
  draw_offset = 0,0

Tall tree:
  cell_size = 32x32
  draw_size = 64x96
  draw_offset = -16,-64
```

### TilePalette

`TilePalette` is the runtime tileset/palette model.

```rust
pub struct TilePalette {
    pub id: PaletteId,
    pub name: String,
    pub source: Option<AssetSource>,
    pub texture: TileTextureSource,
    pub tiles: Vec<TileDef>,
    pub properties: PropertyBag,
}
```

`TilePalette` must support:

- single-image grid tilesets
- image collection tilesets
- runtime-built atlases
- fixed source rects
- per-tile draw size
- per-tile draw offset
- animations
- collision shapes
- custom properties

```rust
pub struct TileDef {
    pub id: TileDefId,
    pub name: Option<String>,
    pub source_rect: RectU,
    pub draw_size: [u32; 2],
    pub draw_offset: [i32; 2],
    pub animation: Option<TileAnimation>,
    pub collision: Option<TileCollision>,
    pub properties: PropertyBag,
}
```

### TileLayer

Layers should have structured semantics instead of relying only on numbers.

```rust
pub struct TileLayer {
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

pub enum LayerKind {
    Tiles,
    Objects,
    Collision,
    Metadata,
}

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
```

`LayerRole` is not a renderer phase. It is gameplay/editor metadata that helps
apps find the right layer.

### LayerData

```rust
pub enum LayerData {
    Tiles(TileLayerData),
    Objects(ObjectLayerData),
    Collision(CollisionLayerData),
    Metadata(MetadataLayerData),
}
```

Tile layer data should remain chunkable and sparse-friendly.

```rust
pub struct TileLayerData {
    pub tiles: ChunkedTileData,
}

pub struct SceneTile {
    pub tile_ref: TileRef,
    pub flags: TileFlags,
    pub tint: Color,
}

pub struct TileRef {
    pub palette: PaletteId,
    pub tile: TileDefId,
}
```

### TileObject

`TileObject` represents object-like content that should be addressable as an
object, even if its visual is a tile.

```rust
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
```

Object visuals can still render through tilemap layers when possible.

```rust
pub enum ObjectVisual {
    Tile(TileRef),
    MultiTile(Vec<ObjectVisualTile>),
    Sprite(SpriteVisualRef),
    None,
}
```

The default path for isometric buildable props should be `ObjectVisual::Tile`,
not `SpriteRenderer`, so placement follows the same tile draw rules as the
ground and walls.

## Rendering Backend Relationship

The existing `TilemapStorage` and `TilemapRenderer` should become the default
render backend for `TileMap`.

```text
TileMap
  -> TileMapRenderSync
  -> TilemapStorage
  -> TilemapRenderer per visual layer
```

The sync layer owns the mapping:

- `LayerId -> Tilemap storage layer`
- `PaletteId -> TilesetGrid / texture handle`
- `TileRef -> TileId`
- scene dirty regions -> tilemap dirty chunks

Manual low-level APIs should continue to exist:

```rust
TilemapStorage
Tilemap
TilemapRenderer
TilesetGrid
```

High-level scene APIs should build on them rather than replace them.

## Authoring Adapter Model

### Tiled Import

Tiled import should become:

```text
Tiled document
  -> TiledImporter
  -> TileMap
  -> optional preserved Tiled metadata
```

Mapping:

- Tiled map orientation -> `GridSpec`
- Tiled tile size -> `GridSpec::cell_size`
- Tiled tilesets -> `TilePalette`
- Tiled tile layers -> `TileLayer { kind: Tiles }`
- Tiled object layers -> `TileLayer { kind: Objects }`
- Tiled properties -> `PropertyBag`
- Tiled layer order -> `TileMap.layers`
- Tiled gids -> `TileRef`
- Tiled tileoffset -> `TileDef.draw_offset` or palette-level default
- Tiled image collection tilesets -> runtime atlas-backed `TilePalette`

The adapter should preserve enough metadata to export back when possible:

- source file path
- tileset source paths
- firstgid mapping
- layer IDs and next IDs
- object IDs and next IDs
- unsupported/raw properties when practical

### Tiled Export

Export should not be mandatory for all scenes, but it should support the common
round trip:

```text
Tiled -> TileMap -> runtime edits -> Tiled-compatible TMJ/TMX
```

Recommended first exporter target: TMJ/JSON.

Reasons:

- easier structured output
- easier partial metadata preservation
- easier automated tests

TMX/XML export can come later.

### Future Adapters

The runtime model should make these future adapters possible:

- LDtk import
- custom SkyEngine tile scene JSON/RON
- generated maps
- in-engine editor save files
- save deltas for player-built worlds

Adapters should convert into `TileMap`; gameplay should not depend on the
adapter-specific document model.

## Runtime Editing API

Games should mutate `TileMap`, not raw Tiled documents.

```rust
impl TileMapEditSession {
    pub fn set_tile(&mut self, layer: LayerId, cell: CellCoord, tile: Option<SceneTile>);
    pub fn fill_rect(&mut self, layer: LayerId, rect: CellRect, tile: Option<SceneTile>);
    pub fn place_object(&mut self, layer: LayerId, object: TileObject) -> TileObjectId;
    pub fn remove_object(&mut self, id: TileObjectId);
    pub fn set_property(&mut self, target: PropertyTarget, key: &str, value: PropertyValue);
}
```

The edit session should track:

- dirty cells/regions
- changed layers
- created/removed objects
- property changes
- undo/redo command data when enabled
- save delta data when enabled

## Persistence Model

Support three persistence modes:

1. Full runtime snapshot

```text
TileMap -> SkyEngine scene file
```

2. Delta save

```text
base authoring scene + player/runtime edits
```

3. Authoring export

```text
TileMap + preserved adapter metadata -> Tiled/LDtk/custom file
```

Player save data should usually prefer deltas. Editor workflows can prefer
full export.

## Miniature Builder Migration

The miniature builder example is a good validation target because it stresses:

- isometric tile coordinates
- large tile draw size
- full transparent canvas alignment
- runtime building/editing
- layer semantics
- Tiled/Kenney authoring data fidelity

Target example structure:

```text
examples/game/miniature_builder/
  main.rs
  assets.rs
  defs.rs
  board.rs
  placement.rs
  render_sync.rs
  input.rs
```

Target scene layers:

```text
Ground
Detail
Props
Walls
Upper
Preview
Collision
Gameplay
```

Migration steps:

1. Keep current visual behavior using native Kenney dimensions:
   - cell size `256x128`
   - tile image canvas `256x512`
   - no transparent trimming
2. Build a runtime tile palette for all used Kenney PNGs.
3. Change the board to store object instances, not sprite entities.
4. Add `render_sync.rs` to write object visuals into tilemap layers.
5. Move props/buildings out of `SpriteRenderer` and into tilemap layers.
6. Keep preview as a temporary tile layer or a sprite only if the visual rule is
   shared with tile placement.
7. Add a Tiled-authored map once the adapter supports image collection atlases.
8. Verify that the same map can be opened in Tiled, loaded in SkyEngine, edited
   in game, and exported back.

## Implementation Phases

### Current Implementation Status

As of 2026-05-17, the first milestone is partially implemented:

- `sky_engine::tile` exports the initial format-neutral runtime types,
  including `TileMap`, `TilePalette`, `TileLayer`, `TileObject`,
  `GridSpec`, `TileMapEditSession`, `TileMapRenderSync`, and
  `TileMapInstance`.
- `src/tile/edit.rs` is only a thin facade. Edit responsibilities are split
  across `edit/summary.rs`, `edit/session.rs`, and `edit/history.rs` so the
  runtime edit path does not grow into a single scene/editor manager.
- The Tiled adapter can convert existing `TiledImport` data into `TileMap`
  and runtime palettes. Future migration should move callers directly onto the
  `TileMap` path instead of adding compatibility shims around the old Tiled
  runtime path.
- `TileMapInstance` no longer exposes Tiled-specific spawn shortcuts. The
  canonical runtime path is `TiledImporter` import plus `TileMapInstance::spawn`.
- `TileMapInstance` can spawn a scene through `TilemapStorage` /
  `TilemapRenderer`, refresh a full scene, incrementally rewrite changed
  layers, or apply `TileMapEditSummary` directly. Tile cell and rect edits
  update only affected cells when layer palette splits are unchanged; palette
  split changes fall back to a full rebuild.
- `TileMapEditSession` records changed layers and object changes, removes
  object IDs from object-layer membership, and mutates object properties in
  place. It also supports moving objects between layers/cells and changing an
  object's visual payload while recording render-sync-friendly object changes.
- `TileMapEditSummary` now carries old/new tile and property values, filters
  no-op tile/property sets, and keeps dirty cell/region data as the lightweight
  render-sync signal.
- Object create/remove changes carry full `TileObject` snapshots. Edit
  sessions can remove properties and can revert a `TileMapEditSummary`,
  producing a new summary that can be used for redo or render refresh.
- `TileMapEditHistory` provides an optional undo/redo stack over summaries,
  skips empty edits, clears redo on new edits, and returns the undo/redo
  summary so render sync can refresh from history operations.
- `miniature_builder_game` stores placed structures as `TileMap` objects and
  refreshes the scene instance from edit summaries instead of rebuilding every
  placement.
- The miniature builder structure atlas keeps native Kenney image dimensions
  and no longer writes wrapped atlas rows over the first row.
- The `TileMap` Tiled adapter now preserves multi-image image collection
  sources as runtime atlas metadata, and `TileMapInstance` builds the packed
  texture atlas during spawn so the high-level scene path matches the lower
  level `TiledImport` image collection behavior.
- A first TMJ export path exists as a focused `TiledExporter`, separate from the
  import-only `TiledImporter`, so import/export responsibilities do not grow into
  a single adapter god object. The exporter round-trips the currently supported
  finite scene subset (single-image palettes, image collection palettes, tile
  layers, tile object layers, and custom properties) through the normal import
  path. The lower-level JSON tileset importer now understands inline image
  collection tile entries, which keeps the TMJ export path format-native instead
  of relying on compatibility shims.
- `TileMapDocument` is now the preferred authoring/exchange container around
  a runtime `TileMap`. It groups the scene, palette store, and authoring
  metadata so the clean path becomes `TiledImporter::load_document` /
  `TiledImporter::import_document` -> edit the runtime scene -> `TiledExporter`
  document export/save, instead of making users manually thread scene and
  palette vectors through every API.
- `TileMapDocument::edit` wraps `TileMapEditSession` and always returns the
  finished `TileMapEditSummary`, giving apps a short safe edit path that is
  harder to misuse:
  `let summary = document.edit(|edit| edit.set_tile(...));`.
- `TileMapDocument` now owns a `TileMapEditHistory` and exposes
  `edit_recorded`, `undo`, `redo`, `clear_history`, `can_undo`, and `can_redo`.
  Document-level editing is now the ergonomic default while the lower-level
  edit session/history types remain available for custom workflows.
- `TileMapInstance` now has document-aware spawn/refresh helpers:
  `spawn_document`, `refresh_document`, `refresh_document_changed_layers`, and
  `refresh_document_edit_summary`. This keeps render synchronization owned by
  the render instance while letting apps pass the document as the natural unit
  after document edits, undo, or redo.
- `TileMapDocument` now has an engine-native JSON persistence path that is
  separate from TMJ authoring export. `src/tile/persistence.rs` owns the stable
  DTO layer for `TileMapSnapshot` and `TileMapDelta`, while the document
  exposes thin ergonomic methods such as `to_snapshot_json_string`,
  `from_snapshot_json_file`, `write_snapshot_json_file`, `edit_delta`, and
  `apply_delta`.
- Document revisions are explicit through `TileDocumentRevision`. New
  documents start at revision `0`; non-empty recorded edits, undo, redo,
  `edit_delta`, and successful non-empty delta applications advance the
  revision. Empty edits and no-op deltas leave the revision unchanged.
- Snapshot persistence stores the full runtime document state: scene grid,
  size, layers, tiles, objects, custom properties, palette store, authoring
  metadata, and revision. It intentionally does not store edit history.
- Delta persistence stores `base_scene`, `base_revision`, and a serialized
  `TileMapEditSummary`, including tile/object/property changes plus dirty
  cell/region/layer render-sync hints. Applying a delta reuses
  `TileMapEditSession`, so persistence does not own a second mutation engine.
- Runtime GPU texture handles are deliberately rejected during snapshot export:
  `TileTextureSource::Texture { .. }` returns
  `TilePersistenceError::RuntimeTextureSource`; persistable sources are
  `None`, single image paths, and image collection atlas source paths.

### Phase 0: Document Current Behavior

- Add tests or notes around current `TilemapRenderer` coordinate semantics:
  - `tile_size` is logical stride.
  - `tile_draw_size` is rendered image size.
  - `tile_offset` is image offset relative to logical cell origin.
- Add a small Kenney/Tiled fixture if licensing allows local test assets, or a
  synthetic equivalent:
  - map tile `256x128`
  - image tile `256x512`
  - full transparent canvas

### Phase 1: Palette and Image Collection Support

- Extend runtime atlas building for image collection tilesets.
- Support multiple PNGs in one Tiled image collection tileset by packing them
  into a runtime atlas.
- Preserve per-image source dimensions as tile draw sizes.
- Avoid transparent trimming by default.
- Add tests for:
  - multi-image image collection TSX
  - per-tile draw size
  - large tile draw over a smaller cell

### Phase 2: TileMap Core Types

- Add format-neutral `TileMap`, `TilePalette`, `TileLayer`, `TileObject`,
  and `GridSpec` types.
- Keep this layer free of Tiled-specific names like gid/firstgid.
- Add conversions from `TileMap` to `TilemapStorage` render data.
- Do not remove existing `TilemapStorage` APIs.

### Phase 3: Tiled Adapter to TileMap

- Refactor `TiledImport` internals to optionally produce `TileMap`.
- Migrate call sites to the `TileMap` path directly; avoid new
  compatibility wrappers for the old Tiled runtime path.
- Add a new path:

```rust
let scene = TiledImporter::load_scene(path)?;
let runtime = TileMapInstance::spawn(world, scene, options)?;
```

- Preserve Tiled metadata for later export.

### Phase 4: Runtime Edit Session

- Add `TileMapEditSession`.
- Track dirty regions and changed objects.
- Sync edits into `TilemapStorage`.
- Add undo/redo-friendly command records later if needed.

### Phase 5: Miniature Builder Refactor

- Replace sprite-based placed objects with `TileMap` objects or tile layer
  writes.
- Use layer roles instead of hard-coded numeric sorting.
- Make build placement mutate `TileMapEditSession`.
- Keep game scoring/occupancy separate from rendering.

### Phase 6: Export and Round Trip

- Implement TMJ export first.
- Export changed tile layers and object layers.
- Preserve layer names, layer order, custom properties, and tileset references
  when possible.
- Add fixture tests:

```text
Tiled fixture -> TileMap -> export TMJ -> re-import -> equivalent TileMap
```

### Phase 7: Broader Tile Types

- Add support for:
  - animated tile editing
  - collision layer extraction
  - object prototypes
  - terrain/autotile metadata
  - runtime-generated palettes
  - stricter/conflict-aware save delta application options

## Testing Strategy

Unit tests:

- grid coordinate conversions
- layer role lookup
- palette tile lookup
- tile draw size / offset mapping
- dirty region tracking
- Tiled gid flag conversion
- import/export equivalence for small fixtures

Render tests:

- large tile over small logical cell
- isometric draw order
- layer order with overhanging tiles
- per-tile draw size from atlas rects

Example checks:

- `cargo check --examples --features app`
- `cargo run --example miniature_builder_game --features app`
- screenshot probes for miniature builder native Kenney dimensions

Tiled fixture tests:

- official or synthetic orthogonal fixture
- official or synthetic isometric fixture
- image collection fixture
- object layer fixture
- properties fixture

## Risks

1. Scope creep
   - Keep `TileMap` minimal first. Do not implement every Tiled feature before
     runtime editing works.

2. Overfitting to Tiled
   - Keep Tiled concepts in the adapter. Runtime types use neutral names.

3. Breaking existing low-level API
   - Preserve `TilemapStorage`, `TilemapRenderer`, and current manual examples.

4. Sorting regressions
   - Add visual tests for isometric large tiles before changing extraction.

5. Atlas packing complexity
   - Start with simple row/column packing. Optimize later.

6. Export fidelity
   - Document what round trips exactly and what is best-effort.

## Open Questions

1. Should `TileMap` live under `sky_engine::tile` or
   `sky_engine::render::tilemap::scene`?

2. Should object visuals default to tilemap rendering, sprite rendering, or a
   policy selected by layer role?

3. Should `TileMap` own `TilemapStorage`, or should render sync create
   `TilemapStorage` as a separate backend cache?

4. How much adapter metadata should be retained for export?

5. Should save deltas be a core feature in Phase 4 or deferred until after TMJ
   export?

6. Should tile collision be stored per tile definition, per collision layer, or
   both?

## First Milestone

The first useful milestone should be small and visible:

1. Add image collection runtime atlas support for Tiled/Kenney-style tilesets.
2. Add a synthetic isometric large-tile fixture.
3. Add `TileMap` core types behind an internal module.
4. Convert the miniature builder object rendering from `SpriteRenderer` to
   tilemap layer writes.
5. Preserve native Kenney dimensions:
   - logical cell `256x128`
   - image tile `256x512`
   - no transparent trimming

Success criteria:

- The miniature builder renders ground and objects through shared tilemap
  placement rules.
- Object placement mutates scene/layer data, not sprite entities.
- A Tiled-style large tile fixture renders correctly.
- Existing manual `TilemapStorage` examples still compile.
