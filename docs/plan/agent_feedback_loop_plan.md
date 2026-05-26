# Sky Agent Feedback Loop Plan

## Summary

This plan makes SkyEngine an AI-native game engine in a concrete, testable way: an agent should be able to write engine or game code, run the result, observe what happened, use engine debug tools, validate the output, and iterate without depending on a human to inspect every frame manually.

The goal is not to add a chat box to the engine. The goal is to expose a stable agent-facing feedback loop:

```text
edit code -> build -> run scenario -> capture output -> inspect diagnostics -> verify checks -> patch again
```

This is the engine equivalent of what a browser gives an agent through DOM inspection, screenshots, click/type actions, and page state. For SkyEngine, the comparable surface should include ECS world state, scene metadata, render diagnostics, frame captures, RenderGraph resources, asset status, physics probes, and structured run reports.

## Current State

SkyEngine already has useful building blocks:

- `App` owns the winit loop, GPU context, frame lifecycle, and `FrameContext`.
- `FrameContext` exposes `ctx.render()`, `ctx.render_stats()`, `ctx.request_exit()`, `ctx.set_title()`, and surface size.
- `Diagnostics` provides structured warning/error events with ids, severities, subsystems, fields, and frame/entity metadata.
- `RenderStats` already reports draw calls, light count, and missing render assets.
- `RenderSettings` supports debug views for modern 3D, GI, and directional shadows.
- RenderGraph has named passes, imported resources, visualization, tests, and readback-adjacent infrastructure.
- GPU context supports headless device creation for tests.
- Scene, asset, physics, audio, and UI modules are increasingly structured and serializable.
- `examples/render/three_d_demo.rs` is a strong first target because it exercises camera, mesh, material, lights, shadows, GI debug views, post effects, and app lifecycle.

Main missing pieces:

- No canonical agent run mode for examples or scenes.
- No fixed-frame app harness that runs, captures, reports, and exits predictably.
- No stable JSON report schema for agents.
- No standard screenshot / render-target capture artifact contract.
- No built-in visual assertions such as non-black frame, non-empty shadow map, or expected draw count.
- No unified way to collect diagnostics, render stats, asset status, and captures from a run.
- No standalone Sky Agent executable to drive scenarios.
- No protocol layer for external agents to query and operate the engine.

## Design Principles

- Make agent verification a first-class engine capability, not a pile of ad hoc example flags.
- The main product is **Sky Agent**, an independent executable distributed as `sky-agent` / `sky-agent.exe`.
- Examples and apps should provide thin probe hooks; they should not own the agent workflow.
- Start with Sky Agent driving one real example, then generalize to scenes and interactive sessions.
- Prefer structured JSON reports over free-form logs.
- Prefer deterministic fixed-frame runs over wall-clock-driven observation.
- Keep human-readable artifacts beside machine-readable reports.
- Make failures explainable: every failed check should say what was observed and what likely subsystem produced it.
- Keep probe APIs usable from CI, local development, and external agents.
- Do not make agent tooling depend on private renderer internals unless the tool is explicitly marked expert/debug.
- Do not require a visible window when a headless or hidden capture path can provide the same signal.
- Treat visual correctness as a testable artifact: screenshots, debug buffers, image stats, and optional diffs.

## Core Concept: Agent Scenario

An `AgentScenario` is a reproducible run target with declared setup, frame count, captures, checks, and outputs.

Target shape:

```rust
let scenario = AgentScenario::new("three_d_demo")
    .frames(120)
    .size(1280, 720)
    .seed(1)
    .capture_color("frame_120.png")
    .capture_debug_view(RenderDebugView::DirectionalShadowCascade(0), "shadow_cascade_0.png")
    .assert_no_diagnostics(DiagnosticSeverity::Error)
    .assert_missing_assets(0)
    .assert_draw_calls_at_least(1)
    .assert_frame_not_black("frame_120.png");
```

The scenario does not replace normal examples. It wraps them in an agent-verifiable run mode.

## Target User Experience

### Standalone Sky Agent First

The primary user-facing entry point should be a directly usable executable:

```bash
cargo run --bin sky-agent --features app -- \
  run-example three_d_demo \
  --frames 120 \
  --size 1280x720 \
  --capture color,shadow_cascade_0 \
  --out target/sky-agent/three_d_demo
```

Packaged usage should be equally direct:

```bash
sky-agent.exe run-example three_d_demo ^
  --frames 120 ^
  --size 1280x720 ^
  --capture color,shadow_cascade_0 ^
  --out target/sky-agent/three_d_demo
```

