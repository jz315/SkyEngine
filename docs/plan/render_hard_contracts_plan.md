# Render Hard Contracts Plan

## Purpose

This plan turns the recent render bug hunt into a structural hardening pass.

The goal is not to add more guidance that engineers must remember. The goal is
to make entire classes of render bugs difficult or impossible to express through
normal APIs. Documentation should explain the rules, but the rules themselves
must live in types, constructors, validated intermediate representations,
lifecycle tokens, and contract tests.

The immediate trigger was a set of bugs and risks found across RenderGraph copy
paths, shadow extraction, phase payload interpretation, backend presentation
lifecycle, skipped-frame handling, tilemap culling, sprite payload packing, and
runtime pipeline churn.

## Design Principle

SkyEngine render should follow the same broad lesson that makes Rust effective:

- invalid states should be unrepresentable where practical;
- dangerous behavior should require an explicit narrow escape hatch;
- resources should move through typed lifecycle states;
- execution code should consume validated objects instead of revalidating loose
  declarations;
- public API semantics should not be silently weakened by internal compression;
- contract tests should pin every historical failure mode.

This is not a plan to make every local variable a typestate object. The hard
constraints should sit at architectural boundaries: RenderGraph compile,
renderer frame lifecycle, extraction scheduling, phase payload dispatch,
resource construction, and public-to-internal API conversion.

## Current Failure Classes

### RenderGraph Copy Contracts

Current bug class:

- buffers infer `COPY_SRC` / `COPY_DST` usage from copy passes;
- textures do not have an equivalent inferred usage path;
- texture copy validation checks format and extent, but misses texture usage and
  MSAA restrictions;
- `BufferToTexture` layout validation rejects some legal single-row or
  sparse-row copies.

Why it happened:

The graph has separate validation/allocation/execution paths, and only buffers
got a complete inferred-usage contract. Texture copies were allowed to pass
through as loose handles until wgpu validation.

Hard-contract target:

- compile copy declarations into `ValidatedCopyOp`;
- allocation consumes inferred texture usage, not raw declared usage;
- execute consumes only `ValidatedCopyOp`;
- missing usage, unsupported sample counts, incompatible formats, bad extents,
  and invalid layouts fail before command recording.

### Renderer Frame Lifecycle

Current bug class:

- `SceneRenderer` exposes loose `begin_frame()`, `render_world()`, and
  `end_frame()` calls;
- alternate backends can acquire or present inside `render_world()`;
- app-level `pre_present_notify()`, screenshots, overlays, and end-frame hooks
  can become incorrectly ordered.

Why it happened:

The backend trait documents lifecycle order but does not encode it. Each backend
can implement the three methods with different ownership semantics.

Hard-contract target:

- `begin_frame()` returns a frame token/session;
- `render_world()` requires a mutable frame token;
- `end_frame()` consumes the frame token and is the only present path;
- app-owned hooks run in the single present path, not by convention around it.

### Skipped Draw Frames

Current bug class:

- if material or asset preparation fails after a surface frame is acquired, the
  runtime can return without drawing;
- the app still screenshots, calls pre-present, submits, and presents.

Why it happened:

`RenderRuntime::render_world()` currently returns no frame outcome. The runner
cannot distinguish "drawn", "skipped after acquire", and "failed with a surface
that needs cleanup".

Hard-contract target:

- render execution returns a `FrameRenderOutcome`;
- acquired but undrawn frames must be explicitly handled by clear, skip-present,
  or error recovery policy;
- screenshot and overlay paths can require a `DrawnFrame` or `ClearedFrame`
  state rather than a generic active frame.

### View-Kind Extraction

Current bug class:

- sprite extraction runs for shadow views even though sprite shadow casting is
  not currently implemented;
- tilemap extraction manually skips shadow views;
- the scheduler relies on each extractor remembering view-kind rules.

Why it happened:

The extractor trait receives every `SceneView` and makes filtering a local
convention.

Hard-contract target:

- extractors declare supported view kinds;
- the extraction scheduler filters by capability before calling them;
- unsupported view kinds are unreachable through the normal extractor callback.

### Phase Payload Type Safety

Current bug class:

- `TransparentPhase` can contain multiple payload types;
- shadow execution reads transparent items as `MeshDrawData`;
- sprite transparent items can be interpreted through the wrong payload type.

Why it happened:

Phase items store a fixed byte payload without a payload-kind contract at the
execution boundary. Sorting and batching are generic, but execution assumes a
family-specific payload.

Hard-contract target:

- phase buckets are grouped by draw function and payload kind before execution;
- draw functions declare their payload type;
- execution receives typed item slices;
- mismatched payload access is impossible in normal draw code.

### Public API Data Fidelity

Current bug class:

- `SpriteRenderer` exposes `f32` size and UV semantics;
- `SpriteDrawData` silently clamps/rounds size to integer `0..4096` and UVs to
  normalized `0..1`.

Why it happened:

The internal payload optimized for compact storage without a type-level or
public API contract that said the data would become packed and lossy.

Hard-contract target:

- public `f32` sprite data remains `f32` through extraction and draw
  preparation; or
- public API changes to explicit packed/normalized types with fallible
  constructors;
- silent lossy conversion is disallowed.

### Tilemap Bounds

Current bug class:

- tilemap culling uses a fixed renderer tile draw size;
- actual instances may use per-tile draw sizes from tileset rect metadata;
- large image-collection tiles can be culled at viewport edges.

Why it happened:

The culling boundary used renderer-level defaults while instance generation used
tile-level metadata. Bounds and instance generation drifted.

Hard-contract target:

- culling uses a `TilemapChunkBounds` computed by the same source of truth as
  instance generation;
- tilesets expose conservative max draw extents or chunk-local bounds;
- tests cover per-tile sizes larger than defaults.

### Runtime Pipeline Rebuild

Current risk class:

- runtime execution currently rebuilds a `FramePipeline` and presentation node
  per frame;
- presentation node creation creates GPU layout/pipeline objects.

Why it happened:

The runtime treats the frame pipeline as a cheap execution skeleton even though
some nodes own GPU resources.

Hard-contract target:

- per-frame execution uses cached node resources;
- resize/surface-format changes invalidate only the resources that depend on
  them;
- frame execution cannot accidentally allocate presentation pipelines every
  frame.

## Target Architecture

The render stack should separate three layers.

### Declaration Layer

This is the ergonomic API:

- `RenderPipelineAsset`;
- `RenderPipelineBuilder`;
- render features;
- extractor registration;
- pass and resource declarations.

This layer may remain flexible and friendly. It is allowed to describe an
invalid graph or incomplete renderer setup because it is not the execution
contract.

### Validation And Compilation Layer

This layer turns declarations into hard contracts:

- `ValidatedFramePlan`;
- `ValidatedCopyOp`;
- inferred texture and buffer usage plans;
- view-kind extractor schedules;
- typed phase buckets;
- renderer frame policy;
- cached runtime node resources.

This is where most errors should be found.

### Execution Layer

This layer only consumes validated data:

- no loose copy handles;
- no untyped phase payload reads;
- no backend-controlled present timing;
- no silent skipped frame present;
- no per-frame GPU object creation unless explicitly marked transient.

Execution should be boring. If it has to ask "is this legal?", the validation
layer probably failed to do its job.

## Proposed Milestones

### Milestone 1: RenderGraph Copy Validation

Deliverables:

- add texture inferred usage equivalent to `buffer_usage_for()`;
- define `ValidatedCopyOp`;
- compile copy passes into validated operations;
- fix `BufferToTexture` required-byte calculation;
- support legal single-row copy layouts;
- reject MSAA texture copies where wgpu copy APIs do not support them.

Contract tests:

- texture-to-texture with explicit usage missing `COPY_SRC` / `COPY_DST`;
- upload-to-texture with explicit usage missing `COPY_DST`;
- buffer-to-texture single-row default layout;
- buffer-to-texture explicit `rows_per_image > height` single-layer copy;
- texture-to-texture with `sample_count > 1`;
- current graph copy tests continue to pass.

Expected verification:

```text
cargo test --features app graph
```

### Milestone 2: View-Kind Extraction Schedule

Deliverables:

- add extractor view-kind capability declaration;
- scheduler filters unsupported view kinds before calling extractors;
- sprite extractor supports main views only unless sprite shadows are
  intentionally implemented;
