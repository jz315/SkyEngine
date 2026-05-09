# Shadow System Direct Rewrite Plan

## Position

This is a direct rewrite plan for SkyEngine's shadow system.

No compatibility layer is required. The current directional shadow view
construction, runtime binding storage, render phase implementation, material
shadow hooks, debug toggles, and shader include shape can be broken freely if the
replacement is cleaner.

The target is not to tune the current implementation until artifacts disappear.
The target is to replace it with a shadow architecture whose contracts are
explicit enough for directional CSM, transparent shadows, contact shadows,
future spot/point shadows, custom materials, debug readbacks, and RenderGraph
integration to share one path without hidden coupling.

This plan complements:

- `docs/plan/wicked_shadow_gap_closure_plan.md`: correctness gap closure against
  WickedEngine-style directional shadows.
- `docs/plan/material_resource_split_plan.md`: material model rewrite that lets
  materials declare shadow participation instead of being special-cased inside
  shadow phases.

## Current Diagnosis

The current implementation already has many modern pieces:

- directional cascaded shadow maps;
- a directional shadow atlas;
- atlas metadata and guard-band clamp;
- one `SceneView` per active cascade;
- fixed PCF, dithered PCF, and PCSS;
- transparent shadow transmittance;
- alpha-tested shadow casters;
- material receive/cast flags;
- contact shadow post-fx;
- cascade debug and raw atlas debug;
- static-when-unchanged cascade update policy;
- render tests that assert coarse atlas and color behavior.

The problem is that the feature set is modern, but the contracts are not modern
enough. The current boundaries are still too soft.

Current high-risk coupling:

- `src/render/lighting/shadow/view.rs`
  - selects shadow lights;
  - computes cascade splits and light-space fitting;
  - creates shadow `SceneView`s;
  - owns `ShadowViewBinding`;
  - resizes GPU render targets;
  - writes shadow uniforms;
  - computes cascade update signatures;
  - counts casters from opaque/transparent phases;
  - emits debug logging.
- `src/render/lighting/shadow/phase.rs`
  - publishes RenderGraph resources;
  - clears atlas rectangles;
  - classifies opaque/alpha-test/transparent shadow casters;
  - knows about `StandardMaterial`;
  - creates shadow pipelines;
  - creates material bind groups;
  - uploads per-pass instance buffers;
  - executes both opaque and transparent shadow passes.
- `src/render/shaders/materials/standard_material.wgsl` and
  `src/render/shaders/materials/standard_material_normal_mapped.wgsl`
  duplicate the shadow sampling implementation.
- `src/render/shaders/postfx/contact_shadows.wgsl` can create artifacts that
  look like CSM/filter/bias artifacts unless debug isolation is enforced.
- RenderGraph shadow texture handles and runtime shadow bind groups are
  published through separate paths that must stay manually synchronized.

The result is a system where one visual artifact can originate in cascade
fitting, caster culling, receiver precision, raster bias, shader compare bias,
PCSS spread, transparent shadow composition, contact shadows, material flags,
RenderGraph ordering, or stale update masks.

## Non-Goals

- Do not preserve old shadow internals.
- Do not preserve old `ShadowViewBinding` shape.
- Do not preserve `DirectionalShadowPhase` internals.
- Do not preserve the current `StandardMaterial` special casing in shadow code.
- Do not keep duplicate shadow shader logic in every material shader.
- Do not tune PCSS/contact shadows before the base CSM contract is validated.
- Do not add spot or point shadows before directional shadows use the new path.
- Do not rewrite unrelated GI, bloom, TAA, app, or ECS systems in this pass.

## Best-Practice Baseline

Use these principles as architecture constraints:

- CSM must be stable before filtering is judged.
- Filtering is separate from projection quality. PCF/PCSS cannot fix bad cascade
  fitting, wrong caster ranges, or mismatched depth conventions.
- Bias must be documented in explicit units.
- Shadow passes should publish explicit graph resources, and sampling passes
  should declare exact reads.
- Materials should declare pass participation. Shadow phases should not need to
  know concrete material types.
