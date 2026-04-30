# Wicked Shadow Infrastructure Plan

## Purpose

Bring SkyEngine's shadow system close enough to WickedEngine's shadow-map architecture that later quality work is mechanical instead of guesswork.

This is an infrastructure plan first. Do not start with PCSS or visual polish. First make the engine able to represent, render, bind, debug, and tune the same shadow data shape Wicked uses.

Primary references:

- `_refs/WickedEngine/WickedEngine/wiRenderer.cpp`
  - `CreateDirLightShadowCams`
  - shadow atlas packing
  - directional cascade setup
  - shadow rasterizer bias setup
- `_refs/WickedEngine/WickedEngine/wiRenderer.h`
  - shadow formats
  - shadow atlas resources
- `_refs/WickedEngine/WickedEngine/shaders/shadowHF.hlsli`
  - Vogel disk filtering
  - border clamp
  - cascade atlas UV mapping
  - optional PCSS path
- `_refs/WickedEngine/WickedEngine/shaders/shadowVS*.hlsl`
  - opaque, alpha-test, transparent shadow pass variants
- `_refs/WickedEngine/WickedEngine/shaders/shadowPS_transparent.hlsl`
  - transparent shadow transmittance color plus secondary-depth output
- `_refs/WickedEngine/WickedEngine/wiRenderer.cpp`
  - `BSTYPE_TRANSPARENTSHADOW` multiplicative color blend and depth-extreme alpha blend

## Current State

What SkyEngine already has:

- `DirectionalShadowPhase` renders depth-only shadow passes.
- `ShadowViewBinding` binds directional cascades to material shading.
- `SceneShadowResources` is the typed producer/consumer contract for shadow resources.
- `ShadowUniform` contains four cascade matrices, split distances, per-cascade params, one light direction, and one packed global parameter vector.
- Standard materials sample a depth texture array with a comparison sampler.
- The shader already uses a 16-sample Vogel disk-style PCF path.
- Standard materials select and blend cascades in shader.
- The runtime builds one directional shadow `SceneView` per active cascade.
- Directional cascades render into a depth texture array, one array layer per cascade.
- RenderGraph can import external textures.
- RenderGraph supports texture mip counts, array layer counts, and subresource read/write dependencies.
- GPU `RenderTarget` supports mip/layer subresource views.
- Debug view and readback infrastructure exist for scene, SSGI, and directional shadow resources.
- Directional light authoring exposes compare bias, raster bias, slope bias, normal bias, filter radius, cascade count, cascade distances, cascade blend, and per-cascade resolution.

Remaining gaps:

- No atlas rect / scale-offset data yet.
- No shadow atlas allocator or border-clamp contract yet.
- No on-scene cascade coverage visualization yet.
- No per-material cast/receive shadow contract.
- Transparent shadow map work is in progress on the fixed directional atlas path.
- No per-cascade caster culling or shadow LOD policy.
- No public texture dump helper dedicated to shadow atlas/cascade subresources yet.

## Design Rules

- Copy Wicked's structure where practical; avoid inventing new shadow math.
- Keep the first implementation wgpu-friendly.
- Prefer small, testable data contracts before shader polish.
- Keep the existing single-shadow path working until CSM replaces it cleanly.
- Add debug views before large visual changes.
- CSM first, atlas second. A working multi-cascade path is easier to verify before atlas packing is introduced.
- Do not mix ray traced shadows, screen-space shadows, capsule shadows, or volumetric cloud shadows into the first pass.
- Treat Wicked-style shadows as the built-in default implementation, not the only implementation.
- Keep shadow resources publishable and consumable through typed pipeline/resource contracts.

## Programmable Shadow Contract

Purpose: make shadows part of the modern programmable render pipeline instead of a hard-wired StandardMaterial feature.

The built-in Wicked-style directional shadow system should be one `ShadowProducer`. User code must be able to replace it, augment it, or consume its outputs without editing engine-private runtime code.

### Producer Contract

A shadow producer is a render phase, compute pass, or graph pass that publishes shadow resources for a view.

Built-in producers:

- `WickedDirectionalShadowProducer`
  - produces directional shadow cascades;
  - eventually packs cascades into a shadow atlas;
  - owns Wicked-style cascade fitting, texel snapping, and filtering metadata.