- tilemap skip logic moves from local convention into scheduler policy.

Contract tests:

- sprite plus directional shadow does not produce sprite transparent items in
  shadow views;
- tilemap-only pipeline with directional shadows does not run tilemap extraction
  for shadow views;
- existing mesh shadow extraction still works.

Expected verification:

```text
cargo test --features app render::extract
cargo test --features app render::runtime::tests::shadows
```

### Milestone 3: Typed Phase Payload Dispatch

Deliverables:

- make draw functions declare a payload type or payload kind;
- group phase items into typed buckets before execution;
- shadow mesh path consumes only mesh payload buckets;
- sprite draw path consumes only sprite payload buckets.

Contract tests:

- mixed sprite and mesh transparent phase cannot be consumed as all mesh;
- wrong payload kind is rejected before execution or is unrepresentable;
- existing phase batching tests still pass.

Expected verification:

```text
cargo test --features app render::phase
cargo test --features app render::runtime::tests::phase_batching
```

### Milestone 4: Shadow Transparent Pass Contract

Deliverables:

- transparent shadow pass setup only requires transparent mesh caster support
  when transparent mesh casters exist;
- `StandardMaterial` requirement is tied to the mesh transparent shadow path,
  not to the existence of a shadow view;
- transparent atlas clear remains valid without transparent casters.

Contract tests:

- directional shadow phase in a no-`StandardMaterial` pipeline does not fail
  when there are no transparent mesh casters;
- transparent `StandardMaterial` caster still writes the transparent shadow
  atlas when registered;
- sprite-only and tilemap-only scenes with directional light do not trigger
  `StandardMaterial` errors.

Expected verification:

```text
cargo test --features app render::runtime::tests::shadows
```

### Milestone 5: Renderer Frame Token

Deliverables:

- introduce a renderer frame/session token;
- make render and present operations require the token;
- move app pre-present notification into the single present path;
- adapt wgpu, Kajiya, and Renderling backends to the same lifecycle;
- Renderling frame acquisition and present move out of `render_world()`.

Contract tests:

- backend lifecycle event log proves `begin -> render -> pre_present -> end/present`;
- Renderling cannot present before app pre-present hook;
- screenshot and overlay hooks run before present for wgpu;
- resize and surface-lost paths drop or rebuild frame state safely.

Expected verification:

```text
cargo test --features app render::backend
cargo check --examples --features app
```

### Milestone 6: Frame Outcome Handling

Deliverables:

- `RenderRuntime::render_world` or backend render call returns a
  `FrameRenderOutcome`;
- asset/material preparation failure produces an explicit skipped/failed
  outcome;
- acquired frames are either drawn, cleared, or not presented;
- screenshots require a presentable drawn/cleared surface.

Contract tests:

- material preparation failure after acquire does not present undefined surface;
- skipped frame either clears to configured color or reports no screenshot;
- overlay rendering does not load from undefined surface contents.

Expected verification:

```text
cargo test --features app render::runtime::tests
```

### Milestone 7: Public Data Fidelity

Deliverables:

- replace lossy `SpriteDrawData` packing for size and UVs with lossless `f32`
  data; or
- introduce explicit packed public types and fallible conversion.

Preferred direction:

Keep public `SpriteRenderer` semantics and make internal payload lossless.

Contract tests:

- fractional sprite size survives extraction;
- size greater than 4096 survives extraction;
- negative size semantics are either preserved or rejected explicitly;
- UV values outside `0..1` survive if wrapping/overscan remains supported.

Expected verification:

```text
cargo test --features app render::extract sprite
cargo test --features app render::runtime::tests::phase_batching
```

### Milestone 8: Tilemap Bounds Source Of Truth

Deliverables:

- compute conservative chunk bounds from tileset draw extents or chunk-local
  tile metadata;
- culling and instance generation use the same tile-size source;
- support image-collection tiles larger than default tile size.

Contract tests:

- oversized tile at viewport edge remains visible;
- default fixed-grid tilemaps keep current culling behavior;
- isometric and staggered orientations use conservative bounds.

Expected verification:

```text
cargo test --features app render::tilemap
cargo test --features app tile::
```