- Contact shadows are an optional close-range supplement, not a replacement for
  valid shadow maps.
- Debug should make each layer independently inspectable:
  - raw depth atlas;
  - cascade coverage;
  - receiver bias;
  - caster bounds;
  - PCF/PCSS mode;
  - transparent shadow atlas;
  - contact shadow mask.

References:

- Microsoft Cascaded Shadow Maps:
  https://learn.microsoft.com/en-us/windows/win32/dxtecharts/cascaded-shadow-maps
- Microsoft Depth Bias:
  https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d10-graphics-programming-guide-output-merger-stage-depth-bias
- NVIDIA GPU Gems 3, Parallel-Split Shadow Maps:
  https://developer.nvidia.com/gpugems/gpugems3/part-ii-light-and-shadows/chapter-10-parallel-split-shadow-maps-programmable-gpus
- NVIDIA GPU Gems 3, Shadow Map Filtering:
  https://developer.nvidia.com/gpugems/gpugems3/part-ii-light-and-shadows/chapter-8-summed-area-variance-shadow-maps
- Filament FrameGraph:
  https://google.github.io/filament/notes/framegraph.html
- Filament Materials:
  https://google.github.io/filament/Materials.md.html
- WickedEngine Transparent Shadow Maps:
  https://wickedengine.net/2018/01/easy-transparent-shadow-maps/

## End State

The shadow system should become a focused module tree:

```text
src/render/lighting/shadow/
├── mod.rs
├── atlas.rs
├── bindings.rs
├── debug.rs
├── extract.rs
├── frame.rs
├── graph.rs
├── phase.rs
├── plan.rs
├── resources.rs
├── shaders.rs
├── update.rs
└── view.rs
```

Expected ownership:

- `atlas.rs`
  - atlas packing;
  - cascade/spot rect metadata;
  - guard-band policy;
  - atlas statistics.
- `bindings.rs`
  - bind group layouts;
  - uniform structs;
  - bind group creation helpers only.
- `debug.rs`
  - debug modes;
  - diagnostic structs;
  - logging formatting;
  - readback helpers if they stay in this module.
- `extract.rs`
  - shadow caster extraction;
  - receiver bounds extraction if needed;
  - material shadow mode resolution;
  - batch grouping inputs.
- `frame.rs`
  - `ShadowFrame`;
  - per-frame shadow resources;
  - prepared shadow payloads attached to `PreparedFrame` / `PreparedView`.
- `graph.rs`
  - RenderGraph publication of shadow resources;
  - graph texture/import keys;
  - exact resource read/write declaration helpers.
- `phase.rs`
  - thin render phase execution;
  - consumes a prepared `ShadowFramePlan`;
  - does not compute cascade math;
  - does not know concrete material types.
- `plan.rs`
  - pure CPU directional shadow planning;
  - cascade splits;
  - light-space fitting;
  - receiver and caster depth extents;
  - texel snapping;
  - bias unit derivation;
  - no GPU objects.
- `resources.rs`
  - shadow atlas render targets;
  - persistent GPU resources;
  - update masks;
  - target resize policy.
- `shaders.rs`
  - shader source selection;
  - shared shadow WGSL module strings;
  - shadow pipeline shader variants.
- `update.rs`
  - static/dynamic update policy;
  - cascade signatures;
  - dirty checks.
- `view.rs`
  - conversion between planned shadow cameras and `SceneView`;
  - no resource mutation.

## Core Contract

### ShadowFramePlan

`ShadowFramePlan` is the single source of truth for shadow behavior in a frame.

Candidate shape:

```rust
pub(crate) struct ShadowFramePlan {
    pub(crate) directional: Vec<DirectionalShadowPlan>,
    pub(crate) atlas: ShadowAtlasPlan,
    pub(crate) diagnostics: ShadowDiagnostics,
}
```

Rules:

- built before GPU resource sync;
- deterministic for the same camera/light/caster inputs;
- contains no `wgpu` objects;
- records all cascade metadata needed by shaders and tests;
- can be unit-tested without a GPU;
- can be dumped to logs or debug UI.

