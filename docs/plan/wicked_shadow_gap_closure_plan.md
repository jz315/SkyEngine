# Wicked Shadow Gap Closure Plan

## Purpose

Close the remaining gap between SkyEngine's current directional shadow path and a WickedEngine-style production shadow system.

This plan is intentionally about correctness and stability before beauty. SkyEngine already has several Wicked-shaped pieces: CSM, an atlas, Vogel PCF, dithered PCF, PCSS, transparent shadow transmittance, cascade debug views, and screen-space contact shadows. The visible problem is now more likely in the system-level contract around cascade fitting, caster/receiver bounds, bias units, depth precision, update stability, and validation.

Do not wholesale copy WickedEngine or Photon. Use WickedEngine as the primary reference for renderer architecture and shadow math, then translate the ideas into SkyEngine's Rust + wgpu + WGSL pipeline.

Primary references:

- WickedEngine `wiRenderer.cpp`
  - `CreateDirLightShadowCams`
  - shadow atlas packing
  - cascade/caster culling
  - shadow rasterizer/depth bias setup
- WickedEngine `shadowHF.hlsli`
  - `shadow_2D`
  - `shadow_border_clamp`
  - Vogel disk filtering
  - optional dithering and PCSS
  - transparent shadow map modulation
- WickedEngine `screenspaceshadowCS.hlsl`
  - screen-space shadow ray marching
  - per-light shadow mask shape
  - depth thickness and edge fading
- SkyEngine current files:
  - `src/render/lighting/shadow/view.rs`
  - `src/render/lighting/shadow/phase.rs`
  - `src/render/lighting/shadow/atlas.rs`
  - `src/render/shaders/materials/standard_material.wgsl`
  - `src/render/shaders/materials/standard_material_normal_mapped.wgsl`
  - `src/render/shaders/postfx/contact_shadows.wgsl`
  - `examples/render/three_d_demo.rs`

## Current Diagnosis

What SkyEngine already has:

- Directional CSM with up to 4 cascades.
- One shadow `SceneView` per active cascade.
- A horizontal directional shadow atlas.
- Atlas scale/add metadata and border clamp behavior.
- Per-light shader compare bias, raster depth bias, slope bias, normal bias, filter radius, sampling mode, and update policy.
- Fixed PCF, dithered PCF, and PCSS sampling modes.
- Transparent shadow atlas and material-aware transparent caster pass.
- Alpha-test shadow caster path.
- Per-material cast and receive shadow flags.
- Per-cascade caster counts and draw-call stats.
- Cascade coverage and raw cascade debug views.
- Contact shadow post-fx using depth and normal inputs.

Remaining likely causes of "shadow still not right":

- Cascade camera fitting is close to Wicked's shape, but the caster/receiver depth model is still approximate.
- Receiver bias and raster bias are not yet tuned from one consistent unit system.
- PCSS softness can hide or amplify CSM errors; it is not a substitute for stable cascade bounds.
- Contact shadows can produce regular bands or over-darkening that looks like a CSM bug.
- Demo defaults may be tuned for a particular camera angle rather than robust across orbit, near/far splits, and grazing surfaces.
- Validation is still mostly unit/compile tests; it lacks repeatable visual probes and readback-based quality checks.

## Non-Goals

- Do not replace SkyEngine with WickedEngine.
- Do not port Wicked's full renderer in one pass.
- Do not return to Photon as the primary shadow reference.
- Do not tune by blindly increasing bias until acne disappears.
- Do not treat contact shadows as a fix for missing or unstable CSM shadows.
- Do not add hardware ray tracing to this phase.
- Do not refactor unrelated GI, TAA, bloom, UI, or backend code while closing this gap.

## Design Principles

- Make shadow defects inspectable before tuning them.
- Fix cascade geometry before filter quality.
- Keep all distances in explicit units:
  - camera split distance in view-space units;
  - atlas filter spread in texels;
  - receiver/raster bias in either world units or depth units, never implicit magic constants.
- Prefer stable conservative fitting over tight-but-shimmering bounds.
- Keep `StandardMaterial` and `StandardMaterialNormalMapped` behavior synchronized.
- Treat normal maps as lighting detail, not as receiver-position bias input.
- Validate with toggles:
  - CSM only;
  - CSM without PCSS;
  - contact shadows off;
  - TAA off;
  - SSGI off.

## Milestone G0: Baseline Capture And Reproduction

