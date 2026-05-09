# Yakui Fork And wgpu 29 UI Upgrade Plan

## Position

SkyEngine's current native UI is useful as a proof of rendering, input, and
text plumbing, but it should not be the long-term game UI authoring surface.

The target direction is:

- upgrade SkyEngine's `wgpu` ecosystem to one current generation;
- introduce a backend-neutral UI host;
- fork or vendor yakui so it is controlled by this repository;
- add yakui as a pluggable experimental game UI backend;
- keep egui for debug/tool overlays;
- keep the current ECS UI only as a legacy bridge until yakui proves itself.

This plan intentionally treats the yakui fork and the SkyEngine `wgpu` upgrade
as one dependency-alignment project. The main failure mode to avoid is having
multiple incompatible `wgpu::Device`, `wgpu::Queue`, or `wgpu::TextureView`
types in the same render path.

## Current Snapshot

At the time this plan was written:

- SkyEngine uses `wgpu = 24`.
- SkyEngine uses `winit = 0.30`.
- `egui-wgpu = 0.31` aligns with `wgpu 24`.
- `glyphon = 0.8` aligns with `wgpu 24`.
- crates.io `yakui-wgpu = 0.3.0` depends on `wgpu 22`.
- upstream yakui main has moved to the `wgpu 29` generation.
- latest `egui-wgpu` and latest `glyphon` are also in the `wgpu 29` generation.
- `renderling = 0.4.9` currently pulls `wgpu 22` and must be treated as a
  separate experimental backend risk.

The clean target is to make the normal app/UI stack use one `wgpu` version.

Progress after initial implementation:

- `UiHost` and the minimal `UiBackend` contract exist under `src/ui/backend/`.
- the existing ECS retained UI is registered through `LegacyUiBackend`;
- `FrameContext::ui()` exposes a backend-neutral `UiFrame` facade;
- `examples/ui/hud_menu.rs` uses `ctx.ui().update()` and
  `ctx.ui().render_overlays()`;
- yakui is not vendored yet, and the `wgpu 29` upgrade has not started.

## Target Dependency Set

Preferred target:

```toml
wgpu = "29"
winit = "0.30"
egui = "0.34"
egui-wgpu = "0.34"
egui-winit = "0.34"
glyphon = "0.11"
```

Yakui should be consumed through a local fork or vendor path, not directly from
crates.io:

```toml
yakui = { path = "crates/vendor/yakui/crates/yakui", optional = true }
yakui-core = { path = "crates/vendor/yakui/crates/yakui-core", optional = true }
yakui-widgets = { path = "crates/vendor/yakui/crates/yakui-widgets", optional = true }
yakui-wgpu = { path = "crates/vendor/yakui/crates/yakui-wgpu", optional = true }
yakui-winit = { path = "crates/vendor/yakui/crates/yakui-winit", optional = true }
```

The exact paths can change if yakui is kept as a git submodule or an external
fork. The important rule is that SkyEngine owns the integration version.

## Feature Strategy

Keep feature boundaries explicit:

```toml
app = ["asset", "dep:wgpu", "dep:winit", "dep:pollster", "dep:bytemuck", "dep:gltf"]
egui = ["app", "dep:egui", "dep:egui-wgpu", "dep:egui-winit"]
ui-core = ["app"]
ui-legacy = ["ui-core", "dep:glyphon"]
ui = ["ui-legacy"]
yakui-ui = [
    "ui-core",
    "dep:yakui",
    "dep:yakui-core",
    "dep:yakui-widgets",
    "dep:yakui-wgpu",
    "dep:yakui-winit",
]
```

Rules:

- `egui` remains the debug/tool UI feature.
- `ui-core` contains only backend-neutral host, input capture, texture bridge,
  and public extension points.
- `ui-legacy` contains the current native retained UI backend.
- `ui` remains an alias for the current legacy UI until migration is complete.
- `yakui-ui` starts experimental and registers a yakui backend into `ui-core`;
  it must not replace `ui` in the first PR.
- examples using game HUDs can opt into `yakui-ui` one at a time.
- backend-neutral APIs should not mention yakui types.
- yakui-specific convenience APIs may exist behind `yakui-ui`, but they should
  be thin extensions over the same backend host used by other UI backends.

## Non-Goals

This project should not:

- rewrite the entire UI authoring model in the same PR as the `wgpu` upgrade;
- delete the current `ui` module before yakui has a working replacement demo;
- remove egui from debug/tool workflows;
- upgrade `winit` to a beta just because `wgpu` is being upgraded;
- force renderling or Kajiya to be fixed in the same slice unless they block
  normal `app` builds;
- design a full editor UI framework before game HUD/menu use cases work.

## Architecture End State

Desired module shape:

```text
src/ui/
  mod.rs
  backend/
    mod.rs
    traits.rs
    host.rs
    context.rs
    capture.rs
    texture.rs
  backends/
    legacy/
      mod.rs
      input.rs
      render.rs
      state.rs
    yakui/
      mod.rs
      input.rs
      render.rs
      state.rs
      texture.rs
      widgets.rs
```

The key architectural rule is that `FrameContext`, `App`, and `GpuContext`
should talk to a backend-neutral `UiHost`, not directly to yakui.

Candidate core types:

```rust
pub struct UiHost {
    backends: Vec<Box<dyn UiBackend>>,
    active_game_backend: Option<UiBackendId>,
}

pub trait UiBackend: Send + 'static {
    fn id(&self) -> UiBackendId;
    fn name(&self) -> &'static str;
    fn handle_event(&mut self, ctx: UiEventContext<'_>) -> UiEventResponse;
    fn begin_frame(&mut self, ctx: UiBeginFrameContext<'_>);
    fn render_overlay(&mut self, ctx: UiRenderContext<'_>) -> Result<(), UiError>;
    fn wants_pointer(&self) -> bool;
    fn wants_keyboard(&self) -> bool;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}
```

Backend-specific authoring should be reached through typed extension methods or
downcast helpers:

```rust
ctx.ui_backend_mut::<YakuiBackend>()?.run(|| {
    yakui::widgets::label("Hello yakui");
});

ctx.render_ui_overlays();
```

The final API can differ, but user code should not need to manually hold a
`yakui_core::Yakui`, a yakui renderer, or raw winit event translation state.
More importantly, adding another backend later must not require adding another
pair of `FrameContext::update_xxx()` / `FrameContext::render_xxx()` methods.

## Pluggable Backend Contract

The backend contract should separate four jobs:

- event ingestion from winit and SkyEngine input;
- per-frame UI build/update;
- overlay rendering into the current surface frame;
- input capture reporting.

The host owns ordering and aggregation:

```text
App receives winit event
  -> SkyEngine input resource
  -> UiHost::handle_event(event)
       -> legacy backend if installed
       -> yakui backend if installed
       -> future backend if installed

Frame update
  -> UiHost::begin_frame(...)
  -> backend-specific build calls
  -> ctx.render()
  -> UiHost::render_overlays(...)
```

Capture aggregation should be backend-neutral:

```rust
ctx.ui_wants_pointer()
ctx.ui_wants_keyboard()
```

Those methods ask `UiHost`, and `UiHost` combines installed backend capture
state. A backend can be configured as game UI, debug UI, passive overlay, or
disabled. The first version can keep this as simple flags instead of a complex
layering system.

Backend plugins should install themselves explicitly:

```rust
YakuiUiPlugin::default().install(world);
LegacyUiPlugin::default().install(world);
```

The plugin installs backend state into `UiHost`; it should not create a second
parallel UI lifecycle.

## Render Integration Model

Every UI backend should start as an overlay path:

```text
winit events
  -> SkyEngine input
  -> UiHost
  -> installed UI backend input adapters

frame update
  -> game update
  -> UI backend build/update
  -> ctx.render()
  -> UiHost renders overlay backends using current surface view
  -> present
```

This mirrors the current egui integration and avoids forcing yakui or any other
game UI backend into the main `RenderComposer` architecture before the API has
proved itself.

Longer term, a backend can become a `RenderFeature` or a render graph node if
that helps composition. The first successful slice should be boring: draw UI on
top of the already-rendered frame through `UiHost`.

## Input Integration Model

UI backends should consume the same winit events that already feed SkyEngine's
input resource.

Required behavior:

- pointer position in logical pixels;
- pointer press/release;
- mouse wheel;
- keyboard text input where yakui supports it;
- window scale factor updates;
- focus/blur handling;
- `wants_pointer` and `wants_keyboard` style queries for game input blocking;
- backend priority when multiple UI backends are installed.

SkyEngine should expose an engine-level query rather than a yakui-specific one:

```rust
ctx.ui_wants_pointer()
ctx.ui_wants_keyboard()
```