### DirectionalShadowPlan

Candidate shape:

```rust
pub(crate) struct DirectionalShadowPlan {
    pub(crate) binding_index: usize,
    pub(crate) light_entity: Option<EntityId>,
    pub(crate) light_direction: [f32; 3],
    pub(crate) cascade_count: u32,
    pub(crate) cascades: [CascadeShadowPlan; MAX_DIRECTIONAL_SHADOW_CASCADES],
    pub(crate) sampling: ShadowSamplingPlan,
    pub(crate) bias: ShadowBiasPlan,
    pub(crate) update_policy: ShadowUpdatePolicy,
}
```

Rules:

- one plan per main view that has an active directional shadow light;
- plan selection is explicit and deterministic;
- no plan is produced for disabled/invisible lights;
- layer masks are resolved during planning.

### CascadeShadowPlan

Candidate shape:

```rust
pub(crate) struct CascadeShadowPlan {
    pub(crate) cascade_index: u32,
    pub(crate) split_near: f32,
    pub(crate) split_far: f32,
    pub(crate) receiver_view_proj: [f32; 16],
    pub(crate) raster_view_proj: [f32; 16],
    pub(crate) culling_view_proj: [f32; 16],
    pub(crate) light_view: [f32; 16],
    pub(crate) atlas_rect: ShadowAtlasRect,
    pub(crate) world_extent: [f32; 2],
    pub(crate) texel_world_size: [f32; 2],
    pub(crate) receiver_depth_extent: ShadowDepthExtent,
    pub(crate) caster_depth_extent: ShadowDepthExtent,
    pub(crate) caster_count: usize,
    pub(crate) update_signature: u64,
}
```

Rules:

- receiver projection is for shader depth comparison;
- raster projection is for shadow depth output;
- culling projection is for caster visibility;
- the three projections may be equal, but their roles are explicit;
- snapping must never shrink receiver coverage;
- every split corner must remain inside the receiver projection after snapping.

### ShadowBiasPlan

Candidate shape:

```rust
pub(crate) struct ShadowBiasPlan {
    pub(crate) raster_constant: i32,
    pub(crate) raster_slope_scale: f32,
    pub(crate) raster_clamp: f32,
    pub(crate) normal_bias_world: f32,
    pub(crate) min_normal_bias_texels: f32,
    pub(crate) compare_bias_depth: f32,
    pub(crate) compare_bias_texels: f32,
}
```

Rules:

- normal bias is in world units;
- compare bias is in normalized shadow depth units;
- texel-derived bias is computed from cascade texel size and depth extent;
- raster bias remains wgpu/API-level state, documented separately;
- shader receives explicit values and does not invent hidden magic constants.

### ShadowCasterBatch

Candidate shape:

```rust
pub(crate) struct ShadowCasterBatch {
    pub(crate) mesh: MeshHandle,
    pub(crate) sub_mesh_index: u32,
    pub(crate) material: ErasedMaterialHandle,
    pub(crate) pass_mode: ShadowPassMode,
    pub(crate) pipeline_key: ShadowPipelineKey,
    pub(crate) model_slots: Range<u32>,
}
```

Rules:

- created during extraction;
- sorted and batched before phase execution;
- contains material pass mode resolved through material interfaces;
- phase execution does not query `TypeId::of::<StandardMaterial>()`;
- transparent shadow batches are separate from opaque/alpha-test batches.

### ShadowPassMode

This should be owned by the material model rewrite, but shadow uses it as a
contract:

```rust
pub enum ShadowPassMode {
    None,
    Opaque,
    AlphaTest,
    Transparent,
}
```

Rules:

- unlit defaults to `None`;
- sprite defaults to `None`;
- standard opaque defaults to `Opaque`;
- standard alpha-mask uses `AlphaTest`;
- alpha-blend uses `Transparent` only when transparent shadow casting is
  explicitly enabled;
- custom materials declare this through `MaterialInterface`.

## Shader Strategy

