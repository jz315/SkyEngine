# Render File Organization Plan

## Purpose

This is an intentionally breaking organization and naming plan for
`src/render`.

The current render refactor has already reduced several god-object pressure
points, but the file tree still does not always explain itself. Some names are
too generic (`assets.rs`, `nodes.rs`, `types.rs`, `pass.rs`), and some folders
mix different architectural dimensions such as declaration, execution,
runtime ownership, backend upload, and ECS authoring.

The goal of this plan is to make file paths readable enough that a maintainer
can usually answer these questions from the path alone:

- Is this CPU-side authoring data or runtime GPU state?
- Is this a declaration/build-time concept or a per-frame execution concept?
- Is this shared infrastructure or renderer-family-specific logic?
- Is this a cache, registry, descriptor, context, step node, or frame stage?

Compatibility with old internal paths is not a goal. Keep each slice compiling
before starting the next one.

## Current Pain Points

- `pipeline/asset.rs` is misleading. It contains pipeline builder, descriptor,
  backend kind, pipeline steps, and material registration, not a normal render
  asset.
- `pipeline/contexts/` contains execution contexts. These are closer to
  `execution/` than to declaration-time `pipeline/`.
- `runtime/nodes/` contains executable step nodes. The folder name is too
  generic and the module belongs closer to the execution layer.
- `asset/mod.rs` is too broad. It mixes mesh assets, material assets, vertex
  layouts, and an authoring collection.
- `resources/mesh.rs` is too broad. It mixes runtime mesh registry, GPU upload,
  layout handling, and CPU ray geometry extraction.
- `runtime/frame/` has useful stage files, but some names should be more
  recipe-like and action-oriented.
- Large test files make behavior hard to locate even after production code is
  split.

## Naming Rules

- Avoid standalone generic filenames:
  - avoid `types.rs`, `nodes.rs`, `assets.rs`, `pass.rs`, `state.rs`
  - prefer names such as `pipeline_descriptor.rs`, `phase_step_node.rs`,
    `texture_cache.rs`, `mesh_registry.rs`, `frame_inputs.rs`
- Use nouns for stable data definitions:
  - `*_asset.rs`
  - `*_descriptor.rs`
  - `*_settings.rs`
  - `*_handle.rs`
- Use lifecycle names for owned runtime containers:
  - `*_registry.rs`
  - `*_cache.rs`
  - `*_store.rs`
- Use action names for frame stages:
  - `collect_*`
  - `prepare_*`
  - `upload_*`
  - `assemble_*`
  - `execute_*`
  - `finish_*`
- Use capability names for contexts:
  - `phase_context.rs`
  - `compute_context.rs`
  - `graph_pass_context.rs`
  - `finalize_pass_context.rs`
- Use explicit node names for execution nodes:
  - `phase_step_node.rs`
  - `compute_step_node.rs`
  - `scene_color_seed_node.rs`
  - `headless_keepalive_node.rs`

## Target Folder Semantics

```text
src/render/
├── asset/              CPU-side authoring assets and descriptors
├── backend/            wgpu/Kajiya/Renderling backend adapters and upload bridges
├── builtins/           Built-in phases, passes, post-fx wrappers, GI/shadow steps
├── component/          ECS-facing authoring components
├── execution/          FramePipeline, execution contexts, step nodes, payload/slot state
├── graph/              Low-level declarative render graph
├── gpu/                Shared GPU primitives and helpers
├── lighting/           Light data, light tables, shadow system
├── material/           Optional future top-level material facade/internals
├── mesh/               Mesh renderer family logic
├── pipeline/           Declaration-time builder, descriptor, step, feature/pass traits
├── postfx/             Reusable effect implementations
├── resources/          Runtime caches and registries
├── runtime/            RenderRuntime, frame coordinator, history, stats, view collection
├── sprite/             Sprite renderer family logic
├── tilemap/            Tilemap renderer family and Tiled import bridge
├── view/               Camera, projection, viewport, SceneView construction
└── live2d/             Live2D feature and low-level Cubism renderer
```

## Architecture Boundaries

### `asset/`

CPU-side render authoring data lives here.

Examples:

