# AGENTS.md — `src/render`

## Overview
- This module is SkyEngine's `wgpu`-based 2D rendering framework.
- It provides a layered architecture: core GPU primitives → declarative render graph → high-level passes & post-processing → resource management.
- The GPU backend is `wgpu` (WebGPU/Vulkan/DX12/Metal).  All rendering goes through `GpuContext` (`src/gpu/context.rs`).
- Shader language is WGSL.  All shaders live under `shaders/`.
- The module is gated behind `features = ["app"]` for window/surface-dependent code.  The optional `live2d` sub-module requires `features = ["live2d"]`.

## Module Architecture

```
render/
├── core/         — Foundational GPU types (camera, color, texture, render target, fullscreen pass)
├── graph/        — Declarative render graph system (has its own AGENTS.md)
├── passes/       — High-level rendering passes (SpriteBatch, LightPass, CompositePass)
├── postfx/       — Post-processing effect chain (Bloom, ToneMap, Vignette)
├── resources/    — Shared resource systems (TextureAtlas, Blackboard, Material)
├── shaders/      — All WGSL shader sources
├── live2d/       — Live2D Cubism model renderer (has its own AGENTS.md, feature-gated)
├── light.rs      — Light2D descriptor and color temperature utility
└── mod.rs        — Module wiring and public re-exports
```

## Sub-Module AGENTS.md References
- **Render Graph**: see [`graph/AGENTS.md`](graph/AGENTS.md) for the full render graph compilation pipeline, handle model, aliasing, reordering, and execution model.
- **Live2D**: see [`live2d/AGENTS.md`](live2d/AGENTS.md) for the Cubism runtime, renderer architecture, and GPU infrastructure requirements.

## File Map

### `mod.rs`
- Module declarations and canonical public re-exports.
- All downstream code should import render types through `sky_engine::render::*`, not through internal sub-module paths.

### `light.rs`
- `Light2D` — 2D point light descriptor with position, color, radius, intensity, temperature (Kelvin), and falloff.
- `color_temperature()` — approximate Planckian-locus RGB from Kelvin (1000–15000 K range, clamped and normalized).
- Builder-style API: `Light2D::new(x, y, radius).temperature(3000.0).intensity(2.0)`.

---

### `core/` — Foundational GPU Types

#### `core/camera.rs`
- `Camera2D` — 2D orthographic camera with position, zoom, and viewport dimensions.
- `CameraUniform` — `#[repr(C)]` GPU-ready struct: 4×4 view-projection matrix, camera params, viewport info.  Shared by sprite, light, and fullscreen passes.
- Coordinate convention: origin at screen centre, +X right, +Y up.
- `screen_to_world()` converts screen pixels to world coordinates (Y-flipped).
- All projection math guards against zero viewport/zoom (clamps to `f32::EPSILON`).

#### `core/color.rs`
- `Color` — linear 32-bit RGBA (0.0–1.0 per channel).
- Constructors: `new`, `rgb`, `rgba8`, `hex` (0xRRGGBB), `hsl` (H 0–360, S/L 0–1).
- Named constants: `WHITE`, `BLACK`, `RED`, `GREEN`, `BLUE`, `YELLOW`, `CYAN`, `MAGENTA`, `TRANSPARENT`.
- `premultiply()` for pre-multiplied alpha blending.
- Implements `From<[f32; 4]>` and `Into<[f32; 4]>`.

#### `core/texture.rs`
- `Texture` — `Arc`-wrapped GPU texture with default view.  Cheaply cloneable, reference-counted.
- Samplers are **not** bundled — use `GpuContext::sampler_linear()` / `sampler_nearest()` when creating bind groups.
- Creation paths:
  - `from_rgba8()` / `from_rgba8_with_label()` / `from_rgba8_with_format()` — raw pixel upload.
  - `from_upload_desc()` / `try_from_upload_desc()` — explicit descriptor API.
  - `from_png()` / `from_file_desc()` / `try_from_file_desc()` — file loading (behind `feature = "asset"`).
- Procedural texture generators: `white_pixel()`, `checkerboard()`, `circle()`, `flat_normal()`, `circle_normal()`.
- `TextureUploadDesc` and `TextureFileDesc` — builder-style descriptor types.
- `TextureError` — validation errors for size mismatches and file load failures.

#### `core/target.rs`
- `RenderTarget` — persistent off-screen texture with view, auto-resizable.
- Usage flags: `RENDER_ATTACHMENT | TEXTURE_BINDING | COPY_SRC | COPY_DST`.
- `resize()` recreates the texture only if dimensions or format actually changed.  Clamps to 1×1 minimum (avoids wgpu panics on window minimize).
- Implements `ColorTargetView` trait for `GpuContext::with_render_pass()` compatibility.
- Used extensively by the render graph as the physical backing for transient/persistent textures.