In the short term, these should ask `UiHost`, which delegates to yakui state
when `yakui-ui` is installed and to legacy `UiState` when `ui-legacy` is
installed.

## Phase 0: Baseline And Safety Checks

Goal: record the current behavior before touching dependencies.

Tasks:

- run `cargo check --features app`;
- run `cargo test`;
- run `cargo test --features app`;
- run `cargo check --examples --features app`;
- run `cargo check --example hud_menu --features ui`;
- run `cargo check --example egui_demo --features egui`;
- record any pre-existing failures in the PR description.

Expected result:

- no code changes;
- known broken examples or tests are documented;
- later failures can be separated from existing worktree damage.

## Phase 1: Upgrade Core wgpu Stack

Goal: move SkyEngine's normal app/render stack to `wgpu 29`.

Tasks:

- update `wgpu` to `29`;
- update `egui`, `egui-wgpu`, and `egui-winit` to the matching current line;
- update `glyphon` to a matching current line;
- keep `winit` on `0.30` unless a direct dependency requires otherwise;
- run `cargo update -p wgpu -p egui -p egui-wgpu -p egui-winit -p glyphon`;
- fix compile errors in `src/gpu`, `src/app`, `src/render`, and `src/ui`.

Likely breakpoints:

- `wgpu::InstanceDescriptor`;
- `wgpu::DeviceDescriptor`;
- `wgpu::SurfaceConfiguration`;
- `wgpu::RenderPassDescriptor`;
- `wgpu::ComputePassDescriptor`;
- pipeline descriptor fields;
- shader entry point fields;
- texture copy type renames;
- surface error enum additions;
- `egui_wgpu::Renderer` constructor and render API;
- glyphon text atlas/cache constructor and prepare/render API.

Do not:

- change render architecture in this phase;
- introduce yakui in this phase;
- fix unrelated material/GI/shadow behavior unless compilation requires it.

Verification:

```powershell
cargo check --features app
cargo test --features app graph
cargo check --example clear_screen --features app
cargo check --example egui_demo --features egui
cargo check --example hud_menu --features ui
```

Acceptance criteria:

- only one `wgpu` version appears in `cargo tree --features app -i wgpu`;
- only one `wgpu` version appears in `cargo tree --features egui -i wgpu`;
- only one `wgpu` version appears in `cargo tree --features ui -i wgpu`;
- app, egui, and legacy UI examples compile.

## Phase 2: Isolate Experimental Renderer Conflicts

Goal: keep normal builds clean even if optional experimental renderers lag.

Tasks:

- inspect `cargo tree --features renderling-renderer -i wgpu`;
- decide whether to upgrade, vendor, or temporarily quarantine renderling;
- ensure `renderling-renderer` cannot pull a second `wgpu` into normal `app`;
- inspect Kajiya feature dependencies for direct or indirect `wgpu` coupling;
- document any renderer temporarily disabled by the `wgpu 29` migration.

Policy:

- normal `app`, `ui`, `egui`, and `yakui-ui` builds must be single-`wgpu`;
- experimental renderer features may temporarily be marked broken in docs;
- do not hide duplicate-`wgpu` type conflicts behind broad trait objects.

Verification:

```powershell
cargo tree --features app -i wgpu
cargo tree --features ui -i wgpu
cargo tree --features egui -i wgpu
cargo tree --features renderling-renderer -i wgpu
```

Acceptance criteria:

- normal features remain clean;
- experimental renderer exceptions are explicit and isolated.

## Phase 3: Vendor Or Fork Yakui

Goal: bring yakui under SkyEngine control without touching runtime integration.

Recommended options:

1. Git fork under the SkyEngine organization or user account.
2. Git submodule under `crates/vendor/yakui`.
3. Vendored copy under `crates/vendor/yakui`.

Preferred for early iteration: vendored copy or submodule. It makes local API
patches easy and keeps the project build reproducible.

Tasks:

- add yakui source under `crates/vendor/yakui`;
- pin it to an upstream commit known to use `wgpu 29`;
- remove yakui examples/tests that are not needed for SkyEngine builds if they
  complicate workspace behavior;
- add path dependencies in `Cargo.toml`;
- add a `yakui-ui` feature;
- run `cargo tree --features yakui-ui -i wgpu`.

Fork maintenance rules:

- keep a `SKYENGINE_CHANGES.md` file in the vendored yakui root;
- every local yakui patch must be listed there;
- prefer small compatibility patches over style rewrites;
- upstream merge points should be tagged in commit messages;
- do not expose fork-only yakui APIs through SkyEngine public API unless needed.

Verification:

```powershell
cargo check --features yakui-ui
cargo tree --features yakui-ui -i wgpu
```

Acceptance criteria:

- yakui crates compile as dependencies;
- no second `wgpu` version appears;
- no SkyEngine app behavior changes yet.

## Phase 4: UI Host And Backend Contract

Goal: introduce a backend-neutral UI host before adding yakui runtime behavior.

Status: initial slice complete. The host, backend trait, legacy adapter, and
`FrameContext::ui()` facade are present. Future work in this phase should focus
on tightening the API after yakui exercises it rather than expanding the trait
preemptively.

Candidate resources:

```rust
pub struct UiHost {
    backends: Vec<Box<dyn UiBackend>>,
    active_game_backend: Option<UiBackendId>,
}

pub struct UiCaptureState {
    wants_pointer: bool,
    wants_keyboard: bool,
}
```

Tasks:

- create `src/ui/backend/` with host, backend trait, context types, capture,
  texture bridge placeholders, and error types;
- add install/ensure resource functions for `UiHost`;
- route winit events into `UiHost` from `App`;
- add `FrameContext::render_ui_overlays()`;
- add `FrameContext::ui_wants_pointer()` and `FrameContext::ui_wants_keyboard()`;
- adapt the current legacy UI path to either register as a backend or bridge
  through the host with a temporary adapter;
- keep backend trait methods small enough for yakui, legacy UI, and future
  backends to implement without fake concepts.

Minimal API sketch:

```rust
ctx.update_ui();
ctx.render();
ctx.render_ui_overlays();
```

For backend-specific authoring:

```rust
ctx.ui_backend_mut::<YakuiBackend>()?.run(|| {
    yakui::widgets::label("Hello yakui");
})?;
```

Verification:

```powershell
cargo check --features ui-core
cargo check --example hud_menu --features ui
cargo test --features ui-core ui
```

Acceptance criteria:

- `UiHost` can be installed lazily;
- app input can fan out to `UiHost`;
- capture state can be queried without knowing the backend;
- legacy UI keeps compiling;
- app builds without `yakui-ui` are unaffected;
- no public `FrameContext` method is yakui-specific in this phase.

## Phase 5: Minimal Yakui Backend

Goal: install yakui as one pluggable backend through `UiHost`.

Tasks:

- create `src/ui/backends/yakui/` behind `yakui-ui`;
- implement `UiBackend` for `YakuiBackend`;
- store `yakui_core::Yakui`, yakui input adapter, and yakui-wgpu renderer inside
  the backend;
- expose a typed extension method for running yakui UI build closures;
- route window resize and scale factor through backend context;
- route winit input through backend context;
- implement capture reporting through `UiBackend::wants_pointer()` and
  `UiBackend::wants_keyboard()`;
- render into `GpuContext::surface_view()` through `UiRenderContext`;
- use load op `Load` so game content is preserved;
- support surface format and sample count from SkyEngine;
- handle empty UI without emitting invalid passes;
- handle surface resize without leaking old texture state.

Frame order:

```rust
ctx.render();
ctx.ui_backend_mut::<YakuiBackend>()?.run(|| {
    // build UI
})?;
ctx.render_ui_overlays();
```

Alternative order if yakui needs build before render:

```rust
ctx.ui_backend_mut::<YakuiBackend>()?.run(|| {
    // build UI
})?;
ctx.render();
ctx.render_ui_overlays();
```

The final order must be documented and consistent with egui and legacy UI.

Verification:

```powershell
cargo check --features yakui-ui
cargo run --example yakui_demo --features yakui-ui
```

Acceptance criteria:

- yakui is installed through `UiHost`, not a parallel lifecycle;
- yakui text and basic widgets render on top of the scene;
- resizing the window keeps UI correctly scaled;
- no extra command buffer submission breaks `GpuContext` frame lifecycle;
- render pass uses the same `wgpu` device as the rest of SkyEngine;
- replacing yakui with another backend would not require changing `App` frame
  lifecycle code.

## Phase 6: First Demo

Goal: prove a pluggable yakui backend can replace real game UI authoring pain.

Create:

```text
examples/ui/yakui_demo.rs
```

The demo should include:

- a screen-space HUD;
- a modal/menu panel;
- buttons;
- slider;
- toggle;
- progress bar;
- dynamic text;
- input blocking against game input;
- a simple animated value from app state;
- window resize support.

Do not make it a marketing page. It should open directly into the usable UI.

Verification:

```powershell
cargo check --example yakui_demo --features yakui-ui
cargo run --example yakui_demo --features yakui-ui
```

Acceptance criteria:

- the demo requires much less boilerplate than `hud_menu`;
- no manual `EntityId` UI bookkeeping is needed for basic widgets;
- button/slider/toggle state flows naturally into app state;
- UI can block pointer input from the game when hovered or active;
- demo code uses the backend-neutral host for lifecycle and only enters
  yakui-specific code for widget authoring.

## Phase 7: Texture And Asset Bridge

Goal: allow pluggable UI backends, starting with yakui, to display SkyEngine
textures.

Tasks:

- add a backend-neutral `UiTextureRegistry`;
- map `Handle<TextureAsset>` or prepared GPU texture views into backend texture
  IDs;
- cache backend texture IDs by SkyEngine asset handle and generation;
- update backend texture views when assets reload;
- remove stale backend texture IDs when assets unload;
- define fallback behavior for missing textures;
- document lifetime rules.

Candidate API:

```rust
let icon = ctx.ui_texture(texture_handle);
yakui::widgets::image(icon, size);
```

Verification:

```powershell
cargo check --example yakui_demo --features yakui-ui,asset
```

Acceptance criteria:

- images render in yakui UI;
- asset reload or replacement does not crash;
- texture cache does not grow forever in normal use;
- texture bridge is not yakui-only at the `FrameContext` or asset layer.

## Phase 8: UI Migration Strategy

Goal: decide whether yakui becomes the primary game UI path.

Migration candidates:

- `examples/ui/hud_menu.rs`;
- `examples/game/lawn_defense.rs`;
- `examples/game/kenney_platformer.rs`;
- `examples/vn/ui_demo.rs`;
- `examples/game/last_light_guild`.

Rules:

- migrate one demo at a time;
- keep old demo versions temporarily if they are useful regression tests;
- compare authoring complexity before deleting old UI code;
- preserve game behavior, not old entity structure.

Decision checklist:

- yakui demo code is materially simpler than ECS UI code;
- layout handles real menus without manual coordinates everywhere;
- styling is not worse than current UI;
- text rendering quality is acceptable;
- input blocking is reliable;
- yakui remains a backend behind `UiHost`, not a special app lifecycle;
- no duplicate `wgpu` dependency remains;
- fork maintenance burden is acceptable.

Possible outcomes:

1. Yakui becomes the default game UI backend and current ECS UI becomes
   `ui-legacy`.
2. Yakui remains optional and current ECS UI gets a declaration facade.
3. Yakui is rejected and the pluggable backend host remains for a SkyEngine UI
   v2 backend.

## Phase 9: Public API Cleanup

Only after at least two real demos are migrated:

- decide whether `ui` should point to yakui, legacy UI, or just `ui-core`;
- decide whether legacy UI should move to `ui-legacy`;
- decide which backend is installed by default, if any;
- update `docs/ui.md`;
- update `docs/ui_tutorial.md`;
- update `examples/README.md`;
- update README feature matrix;
- add `src/ui/AGENTS.md` if the UI module grows multiple backends;
- document yakui fork policy.

Do not do this cleanup in the first yakui PR.

## Testing Matrix

Core commands:

```powershell
cargo test
cargo check --features app
cargo test --features app
cargo check --examples --features app
```

UI commands:

```powershell
cargo check --example hud_menu --features ui
cargo check --example weird_ui_lab --features ui
cargo check --example egui_demo --features egui
cargo check --example yakui_demo --features yakui-ui
```

Dependency checks:

```powershell
cargo tree --features app -i wgpu
cargo tree --features ui -i wgpu
cargo tree --features egui -i wgpu
cargo tree --features yakui-ui -i wgpu
```

Render checks:

```powershell
cargo test --features app graph
cargo test --features app render::runtime
cargo check --example clear_screen --features app
cargo check --example sprite_demo --features app
cargo check --example textured_demo --features app
```

If render APIs change while upgrading `wgpu`, also run:

```powershell
cargo check --examples --features app
```

## Risk Register

### Duplicate wgpu Versions