The tool owns:

- command-line UX;
- output directory layout;
- report schema;
- stdout/stderr capture;
- exit code policy;
- artifact naming;
- scenario orchestration;
- future interactive protocol.

The example only needs enough integration to accept probe instructions, run deterministically, expose diagnostics/stats/captures, and exit.

### Thin Example Hook

The example-local form can exist as an implementation detail and debugging escape hatch:

```bash
cargo run --example three_d_demo --features app --release -- \
  --agent-probe \
  --frames 120 \
  --out target/sky-agent/three_d_demo
```

Expected output directory:

```text
target/sky-agent/three_d_demo/
  report.json
  color_0120.png
  shadow_cascade_0_0120.png
  diagnostics.json
  render_stats.json
  run.log
```

Future scene target:

```bash
sky-agent run-scene assets/scenes/test.scene.json \
  --pipeline modern_3d \
  --frames 60 \
  --capture color,depth,normal \
  --out target/sky-agent/test_scene
```

### Agent Consumption

An agent should be able to read one file first:

```json
{
  "schema_version": 1,
  "scenario": "three_d_demo",
  "status": "failed",
  "command": "cargo run --example three_d_demo --features app -- --agent-probe --frames 120",
  "frames": {
    "requested": 120,
    "completed": 120
  },
  "diagnostics": {
    "error_count": 0,
    "warning_count": 1,
    "events_path": "diagnostics.json"
  },
  "render_stats": {
    "draw_calls": 0,
    "light_count": 5,
    "missing_render_assets": 0
  },
  "captures": [
    {
      "name": "color",
      "path": "color_0120.png",
      "width": 1280,
      "height": 720,
      "checks": {
        "not_black": false,
        "nonzero_pixel_ratio": 0.0
      }
    }
  ],
  "checks": [
    {
      "id": "render.draw_calls.at_least",
      "status": "failed",
      "expected": ">= 1",
      "observed": "0",
      "help": "The frame rendered no draw calls. Check camera visibility, mesh extraction, material registration, or render pipeline setup."
    }
  ]
}
```

## Architecture

### Independent Tool Boundary

Sky Agent is the orchestration boundary. It should be able to run from outside the app process, collect artifacts, normalize reports, and provide an eventual interactive protocol.

Responsibilities:

- choose scenario target;
- spawn the example/app/scene runner;
- pass probe configuration through CLI, environment, or a small config file;
- capture stdout/stderr;
- watch timeout and process exit;
- collect artifacts from the run directory;
- parse and validate the inner report;
- write the final canonical `report.json`;
- choose process exit code for CI and agents.

The app process is a probe target. It should avoid knowing about external agent concerns such as cargo command shape, CI artifact rules, or protocol transport.

### In-Engine Probe Hook

Add a small optional app-side probe hook:

```rust
pub struct AgentProbeConfig {
    pub enabled: bool,
    pub output_dir: PathBuf,
    pub max_frames: u64,
    pub capture_frames: Vec<u64>,
    pub captures: Vec<AgentCaptureRequest>,
    pub checks: Vec<AgentCheckSpec>,
    pub exit_when_complete: bool,
}
```

Potential locations:

- `src/app/agent.rs` for app lifecycle integration.
- `src/render/probe.rs` for render capture/check helpers.
- `src/diagnostics/report.rs` for report serialization helpers.
- `src/bin/sky-agent.rs` for the independent orchestration tool.

The hook should stay small. If the concept stabilizes, expose a public `sky_engine::agent` module behind a feature flag, but keep orchestration in the `sky-agent` executable.

### Report Types

Reports should be plain serde structs:

```rust
pub struct AgentRunReport {
    pub schema_version: u32,
    pub engine_version: String,
    pub scenario: String,
    pub status: AgentRunStatus,
    pub command: Vec<String>,
    pub cwd: String,
    pub git: Option<AgentGitInfo>,
    pub frames: AgentFrameSummary,
    pub diagnostics: AgentDiagnosticsSummary,
    pub render_stats: Option<AgentRenderStats>,
    pub captures: Vec<AgentCaptureReport>,
    pub checks: Vec<AgentCheckReport>,
    pub artifacts: Vec<AgentArtifact>,
}
```

Required properties:

- Stable field names.
- Relative artifact paths from the output directory.
- Human-oriented `help` field on failures.
- Machine-oriented `id`, `status`, `expected`, and `observed` fields.
- Schema version for future changes.

### Captures

Capture types:

- `color`: final presented color or current scene color.
- `depth`: scene depth.
- `normal`: prepass normal.
- `material`: material id/properties debug output.
- `velocity`: motion vectors when available.
- `shadow_cascade_N`: directional shadow cascade.
- `shadow_coverage`: cascade coverage overlay.
- `gi_probes`, `gi_irradiance`, `gi_visibility`, `gi_ray_budget`.
- `render_graph_resource:<name>` for expert tooling.

Initial MVP only needs:

- final color screenshot;
- directional shadow cascade 0 debug view;
- optional current `RenderDebugView` capture if already supported by settings.

### Checks

Checks are the agent-facing assertions that turn visual/runtime behavior into actionable feedback.

MVP checks:

- `app.completed_frames`: app reached requested frame count.
- `diagnostics.no_errors`: no error diagnostics emitted.
- `render.draw_calls.at_least`: draw calls above threshold.
- `render.missing_assets.equals`: missing render asset count equals expected value.
- `capture.not_black`: captured image has enough nonzero pixels.
- `capture.not_flat`: image has enough luminance/color variance.
- `capture.exists`: artifact was written.

Renderer-specific checks:

- `shadow.non_empty_depth`: shadow map contains depth variation.
- `gbuffer.normal_not_flat`: normal buffer contains more than a default clear value.
- `gi.debug_visible`: GI debug capture has nonzero probe/irradiance signal.
- `postfx.changed_image`: post effect output differs from input above threshold.

Scene/asset checks:

- `scene.main_camera.exists`.
- `scene.visible_mesh_count.at_least`.
- `asset.manifest.valid`.
- `asset.dependencies.resolved`.
- `physics.contacts.observed`.

### Diagnostics Integration

The probe should always dump diagnostics:

- full `diagnostics.json`;
- count by severity;
- count by subsystem;
- first N errors/warnings embedded in `report.json`;
- missed/dropped diagnostic counts.

Important diagnostic ids to add over time:

- `agent.capture.failed`
- `agent.check.failed`
- `render.frame.black`
- `render.shadow.empty`
- `render.camera.no_visible_entities`
- `render.pipeline.no_main_camera`
- `render.pipeline.no_color_output`
- `asset.probe.missing_dependency`

### App Lifecycle Integration

The app runner needs a deterministic probe path:

```text
setup
for frame in 1..=max_frames:
  sync input
  tick schedule
  update app state
  render
  collect stats
  run requested captures
  run checks that are frame-local
write report
shutdown
exit
```

Design requirements:

- Fixed frame cap.
- Optional fixed delta.
- Optional seed.
- Exit even if the example would normally run forever.
- Clean error if a capture is requested before a surface/GPU target exists.
- Record partial report on panic or early failure where possible.

### Headless And Windowed Modes

Support two execution modes:

- `windowed-hidden` or normal windowed capture: easiest first path for examples using winit surfaces.
- `headless`: later path for CI and agent farms, using `GpuContext::new_headless()` and explicit render targets.

MVP can use a real window if that gets the loop working fastest. Long term, headless should be the preferred CI mode.

### Tool Protocol Layer

After the Sky Agent executable exists, add an interactive tool protocol for longer agent sessions.

Potential commands:

```json
{ "tool": "world.query", "input": { "components": ["Transform", "Name"] } }
{ "tool": "scene.patch", "input": { "patch": [...] } }
{ "tool": "app.step", "input": { "frames": 10 } }
{ "tool": "render.capture", "input": { "target": "color" } }
{ "tool": "render.debug_view", "input": { "mode": "shadow_cascade_0" } }
{ "tool": "diagnostics.read", "input": { "since": 42 } }
{ "tool": "physics.raycast", "input": { "origin": [0, 2], "dir": [1, 0], "max": 100 } }
```

This can later be exposed as:

- JSON-RPC over stdio;
- local HTTP;
- MCP-style server;
- editor/plugin bridge.

The protocol should be built on the same scenario/report/capture primitives, not a separate system.

## Milestone 0: Freeze The Agent Contract

Purpose: define the language and artifacts before implementation spreads into examples.

Tasks:

- Add this plan.
- Add a short `docs/agent.md` once the first API lands.
- Define report schema structs in a private module or draft file.
- Define canonical artifact directory layout.
- Define check id naming rules.
- Decide feature flag name:
  - `agent` for public agent APIs;
  - or `app`-only internal first implementation.

Acceptance:

- The report schema can represent successful, failed, and partial runs.
- The artifact layout is documented.
- `three_d_demo` is named as the first scenario target.

## Milestone 1: Sky Agent Skeleton

