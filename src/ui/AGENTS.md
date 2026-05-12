# AGENTS.md - `src/ui`

## Overview
- This module owns SkyEngine's current game UI integration behind UI feature flags.
- `ui-core` provides the backend-neutral host contract.
- `ui-legacy` provides the retained ECS UI implementation and adapts it into `UiHost`.
- `yakui-ui` installs an experimental yakui backend into the same `UiHost`.
- `egui` is not part of this module; it is a separate immediate-mode app overlay in `src/app/egui_integration.rs`.

## Feature Flags
- `ui-core`: compiles `UiHost`, `UiBackend`, event/capture/render contexts, and `FrameContext::ui()` facade methods.
- `ui-legacy`: enables retained ECS UI components, layout, input, state, text, renderer, and `LegacyUiBackend`.
- `ui`: compatibility alias for the current retained ECS UI (`ui-legacy`).
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
  - `install_ui`
  - `UiNode`, `UiPanel`, `UiButton`, `UiText`, `UiImage`, `UiProgressBar`, `UiSlider`, `UiToggle`, `UiScroll`
  - `UiState`, `UiEvents`, `UiTheme`, `UiConfig`, `UiFontBook`
- Yakui UI:
  - `YakuiUiPlugin`
  - `install_yakui_backend`
  - `YakuiBackend`

## File Map
- `mod.rs`: module wiring, feature-gated re-exports, legacy resource installation, and tests.
- `backend/traits.rs`: backend-neutral UI lifecycle traits and context types.
- `backend/host.rs`: `UiHost`, backend registration, event dispatch, frame updates, overlay rendering, capture aggregation.
- `backends/legacy.rs`: adapter that exposes the retained ECS UI as a `UiBackend`.
- `backends/yakui.rs`: experimental yakui backend and installer.
- `components.rs`: retained ECS UI components and authoring types (`ui-legacy`).
- `layout.rs`: retained UI layout, hit testing, rect maps, and world-layout resolution (`ui-legacy`).
- `input.rs`: retained UI interaction/update logic (`ui-legacy`).
- `state.rs`: retained UI runtime state, config, theme, and event queue (`ui-legacy`).
- `text.rs`: retained UI font discovery/bookkeeping (`ui-legacy`).
- `render.rs`: retained UI overlay renderer, image quads, glyphon text, and texture binding cache (`ui-legacy`).

## App Integration
- App window events can flow through `handle_ui_event(...)`; backends return `UiEventResponse::consumed()` when they consume an event.
- Per-frame UI backend updates flow through `FrameContext::ui().update()` or `FrameContext::update_ui_backends()`.
- Overlay rendering flows through `FrameContext::ui().render_overlays()` or `FrameContext::render_ui_overlays()`.
- Legacy-only direct calls `FrameContext::update_ui()` and `FrameContext::render_ui()` remain available under `ui-legacy`.
- UI capture is surfaced through `FrameContext::ui_wants_pointer()` / `ui_wants_keyboard()` and `UiCaptureState`.

## Current Rendering Model
- UI overlays currently render after scene rendering directly onto the active wgpu surface frame.
- The retained UI renderer in `render.rs` owns its own wgpu pipelines and glyphon renderer.
- `YakuiBackend` renders through `yakui_wgpu`.
- There is currently no canonical render-pipeline `UiPhase` or `UiFeature`.
- Do not document a future UI phase/feature as current behavior. If planning that migration, put it under `docs/plan/`.

## Implementation Guidelines
- Keep backend-neutral lifecycle and capture logic in `backend/`.
- Keep retained ECS UI behavior behind `ui-legacy`.
- Keep yakui-specific state and winit/wgpu adapters in `backends/yakui.rs`.
- Do not make legacy widgets depend on yakui, and do not make yakui depend on legacy widget state.
- Preserve input capture semantics when changing event routing: pointer and keyboard capture are independent.
- Avoid adding direct app-runner branches for each UI backend; route through `UiHost` where possible.
- Keep `egui` separate unless a deliberate migration plan says otherwise.

## Validation
- Run legacy UI tests/builds after retained UI changes:
  - `cargo test --features ui`
  - `cargo check --examples --features ui`
- Run yakui checks after yakui backend changes:
  - `cargo test --features yakui-ui`
  - `cargo check --examples --features yakui-ui`
- Run app checks after changing `FrameContext` UI methods or event routing:
  - `cargo test --features app`
  - `cargo check --examples --features app`
