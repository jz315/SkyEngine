# Render God Object Refactor Plan

## Purpose

This plan reduces god-object and god-module pressure in SkyEngine's render
runtime without changing the public render architecture.

The goal is not a broad rewrite. The goal is to make the current
registration-driven renderer easier to evolve by moving responsibilities out of
large orchestration points and into smaller internal helpers.

The canonical architecture remains:

```text
RenderPipelineAsset
  -> RenderComposer
  -> PreparedFrame / PreparedView
  -> FramePipeline
  -> RenderGraph / wgpu execution
```

Keep the existing high-level surface stable unless a later task explicitly
requests a breaking render API rewrite.

## Current Diagnosis

The render module has several large files, but not every large file is equally
dangerous. The main risk is concentrated where one object or module knows too
many domains at once.

### Highest Risk: RenderComposer Frame Flow

Current root:

```text
src/render/runtime/composer.rs
src/render/runtime/frame_builder.rs
```

`RenderComposer` itself is small, but `RenderComposer::render_world` in
`frame_builder.rs` currently coordinates almost the entire frame:

- material registration and dirty preparation
- built-in mesh initialization
- phase, shadow, and GI runtime initialization
- render settings and render asset frame state
- transform resolution
- view collection and ordering
- feature extraction and preparation
- extractor execution
- opaque and transparent phase construction
- model matrix assignment
- previous-frame model matrix tracking
- light collection and GPU table upload
- GI renderable collection and provider preparation
- directional shadow view synchronization
- render pipeline construction
- `PreparedFrame` and `PreparedView` assembly
- frame execution
- render statistics and timing output

This makes `RenderComposer` more than an orchestrator. It becomes the place that
understands ECS extraction, assets, materials, lights, GI, shadows, frame
payloads, phase payloads, execution, and stats.

### High Risk: runtime/nodes.rs

Current root:

```text
src/render/runtime/nodes.rs
```

This file owns runtime-installed execution nodes for:

- scene color seeding
- phase setup and execution
- built-in opaque and transparent phase behavior
- compute pass setup and execution
- graph pass setup and execution
- post-fx setup and execution
- finalize render pass setup and execution
- headless keep-alive behavior

It also holds raw pointers to mutable renderer state:

```rust
*mut dyn RenderPhase
*mut DrawFunctionRegistry
*mut MaterialRegistry
*const MeshRegistry
*const Texture
```

The current model may be workable, but the unsafe lifetime boundary is spread
through a large mixed-responsibility file. Future changes to the execution
pipeline should not need to reason through all node families at once.

### High Risk: pipeline/contexts.rs

Current root:

```text
src/render/pipeline/contexts.rs
```

The file defines many public setup and execute contexts:

- `ComputePassSetupContext`
- `ComputePassExecuteContext`
- `GraphPassSetupContext`
- `GraphPassExecuteContext`
- `PostFxPassSetupContext`
- `PostFxPassExecuteContext`
- `RenderPassSetupContext`
- `RenderPassExecuteContext`
- `RenderPhaseSetupContext`
- `RenderPhaseExecuteContext`

Most of these contexts expose overlapping capabilities:

- frame and view payload access
- blackboard access
- scene texture access
- history texture requests
- scene lighting access
- scene shadow access
- graph/pass/resource access

The risk is duplication. Adding one shared frame capability tends to require
editing several context types.

### Medium Risk: pipeline/builtins.rs

Current root:

```text
src/render/pipeline/builtins.rs
```

This is mostly a god-module issue. The file mixes several unrelated built-in
pipeline step families:

- scene normal prepass
- scene material prepass
- GI update and composite wrappers
- contact shadows
- bloom
- tone mapping
- temporal anti-aliasing
- debug view
- sharpening
- vignette

This is a good first refactor because it can be split with very little behavior
risk.

### Medium Risk: phase/draw.rs

Current root:

```text
src/render/phase/draw.rs
```

This file combines:

- draw function trait and registry
- draw errors
- shared draw contexts
- standalone draw context
- mesh draw batching
- mesh material binding and pipeline selection
- scene prepass draw path
- sprite draw runtime and pipeline cache
- sprite batching and draw execution

This is performance-sensitive and should be split after safer runtime and
module-boundary work is complete.

### Medium Risk: MaterialRegistry

Current root:

```text
src/render/resources/material/registry.rs
```

`MaterialRegistry` currently owns:

- material model registration
- material instance storage
- generation-checked handles
- dirty instance tracking
- material preparation
- prepared material lookup
- pipeline cache coordination
- debug summaries

This is still within the material domain, so it is less urgent than
`RenderComposer::render_world`. It should remain the public facade while
internal responsibilities are gradually extracted.

### Lower Priority: tilemap/tiled.rs

Current root:

```text
src/render/tilemap/tiled.rs
```

This file is large, but its responsibilities are mostly confined to the Tiled
import domain:

- TMX parsing
- JSON/TMJ parsing
- tileset metadata
- layer and object decoding
- compressed tile data
- coordinate conversion
- imported map to renderer conversion
- import tests

It should be split eventually, but it is not a core render-runtime god object.

## Refactor Rules

- Preserve the registration-driven render model.
- Preserve public app-facing render APIs unless explicitly approved later.
- Prefer private helper modules over broad trait redesign.
- Keep `RenderComposer` as the public facade and orchestrator.
- Do not push renderer-family-specific state into generic execution contexts.
- Do not turn `GpuScene` into a universal cache for every renderer.
- Do not mix this cleanup with shader rewrites, benchmark cleanup, or unrelated
  API changes.
- Keep each phase compiling before moving to the next phase.
- Treat the current dirty worktree as user-owned. Only edit files needed for
  this plan when implementing it.

## Target Shape

The desired end state is:

- `RenderComposer::render_world` reads like a high-level frame recipe.
- Frame preparation, phase extraction, scene upload, GI preparation, shadow
  preparation, prepared-frame assembly, and stats finalization live in focused
  internal helpers.
- `runtime/nodes.rs` is split by node family.
- Unsafe raw-pointer access in runtime step nodes is isolated behind a small
  internal reference wrapper with documented invariants.
- `pipeline/contexts.rs` uses shared internal context cores to reduce repeated
  methods while preserving the public context names.
- `pipeline/builtins.rs` becomes a module tree of built-in pass families.
- `phase/draw.rs` separates trait/registry, contexts, mesh draw, sprite draw,
  scene bindings, and batching helpers.
- `MaterialRegistry` remains the material facade, but debug and preparation
  helper logic no longer inflate the core registry file.

## Phase 0: Baseline And Safety

### Tasks

1. Capture the current state before refactoring:

   ```bash
   git status --short
   ```

2. Identify which existing changes are user-owned and avoid touching them unless
   they are directly part of the refactor.

3. Use small, mechanical commits or review slices when possible:

   - one slice for `builtins`
   - one slice for frame flow extraction
   - one slice for node splitting
   - one slice for context-core extraction
   - one slice for draw splitting

4. Prefer mechanical moves before behavior edits.

### Validation

At minimum:

```bash
cargo fmt --check
cargo test --features app render::runtime
cargo test --features app render::pipeline
cargo test --features app render::phase
cargo check --examples --features app
```

If a phase only moves files and imports, run the smallest relevant test first,
then run the full render example check before considering the phase complete.

## Phase 1: Split pipeline/builtins.rs

### Intent

Reduce a large mixed built-in pass file into a module tree. This is the safest
first step because it should preserve behavior and public exports.

### Proposed Structure

```text
src/render/pipeline/builtins/
├── mod.rs
├── prepass.rs
├── gi.rs
├── shadows.rs
├── postfx.rs
└── debug.rs
```

### Ownership

Move code as follows:

```text
SceneNormalPrepass        -> prepass.rs
SceneMaterialPrepass      -> prepass.rs
GiUpdateCompute           -> gi.rs
GiCompositePass           -> gi.rs
ContactShadows            -> shadows.rs
Bloom                     -> postfx.rs
ToneMap                   -> postfx.rs
TemporalAntiAliasing      -> postfx.rs
Sharpen                   -> postfx.rs
Vignette                  -> postfx.rs
DebugView                 -> debug.rs
DebugViewSource           -> debug.rs
shared helpers            -> private modules or mod.rs, depending on scope
```

### Steps

1. Create the `builtins/` directory.
2. Move one family at a time.
3. Keep exports stable from `src/render/pipeline/mod.rs`.
4. Keep user-facing names unchanged.
5. Do not change pass order in `RenderPipelineAsset` constructors.
6. Run formatting after imports settle.

### Public Compatibility

Existing code should continue to compile:

```rust
use sky_engine::render::{Bloom, ToneMap, Vignette};
use sky_engine::render::pipeline::builtins::SceneNormalPrepass;
```

If old imports point directly at `crate::render::pipeline::builtins`, preserve
them with `pub use`.

### Validation

```bash
cargo fmt
cargo test --features app render::pipeline
cargo check --examples --features app
```

## Phase 2: Extract RenderComposer Frame Flow

### Intent

Make `RenderComposer::render_world` an orchestration method instead of the home
of all frame logic.

### Proposed Modules

```text
src/render/runtime/
├── frame_builder.rs
├── frame_prepare.rs
├── phase_extract.rs
├── scene_upload.rs
├── gi_prepare.rs
├── shadow_prepare.rs
└── frame_assemble.rs
```

The exact filenames can change if local patterns suggest better names. Keep the
modules private to `runtime`.

### Proposed Data Types

```rust
pub(crate) struct FramePrelude<'a> {
    pub(crate) asset_server: Option<crate::asset::AssetServer>,
    pub(crate) render_assets:
        Option<&'a crate::render::resources::assets::SharedRenderAssetCache>,
    pub(crate) resolved_transforms:
        crate::render::view::ResolvedSceneTransforms,
}
```

```rust
pub(crate) struct ExtractedFramePhases {
    pub(crate) views: Vec<crate::render::view::SceneView>,
    pub(crate) opaque: Vec<crate::render::phase::OpaquePhase>,
    pub(crate) transparent: Vec<crate::render::phase::TransparentPhase>,
}
```

```rust
pub(crate) struct UploadedSceneData {
    pub(crate) lights: Vec<crate::render::GpuLight>,
    pub(crate) model_matrices: Vec<[f32; 16]>,
    pub(crate) previous_model_matrices: PreviousModelMatrices,
}
```

```rust
pub(crate) struct ShadowFrameSummary {
    pub(crate) debug_resources:
        Option<crate::render::lighting::shadow::ShadowDebugResources>,
    pub(crate) stats: FrameShadowStats,
    pub(crate) draw_calls: usize,
    pub(crate) draw_calls_by_cascade:
        [usize; crate::render::component::MAX_DIRECTIONAL_SHADOW_CASCADES],
}
```

These types should remain crate-private or module-private. They are internal
bookkeeping, not API.

### Target render_world Shape

The target method should read close to this:

```rust
pub fn render_world(&mut self, gpu: &mut GpuContext, world: &World) {
    self.ensure_frame_runtime(gpu, world);

    let frame_start = timing_start();
    let prelude = self.prepare_frame_inputs(gpu, world);
    let phases = self.extract_frame_phases(gpu, world, &prelude);
    let uploads = self.upload_scene_data(gpu, world, &prelude, &phases);
    self.prepare_gi(gpu, &phases, &uploads);
    let shadows = self.prepare_shadows(gpu, world, &phases, &uploads);
    let execution = self.execute_prepared_frame(gpu, &phases, &uploads, &shadows);

    self.finish_frame_stats(world, frame_start, &phases, &uploads, &shadows, execution);
}
```

This is illustrative, not a required exact signature. Borrowing constraints may
require slightly different ownership boundaries.

### Extraction Boundaries

#### frame_prepare.rs

Own:

- `ensure_registered_materials`
- `ensure_builtin_meshes`
- `ensure_phase_runtime`
- `ensure_shadow_runtime`
- `ensure_gi_runtime`
- material pipeline cache frame begin
- render settings fetch
- render asset cache frame begin
- asset event dispatch
- sprite material frame clear

Do not own:

- view collection
- phase item creation
- GI renderable creation
- shadow atlas sync

#### phase_extract.rs

Own:

- transform resolution
- world view collection
- runtime feature `extract`
- runtime feature `collect_views`
- directional shadow view append
- view ordering/finalization
- temporal view tracking
- runtime feature `prepare`
- extractor execution
- feature phase item append
- opaque and transparent phase sorting

Do not own:

- GPU table upload
- material dirty preparation
- GI provider preparation
- shadow GPU sync

#### scene_upload.rs

Own:

- model matrix assignment
- previous model matrix construction
- entity-to-model-slot map
- light collection
- `ModelMatrixTable` upload
- `LightTable` upload
- previous model tracking update at frame end

Do not own:

- GI provider resource preparation
- shadow cascade setup
- phase extraction

#### gi_prepare.rs

Own:

- standard material GI renderable collection
- primary lit view selection
- `GiSceneInput` construction
- GI provider `prepare`

Do not own:

- GI compute pass execution
- scene texture allocation
- generic render graph behavior

#### shadow_prepare.rs

Own:

- `sync_shadow_views`
- shadow debug resource discovery
- shadow stats aggregation
- shadow draw call counting
- cascade draw call counting

Do not own:

- generic light upload
- frame stats assembly beyond shadow summary

#### frame_assemble.rs

Own:

- `PreparedFrame` construction
- frame payload insertion
- `PreparedView` construction
- view payload insertion
- feature frame/view payload insertion
- pipeline execution wrapper

Do not own:

- extraction
- upload
- shadow or GI preparation

### Validation

After each extracted helper compiles:

```bash
cargo test --features app render::runtime
```

After the phase is complete:

```bash
cargo fmt
cargo test --features app render::runtime
cargo check --examples --features app
```

## Phase 3: Isolate runtime/nodes.rs Unsafe Boundaries

### Intent

Do not redesign `FramePipeline` yet. First, isolate raw pointer dereferencing and
document the invariants.

### Proposed Structure

```text
src/render/runtime/nodes/
├── mod.rs
├── seed.rs
├── phase.rs
├── compute.rs
├── graph.rs
├── postfx.rs
├── finalize.rs
└── step_refs.rs
```

### step_refs.rs

Add small internal wrappers, for example:

```rust
pub(crate) struct PhaseStepRefs {
    phase: *mut dyn RenderPhase,
    draw_functions: *mut DrawFunctionRegistry,
    materials: *mut MaterialRegistry,
    mesh_registry: *const MeshRegistry,
    fallback_texture: *const Texture,
}
```

The wrapper should expose safe-looking methods only inside the runtime module:

```rust
impl PhaseStepRefs {
    pub(crate) fn phase(&self) -> &dyn RenderPhase;
    pub(crate) fn phase_mut(&mut self) -> &mut dyn RenderPhase;
    pub(crate) fn draw_functions_mut(&mut self) -> &mut DrawFunctionRegistry;
    pub(crate) fn materials_mut(&mut self) -> &mut MaterialRegistry;
    pub(crate) fn mesh_registry(&self) -> &MeshRegistry;
    pub(crate) fn fallback_texture(&self) -> &Texture;
}
```

Each unsafe block should have a short invariant comment:

```rust
// SAFETY: These pointers are created while building a FramePipeline from a live
// RenderComposer. The composer owns the pointed-to state for the duration of
// pipeline execution, and FramePipeline executes nodes serially.
```

### Node Split

Move families without changing behavior:

```text
SceneColorSeedNode       -> seed.rs
PhaseStepNode            -> phase.rs
ComputeStepNode          -> compute.rs
GraphPassStepNode        -> graph.rs
PostFxStepNode           -> postfx.rs
RenderPassStepNode       -> finalize.rs
HeadlessKeepAliveNode    -> finalize.rs or seed.rs
```

