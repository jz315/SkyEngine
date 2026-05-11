# Shadow CSM / PCSS Debug Notes

Date: 2026-05-11
Repo: `c:\Coding\SkyEngine`

## User-Reported Symptom

- In `examples/render/three_d_demo.rs`, roof shadows have a hard abrupt transition.
- The user suspects CSM/cascade shadow mapping.
- User screenshots:
  - Lit view: a horizontal/near-horizontal brightness jump across the roof.
  - `pcss blocker` debug view: the same region corresponds to a large PCSS blocker-debug discontinuity.

## Repro Commands

Use the built-in screenshot path in the 3D demo:

```powershell
$env:SKY_DEMO_SCREENSHOT_PATH='C:\Coding\SkyEngine\.tmp\shadow-roof-lit-before.png'
$env:SKY_DEMO_SCREENSHOT_FRAME='45'
$env:SKY_DEMO_EXIT_AFTER_SCREENSHOT='1'
$env:SKY_DEMO_LOCK_REPRO_CAMERA='1'
$env:SKY_DEMO_CAMERA_YAW='0.0'
$env:SKY_DEMO_CAMERA_PITCH='-0.82'
$env:SKY_DEMO_CAMERA_DISTANCE='13.0'
$env:SKY_DEMO_SHADOW_DEBUG='lit'
cargo run --example three_d_demo --features app
```

```powershell
$env:SKY_DEMO_SCREENSHOT_PATH='C:\Coding\SkyEngine\.tmp\shadow-roof-pcss-before.png'
$env:SKY_DEMO_SCREENSHOT_FRAME='45'
$env:SKY_DEMO_EXIT_AFTER_SCREENSHOT='1'
$env:SKY_DEMO_LOCK_REPRO_CAMERA='1'
$env:SKY_DEMO_CAMERA_YAW='0.0'
$env:SKY_DEMO_CAMERA_PITCH='-0.82'
$env:SKY_DEMO_CAMERA_DISTANCE='13.0'
$env:SKY_DEMO_SHADOW_DEBUG='pcss'
cargo run --example three_d_demo --features app
```

Observed local screenshot outputs:

- `.tmp/shadow-roof-lit-before.png`
- `.tmp/shadow-roof-pcss-before.png`
- `.tmp/shadow-lit-before.png`
- `.tmp/shadow-pcss-before.png`

The roof repro images match the user's issue closely.

## Important Warning From Verification

Do not run multiple 3D demo screenshot captures in parallel. I tried split/fade/coverage/log captures at once and Vulkan/wgpu hit an out-of-memory panic plus a swapchain teardown panic. Run these screenshot commands serially.

The one serial debug-log capture worked:

```powershell
$env:SKY_RENDER_DEBUG_LOG='1'
$env:SKY_DEMO_SCREENSHOT_PATH='C:\Coding\SkyEngine\.tmp\shadow-roof-log-before.png'
$env:SKY_DEMO_SCREENSHOT_FRAME='3'
$env:SKY_DEMO_EXIT_AFTER_SCREENSHOT='1'
$env:SKY_DEMO_LOCK_REPRO_CAMERA='1'
$env:SKY_DEMO_CAMERA_YAW='0.0'
$env:SKY_DEMO_CAMERA_PITCH='-0.82'
$env:SKY_DEMO_CAMERA_DISTANCE='13.0'
$env:SKY_DEMO_SHADOW_DEBUG='pcss'
cargo run --example three_d_demo --features app
```

Relevant log values from that run:

```text
cascade 0 split=(0.000->5.500)  extent=(12.867,12.867) texel_world=(0.00628,0.00628) receiver_depth_extent=25.734 caster_depth_extent=40.000 casters=15
cascade 1 split=(5.500->13.000) extent=(28.606,28.606) texel_world=(0.01397,0.01397) receiver_depth_extent=57.212 caster_depth_extent=57.212 casters=21
cascade 2 split=(13.000->30.000) extent=(65.934,65.934) texel_world=(0.03219,0.03219) receiver_depth_extent=131.869 caster_depth_extent=131.869 casters=21
cascade 3 split=(30.000->80.000) extent=(177.085,177.085) texel_world=(0.08647,0.08647) receiver_depth_extent=354.170 caster_depth_extent=354.170 casters=21
```

