# Material Resource Direct Rewrite Plan

## Position

This is a direct rewrite plan for SkyEngine's material subsystem.

No compatibility layer is required. The current material API, storage model,
pipeline cache shape, handle naming, and custom-material trait can be broken
freely if the replacement is cleaner.

The target is not to split the current large module mechanically. The target is
to replace it with a material system whose concepts are explicit enough for
built-in materials, custom materials, shadows, GI, prepasses, and future render
features to share one path without renderer-private hooks leaking into user
code.

## Current State

The active material implementation is now rooted at:

```text
src/render/resources/material/mod.rs
```

That root is a thin module facade. The material implementation has been split
into focused files such as:

```text
binding.rs
builtins/
dirty_queue.rs
id.rs
instance.rs
instance_store.rs
interface.rs
model.rs
pass.rs
pipeline.rs
prepare.rs
prepared.rs
records.rs
registry.rs
scene.rs
scene_binding.rs
shader.rs
storage.rs
```

Removed compatibility exports/files include:

```rust
MaterialBindContext
MaterialResourceBindings
MaterialInstance
SpriteMaterialModel
UnlitMaterialModel
StandardMaterialModel
```

The built-in material data types remain the models directly: `SpriteMaterial`,
`UnlitMaterial`, and `StandardMaterial`.

## Rewrite Rules

- Remove the old material subsystem as an implementation dependency.
- Do not keep deprecated shims.
- Do not keep old import paths solely for compatibility.
- Do not keep the old `Material` trait shape.
- Do not keep the old typed storage API if it fights the new registry model.
- Do not preserve old pipeline cache keys.
- Do not preserve raw bind group slot assumptions in user-facing APIs.
- Do not special-case `StandardMaterial` throughout renderer internals.
- Update call sites to the new API in the same rewrite.
- Keep the vertical slice small enough to land, but make the architecture final.

## End State

The material subsystem should be a real module tree:

```text
src/render/resources/material/
├── mod.rs
├── error.rs
├── id.rs
├── model.rs
├── instance.rs
├── interface.rs
├── shader.rs
├── binding.rs
├── properties.rs
├── prepare.rs
├── prepared.rs
├── registry.rs
├── pipeline.rs
├── pass.rs
├── scene.rs
├── debug.rs
└── builtins/
    ├── mod.rs
    ├── sprite.rs
    ├── unlit.rs
    ├── standard.rs
    └── common.rs
```

`mod.rs` should mostly be curated exports, not implementation.

## Core Design

### Material Model

A material model is the static type-level definition of a material family.

Examples:

- sprite;
- unlit mesh;
- standard PBR;
- toon;
- water;
- custom user material.

It declares:

- name;
- shader sources and entry points;
- vertex requirements;
- material bind group layout;
- scene resource requirements;
- render state;
- pass participation;
- variant policy;
- how CPU data prepares GPU state.

Candidate shape:

```rust
pub trait MaterialModel: Send + Sync + 'static {
    type Data: Clone + Send + Sync + 'static;

    fn interface() -> MaterialInterface;

    fn variant(data: &Self::Data, ctx: &MaterialVariantContext<'_>) -> ShaderVariantKey;

    fn prepare(
        data: &Self::Data,
        ctx: &mut MaterialPrepareContext<'_>,
    ) -> Result<PreparedMaterial, MaterialError>;
}
```

`MaterialModel` replaces the current instance-oriented `Material` trait.

### Material Instance

A material instance is renderer-owned CPU data plus version state.

It should store:

- model id;
- generation;
- version;
- CPU `Data`;
- last prepared version;
- last selected variant;
- optional debug label.

Per-instance values should not affect pipeline identity unless they change a
declared shader variant or render state dimension.

### Prepared Material

Prepared material is the GPU-facing result of preparing an instance.

It should contain:

- material bind group;
- uniform/storage buffers owned by the prepared state;
- texture/sampler bindings;
- dynamic offset metadata if needed;
- selected shader variant key;
- prepared version;
- debug label;
- optional diagnostics.