### Validation

```bash
cargo fmt
cargo test --features app render::runtime
cargo test --features app render::pipeline
```

## Phase 4: Reduce pipeline/contexts.rs Duplication

### Intent

Keep the public context types but share their implementation through internal
core structs.

### Proposed Internal Cores

```rust
pub(crate) struct SetupContextCore<'graph, 'frame> {
    graph: &'graph mut RenderGraph,
    state: &'graph mut PhaseState,
    frame: &'frame PreparedFrame<'frame>,
    view: &'frame PreparedView<'frame>,
}
```

```rust
pub(crate) struct ExecuteContextCore<'gpu, 'frame> {
    gpu: &'gpu mut GpuContext,
    pass: &'frame CompiledPass,
    resources: &'frame PhysicalResources<'frame>,
    execution: &'frame ViewExecutionContext<'frame>,
}
```

Then keep public contexts as wrappers:

```rust
pub struct ComputePassSetupContext<'graph, 'frame> {
    core: SetupContextCore<'graph, 'frame>,
}
```

### Rules

- Do not remove public context type names.
- Do not force user pass implementations through generic traits.
- Do not make public APIs less discoverable.
- Do not change lifetime semantics unless required by the compiler.
- Move common helpers into `impl SetupContextCore` and `impl ExecuteContextCore`.
- Keep pass-specific methods on the pass-specific context.

### Candidate Shared Setup Methods

- `graph`
- `state`
- `frame`
- `view`
- `frame_payload`
- `view_payload`
- `scene_lighting`
- `optional_scene_shadows`
- `require_scene_shadows`
- `publish_scene_shadows`
- `blackboard`
- `blackboard_ref`
- `blackboard_set`
- `blackboard_get`
- `blackboard_get_mut`
- `optional_scene_texture`
- `require_scene_texture`
- `set_scene_texture`
- `ensure_scene_texture`
- `create_texture`
- `history_texture`

### Candidate Shared Execute Methods

- `gpu`
- `pass`
- `resources`
- `frame`
- `view`
- `view_index`
- `frame_payload`
- `view_payload`
- `blackboard`
- `blackboard_get`
- read/write texture helpers
- read/write subresource helpers

### Validation

```bash
cargo fmt
cargo test --features app render::pipeline
cargo test --features app render::runtime
cargo check --examples --features app
```

## Phase 5: Split phase/draw.rs

### Intent

Separate draw registration, contexts, mesh drawing, sprite drawing, and binding
helpers while preserving hot-path behavior.

### Proposed Structure

```text
src/render/phase/
├── draw.rs
├── draw_context.rs
├── draw_registry.rs
├── mesh_draw.rs
├── sprite_draw.rs
├── scene_bindings.rs
└── batching.rs
```

### Ownership

```text
DrawError                 -> draw.rs or error.rs
DrawFunction              -> draw.rs
DrawFunctionRegistry      -> draw_registry.rs
DrawContext               -> draw_context.rs
StandaloneDrawContext     -> draw_context.rs
DrawMesh<M>               -> mesh_draw.rs
DrawSprite                -> sprite_draw.rs
DrawSpriteRuntime         -> sprite_draw.rs
scene binding resolution  -> scene_bindings.rs
batch cursor helpers      -> batching.rs
```

### Rules

- Do not change phase item sorting.
- Do not change batching keys.
- Do not introduce per-item allocations in hot loops.
- Do not alter `DrawFunction` behavior before and after the move.
- Keep mesh and sprite rendering tests passing after each move.

### Validation

```bash
cargo fmt
cargo test --features app render::phase
cargo test --features app render::runtime
cargo check --examples --features app
```

## Phase 6: Prepare MaterialRegistry For Later Splitting

### Intent

Keep `MaterialRegistry` as the material subsystem facade, but stop it from
growing further.

### Candidate Extracts