## Current Findings

- The issue is in the 3D directional shadow path, not in the old 2D light pass.
- Relevant files:
  - `src/render/shaders/materials/standard_material.wgsl`
  - `src/render/shaders/materials/standard_material_normal_mapped.wgsl`
  - `src/render/lighting/shadow/view.rs`
  - `src/render/lighting/shadow/phase.rs`
  - `src/render/lighting/shadow/formula.rs`
  - `examples/render/three_d_demo.rs`
- The demo directional light is created around `examples/render/three_d_demo.rs:885`:
  - 4 cascades
  - splits `[5.5, 13.0, 30.0, 80.0]`
  - `cascade_blend(0.18)`
  - `shadow_resolution_per_cascade(2048)`
  - `radius(0.055)`
  - `shadow_bias(0.0008)`
  - `shadow_depth_bias(3)`
  - `shadow_slope_bias(1.8)`
  - `shadow_normal_bias(0.002)`
  - `.pcss_shadows()`
- The roof/ceiling is a large cube spawned around `examples/render/three_d_demo.rs:712`.
- In lit roof view, the lower/near part of the roof is darker than the upper/far part, with a hard horizontal transition.
- In `pcss blocker` debug, that lower/near part is red/pink, meaning `shadow_find_blocker()` is reporting blockers across the receiver surface. The upper/far part is mostly black, meaning no blockers. The lit discontinuity tracks this PCSS blocker discontinuity.
- This looks like cascade-specific self-blocker/acne in PCSS, not just missing cascade fade. The blocker search reports a broad receiver-surface blocker field in one cascade and not the next.

## WickedEngine Reference Research

Local reference path: `_refs/WickedEngine`.

Relevant WickedEngine files:

- `_refs/WickedEngine/WickedEngine/shaders/shadowHF.hlsli`
- `_refs/WickedEngine/WickedEngine/shaders/lightingHF.hlsli`
- `_refs/WickedEngine/WickedEngine/wiRenderer.cpp`

Source anchors checked locally:

- `shadowHF.hlsli:154` `sample_shadow()`
- `shadowHF.hlsli:164` optional `SHADOW_SAMPLING_PCSS`
- `shadowHF.hlsli:233` `shadow_border_clamp()`
- `shadowHF.hlsli:241` `shadow_2D()`
- `lightingHF.hlsli:86` directional cascade loop
- `lightingHF.hlsli:100` cascade edge fade
- `lightingHF.hlsli:112` current-cascade sample
- `lightingHF.hlsli:122` fallback-cascade sample
- `wiRenderer.cpp:2268` shadow raster bias setup
- `wiRenderer.cpp:2941` camera jitter removal
- `wiRenderer.cpp:2948` origin light-view matrix
- `wiRenderer.cpp:3018` cascade texel snapping
- `wiRenderer.cpp:3029` tight sampling Z range
- `wiRenderer.cpp:3046` wider culling Z range
- `wiRenderer.cpp:6808` per-cascade caster mask

### What Wicked Does In The Shadow Filter

- `shadowHF.hlsli` has an optional `SHADOW_SAMPLING_PCSS` path, but it is a compile-time mode, not the whole shadow system's foundation.
- Its soft-shadow spread is atlas-texel based:
  - `spread = shadow_atlas_resolution_rcp * (radius * 8 + 2)`
  - this matches the existing Sky remap constants.
- Its PCSS blocker search is intentionally simple:
  - 5x5 grid offsets from `-2..2`;
  - each grid position gathers four depth samples;
  - blocker search radius uses `grid_offset * 4 * spread`;
  - blocker depth is counted when the stored depth is closer than the receiver depth.
- Wicked does not add a separate blocker-only epsilon in this shader. The comparison sign is opposite from Sky's current shader because Wicked uses reversed-Z shadow depth and a GREATER_EQUAL comparison sampler.
- If blockers exist, Wicked computes:
  - average blocker depth;
  - `penumbra = abs(receiver - blocker_average) * 200`;
  - final spread clamped between 2 atlas texels and 4x the base spread.