- Future producers:
  - point-light shadow producer;
  - spot-light shadow producer;
  - ray traced shadow mask producer;
  - screen-space contact shadow producer.

Required producer capabilities:

- declare all shadow resources through RenderGraph;
- publish a typed `SceneShadowResources` payload;
- publish debug/readback handles;
- support per-view and per-light enable/disable;
- expose enough metadata for custom material shaders.

Non-goals for the first producer:

- hardware ray tracing;
- transparent shadow atlas;
- volumetric/cloud shadows;
- capsule shadows.

### Resource Contract

`SceneShadowResources` should become the typed bridge between shadow producers and consumers.

It should describe:

- shadow map texture or atlas texture;
- optional transparent shadow texture;
- comparison sampler;
- cascade count;
- cascade matrices;
- cascade split distances;
- cascade atlas rects / scale-offsets;
- per-cascade texel size;
- bias/filter parameters;
- light direction and light id;
- debug labels and readback source handles.

The data can begin as an internal struct, but the design should keep a clean path toward `render::expert` exposure.

Suggested staged shape:

```rust
pub struct SceneShadowResources {
    pub kind: ShadowResourceKind,
    pub atlas: TextureHandle,
    pub comparison_sampler: ShadowSamplerHandle,
    pub metadata: ShadowMetadataHandle,
}

pub enum ShadowResourceKind {
    DirectionalCascades,
    SingleDirectionalMap,
}
```

The exact type names can change. The important rule is that material shaders and custom passes consume a resource contract, not `ShadowViewBinding` internals.

### Consumer Contract

A shadow consumer is any material, fullscreen pass, compute pass, or debug pass that reads shadow resources.

Built-in consumers:

- `StandardMaterial`
- `StandardMaterialNormalMapped`
- shadow debug view

User consumers should be able to:

- request current view shadow resources from setup/execute context;
- bind the comparison sampler and atlas/metadata without knowing the built-in producer internals;
- opt out of built-in shadow sampling;
- read shadow debug outputs;
- use shadow resources in custom lighting passes.

Desired API direction:

```rust
ctx.require_scene_shadows();
ctx.optional_scene_shadows();
ctx.publish_scene_shadows(resources);
```

This mirrors the existing scene texture and blackboard direction, but with a typed contract for shadows.

### Pipeline Contract

`RenderPipelineAsset` should support three modes:

- built-in Wicked shadows:
  - default for `modern_3d()`;
  - uses the producer from this plan.
- custom shadow producer:
  - user disables/replaces built-in shadow phase;
  - user publishes `SceneShadowResources`.
- no shadows:
  - material consumers receive a disabled/null resource contract.

The target shape is:

```rust
RenderPipelineAsset::builder()
    .add_phase(WickedDirectionalShadowPhase::default())
    .add_phase(OpaquePhase::new())
    .build();
```

And later:

```rust
RenderPipelineAsset::builder()
    .add_phase(MyShadowProducer::default())
    .add_phase(MyLightingPhase::default())
    .build();
```

The built-in material path should not require the built-in shadow producer specifically. It should require only `SceneShadowResources`.

### Debug And Readback Contract

Every shadow producer should publish inspectable resources.

Required built-in debug views:

- full shadow map / atlas;
- selected cascade;
- cascade split visualization;
- light frustum / cascade bounds visualization;
- shadow receiver compare depth if practical.

Readback requirements:

- shadow atlas can be dumped through existing texture readback;
- cascade subresources can be dumped individually;
- debug output should not require editing shader code.

### Decoupling Requirements

When this plan is complete:

- `StandardMaterial` consumes shadow resources but does not own shadow setup.
- `DirectionalShadowPhase` produces resources but does not know StandardMaterial internals.
- RenderGraph owns resource lifetime and dependencies.
- `RenderComposer` only coordinates producer/consumer payloads.
- Custom user phases can publish or consume shadow resources.
- Wicked-style CSM is a default plugin, not a renderer singleton.

## Milestone P0: Shadow Resource Contract Before Quality Work

Purpose: add the programmable contract before the implementation gets larger.

Status:

