# SSGI Architecture Refactor Plan

## Purpose

Refactor SkyEngine's SSGI provider from a large mixed-responsibility pass into a clear screen-space GI technique pipeline.

This is not just a file split. The goal is to make the algorithm architecture explicit enough that resource layout, render graph declaration, GPU bindings, compute execution, temporal stability, denoising, upscale, composite, and GI-provider integration can evolve independently without hidden ordering contracts.

The current implementation is rooted at:

```text
src/render/gi/providers/ssgi.rs
```

The desired end state can preserve the public SSGI API while replacing the internal shape.

## Primary References

- Refactoring large classes:
  - Martin Fowler, "This class is too large"
  - Refactoring.Guru, "Extract Class"
- Render graph architecture:
  - Unity Render Graph fundamentals
  - Bevy RenderGraph API
  - Frostbite FrameGraph-style resource/pass dependency model
- Screen-space GI and reconstruction:
  - WickedEngine SSGI as the current inspiration point
  - Intel ASSAO and deinterleaved screen-space processing
  - NVIDIA Deinterleaved Texturing
  - NVIDIA SVGF, "Spatiotemporal Variance-Guided Filtering"
  - AMD FidelityFX SSSR integration contract
  - Unreal Engine SSGI quality model and fallback guidance
  - Activision GTAO/GTSO papers for horizon-based screen-space indirect visibility

Use these as design references, not as code to port wholesale.

## Current Diagnosis

`ssgi.rs` currently combines many reasons to change:

- public API:
  - `SsgiSettings`
  - `global_illumination`
  - `SsgiProviderFactory`
- algorithm constants:
  - mip count
  - atlas layers
  - texture formats
  - pass names
  - sample parameters
- CPU resource planning:
  - target size
  - aligned size
  - atlas size
  - mip dimensions
  - texture specs
- render graph declaration:
  - intermediate texture creation
  - compute pass declaration
  - final render pass declaration
- GPU object ownership:
  - bind group layouts
  - uniform buffer
  - compute pipelines
  - final fullscreen pipeline
- bind group creation:
  - scene inputs
  - deinterleave outputs
  - diffuse inputs and outputs
  - upsample inputs and outputs
  - final composite inputs
- pass execution:
  - pass-name dispatch
  - graph resource lookup
  - subresource view creation
  - uniform writes
  - compute dispatch
  - fullscreen final draw
- GI provider runtime:
  - settings downcast
  - provider lifecycle
  - sampling binding
  - composite descriptor
- debug logging and tests.

The most fragile part is the hidden ordering contract between graph declaration and execution. The graph records reads and writes, and execution later fetches resources by nth-read or nth-write position. That means pass name arrays, graph setup order, bind group binding order, and shader expectations must all stay synchronized manually.

## Non-Goals

- Do not rewrite the whole GI system in this phase.
- Do not replace DDGI or change the global `GiProviderRuntime` trait unless the SSGI refactor proves a small trait adjustment is necessary.
- Do not turn SSGI into a universal GI schema for every provider.
- Do not merge SSGI and DDGI into one renderer.
- Do not optimize before the data-flow contract is clean and testable.
- Do not add ray tracing hardware requirements.
- Do not mechanically split files without reducing coupling.

## Design Principles

- Provider code is orchestration, not algorithm storage.
- Resource layout is pure CPU logic and is unit-testable without `wgpu`.
- Render graph code declares data flow only.
- Execution code consumes compiled pass contracts only.
- Shader bindings are explicit, named, and centralized.
- Pass contracts are the single source of truth for:
  - pass name;
  - pass kind;
  - graph reads and writes;
  - bind group slots;
  - shader entry point;
  - dispatch scale;
  - resource view dimensions.
- Low-sample screen-space GI is a signal reconstruction problem:
  - trace;
  - validate;
  - temporally accumulate;
  - spatially denoise;
  - edge-aware upscale;
  - composite with fallback lighting.
- SSGI is a near-field screen-space layer, not a complete GI solution.
- DDGI, ambient, IBL, or another provider should supply stable low-frequency and off-screen indirect lighting when available.

## End State Module Shape

Replace the single implementation file with a module tree:

```text
src/render/gi/providers/ssgi/
├── mod.rs
├── settings.rs
├── constants.rs
├── layout.rs
├── graph.rs
├── contract.rs
├── uniforms.rs
├── bindings.rs
├── pipelines.rs
├── executor.rs
├── provider.rs
├── debug.rs
└── tests.rs
```

`mod.rs` should mostly be curated exports:

```rust
pub use provider::SsgiProviderFactory;
pub use settings::{global_illumination, SsgiSettings, SSGI_PROVIDER_ID};
pub use layout::{SsgiComputeTextureLayout, SsgiMipLevel, SsgiResources};
```