Draw code should consume `PreparedMaterial` without downcasting into the user
material data.

### Material Interface

`MaterialInterface` is the static contract between a model and the renderer.

Candidate fields:

```rust
pub struct MaterialInterface {
    pub name: &'static str,
    pub shader: MaterialShaderSet,
    pub vertex: VertexRequirements,
    pub bindings: MaterialBindingLayout,
    pub scene: SceneResourceRequirements,
    pub render_state: MaterialRenderState,
    pub passes: MaterialPassSet,
    pub variants: ShaderVariantPolicy,
}
```

The interface must be validated at registration time.

Validation should catch:

- duplicate material bindings;
- invalid binding visibility;
- duplicate variant dimensions;
- missing required shader entries if cheaply knowable;
- impossible pass/render-state combinations;
- scene resources declared as ad-hoc material bindings.

## IDs And Handles

Use generational IDs.

Recommended split:

```rust
pub struct MaterialModelId {
    index: u32,
    generation: u32,
}

pub struct MaterialInstanceId {
    index: u32,
    generation: u32,
}

pub struct MaterialHandle<M: MaterialModel> {
    id: MaterialInstanceId,
    marker: PhantomData<fn() -> M>,
}

pub struct ErasedMaterialHandle {
    model: MaterialModelId,
    instance: MaterialInstanceId,
}
```

Rules:

- user insertion returns typed `MaterialHandle<M>`;
- ECS-facing renderer components may store erased handles;
- stale generations fail lookup;
- wrong model lookup fails clearly;
- draw extraction converts typed handles to erased handles only at boundaries.

## Registry

`MaterialRegistry` owns models, instances, prepared state, and pipeline cache
coordination.

Responsibilities:

- register material models;
- store model interfaces;
- validate interfaces once;
- insert/update/remove instances;
- track dirty instances;
- prepare dirty instances before draw;
- expose prepared materials to phases;
- expose debug summaries;
- own or coordinate the material pipeline cache.

The registry should not contain built-in shader logic. Built-ins live in
`builtins/` and implement `MaterialModel`.

Recommended user shape:

```rust
let handle = renderer.insert_material::<StandardMaterial>(StandardMaterial {
    albedo: Color::WHITE,
    roughness: 0.55,
    ..Default::default()
});

renderer.set_material(handle, |mat| {
    mat.roughness = 0.8;
})?;
```

## Scene Resources

Materials request scene capabilities, not concrete bind group slots.

```rust
pub enum SceneResourceKind {
    Camera,
    Model,
    Lighting,
    Shadows,
    Gi,
    SceneDepth,
    SceneNormal,
    SceneVelocity,
    MaterialPrepass,
}
```

Examples:

- sprite requests camera;
- unlit requests camera and model;
- standard requests camera, model, lighting;
- standard requests shadows only when the selected policy needs shadow sampling;
- standard requests GI only when renderer policy and material policy enable it.

The renderer maps scene capabilities to concrete bind groups. Custom materials
must not need to know internal scene bind group slots.

## Material Bindings

Material bindings are separate from scene resources.

Candidate API:

```rust
pub enum MaterialBinding {
    Uniform {
        binding: u32,
        size: wgpu::BufferSize,
        visibility: wgpu::ShaderStages,
    },
    Texture2D {
        binding: u32,
        sample_type: wgpu::TextureSampleType,
        visibility: wgpu::ShaderStages,
    },
    Sampler {
        binding: u32,
        kind: wgpu::SamplerBindingType,
        visibility: wgpu::ShaderStages,
    },
    StorageBuffer {
        binding: u32,
        read_only: bool,
        visibility: wgpu::ShaderStages,
    },
}
```

Rules:

- duplicate binding numbers are registration errors;
- material bind group layout is derived from declarations;
- binding resources are prepared by the model;
- scene resources are never declared as material bindings;
- a material can have zero material bindings.

