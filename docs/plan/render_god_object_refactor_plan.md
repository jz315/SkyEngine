# Render God Object Breaking Refactor Plan

## Purpose

This is an intentionally breaking render-runtime refactor plan.

The old goal was to reduce god-object pressure while preserving the current
public render architecture. That is no longer the goal. The new goal is to make
the render runtime structurally cleaner even if public names, import paths,
context types, builder APIs, examples, and downstream app code must be migrated.

The refactor may replace the current shape:

```text
RenderPipelineAsset
  -> RenderComposer
  -> PreparedFrame / PreparedView
  -> FramePipeline
  -> RenderGraph / wgpu execution
```

with a new shape:

```text
RenderPipelineAsset or replacement descriptor
  -> RenderRuntime
  -> FrameStages
  -> RenderExecutor
  -> RenderGraph / wgpu execution
```

Names are provisional. The important architectural change is that frame
preparation, extraction, scene upload, lighting, shadows, GI, frame assembly,
and execution become explicit stages with clear ownership instead of being
coordinated through one long `RenderComposer::render_world` method.

## Current Code Facts

This plan is written against the current tree, not the older compatibility plan.

- `src/render/mod.rs` keeps most render internals `pub(crate)` and exposes a
  curated facade. External imports such as
  `sky_engine::render::pipeline::builtins::*` are not a compatibility contract
  today.
- `src/render/runtime/frame_coordinator.rs` contains the frame recipe that
  replaced the old god-object pressure: `RenderRuntime::render_world`
  delegates initialization, extraction, phase population, GPU table upload,
  GI/shadow preparation, `PreparedFrame` assembly, execution, and stats to
  explicit stage functions.
- `src/render/execution/step_nodes/` contains runtime node families and raw pointer
  dereferencing, but the raw pointers are created in
  `src/render/runtime/pipeline_runtime.rs`. Fixing only `nodes.rs` is not
  enough.
- `src/render/execution/contexts/` is not reducible to one simple setup core
  and one simple execute core. Current contexts differ by view vs finalize
  state, phase vs non-phase execution, and extra draw/material/mesh services.
- `src/render/resources/material/debug.rs` and
  `src/render/resources/material/prepare.rs` already exist. Material cleanup
  should target remaining registry responsibilities, not pretend those files
  are missing.
- The current worktree is dirty. Existing local changes are user-owned unless
  the implementation task explicitly targets them.

## Breaking Refactor Rules

- Public render API stability is not a goal.
- Do not preserve old import paths.
- It is acceptable to rename or remove `RenderComposer`,
  `RenderPipelineBuilder`, `PipelineStep`, pass context types, and built-in pass
  locations.
- Prefer removing awkward lifetime/unsafe patterns over wrapping them for
  compatibility.
- Keep behavior-focused tests meaningful, but update tests and examples to the
  new API instead of forcing the new implementation through old names.
- Do not combine this refactor with shader rewrites, benchmark-number updates,
  or unrelated renderer-family feature work.
- Keep each implementation slice compiling before starting the next slice.

## Design Quality Bar

The refactor should not merely move code into more files. The new render
architecture should be modern, decoupled, clear to read, pleasant to use, and
harder to misuse.

### Modern

- Prefer explicit data flow over hidden mutable global state.
- Prefer typed stage inputs and outputs over unstructured bags of optional
  resources.
- Prefer owned execution plans or short-lived borrows over raw pointers and
  lifetime workarounds.
- Prefer capability-specific contexts over one context type that exposes every
  subsystem.

### Decoupled

- Each stage owns one reason to change:
  - frame input collection
  - view collection
  - ECS extraction
  - scene upload
  - lighting
  - shadows
  - GI
  - frame assembly
  - graph execution
  - stats
- Stages communicate through small records, not by reaching back into a shared
  god runtime.
- Renderer-family state stays local to that renderer family unless it is truly
  cross-renderer frame data.
- Generic execution contexts must not expose material, mesh, sprite, shadow, or
  GI services unless that execution mode needs them.

### Elegant And Clear

- The top-level frame method should read as a recipe.
- Public names should describe user intent, not implementation history.
- Avoid "manager", "helper", and "state" names when a more specific domain name
  exists.