- `mesh_asset.rs`
- `material_asset.rs`
- `vertex_layout.rs`
- `render_assets.rs`

Do not put GPU residency, texture upload queues, backend handles, or frame
resource lifetime here.

### `pipeline/`

Declaration-time configuration lives here.

This folder should answer: "What does the user want the renderer to run?"

Examples:

- pipeline descriptor
- builder
- step list
- backend kind
- feature traits
- pass/phase trait definitions
- material registration declarations

Do not put per-frame execution contexts or step-node execution logic here.

### `execution/`

Per-frame execution mechanics live here.

This folder should answer: "How is the prepared frame executed?"

Examples:

- `FramePipeline`
- setup/view/finalize execution nodes
- execution contexts
- scene texture slots
- typed payload stores
- step nodes adapted from pipeline steps

### `runtime/`

The app-facing runtime owner and frame recipe live here.

This folder should answer: "How does `RenderRuntime::render_world` coordinate a
frame?"

Keep concrete pass/node execution details out of this folder unless they are
truly runtime orchestration.

### `resources/`

Runtime caches and registries live here.

Examples:

- texture cache
- mesh registry
- material registry
- blackboard
- atlas

Do not put CPU authoring assets here. Do not use this folder as a dumping
ground for any type that happens to be long-lived.

### Renderer Families

Renderer-family logic should stay local:

- `sprite/`
- `mesh/`
- `tilemap/`
- `lighting/`
- `live2d/`

Promote concepts to shared folders only when they are genuinely cross-renderer:
view, camera, viewport, render order/layer semantics, shared scene textures,
or common GPU tables.

## Execution Plan

### Phase 1: Split Pipeline Declaration Files

Break up `src/render/pipeline/asset.rs`.

Target files:

```text
src/render/pipeline/
├── backend_kind.rs
├── builder.rs
├── descriptor.rs
├── material_registration.rs
├── step.rs
├── features.rs
├── passes.rs
├── phases.rs
└── resource_spec.rs
```

Expected result:

- `pipeline/` reads as declaration-time configuration.
- `RenderPipelineAsset`, `RenderPipelineBuilder`, `PipelineStep`, and
  `RenderBackendKind` still re-export through `pipeline/mod.rs` and
  `render/mod.rs`.
- No compatibility aliases for old internal file paths.

Validation:

```powershell
cargo fmt
cargo check --features app
cargo check --examples --features app
cargo test --features app render::pipeline
```

### Phase 2: Move Execution Contexts

Move `src/render/pipeline/contexts/` to
`src/render/execution/contexts/`.

Target files:

```text
src/render/execution/contexts/
├── compute_context.rs
├── graph_pass_context.rs
├── finalize_pass_context.rs
├── phase_context.rs
├── postfx_context.rs
├── scene_texture_context.rs
└── tests.rs
```

Expected result:

- Pipeline traits may import context types from `execution`.
- Public facade may still re-export context types from `render::*` if desired,
  but the implementation location should reflect execution ownership.

Validation:

```powershell
cargo fmt
cargo check --features app
cargo check --examples --features app
cargo test --features app render::execution::contexts
```

### Phase 3: Move Runtime Step Nodes Into Execution

Move `src/render/runtime/nodes/` to
`src/render/execution/step_nodes/`.

Target files:

```text
src/render/execution/step_nodes/
├── compute_step_node.rs
├── graph_pass_step_node.rs
├── finalize_pass_step_node.rs
├── phase_step_node.rs
├── postfx_step_node.rs
├── scene_color_seed_node.rs
├── headless_keepalive_node.rs
└── render_services.rs
```

Expected result:

- `runtime/pipeline_runtime.rs` becomes the adapter that builds an execution
  pipeline from declaration-time steps.
- Step node code no longer lives under `runtime/`.
- Node filenames state exactly which step they execute.

Validation:

```powershell
cargo fmt
cargo check --features app
cargo check --examples --features app
cargo test --features app render::runtime::tests -- --test-threads=1
```

### Phase 4: Split CPU-Side Render Assets

Split `src/render/asset/mod.rs`.

Target files:

```text
src/render/asset/
├── mesh_asset.rs
├── material_asset.rs
├── vertex_layout.rs
├── render_assets.rs
└── mod.rs
```

Expected result:

- CPU-side asset definitions are easy to find.
- Backend upload code imports semantic asset types from explicit files through
  the `asset` module facade.
- `resources/texture_cache.rs` remains the runtime GPU texture cache.

Validation:

```powershell
cargo fmt
cargo check --features app
cargo check --examples --features app
cargo test --features app render::backend
```

### Phase 5: Split Runtime Mesh Resources

Split `src/render/resources/mesh.rs` into the `src/render/resources/mesh/`
module directory.

Target files:

```text
src/render/resources/mesh/
├── mod.rs
├── error.rs
├── gpu_mesh.rs
├── math.rs
├── registry.rs
├── ray_geometry.rs
├── shape.rs
├── tests.rs
└── vertex_layout.rs
```

Expected result:

- Mesh registry lifetime and handle logic are separate from upload/layout
  helpers.
- CPU ray geometry extraction is isolated from normal renderability.
- Existing public mesh resource exports stay curated through
  `resources/mesh/mod.rs`.

Validation:

```powershell
cargo fmt
cargo check --features app
cargo check --examples --features app
cargo test --features app render::mesh
```

### Phase 6: Rename Frame Stage Files

Make `src/render/runtime/frame/` read like a recipe.

Target renames:

```text
inputs.rs                -> collect_frame_inputs.rs
extract.rs               -> extract_frame.rs
scene_upload.rs          -> upload_scene.rs
assemble.rs              -> assemble_frame.rs
execute.rs               -> execute_frame.rs
finalize.rs              -> finish_frame.rs
resource_preparation.rs  -> prepare_frame_resources.rs
```

Expected result:

- `frame_coordinator.rs` reads as a short explicit frame recipe.
- Frame stage names use action verbs consistently.

Validation:

```powershell
cargo fmt
cargo check --features app
cargo check --examples --features app
cargo test --features app render::runtime::tests -- --test-threads=1
```

### Phase 7: Split Large Runtime Tests

Split `src/render/runtime/tests.rs` into the `src/render/runtime/tests/`
module directory.

Target layout:

```text
src/render/runtime/tests/
├── mod.rs
├── common.rs
├── custom_materials.rs
├── custom_steps.rs
├── global_illumination.rs
├── live2d_sorting.rs
├── phase_batching.rs
├── pipeline_order.rs
├── shadows.rs
├── startup.rs
├── texture_assets.rs
└── views.rs
```

Expected result:

- Behavior areas are discoverable by filename.
- GPU-heavy runtime tests can be run by theme.

Validation:

```powershell
cargo fmt
cargo test --features app render::runtime::tests -- --test-threads=1
```

### Phase 8: Split Graph Tests

Split `src/render/graph/tests.rs` into the `src/render/graph/tests/`
module directory after runtime tests are clean.

Target layout:

```text
src/render/graph/tests/
├── mod.rs
├── aliasing.rs
├── common.rs
├── compile_order.rs
├── copy_passes.rs
├── execution.rs
├── handles_and_builders.rs
├── imported_resources.rs
├── pass_outputs.rs
├── physical_allocation.rs
└── validation.rs
```

Validation:

```powershell
cargo fmt
cargo test --features app render::graph
```

## Non-Goals

- Do not rewrite shaders as part of this plan.
- Do not change rendering behavior unless a rename exposes an existing bug.
- Do not tune benchmarks or profile output as part of this plan.
- Do not merge renderer families into one universal scene schema.
- Do not add compatibility shims for old internal module paths.

## Acceptance Checklist

- File and folder names communicate architectural layer and responsibility.
- CPU-side assets, runtime resources, and backend upload bridges are distinct.
- Pipeline declaration code is separate from execution mechanics.
- Runtime owns orchestration, not every step-node implementation.
- Large files that remain large have a clear reason to exist.
- Documentation points to current paths.
- These commands pass:

```powershell
cargo fmt
cargo check --features app
cargo check --examples --features app
cargo test --features app render::pipeline
cargo test --features app render::execution
cargo test --features app render::runtime::tests -- --test-threads=1
```