- Done: introduced `SceneShadowResources` and `ShadowResourceKind`.
- Done: `DrawContext` material binding now consumes `SceneShadowResources` instead of raw `ShadowSceneBindingLayout` plus `ShadowViewBinding`.
- Done: setup/execute contexts expose `optional_scene_shadows()` and `require_scene_shadows()`.
- Done: built-in single-map directional shadows are wrapped as `SceneShadowResources`.
- Done: setup contexts expose `publish_scene_shadows(resources)` for custom producers.
- Done: disabled/null custom shadow resource tests.
- Done: view execution receives completed view resource state, so later consumers can see published shadow resources during execute.
- Done: custom non-null shadow resources can be constructed from user bind group handles and consumed during execute.
- Done: full custom material draw sample binds a user shadow resource through `SceneBindingKind::ShadowView`.
- P0 is complete.

Tasks:

- Introduce an internal `SceneShadowResources` payload type.
- Move material-facing shadow binding decisions behind this typed payload.
- Keep `ShadowViewBinding` as the current implementation detail.
- Add setup/execute helpers for optional/required shadow resources.
- Add tests that a custom phase can publish a disabled/null shadow resource.
- Add tests that material setup can consume shadow resources without depending on the concrete producer type.

Files:

- `src/render/lighting/shadow/`
- `src/render/pipeline/contexts.rs`
- `src/render/phase/draw.rs`
- `src/render/resources/material.rs`
- `src/render/runtime/frame_builder.rs`

Acceptance:

- Built-in single-map shadows still work.
- A custom pass can publish a shadow resource payload.
- Standard material code consumes the typed contract rather than directly depending on producer internals where possible.

## Milestone S0: Document And Lock The Existing Contract

Purpose: make the current behavior explicit before changing the data model.

Status:

- Done: `ShadowUniform` layout and packed parameter contract are covered by tests.
- Done: `DirectionalShadowPhase` setup is covered for an enabled shadow view.
- Done: `SceneBindingKind::ShadowView` material binding is covered by the custom material draw test from P0.
- Done: standard material shaders document current `shadow_params` meanings.

Tasks:

- Add tests around current `ShadowUniform` size/layout.
- Add tests that `DirectionalShadowPhase` still renders for a shadow view.
- Add tests that a material with `SceneBindingKind::ShadowView` receives the shadow bind group.
- Document current shader parameter meanings:
  - `shadow_params.x`: cascade count;
  - `shadow_params.y`: cascade blend width;
  - `shadow_params.z`: cascade coverage debug flag;
  - `shadow_params.w`: enabled flag.
  - `cascade_params[i].x`: shader compare bias;
  - `cascade_params[i].y`: texel size;
  - `cascade_params[i].z`: light radius;
  - `cascade_params[i].w`: cascade enabled flag.

Files:

- `src/render/lighting/shadow/bindings.rs`
- `src/render/lighting/shadow/view.rs`
- `src/render/lighting/shadow/phase.rs`
- `src/render/shaders/materials/standard_material.wgsl`
- `src/render/shaders/materials/standard_material_normal_mapped.wgsl`

Acceptance:

- `cargo test --features app shadow`
- `cargo test --features app render`

## Milestone S1: Shadow Debug And Tunable Bias

Purpose: make shadow problems inspectable and tuneable before CSM.

Status:

- Done: added `RenderDebugView::DirectionalShadowMap`.
- Done: runtime publishes frame-level `ShadowDebugResources` for the active directional shadow map.
- Done: debug view can import and visualize the active directional shadow depth texture without depending on scene-depth slots.
- Done: shadow depth uses a dedicated inverted depth visualization mode.
- Done: `DirectionalLight` exposes shader compare bias, raster depth bias, raster slope bias, normal bias, and shadow filter radius.
- Done: `DirectionalLight::radius` is decoupled from `shadow_filter_radius`; shadow softness must be set through explicit shadow fields or presets.
- Done: `DirectionalShadowPhase` keys and builds shadow pipelines from the active raster bias instead of hard-coded bias state.
- Done: added sharp, soft, and contact-safe shadow preset helpers.
- Done: `shadow_normal_bias` is uploaded through the shadow uniform and applied before standard material shadow projection.
- Done: S2 has begun with a fixed 4-cascade public light contract and a fixed-size cascade shadow uniform.
- Done: standard material shadow sampling selects a cascade slot from view-space depth; the current render path mirrors the single map into all active slots until multi-target cascade rendering lands.
- Done: the runtime builds one directional shadow `SceneView` per active cascade.
- Done: directional shadows now render into one depth texture array, with one array layer and one shadow-pass uniform per cascade.
- Done: standard materials sample the shadow depth array layer selected by cascade index.
- Done: standard materials blend from the current cascade to the next near cascade projection edges, following Wicked's cascade edge-fade shape.
- Done: debug view can inspect a selected directional cascade layer.
- Done: on-scene cascade coverage debug mode.
- Done: `three_d_demo` uses 4 cascades by default and exposes raw cascade/coverage debug toggles.
- Done: S3.1/S3.2 started by switching directional cascades to Wicked-style horizontal atlas sampling:
  - one `shadowAtlasMulAdd`-style value;
  - `shadow_uv.x += cascade`;
  - `shadow_border_clamp` from atlas resolution;
  - one depth atlas texture instead of a depth texture array.
- Done: S3 debug inspection crops/zooms selected atlas rects.
- Done: S3.3 started with explicit `ShadowAtlasLayout`, guard-band metadata, correct atlas load/clear behavior, and render stats surfaces.
- Done: S3.4 started with a Wicked-style grow-to-fit atlas allocator boundary. Directional lights now receive a packed light rect and keep cascades as slices inside that rect, matching Wicked's `shadowAtlasMulAdd + slice` shader model.
- Done: spot lights are now a first-class scene/GPU light type with cone attenuation data, shadow-ready authoring fields, and a packed-atlas layout path for future spot shadow rects.

Tasks:

- Add `RenderDebugView::DirectionalShadowMap`.
- Expose the active shadow target from `ShadowViewBinding` to debug view setup.
- Add depth visualization mode suitable for shadow depth.
- Add directional-light tuning fields:
  - `shadow_depth_bias`
  - `shadow_slope_bias`
  - `shadow_normal_bias`
  - `shadow_filter_radius`
- Move hard-coded `DirectionalShadowPhase` depth bias into light/settings-derived pipeline keys where possible.
- Keep `shadow_bias` as the explicit shader compare bias field.
- Add helper presets:
  - sharp sun shadow;
  - soft sun shadow;
  - contact-safe high-bias shadow.

Wicked references:

- `wiRenderer.cpp` rasterizer states around `RSTYPE_SHADOW`.
- `shadowHF.hlsli::sample_shadow`.

Files:

- `src/render/component/light.rs`
- `src/render/lighting/shadow/phase.rs`
- `src/render/lighting/shadow/view.rs`
- `src/render/component/settings.rs`
- `src/render/pipeline/builtins.rs`
- `src/render/postfx/debug_view.rs`
- `src/render/shaders/materials/standard_material.wgsl`
- `src/render/shaders/materials/standard_material_normal_mapped.wgsl`

Acceptance:

- Debug view can display the active directional shadow depth texture.
- Demo can tune shadow bias without editing shader source.
- No regression in current single-map shadows.

## Milestone S2: Directional Cascades Without Atlas

Purpose: introduce Wicked-style CSM while avoiding atlas complexity.

Status:

- Done: fixed-size public cascade contract on `DirectionalLight`.
- Done: fixed-size cascade uniform contract.
- Done: one shadow `SceneView` per active cascade.
- Done: one depth texture array layer per active cascade.
- Done: material shader cascade selection by view-space depth.
- Done: material shader cascade edge blending.
- Done: cascade depth debug view through `RenderDebugView::DirectionalShadowCascade(index)`.
- Done: lit-scene cascade coverage through `RenderDebugView::DirectionalShadowCoverage`.
- Done: demo defaults and debug toggles make the current CSM behavior inspectable.
- S2 is complete enough to begin atlas metadata.

Data model:

- Add a fixed maximum cascade count, initially 4.
- Done: add to `DirectionalLight`:
  - `cascade_count`
  - `cascade_distances`
  - `cascade_blend`
  - `shadow_resolution_per_cascade`