- Keep module paths predictable:
  - public authoring API under the render facade
  - frame runtime under `runtime`
  - execution under `executor`
  - built-in passes under `builtins`
  - renderer-family internals under their family modules

### Easy To Use

- A minimal app should need only a small set of public concepts:
  - a render descriptor or builder
  - a render runtime
  - features or built-ins
  - ECS authoring components
- Advanced users can opt into lower-level execution APIs, but common sprite,
  mesh, lighting, shadow, GI, and post-fx setup should not require touching
  internals.
- Built-in pass ordering should be obvious from the builder or descriptor.
- Error messages should mention the missing capability, stage, pass, or payload
  by name.

### Hard To Misuse

- Make invalid states unrepresentable where practical:
  - no executable runtime before GPU-dependent initialization
  - no phase execution context without draw services
  - no finalize context pretending to have a current view
  - no scene texture access before the graph state declares it
- Prefer typed payload access and typed handles over stringly typed lookups.
- Keep unsafe code out of normal frame execution. If unsafe remains, isolate it
  behind a tiny internal API with documented invariants and tests.
- Do not introduce service locators that make every subsystem reachable from
  every stage.

## Target Architecture

### Runtime Ownership

Replace `RenderComposer` as the central owner of every concern with a runtime
that owns explicit subsystems:

```text
RenderRuntime
├── pipeline: PipelineRuntime
├── resources: RenderResourceHub
├── frame: FrameCoordinator
├── views: ViewSystem
├── extraction: ExtractionSystem
├── scene: SceneUploadSystem
├── lighting: LightingSystem
├── shadows: ShadowSystem
├── gi: GiSystem
├── executor: RenderExecutor
└── stats: RenderStatsCollector
```

`RenderRuntime` should remain the app-facing object, but it should delegate
nearly all work. It should not understand the details of material preparation,
phase population, shadow cascade synchronization, GI renderable collection, and
graph execution at the same time.

### Frame Data

Frame stages should pass owned or narrowly borrowed data through explicit
records:

```rust
pub(crate) struct FrameInputs { /* settings, assets, transforms, frame timing */ }
pub(crate) struct FrameViews { /* sorted scene and shadow views */ }
pub(crate) struct ExtractedPhases { /* per-view opaque/transparent/other phases */ }
pub(crate) struct SceneUploads { /* model slots, matrices, lights, GPU table handles */ }
pub(crate) struct LightingFrame { /* light resources and frame-visible light data */ }
pub(crate) struct ShadowFrame { /* shadow bindings, debug resources, stats */ }
pub(crate) struct GiFrame { /* provider inputs and prepared provider state */ }
pub(crate) struct FrameAssembly { /* PreparedFrame, PreparedView, payload ownership */ }
```

Do not force these exact names. The requirement is that `render_world` no
longer owns a long chain of unrelated local variables whose lifetimes all depend
on one method body.

### Execution Model

The executor should not need raw pointers into `RenderRuntime`.

Instead of building a `FramePipeline` from borrowed phase/pass objects and
storing raw pointers in nodes, build an execution plan that either:

- owns the runtime step objects, or
- borrows them only for the duration of a single execute call through an
  explicit `RenderServices` parameter.

Proposed service container:

```rust
pub(crate) struct RenderServices<'a> {
    pub(crate) gpu: &'a mut GpuContext,
    pub(crate) draw_functions: &'a mut DrawFunctionRegistry,
    pub(crate) materials: &'a mut MaterialRegistry,
    pub(crate) meshes: &'a MeshRegistry,
    pub(crate) fallback_texture: &'a Texture,
}
```

`RenderServices` is passed into execution. It is not stored inside long-lived
nodes.

### Context API

Break the current context API. Replace the many overlapping public contexts
with fewer context families that match real execution modes:

```text
ViewSetupContext
ViewExecuteContext
FinalizeSetupContext
FinalizeExecuteContext
PhaseExecuteContext
```

`PhaseExecuteContext` owns a `PhaseDrawServices` capability for
draw/material/mesh/fallback access. Generic compute, graph, post-fx, and
finalize contexts should not expose phase-only services.