```text
src/render/resources/material/
├── registry.rs
├── debug.rs
├── prepare_queue.rs
└── records.rs
```

Move:

- debug summary construction to `debug.rs`
- dirty preparation helper logic to `prepare_queue.rs`
- internal record structs to `records.rs` if it improves readability

### Rules

- Keep typed material storage API stable.
- Keep handle validation behavior stable.
- Keep pipeline cache access stable.
- Do not combine this with material trait redesign.
- Do not duplicate material instance state.

### Validation

```bash
cargo fmt
cargo test --features app render::resources::material
cargo test --features app render::runtime
cargo check --examples --features app
```

## Phase 7: Optional Tilemap Import Split

### Intent

Improve maintainability of the Tiled importer after the core render runtime is
cleaner.

### Proposed Structure

```text
src/render/tilemap/tiled/
├── mod.rs
├── error.rs
├── types.rs
├── json.rs
├── tmx.rs
├── tileset.rs
├── layer.rs
├── data.rs
└── tests.rs
```

### Rules

- Keep `TiledImport` as the public entry point.
- Keep `TiledMapInstance` behavior unchanged.
- Keep official Tiled sample tests.
- Do not move Tiled semantics into generic render modules.

### Validation

```bash
cargo fmt
cargo test --features app render::tilemap::tiled
cargo test --features app render::tilemap
```

## Suggested Implementation Order

1. Split `pipeline/builtins.rs`.
2. Extract `RenderComposer::render_world` helpers.
3. Split `runtime/nodes.rs` and isolate unsafe references.
4. Add shared context cores in `pipeline/contexts.rs`.
5. Split `phase/draw.rs`.
6. Extract non-core helpers from `MaterialRegistry`.
7. Optionally split `tilemap/tiled.rs`.

This order gives the best risk profile:

- Phase 1 is mostly mechanical.
- Phase 2 removes the largest architectural pressure point.
- Phase 3 narrows unsafe code after the frame flow is easier to read.
- Phase 4 reduces future API duplication.
- Phase 5 touches hot draw paths only after the surrounding runtime is calmer.
- Phase 6 and Phase 7 are cleanup once the core pressure points are handled.

## Acceptance Criteria

The refactor is complete when:

- `RenderComposer::render_world` is a readable orchestration method rather than
  a full frame implementation.
- Built-in pipeline passes live in focused modules.
- Runtime step nodes are split by step family.
- Raw-pointer dereferencing in runtime nodes is isolated and documented.
- Pipeline contexts share internal core logic where appropriate.
- Draw implementation is split without changing batching behavior.
- `MaterialRegistry` is still the facade, but debug and preparation helpers are
  not expanding the core file.
- Public render examples still compile.

## Final Validation

Run:

```bash
cargo fmt --check
cargo test --features app render::runtime
cargo test --features app render::pipeline
cargo test --features app render::phase
cargo test --features app render::resources::material
cargo check --examples --features app
```

If render graph internals are touched unexpectedly, also run:

```bash
cargo test --features app graph
```

If tilemap importer splitting is included, also run:

```bash
cargo test --features app render::tilemap
```

## Known Risks

- `RenderComposer::render_world` extraction may hit borrow checker pressure
  because many helpers need partial access to `self`.
- `runtime/nodes.rs` uses raw pointers because the pipeline owns node objects
  while runtime state remains in `RenderComposer`. Do not casually replace this
  with references without checking lifetime and object-safety constraints.
- `phase/draw.rs` is hot-path code. File splitting must not introduce extra
  allocation, dynamic dispatch, or hash lookups in per-item loops.
- Context deduplication can make public APIs harder to discover if overdone.
  Keep public context methods explicit even if they forward to internal cores.
- Existing local changes are extensive. Avoid broad formatting or unrelated file
  churn when implementing this plan.

## Non-Goals

- No material system rewrite.
- No render graph rewrite.
- No shader rewrite.
- No new renderer family.
- No public API break unless separately approved.
- No benchmark number update.
- No cleanup of unrelated worktree changes.
