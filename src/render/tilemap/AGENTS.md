# AGENTS.md — `src/render/tilemap`

## Overview
- This module owns SkyEngine's chunked 2D tilemap renderer and the Tiled import bridge.
- Keep tilemap-specific logic here. Do not add Tiled concepts to generic render runtime, phase sorting, sprite, mesh, view, or execution modules unless the concept is truly shared.
- The high-level public path is:
  - `TilemapFeature` — registers tilemap extraction and drawing.
  - `TilemapStorage` / `TilemapHandle` — owns large map data outside ECS component storage.
  - `TilemapRenderer` — ECS-facing component in `src/render/component/tilemap.rs`.
  - `TiledImport` — parses `.tmx`, `.tmj`, and `.json` into engine tilemap data.
  - `TiledMapInstance` — spawns/despawns one imported Tiled map in a `World`.
- Rendering is transparent-phase, texture-atlas based, and uses instanced quads.

## File Map
- `mod.rs` — module exports and tilemap internal wiring.
- `storage.rs` — `Tile`, `TileId`, `TileFlags`, `Tilemap`, `TilemapStorage`, handles, chunks, and dirty versions.
- `tiled.rs` — Tiled parsing/import, tileset metadata, layer/object decoding, orientation conversion.
- `instance.rs` — `TiledMapInstance`, spawn/despawn lifecycle, tile object spawning, parallax synchronization.
- `feature.rs` — `TilemapFeature` registration.
- `extract.rs` — ECS extraction, visibility culling, tile-to-instance conversion, sort order, frame-cache population.
- `cache.rs` — per-frame and retained GPU instance buffers for chunks/batches.
- `draw.rs` — tilemap draw function, WGSL pipeline, bind groups, phase payloads.
- `../component/tilemap.rs` — ECS-facing tilemap authoring types such as `TilemapRenderer` and `TilesetGrid`.
- `../shaders/tilemap/tilemap_draw.wgsl` — tilemap draw shader.

## Architecture

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

## Data Model
- `TilemapStorage` is a `World` resource. Large tile data belongs there, not inside ECS components.
- `TilemapHandle` is a generation-checked handle into `TilemapStorage`.
- `Tilemap` is layered and chunked. Chunks track:
  - dirty `version`
  - non-empty tile count
- `TilemapRenderer` selects one map layer and describes how to draw it:
  - tileset texture/grid
  - logical tile size
  - draw size and tile offset
  - orientation, stagger/hex settings, render order, depth sort
  - visibility, color, layer mask
- Prefer one `TilemapRenderer` entity per visual tile layer.

## Tiled Import
- `TiledImport` is parsing/conversion only. It should not mutate a `World`.
- `TiledMapInstance` owns runtime spawning:
  - loads tileset texture into `AssetServer`
  - inserts map data into `TilemapStorage`
  - spawns tile layer entities
  - spawns tile objects as sprites
  - records entities/resources for despawn
  - syncs Tiled parallax when the app provides camera position
- `TiledMapInstance` is not a scene graph. It is a loaded-map handle.
- Keep `TiledSpawnOptions` small and focused on spawn policy.

## Rendering And Batching
- Ordinary tilemap layers are extracted into per-view/per-layer GPU instance batches.
- `TilemapDepthSort::YThenLayer` keeps finer-grained draw payloads to preserve overhanging wall/foreground correctness.
- Do not sacrifice Tiled visual correctness for larger batches. Sorting correctness comes first.
- Batch keys should stay compatible with the existing transparent phase grouping.
- Texture changes are a batch boundary. Mixed tileset textures in one draw batch should be rejected or split.
- GPU caches live in `cache.rs`; extraction should populate them, draw should only consume prepared payloads.

## Coordinate And Sorting Rules
- Tiled uses top-left-ish map data conventions; SkyEngine rendering uses the engine's world-space conventions.
- Import code is responsible for converting Tiled coordinates into engine tile coordinates.
- `TilemapRenderer::cell_to_local_origin` and `cell_to_local_center` are the source of truth for tile placement.
- `tile_size` is logical cell stride.
- `tile_draw_size` is the rendered image size.
- `tile_offset` is the image offset relative to the logical cell.
- Be careful with isometric/staggered/hexagonal parity. Tests cover official Tiled samples; extend them when fixing layout bugs.

## Boundaries
- Keep generic render-phase and draw contexts generic. Do not add tilemap-specific fields to shared execution state.
- Do not push parallax into global camera/view logic. Tiled parallax currently belongs to `TiledMapInstance::sync_parallax`.
- Do not make sprites understand tilemaps. Tile object rendering may use `SpriteRenderer`, but the conversion lives here.
- Do not turn `GpuScene` into a tilemap cache. Tilemap-specific GPU state belongs in `TilemapFrameCache`.
- Do not use `TiledMapInstance` as a general app scene system.

## Public API Expectations
- Normal app code should be able to load a map with:

```rust
let map = TiledMapInstance::spawn(world, path, TiledSpawnOptions::centered())?;
```

- Keep this path simple. Demos should not manually assemble layer entities, storage handles, texture handles, and parallax metadata unless they are demonstrating low-level APIs.
- Manual tilemap construction should continue to use:

```rust
let mut storage = TilemapStorage::new();
let map = storage.create(TilemapDescriptor::new(width, height, layers));
storage.get_mut(map).unwrap().set_tile(layer, x, y, tile);
world.insert_resource(storage);
world.spawn((Transform::default(), TilemapRenderer::new(map, tileset), SortingLayer(0)));
```

## Supported Tiled Surface
- Currently supported:
  - `.tmx`
  - `.tmj` / `.json`
  - orthogonal, isometric, staggered, and hexagonal orientations
  - finite layers and normalized infinite-map chunks
  - group layer inheritance for visibility/opacity/offset/parallax
  - CSV and base64 tile data
  - zlib and gzip compression
  - external `.tsx` / `.tsj` tilesets
  - single-image tilesets
  - limited single-image image collection tilesets
  - tile offset, margin, spacing, transparent color, animation
  - tile flip flags in tile layers
  - TMX tile objects as sprites
- Known limits:
  - multiple used tilesets in tile layers are rejected
  - image collection tilesets using multiple images are rejected
  - non-tile object shapes/text/polygons are not a complete rendering path
  - JSON object layers are not as complete as TMX object groups
  - infinite maps are imported into a bounded tilemap rather than streamed

## Tests And Validation
- Tilemap-focused tests:

```bash
cargo test --features app render::tilemap
```

- Import-only tests:

```bash
cargo test --features app render::tilemap::tiled
```

- Instance lifecycle tests:

```bash
cargo test --features app render::tilemap::instance
```

- Example compatibility after public API or render pipeline changes:

```bash
cargo check --examples --features app
```

- Useful demo runs:

```bash
cargo run --example tiled_browser_demo --features app
cargo run --example tiled_browser_demo --features app -- examples/assets/tiled/tiled/examples/forest/forest.tmx
cargo run --example tiled_import_demo --features app -- examples/assets/tiled/sewers.tmx
```

## Review Checklist
- Does the change stay within tilemap-specific modules unless a generic concept is truly needed?
- Are Tiled coordinate conversions tested with official sample maps?
- Does `perspective_walls.tmx` or any overhanging wall map still sort correctly?
- Does ordinary layer batching still preserve visible cell order?
- Are animated tiles invalidating the GPU cache when their frame changes?
- Are despawn paths removing spawned entities, storage handles, and runtime textures?
- Did `cargo fmt --check`, relevant tilemap tests, and example checks pass?