Compatibility wrappers for the previous phase context names should not be
added.

### Built-In Pass Locations

Built-in pass families should move out of one mixed file. Since this is a
breaking refactor, choose public names after the split instead of preserving
`pipeline::builtins`.

Suggested internal layout:

```text
src/render/builtins/
├── mod.rs
├── prepass.rs
├── gi.rs
├── shadows.rs
├── postfx.rs
└── debug.rs
```

The final facade may expose either:

```rust
sky_engine::render::builtins::Bloom
```

or curated root exports:

```rust
sky_engine::render::Bloom
```

but this should be a deliberate new API decision, not a compatibility promise.

## Phase 0: Baseline And Decision Record

### Tasks

1. Capture the current local state:

   ```bash
   git status --short
   ```

2. Record the intended breaking API decisions before editing code:

   - new app-facing runtime type name
   - new pipeline descriptor/builder names
   - new pass trait names, if any
   - new context type names
   - new built-in pass export path

3. Identify user-owned worktree changes and avoid unrelated cleanup.

4. Do not add compatibility shims, aliases, or old-path re-exports.

### Validation

No behavior validation is required in this phase. The output is a written API
decision record or a short checklist in the implementation task.

## Phase 1: Define The New Public Surface

### Intent

Stop treating the current facade as fixed. Define the new public API first so
the implementation has a target.

### Tasks

1. Decide the replacement for `RenderComposer`; do not keep it as a type alias
   for migration.
2. Decide whether `RenderPipelineAsset` remains the user-authored descriptor or
   is replaced by a clearer descriptor/runtime split.
3. Replace the current pass context export set with the new context family.
4. Move or rename built-in pass exports.
5. Update `src/render/mod.rs` and `src/render/expert.rs` to expose the new API.
6. Update examples only after the implementation phases introduce the new
   behavior.

### Validation

```bash
cargo check --features app
```

Temporary example failures are acceptable during this phase only if the next
phase explicitly migrates them.

## Phase 2: Split Pipeline Declarations From Runtime Steps

### Intent

Remove the lifetime pressure that currently leads to raw pointers in runtime
nodes.

### Tasks

1. Split declaration-time pipeline data from executable runtime state.
2. Ensure runtime steps are owned by `PipelineRuntime` or are borrowed only
   within a single execute call.
3. Replace `build_runtime_pipeline` pointer capture with an executor call that
   receives `RenderServices`.
4. Remove `*mut dyn RenderPhase`, `*mut dyn ComputePass`,
   `*mut dyn GraphPass`, `*mut dyn PostFxPass`, and `*mut dyn RenderPass` from
   long-lived nodes.
5. Decide whether built-in opaque/transparent behavior remains special-cased or
   becomes ordinary phase implementations.

### Validation

```bash
cargo fmt
cargo test --features app render::runtime
cargo test --features app render::pipeline
```

## Phase 3: Replace `RenderComposer::render_world` With Frame Stages

### Intent

Make frame flow explicit and owned by stage objects.

### Proposed Modules

```text
src/render/runtime/
├── runtime.rs
├── frame/
│   ├── mod.rs
│   ├── inputs.rs
│   ├── views.rs
│   ├── extract.rs
│   ├── scene_upload.rs
│   ├── lighting.rs
│   ├── shadows.rs
│   ├── gi.rs
│   ├── assemble.rs
│   └── stats.rs
└── executor/
    ├── mod.rs
    ├── plan.rs
    ├── view.rs
    └── finalize.rs
```

Use different filenames if the implementation becomes clearer, but keep stage
ownership visible in the module tree.

### Target Runtime Flow

