# AGENTS.md — `src/render/live2d`

## Overview
- This module implements Live2D Cubism loading, runtime update, clipping, and rendering on top of `cubism-sys` and `wgpu`.
- The implementation is Rust-native and binds Cubism Core directly, not the full official Framework.
- The current architecture is intentionally split into four real sub-domains:
  - `asset/` — immutable loaded assets and model-relative metadata
  - `model/` — safe Cubism Core model wrapper and CPU-side render/state access
  - `runtime/` — mutable per-instance controllers and update chain
  - `render/` — clipping, prepared frames, renderer execution, and graph integration
- Treat this folder as one pipeline, not a bag of helpers:
  - load `.model3.json` and assets
  - instantiate mutable runtime state
  - update model state every frame
  - prepare per-frame GPU work
  - execute mask/model/offscreen/composite passes
  - optionally attach prepared output into `PreparedFrame`

## Top-Level Layout
- `mod.rs`: module wiring and curated re-exports for common Live2D types.
- `asset/`: `Live2DModelResource`, file loading, texture upload helpers, static metadata parsing.
- `model/`: `Live2DModel`, layout/projection logic, parameter APIs, drawable/offscreen accessors, tests.
- `runtime/`: `Live2DUserModel`, update scheduler, effects, expression, motion, physics, pose.
- `render/`: `ClippingManager`, `PreparedLive2DFrame*`, `Live2DRenderer`, `Live2DOverlayNode`.

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
  - `render::Live2DRenderer::prepare_frame_for_surface()`
  - `render::Live2DRenderer::prepare_frame_for_view()`
  - `render::Live2DRenderer::prepare_frame_for_target()`
- Execute rendering:
  - `render::Live2DRenderer::execute_prepared_to_surface()`
  - `render::Live2DRenderer::execute_prepared_to_target()`
  - `render::Live2DRenderer::execute_prepared_model_to_target()`
- Render graph integration:
  - `render::Live2DOverlayNode`

## Domain Map

### `asset/`
- `core.rs`:
  - owns `Live2DModelResource`
  - parses `.model3.json`
  - resolves `Moc`, `Textures`, `Physics`, `Pose`, `Motions`, `UserData`, `DisplayInfo`
  - builds immutable runtime templates from a temporary `Live2DModel`
- `helpers.rs`:
  - texture load/upload helpers
  - `Layout`, hit-area, user-data, and display-info parsing
  - CPU-side RGBA premultiplication for Live2D textures
- `runtime_api.rs`:
  - `instantiate()`
  - texture and metadata accessors
- `tests.rs`:
  - parser/helper tests close to the asset code

### `model/`
- `mod.rs`:
  - owns aligned moc/model allocations
  - revives the Cubism Core model
  - reads static parent/offscreen relations
  - initializes saved parameter state and sorted render-object caches
- `layout.rs`:
  - `.model3.json` layout application
  - aspect/projection fitting
  - `render_matrix*()` APIs
- `parameters.rs`:
  - parameter and part APIs
  - saved baseline load/save
  - virtual parameter support used by part-driven logic
- `drawables.rs`:
  - drawable/offscreen accessors
  - hit-test bounds
  - sorted drawable/render-object caches
- `tests.rs`:
  - layout/projection-focused tests

### `runtime/`
- `effects.rs`:
  - EyeBlink, Look, LipSync, Breath
- `expression.rs`:
  - `exp3.json` parsing and expression playback
- `motion/`:
  - `types.rs`: motion data structures and scheduler state
  - `parsing.rs`: segment/curve parsing and evaluation helpers
  - `player.rs`: `Live2DMotionPlayer`
  - `tests.rs`
- `physics/`:
  - `types.rs`: physics data structures
  - `math.rs`: parse/evaluate helpers
  - `runtime.rs`: `Live2DPhysics`
  - `tests.rs`
- `pose.rs`:
  - `pose3.json` runtime
- `update.rs`:
  - fixed update stage ordering used by `Live2DUserModel::update()`
- `user_model/`:
  - `mod.rs`: `Live2DUserModel` state
  - `build.rs`: instance construction from `Live2DModelResource`
  - `update_api.rs`: per-frame runtime chain
  - `control.rs`: motion/expression control APIs
  - `interaction.rs`: drag, lip-sync, hit-testing, tap behavior

### `render/`
- `clipping.rs`:
  - drawable/offscreen clipping context building
  - per-frame clipping matrix updates