The public import path should remain stable through re-exports from `src/render/gi/providers/mod.rs`.

## Core Architecture

### Settings Layer

Owns only user-facing configuration and provider id.

Responsibilities:

- `SSGI_PROVIDER_ID`
- `SsgiSettings`
- defaults
- `global_illumination(settings)`
- future quality preset conversion.

Settings should separate concerns:

- artistic controls:
  - intensity;
  - radius;
  - depth rejection;
  - normal rejection;
- performance controls:
  - resolution scale;
  - mip count;
  - ray count or sample pattern;
  - pass budget;
- stability controls:
  - history weight;
  - disocclusion threshold;
  - variance clamp;
  - spatial denoise pass count.

### Layout Layer

Owns pure CPU resource planning.

Responsibilities:

- target size clamping;
- internal alignment;
- deinterleave atlas dimensions;
- regular mip dimensions;
- texture format and usage contract;
- `SsgiResources`;
- `SsgiMipLevel`;
- `SsgiComputeTextureLayout`.

This layer must not import render graph execution contexts or create GPU objects.

### Graph Layer

Owns `RenderGraph` declaration.

Responsibilities:

- create named intermediate textures;
- declare compute pass dependencies;
- declare final composite pass;
- write graph handles into blackboard when needed;
- update scene color state.

This layer should not create bind groups, pipelines, or perform resource view lookup.

### Contract Layer

Owns pass definitions and their stable resource/binding contract.

Responsibilities:

- `SsgiPassKind`;
- pass names;
- pass ordering;
- read and write roles;
- source and output scales;
- shader stage selection;
- bind group shape.

The graph layer and executor layer should both derive from this contract.

Avoid this pattern:

```rust
let depth = pass_nth_read_subresource(pass, 0, "ssgi", "depth");
```

Prefer role-based lookup driven by a pass contract:

```rust
let depth = pass_resources.read(SsgiRead::LowDepth);
```

The exact API can differ, but the concept should be role-based rather than position-based.

### Uniform Layer

Owns CPU-to-shader parameter conversion.

Responsibilities:

- `SsgiUniform`;
- view/projection inverse calculation;
- per-pass sample parameter selection;
- clamp and reciprocal rules;
- debug-friendly derived values.

Uniform construction should be deterministic and unit-testable.

### Bindings Layer

Owns bind group layout and bind group creation.

Responsibilities:

- final composite texture bind group layout;
- uniform bind group layout;
- compute scene input layout;
- deinterleave output layout;
- diffuse input and output layouts;
- upsample input and output layouts;
- role-to-binding mapping.

Shader binding numbers should live here or in `contract.rs`, not inside execution branches.

### Pipelines Layer

Owns GPU pipeline caches.

Responsibilities:

- deinterleave compute pipeline;
- diffuse compute pipeline;
- upsample compute pipeline;
- final fullscreen pipeline;
- shader module source selection;
- entry point selection.

This layer should not know graph resource order.

### Executor Layer

Owns pass execution.

Responsibilities:

- match compiled pass to `SsgiPassKind`;
- resolve role-based graph resources;
- create texture views;
- write uniform buffer;
- bind pipeline and bind groups;
- dispatch compute passes;
- draw final fullscreen composite.

Execution should be short and branch by pass kind, not by raw pass name strings scattered through the code.

### Provider Layer

Owns GI provider integration only.

Responsibilities:

- `SsgiProviderFactory`;
- `SsgiRuntime`;
- settings downcast;
- sampling binding lifecycle;
- `setup_composite`;
- `execute_composite`;
- `composite_descriptor`;
- `shader_descriptor`.

The provider should be a small shell around graph setup and execution.

## Algorithm Pipeline

The SSGI algorithm should be modeled as a technique pipeline:

```text
Scene Inputs
  -> Canonicalize GBuffer
  -> Build Depth/Normal/Color Hierarchy
  -> Low-Resolution or Deinterleaved GI Trace
  -> Temporal Reprojection
  -> History Validation
  -> Variance or Confidence Filtering
  -> Edge-Aware Spatial Denoise
  -> Bilateral Upscale
  -> Composite With Fallback GI
```

The current Wicked-inspired implementation already contains:

- deinterleave;
- low-resolution diffuse calculation;
- upsample;
- final composite.

Future work should add temporal and denoise stages as independent steps rather than folding them into trace or composite.

### Scene Input Contract

Introduce an internal `SsgiInputs` concept, even if it is initially a small struct assembled from existing frame state.

Potential fields:

- scene color or direct-lit color;
- depth;
- normal;
- velocity or motion vectors;
- current view/projection;
- previous view/projection;
- exposure or luminance info;
- fallback indirect lighting input;
- debug mode.

