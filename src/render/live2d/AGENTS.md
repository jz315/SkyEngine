# AGENTS.md — `src/render/live2d`

## Overview
- This module implements Live2D Cubism model loading and rendering on top of `cubism-sys` (Core only) and `wgpu`.
- The current design is intentionally split into:
  - CPU/runtime layer: `loader.rs`, `model.rs`, `motion.rs`, `runtime.rs`, `physics.rs`, `pose.rs`
  - GPU/render layer: `renderer.rs`, `clipping.rs`
- The main external reference is SakuraEngine's Live2D module at `C:\Coding\SakuraEngine_ref\engine\modules\render\live2d`.
- The target direction is to converge toward SakuraEngine / Cubism Framework behavior, but the implementation here is still partial and Rust-native rather than a direct Framework bridge.

## File Map
- `mod.rs`: module wiring and public re-exports.
- `model.rs`: safe wrapper over Cubism Core model memory and drawable access.
- `loader.rs`: synchronous `.model3.json` loader; resolves moc, textures, motion, physics, pose, and runtime helpers.
- `expression.rs`: minimal `exp3.json` loader and single-active-expression player.
- `motion.rs`: current `motion3.json` support. Right now it only loads the `Idle` group and loops one selected idle motion.
- `runtime.rs`: lightweight `EyeBlink` and `Breath` runtimes.
- `physics.rs`: Rust port of the core `physics3.json` evaluation path.
- `pose.rs`: Rust port of the core `pose3.json` logic.
- `clipping.rs`: mask grouping / clipping context preparation.
- `renderer.rs`: `wgpu` draw path for mask pass and model pass.

## Current Runtime Chain
- `Live2DModelResource::update()` currently runs:
  - `motion`
  - `eye blink`
  - `expression`
  - `breath`
  - `physics`
  - `pose`
  - `model.update()`
- This order intentionally follows SakuraEngine's `csmUserModel::update()` at a coarse level.
- The runtime is not complete yet. Missing pieces still include:
  - full motion manager behavior across groups/priorities/fade handling
  - drag / eye-ball / body-angle interaction layer
  - lip sync
  - hit area utilities

## Rendering Model
- Live2D rendering is not sprite-batch rendering. Each drawable has its own:
  - vertex data
  - index data
  - uniform data
  - texture selection
  - blend mode
  - optional mask context
- The renderer depends on engine-level GPU infrastructure, not module-local upload hacks:
  - frame-local buffer sub-allocation in `GpuContext`
  - dynamic uniform offsets
  - explicit `GpuFrame` / pass recorder support
- Mask rendering and model rendering should stay grouped into as few passes as practical. Do not regress back to “one drawable, one render pass”.

## Model / JSON Conventions
- `.model3.json` paths are read from `FileReferences`, not from root-level fallback keys.
- Live2D textures are loaded as linear `Rgba8Unorm`, not `Rgba8UnormSrgb`.
- `motion3.json`, `physics3.json`, and `pose3.json` should be treated as model-relative assets resolved from the `.model3.json` directory.
- When a model logs `motion no`, `physics no`, or `pose no`, check JSON path traversal before assuming runtime logic is broken.

## Safe Editing Rules
- Prefer small vertical changes. Fix one subsystem at a time and re-run the demo or targeted tests before moving on.
- Do not land broad Live2D rewrites in one patch unless explicitly requested.
- Keep runtime behavior aligned with SakuraEngine / Cubism Framework when in doubt. If this module and SakuraEngine disagree, assume this module is more likely wrong.
- Do not introduce Live2D-specific GPU infrastructure into this folder if the same concept belongs in `src/gpu/`.
- Avoid adding model-specific hacks for Haru or any one sample; implementations should stay data-driven from JSON/Core state.

## `cubism-sys` Boundary
- This repo currently binds Cubism Core, not the full Cubism Framework.
- That means:
  - drawable data and parameter/part buffers come from Core
  - higher-level runtime systems (`pose`, `physics`, `expression`, motion management) must be implemented in Rust or bridged explicitly
- Before adding a C++ bridge, verify whether the needed behavior can be ported cleanly from:
  - `C:\Coding\SakuraEngine_ref\engine\modules\render\live2d\CubismFramework`
  - `C:\Coding\CubismSdkForNative\CubismSdkForNative-5-r.5\Framework\src`

## Known Gaps
- `motion.rs` is still minimal relative to Cubism/SakuraEngine:
  - only `Idle` motions are loaded
  - no full priority manager
  - no expression blend integration
  - no effect-ID aware blink/lipsync suppression logic
- `renderer.rs` is functional but not fully feature-complete relative to SakuraEngine:
  - verify `inverted mask` handling before claiming parity
  - verify alpha/fringe behavior before claiming visual parity
- The current aspect/projection path is simplified. If proportions drift, compare against SakuraEngine's layout/view/model matrix flow before changing shaders.

## Testing Commands
- Compile the Live2D demo:
  - ``$env:LIVE2D_CUBISM_SDK_NATIVE_DIR='C:\Coding\CubismSdkForNative\CubismSdkForNative-5-r.5'; cargo check --example live2d_demo --features live2d``
- Run a focused runtime test:
  - ``$env:LIVE2D_CUBISM_SDK_NATIVE_DIR='C:\Coding\CubismSdkForNative\CubismSdkForNative-5-r.5'; cargo test --features live2d render::live2d::motion::tests -- --nocapture``
  - ``$env:LIVE2D_CUBISM_SDK_NATIVE_DIR='C:\Coding\CubismSdkForNative\CubismSdkForNative-5-r.5'; cargo test --features live2d render::live2d::physics::tests -- --nocapture``
- Run the sample model:
  - ``$env:LIVE2D_CUBISM_SDK_NATIVE_DIR='C:\Coding\CubismSdkForNative\CubismSdkForNative-5-r.5'; cargo run --example live2d_demo --features live2d --release -- "C:\Coding\CubismSdkForNative\CubismSdkForNative-5-r.5\Samples\Resources\Haru\Haru.model3.json"``

## Debugging Checklist
- If geometry is corrupted:
  - check frame upload offsets and buffer slicing first
  - then check projection/mask matrices
- If the model is static:
  - check whether `motion` loaded
  - then check whether runtime `update(dt)` is being called
- If arms/parts overlap incorrectly:
  - check `pose3` loading and part opacity updates
- If hair/scarf are rigid:
  - check `physics3` loading and evaluation
- If colors or edges look wrong:
  - check linear vs sRGB texture upload
  - then check premultiplied/straight alpha assumptions in the shader and blend state

## Preferred Reference Path
- For runtime behavior, prefer SakuraEngine's `src/l2d_model_resource.cpp`.
- For GPU rendering structure, prefer SakuraEngine's `src/live2d_render_effects.cpp`.
- For exact Cubism math/semantics, prefer the official SDK files under `CubismSdkForNative/.../Framework/src`.