Risk: two dependencies pull incompatible `wgpu` versions.

Mitigation:

- run `cargo tree -i wgpu` for every UI/render feature;
- align egui, glyphon, yakui, and SkyEngine first;
- quarantine renderling if needed.

### wgpu API Churn

Risk: the upgrade touches many renderer files at once.

Mitigation:

- phase 1 is only dependency/API repair;
- no UI architecture rewrite during phase 1;
- prefer mechanical compatibility fixes before behavior changes.

### Yakui Fork Drift

Risk: local yakui patches become unmergeable.

Mitigation:

- keep patches small;
- document each patch in `SKYENGINE_CHANGES.md`;
- upstream regularly while the fork is still small.

### Input Semantics Diverge

Risk: SkyEngine input, egui input, legacy UI input, and yakui input disagree.

Mitigation:

- centralize winit event fan-out in `App`;
- expose engine-level `ui_wants_pointer` and `ui_wants_keyboard`;
- add demos that prove game input is blocked when UI is active.

### Text Rendering Regression

Risk: glyphon upgrade or yakui text path changes quality/performance.

Mitigation:

- keep `hud_menu` and `weird_ui_lab` compiling during the transition;
- add visual manual checks for text clipping, scale factor, and resizing;
- do not remove glyphon until yakui text quality is accepted.

### Surface Frame Lifecycle

Risk: yakui renderer submits command buffers or begins passes in a way that
conflicts with `GpuContext::begin_frame()` and `end_frame()`.

Mitigation:

- prefer `paint_with_encoder` style integration;
- render through the current `GpuContext` encoder where possible;
- if yakui must create its own command buffer, flush in a clearly documented
  place and verify presentation still works.

### Over-Abstracted Backend Contract

Risk: the `UiBackend` trait becomes too abstract before yakui and legacy UI
prove what they actually need.

Mitigation:

- keep the trait small: events, begin frame, render overlay, capture queries;
- put yakui-specific authoring behind typed extension methods;
- avoid modeling widgets, layout, or styling in `ui-core` during the first
  integration;
- revise the trait after the first yakui demo rather than predicting every
  future backend.

## PR Slicing

Recommended PR sequence:

1. `wgpu 29` dependency upgrade and compile repair.
2. egui/glyphon compatibility repair and examples.
3. experimental backend quarantine or repair.
4. yakui fork/vendor import with no runtime integration.
5. `ui-core` host and `UiBackend` contract.
6. legacy UI adapter or temporary bridge into `UiHost`.
7. `yakui-ui` feature and minimal yakui backend state.
8. yakui overlay renderer through `UiHost`.
9. `yakui_demo`.
10. backend-neutral texture bridge.
11. migrate one real game UI demo.
12. decide primary UI backend direction and update docs.

Each PR should have a narrow acceptance checklist and should not depend on a
visual redesign landing at the same time.

## Acceptance Criteria For The Whole Project

The project is complete when:

- normal app/render/UI features use one `wgpu` version;
- `egui` still works for debug/tool overlays;
- current legacy `ui` still compiles or has an intentional replacement path;
- `UiHost` exists and owns backend lifecycle, event fan-out, overlay rendering,
  and input capture aggregation;
- yakui is available through a controlled fork or vendor path;
- `yakui-ui` registers as a pluggable backend, not a hard-coded app path;
- `yakui-ui` can render an overlay in a real SkyEngine app through `UiHost`;
- yakui can consume SkyEngine input and report UI input capture;
- yakui can display text, buttons, sliders, toggles, progress, and images;
- at least one non-trivial game UI example is migrated or reproduced;
- docs clearly state which UI path is recommended for games;
- duplicate experimental backend dependency risks are documented or fixed.

## Open Questions

- Should backend modules live under `src/ui/backends/*` or separate top-level
  modules?
- Should `ui` eventually alias a default backend, or should users always choose
  `ui-legacy` / `yakui-ui` explicitly?
- Should legacy ECS UI remain supported for data-driven/editor use cases?
- Should yakui be vendored, submoduled, or consumed from a SkyEngine fork?
- How much of yakui's widget/style API should SkyEngine re-export?
- Should UI backends eventually become `RenderFeature`s instead of overlay
  helpers?
- Should the first production migration target a HUD, a VN dialogue UI, or a
  full menu-heavy demo?
- Should multiple game UI backends be allowed simultaneously, or should there
  be exactly one active game UI backend plus passive overlays?
