# Programmable Modern Render Pipeline Plan

## Summary

This plan turns SkyEngine's current renderer from "registration-driven and partially programmable" into a genuinely modern, clear, decoupled, user-programmable render pipeline.

The goal is not to replace the existing high-level API. The goal is to make the existing API deep enough that advanced users can build custom 3D pipelines, GI passes, post effects, debug views, and renderer experiments without forking the engine or touching private runtime internals.

Current state:

- `RenderPipelineAsset`, `RenderPipelineBuilder`, `RenderComposer`, `RenderFeature`, `RenderPhase`, `ComputePass`, `PostFxPass`, and `RenderPass` already provide a useful plugin shape.
- `modern_3d()` exists and starts to follow a WickedEngine-style pass order.
- `SceneSnapshot` and Kajiya backend boundaries are clearer.
- SSGI has a Wicked-inspired deinterleave/diffuse/upsample structure.
- RenderGraph can allocate textures with sample count, mip count, and array layer count metadata.

Main missing pieces:

- Compute/storage texture support is not first-class enough.
- RenderGraph cannot yet declare subresource-level reads/writes.
- User passes cannot easily request named scene resources, history buffers, texture arrays, storage views, or debug outputs.
- Temporal infrastructure is incomplete.
- GBuffer/material layout is not yet stable enough for user-authored modern 3D passes.
- Debug tooling is too thin for serious renderer development.

## Design Principles

- Keep user-facing rendering built around `RenderPipelineAsset` and `RenderPipelineBuilder`.
- Keep backend-specific types out of the public API.
- Keep Kajiya as a backend fallback and comparison target, not the design center.
- Keep `modern_3d()` experimental but stable enough for examples and tests.
- Avoid one giant universal scene schema. Use typed payloads and declared graph resources.
- Make simple rendering easy and advanced rendering possible.
- Prefer explicit resource declarations over hidden global state.
- Keep high-level app examples unchanged unless the example specifically demonstrates programmable rendering.
- Preserve `forward_3d()` and `kajiya_3d()` compatibility.

## Target API Shape

The final user experience should look roughly like this:

```rust
let pipeline = RenderPipelineAsset::builder()
    .add_feature(MeshFeature::default())
    .add_phase(SceneMaterialPrepass::default())
    .add_phase(DirectionalShadowPhase::default())
    .add_phase(OpaquePhase::default())
    .add_compute(MySsgiPass::default())
    .add_postfx(MyTonemap::default())
    .build();
```

Advanced users should also be able to do:

```rust
builder.add_graph_pass(MyPass::new()
    .reads(SceneTexture::Depth)
    .reads(SceneTexture::Normal)
    .writes("my_indirect_diffuse", TextureDesc::rgba16f_half_res())
    .dispatch(|ctx| {
        // create bind groups from declared resources
        // dispatch compute or draw fullscreen
    }));
```

This exact syntax can change, but the capability must be real:

- declare resources;
- declare pass order;
- read/write scene textures;
- read/write storage textures;
- request persistent history;
- inspect/debug outputs;
- stay outside engine-private internals.

## Milestone 0: Freeze The Current Contract

Purpose: stop the renderer from drifting while infrastructure is being added.

### Tasks

- Add a document section to `src/render/AGENTS.md` naming the canonical programmable render boundary:
  - `RenderPipelineAsset`
  - `RenderPipelineBuilder`
  - `PipelineStep`
  - `RenderFeature`
  - `RenderPhase`
  - `ComputePass`
  - `RenderPass`
  - `PostFxPass`
  - `PreparedFrame`
  - `PreparedView`
  - `RenderGraph`
  - `PhaseState`
- Add tests that lock these invariants:
  - `modern_3d()` includes prepass, shadow, opaque, SSGI, transparent, sharpen, bloom, tonemap.
  - `forward_3d()` still does not require material prepass.
  - `kajiya_3d()` does not expose Kajiya types outside backend code.
  - user examples compile with `cargo check --examples --features app`.

### Files

- `src/render/AGENTS.md`
- `src/render/runtime/tests.rs`
- `src/render/pipeline/asset.rs`
- `src/render/backend/AGENTS.md`