### Milestone 9: Cached Runtime Execution Resources

Deliverables:

- move presentation node and GPU pipeline ownership out of per-frame local
  construction;
- cache reusable `FramePipeline` node resources or split node resources from
  frame-local graph state;
- invalidate cached resources only on surface format, feature set, or resize
  changes as needed.

Contract tests:

- two consecutive frames do not recreate viewport blit shader/pipeline;
- graph transient pools persist across frames where intended;
- resize invalidates only size-dependent resources.

Expected verification:

```text
cargo test --features app render::runtime::tests
cargo check --examples --features app
```

## API And Type Guidelines

### Prefer Validated IR Over Runtime Guessing

If a declaration can be wrong, compile it into a validated representation before
execution.

Examples:

- `CopyOp` -> `ValidatedCopyOp`;
- loose extractor list -> view-kind filtered schedule;
- mixed phase item list -> typed phase buckets;
- renderer declarations -> frame lifecycle protocol.

### Avoid Dangerous Replacement APIs In Friendly Builders

APIs like `TextureBuilder::usage(...)` replace the complete usage mask and make
it easy to remove required flags. Friendly APIs should prefer additive methods.

If full replacement is needed, move it behind an expert-facing name or require
explicit validation.

### Make Escape Hatches Visibly Different

Escape hatches should be named and scoped like expert operations. They should
not be the easiest path.

Examples:

- `expert_usage_exact(...)`;
- `unchecked_imported_texture(...)`;
- `raw_phase_item(...)`.

Each escape hatch should either be `unsafe`, fallible, or documented as
expert-only with contract tests around the safe path.

### Keep Type Constraints At Boundaries

Do not spread heavy generic typestate into every internal helper. Put hard
constraints where data crosses a boundary:

- builder -> compiled plan;
- app -> renderer backend;
- extractor scheduler -> extractor implementation;
- phase sorter -> draw execution;
- CPU asset -> GPU resource.

## Test Standard

Every fixed bug gets a contract test named after the invariant, not the
implementation detail.

Good names:

- `texture_copy_infers_required_usage`;
- `single_row_buffer_to_texture_copy_accepts_tight_layout`;
- `sprite_extractor_is_not_called_for_shadow_views`;
- `renderling_present_runs_after_pre_present_notify`;
- `skipped_material_frame_does_not_present_undefined_surface`;
- `sprite_draw_data_preserves_fractional_size`;
- `tilemap_culling_uses_tileset_draw_extents`.

Avoid tests that merely assert current helper names or private layout unless the
private layout is itself the contract.

## CI Gate Proposal

Create or tag a contract-test set that runs in normal render CI:

```text
cargo test --features app graph
cargo test --features app render::extract
cargo test --features app render::runtime::tests
cargo test --features app render::backend
cargo test --features app render::tilemap
cargo check --examples --features app
```

If these are too slow for every local edit, keep a smaller pre-push set and make
the full set required before merging render changes.

## Migration Notes

- Breaking API changes are acceptable if they remove ambiguous lifecycle or
  payload semantics.
- Examples should be migrated to the new safe path, not patched around old
  names.
- `src/main.rs` remains scratch and should not drive compatibility decisions.
- User-owned dirty worktree changes must not be reverted during implementation.
- Each milestone should compile before starting the next one.

## Non-Goals

- This plan does not rewrite shaders for visual improvements.
- This plan does not introduce multi-queue async compute.
- This plan does not require a universal scene schema for all renderer
  backends.
- This plan does not attempt to make every render helper public.
- This plan does not preserve misleading names if they block hard contracts.

## Definition Of Done

The plan is complete when:

- RenderGraph copy operations cannot reach wgpu with missing required usage or
  invalid layouts through safe APIs;
- backend present order is enforced by frame-token ownership;
- skipped draw frames are explicit and cannot present undefined surface content;
- unsupported extractors cannot run for unsupported view kinds;
- phase execution cannot read payloads as the wrong type through normal APIs;
- sprite public data semantics are preserved or explicitly rejected;
- tilemap culling uses the same draw-size truth as instance generation;
- runtime GPU pipeline creation no longer happens accidentally every frame;
- all contract tests listed by the implemented milestones pass.