```rust
pub fn render_world(&mut self, gpu: &mut GpuContext, world: &World) {
    let frame = self.frame.begin(gpu, world, &mut self.resources);
    let views = self.views.collect(world, &frame);
    let phases = self.extraction.extract(gpu, world, &frame, &views, &mut self.resources);
    let uploads = self.scene.upload(gpu, world, &frame, &phases, &mut self.resources);
    let lighting = self.lighting.prepare(gpu, world, &frame, &views, &uploads);
    let shadows = self.shadows.prepare(gpu, world, &frame, &views, &phases, &uploads);
    let gi = self.gi.prepare(gpu, &frame, &views, &phases, &uploads, &lighting);
    let assembly = self.frame.assemble(&frame, &views, &phases, &uploads, &lighting, &shadows, &gi);
    let stats = self.executor.execute(gpu, &self.pipeline, assembly, &mut self.resources);
    self.stats.finish(frame, stats);
}
```

This shape is illustrative. The implementation may use fewer objects, but the
final method should read as a frame recipe, not as the implementation of every
stage.

### Validation

```bash
cargo fmt
cargo test --features app render::runtime
```

## Phase 4: Replace Contexts With Mode-Specific APIs

### Intent

Break the duplicated context API instead of preserving the old context names.

### Tasks

1. Introduce mode-specific setup and execute contexts:

   - `ViewSetupContext`
   - `ViewExecuteContext`
   - `FinalizeSetupContext`
   - `FinalizeExecuteContext`
   - `PhaseExecuteContext`

2. Move shared graph/frame/view helpers into private reusable cores only where
   lifetimes actually match.
3. Keep finalize state separate from view state.
4. Keep phase draw services out of non-phase contexts.
5. Update all built-in and example pass implementations to the new context API.
6. Delete old context exports once call sites are migrated.

### Validation

```bash
cargo fmt
cargo test --features app render::pipeline
cargo test --features app render::runtime
```

## Phase 5: Move Built-Ins Into A New Module Tree

### Intent

Split the current monolithic `pipeline/builtins.rs` after the new context API is
defined, so moved code can target the new API immediately.

### Proposed Ownership

```text
SceneNormalPrepass        -> builtins/prepass.rs
SceneMaterialPrepass      -> builtins/prepass.rs
GiUpdateCompute           -> builtins/gi.rs
GiCompositePass           -> builtins/gi.rs
ContactShadows            -> builtins/shadows.rs
Bloom                     -> builtins/postfx.rs
ToneMap                   -> builtins/postfx.rs
TemporalAntiAliasing      -> builtins/postfx.rs
Sharpen                   -> builtins/postfx.rs
Vignette                  -> builtins/postfx.rs
DebugView                 -> builtins/debug.rs
```

### Rules

- Do not preserve `render::pipeline::builtins`; choose a new built-in export
  path.
- Prefer family-local helpers over a large shared `mod.rs`.
- Update facade exports after the split.

### Validation

```bash
cargo fmt
cargo test --features app render::pipeline
cargo check --examples --features app
```

## Phase 6: Split Draw Execution

### Intent

Break up `phase/draw.rs` and separate draw registration from mesh/sprite draw
backends.

### Proposed Structure

```text
src/render/phase/
├── draw_context.rs
├── draw_registry.rs
├── errors.rs
├── mesh_draw.rs
├── sprite_draw.rs
├── scene_prepass.rs
├── scene_bindings.rs
└── mesh_instance.rs
```

### Rules

- Preserve batching behavior unless a benchmarked follow-up intentionally
  changes it.
- Do not introduce per-item allocations in phase execution.
- Move code family by family and run tests after each family.
- If the new context API changes draw call signatures, update the draw traits
  once and migrate all draw implementations together.

### Validation

```bash
cargo fmt
cargo test --features app render::phase
cargo test --features app render::runtime
```

## Phase 7: Split Material Registry Internals

### Intent

Keep or replace the material facade deliberately, but remove remaining registry
god-object responsibilities.

### Current State

`debug.rs` and `prepare.rs` already exist. Do not create a plan that treats them
as missing.

### Candidate Structure

```text
src/render/resources/material/
├── registry.rs
├── records.rs
├── instance_store.rs
├── dirty_queue.rs
├── debug.rs
├── prepare.rs
└── ...
```

### Tasks

1. Move `ModelRecord` into `records.rs`.
2. Move generational instance slot bookkeeping into `instance_store.rs`.
3. Move dirty instance queue logic into `dirty_queue.rs`.
4. Keep debug summary construction near `debug.rs` types or expose a narrow
   internal helper.