### Acceptance

- Running `cargo test --features app render::runtime::tests::modern_3d_descriptor_uses_wicked_style_pass_order` passes.
- Running `cargo check --examples --features app` passes.
- No new public Kajiya or Wicked types appear in `src/render/mod.rs`.

## Milestone 1: RenderGraph Resource Model

Purpose: make the graph capable of representing modern GPU resources rather than only simple render targets.

### 1.1 Texture Descriptor Completion

Current state:

- Texture descriptors track width/height, format, sample count, mip count, and array layer count.
- Physical targets have `array_layer_count`.

Next tasks:

- Add explicit texture usage to graph-created textures:
  - `RENDER_ATTACHMENT`
  - `TEXTURE_BINDING`
  - `STORAGE_BINDING`
  - `COPY_SRC`
  - `COPY_DST`
- Add builder methods:
  - `usage(wgpu::TextureUsages)`
  - `add_usage(wgpu::TextureUsages)`
  - `storage_binding()`
  - `sampled()`
  - `render_attachment()`
  - `copy_src()`
  - `copy_dst()`
- Ensure pool keys include texture usage.
- Ensure aliasing only allows matching usage.

Files:

- `src/render/graph/builder.rs`
- `src/render/graph/types.rs`
- `src/render/graph/pool.rs`
- `src/render/graph/alias.rs`
- `src/render/graph/allocate.rs`
- `src/render/gpu/target.rs`

Tests:

- `texture_builder_tracks_usage_flags`
- `transient_pool_separates_storage_and_render_attachment_usage`
- `alias_rejects_mismatched_texture_usage`

Acceptance:

- A graph texture can be declared as storage-only and allocated with `STORAGE_BINDING`.
- A render attachment texture still defaults to sampled + renderable + copyable behavior.

### 1.2 Subresource Handles

Problem:

Wicked-style resources use specific mip levels and array layers. Today RenderGraph dependencies are whole-texture.

Add:

```rust
pub struct TextureSubresource {
    pub texture: TextureHandle,
    pub base_mip_level: u32,
    pub mip_level_count: u32,
    pub base_array_layer: u32,
    pub array_layer_count: u32,
}
```

Add pass setup methods:

- `read_texture(handle)`
- `read_subresource(subresource)`
- `write_texture(handle)`
- `write_subresource(subresource)`
- `readwrite_subresource(subresource)`

Dependency policy:

- Whole-texture read conflicts with any subresource write.
- Whole-texture write conflicts with any subresource read/write.
- Subresource read/write conflicts only when ranges overlap.
- Initial implementation may conservatively treat all subresources of a texture as overlapping if range logic gets risky; but the public shape should still exist.

Files:

- `src/render/graph/types.rs`
- `src/render/graph/builder.rs`
- `src/render/graph/compile.rs`
- `src/render/graph/reorder.rs`
- `src/render/graph/visualize.rs`
- `src/render/graph/tests.rs`

Tests:

- `subresource_read_after_write_creates_dependency`
- `non_overlapping_subresource_writes_can_coexist`
- `whole_texture_read_depends_on_subresource_write`
- `subresource_handles_reject_foreign_texture_handles`

Acceptance:

- A pass can declare `write_subresource(texture mip=2 layer=7)`.
- The graph orders later readers correctly.

### 1.3 Physical Subresource Views

Add `PhysicalResources` helpers:

- `texture_view(handle)`
- `texture_subresource_view(subresource, dimension)`
- `storage_texture_view(subresource, dimension)`
- `render_attachment_view(subresource)`

Rules:

- `render_attachment_view` must produce `D2` view for one mip and one layer.
- `texture_subresource_view` may produce `D2`, `D2Array`, or default full view.
- `storage_texture_view` must validate usage includes `STORAGE_BINDING`.

Files:

- `src/render/graph/types.rs`
- `src/render/gpu/target.rs`
- `src/render/graph/tests.rs`

Acceptance:

- SSGI can eventually bind a real `texture_2d_array` for atlas color/depth.

## Milestone 2: First-Class Compute And Storage Texture Passes

Purpose: stop implementing compute algorithms as fullscreen render-pass workarounds.

### 2.1 Compute Pipeline Helper

