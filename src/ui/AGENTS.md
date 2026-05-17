# AGENTS.md - `src/ui`

## Overview
- This module owns SkyEngine's current game UI integration behind UI feature flags.
- `ui-core` provides the backend-neutral host contract.
- `ui-legacy` provides the retained ECS UI implementation and adapts it into `UiHost`.
- `ui-neo` provides the experimental EUI-NEO-style declarative runtime and adapts it into `UiHost`.
- `yakui-ui` installs an experimental yakui backend into the same `UiHost`.
- `egui` is not part of this module; it is a separate immediate-mode app overlay in `src/app/egui_integration.rs`.

## Feature Flags
- `ui-core`: compiles `UiHost`, `UiBackend`, event/capture/render contexts, and `FrameContext::ui()` facade methods.
- `ui-legacy`: enables retained ECS UI components, layout, input, state, text, renderer, and `LegacyUiBackend`.
- `ui`: compatibility alias for the current retained ECS UI (`ui-legacy`).
- `ui-neo`: enables `sky_engine::ui::neo`, `NeoUiBackend`, `NeoUiPlugin`, EUI-NEO-style widgets, glyphon text, HTTP/Bing image loading, and clipboard support.
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
- Neo UI:
  - `neo::NeoUiPlugin`
  - `neo::install_neo_ui_backend`
  - `neo::NeoUiBackend`
  - `neo::NeoRuntime`
  - `neo::Ui`, `neo::NeoState`, `neo::Binding`, and `neo::widgets`
  - `neo::compose`
  - `neo::open_window`

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
- `neo/mod.rs`: neo module wiring and re-exports.
- `neo/config.rs`: `NeoUiConfig` and `NeoWindowConfig`.
- `neo/api.rs`: `compose(...)` and `open_window(...)`.
- `neo/plugin.rs`: `NeoUiPlugin` and backend installation.
- `neo/backend.rs`: `NeoUiBackend` and neo-specific capture/IME/render integration.
- `neo/window.rs`: auxiliary native window client for neo-driven windows.
- `neo/input_bridge.rs`: raw input and winit keyboard/IME translation for neo.
- `neo/`: experimental EUI-NEO-style DSL, layout, event, runtime, animation, draw list, renderer, state binding, and widget port.
- `neo/widgets/`: EUI-NEO component ports including button, input, dialog, dropdown, context menu, toast, date/time/color pickers, data table, and charts.

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
- `NeoUiBackend` renders through `neo::NeoRenderer`, a wgpu overlay renderer that uses glyphon for text and SkyEngine app/platform seams for screenshots, URLs, clipboard, and image loading.
- There is currently no canonical render-pipeline `UiPhase` or `UiFeature`.
- Do not document a future UI phase/feature as current behavior. If planning that migration, put it under `docs/plan/`.

## Neo UI Rules
- Reference repository for behavior parity is `C:\Coding\EUI-NEO`. Check the local source before changing `src/ui/neo/` behavior.
- Keep widget behavior traceable to EUI-NEO `components/*.h` and runtime/layout/animation behavior traceable to `core/*.h`.
- Preserve EUI-NEO callback ordering, clamp rules, z-index/layering, modal hit blocking, focus, keyboard, clipboard, IME rect, dirty/redraw, and animation semantics unless there is a documented SkyEngine platform adaptation.
- Keep reusable behavior in `src/ui/neo/` or `src/ui/neo/widgets/`; examples should demonstrate parity and should not contain hidden widget implementations.
- Use engine screenshots through `FrameContext::request_screenshot` for visual checks. The neo examples expose `SKY_NEO_SCREENSHOT_PATH`, `SKY_NEO_SCREENSHOT_FRAME`, and `SKY_NEO_EXIT_AFTER_SCREENSHOT`.
- Keep active EUI-NEO port tracking in `docs/plan/eui_neo_rust_ui_port_plan.md` only; do not create scattered parity TODO files.

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
  - `cargo test --features ui`
  - `cargo check --examples --features ui`
- Run neo UI tests/builds after EUI-NEO-style UI changes:
  - `cargo test --features ui-neo ui::neo`
  - `cargo check --examples --features ui-neo`
- Run yakui checks after yakui backend changes:
  - `cargo test --features yakui-ui`
  - `cargo check --examples --features yakui-ui`
- Run app checks after changing `FrameContext` UI methods or event routing:
  - `cargo test --features app`
  - `cargo check --examples --features app`
