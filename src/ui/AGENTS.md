# AGENTS.md - `src/ui`

## Overview
- This module owns SkyEngine's current game UI integration behind UI feature flags.
- `ui-core` provides the backend-neutral host contract.
- `ui-legacy` provides the retained ECS UI implementation and adapts it into `UiHost`.
- `ui-serein` adapts the independent `serein` declarative runtime into `UiHost`.
- `yakui-ui` installs an experimental yakui backend into the same `UiHost`.
- `egui` is not part of this module; it is a separate immediate-mode app overlay in `src/app/egui_integration.rs`.

## Feature Flags
- `ui-core`: compiles `UiHost`, `UiBackend`, event/capture/render contexts, and `FrameContext::ui()` facade methods.
- `ui-legacy`: enables retained ECS UI components, layout, input, state, text, renderer, and `LegacyUiBackend`.
- `ui-serein`: enables `sky_engine::ui::serein`, `SereinUiBackend`, `SereinUiPlugin`, `serein` widgets, glyphon text, and SkyEngine-backed HTTP/Bing image loading.
- `yakui-ui`: enables `YakuiBackend` and `YakuiUiPlugin`.

## Current Public Surface
- Backend host and lifecycle:
  - `UiHost`
  - `UiBackend`
  - `UiBackendId`
  - `UiCaptureState`
  - `UiBeginFrameContext`
  - `UiEventContext`
  - `UiEventResponse`
  - `UiRenderContext`
- Host helpers:
  - `ensure_ui_host`
  - `handle_ui_event`
  - `update_ui_backends`
  - `render_ui_overlays`
  - `ui_wants_pointer`
  - `ui_wants_keyboard`
  - `with_ui_backend_mut`
- Legacy UI:
  - `UiPlugin`
  - `UiNode`, `UiPanel`, `UiButton`, `UiText`, `UiImage`, `UiProgressBar`, `UiSlider`, `UiToggle`, `UiScroll`
  - `UiState`, `UiEvents`, `UiTheme`, `UiConfig`, `UiFontBook`
- Yakui UI:
  - `YakuiUiPlugin`
  - `YakuiBackend`
  - `yakui::run`
- Serein UI:
  - `serein::SereinUiPlugin`
  - `serein::install_serein_ui_backend`
  - `serein::SereinUiBackend`
  - `serein::Runtime`
  - `serein::Ui`, `serein::State`, `serein::Signal`, `serein::SignalKey`, and `serein::widgets`
  - `serein::compose`
  - `serein::open_window`
  - Common layout-safe widget helpers live in `serein::widgets`: `scroll_y`, `popover`, and rounded clipping through `.rounded_clip(...)` / `.clip_to_radius()`.

## File Map
- `mod.rs`: module wiring and feature-gated re-exports.
- `core/traits.rs`: backend-neutral UI lifecycle traits and context types.
- `core/host.rs`: `UiHost`, backend registration, event dispatch, frame updates, overlay rendering, capture aggregation.
- `legacy/mod.rs`: legacy UI exports.
- `legacy/plugin.rs`: retained UI resource installation and `UiPlugin`.
- `legacy/backend.rs`: adapter that exposes the retained ECS UI as a `UiBackend`.
- `legacy/components.rs`: retained ECS UI components and authoring types (`ui-legacy`).
- `legacy/layout.rs`: retained UI layout, hit testing, rect maps, and world-layout resolution (`ui-legacy`).
- `legacy/input.rs`: retained UI interaction/update logic (`ui-legacy`).
- `legacy/state.rs`: retained UI runtime state, config, theme, and event queue (`ui-legacy`).
- `legacy/text.rs`: retained UI font discovery/bookkeeping (`ui-legacy`).
- `legacy/render.rs`: retained UI overlay renderer, image quads, glyphon text, and texture binding cache (`ui-legacy`).
- `yakui/mod.rs`: yakui public helpers and re-exports.
- `yakui/backend.rs`: experimental yakui backend and installer.
- `serein/mod.rs`: serein module wiring and re-exports.
- `serein/config.rs`: `SereinUiConfig` and `SereinWindowConfig`.
- `serein/api.rs`: `compose(...)` and `open_window(...)`.
- `serein/plugin.rs`: `SereinUiPlugin` and backend installation.
- `serein/backend.rs`: `SereinUiBackend`, `SereinPendingInput`, and serein-specific capture/IME/render integration.
- `serein/window.rs`: auxiliary native window client for serein-driven windows.
- `serein/input_bridge.rs`: raw input and winit keyboard/IME translation for serein.
- `serein/`: SkyEngine adapter for the independent `serein` runtime, including backend installation, winit input translation, native windows, image resources, and overlay rendering.

## Serein Crate Map
- Serein is a separate sibling repository at `C:\Coding\Serein`; SkyEngine must not regain local copies of its crates.
- `../Serein/crates/serein/src/core/`: ids, elements, events, geometry, styles, fonts, colors, and caches.
- `../Serein/crates/serein/src/reactive/`: external state, signals, the dependency graph, and runtime subscription registry.
- `../Serein/crates/serein/src/dsl/`: the public `Ui` facade, scopes, builders, callbacks, and callback registry.
- `../Serein/crates/serein/src/runtime/frame/`: frame planning, transactional composition, prepared cleanup, commit validation, and infallible apply.
- `../Serein/crates/serein/src/runtime/reconcile/`: scope lifecycle, reuse decisions, structure plans, and retained tree state.
- `../Serein/crates/serein/src/runtime/{input,layers,animation,invalidation}/`: focused runtime subsystems; runtime state must not depend on widgets.
- `../Serein/crates/serein/src/{layout,draw,widgets,agent_debug,testing}/`: layout, draw-list construction, vertical widget slices, diagnostics, and test support.
- `../Serein/crates/serein-wgpu/src/renderer/`: public renderer orchestration plus focused buffer, image, text, backdrop, pipeline, and per-primitive collectors.
- `../Serein/crates/serein-winit/src/lib.rs`: the small winit event adapter; it must not own runtime state.