Add a reusable helper similar to `FullscreenPipeline`:

```rust
pub struct ComputePipelineCache {
    shader: Arc<wgpu::ShaderModule>,
    entry: &'static str,
    layouts: Vec<wgpu::BindGroupLayout>,
    pipeline: Option<Arc<wgpu::ComputePipeline>>,
}
```

Files:

- `src/render/gpu/compute.rs`
- `src/render/gpu/mod.rs`

Tests:

- `compute_pipeline_cache_reuses_pipeline`

Acceptance:

- Built-in compute passes do not open-code shader module + pipeline layout creation.

### 2.2 Storage Texture Bind Helpers

Add helper functions:

- sampled texture entry
- storage texture entry
- uniform buffer entry
- storage buffer entry
- sampler entry

Candidate location:

- `src/render/gpu/bindings.rs`

The helpers should support:

- `TextureViewDimension::D2`
- `TextureViewDimension::D2Array`
- storage access mode `WriteOnly`
- float formats needed by SSGI/DDGI:
  - `R32Float`
  - `Rgba16Float`
  - `Rg16Float` if supported

Tests:

- `storage_texture_bind_group_accepts_d2_array_view`

### 2.3 Compute Dispatch Context

Current `ComputePass` can execute, but user-facing ergonomics are weak.

Add context helpers:

- `ctx.read_texture(index)`
- `ctx.write_texture(index)`
- `ctx.read_subresource(index)`
- `ctx.write_subresource(index)`
- `ctx.dispatch_2d(width, height, block_size)`
- `ctx.dispatch_3d(width, height, depth, block_size)`

Files:

- `src/render/pipeline/contexts.rs`
- `src/render/runtime/nodes.rs`

Acceptance:

- A compute pass can be written without manually digging through `CompiledPass.reads` and `CompiledPass.writes`.

## Milestone 3: Scene Resource Registry

Purpose: make user passes able to request canonical renderer outputs without depending on private names.

### 3.1 Scene Texture Enum

Add:

```rust
pub enum SceneTexture {
    Color,
    Depth,
    Normal,
    Velocity,
    Albedo,
    Material,
    Emissive,
    Light,
    IndirectDiffuse,
}
```

Map it to `PhaseState` typed slots.

Files:

- `src/render/execution/slots.rs`
- `src/render/execution/helpers.rs`
- `src/render/mod.rs`

### 3.2 Scene Resource Requests

User passes should be able to say:

- require `SceneTexture::Depth`
- create if missing `SceneTexture::Velocity`
- optional read `SceneTexture::Emissive`

Add setup helpers:

- `ctx.require_scene_texture(SceneTexture::Depth)`
- `ctx.ensure_scene_texture(SceneTexture::Velocity, desc)`
- `ctx.optional_scene_texture(SceneTexture::Emissive)`
- `ctx.set_scene_texture(SceneTexture::IndirectDiffuse, handle, format)`

Files:

- `src/render/pipeline/contexts.rs`
- `src/render/execution/helpers.rs`

Tests:

- `custom_pass_can_require_scene_depth`
- `custom_pass_can_publish_indirect_diffuse`
- `scene_texture_requests_do_not_use_generic_slot_map`

Acceptance:

- User-authored pass no longer needs to know `PhaseState` internals.

## Milestone 4: History And Temporal Infrastructure

Purpose: unlock TAA, temporal SSGI, denoisers, reprojection, and stable GI.

### 4.1 Persistent History Textures

Add a `HistoryTexturePool` owned by `RenderComposer` or a dedicated runtime state:

- keyed by view id;
- keyed by name;
- resized on target size/format changes;
- ping-pong support.

API concept:

```rust
let history = ctx.history_texture("taa_color")
    .format(wgpu::TextureFormat::Rgba16Float)
    .size(SceneSize::Full)
    .ping_pong()
    .get();
```

Files:

- `src/render/runtime/history.rs`
- `src/render/runtime/state.rs`
- `src/render/pipeline/contexts.rs`

Tests:

- `history_texture_persists_across_frames`
- `history_texture_resizes_on_view_resize`
- `history_texture_is_isolated_per_view`