This keeps SSGI from directly reaching into unrelated frame state from many call sites.

### Trace Stage

Trace should produce a raw indirect diffuse signal and confidence/validity metadata when available.

Recommended outputs:

- raw diffuse irradiance;
- depth at trace resolution;
- normal at trace resolution;
- validity or confidence;
- optional variance.

Trace should not composite into scene color directly.

### Temporal Stage

Temporal accumulation should be separate from tracing.

Responsibilities:

- reproject previous SSGI;
- reject disocclusion;
- clamp history against neighborhood statistics;
- track history length or confidence;
- reset on camera or projection discontinuities.

This stage requires motion vectors or robust previous/current matrix reprojection. If motion vectors are not yet ready, keep the stage planned but disabled.

### Spatial Denoise Stage

Spatial filtering should be edge-aware.

Inputs:

- current noisy SSGI;
- depth;
- normal;
- variance or confidence;
- settings.

The denoiser should avoid leaking light across depth and normal discontinuities.

### Upscale Stage

Upscale should remain bilateral and independent from trace.

Inputs:

- lower-resolution SSGI;
- full-resolution depth;
- full-resolution normal;
- low-resolution depth and normal hierarchy.

This keeps half-resolution and quarter-resolution modes easy to add.

### Composite Stage

Composite should be the only stage that modifies scene color.

Responsibilities:

- apply intensity;
- combine with fallback indirect lighting;
- preserve direct lighting energy expectations;
- expose debug output modes.

The composite stage should make the role of SSGI explicit:

```text
final_indirect = fallback_indirect + screen_space_near_field_indirect
```

or another clearly documented energy model.

## Quality Model

Avoid an unstructured pile of floats.

Add a quality preset model before adding more controls:

```text
Off
Low
Medium
High
Ultra
Custom
```

Each preset should map to:

- internal resolution scale;
- mip count;
- trace sample count or step count;
- temporal accumulation enablement;
- spatial denoise pass count;
- upscale mode;
- debug cost budget.

The public `SsgiSettings` can keep low-level custom fields for expert users, but examples should prefer named presets once available.

## Debug And Validation

SSGI needs explicit inspectability before tuning.

Add debug modes over time:

- off;
- raw trace;
- temporal history;
- variance or confidence;
- depth hierarchy;
- normal hierarchy;
- final indirect only;
- composite difference;
- rejected history;
- screen-space miss mask.

Add tests at the layer where they are cheapest:

- layout tests:
  - zero size clamps to one;
  - aligned size matches expected contract;
  - atlas dimensions match deinterleave assumptions;
  - texture formats and usages are stable;
- contract tests:
  - every pass name maps to exactly one pass kind;
  - graph declarations and executor resource roles agree;
  - binding roles are unique within each group;
- graph tests:
  - declared reads and writes match contract;
  - final output keeps the compute chain alive;
  - repeated setup remains deterministic;
- shader validation tests:
  - all WGSL modules compile;
  - normal/depth convention assumptions remain synchronized;
- execution smoke tests where GPU is available:
  - zero-size or tiny target does not dispatch invalid work;
  - pass dispatch dimensions match target resource extents.

## Migration Plan

### Milestone S0: Document Current Contract

Purpose: freeze the intended current behavior before moving code.

Tasks:

- Record current SSGI pass chain:
  - deinterleave 2x, 4x, 8x, 16x;
  - diffuse 16x, 8x, 4x, 2x or current effective order;
  - upsample 16x to 8x, 8x to 4x, 4x to 2x;
  - final full-resolution composite.
- Record current texture formats and usages.
- Record current shader binding numbers.
- Record current pass read/write ordering.

Acceptance:

- A reader can understand the current graph contract without opening `ssgi.rs`.

### Milestone S1: Split Pure CPU Layout

Purpose: isolate resource math.

Tasks:

- Create `ssgi/layout.rs`.
- Move:
  - `SsgiResources`;
  - `SsgiMipLevel`;
  - `SsgiComputeTextureLayout`;
  - texture spec conversion;
  - alignment helpers used only by layout.
- Keep current public re-exports.
- Keep existing layout tests close to the layout module.

Acceptance:

- Layout tests pass.
- No behavior changes in graph or execution.

### Milestone S2: Introduce Pass Contract

Purpose: remove scattered pass-name logic.

Tasks:

- Create `ssgi/contract.rs`.
- Move pass names and `SsgiComputePassKind`.
- Replace raw name arrays with pass descriptors.
- Add tests that all pass names are unique.
- Add tests that every compute descriptor maps to a kind.

Acceptance:

- Existing pass order is preserved.
- There is one source of truth for SSGI pass identity.

### Milestone S3: Split Graph Declaration

Purpose: make data flow readable.

Tasks:

- Create `ssgi/graph.rs`.
- Move:
  - compute resource creation;
  - compute pass declaration;
  - final pass declaration;
  - blackboard key handling if still needed.
- Make graph declaration consume `SsgiResources` and contract descriptors.

Acceptance:

- Graph tests prove declared dependencies match the pass contract.
- The final output keeps all required compute passes alive.

### Milestone S4: Split Bindings And Pipelines

Purpose: centralize shader/GPU interface.

Tasks:

- Create `ssgi/bindings.rs`.
- Create `ssgi/pipelines.rs`.
- Move bind group layout creation out of pass execution.
- Move pipeline cache creation out of pass execution.
- Replace repeated layout creation checks with a small GPU state object.

Acceptance:

- Binding roles and binding numbers are inspectable in one place.
- Pipeline creation does not require scanning executor logic.

### Milestone S5: Split Uniform Construction

Purpose: make shader parameters deterministic and testable.

Tasks:

- Create `ssgi/uniforms.rs`.
- Move `SsgiUniform`.
- Add a constructor that accepts:
  - settings;
  - scene view;
  - pass kind;
  - sample parameters.
- Add tests for clamping and reciprocal behavior.

Acceptance:

- Executor writes an already-built uniform.
- Uniform math is not duplicated between compute and final paths.

### Milestone S6: Split Executor

Purpose: reduce pass execution to role-based resource resolution.

Tasks:

- Create `ssgi/executor.rs`.
- Move compute and final execution.
- Resolve resources by contract roles instead of nth-read/nth-write where practical.
- Keep final fullscreen composite separate from compute dispatch.

Acceptance:

- Unknown pass names fail clearly.
- Resource-role mismatch fails at the contract boundary.
- Execution code no longer owns layout, graph, provider, or settings policy.

### Milestone S7: Thin Provider Runtime

Purpose: make provider lifecycle code boring.

Tasks:

- Create `ssgi/provider.rs`.
- Move `SsgiProviderFactory` and `SsgiRuntime`.
- Provider stores settings and executor state.
- Provider delegates setup to graph code.
- Provider delegates execution to executor code.

Acceptance:

- `provider.rs` can be understood without knowing shader binding details.
- The SSGI public API remains stable.

### Milestone S8: Add Technique Pipeline Extensions

Purpose: prepare SSGI for production stability.

Tasks:

- Add optional internal stage slots:
  - trace;
  - temporal;
  - denoise;
  - upscale;
  - composite.
- Add disabled-by-default temporal state if motion vectors are not ready.
- Add debug modes for raw trace and final indirect.
- Add settings shape for quality presets.

Acceptance:

- New stages can be added without changing provider runtime shape.
- Composite remains the only stage that writes scene color.

## Files Expected To Change

Primary:

```text
src/render/gi/providers/ssgi.rs
src/render/gi/providers/ssgi/mod.rs
src/render/gi/providers/ssgi/settings.rs
src/render/gi/providers/ssgi/constants.rs
src/render/gi/providers/ssgi/layout.rs
src/render/gi/providers/ssgi/graph.rs
src/render/gi/providers/ssgi/contract.rs
src/render/gi/providers/ssgi/uniforms.rs
src/render/gi/providers/ssgi/bindings.rs
src/render/gi/providers/ssgi/pipelines.rs
src/render/gi/providers/ssgi/executor.rs
src/render/gi/providers/ssgi/provider.rs
src/render/gi/providers/ssgi/debug.rs
```

Likely touched:

```text
src/render/gi/providers/mod.rs
src/render/gi/mod.rs
src/render/execution/
src/render/pipeline/
src/render/shaders/gi/
docs/render_deep_dive.md
```

Only touch broader execution or pipeline code if the SSGI contract reveals an existing reusable abstraction, such as role-based pass resource lookup.

## Validation Commands

Run the narrowest useful checks after each milestone:

```bash
cargo test --features app ssgi
cargo test --features app gi
cargo test --features app graph
cargo check --examples --features app
```

For shader or graph execution changes, also run the render examples that exercise modern 3D and GI when the local GPU environment supports them.

## Success Criteria

- `SsgiRuntime` is a small provider shell.
- Resource layout can be tested without GPU setup.
- Render graph declaration can be reviewed without reading execution code.
- Shader binding numbers are centralized.
- Pass identity and resource roles have one source of truth.
- Compute execution no longer depends on fragile nth-read/nth-write assumptions where a role-based contract is available.
- The algorithm pipeline has clear extension points for temporal accumulation, denoising, quality presets, and debug views.
- Public SSGI usage remains simple:

```rust
GlobalIllumination::provider(GiProviderConfig::new(SSGI_PROVIDER_ID, SsgiSettings::default()))
```

or:

```rust
ssgi::global_illumination(SsgiSettings::default())
```