#### `core/fullscreen.rs`
- `FullscreenPass` — stateless fullscreen triangle drawer.  `draw(pass)` emits a 3-vertex draw call (vertex-index-generated fullscreen triangle).
- `FullscreenPipeline` — compiled fullscreen pipeline with per-format pipeline caching (`FxHashMap<TextureFormat, Arc<RenderPipeline>>`).
- `compose_fullscreen_shader()` — prepends the shared `fullscreen.wgsl` vertex shader to a fragment shader source.
- All post-fx passes (Bloom, ToneMap, Vignette) and CompositePass are built on top of `FullscreenPipeline`.

---

### `passes/` — High-Level Rendering Passes

#### `passes/batch.rs`
- `SpriteBatch` — GPU-instanced 2D sprite renderer.  Handles thousands of sprites in minimal draw calls.
- Per-instance data: transform (x, y, w, h), rotation (sin/cos), color (RGBA), UV rect.
- `MAX_SPRITES = 262,144` per batch before overflow warning.
- Workflow: `begin()` → `set_texture()` / `unset_texture()` → `draw(Sprite)` → `draw_to_surface()` / `draw_to_target()`.
- Automatic draw command batching: texture changes trigger new draw commands; same-texture sprites are coalesced.
- Pipeline caching per target format (separate pipelines for textured vs color-only fragments).
- `Sprite` — per-sprite descriptor with builder-style `.rotation()`, `.color()`, `.uv()`.

#### `passes/light_pass.rs`
- `LightPass` — instanced additive light accumulation pass with normal map support.
- `MAX_LIGHTS = 4,096` per frame (with truncation warning).
- Renders instanced quad lights with position, radius, color (temperature-adjusted), and falloff.
- Supports optional normal map input; falls back to flat (+Z) normal when none provided.
- Additive blending (`One + One`) accumulates light contributions into a lightmap `RenderTarget`.
- Ambient light is applied as the clear color of the lightmap.
- Per-format pipeline caching.

#### `passes/composite_pass.rs`
- `CompositePass` — fullscreen scene × lightmap compositing pass.
- Multiplies `scene_color * lightmap_color → output`.
- Built on `FullscreenPipeline` with a dual-texture bind group (scene + lightmap).
- Can render to either a `RenderTarget` or the presentation surface.

---

### `postfx/` — Post-Processing Effects

#### `postfx/mod.rs`
- `PostFx` trait — shared interface for post-processing passes: `apply_to_target(&mut self, ctx, input, output)`.
- All effects implement this trait for uniform chaining.

#### `postfx/bloom.rs`
- `Bloom` — multi-pass bloom with configurable threshold, intensity, and radius.
- Pipeline: bright pass → 4-level downsample → per-level horizontal+vertical Gaussian blur → upsample (additive) → combine.
- Internal mip chain and temp targets auto-resize to match input dimensions.
- 5 distinct `FullscreenPipeline` instances (bright, downsample, blur, upsample, combine).

#### `postfx/tonemap.rs`
- `ToneMap` — HDR tone mapping with configurable exposure and gamma.
- Can apply to either a `RenderTarget` or the presentation surface.
- Built on `FullscreenPipeline`.

#### `postfx/vignette.rs`
- `Vignette` — screen-edge darkening with configurable intensity and smoothness.
- Can apply to either a `RenderTarget` or the presentation surface.
- Built on `FullscreenPipeline`.

---

### `resources/` — Shared Resource Systems

#### `resources/atlas.rs`
- `TextureAtlas` — packed texture atlas with named UV region lookup.
- `AtlasPacker` — builder API with shelf-based rectangle packing (tallest-first sort, 1px padding).
- Validates duplicate names and pixel data size mismatches.
- `UvRect` — UV rectangle with `to_array()` for direct use with `Sprite::uv()`.

#### `resources/blackboard.rs`
- `Blackboard` — typed key-value store for cross-pass data sharing.  Inspired by SakuraEngine.
- Values stored as `Box<dyn Any>` with downcasting on retrieval.
- Owned by `RenderGraph` and cleared on `reset()`.
- API: `set()`, `get::<T>()`, `get_mut::<T>()`, `contains()`, `remove::<T>()`, `clear()`.

#### `resources/material.rs`
- `MaterialProperties` — typed uniform buffer with named float/vector properties.
  - Properties are laid out with GPU-compatible alignment (4/8/16 bytes).
  - Dirty-tracking: `upload()` only writes when modified.
  - API: `set_float`, `set_vec2`, `set_vec3`, `set_vec4` (with `try_` fallible variants).