### 4.2 Jitter And Previous Matrices

Need stable temporal data:

- current view-projection;
- previous view-projection;
- camera jitter;
- previous jitter;
- reset flag when camera cuts or projection changes.

Files:

- `src/render/view/scene_view.rs`
- `src/render/view/camera.rs`
- `src/render/runtime/view_collection.rs`
- `src/render/pipeline/builtins.rs`
- `src/render/shaders/prepass/*`

Tests:

- `scene_view_tracks_previous_view_projection`
- `taa_jitter_changes_per_frame`
- `history_reset_on_projection_change`

Acceptance:

- TAA and temporal SSGI can reproject without hidden state.

### 4.3 Velocity Buffer Completion

Current velocity support is partial.

Tasks:

- Ensure material prepass always writes velocity when modern pipeline requests it.
- Ensure static geometry writes zero velocity.
- Ensure animated/model-matrix changes write motion.
- Add `scene_velocity` to standard modern pipeline resources.

Files:

- `src/render/pipeline/builtins.rs`
- `src/render/shaders/prepass/*`
- `src/render/resources/material.rs`

Tests:

- `scene_material_prepass_writes_velocity`
- `static_mesh_velocity_is_zero`
- `moving_mesh_velocity_is_nonzero`

## Milestone 5: User-Programmable Pass API

Purpose: make custom render pipeline work feel supported, not like hacking internals.

### 5.1 Declarative Graph Pass Trait

Add an optional higher-level pass trait:

```rust
pub trait GraphPass: Send + 'static {
    fn name(&self) -> &'static str;
    fn setup(&mut self, ctx: &mut GraphPassSetupContext<'_, '_>);
    fn execute(&mut self, ctx: &mut GraphPassExecuteContext<'_, '_>) -> Result<(), RenderGraphError>;
}
```

This does not replace `RenderPass`, `ComputePass`, or `PostFxPass`. It wraps common graph-resource ergonomics.

Files:

- `src/render/pipeline/passes.rs`
- `src/render/pipeline/contexts.rs`
- `src/render/runtime/nodes.rs`
- `src/render/pipeline/asset.rs`

Builder:

- `add_graph_pass(pass)`
- `add_graph_compute(pass)` if separate naming is clearer

Tests:

- `custom_graph_pass_can_read_scene_depth_and_write_texture`
- `custom_graph_pass_execution_order_matches_builder_order`

### 5.2 Public Resource Descriptors

Add user-friendly descriptors:

```rust
TextureSpec::rgba16f("name").half_res().storage().sampled()
TextureSpec::r32f("name").array_layers(16).mips(4)
TextureSpec::depth32("name").full_res()
```

Map these into `RenderGraph::create_texture`.

Files:

- `src/render/pipeline/resource_spec.rs`
- `src/render/mod.rs`
- `src/render/expert.rs`

Tests:

- `texture_spec_builds_expected_graph_texture`
- `texture_spec_half_res_resolves_against_surface`

### 5.3 User Payloads And Blackboard

Users need pass-to-pass communication:

- preserve existing typed payload store for prepared frame/view;
- expose safe graph blackboard helpers to graph pass contexts;
- allow pass setup to publish typed IDs or handles.

Files:

- `src/render/resources/blackboard.rs`
- `src/render/pipeline/contexts.rs`

Acceptance:

- A user pass can create a texture in setup and retrieve it in execute without private fields.

## Milestone 6: Modern 3D Resource Contract

Purpose: make `modern_3d()` predictable and usable by custom passes.

### 6.1 Stable GBuffer Layout

Define exact formats:

- `scene_depth`: `Depth32Float`
- `scene_normal`: `Rgba8Unorm` for now, later consider `Rg16Float` oct encoding
- `scene_albedo`: `Rgba8Unorm`
- `scene_material`: packed roughness/metallic/occlusion/flags
- `scene_emissive`: `Rgba16Float`
- `scene_velocity`: `Rg16Float` or `Rg32Float`
- `scene_light`: `Rgba16Float`
- `scene_indirect_diffuse`: `Rgba16Float`

Document packing:

- material.r = roughness
- material.g = metallic
- material.b = ambient occlusion
- material.a = flags or unused