The current standard and normal-mapped standard shaders duplicate shadow helper
functions. Replace this with a shared source unit.

Target source layout:

```text
src/render/shaders/lighting/
├── shadow_common.wgsl
├── shadow_depth.wgsl
├── shadow_depth_alpha_test.wgsl
└── shadow_transparent.wgsl
```

Rules:

- material shaders include or compose the same shadow sampling source;
- normal-mapped materials use geometric normals for receiver bias;
- normal maps affect lighting, not receiver-position bias;
- cascade selection uses the unbiased receiver position;
- cascade projection uses the biased receiver position;
- filter spread is derived from:
  - filter radius in world units;
  - cascade texel world size;
  - atlas reciprocal size;
- PCSS blocker search and PCF filtering share the same border clamp contract.

If WGSL source composition remains string-based, add a small shader source
builder in `shaders.rs` instead of duplicating helper code manually.

## RenderGraph Strategy

Shadow graph resources should be explicit.

Candidate shape:

```rust
pub(crate) struct ShadowGraphResources {
    pub(crate) binding_index: usize,
    pub(crate) depth_atlas: TextureHandle,
    pub(crate) transparent_atlas: Option<TextureHandle>,
    pub(crate) debug_atlas: Option<TextureHandle>,
}
```

Rules:

- shadow setup imports or creates atlas resources once per shadow binding;
- every cascade pass writes only its atlas rect;
- opaque/transparent/main phases read the exact texture handles published by
  shadow setup;
- skipped static cascades still publish reusable resources;
- debug passes read the same resources as production sampling;
- no sampling pass guesses texture slot names.

## Debug Strategy

Debug must isolate the system into layers.

Required debug modes:

- `ShadowOff`
- `ShadowCsmOnly`
- `ShadowFixedPcf`
- `ShadowPcss`
- `ShadowCascadeCoverage`
- `ShadowRawAtlas`
- `ShadowTransparentAtlas`
- `ShadowContactOnly`
- `ShadowContactOff`
- `ShadowBiasHeatmap`
- `ShadowCasterBounds`

Required diagnostics:

```rust
pub(crate) struct ShadowDiagnostics {
    pub(crate) directional_light_count: usize,
    pub(crate) active_directional_shadow_count: usize,
    pub(crate) cascade_count: usize,
    pub(crate) caster_count: usize,
    pub(crate) updated_cascade_count: usize,
    pub(crate) skipped_static_cascade_count: usize,
    pub(crate) atlas_size: [u32; 2],
    pub(crate) atlas_used_ratio: f32,
    pub(crate) warnings: Vec<ShadowDiagnosticWarning>,
}
```

Warnings should include:

- no active shadow light;
- no casters;
- cascade receiver coverage failed;
- caster culling range is much larger than receiver range;
- compare bias exceeds a threshold;
- PCSS kernel exceeds guard-band-safe range;
- material declares shadow pass but lacks required vertex attributes;
- transparent shadow enabled without transparent atlas support.

## Execution Plan

### Phase 0: Freeze Baseline And Add Rewrite Guardrails

Purpose: make current behavior inspectable before replacement.

Tasks:

- Add a short current-state note with the canonical bad-shadow repro.
- Add an env/debug preset for:
  - CSM only;
  - fixed PCF;
  - contact shadows off;
  - TAA off;
  - GI off.
- Run current shadow tests and record failures or flaky cases.
- Add TODO markers that identify temporary old-path entry points.

Files:

- `docs/render_deep_dive.md` or `docs/plan/wicked_shadow_debug_notes.md`
- `examples/render/three_d_demo.rs`
- `src/render/component/settings.rs`

Acceptance:

- one canonical repro exists;
- old behavior can be compared against the rewrite;
- no architecture changes yet.

### Phase 1: Introduce Pure Shadow Planning

Purpose: extract cascade math and bias derivation out of runtime GPU sync.

Tasks:

- Create `src/render/lighting/shadow/plan.rs`.
- Move pure helpers from `view.rs`:
  - cascade count resolution;
  - cascade split resolution;
  - frustum corner reconstruction;
  - light view basis;
  - cascade fitting;
  - texel snapping.