- `MaterialResourceBindings` — manages a bind group layout and runtime bind group for textures/samplers/buffers.
  - `set_resources()` builds the bind group from provided `BindingResource`s.
  - Validates binding count and duplicate binding indices.

- `MaterialPipelineCache` — shader + per-format render pipeline cache.
  - `MaterialPipelineDesc` — describes vertex/fragment entry points, blend state, vertex layout, bind group slot assignments.
  - Automatic bind group slot resolution: properties and resources slots are auto-assigned if not specified, and conflict-checked.

- `MaterialInstance` — combines `MaterialProperties` + `MaterialResourceBindings` into one reusable unit.

---

### `shaders/` — WGSL Shader Sources

| File              | Purpose                                               |
|-------------------|-------------------------------------------------------|
| `fullscreen.wgsl` | Shared fullscreen triangle vertex shader              |
| `sprite.wgsl`     | Instanced sprite vertex + textured/color-only fragment|
| `light.wgsl`      | Instanced 2D light accumulation with normal mapping   |
| `composite.wgsl`  | Scene × lightmap multiplication                       |
| `bloom.wgsl`      | Bright extract, downsample, blur, upsample, combine   |
| `tonemap.wgsl`    | HDR tone mapping (Reinhard + gamma correction)        |
| `vignette.wgsl`   | Screen-edge vignette effect                           |
| `live2d.wgsl`     | Live2D model rendering (feature-gated)                |

- `fullscreen.wgsl` is prepended to all post-fx shaders via `compose_fullscreen_shader()`.
- All shaders are included at compile time via `include_str!()`.

---

## Rendering Pipeline (Typical Frame)

A typical lit 2D scene frame follows this order:

1. **Scene pass**: `SpriteBatch::draw_to_target()` → renders sprites into an HDR `RenderTarget`.
2. **Light pass**: `LightPass::render()` → renders lights into a lightmap `RenderTarget` (with ambient clear).
3. **Composite pass**: `CompositePass::render_to_target()` → multiplies scene × lightmap → composited `RenderTarget`.
4. **Post-FX chain**: `Bloom::apply()` → `ToneMap::apply_to_target()` → `Vignette::apply_to_surface()`.
5. **Present**: `GpuContext::end_frame()` submits and presents.

## Pipeline Caching Pattern

All rendering passes use per-target-format pipeline caching via `FxHashMap<TextureFormat, Arc<RenderPipeline>>`.  This avoids redundant pipeline creation when rendering to different format targets across frames.  The pattern is used consistently in:
- `SpriteBatch` (separate color and textured pipeline maps)
- `LightPass`
- `FullscreenPipeline` (used by CompositePass and all PostFx)
- `MaterialPipelineCache`

## Implementation Guidelines
- All new passes that render to `RenderTarget` must support per-format pipeline caching.
- New fullscreen effects should build on `FullscreenPipeline` + `compose_fullscreen_shader()`, not create standalone vertex shaders.
- Textures loaded for Live2D use `Rgba8Unorm` (linear); all other sprite textures default to `Rgba8UnormSrgb` (sRGB).  Do not mix these up.
- `RenderTarget` always includes `COPY_SRC | COPY_DST` usage — this is intentional for render graph copy ops.
- Do not bundle samplers with `Texture` objects.  Sampler selection happens at bind group creation time.
- Keep the `PostFx` trait simple — `apply_to_target(ctx, input, output)`.  Surface-targeting variants are pass-specific convenience methods, not trait methods.
- Uniform buffers must follow WGSL alignment rules (16-byte struct alignment, 4/8/16 per field).
- When adding new shader files to `shaders/`, remember to update this file map.

## Test Commands
- Run all render tests: `cargo test --features app`
- Run render graph tests: `cargo test --features app graph`
- Run specific pass tests: `cargo test --features app render::passes`
- Run post-fx tests: `cargo test --features app render::postfx`
- Run material tests: `cargo test --features app render::resources::material`
- Run atlas tests: `cargo test --features app render::resources::atlas`
- Run blackboard tests: `cargo test --features app render::resources::blackboard`
- GPU-dependent tests require a GPU-capable environment and use `GpuContext::new_headless()`.

## Relation to Other Modules
- **GPU**: `src/gpu/context.rs` provides `GpuContext` — the wgpu device/queue/surface wrapper.  All render code takes `&GpuContext` or `&mut GpuContext`.
- **ECS**: render passes and resources are not ECS-aware.  Integration happens at the application level (`src/app/runner.rs`), which calls render code during the frame loop.
- **App**: `AppRunner` manages the winit event loop and `GpuContext` lifecycle.  Behind `features = ["app"]`.