Files:

- `src/render/pipeline/builtins.rs`
- `src/render/shaders/prepass/*`
- `src/render/shaders/materials/*`
- `docs/render_api.md`

Tests:

- `modern_3d_material_prepass_publishes_all_gbuffer_slots`
- `standard_material_prepass_packs_roughness_metallic`

### 6.2 Lighting Contract

Expose canonical light buffers:

- directional lights;
- point lights;
- spot lights;
- shadow atlas/view data;
- ambient/environment settings.

Do not expose internal `GpuScene` as the only route. Provide bind group helper APIs.

Files:

- `src/render/lighting/gpu_table.rs`
- `src/render/lighting/pass.rs`
- `src/render/pipeline/contexts.rs`

Acceptance:

- A custom material or compute pass can bind light data through a stable helper.

## Milestone 7: Wicked SSGI Conversion To Real Texture2DArray Compute

Purpose: replace the current packed-2D/render-pass SSGI workaround with a proper Wicked-like compute path.

Current state:

- `SsgiResources` tracks Wicked-like sizes.
- SSGI uses compute/storage textures instead of the old fullscreen workaround.
- SSGI atlas resources are real `Texture2DArray` textures with 16 layers and 4 mips.
- Deinterleave is split into one compute pass per mip to fit the current render graph and wgpu storage texture model.
- Diffuse and upsample passes follow Wicked's shader structure, but still use direct texture loads instead of Wicked's groupshared `R11G11B10` cache.

Target:

- `texture_atlas_depth`: `R32Float`, `Texture2DArray`, 16 layers, 4 mips.
- `texture_atlas_color`: `Rgba16Float` or `R11G11B10` equivalent if supported, `Texture2DArray`, 16 layers, 4 mips.
- `texture_depth_mips`: `R32Float`, 4 mips.
- `texture_normal_mips`: `Rg16Float` or `Rgba16Float`, 4 mips.
- `texture_diffuse_mips`: `Rgba16Float`, 4 mips.

### 7.1 Resource Allocation

Use RenderGraph array-layer and mip support:

- create atlas textures with `array_layer_count(16)` and `mip_level_count(4)`;
- create regular mip textures with `mip_level_count(4)`;
- create subresource views for every mip/layer needed by passes.

Files:

- `src/render/gi/ssgi.rs`
- `src/render/graph/types.rs`
- `src/render/graph/builder.rs`

### 7.2 Compute Deinterleave

Port `_refs/WickedEngine/WickedEngine/shaders/ssgi_deinterleaveCS.hlsl` to WGSL compute.

Keep:

- 8x8 thread group shape where practical.
- 16 slices.
- energy cutoff `all(color <= 1) ? 0 : color`.
- energy loss `0.96`.
- regular depth/normal mip output.

Do not keep:

- exact HLSL pack intrinsics if WGSL format support makes them awkward.
- velocity reprojection in the first compute port unless velocity is ready.

Wicked alignment checklist:

- [x] `Texture2DArray` atlas depth/color resources with 16 slices.
- [x] 2x/4x/8x/16x regular depth and normal mip outputs.
- [x] layer mapping: `flatten2D(st % 4, 4)`.
- [x] atlas coordinate mapping: `st >> 2`.
- [x] light cutoff: `all(color <= 1) ? 0 : color`.
- [x] energy loss: `color *= 0.96`.
- [x] Wicked-style source lattice: source pixel is `regular_pixel * scale`, not a centered box sample.
- [x] normal decode keeps SceneNormalPrepass view-space Z direction; only depth reconstruction flips RH view `-Z` into Wicked-style positive view distance.
- [x] tests lock SSGI normal/depth convention.
- [ ] velocity reprojection of the lit input (`prevUV = uv + velocity`).
- [ ] single-pass groupshared 2x/4x/8x/16x fan-out.
- [ ] packed `R11G11B10` cache path.
- [ ] octahedral `Rg16Float` normal storage; current path stores `Rgba16Float` encoded view normals.

Files:

- `src/render/shaders/gi/ssgi_deinterleave_compute.wgsl`
- `src/render/gi/ssgi.rs`

Tests:

- compile/run `modern_3d_ssgi_executes_with_standard_material_geometry`
- resource layout tests for array layers and mips

### 7.3 Compute Diffuse

Port `ssgiCS.hlsl` more directly:

- atlas depth/color inputs as `texture_2d_array`;
- regular normal mip input;
- diffuse mip storage output;
- wide and narrow variants:
  - 16x wide range 8 spread 2
  - 8x wide range 4 spread 4
  - 4x narrow range 2 spread 2
  - 2x narrow range 2 spread 2

Wicked alignment checklist:

- [x] atlas depth/color inputs.
- [x] regular normal mip input.
- [x] normal falloff: `saturate(dot(origin_normal, origin_to_sample))`.
- [x] depth rejection: `saturate(1 + origin_to_sample.z * depth_rejection_rcp)`.
- [x] DDA occlusion replacement sample.
- [x] range/spread schedule: 2x `(2,2)`, 4x `(2,2)`, 8x `(4,4)`, 16x `(8,2)`.
- [ ] groupshared tile cache.
- [ ] `group_valid` early-out for empty light tiles.
- [ ] Wicked exact `decode_oct(input_normal[interleaved_pixel].rg)` path.

Files:

- `src/render/shaders/gi/ssgi_compute.wgsl`
- `src/render/gi/ssgi.rs`

### 7.4 Compute Upsample

Port `ssgi_upsampleCS.hlsl`:

- 16x to 8x: range 3 spread 2
- 8x to 4x: range 2 spread 3
- 4x to 2x: range 1 spread 2
- 2x to output: range 1 spread 1
- use high/low depth and normal mips;
- final output composites into scene color or writes indirect diffuse depending on settings.

Wicked alignment checklist:

- [x] low/high depth and normal inputs.
- [x] bilateral depth weight with threshold `0.1`.
- [x] bilateral normal power default `64`.
- [x] upsample schedule: 16x->8x `(3,2)`, 8x->4x `(2,3)`, 4x->2x `(1,2)`, 2x->output `(1,1)`.
- [ ] groupshared gather/cache path from `ssgi_upsampleCS.hlsl`.
- [ ] optional output-to-indirect-diffuse mode; current final path composites into scene color.

Files:

- `src/render/shaders/gi/ssgi_upsample_compute.wgsl`
- `src/render/gi/ssgi.rs`

Acceptance:

- SSGI no longer uses packed 2D atlas.
- Pass count and resource layout are documented and tested.
- `cargo test --features app ssgi` passes.
- `cargo test --features app render` passes.

## Milestone 8: TAA And Sharpen

Purpose: clarity and temporal stability without Kajiya-like blur.

### Tasks

- Add `TemporalAntiAliasingSettings`:
  - enabled
  - feedback
  - jitter scale
  - history clamp
  - sharpen amount
- Add Halton jitter sequence.
- Use velocity + depth for reprojection.
- Add neighborhood clamp.
- Keep motion blur disabled by default.
- Keep sharpen separate and controllable.

Files:

- `src/render/component/settings.rs`
- `src/render/postfx/taa.rs`
- `src/render/shaders/postfx/taa.wgsl`
- `src/render/runtime/history.rs`

Tests:

- `taa_settings_default_disabled`
- `taa_history_ping_pongs`
- `taa_jitter_sequence_is_stable`

Acceptance:

- Static scene edges are stable.
- No obvious blur when camera is still.
- User can disable TAA and keep sharpen.

## Milestone 9: Debug And Inspection Tools

Purpose: renderer work is impossible without seeing intermediate buffers.

### 9.1 Debug View System

Add a user-facing debug mode:

```rust
RenderDebugView::None
RenderDebugView::SceneDepth
RenderDebugView::SceneNormal
RenderDebugView::Albedo
RenderDebugView::Roughness
RenderDebugView::Metallic
RenderDebugView::Velocity
RenderDebugView::SsgiDiffuseMip(u32)
RenderDebugView::SsgiAtlasLayer { mip: u32, layer: u32 }
```

Files:

- `src/render/component/settings.rs`
- `src/render/postfx/debug_view.rs`
- `src/render/shaders/postfx/debug_view.wgsl`

### 9.2 Texture Dump