- Add `ShadowFramePlan`, `DirectionalShadowPlan`, and `CascadeShadowPlan`.
- Separate receiver, raster, and culling projections.
- Add `ShadowBiasPlan`.
- Make planning take explicit input structs instead of `World` directly where
  practical.

Files:

- `src/render/lighting/shadow/plan.rs`
- `src/render/lighting/shadow/view.rs`
- `src/render/lighting/shadow/mod.rs`

Tests:

- split corners remain inside receiver projection after snapping;
- snapping never shrinks coverage;
- light basis does not flip for near-vertical light directions;
- receiver depth extent stays finite;
- caster depth extent can include off-slice casters;
- invalid light direction produces no plan.

Acceptance:

- cascade planning can be unit-tested without a GPU;
- `view.rs` no longer owns most shadow math;
- old runtime still renders by adapting from the new plan if needed.

### Phase 2: Introduce Shadow Extraction And Pass Modes

Purpose: stop shadow phases from discovering material behavior at draw time.

Tasks:

- Create `src/render/lighting/shadow/extract.rs`.
- Add `ShadowPassMode` to the material rewrite target API, or add a temporary
  internal equivalent while material rewrite is in flight.
- Extract shadow caster records before phase execution.
- Resolve each caster's shadow pass mode through material interface data.
- Group opaque, alpha-test, and transparent casters into explicit batches.
- Remove `TypeId::of::<StandardMaterial>()` from the new extraction path.

Files:

- `src/render/lighting/shadow/extract.rs`
- `src/render/resources/material/*`
- `src/render/phase/*`
- `src/render/runtime/frame_builder.rs`

Tests:

- opaque standard material becomes `ShadowPassMode::Opaque`;
- alpha-mask standard material becomes `ShadowPassMode::AlphaTest`;
- transparent material becomes `ShadowPassMode::Transparent` only when enabled;
- unlit and sprite default to `None`;
- custom material can opt into alpha-test shadow without editing shadow phase;
- missing UV for alpha-test produces a precise extraction/pipeline error.

Acceptance:

- shadow phase consumes typed batches;
- shadow phase does not inspect concrete material types;
- material shadow participation is visible in diagnostics.

### Phase 3: Rewrite Shadow Resources

Purpose: move persistent GPU resource ownership out of view planning.

Tasks:

- Create or rewrite `src/render/lighting/shadow/resources.rs`.
- Replace `ShadowViewBinding` with resource structs aligned to
  `ShadowFramePlan`.
- Own:
  - depth atlas target;
  - optional transparent atlas target;
  - shadow scene bind group;
  - shadow pass bind groups;
  - uniform buffers;
  - previous update signatures.
- Keep resize policy local to resources.
- Keep update-mask computation local to `update.rs`.

Files:

- `src/render/lighting/shadow/resources.rs`
- `src/render/lighting/shadow/update.rs`
- `src/render/lighting/shadow/bindings.rs`
- `src/render/runtime/state.rs`

Tests:

- atlas resizes when cascade count or resolution changes;
- static cascades skip update but keep resources published;
- changing caster signature marks only affected cascades;
- disabled shadows publish disabled resources with no stale bind group use.

Acceptance:

- `view.rs` does not resize targets or write buffers;
- resource state can be inspected independently from cascade planning;
- update policy is testable without running the render phase.

### Phase 4: Rewrite RenderGraph Publication

Purpose: make graph dependencies explicit and centralized.

Tasks:

- Create `src/render/lighting/shadow/graph.rs`.
- Publish `ShadowGraphResources` from shadow setup.
- Make main opaque passes read shadow resources by handle, not by slot name.
- Ensure transparent shadow atlas reads/writes are represented exactly.
- Keep skipped static cascades graph-visible without adding write passes.

Files:

- `src/render/lighting/shadow/graph.rs`
- `src/render/lighting/shadow/phase.rs`
- `src/render/runtime/nodes.rs`
- `src/render/execution/slots.rs`

Tests:

- opaque pass reads exact depth atlas handle;
- opaque pass reads exact transparent atlas handle when transparent shadows are
  enabled;
- clean static cascades publish graph resources but add no write pass;
- graph compile order keeps shadow writes before shadow reads.

Acceptance:

- shadow graph publication is not spread across runtime nodes and phase code;
- missing resources fail with clear errors.

### Phase 5: Thin Directional Shadow Phase

Purpose: replace the monolithic phase with a renderer for prepared batches.

Tasks:

- Rewrite `DirectionalShadowPhase`.
- Keep only:
  - clear atlas rect;
  - bind shadow pass resources;
  - bind prepared material shadow resources;
  - draw prepared batches.
- Move pipeline key creation into a shadow pipeline helper.
- Remove direct material bind group creation from the phase where prepared
  materials can provide it.
- Use `ShadowCasterBatch` for batching.

Files:

- `src/render/lighting/shadow/phase.rs`
- `src/render/lighting/shadow/shaders.rs`
- `src/render/resources/material/pipeline.rs`
- `src/render/resources/material/pass.rs`

Tests:

- depth-only opaque caster renders;
- alpha-test caster discards below threshold;
- transparent caster modulates transparent atlas;
- pipeline cache key changes for pass mode and vertex layout;
- pipeline cache key does not change for material instance color values.

Acceptance:

- `phase.rs` becomes execution-focused;
- no `StandardMaterial` special cases remain in the phase;
- shadow draw-call stats are derived from batches.

### Phase 6: Shared Shadow Shader Module

Purpose: eliminate duplicated shadow sampling code.

Tasks:

- Add `src/render/shaders/lighting/shadow_common.wgsl`.
- Compose standard and normal-mapped standard shaders with shared shadow
  helpers.
- Keep shader struct layout synchronized with Rust `ShadowUniform`.
- Move PCF/PCSS/filter/border helpers into shared source.
- Ensure normal-mapped materials use geometric normals for receiver bias.

Files:

- `src/render/shaders/lighting/shadow_common.wgsl`
- `src/render/shaders/materials/standard_material.wgsl`
- `src/render/shaders/materials/standard_material_normal_mapped.wgsl`
- `src/render/lighting/shadow/shaders.rs`

Tests:

- shader source builder includes exactly one shadow helper source;
- standard and normal-mapped variants compile;
- normal map does not alter receiver-position bias;
- cascade selection uses unbiased world position.

Acceptance:

- no duplicated shadow sampling blocks remain in built-in material shaders;
- changes to shadow sampling happen in one file.

### Phase 7: Contact Shadow Isolation

Purpose: keep screen-space contact shadows from hiding CSM regressions.

Tasks:

- Treat contact shadows as a separate post-fx layer with its own debug resource.
- Add a contact-only debug mode.
- Add contact-off diagnostic preset.
- Verify view-space light direction convention.
- Verify depth reconstruction and thickness units.
- Consider replacing geometric stepping with a Wicked-like linear/dithered step
  policy after CSM is stable.

Files:

- `src/render/postfx/contact_shadows.rs`
- `src/render/shaders/postfx/contact_shadows.wgsl`
- `src/render/component/settings.rs`
- `examples/render/three_d_demo.rs`

Tests:

- contact disabled leaves CSM unchanged;
- contact enabled only affects close-range regions;
- flat floor does not produce regular banding;
- contact-only debug output is non-empty in a controlled scene.

Acceptance:

- contact shadows can no longer be mistaken for base CSM behavior during debug.

### Phase 8: Automated Readback And Visual Regression

Purpose: make shadow defects fail tests or produce clear diagnostics.

Tasks:

- Add headless readback tests for simple caster/receiver scenes.
- Read depth atlas and final color.
- Assert:
  - atlas has non-clear depth;
  - receiver darkens when shadows are enabled;
  - receive-shadow disabled material does not darken;
  - alpha-test caster respects alpha threshold;
  - transparent caster affects transparent atlas;
  - cascade 0 has higher effective detail than farther cascades.