- All PCF taps use the original receiver compare depth. Wicked does not do receiver-plane depth-gradient correction per tap in this path.
- Atlas safety is handled with `shadow_border_clamp()`, which clamps samples to the last texel center. In the disk-sampling path it uses `0.75 * shadow_atlas_resolution_rcp` as the border padding.

### What Wicked Does At Cascade Selection

- `lightingHF.hlsli` loops cascades from nearest to farthest.
- It projects the receiver into each cascade and picks the first cascade whose projected UV/depth is saturated/in bounds.
- Cascade edge fade is based on the projected shadow box, not camera split distance:
  - build `shadow_box = (shadow_pos.xy, shadow_pos.z * 2 - 1)`;
  - compute fade when `abs(shadow_box)` passes `0.8`;
  - fade reaches full strength at the shadow box edge.
- Without cascade dithering, Wicked samples the current cascade and the next cascade, then lerps between them by that edge fade.
- With cascade dithering enabled, it stochastically skips to the next cascade near the edge.
- The implication is important: Wicked's default blend is primarily an atlas/projection-edge blend. It does not appear to blend across the camera split plane the way Sky currently does with split-depth overlap.

### What Wicked Does In CSM Camera Construction

`CreateDirLightShadowCams()` in `wiRenderer.cpp` is the key reference.

- It removes camera jitter before computing shadow cascades.
- It builds a light view matrix at the origin and keeps the eye translation out of the view matrix. The comment says this is important because texel snapping must happen on the projection matrix.
- It unprojects the main camera frustum corners, then lerps near/far corners by cascade split fractions.
- It transforms split corners into light-view space and fits a bounding sphere around those corners.
- It fits an AABB around that sphere, then snaps both min and max to the shadow texel grid.
- It uses two different Z ranges:
  - tight sampling projection Z for precision, expanded to 4x the receiver half-depth;
  - wider culling frustum Z so off-slice casters between the light and receiver survive culling.
- For culling, it expands Z by at least `min(2000, camera_far) * 0.5`.
- Shadow rasterizer state disables depth clipping, so far casters accepted by the culling frustum can still contribute even when outside the tight sampling projection.
- Shadow raster bias is negative in Wicked's reversed-Z convention:
  - UNORM shadow depth: depth bias `-1`, slope bias `-4.0`;
  - float shadow depth: depth bias `-10`, slope bias `-3.4`.

### How Wicked Renders Directional Cascades

- Directional cascades are rendered as multiple viewports into one horizontal atlas strip.
- Each shadow caster is assigned a cascade bitmask by testing its AABB against each widened shadow frustum.
- One draw queue can render into multiple cascades using that camera mask.
- Objects can skip low-detail/near cascades via `object.cascadeMask`, but ordinary casters are otherwise included wherever the widened cascade frustum intersects them.

### Conclusion For The SkyEngine Roof Issue

Wicked is not solving this exact class of artifact by adding a blocker-search epsilon. Its stronger protection comes from the whole contract:

- reversed-Z shadow maps plus matching GREATER_EQUAL compare;
- raster depth/slope bias in the shadow pass;
- stable sphere-fitted, texel-snapped cascades;
- separate tight receiver projection and wide caster culling projection;
- depth clipping disabled for shadow rasterization;
- cascade blending only at projection-box edges;
- PCSS kept as a thin optional layer on top of a stable CSM baseline.

That changes the likely SkyEngine fix direction:

- Do not treat PCSS blocker epsilon as the primary fix. It can be a guardrail, but Wicked does not need it for the core path.
- First verify Sky's fixed PCF / dithered PCF roof result. If fixed PCF has no band but PCSS does, calibrate PCSS. If fixed PCF also has cascade inconsistency, fix CSM/bias/caster range first.
- Audit Sky's cascade fit against Wicked:
  - jitter removal;
  - light view at origin;
  - sphere fit;
  - texel snapping coverage;
  - tight receiver Z range;
  - wider caster culling Z range;
  - shadow pass depth clipping behavior.