Add CPU readback helper for tests and debugging:

- dump texture to `.hdr`, `.png`, or raw `.bin`;
- support single mip/layer;
- gated behind debug/dev API.

Files:

- `src/render/gpu/readback.rs`
- `src/render/expert.rs`

Acceptance:

- Can inspect SSGI atlas layers without changing shader code.

## Milestone 10: Documentation And Examples

Purpose: make the programmable model teachable.

### Examples

Add:

- `examples/render/modern_3d.rs`
  - indoor occluders;
  - direction light;
  - point light;
  - normal mapped material;
  - roughness/metallic variation;
  - SSGI toggle;
  - debug view toggle.
- `examples/render/custom_graph_pass.rs`
  - reads scene depth/normal;
  - writes a custom color effect;
  - uses a persistent history texture.
- `examples/render/custom_compute_postfx.rs`
  - uses storage texture compute;
  - demonstrates user-programmable pass.

### Docs

Update:

- `docs/render.md`
- `docs/render_api.md`
- `docs/wicked_modern_3d_plan.md`

Add sections:

- pipeline builder basics;
- phase vs render pass vs compute pass vs postfx;
- scene resources;
- history textures;
- debug views;
- backend boundary;
- what is public API vs expert API.

Acceptance:

- A user can write a custom post effect by following docs only.
- A user can insert a custom compute pass into `modern_3d()`.

## Milestone 11: Public API Stability Pass

Purpose: make the programmable API coherent before it spreads.

### Review Checklist

- Are names consistent?
  - `TextureSpec` vs `TextureDesc`
  - `SceneTexture` vs `SceneGBufferSlot`
  - `GraphPass` vs `ComputePass`
- Are advanced APIs under `render::expert` when appropriate?
- Are common APIs under `render::*`?
- Does `RenderPipelineAsset::modern_3d()` remain simple?
- Can examples avoid private modules?
- Is Kajiya still hidden behind backend?

### Compatibility Tests

Run:

```powershell
cargo check --examples --features app
cargo test --features app render
cargo test --features app graph
```

Also verify:

```powershell
rg "kajiya::" src examples -n
rg "Wicked" src/render -n
```

Expected:

- Kajiya direct references only inside backend/vendor/reference paths.
- Wicked references only in comments/docs/license attribution, not runtime dependencies.

## Concrete Implementation Order

Recommended order from here:

1. Texture usage in RenderGraph descriptors.
2. Subresource declarations and physical subresource view helpers.
3. Compute pipeline/cache helper.
4. Storage texture bind helpers.
5. SceneTexture request helpers in pass contexts.
6. History texture pool.
7. Velocity buffer completion.
8. Convert SSGI to real compute + `Texture2DArray`.
9. Add TAA.
10. Add debug views and texture dumps.
11. Add user examples and docs.

Do not start with TAA or DDGI before steps 1-5. Those features will otherwise create more private shortcuts.

## Risk List

- `wgpu` storage texture format limitations may force `Rgba16Float` instead of `R11G11B10`.
- Texture array render attachment support requires careful per-layer `D2` views.
- Subresource dependency tracking can become complex; start conservative.
- Persistent history per view needs stable view identity. If view identity is weak, history will flicker or leak across cameras.
- Motion vectors require previous model matrices; missing model history will break TAA.
- Too much API in `render::*` can overwhelm normal users. Keep advanced knobs in `render::expert` unless they are common.

## Definition Of Done

SkyEngine can claim "modern, clear, decoupled, pluggable, user-programmable render pipeline" when all of the following are true:

- A user can build a custom pipeline from public APIs.
- A user can insert custom render, compute, and post-fx passes.
- A user pass can declare graph resources without touching private graph internals.
- A user pass can read canonical scene depth/normal/albedo/material/velocity.
- A user pass can write storage textures.
- A user pass can use persistent history textures.
- `modern_3d()` provides a stable PBR + shadow + GI + post stack.
- SSGI runs through compute/storage resources rather than fullscreen workaround passes.
- Debug views can inspect GBuffer and GI resources.
- Examples compile and demonstrate the custom pass path.
- Kajiya and Wicked do not leak into the public user API.