Purpose: identify whether the current artifact comes from CSM, PCSS, transparent shadow, contact shadow, TAA, or GI.

Tasks:

- Add a written reproduction note for the current bad shadow in `docs/render_deep_dive.md` or a small companion debug note:
  - camera position/orbit angle;
  - light direction;
  - active debug mode;
  - enabled post-fx;
  - screenshot path if available.
- Run `three_d_demo` under controlled combinations:
  - default;
  - `SKY_DEMO_DISABLE_CONTACT_SHADOWS=1`;
  - `SKY_DEMO_DISABLE_TAA=1`;
  - `SKY_DEMO_DISABLE_SSGI=1`;
  - fixed PCF instead of PCSS;
  - cascade coverage debug;
  - raw cascade 0-3 debug.
- Record which toggle removes or changes the artifact.
- Add a short debug checklist to the demo docs or render deep dive:
  - if artifact moves with camera projection, suspect contact/TAA;
  - if artifact follows cascade color boundary, suspect CSM split/fade;
  - if artifact follows shadow-map texel grid, suspect bias/atlas/filter;
  - if artifact only appears near object contact, suspect contact shadow.

Files:

- `examples/render/three_d_demo.rs`
- `docs/render_deep_dive.md`
- optional `docs/plan/wicked_shadow_debug_notes.md`

Acceptance:

- There is one canonical repro scenario.
- The team knows whether the dominant issue is CSM, contact shadow, or post-process history.
- No code tuning starts before this baseline is captured.

## Milestone G1: Cascade Fit And Stability Audit

Purpose: make the directional shadow camera match Wicked's stability expectations.

Tasks:

- Audit `build_shadow_view()` against Wicked's `CreateDirLightShadowCams`.
- Verify the split frustum corners:
  - near/far split interpolation is correct;
  - split distances are in camera view units;
  - full frustum inverse projection uses the same clip-space depth convention as wgpu/WGSL.
- Verify the light view basis:
  - stable up vector for near-vertical light directions;
  - no sudden basis flip as the light direction changes.
- Revisit sphere fitting:
  - confirm radius is conservative enough for the split;
  - confirm radius does not change due to tiny camera rotations more than necessary.
- Revisit texel snapping:
  - snap cascade center to texel grid, not both bounds in a way that can shrink coverage;
  - preserve coverage after snapping;
  - write tests for "snapping never excludes original split corners".
- Separate matrices explicitly:
  - sampling projection for receiver compare;
  - culling/raster projection for caster inclusion.
- Add debug logging fields if missing:
  - cascade index;
  - split near/far;
  - world extent;
  - texel world size;
  - receiver depth extent;
  - caster depth extent;
  - caster count;
  - update mask.

Files:

- `src/render/lighting/shadow/view.rs`
- `src/render/view/projection.rs`
- `src/render/view/scene_view.rs`
- `examples/render/three_d_demo.rs`

Acceptance:

- Unit tests prove every cascade split corner remains inside the sampling projection after snapping.
- Camera orbiting does not cause obvious cascade scale jitter in coverage debug.
- The raw shadow atlas does not show clipped receivers for the canonical demo scene.

## Milestone G2: Caster And Receiver Depth Contract

Purpose: fix missing or smeared shadows caused by an approximate Z range.

Current concern:

- SkyEngine currently computes a receiver depth range from the cascade sphere, then uses a broader view-far-derived culling range for casters. This is safe-ish, but not a full Wicked-style scene/caster-aware contract.

Tasks:

- Define a clear receiver depth extent:
  - used only for depth comparison and sampling precision;
  - should be tight enough to preserve depth precision;
  - should never clip the receiver cascade.
- Define a clear caster depth extent:
  - used for shadow-view frustum culling;
  - includes casters between the light and receiver slice;
  - does not unnecessarily explode to the camera far plane for all cascades.
- Consider scene-aware caster bounds:
  - use extracted shadow caster bounds if already available;
  - otherwise add a cheap per-cascade caster-bound pass during extraction;
  - keep it optional if it risks broad churn.
- Add a test scene where a caster sits outside the receiver split but between the light and receiver.
- Add a test scene where a far caster should not degrade near cascade precision.
- Confirm shadow depth shader does not bend triangles through vertex-stage depth clamping.

Files:

- `src/render/lighting/shadow/view.rs`
- `src/render/lighting/shadow/phase.rs`
- `src/render/shaders/lighting/shadow_depth.wgsl`
- `src/render/shaders/lighting/shadow_depth_alpha_test.wgsl`
- `src/render/shaders/lighting/shadow_transparent.wgsl`

