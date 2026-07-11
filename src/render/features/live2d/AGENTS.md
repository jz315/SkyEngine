# AGENTS.md — `src/render/features/live2d`

## Overview
- This module implements Live2D Cubism loading, runtime update, clipping, and rendering on top of `cubism-sys` and `wgpu`.
- The implementation is Rust-native and binds Cubism Core directly, not the full official Framework.
- The architecture is split into four layers:
  - `asset/` — immutable loaded assets and model-relative metadata
  - `model/` — safe Cubism Core model wrapper and CPU-side state access
  - `runtime/` — mutable per-instance controllers and update chain
  - `render/` — clipping, prepared frames, and renderer execution
- Treat this folder as one vertical pipeline:
  - load `.model3.json` and assets
  - instantiate mutable runtime state
  - update model state every frame
  - prepare GPU work
  - execute mask/model/offscreen/composite passes
  - optionally feed the prepared work into the high-level transparent-phase integration

## Canonical Entry Points
- Load immutable assets:
  - `asset::Live2DModelResource::load()`
- Instantiate mutable runtime state:
  - `asset::Live2DModelResource::instantiate()`
  - `runtime::Live2DUserModel::from_resource(...)`
- Advance runtime:
  - `runtime::Live2DUserModel::update(dt)`
- Finalize CPU model state:
  - `model::Live2DModel::update()`
- Prepare rendering:
  - `render::Live2DRenderer::prepare_frame_for_view()`
  - `render::Live2DRenderer::prepare_frame_with_projection()`
  - `render::Live2DRenderer::prepare_frame_for_target()`
- Execute rendering:
  - `render::Live2DRenderer::execute_prepared_to_target()`
  - `render::Live2DRenderer::execute_prepared_mask_pass()`
  - `render::Live2DRenderer::execute_prepared_model_to_surface()`
  - `render::Live2DRenderer::execute_prepared_model_to_target()`
- High-level render integration:
  - `crate::render::features::live2d::Live2DFeature`
  - `crate::render::features::live2d::DrawLive2D`

## Top-Level Layout
- `mod.rs`: module wiring and curated re-exports.
- `asset/`: loading, metadata parsing, texture upload helpers.
- `model/`: aligned Core model wrapper, layout/projection, parameter/part/drawable access.
- `runtime/`: motion, expression, effects, physics, pose, per-instance update chain.
- `render/`: clipping, prepared frame data, renderer state, prepare and execute entry points.

## Full Chain
1. `Live2DModelResource::load(ctx, model_json_path)`
2. parse `.model3.json`
3. resolve `Moc`, textures, physics, pose, motions, user data, and display info
4. build immutable runtime templates from a temporary `Live2DModel`
5. instantiate a mutable `Live2DUserModel`
6. each frame call `Live2DUserModel::update(dt)`
7. prepare render work through `Live2DRenderer`
8. either execute directly to a surface/target or hand the prepared work to the high-level transparent phase

## Runtime Update Chain
- `Live2DUserModel::update(dt)` currently runs:
  - `model.load_parameters()`
  - motion update
  - `model.save_parameters()`
  - late runtime effects through `Live2DUpdateScheduler`
  - `model.update()`
- This order intentionally mirrors the official Cubism coarse flow:
  - restore baseline parameters
  - let motion overwrite the base layer
  - save the post-motion baseline
  - apply late-runtime effects
  - finalize through Core update

## CPU Model Notes
- `Live2DModel` is the authoritative source for:
  - parameters
  - parts
  - drawables
  - offscreens
  - render order
  - masks
  - blend mode
  - multiply/screen colors
  - drawable ids and hit-test bounds
- `model.update()` is the Core boundary:
  - `csmUpdateModel(self.model)`
  - `csmResetDrawableDynamicFlags(self.model)`
  - `refresh_sorted_drawables()`
- Important caches:
  - `sorted_drawables`
  - `sorted_objects`
- Offscreen bugs usually come from reaching for the first list when the second one is required.

## Render Preparation Chain
- Public prepare entry points:
  - `prepare_frame_for_view()`
  - `prepare_frame_with_projection()`
  - `prepare_frame_for_target()`
