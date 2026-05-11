# AGENTS.md - `src/render/gi`

## Overview
- This module owns provider-driven global illumination.
- The public surface is centered on `GiRuntime`, `GiProviderRegistry`, `GiProviderFactory`, and `GiProviderRuntime`.
- GI is provider-based, not a monolithic renderer path. DDGI and SSGI are separate providers that share the same runtime contract.
- `GiRuntime` is the orchestration shell. Provider-specific data, shader source, GPU bindings, update/composite descriptors, and sampling payloads should stay inside provider modules.

## Module Shape

```text
gi/
+-- mod.rs          - runtime, registry, shared GI contracts
+-- providers/
    +-- mod.rs      - provider registration
    +-- ddgi.rs     - DDGI provider
    +-- ssgi/       - Wicked-inspired SSGI provider
```

## Canonical GI Surface
- `GiRuntime::prepare(...)` is the provider selection and per-frame setup entry point.
- `GiRuntime::update_descriptor()` exposes optional provider-owned compute work.
- `GiRuntime::sampling_binding()` exposes the provider's material-facing bind group.
- `GiRuntime::shader_descriptor()` exposes the provider shader injection source.
- `GiRuntime::composite_descriptor()` exposes optional post-fx style composite work.

## Shared GI Contract
- `GiProviderFactory` creates a `GiProviderRuntime` for the active GPU.
- `GiProviderRuntime` should stay small and explicit:
  - `prepare(...)`
  - optional `update(...)`
  - optional `setup_composite(...)`
  - optional `execute_composite(...)`
  - optional `update_descriptor(...)`
  - `sampling_binding()`
  - `shader_descriptor()`
  - optional `composite_descriptor()`
- `GiSamplingBinding` is what material shaders consume through `SceneBindingKind::GlobalIllumination`.
- `GiShaderDescriptor` is injected into material WGSL through the `/*GI_SHADER*/` placeholder.

## Provider Rules
- Keep DDGI and SSGI isolated from each other.
- Use provider-local types for settings, GPU state, and internal layouts.
- Do not push provider-specific fields into shared GI runtime state unless they are truly cross-provider.
- If a provider needs a sampling shader, return it through `shader_descriptor()` rather than coupling it to material code directly.

## SSGI-Specific Notes
- SSGI should be aligned to the Wicked-style contract:
  - produce a sampled indirect diffuse result;
  - keep the material-facing indirect lookup separate from any final scene composite;
  - treat velocity, depth, and normal as first-class scene inputs;
  - keep the pass chain and resource roles explicit.
- In SkyEngine, the decisive integration point is material GI sampling, not scene-color post-fx alone.
- If SSGI exposes a post-fx composite, that should be treated as an optional final step. It should not become the only way GI reaches materials.
- Preserve the distinction between:
  - provider compute/update work;
  - provider sampling shader injection;
  - optional composite presentation.

## Implementation Guidelines
- Keep `GiRuntime` boring. It should select a provider, hand over scene inputs, and expose the current provider's descriptors.
- Prefer provider-owned helper modules over large inline provider bodies.
- When changing GI behavior, check both:
  - material shader injection through `GiShaderDescriptor`;
  - runtime step ordering in `src/render/builtins/gi.rs` and `src/render/execution/step_nodes/phase_step_node.rs`.
- Be careful with scene bindings:
  - `SceneBindingKind::GlobalIllumination` is the material-side contract;
  - `scene_indirect_diffuse` in the execution slots is the runtime-side target for GI output textures.

## Validation
- Run GI-focused tests after changing provider logic:
  - `cargo test --features app gi`
  - `cargo test --features app ssgi`
- If material shader injection changes, also run:
  - `cargo check --examples --features app`
- If graph or resource contracts change, run:
  - `cargo test --features app graph`