## App Integration
- App window events can flow through `handle_ui_event(...)`; backends return `UiEventResponse::consumed()` when they consume an event.
- Per-frame UI backend updates flow through `FrameContext::ui().update()` or `FrameContext::update_ui_backends()`.
- Overlay rendering flows through `FrameContext::ui().render_overlays()` or `FrameContext::render_ui_overlays()`.
- Legacy-only direct calls `FrameContext::update_ui()` and `FrameContext::render_ui()` remain available under `ui-legacy`.
- UI capture is surfaced through `FrameContext::ui_wants_pointer()` / `ui_wants_keyboard()` and `UiCaptureState`.

## Current Rendering Model
- UI overlays currently render after scene rendering directly onto the active wgpu surface frame.
- The retained UI renderer in `legacy/render.rs` owns its own wgpu pipelines and glyphon renderer.
- `YakuiBackend` renders through `yakui_wgpu`.
- `SereinUiBackend` renders through an internal renderer backed by `serein-wgpu`; text uses glyphon and image resources are resolved through SkyEngine asset/render caches.
- `serein-wgpu` receives `UiClip` rect/radius data from `serein`; primitive shaders apply rounded clipping, while text remains bounded through glyphon text bounds.
- There is currently no canonical render-pipeline `UiPhase` or `UiFeature`.
- Do not document a future UI phase/feature as current behavior. If planning that migration, put it under `docs/plan/`.

## Serein UI Rules
- Reference repository for behavior parity is `C:\Coding\EUI-NEO`. Check it before changing `C:\Coding\Serein` runtime/widget behavior or `src/ui/serein/` platform adaptation.
- Keep widget behavior traceable to EUI-NEO `components/*.h` and runtime/layout/animation behavior traceable to `core/*.h`.
- Preserve EUI-NEO callback ordering, clamp rules, z-index/layering, modal hit blocking, focus, keyboard, clipboard, IME rect, dirty/redraw, and animation semantics unless there is a documented SkyEngine platform adaptation.
- Keep reusable behavior in the separate Serein repository; `src/ui/serein/` should remain a SkyEngine adapter, and examples should demonstrate parity rather than hide widget implementations.
- Keep `Runtime` and `WgpuRenderer` as public facades over focused subsystem modules. Do not reintroduce a god-object `runtime.rs` or `renderer.rs`, and do not create a broad `RuntimeContext` that centralizes unrelated state.
- Prefer `Ui::scroll_y` / `widgets::scroll_y` for vertical scrollable panels instead of manual viewport + content translation + scrollbar composition. Use `.inset(...)` when the scroll area lives inside a rounded panel so the scrollbar and clipped viewport do not occupy the outer rounded edge.
- Prefer `Ui::popover` / `widgets::popover` for dropdowns, context menus, pickers, and other floating UI that should sit on a root layer instead of resizing the parent layout. Anchor popovers to stable element ids and provide a fallback rect when first-frame placement matters.
- Use `.rounded_clip(radius)` or `.clip_to_radius()` for rounded shells whose children should be clipped to the same visible shape; this affects draw-list clips and hit testing, not only styling.
- Use engine screenshots through `FrameContext::request_screenshot` for visual checks. The serein examples expose `SKY_SEREIN_SCREENSHOT_PATH`, `SKY_SEREIN_SCREENSHOT_FRAME`, and `SKY_SEREIN_EXIT_AFTER_SCREENSHOT`.
- Keep Serein implementation plans in the Serein repository; SkyEngine planning documents must only cover the adapter and engine integration.

## Implementation Guidelines
- Keep backend-neutral lifecycle and capture logic in `core/`.
- Keep retained ECS UI behavior behind `ui-legacy`.
- Keep yakui-specific state and winit/wgpu adapters in `yakui/backend.rs`.
- Do not make legacy widgets depend on yakui, and do not make yakui depend on legacy widget state.
- Preserve input capture semantics when changing event routing: pointer and keyboard capture are independent.
- Avoid adding direct app-runner branches for each UI backend; route through `UiHost` where possible.
- Keep `egui` separate unless a deliberate migration plan says otherwise.

## Validation
- Run legacy UI tests/builds after retained UI changes:
  - `cargo test --features ui-legacy`
  - `cargo check --examples --features ui-legacy`
- Run Serein UI tests/builds after changing the adapter or the sibling crates:
  - `cargo test --manifest-path ../Serein/crates/serein/Cargo.toml`
  - `cargo test --manifest-path ../Serein/crates/serein-wgpu/Cargo.toml`
  - `cargo test --features ui-serein ui::serein`
  - `cargo check --examples --features ui-serein`
- Run yakui checks after yakui backend changes:
  - `cargo test --features yakui-ui`
  - `cargo check --examples --features yakui-ui`
- Run app checks after changing `FrameContext` UI methods or event routing:
  - `cargo test --features app`
  - `cargo check --examples --features app`