- `prepare_frame(...)` always:
  - ensures pipelines
  - ensures mask textures
  - updates clipping matrices when needed
  - clears staged dynamic uniforms
  - chooses direct or offscreen traversal
  - uploads staged uniform buffers
  - returns `PreparedLive2DFrame`

## Mask / Clipping Chain
- `ClippingManager` deduplicates equal mask sets into `ClippingContext`s.
- There are two clipping domains:
  - drawable clipping
  - offscreen clipping
- `prepare_mask_draws_for_requests(...)` expands requested clipping contexts into concrete `PreparedMaskDraw`s.
- Mask work is grouped per prepared pass/segment, not as an immediate one-draw-per-mask renderer.

## Render Execution Chain
- Public execution entry points live in `render/renderer/frame_entrypoints.rs`.
- Surface execution chooses between:
  - simple direct-to-surface root path
  - complex offscreen path using intermediate targets plus final blit
- Target execution renders against the supplied target hierarchy directly.

## High-Level Integration
- High-level integration lives directly in `src/render/features/live2d/`.
- `Live2DFeature` is the registration/runtime bridge used by `RenderRuntime` and is exposed from `sky_engine::render::features::live2d` when `live2d` is enabled.
- `Live2DFeature`:
  - extracts visible `Live2DModelInstance`s from ECS
  - collects/filters views
  - prepares one `PreparedLive2DFrame` per visible instance/view pair
  - submits matching `PhaseItem`s into `TransparentPhase`
- `DrawLive2D` is a crate-internal standalone draw function:
  - it runs from the shared transparent phase
  - it resolves the prepared frame for the current entity
  - it executes Live2D mask/model/offscreen/composite work against the active render target
- Live2D now enters the shared frame pipeline through feature registration + phase items, not through a separate legacy high-level integration object.

## Public Surface Notes
- `sky_engine::render::features::live2d` exposes the app-facing Live2D feature and ECS components when `live2d` is enabled:
  - `Live2DFeature`
  - `Live2DModelInstance`
  - `Live2DAnimator`
  - `Live2DCommand`
  - `Live2DCommands`
- `sky_engine::render::expert::live2d` exposes low-level Live2D types and submodules:
  - `Live2DModelResource`
  - `Live2DUserModel`
  - `Live2DRenderer`
  - `PreparedLive2DFrame`
  - `Live2DModel`
  - `asset`
  - `model`
  - `runtime`
  - `render`

## Texture Conventions
- Live2D textures must stay linear:
  - use `Rgba8Unorm`
  - do not silently switch them to sRGB
- CPU-side premultiplication happens in `asset/helpers.rs`.
- If output looks too dark or fringed, check PMA assumptions before changing texture format.

## Current Known Gaps
- Motion manager behavior is still below full Cubism parity.
- EyeBlink / Look / LipSync / Breath do not yet match every official edge case.
- Renderer parity still needs validation for:
  - inverted masks
  - advanced blend behavior
  - exact offscreen timing vs official implementations
- The aspect/layout/projection flow is still simplified relative to the official sample.

## Editing Rules
- Do not leave empty hooks or no-op placeholders in the middle of the chain.
- When fixing bugs, identify the chain segment first:
  - asset load
  - runtime update
  - clipping
  - offscreen traversal
  - composite/blend
  - high-level feature integration
- Prefer small vertical fixes that keep one chain intact end-to-end.
- Do not add Live2D-specific GPU infrastructure here if it belongs in `src/gpu/`.
- Avoid single-model hacks or Haru-specific special-casing.

## Debugging Checklist
- If the model loads but never animates:
  - check `Live2DUserModel::update(dt)` is being called
  - then check motion/expression assets actually loaded
- If parts overlap incorrectly:
  - check pose loading and pose update
- If physics-driven parts are rigid:
  - check physics loading and evaluation
- If clipping is wrong:
  - check clipping matrix updates
  - then mask draw preparation
  - then segment timing in offscreen traversal
- If nested offscreens look wrong:
  - check `sorted_render_objects()`
  - then offscreen close/finalize timing
- If colors/fringes look wrong:
  - check linear texture upload
  - check PMA assumptions in shaders
  - check blend/composite destination sampling