## Vertex Requirements

Materials declare semantic requirements, not fixed offsets.

Required semantics:

- `Position`;
- `Normal`;
- `Tangent`;
- `Uv0`;
- `Color0`;
- sprite-specific packed data if the sprite path needs it.

Rules:

- pipeline creation resolves requirements against actual mesh vertex layouts;
- mesh layouts may contain extra attributes;
- missing required semantics produce `MaterialError::MissingVertexAttribute`;
- incompatible formats produce `MaterialError::VertexAttributeFormatMismatch`;
- standard PBR without normal map should not require tangent;
- standard PBR with normal map must require tangent;
- sprite should use a sprite vertex contract, not mesh PBR requirements.

## Shader Variants

Variants must be explicit and auditable.

Good variant dimensions:

- alpha mode;
- normal map on/off;
- receive shadows on/off when it removes bindings or shader code;
- GI sampling on/off when it removes bindings or shader code;
- prepass mode;
- skinning/morphing later;
- shading model.

Bad variant dimensions:

- albedo color;
- roughness value;
- metallic value;
- emissive color;
- texture asset identity;
- UV scale.

Expose debug stats:

- active variants per model;
- active pipelines per model;
- selected variant per instance;
- variant dimensions declared by each model.

## Pipeline Cache

Pipeline keys should be transparent and derived from explicit state.

Key inputs:

- material model id;
- shader identity;
- shader variant key;
- pass/phase id;
- vertex layout fingerprint;
- scene resource layout key;
- material binding layout key;
- color target formats;
- depth target format;
- sample count;
- render state.

Key inputs must not include:

- material instance id;
- albedo color;
- roughness;
- metallic;
- texture handle identity unless it changes a variant.

The pipeline cache should move from trait-instance-driven compilation to
interface-and-variant-driven compilation.

## Pass Participation

Materials explicitly declare pass participation.

Candidate shape:

```rust
pub struct MaterialPassSet {
    pub main: MainPassMode,
    pub prepass: Option<MaterialPrepassMode>,
    pub shadow: ShadowPassMode,
}
```

Rules:

- opaque, alpha-mask, and transparent paths are explicit;
- transparent materials cannot silently enter opaque-only phases;
- alpha-mask standard materials can cast alpha-tested shadows;
- alpha-blend standard materials can cast transparent shadows only if enabled;
- unlit defaults to no shadows;
- sprite defaults to no 3D shadow/prepass participation;
- pass participation appears in material debug output.

## Built-In Materials

### Sprite

Purpose: efficient 2D rendering.

Data:

- color;
- UV rect;
- texture handle;
- alpha mode if needed.

Interface:

- sprite vertex requirements;
- camera scene resource;
- material texture/sampler/uniform bindings;
- transparent or alpha-mask main pass;
- no 3D shadows;
- no GI.

### Unlit

Purpose: simplest 3D mesh material and first rewrite slice.

Data:

- color;
- optional albedo texture;
- alpha mode.

Interface:

- position and UV when textured;
- camera and model scene resources;
- material uniform/texture/sampler bindings;
- opaque or transparent main pass;
- no lighting;
- no shadows;
- no GI.

### Standard

Purpose: default PBR material.

Data:

- albedo color and optional texture;
- normal texture;
- metallic;
- roughness;
- emissive color and optional texture;
- alpha mode;
- receive shadows;
- cast shadow mode;
- GI mode;
- prepass mode.

Interface:

- position, normal, UV;
- tangent only for normal-mapped variant;
- camera and model scene resources;
- lighting scene resource;
- shadows through `SceneResourceKind::Shadows`;
- GI through `SceneResourceKind::Gi`;
- material/prepass participation through declared passes.

`StandardMaterial` may be the built-in PBR model, but renderer internals should
interact with it through the same model/interface/prepare path as custom
materials.

## Custom Material Shape

The replacement API should let custom materials render without editing engine
internals.

Desired shape:

```rust
#[derive(Clone)]
pub struct ToonMaterial {
    pub color: Color,
    pub ramp: TextureHandle,
}

pub struct ToonMaterialModel;

impl MaterialModel for ToonMaterialModel {
    type Data = ToonMaterial;

    fn interface() -> MaterialInterface {
        MaterialInterface::builder("toon")
            .shader(MaterialShaderSet::wgsl(include_str!("toon.wgsl")))
            .vertex(VertexRequirements::standard_mesh())
            .scene(SceneResourceRequirements::new()
                .camera()
                .model()
                .lighting()
                .shadows_optional())
            .binding(MaterialBinding::uniform::<ToonUniform>(0))
            .binding(MaterialBinding::texture_2d(1))
            .binding(MaterialBinding::sampler(2))
            .main_pass(MainPassMode::Opaque)
            .build()
    }

    fn variant(_data: &ToonMaterial, _ctx: &MaterialVariantContext<'_>) -> ShaderVariantKey {
        ShaderVariantKey::default()
    }

    fn prepare(
        data: &ToonMaterial,
        ctx: &mut MaterialPrepareContext<'_>,
    ) -> Result<PreparedMaterial, MaterialError> {
        ctx.bindings()
            .uniform(0, ToonUniform::from(data))
            .texture(1, data.ramp)
            .sampler(2, ctx.samplers().linear())
            .build()
    }
}
```

The exact API can differ, but it must preserve these properties:

- interface is declarative;
- preparation is typed;
- draw code consumes erased prepared state;
- no custom material needs renderer-private binding slots;
- no custom material needs to patch a built-in phase to render.

## Runtime Flow

Target flow:

```text
register material model
    -> validate MaterialInterface
    -> insert/update material instance
    -> mark instance dirty
    -> choose ShaderVariantKey
    -> prepare PreparedMaterial
    -> resolve scene resource layout
    -> build/reuse pipeline from explicit key
    -> draw phase binds scene resources + prepared material + mesh buffers
```

Frame behavior:

- prepare runs before draw extraction or before phase execution;
- unchanged instances reuse prepared state;
- value changes update buffers/bind groups without rebuilding pipelines;
- variant changes only affect the instance and its pipeline lookup;
- removed/stale handles fail before draw;
- missing required scene resources produce clear errors or declared fallbacks.

## Renderer Integration Changes

The rewrite must update these integration points instead of preserving old API
adapters:

- `src/render/mod.rs` exports;
- `src/render/component/mesh.rs` material handle fields;
- `src/render/pipeline/material_registration.rs` material registration;
- `src/render/pipeline/features.rs` built-in registration;
- `src/render/execution/contexts/` material registry access;
- `src/render/runtime/composer.rs` registration and storage access;
- `src/render/runtime/frame_coordinator.rs` and `src/render/runtime/frame/` extraction and batching;
- `src/render/execution/step_nodes/` prepared material lookup;
- render/runtime tests that implement custom materials;
- examples that create `StandardMaterial`, `UnlitMaterial`, or `SpriteMaterial`.

Old APIs should be removed from call sites as they are encountered.

## Execution Plan

### Phase 0: Delete The Old Surface

- Replace `src/render/resources/material/mod.rs` with a thin module root.
- Move or delete old split-attempt files as needed.
- Remove old exports from `src/render/mod.rs`.
- Let compilation errors reveal all old surface usage.

Removed legacy symbols:

- `MaterialBindContext`;
- `MaterialStorage`;
- old `MaterialHandle`;
- `register_material::<M>()` using the old trait;
- `materials::<M>()` and `materials_mut::<M>()` using old typed storage.

### Phase 1: Core Types

Create:

- `error.rs`;
- `id.rs`;
- `shader.rs`;
- `binding.rs`;
- `scene.rs`;
- `pass.rs`;
- `interface.rs`;
- `model.rs`;
- `instance.rs`;
- `prepared.rs`;
- `registry.rs`.