Purpose: create the independent tool first, even before all capture features exist.

Tasks:

- Add `src/bin/sky-agent.rs`.
- Add CLI shape:
  - `run-example NAME`
  - `--frames N`
  - `--out PATH`
  - `--timeout SECONDS`
  - `--features FEATURES`
- Spawn `cargo run --example NAME --features ... -- --agent-probe ...`.
- Capture stdout/stderr to `run.log`.
- Create the output directory.
- Read the inner report produced by the example.
- Write or normalize the canonical `report.json`.
- Set process exit code from report status.

Acceptance:

```bash
cargo run --bin sky-agent --features app -- run-example three_d_demo --frames 120 --out target/sky-agent/three_d_demo
```

runs a child process, captures logs, collects a report, and exits with a useful status code.

## Milestone 2: Thin Probe Hook For `three_d_demo`

Purpose: make one real render-heavy example runnable by the independent tool.

Tasks:

- Add simple CLI parsing to `examples/render/three_d_demo.rs`:
  - `--agent-probe`
  - `--frames N`
  - `--out PATH`
  - `--fixed-dt SECONDS`
- Add an app-state field for probe config and current frame.
- Request exit after the target frame.
- Collect final `RenderStats`.
- Dump diagnostics if `Diagnostics` resource exists.
- Write `report.json`.
- Add `assert_draw_calls_at_least(1)`.
- Add `assert_missing_assets(0)`.
- Add `assert_no_error_diagnostics()`.

Acceptance:

```bash
cargo run --bin sky-agent --features app -- run-example three_d_demo --frames 120 --out target/sky-agent/three_d_demo
```

produces `report.json` through the tool and exits without manual input.

## Milestone 3: Final Color Capture

Purpose: let an agent see the rendered result.

Tasks:

- Add a screenshot/readback helper for the final frame.
- Save `color_XXXX.png`.
- Compute image statistics:
  - min/max luminance;
  - average luminance;
  - nonzero pixel ratio;
  - variance estimate;
  - alpha coverage if relevant.
- Add `capture.not_black`.
- Add `capture.not_flat`.

Acceptance:

- Probe report links to the color capture.
- If the frame is fully black or clear-color-only, the report fails with a helpful check.

## Milestone 4: Debug View Capture

Purpose: let agents inspect renderer internals without editing demo code.

Tasks:

- Add capture requests that temporarily switch `RenderSettings.debug_view`.
- Capture at least:
  - lit scene;
  - directional shadow cascade 0;
  - shadow coverage overlay if available.
- Restore previous debug settings after capture.
- Include each capture in the report with stats.

Acceptance:

- `three_d_demo` can output both lit color and shadow debug captures in one probe run.
- Empty shadow captures fail `shadow.non_empty_depth` or an equivalent image-stat check.

## Milestone 5: Reusable In-Engine Probe Module

Purpose: avoid copying app-side hook code into every example while keeping Sky Agent as the owner.

Tasks:

- Extract shared config, report, checks, artifact writing, and capture helpers.
- Add a small public or crate-private API:

```rust
pub struct AgentProbe;

impl AgentProbe {
    pub fn from_args(args: impl Iterator<Item = String>) -> Option<Self>;
    pub fn begin_frame(&mut self, ctx: &mut FrameContext);
    pub fn after_render(&mut self, ctx: &mut FrameContext);
    pub fn finish(&mut self, world: &mut World);
}
```

- Keep example integration minimal.

Acceptance:

- A second example can opt into probe mode with little boilerplate.
- Existing non-probe example behavior is unchanged.

## Milestone 6: Stronger Sky Agent Orchestration

Purpose: turn the skeleton into a robust agent-facing command-line tool.

Tasks:

- Normalize output directory creation and report paths.
- Capture stdout/stderr to `run.log`.
- Parse the produced report and set process exit code:
  - `0` for success;
  - nonzero for failed checks or run failure.
- Add `--open-report` only as a human convenience later, not in MVP.

Acceptance:

```bash
cargo run --bin sky-agent --features app -- run-example three_d_demo --frames 120
```

runs the scenario and returns a useful exit code for CI and agents.

## Milestone 7: Persistence And Asset Probes

Purpose: extend beyond hard-coded examples.

Tasks:

- Add a persistence-document probe target built on `Persistence` / `PersistDocument`.
- Register known component serializers for built-in render, physics, audio, and UI components as features allow.
- Add asset validation report:
  - manifest present;
  - source/cooked artifacts present;
  - dependencies resolved;
  - runtime load/install state.