5. Revisit whether `MaterialRegistry` remains the public facade or becomes an
   internal service inside `RenderResourceHub`.

### Validation

```bash
cargo fmt
cargo test --features app render::resources::material
cargo test --features app render::runtime
```

## Phase 8: Migrate Examples And Facade Docs

### Intent

Because this refactor is breaking, examples and docs must move to the new API
instead of being used to constrain the refactor.

### Tasks

1. Update render examples under `examples/render/`.
2. Update demo examples under `examples/demo/` if they use the old runtime API.
3. Update `README.md`, `README_EN.md`, and `docs/api.md` render snippets.
4. Update `src/render/mod.rs` facade export tests.
5. Remove obsolete aliases and old-path exports instead of preserving them.

### Validation

```bash
cargo fmt
cargo check --examples --features app
cargo test --features app
```

## Phase 9: Optional Tilemap Import Split

### Intent

Split the Tiled importer after the core runtime is stable. This is not on the
critical path for the render god-object refactor.

### Proposed Structure

```text
src/render/tilemap/tiled/
├── mod.rs or tiled.rs
├── data.rs
├── error.rs
├── json.rs
├── layer.rs
├── object.rs
├── properties.rs
├── tmx.rs
├── tileset.rs
├── types.rs
├── util.rs
└── tests.rs
```

### Validation

```bash
cargo fmt
cargo test --features app render::tilemap
```

## Suggested Implementation Order

1. Define the new public API and delete the old public surface.
2. Split pipeline declarations from runtime step ownership.
3. Replace raw-pointer node execution with service-passed execution.
4. Stage `render_world` into explicit frame systems.
5. Replace old pass contexts with mode-specific contexts.
6. Move built-ins to the new module tree.
7. Split draw execution internals.
8. Split material registry internals.
9. Migrate examples and docs.
10. Optionally split tilemap import.

This order prioritizes architectural blockers before mechanical file splits.
Splitting `builtins.rs` first is no longer preferred because the old built-ins
would otherwise be moved once for the old context API and again for the new one.

## Acceptance Criteria

The refactor is complete when:

- The app-facing render runtime no longer exposes or depends on the old
  `RenderComposer` public surface or god-object shape.
- `render_world` is a short orchestration method over explicit frame stages.
- Each frame stage has a narrow typed input and output, and no stage reaches
  through a shared runtime to mutate unrelated subsystems.
- Runtime execution no longer stores raw pointers to phases, passes, material
  registry, draw registry, mesh registry, or fallback texture.
- The old duplicated context family is removed.
- The new context types are capability-specific; generic compute/graph/post-fx
  contexts do not expose phase-only draw or material services.
- Built-in passes live in focused modules and target the new context API.
- Public render setup has a small happy path for normal apps and a separate
  expert path for lower-level execution control.
- Invalid execution modes are rejected by type shape or early named errors, not
  by late panics from missing payloads.
- Draw execution is split by registry, context, mesh draw, sprite draw, scene
  prepass, and batching concerns.
- Material registry internals no longer mix record storage, dirty queue logic,
  debug summary construction, and preparation orchestration in one file.
- Render examples and docs compile against the new API.

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

Then run the broader app feature suite:

```bash
cargo test --features app
```

If render graph internals are touched, also run:

```bash
cargo test --features app graph
```

## Known Risks

- This is a breaking refactor. Examples, downstream apps, and docs will fail
  until migrated.
- Replacing raw-pointer node execution may require changing `FramePipeline` or
  replacing it with a new executor. This is expected.
- Context API replacement will touch every built-in pass and any custom example
  pass implementation.
- `phase/mesh_draw.rs` and `phase/sprite_draw.rs` contain hot draw paths.
  Splitting them further must not introduce extra allocation, dispatch, or hash
  lookups in per-item loops unless a later benchmarked task approves the cost.
- The dirty worktree is extensive. Avoid formatting or touching files outside
  the active implementation slice.

## Non-Goals

- No shader behavior rewrite.
- No render graph algorithm rewrite unless required by the new executor.
- No new renderer family.
- No benchmark result updates.
- No compatibility promise for the old render API.
