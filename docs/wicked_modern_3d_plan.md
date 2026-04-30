# Wicked-Inspired SkyEngine 3D Renderer Plan

## Summary

- Rebuild SkyEngine's own modern 3D renderer around WickedEngine-style `RenderPath3D` ideas while keeping Kajiya as a fallback backend.
- Do not copy the C++ engine wholesale. Port the algorithm structure, pass order, resource layout, shader math, and practical defaults into the existing Rust + wgpu + WGSL renderer.
- Do not target hardware ray tracing in the first stage. The first implementation should focus on raster/compute features that fit the current `wgpu` path: PBR, shadows, TAA/sharpen, and SSGI. DDGI and Surfel GI are later work.

## Key Changes

- Add an experimental public entry point: `RenderPipelineAsset::modern_3d()`.
- Keep `RenderPipelineAsset::forward_3d()` and `RenderPipelineAsset::kajiya_3d()` stable.
- Build a Wicked-style pass order:
  `depth/normal/material prepass -> shadow maps -> direct lighting -> SSGI -> TAA/sharpen -> bloom/tone map -> present`.
- Expand `SceneSnapshot` into a fuller 3D frame contract:
  mesh instances, materials, cameras, directional/point/spot lights, shadow flags, and render settings.
- Keep backend rules intact:
  backends consume `SceneSnapshot`; they must not query ECS directly.
- Refactor `src/render/gi` into a broader GI subsystem:
  first add `SsgiPass`, then later improve DDGI, then investigate Surfel GI.
- Port Wicked shader ideas manually to WGSL.
  The first shader target is a simplified version of Wicked's `ssgiCS` / `ssgi_upsampleCS` flow.
- Preserve license attribution for ported WickedEngine-derived algorithms, constants, and shader code.

## Implementation Order

### Milestone 1: Picture Baseline

- Verify the standard PBR input chain: albedo, normal, roughness, metallic, and emissive.
- Strengthen material prepass outputs so compute passes can reliably read:
  `scene_depth`, `scene_normal`, `scene_albedo`, `scene_material`, and `scene_emissive`.
- Add `RenderPipelineAsset::modern_3d()` without replacing `forward_3d()`.

### Milestone 2: Shadows And Clarity

- Improve directional shadows using Wicked-inspired defaults:
  cascades, bias tuning, PCF, and a reserved contact-shadow path.
- Add TAA history/jitter and sharpen controls.
- Keep motion blur disabled by default.
- Acceptance:
  static scenes are stable, shadows do not visibly float, and the image does not become blurry.

### Milestone 3: SSGI

- Add `SsgiResources` and `SsgiPass`.
- Inputs:
  depth, normal, lit color/albedo.
- Output:
  low-resolution indirect diffuse, upsampled and composited into the scene.
- Use a simplified Wicked-style flow:
  deinterleave -> diffuse sample -> upsample.
- Add `GlobalIlluminationMode`:
  `Off`, `Ssgi`, `Ddgi`.
- Keep `Off` as the default.

### Milestone 4: DDGI Rework

- Keep the existing DDGI public settings where practical.
- Rework internals toward Wicked's SH irradiance, depth visibility, probe offset, and blend speed model.
- Prioritize:
  probe sampling quality, visibility leaks, and history stability.
- Do not prioritize multi-bounce complexity in the first DDGI rewrite.

### Milestone 5: RT And Advanced GI

- Do not make hardware RT a requirement for `modern_3d()`.
- If RTX support is added later, put it behind a separate native/backend feature.
- Treat Surfel GI and RTDiffuse as future advanced paths, not blockers for the main renderer.

## Test Plan

- Compile:
  - `cargo check --examples --features app`
  - `cargo test --features app render`
- Add tests for:
  - `modern_3d()` descriptor includes prepass, shadow, SSGI/TAA, and post-fx steps in the expected order.
  - `SceneSnapshot` extracts complete render-frame data without backend ECS queries.
  - SSGI resource sizing and resize behavior are correct.
- Runtime validation:
  - Run `examples/render/three_d_demo.rs` through `modern_3d()`.
  - Add or update a focused `examples/render/modern_3d.rs` scene with indoor occlusion, directional light, point lights, normal maps, roughness, metallic, and emissive materials.
- Regression checks:
  - `forward_3d()` examples still compile.
  - `kajiya_3d()` examples still compile.
  - No WickedEngine or Kajiya types leak into public user APIs.
  - `_refs/WickedEngine` remains a reference checkout, not a runtime dependency.

## Assumptions

- The first target is a controllable, clear, stable real-time renderer, not UE/Lumen parity.
- Kajiya remains available as a comparison and fallback backend.
- WickedEngine is used as an MIT-licensed reference for design and shader math, not as a copied C++ dependency.
- The main renderer path stays on `wgpu + WGSL`; hardware ray tracing is separate future work.