- Done: replace single shadow matrix with a cascade array:
  - `light_view_proj[4]`
  - `cascade_splits[4]`
  - `cascade_count`
  - per-cascade texel size
  - per-cascade bias/filter values

Render path:

- Done: build one `SceneViewKind::DirectionalShadow` view per cascade.
- Use Wicked's `CreateDirLightShadowCams` logic:
  - split camera frustum by cascade distance;
  - transform split corners into light space;
  - fit bounding sphere;
  - snap to shadow texel grid;
  - expand Z using Wicked's conservative depth extent.
- Done: initially allocate one depth texture array and render each cascade into its own array layer.
- Done: bind the cascade texture array as `texture_depth_2d_array`.

Shader path:

- Select cascade by view depth.
- Project world position by selected cascade matrix.
- Sample selected cascade depth.
- Keep existing 16-tap filter.
- Blend to the next cascade near projection edges using Wicked's edge fade pattern.

Wicked references:

- `wiRenderer.cpp::CreateDirLightShadowCams`
- `shadowHF.hlsli::shadow_2D`

Files:

- `src/render/lighting/shadow/view.rs`
- `src/render/lighting/shadow/bindings.rs`
- `src/render/lighting/shadow/phase.rs`
- `src/render/shaders/materials/standard_material.wgsl`
- `src/render/shaders/materials/standard_material_normal_mapped.wgsl`
- `src/render/runtime/frame_builder.rs`

Acceptance:

- Near cascade has visibly higher texel density than current single-map path.
- Camera movement does not shimmer heavily because texel snapping is preserved.
- Tests cover cascade split creation and cascade selection.

### S2 Closeout: Cascade Coverage Debug And Demo Tuning

Purpose: make the CSM split behavior visible in the lit scene before moving to atlas packing.

Tasks:

- Done: add a lit-scene cascade coverage debug mode:
  - candidate API: `RenderDebugView::DirectionalShadowCoverage`;
  - tint receivers by selected cascade index after the same cascade-selection logic used by material shading;
  - include blend-zone visibility, either by desaturating blended pixels or by adding a small edge-band tint.
- Done: keep depth-layer debug modes:
  - `DirectionalShadowMap` remains cascade 0/full default;
  - `DirectionalShadowCascade(index)` remains the raw selected layer view.
- Done: keep duplicated standard material cascade-selection functions explicitly synchronized for now.
- Done: add shader comments that name the convention:
  - view-space depth is `max(-receiver_view.z, 0.0)`;
  - split distances are in camera view distance units;
  - cascade blend is projection-edge fade width.
- Done: tune `examples/render/three_d_demo.rs` around 3 or 4 cascades:
  - set explicit cascade distances for the current camera range;
  - set `cascade_blend` high enough to hide transitions but low enough to preserve near-cascade sharpness;
  - set per-cascade resolution and filter radius as the new default quality reference.
- Done: add runtime input toggles in the demo:
  - normal lit mode;
  - raw cascade 0/1/2/3 depth;
  - cascade coverage overlay.
- Done: add tests:
  - resolved cascade splits remain monotonic and fill the far split;
  - shadow views carry the correct cascade index;
  - debug resources clamp out-of-range selected cascade layers;
  - shader/uniform layout still matches the cascade contract.

Files:

- `src/render/component/settings.rs`
- `src/render/lighting/shadow/view.rs`
- `src/render/lighting/shadow/resources.rs`
- `src/render/pipeline/builtins.rs`
- `src/render/shaders/materials/standard_material.wgsl`
- `src/render/shaders/materials/standard_material_normal_mapped.wgsl`
- `src/render/shaders/postfx/debug_view.wgsl`
- `examples/render/three_d_demo.rs`

Acceptance:

- A developer can see which cascade shades each receiver without inspecting the shadow map.
- The 3D demo uses multi-cascade shadows by default.
- Done: `cargo test --features app shadow`
- Done: `cargo check --example three_d_demo --features app`

## Milestone S3: Shadow Atlas

Purpose: converge the resource shape toward Wicked's atlas model.

Status:

- In progress.
- Done: directional cascades now render into one horizontal depth atlas instead of array layers.
- Done: material shaders follow Wicked's `shadow_2D` shape: add cascade to `shadow_uv.x`, apply `shadowAtlasMulAdd`, then sample with `shadow_border_clamp`.
- Done: selected-rect debug view crops/zooms the requested atlas rect.
- Done: explicit atlas padding/stat surfaces are in place for the fixed layout.

