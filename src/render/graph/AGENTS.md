# AGENTS.md — `src/render/graph`

## Overview
- This module is a declarative render-graph system for GPU workload orchestration on top of `wgpu`.
- Passes declare virtual resource reads/writes through builder closures; the graph automatically resolves dependencies, culls dead passes, tracks resource lifetimes, pools transient GPU resources, and executes passes in topological order.
- Inspired by SakuraEngine's `SkrRenderGraph`.  Implements single-queue execution with execution reordering and memory aliasing optimizations.
- The GPU backend is `wgpu` (WebGPU/Vulkan/DX12/Metal); all physical resources are `wgpu::Texture` / `wgpu::Buffer` / `RenderTarget`.

## File Map
- `mod.rs`: core `RenderGraph` struct definition, resource creation, pass registration, reset/destroy, query accessors, and handle validation helpers.
- `compile.rs`: compilation pipeline (`compile()` method) — dependency analysis, topological sort, dead-pass culling, execution reordering, and resource lifetime analysis.  Memory alias analysis is deferred to `allocate.rs`.
- `allocate.rs`: physical resource lifecycle — `allocate_physical_resources()` (with deferred alias computation using real surface dimensions), `release_transient_resources()`, resolve helpers (`try_resolve_texture`, `try_resolve_buffer`, `resolve_texture_extent`), buffer usage inference, and public physical resource accessors.
- `reorder.rs`: execution reordering for cache locality and lifetime compression.  Adapted from SakuraEngine's `ExecutionReorderPhase`.  Single forward-pass algorithm with BFS-based DAG safety checks and Jaccard resource affinity scoring.  Configurable via `ReorderConfig` (max attraction distance, min affinity score).
- `alias.rs`: memory aliasing for transient textures.  Adapted from SakuraEngine's `MemoryAliasingPhase`.  Bucket-based best-fit algorithm where transient textures with non-overlapping lifetimes and matching formats share physical `RenderTarget` objects.  Exports `AliasingStats` for compression ratio monitoring.
- `types.rs`: all public and internal type definitions — `TextureHandle`, `BufferHandle`, `PassHandle`, `ResourceRef`, `TargetSize`, `PassType`, `PassFlags` (bitflags), `CopyOp`, `LoadOp`, `ColorOutput`, `DepthStencilOutput`, `TextureDesc`, `BufferDesc`, `ImportedTexture`, `PassEntry`, `CompiledPass`, `ResourceLifetime`, `PhysicalResources`, `PhysicalTextureRef`, and helpers (`resolve_target_size`, `texture_format_bytes_per_pixel`).
- `builder.rs`: builder types for graph construction — `TextureBuilder` (name, size, format, transient/persistent, import), `BufferBuilder` (name, size, usage, transient/persistent, import), `PassSetup` (read/write/readwrite for textures and buffers, MRT color outputs with load ops, depth-stencil, surface writes, flags), `CopyPassSetup` (texture-to-texture, buffer-to-buffer, buffer-to-texture, upload-to-texture).
- `error.rs`: `RenderGraphError` enum (including dependency, handle, copy/upload validation, allocation, and execution failures), `RenderGraphProfiler` trait, and `DebugProfiler`.
- `pool.rs`: transient resource pools — `TransientPool` (keyed on `{format, width, height}` for `RenderTarget` recycling) and `TransientBufferPool` (keyed on `{size_bytes, usage}` for `wgpu::Buffer` recycling). Both use `FxHashMap<Key, Vec<T>>` stacks.
- `visualize.rs`: GraphViz DOT export with resource/pass nodes and read/write edges, coloured by pass type.
- `tests.rs`: render graph regression tests covering compilation, dependency analysis, dead-pass culling, reordering, aliasing, physical resource allocation/release, builder APIs, MRT, depth-stencil, copy ops, import, execution errors, and PhysicalResources resolution.

## Handle Model
- Handles are `(usize, u64)` tuples: index into the resource/pass array + a per-graph `handle_token`.
- `handle_token` is a process-wide monotonic counter (`AtomicU64`), changed on `reset()` to invalidate stale handles.
- `ResourceRef` is the unified reference type: `Surface`, `Texture(TextureHandle)`, or `Buffer(BufferHandle)`.
- Handle validity is always checked before use; stale/foreign handles produce `InvalidResourceHandle` errors.

## Compilation Pipeline
The graph compiles in four phases (in `compile()`), with alias analysis deferred to `allocate_physical_resources()`:

1. **Dependency analysis**: scan all passes in declaration order. For each read, edge from last writer → this pass. For each write, edge from last writer and all prior readers → this pass. Detect `ReadBeforeWrite` for resources that are neither externally provided nor persistent across frames. Uses Kahn's algorithm with index-sorted ready queue for deterministic topological order. Detects cycles.
2. **Dead-pass culling**: backward-propagation from passes that write to externally-visible sinks (`Surface`, imported resources, or persistent graph-owned resources). Passes with no path to a sink are marked dead and excluded from execution.
3. **Execution reordering** (SakuraEngine-inspired): reshuffles alive passes within valid topological orderings to maximize resource affinity between adjacent passes.  Single forward-pass with bounded look-ahead (`max_attraction_distance=10`), BFS DAG safety checks, and Jaccard similarity scoring.  Dependency edges are preserved by `reorder.rs`.
4. **Resource lifetime analysis**: for each non-culled pass in (reordered) execution order, track `first_use`/`last_use` per resource. Used for both pool reclamation and alias analysis.

**Deferred to `allocate_physical_resources()`:**
5. **Memory alias analysis** (SakuraEngine-inspired): identifies transient textures with non-overlapping lifetimes and matching formats that can share the same physical `RenderTarget`.  Uses a best-fit bucket strategy sorted by pixel count descending.  Deferred from `compile()` to `allocate_physical_resources()` so that real surface dimensions are available for accurate waste calculations.  Results stored as `AliasGroup`s; statistics available via `alias_stats()`.

Compilation is **idempotent** and cached; the cache is invalidated when passes or resources are added.

## Physical Resource Management
- **Alias groups**: transient textures in the same alias group share physical `RenderTarget` objects.  Each group allocates one target with `max(width) × max(height)` dimensions.  `alias_group_count()` only counts multi-member groups (groups with actual sharing).  Non-aliased transient textures use the normal pool path.
- **Transient textures**: allocated from `TransientPool` on first use, returned after frame execution via `release_transient_resources()`. Pool key is `{format, width, height, sample_count, mip_level_count}`.
- Virtual texture descriptors also track `sample_count` and `mip_level_count`; pooling and aliasing require those values to match exactly.
- **Persistent textures**: owned by the graph, resized on surface size changes, not pooled, and treated as cross-frame external sources/sinks by the compiler.
- **Imported textures**: external `Arc<wgpu::Texture>` + `Arc<wgpu::TextureView>`, never pooled or resized.  Imported resources are treated as both external sources and external sinks (intentional — the caller retains a reference and observes writes).
- **Transient buffers**: allocated from `TransientBufferPool`, keyed by `{size_bytes, usage}`.
- **Imported buffers**: external `Arc<wgpu::Buffer>`, never pooled.
- Buffer usage flags are augmented after compilation based on actual usage in alive passes.  `buffer_usage_for()` has a `debug_assert!(self.compiled)` guard to catch misuse.

## Execution Model
- `try_execute(ctx, run_pass)`: compile → allocate → iterate compiled passes → for each `Copy` pass run `execute_copy_pass()` internally, for each `Render`/`Compute` pass call user's `run_pass` closure with `(&CompiledPass, &mut GpuContext, &PhysicalResources) -> Result<(), RenderGraphError>` → release transients.
- `execute_profiled(ctx, profiler, run_pass)`: same but with `RenderGraphProfiler` callbacks around each pass.
- Copy passes flush any active frame encoder before running. Copy ops inside one copy pass are batched into a shared command encoder when possible; `UploadToTexture` forces a submit boundary because it uses `queue.write_texture()`.
- The user closure receives `PhysicalResources` for resolving virtual handles to `&RenderTarget`, `&wgpu::TextureView`, or `&wgpu::Buffer`.

## Pass Types
- **Render**: rasterisation draw calls. Supports MRT `color_outputs` (with `LoadOp::Clear`/`Load`/`DontCare`) and `depth_stencil` (with clear depth/stencil and store flags).
- **Compute**: compute shader dispatches. Same read/write/readwrite builder API.
- **Copy**: explicit GPU copy operations (`TextureToTexture`, `BufferToBuffer`, `BufferToTexture`, `UploadToTexture`). The graph validates copy compatibility up front rather than silently truncating mismatched copies.

## PassFlags (Scheduling Hints)
- `PREFER_ASYNC_COMPUTE` (0x02): hint for future multi-queue support.
- `COMPUTE_INTENSIVE` (0x10), `VERTEX_BOUND_INTENSIVE` (0x20), `PIXEL_BOUND_INTENSIVE` (0x40), `BANDWIDTH_INTENSIVE` (0x80): workload characterization hints.
- Currently informational only; no multi-queue scheduler is implemented.