- Audit compare direction and bias units. Sky uses normal-Z depth with LessEqual compare, so Wicked's negative raster bias and `depth > receiver` blocker condition must be translated carefully, not copied literally.
- Consider simplifying PCSS temporarily to a Wicked-like mode for diagnosis:
  - no receiver-plane gradient in blocker search;
  - count blockers only against the center receiver compare depth;
  - penumbra from average receiver-blocker depth gap;
  - clamp spread to `[2 texels, base_spread * 4]`.
- After the baseline matches Wicked behavior, decide whether Sky's receiver-plane gradient and blocker epsilon are worth keeping as an enhancement.

## Most Suspicious Shader Path

In both material shaders:

- `shadow_transmittance()`
- `shadow_sample_projected_cascade()`
- `shadow_filter_pcss()`
- `shadow_find_blocker()`
- `shadow_pcss_debug_color()`

Current blocker test:

```wgsl
let sample_compare_depth = shadow_receiver_plane_compare_depth(uv, sample_uv, compare_depth, depth_gradient);
if (sample_depth < sample_compare_depth) {
    average_receiver_gap += sample_compare_depth - sample_depth;
    blocker_count += 1.0;
}
```

Hypothesis: PCSS blocker search needs a stricter blocker threshold than the final PCF compare, probably at least one cascade-scaled texel/depth epsilon. Right now tiny depth disagreement/self-depth from the receiver or its own box faces can count as blockers, and because texel/depth ranges differ per cascade, this can flip abruptly at the CSM split.

The fix should likely be applied to both:

- `standard_material.wgsl`
- `standard_material_normal_mapped.wgsl`

And mirrored in CPU formula tests:

- `src/render/lighting/shadow/formula.rs`

## Possible Fix Direction

Introduce a PCSS blocker-only rejection bias, separate from the final visibility compare:

- Compute a cascade-scaled depth epsilon from:
  - `shadow.cascade_params[cascade].y` world units per texel
  - `shadow.cascade_params[cascade].w` light-space depth range
  - maybe the existing receiver/compare normal bias
- Require `sample_depth + blocker_epsilon < sample_compare_depth` before counting a blocker.
- Accumulate `sample_compare_depth - sample_depth - blocker_epsilon` or keep raw gap after threshold; the former is safer for penumbra.

Important: keep real occluders like the small cube on the roof visible in PCSS debug. The rejection should remove receiver-surface false blockers, not erase actual caster shadows.

## Tests / Checks To Run

Run focused Rust tests first:

```powershell
cargo test --features app render::lighting::shadow::formula
cargo test --features app render::runtime::tests::shadows
```

Then run the roof screenshot repro serially:

```powershell
$env:SKY_DEMO_SCREENSHOT_PATH='C:\Coding\SkyEngine\.tmp\shadow-roof-lit-after.png'
$env:SKY_DEMO_SCREENSHOT_FRAME='45'
$env:SKY_DEMO_EXIT_AFTER_SCREENSHOT='1'
$env:SKY_DEMO_LOCK_REPRO_CAMERA='1'
$env:SKY_DEMO_CAMERA_YAW='0.0'
$env:SKY_DEMO_CAMERA_PITCH='-0.82'
$env:SKY_DEMO_CAMERA_DISTANCE='13.0'
$env:SKY_DEMO_SHADOW_DEBUG='lit'
cargo run --example three_d_demo --features app
```

```powershell
$env:SKY_DEMO_SCREENSHOT_PATH='C:\Coding\SkyEngine\.tmp\shadow-roof-pcss-after.png'
$env:SKY_DEMO_SCREENSHOT_FRAME='45'
$env:SKY_DEMO_EXIT_AFTER_SCREENSHOT='1'
$env:SKY_DEMO_LOCK_REPRO_CAMERA='1'
$env:SKY_DEMO_CAMERA_YAW='0.0'
$env:SKY_DEMO_CAMERA_PITCH='-0.82'
$env:SKY_DEMO_CAMERA_DISTANCE='13.0'
$env:SKY_DEMO_SHADOW_DEBUG='pcss'
cargo run --example three_d_demo --features app
```

Visual target:

- Lit roof no longer has a hard CSM band.
- PCSS debug no longer paints an entire near cascade as blockers.
- Real roof cube shadow remains visible.

## Worktree Note

The worktree was already dirty before this debug task. Do not revert unrelated files.