- `prepared.rs`:
  - `PreparedLive2DFrame`
  - `PreparedLive2DFrameSet`
  - prepared pass/draw structs
- `renderer/`:
  - `state.rs`: renderer-owned GPU state and cached resources
  - `init.rs`: constructor wiring for `Live2DRenderer`
  - `prepare.rs`: frame preparation and offscreen traversal
  - `composite.rs`: composite draw prep, bind groups, pipeline setup helpers
  - `execute.rs`: prepared pass execution and root/backdrop copies
  - `frame_entrypoints.rs`: public prepare/execute entry points
- `feature.rs`:
  - graph-backed overlay node for `FramePipeline` view phase

## Full Chain
1. `Live2DModelResource::load(ctx, model_json_path)`
2. parse `.model3.json`
3. read `FileReferences.Moc`
4. build a temporary `Live2DModel::from_moc3_bytes()` for template resolution
5. optional `Layout` parse and model transform construction
6. build runtime templates:
   - `Live2DMotionPlayer::from_model_json(...)`
   - `Live2DEyeBlink::from_model_json(...)`
   - `Live2DExpressionPlayer::from_model_json(...)`
   - `Live2DLook::from_model(...)`
   - `Live2DBreath::from_model(...)`
   - `Live2DPhysics::from_json_str(...)`
   - `Live2DLipSync::from_model_json(...)`
   - `Live2DPose::from_json_str(...)`
   - hit-area / user-data / display-info parsing
7. load textures as linear `Rgba8Unorm` and premultiply CPU-side RGBA bytes
8. instantiate `Live2DUserModel` from the immutable resource
9. each frame call `Live2DUserModel::update(dt)`
10. prepare render work through `Live2DRenderer`
11. execute prepared passes directly or through `PreparedLive2DFrameSet + Live2DOverlayNode`

## Runtime Update Chain
- `Live2DUserModel::update(dt)` currently runs:
  - `model.load_parameters()`
  - `motion_player.update(...)`
  - `model.save_parameters()`
  - `Live2DUpdateScheduler::run(...)`, which applies:
    - EyeBlink when motion did not already drive it
    - Expression
    - Look
    - Breath
    - Physics
    - LipSync
    - Pose
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
  - `sorted_drawables`: drawables only
  - `sorted_objects`: drawables + offscreens
- Offscreen bugs usually come from reaching for the first list when the second one is required.

## Render Preparation Chain

### Public prepare entry points
- `prepare_frame_for_surface()`
- `prepare_frame_for_view()`
- `prepare_frame_for_target()`

### `prepare_frame(...)`
- Always does:
  - `ensure_pipelines(...)`
  - `ensure_mask_texture(...)`
  - `clip_mgr.update_matrices(model)` when drawable clipping exists
  - clear staged dynamic uniform buffers
- Then chooses one of two branches:
  - no offscreens: simple root pass path
  - offscreens present: `prepare_frame_with_offscreens(...)`
- At the end:
  - upload staged uniform buffers
  - return `PreparedLive2DFrame`

### `prepare_frame_with_offscreens(...)`
- This is the critical mixed render-order walk.
- It:
  - ensures reusable root/blend/offscreen targets
  - builds or refreshes cached offscreen clipping
  - iterates `model.sorted_render_objects()`
  - collects drawables into the current segment
  - closes segments when offscreen boundaries change
  - emits composite draws back into parent targets
- Segment construction still revolves around:
  - `SegmentBuilder`
  - `close_completed_offscreens(...)`
  - `object_is_inside_offscreen(...)`
  - `finalize_segment(...)`
  - `prepare_composite_draw(...)`

## Mask / Clipping Chain
- `ClippingManager` deduplicates equal mask sets into `ClippingContext`s.
- There are two clipping domains:
  - drawable clipping
  - offscreen clipping
- `ClippingManager::update_matrices(model)` computes bounds from clipped targets, not from mask drawables.
- `prepare_mask_draws_for_requests(...)` expands requested clipping contexts into concrete `PreparedMaskDraw`s.
- Our current semantics group mask work per prepared pass/segment; it is not a one-draw-call-per-mask immediate renderer.

## Render Execution Chain
- Public execution entry points live in `render/renderer/frame_entrypoints.rs`.
- Surface execution chooses between:
  - simple direct-to-surface root path
  - complex offscreen path using `root_intermediate_target` plus final blit