Acceptance:

- Near cascade keeps high depth precision.
- Off-slice casters that should cast into the receiver slice survive culling.
- Far unrelated casters do not bloat near-cascade receiver precision.
- `cargo test --features app shadow`.

## Milestone G3: Bias Unit Unification

Purpose: stop tuning bias as unrelated magic numbers.

Tasks:

- Document the active bias stack:
  - raster depth bias;
  - raster slope bias;
  - shader compare bias;
  - receiver normal/world bias;
  - PCF/PCSS filter radius.
- Convert bias recommendations to stable units:
  - receiver normal bias in world units;
  - minimum receiver bias derived from cascade texel world size;
  - compare depth bias derived from texel world size divided by cascade depth range;
  - raster bias kept as API-level integer/slope values but documented separately.
- Create a small table of recommended presets:
  - sharp diagnostic shadows;
  - stable fixed PCF;
  - soft PCSS;
  - contact-shadow-friendly.
- Ensure both standard material WGSL files use the same bias helpers.
- Add tests that normal-mapped materials use geometric normals for receiver bias.
- Add tests that cascade selection uses the unbiased receiver position.
- Add tests that compare depth bias scales with texel size and angle.

Files:

- `src/render/component/light.rs`
- `src/render/resources/material.rs`
- `src/render/shaders/materials/standard_material.wgsl`
- `src/render/shaders/materials/standard_material_normal_mapped.wgsl`
- `docs/render_deep_dive.md`

Acceptance:

- Acne, peter-panning, and cascade artifacts can be discussed in terms of one documented bias stack.
- Fixed PCF diagnostic mode can produce stable, explainable shadows before PCSS is enabled.
- `cargo test --features app standard_material`.

## Milestone G4: Filter And PCSS Calibration

Purpose: make Wicked-style PCF/PCSS behave consistently after the CSM basis is correct.

Tasks:

- Keep fixed PCF as the baseline diagnostic mode.
- Verify atlas-space spread:
  - filter radius in world units;
  - converted to cascade texels by `radius / texel_world_size`;
  - converted to atlas UV by atlas reciprocal resolution.
- Verify border clamp:
  - taps never cross into neighboring cascade rects;
  - guard band matches the maximum practical kernel;
  - selected cascade debug view crops the same rect the shader samples.
- Revisit PCSS blocker search:
  - search radius should not exceed guard-band-safe range;
  - blocker depth load uses the same atlas UV convention as compare sampling;
  - penumbra growth should be clamped per cascade.
- Add a test or source assertion that PCSS cannot request a filter kernel larger than the safe atlas border contract.
- Consider temporal dither integration:
  - if TAA is enabled, use frame/temporal rotation rather than static noise;
  - if TAA is disabled, prefer fixed PCF for diagnostics.

Files:

- `src/render/lighting/shadow/atlas.rs`
- `src/render/lighting/shadow/bindings.rs`
- `src/render/shaders/materials/standard_material.wgsl`
- `src/render/shaders/materials/standard_material_normal_mapped.wgsl`
- `src/render/postfx/taa.rs`

Acceptance:

- Fixed PCF, dithered PCF, and PCSS are visibly distinct and predictable.
- PCSS no longer creates grid-like or cascade-edge artifacts in the canonical repro.
- `cargo test --features app shadow`.

## Milestone G5: Contact Shadow Isolation And Rewrite Boundary

Purpose: keep screen-space contact shadows from masking or corrupting CSM diagnosis.

Tasks:

- Treat contact shadow as an optional close-range supplement.
- Add a debug mode or output stat that makes contact shadow visibility inspectable separately from CSM.
- Verify the ray direction convention:
  - view-space light direction;
  - ray marches toward the light;
  - depth comparison matches SkyEngine depth reconstruction.
- Verify thickness units:
  - constant minimum thickness;
  - depth-scaled thickness for distant samples;
  - no broad over-occlusion on flat surfaces.
- Compare current geometric/jittered stepping to Wicked's screen-space shadow shape:
  - range divided by sample count;
  - dithered initial offset;
  - depth delta threshold;
  - screen-edge fade.
- Decide whether to keep the current Photon-inspired geometric stepping or switch to a Wicked-like linear stepping with dither.
- Add a test scene with:
  - object on floor contact;
  - wall-floor corner;
  - thin vertical occluder;
  - grazing sun angle.