Implementation rule:

- Keep the existing texture-array path behind a fallback or short-lived compatibility branch until the atlas path has equivalent debug coverage.
- Do not combine atlas packing with PCSS, alpha-test, or transparent shadows.
- First atlas layout should be fixed and deterministic; a general rect packer can come later.

Tasks:

- Add `ShadowAtlas` runtime resource:
  - depth texture;
  - optional transparent shadow color texture later;
  - atlas resolution;
  - allocated rects.
- Add per-cascade atlas rect data:
  - `shadow_atlas_mul_add`
  - border clamp rect
  - cascade index
- Pack directional cascades horizontally first.
- Add simple fixed atlas layout before a general rect packer.
- Update material shader:
  - local cascade UV -> atlas UV;
  - clamp with Wicked-style `shadow_border_clamp`;
  - sample single atlas texture.
- Update debug view:
  - full atlas;
  - selected cascade rect.

Wicked references:

- `ALLOW_SHADOW_ATLAS_PACKING`
- `shadowAtlasMulAdd`
- `shadowHF.hlsli::shadow_border_clamp`

Files:

- `src/render/lighting/shadow/atlas.rs`
- `src/render/lighting/shadow/bindings.rs`
- `src/render/lighting/shadow/view.rs`
- `src/render/shaders/materials/standard_material.wgsl`
- `src/render/shaders/materials/standard_material_normal_mapped.wgsl`

Acceptance:

- All cascades render into one atlas.
- PCF samples do not bleed into neighboring cascades.
- Debug view can inspect atlas and per-cascade rects.

### S3.1 Atlas Metadata Contract

Purpose: change the data contract before changing the render target.

Tasks:

- Done: add Wicked-style `shadowAtlasMulAdd` metadata:
  - `{ rect_width / atlas_width, rect_height / atlas_height, rect_x / atlas_width, rect_y / atlas_height }`.
- Done: add atlas resolution reciprocal metadata for border clamp and filter spread.
- Done: update shader sampling to pass through Wicked-style atlas transform.
- Dropped: texture-array identity path, per "no compatibility".

Acceptance:

- Material shaders consume atlas metadata directly.
- The old array-layer path is removed from the material/shadow binding path.

### S3.2 Fixed Horizontal Atlas

Purpose: render all directional cascades into one depth texture using a simple deterministic layout.

Tasks:

- Done: create one atlas depth target sized:
  - width = `cascade_resolution * active_cascade_count`;
  - height = `cascade_resolution`;
  - format = current directional shadow depth format.
- Done: render each cascade into a viewport/scissor rect inside the atlas.
- Done: remove array-layer render target dependency from the atlas path.
- Update debug resources to publish:
  - full atlas;
  - selected cascade rect as a view or UV-windowed debug source.
- Done: update material shader:
  - transform local cascade UV to atlas UV;
  - clamp PCF taps against `border_min_max`;
  - sample the single atlas depth texture.

Acceptance:

- Four active cascades render into one horizontal atlas.
- Neighboring cascades do not bleed under the existing 16-tap PCF kernel.
- Pending: selected cascade debug view should crop/zoom to that atlas rect instead of showing the full atlas.

### S3.3 Atlas Padding And Future Packer Boundary

Purpose: make the fixed layout compatible with a future mixed light atlas.

Tasks:

- Done: reserve one configurable guard band around each cascade rect.
- Done: shrink `border_min_max` to exclude the guard band.
- Done: clear atlas on cascade 0 and load it for subsequent cascade rect passes so stale depth cannot affect border samples.
- Done: add `ShadowAtlasLayout` as a small internal abstraction:
  - directional lights are represented as one packed light rect with cascade slices inside it;
  - the allocator grows atlas dimensions like Wicked's rectpacker state;
  - spot shadow rect requests are represented in the layout boundary;
  - point shadows can add six-slice rect requests without changing material sampling.
- Done: record atlas usage stats:
  - active cascade count;
  - atlas resolution;
  - used rect count;
  - used pixel ratio.

Acceptance:

- Fixed layout remains simple, but no shader or material code assumes cascades are horizontal forever.
- Future point/spot shadows can be added by replacing layout allocation rather than rewriting consumers.

## Milestone S4: Per-Material Cast/Receive Shadows

Purpose: make the renderer match real material behavior instead of casting every opaque mesh uniformly.

Tasks:

- Done: add cast/receive shadow flags to the appropriate public render component or material layer:
  - `WgpuMeshRenderer::casts_shadows`;
  - `MeshRenderer::casts_shadows`;
  - `StandardMaterial::receive_shadows`;
  - `StandardMaterialAsset::receive_shadows`.
- Done: ensure shadow-view extraction filters non-casters before the shadow phase.
- Done: material shading skips shadow sampling when receive shadow is disabled.
- Done: keep defaults compatible:
  - opaque meshes cast and receive shadows by default.

Wicked references:

- `SHADERMATERIAL_OPTION_BIT_RECEIVE_SHADOW`
- `SHADERMATERIAL_OPTION_BIT_CAST_SHADOW`

Files:

- `src/render/component/mesh.rs`
- `src/render/resources/material.rs`
- `src/render/phase/draw.rs`
- `src/render/lighting/shadow/phase.rs`
- material WGSL shaders

Acceptance:

- Done: a mesh can receive but not cast shadows.
- Done: a mesh can cast but not receive shadows.
- Done: existing examples behave unchanged by default.

## Milestone S5: Alpha-Test Shadow Pass

Purpose: support foliage/fences/grates and other cutout materials.

Status:

- Done: `AlphaMode::Mask` is a first-class alpha-test mode for `StandardMaterial` and `StandardMaterialAsset`.
- Done: standard forward and material prepass shaders discard masked pixels using albedo alpha and `alpha_cutoff`.
- Done: added a Wicked-style alpha-test shadow depth shader variant.
- Done: directional shadow execution routes masked `StandardMaterial` casters through the alpha-test pipeline.
- Done: alpha-test shadow batches bind the standard material uniform/albedo texture/cutoff.
- Done: opaque shadow batches stay on the position-only depth pipeline.
- S5 is complete.

Tasks:

- Done: add shadow alpha-test shader variant.
- Done: bind material alpha texture/threshold into the shadow pass.
- Done: route alpha-test materials into the alpha-test shadow pipeline.
- Done: keep opaque shadow path position-only for speed.

Wicked references:

- `shadowVS_alphatest.hlsl`
- `shadowPS_alphatest.hlsl`

Files:

- `src/render/lighting/shadow/phase.rs`
- `src/render/shaders/lighting/shadow_depth.wgsl`
- new WGSL alpha-test shadow shader
- material pipeline metadata

Acceptance:

- Done: alpha-cutout material casts cutout shadows.
- Done: opaque shadow path remains position-only.

## Milestone S6: PCSS And Dithered Filtering

Purpose: improve soft shadow quality after CSM and atlas are correct.

Status:

- Done: added `ShadowSamplingMode` with fixed PCF, dithered PCF, and PCSS modes.
- Done: directional lights can select filtering through explicit builder methods.
- Done: fixed PCF remains the default path for stable screenshots/tests.
- Done: standard material shaders support per-pixel dither rotation for the Vogel disk.
- Done: standard material shaders support a disabled-by-default Wicked-style PCSS blocker search and penumbra growth.
- Done: sampling mode is uploaded through the existing shadow atlas metadata spare slot, so the shadow uniform layout stays stable.
- S6 is complete.

Tasks:

- Done: port Wicked's optional PCSS blocker search.
- Done: add optional per-pixel dither rotation.
- Done: control it from light/settings:
  - fixed PCF;
  - dithered PCF;
  - PCSS.
- Done: keep PCSS disabled by default until performance is understood.

Wicked references:

- `SHADOW_SAMPLING_PCSS`
- `SHADOW_SAMPLING_DITHERING`
- `shadowHF.hlsli::sample_shadow`

Acceptance:

- Done: contact shadows stay tighter near caster/receiver contact when PCSS is enabled.
- Done: penumbra grows with blocker/receiver separation when PCSS is enabled.
- Done: fixed PCF remains available as the default stable path.