- Target execution always renders against the supplied target hierarchy directly.
- Core helpers still include:
  - `render_mask_pass(...)`
  - `render_target_pass(...)`
  - `render_root_pass_to_surface(...)`
  - backdrop/backup copies for advanced blend/composite cases

## Graph Integration
- `PreparedLive2DFrameSet` is the multi-view sparse container for prepared frames.
- `Live2DOverlayNode`:
  - is a `FrameViewNode`
  - reads the `"current_color"` slot from the generic `PhaseState`
  - allocates `live2d_overlay_out`
  - looks up `PreparedLive2DFrameSet` from `PreparedFrame`
  - executes all prepared Live2D frames for the active view
- Live2D integration happens at the prepared-frame boundary.
- Do not force Live2D data into sprite-batch scene structures just to reuse unrelated code.

## Public Surface Notes
- `crate::render::live2d` keeps the high-frequency re-exports:
  - `Live2DModelResource`
  - `Live2DUserModel`
  - `Live2DRenderer`
  - `PreparedLive2DFrame`
  - `PreparedLive2DFrameSet`
  - `Live2DOverlayNode`
  - `Live2DModel`
- `crate::render::expert::live2d` now exposes submodules by responsibility:
  - `asset`
  - `model`
  - `runtime`
  - `render`

## Texture Conventions
- Live2D textures must stay linear:
  - use `Rgba8Unorm`
  - do not silently switch them to sRGB
- CPU-side premultiply happens in `asset/helpers.rs` via `premultiply_rgba8(...)`.
- If output looks too dark or fringed, check PMA assumptions before changing texture format.

## Current Known Gaps
- Motion manager behavior is still below full Cubism parity.
- EyeBlink / Look / LipSync / Breath are wired, but do not yet match every official edge case.
- Renderer parity still needs validation for:
  - inverted masks
  - advanced blend behavior
  - exact offscreen timing vs official Vulkan/Framework behavior
- The aspect/layout/projection flow is simplified relative to the official sample.
- `.userdata3.json` and `.cdi3.json` are parsed and exposed, but not yet consumed by richer higher-level tooling.
- Motion `Sound` metadata is surfaced as file paths/events only; actual audio playback remains the app layer's job.

## Editing Rules
- Do not leave empty hooks or no-op placeholders in the middle of the chain.
- When fixing bugs, identify the chain segment first:
  - asset load
  - runtime update
  - clipping
  - offscreen traversal
  - composite/blend
  - graph integration
- Prefer small vertical fixes that keep one chain intact end-to-end.
- Do not add Live2D-specific GPU infrastructure here if the abstraction belongs in `src/gpu/`.
- Avoid single-model hacks or Haru-specific special-casing.

## Debugging Checklist
- If the model loads but never animates:
  - check `Live2DUserModel::update(dt)` is being called
  - then check motion/expression assets actually loaded
- If parts overlap incorrectly:
  - check `pose3.json` loading and `pose.update_parameters(...)`
- If physics-driven parts are rigid:
  - check `physics3.json` loading and `physics.evaluate(...)`
- If clipping is wrong:
  - check `ClippingManager::update_matrices(...)`
  - then `prepare_mask_draws_for_requests(...)`
  - then segment timing in `prepare_frame_with_offscreens(...)`
- If nested offscreens look wrong:
  - check `sorted_render_objects()`
  - then `close_completed_offscreens(...)`
  - then `prepare_composite_draw(...)`
- If colors/fringes look wrong:
  - check linear texture upload
  - check PMA assumptions in `live2d.wgsl` and `live2d_offscreen.wgsl`
  - check blend/composite destination sampling

## Preferred Reference Path
- Runtime behavior:
  - official `CubismSdkForNative/.../Framework/src/Motion`
  - SakuraEngine Live2D runtime path
- Render structure:
  - official `CubismSdkForNative/.../Framework/src/Rendering/Vulkan/CubismRenderer_Vulkan.cpp`
  - official sample analysis in `CubismSdkForNative/.../Samples/Vulkan/Demo/proj.win.cmake/src/AGENTS.md`

## One-Line Summary
- The canonical mental model for this folder is:
  - `asset` builds an immutable `Live2DModelResource`
  - `instantiate` creates a mutable `Live2DUserModel`
  - `runtime` mutates Core model state in the correct order
  - `render` turns sorted drawable/offscreen state into `PreparedLive2DFrame`
  - `execute` consumes that prepared frame into a surface/target/graph view
- Any fix that ignores one of those stages is likely incomplete.