Minimum compile target:

- model registration compiles;
- interface validation has unit tests;
- typed and erased handles compile;
- stale/wrong-model handle tests pass.

### Phase 2: Unlit Vertical Slice

Implement `UnlitMaterial` first.

This slice must prove:

```text
UnlitMaterial data
-> registry insert/update
-> dirty tracking
-> prepared material bind group
-> explicit interface
-> pipeline key/cache
-> mesh draw
-> render smoke test
```

Update enough renderer code for one unlit mesh to render through the new path.

Do not start with `StandardMaterial`; it has too many dependencies and will hide
architecture mistakes.

### Phase 3: Sprite

Port sprite material to the same model/registry/prepared path.

Goals:

- remove sprite-specific material storage assumptions;
- keep sprite batching behavior;
- keep texture fallback behavior;
- ensure transparent ordering still works;
- add smoke coverage.

### Phase 4: Standard PBR

Port standard material after unlit and sprite are stable.

Goals:

- normal-map variant requires tangent;
- plain variant does not require tangent;
- alpha modes select correct pass state;
- shadows are requested through scene capabilities;
- GI is requested through scene capabilities;
- scene material prepass is declared through pass support;
- no scattered `TypeId::of::<StandardMaterial>()` checks remain except temporary
  migration scaffolding removed before phase completion.

### Phase 5: Custom Materials

Port runtime custom material tests to the new `MaterialModel` API.

Required proof:

- custom opaque material renders without engine changes;
- custom transparent material renders without engine changes;
- custom material can opt into scene material prepass;
- custom material can request optional scene resources.

### Phase 6: Cleanup

- Remove dead old files.
- Remove old test assumptions.
- Update examples.
- Update `src/render/AGENTS.md` with the new canonical material API.
- Update public README snippets if they mention old material APIs.

## Error And Diagnostic Requirements

Material errors should include the material model name when possible.

Required diagnostics:

- duplicate material binding number;
- missing required vertex semantic;
- incompatible vertex format;
- unregistered material model;
- stale material handle;
- wrong material model for handle;
- missing prepared material;
- missing required scene resource;
- unsupported material/pass combination;
- shader variant declares resources not present in interface;
- transparent material submitted to opaque-only phase.

Debug surfaces:

- list registered models;
- list instances per model;
- show dirty/prepared versions;
- show selected variant;
- show material bindings;
- show scene requirements;
- show pass participation;
- show pipeline count by model;
- show active variants by model.

## Tests

Add tests with the rewrite rather than after it.

Core tests:

- interface validation rejects duplicate bindings;
- interface validation rejects invalid pass/render-state combinations;
- typed handles reject wrong model use;
- stale handles fail after removal/reuse;
- dirty instance preparation reuses prepared state when unchanged;
- changing a uniform-only value does not change pipeline key;
- changing a variant value changes pipeline key;
- missing vertex attributes fail precisely.

Render tests:

- unlit material pipeline compiles;
- sprite material pipeline compiles;
- standard material pipeline compiles;
- normal-mapped standard material requires tangent;
- custom material renders without engine changes;
- custom prepass material participates in scene material prepass.

Commands:

```powershell
cargo test --features app material -- --nocapture
cargo test --features app standard_material -- --nocapture
cargo test --features app render::runtime -- --nocapture
cargo check --examples --features app
```

## Acceptance Criteria

The rewrite is done when:

- the old material trait/storage/cache API is gone;
- material logic is split into focused files;
- `mod.rs` is a curated export root;
- built-in and custom materials use the same model path;
- material instances update GPU state without unnecessary pipeline rebuilds;
- pipeline variants are declared and inspectable;
- scene resources are requested by capability;
- shadow/GI/prepass participation is declared through material interfaces;
- stale and wrong-typed handles are detected;
- renderer call sites no longer depend on raw material bind group slots;
- examples build with the new API;
- render tests cover unlit, sprite, standard, and custom materials.