## Blackboard
- `RenderGraph` owns a `Blackboard` instance for cross-pass data sharing.
- `graph.blackboard().set("key", value)` / `graph.blackboard_ref().get::<T>("key")`.
- Cleared on `reset()`.

## Visualization
- `export_dot()` produces a GraphViz DOT string with resource nodes (textures, buffers, surface), pass nodes (colored by type, dashed if culled), and read/write edges.

## Borrow Pattern in Execution
- During `try_execute`, `PhysicalResources` holds shared (`&`) borrows of `physical_textures`, `physical_buffers`, `textures`, `buffers`, and `alias_redirects`.
- `execute_copy_pass()` is called within this borrow scope, so it **must** remain `&self` (not `&mut self`).  If it ever needs mutation, the execution must be restructured (e.g. clone the compiled pass list or split the struct).

## Implementation Guidelines
- All resource handle validation must use the `handle_token` mechanism; never index into `textures`/`buffers` without checking the token first.
- `compile()` is the single source of truth for execution order, alive state, and dependency levels. Never manually manipulate `order` or `cached_compiled`.
- `compile()` must remain idempotent — repeated calls return the cached result. Adding passes/resources invalidates the cache via `self.compiled = false`.
- `buffer_usage_for()` must only be called after `compile()` (enforced by `debug_assert`).  Pre-compilation calls would miss COPY_SRC/COPY_DST flags inferred from copy passes.
- Copy passes must flush the current frame encoder before submitting their own command buffers to preserve global GPU submission order.
- All new `CopyOp` variants must register proper reads/writes in `CopyPassSetup` to participate in dependency analysis.
- `queue.write_texture()` (used by `UploadToTexture`) does NOT require 256-byte `bytes_per_row` alignment; only `encoder.copy_buffer_to_texture()` does.
- `write_color()` defaults to `LoadOp::DontCare`, not `LoadOp::Load`. Use `write_color_loaded()` when preserving prior contents is intentional.
- `set_depth_stencil()` defaults to clearing depth to `1.0`. Use `set_depth_stencil_loaded()` when preserving prior depth contents is intentional.
- `TextureToTexture` copies require matching format and extent; `BufferToBuffer` copies require matching declared sizes; `BufferToTexture`/`UploadToTexture` are validated for layout and byte counts before dispatch.
- If changing dependency analysis, run the full test suite — the current tests cover linear chains, diamond dependencies, dead-pass culling, transitive culling, stale handle rejection, foreign handle rejection, read-before-write detection, cycle detection, duplicate name handling, MRT slots, depth-stencil, pass flags, readwrite deduplication, buffer dependencies, physical allocation, alias sharing, copy ops, execution error propagation, PhysicalResources resolution, and more.
- Transient pool keys must be kept small and cheap to hash (`FxHashMap`).
- Do not add heavyweight per-frame allocations to the compilation pipeline; the graph may be compiled once and executed many frames.

## Test Commands
- Run all render graph tests: `cargo test --features app graph`
- Run a specific test: `cargo test --features app render::graph::tests::linear_chain_orders_correctly`
- Run reorder tests only: `cargo test --features app reorder::tests`
- Run alias tests only: `cargo test --features app alias::tests`
- Tests that need a GPU device use `create_test_device()` and require a GPU-capable environment.

## Current Limitations and Future Work
- **Single-queue only**: no multi-queue scheduling, no async compute dispatch. `PassFlags::PREFER_ASYNC_COMPUTE` is recorded but unused.
- **No barrier generation**: wgpu handles resource state transitions internally; no explicit barrier phase (by design — unnecessary under wgpu).
- **No bind-table management**: descriptor set / bind group creation is left to the caller.
- **Texture-only aliasing**: buffer aliasing is not implemented (low impact since buffers are typically few and small).
- **2D physical targets**: render-graph-managed physical textures are still backed by `RenderTarget` and therefore assume 2D attachments, even though sample/mip metadata is now tracked explicitly.
- These remaining limitations are tracked against the SakuraEngine reference implementation's 12-phase pipeline (see `SakuraEngine_ref/engine/modules/render/render_graph/claude.md`).

## Relation to Broader Render Module
- `RenderGraph` is re-exported from `src/render/mod.rs` and is the primary entry point for frame orchestration.
- Physical textures are backed by `RenderTarget` (`src/render/target.rs`).
- Higher-level passes (`LightPass`, `CompositePass`, `PostFx`) build on top of `RenderGraph` by calling `add_render_pass`/`add_compute_pass` and resolving physical resources from `PhysicalResources`.
- The `Blackboard` shared data system lives at `src/render/blackboard.rs`.