## Milestone S7: Performance And Update Policy

Purpose: make higher-quality shadows affordable.

Status:

- Done: per-cascade shadow-view extraction now culls casters by each cascade frustum and by an explicit shadow cascade mask.
- Done: `MeshRenderer` and `WgpuMeshRenderer` expose `shadow_cascade_mask` and `shadow_lod_cascades(...)` for Wicked-style shadow LOD override behavior.
- Done: render stats expose per-cascade caster counts and per-cascade shadow draw-call counts.
- Done: `ShadowUpdatePolicy` supports every-frame and static-when-unchanged directional shadow updates.
- Done: unchanged static cascades skip render-pass submission while preserving the previously rendered atlas contents.
- Done: updated cascades clear only their atlas rect before drawing, so skipped cascades remain reusable.
- S7 is complete for the current fixed directional-atlas path.

Tasks:

- Done: per-cascade caster culling.
- Done: per-cascade draw-call counts in render stats.
- Done: shadow LOD override.
- Done: static shadow update policy.
- Done: only update cascades when view/light/casters require it.

Wicked references:

- `SHADOW_LOD_OVERRIDE`
- per-cascade frustum checks in `wiRenderer.cpp`

Acceptance:

- Done: render stats expose cascade count, caster count, shadow draw calls, and atlas usage.
- Done: large scenes do not render every caster into every cascade.
- Done: `cargo test --features app shadow`

## Milestone S8: Wicked Transparent Shadows

Goal: copy Wicked's transparent shadow-map shape for alpha-blended materials before moving on to point/spot atlas work.

Wicked references:

- `shadowPS_transparent.hlsl`
- `shadowHF.hlsli`
- `BSTYPE_TRANSPARENTSHADOW` in `wiRenderer.cpp`

Implementation:

- Done: `ShadowViewBinding` owns a second directional atlas for transparent shadow transmittance.
- Done: the shared shadow scene bind group exposes the transparent atlas and filtering sampler alongside the depth atlas.
- Done: `DirectionalShadowPhase` submits a second per-cascade pass for transparent casters after the depth pass.
- Done: transparent shadow rendering uses a Wicked-style material-aware shader that outputs transmittance RGB and secondary depth in alpha.
- Done: transparent shadow color accumulates with Wicked's multiplicative color blend; alpha uses Wicked's MAX blend and `a > cmp` secondary-depth check, with a reversed key at write time for SkyEngine's non-reversed depth atlas.
- Done: standard material shadow sampling applies transparent transmittance inside the PCF/PCSS sample loop, mirroring Wicked's `shadowHF.hlsli` shape.
- Done: static cascade signatures and render stats include transparent shadow casters.
- Done: directional shadow debug resources carry the packed light rect `shadowAtlasMulAdd`, so selected cascade debug views no longer assume the directional rect starts at atlas origin.
- Done: `SpotLight` now uploads GPU cone direction and inner/outer cone cosines, and StandardMaterial/DDGI distinguish spot lights from directional lights.

Acceptance:

- `cargo test --features app shadow`
- `cargo test --features app standard_material`
- `cargo check --examples --features app`

## Recommended Implementation Order

Completed order:

1. P0: introduce the programmable shadow resource contract.
2. S0: lock current behavior.
3. S1: debug view and tunable bias.
4. Most of S2: CSM without atlas.

Current order from here:

1. Done: S3 debug crop/zoom selected cascade atlas rect in debug view.
2. Done: S3.3 atlas padding, stats, and future packer boundary.
3. Done: S4 cast/receive flags.
4. Done: S5 alpha-test shadows.
5. Done: S6 PCSS/dither.
6. Done: S7 optimization/update policy.
7. Done: S8 transparent shadow map.

## Next Useful PR Scope

Continue beyond the current Wicked directional-shadow baseline:

- Render spot-light shadow views into the existing depth/transparent atlases and bind spot shadow records into material sampling.
- Add point-light six-slice atlas rects after spot-light shadows validate the allocator path.
- Add dedicated shadow atlas/cascade dump helpers on top of the readback path.
- Add a public expert-facing shadow producer contract once the point/spot atlas shape lands.

Acceptance:

- `cargo test --features app shadow`
- `cargo check --examples --features app`