- Add persistence document checks:
  - main camera exists;
  - at least one visible renderer;
  - no duplicate persist ids;
  - referenced assets exist.

Acceptance:

- An agent can load a saved scene, run it for N frames, and receive a report without writing a custom example.

## Milestone 8: RenderGraph And GPU Inspectors

Purpose: let agents debug modern renderer failures.

Tasks:

- Dump RenderGraph pass order.
- Dump declared resources and physical aliases.
- Dump per-pass input/output resource names.
- Dump active pipeline asset steps.
- Add optional DOT graph artifact.
- Add GPU timing stats when profiler support is enabled.
- Add checks:
  - required pass exists;
  - pass produced non-empty output;
  - imported resource bound;
  - history resource available.

Acceptance:

- If a new post effect is black, an agent can see which pass produced the black texture and which resources it read.

## Milestone 9: Interactive Agent Session

Purpose: move from one-shot verification to live debugging.

Tasks:

- Add a long-running probe mode:

```bash
sky-agent serve --scenario three_d_demo --stdio
```

- Support commands:
  - step frames;
  - capture target;
  - switch debug view;
  - read diagnostics;
  - query ECS summary;
  - inspect selected entity;
  - apply scene patch if scene runtime is active.
- Return structured JSON for every command.

Acceptance:

- An external agent can run:

```text
step 30 -> capture color -> read diagnostics -> switch shadow debug -> capture -> step 1
```

without restarting the app.

## Milestone 10: CI And Regression Library

Purpose: make visual/runtime expectations durable.

Tasks:

- Add probe scenarios for:
  - `three_d_demo`;
  - sprite demo;
  - textured demo;
  - lighting demo;
  - render graph showcase;
  - physics arcade demo;
  - tiled physics demo.
- Add golden or statistical thresholds where stable.
- Store reports as CI artifacts.
- Add a short guide for adding new scenarios.

Acceptance:

- Renderer/API changes can be validated by both `cargo test` and agent-visible scenario probes.

## Report Schema Rules

- Every check has a stable `id`.
- Every check has `status`: `passed`, `failed`, or `skipped`.
- Failed checks include `expected`, `observed`, and `help`.
- Artifact paths are relative to the report directory.
- Reports include a schema version.
- Reports include engine version and command line.
- Reports should include git info when available but must still work outside a git checkout.
- Reports should be compact enough for agents to read directly.
- Large raw data belongs in separate artifact files.

## Check Id Naming

Use subsystem-prefixed ids:

```text
app.completed_frames
diagnostics.no_errors
render.draw_calls.at_least
render.missing_assets.equals
capture.not_black
capture.not_flat
shadow.non_empty_depth
scene.main_camera.exists
asset.dependencies.resolved
physics.contacts.observed
```

This matches the existing diagnostics direction and keeps failure handling predictable.

## First MVP Definition

The smallest useful version is:

- `sky-agent run-example three_d_demo` exists.
- Sky Agent launches the example as a child process.
- The example supports a thin `--agent-probe` hook.
- The app runs for a fixed number of frames.
- The app exits automatically.
- Sky Agent writes or normalizes `report.json`.
- It records render stats.
- It records diagnostics.
- It fails if there are error diagnostics.
- It fails if draw calls are zero.
- It fails if missing render assets are nonzero.

The first visual version adds:

- final color capture;
- image statistics;
- `capture.not_black`.

The first renderer-debug version adds:

- shadow cascade debug capture;
- shadow non-empty check;
- RenderGraph pass list in the report.

## Open Questions

- Should the public feature be named `agent`, `probe`, or remain under `app` until stable?
- Should `sky-agent run-example` shell out to cargo first, or should it eventually use a compiled scenario registry?
- How much of `RenderGraph` inspection should be public stable API versus expert/debug-only?
- Should image diffing use absolute thresholds, perceptual thresholds, or mostly statistical smoke checks at first?
- Should headless rendering be required before CI adoption, or can the first CI pass use windowed capture on machines that support it?
- Should agent reports include full ECS summaries by default, or only when requested to avoid huge JSON?

## Why This Matters

Traditional engine tests prove that isolated logic is correct. Agent probes prove that a real app produced the expected observable behavior.

For an AI coding agent, that difference is enormous. `cargo check` can say the code compiles. A probe report can say:

- the app launched;
- the scene rendered;
- the frame was not black;
- the shadow map had content;
- no assets were missing;
- no render diagnostics fired;
- draw calls and light counts looked plausible.

That closes the loop from code generation to observed engine behavior.