Files:

- `src/render/postfx/contact_shadows.rs`
- `src/render/shaders/postfx/contact_shadows.wgsl`
- `src/render/component/settings.rs`
- `examples/render/three_d_demo.rs`

Acceptance:

- With contact shadows disabled, CSM quality is still correct.
- With contact shadows enabled, only near-contact detail changes.
- Contact shadows do not introduce regular staircase/banding in the canonical repro.
- `cargo test --features app contact_shadows`.

## Milestone G6: Demo Defaults And Debug UX

Purpose: make `three_d_demo` a trustworthy shadow validation scene.

Tasks:

- Add visible debug toggles or log output for:
  - current shadow sampling mode;
  - cascade splits;
  - bias values;
  - contact shadow enabled/intensity/range;
  - TAA and SSGI status.
- Add a shadow diagnostic preset in the demo:
  - fixed PCF;
  - lower filter radius;
  - contact shadows disabled;
  - TAA disabled if needed.
- Keep the pretty default separate from the diagnostic default.
- Add a few intentional shadow test objects:
  - thin caster;
  - low block on floor;
  - tall wall caster;
  - sloped or normal-mapped surface;
  - far caster outside cascade 0.
- Avoid tuning only for the initial camera angle.

Files:

- `examples/render/three_d_demo.rs`
- `docs/render_deep_dive.md`

Acceptance:

- A developer can switch from "pretty" to "diagnostic" without editing code.
- The demo exposes whether an artifact comes from CSM, filter, contact shadow, TAA, or GI.
- `cargo check --example three_d_demo --features app`.

## Milestone G7: Automated Visual/Readback Regression

Purpose: prevent future shadow fixes from becoming guesswork again.

Tasks:

- Add a headless or near-headless render test for a simple caster/receiver scene.
- Read back the output or shadow atlas and assert coarse properties:
  - shadow atlas contains non-clear depth;
  - receiver output darkens where expected;
  - receive-shadow disabled material does not darken;
  - cascade 0 has higher effective resolution than far cascades.
- Add a shadow atlas dump helper if current readback tools are too clumsy.
- Store small generated snapshots only if the repo already accepts snapshot artifacts; otherwise keep numeric image assertions.
- Add a smoke command list:
  - `cargo test --features app shadow -- --nocapture`
  - `cargo test --features app standard_material -- --nocapture`
  - `cargo test --features app contact_shadows -- --nocapture`
  - `cargo check --example three_d_demo --features app`
  - `cargo check --examples --features app` after API-level changes.

Files:

- `src/render/runtime/tests.rs`
- `src/render/lighting/shadow/resources.rs`
- `src/render/postfx/contact_shadows.rs`
- `docs/render_deep_dive.md`

Acceptance:

- Shadow regressions fail a test or produce a clear diagnostic screenshot/readback.
- The canonical repro has a before/after validation path.

## Milestone G8: Future Wicked Parity Work

Purpose: define the next horizon after directional shadows are stable.

Tasks:

- Add spot-light shadow rendering into the existing atlas boundary.
- Add point-light six-face shadow support only after spot lights validate the atlas allocator.
- Consider per-light screen-space shadow masks if the renderer moves closer to Wicked's tiled lighting model.
- Consider ray-traced shadow masks only behind a separate backend/feature gate.
- Consider shadow denoising only after the core raster shadow path is stable.

Acceptance:

- Directional shadows are no longer the limiting visual defect in `three_d_demo`.
- Future spot/point/RT work can reuse the same atlas, debug, and resource contracts.

## Recommended Implementation Order

1. G0: capture the repro and isolate the artifact source.
2. G1: audit cascade fitting and snapping.
3. G2: fix caster/receiver depth ranges.
4. G3: unify bias units and presets.
5. G4: recalibrate PCF/PCSS.
6. G5: isolate and tune contact shadows.
7. G6: improve demo debug UX.
8. G7: add readback/visual regression coverage.
9. G8: continue to spot/point/advanced Wicked parity.

## Immediate Next PR Scope

Recommended first PR:

- Add the G0 debug/repro note.
- Add diagnostic toggles or logging in `three_d_demo`.
- Add one or two tests around cascade snapping coverage.
- Do not change PCSS or contact shadow tuning yet.

Acceptance:

- `cargo test --features app shadow -- --nocapture`
- `cargo check --example three_d_demo --features app`