- Add optional dump helpers for debug artifacts.

Files:

- `src/render/runtime/tests.rs`
- `src/render/lighting/shadow/debug.rs`
- `src/render/lighting/shadow/resources.rs`

Tests:

```powershell
cargo test --features app shadow -- --nocapture
cargo test --features app standard_material -- --nocapture
cargo test --features app contact_shadows -- --nocapture
cargo check --example three_d_demo --features app
```

Acceptance:

- common regressions are caught numerically;
- failed tests report useful shadow diagnostics.

### Phase 9: Remove Old Path

Purpose: finish the direct rewrite.

Tasks:

- Delete old helpers no longer used by the new plan/resource/phase path.
- Remove temporary adapters.
- Remove old `StandardMaterial` shadow special casing.
- Update `src/render/AGENTS.md`.
- Update examples and docs.
- Run full render compatibility checks.

Files:

- `src/render/lighting/shadow/*`
- `src/render/AGENTS.md`
- `examples/render/*`
- `docs/render_deep_dive.md`

Commands:

```powershell
cargo test --features app shadow -- --nocapture
cargo test --features app render::runtime -- --nocapture
cargo check --examples --features app
```

Acceptance:

- no old shadow path remains;
- `phase.rs` is not a god object;
- shadow behavior is driven by `ShadowFramePlan`;
- material shadow participation is declarative;
- RenderGraph dependencies are explicit;
- debug can isolate CSM, transparent shadow, PCSS, and contact shadow behavior.

## Recommended PR Slices

### PR 1: Pure Plan

- Add `plan.rs`.
- Move pure cascade math.
- Add coverage/snapping/depth tests.
- Keep runtime behavior mostly unchanged.

### PR 2: Extraction Contract

- Add `extract.rs`.
- Add temporary or final `ShadowPassMode`.
- Create caster batches.
- Keep old phase execution behind an adapter if needed.

### PR 3: Resource Ownership

- Replace `ShadowViewBinding` internals with explicit shadow resources.
- Add update signatures and resize tests.

### PR 4: Graph Boundary

- Add `graph.rs`.
- Centralize shadow graph resource publication.
- Make opaque pass read exact handles.

### PR 5: Thin Phase

- Rewrite `DirectionalShadowPhase` around prepared batches.
- Remove `StandardMaterial` checks from phase code.

### PR 6: Shader Dedup

- Add shared shadow WGSL.
- Compose built-in material shaders through one source path.

### PR 7: Contact And Debug

- Add debug presets and contact-only/contact-off isolation.
- Add readback diagnostics.

### PR 8: Cleanup

- Delete old helpers/adapters.
- Update examples, docs, and AGENTS.

## Implementation Notes

- Keep directional shadows first. Spot and point shadows come after the new
  contract is proven.
- Keep fixed PCF as the diagnostic baseline.
- Add PCSS only after raw atlas and fixed PCF look correct.
- Treat transparent shadows as an additive feature over valid opaque depth.
- Keep shadow uniforms compact, but prefer explicit fields over overloaded
  vector lanes when debugging clarity matters.
- Prefer numeric readback assertions over checked-in snapshot images unless the
  repo later adopts snapshot artifacts.
- Keep hot draw loops simple. The rewrite should clarify ownership without
  adding avoidable per-frame allocations.

## Acceptance Criteria

The rewrite is done when:

- `ShadowFramePlan` is the single source of truth for per-frame shadow behavior;
- cascade math is pure and unit-tested;
- receiver/raster/culling projections have explicit roles;
- bias units are documented and encoded in `ShadowBiasPlan`;
- shadow caster extraction produces explicit pass-mode batches;
- shadow phase does not know about `StandardMaterial`;
- material interfaces declare shadow participation;
- graph resource publication is centralized;
- main passes read exact shadow texture handles;
- standard and normal-mapped shaders share one shadow sampling implementation;
- contact shadows can be disabled and debugged independently;
- readback tests catch empty atlas, missing darkening, and material flag
  regressions;
- `cargo check --examples --features app` passes after API-level changes.
