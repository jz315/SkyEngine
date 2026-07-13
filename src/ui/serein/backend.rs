use std::any::Any;

use crate::asset::Assets;
use crate::input::Input;
use crate::ui::{
    UiBackend, UiBackendId, UiBeginFrameContext, UiCaptureState, UiError, UiEventContext,
    UiEventResponse, UiRenderContext,
};
use serein::{CursorShape, PlatformCaptureState, PlatformEffect};
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Ime, WindowEvent};
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, Window};

use super::config::SereinUiConfig;
use super::input_bridge::{
    keyboard_from_key_event, merge_keyboard_event, pointer_from_input, scroll_from_input,
};
use super::renderer::{SereinRenderResourceStatus, SereinRenderer};
use super::{FrameInput, KeyboardEvent, PointerEvent, Runtime, Screen, ScrollEvent, Ui};

/// Pluggable backend for the serein runtime.
#[derive(Debug)]
pub struct SereinUiBackend {
    runtime: Runtime,
    renderer: Option<SereinRenderer>,
    pending_input: SereinPendingInput,
    modifiers: ModifiersState,
    capture: UiCaptureState,
    screen: Screen,
    delta_seconds: f32,
    clipboard: SereinClipboard,
}

struct SereinClipboard(Option<arboard::Clipboard>);

impl std::fmt::Debug for SereinClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SereinClipboard")
            .field(&self.0.is_some())
            .finish()
    }
}

impl Default for SereinClipboard {
    fn default() -> Self {
        Self(arboard::Clipboard::new().ok())
    }
}

impl SereinClipboard {
    fn set_text(&mut self, text: String) {
        if let Some(clipboard) = self.0.as_mut() {
            let _ = clipboard.set_text(text);
        }
    }
}

#[derive(Debug, Default)]
struct SereinPendingInput {
    pointer: PointerEvent,
    pointer_events: Vec<PointerEvent>,
    scroll: ScrollEvent,
    keyboard: KeyboardEvent,
    event_pointer_position: Option<[f32; 2]>,
    event_left_down: bool,
    event_right_down: bool,
}

impl SereinPendingInput {
    fn begin_frame(&mut self, input: &Input) {
        let pointer = pointer_from_input(input);
        self.pointer = pointer;
        if self.pointer_events.is_empty() {
            self.event_pointer_position = pointer.position();
            self.event_left_down = pointer.down;
            self.event_right_down = pointer.right_down;
        }
        self.scroll = scroll_from_input(input);
    }

    fn drain_frame_input(&mut self, screen: Screen, delta_seconds: f32) -> FrameInput {
        let input = FrameInput::new(screen, delta_seconds)
            .pointer(self.pointer)
            .pointer_events(std::mem::take(&mut self.pointer_events))
            .scroll(self.scroll)
            .keyboard(std::mem::take(&mut self.keyboard));
        self.scroll = ScrollEvent::default();
        input
    }
}

impl SereinUiBackend {
    pub const ID: UiBackendId = UiBackendId::new("serein");

    pub fn new(config: SereinUiConfig) -> Self {
        Self {
            runtime: Runtime::new(config.page_id),
            renderer: None,
            pending_input: SereinPendingInput::default(),
            modifiers: ModifiersState::default(),
            capture: UiCaptureState::default(),
            screen: Screen::default(),
            delta_seconds: 0.0,
            clipboard: SereinClipboard::default(),
        }
    }

    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut Runtime {
        &mut self.runtime
    }

    pub fn begin_frame_snapshot(
        &mut self,
        input: &Input,
        logical_surface_size: [f32; 2],
        delta_seconds: f32,
    ) {
        self.screen = Screen {
            width: logical_surface_size[0],
            height: logical_surface_size[1],
        };
        self.delta_seconds = delta_seconds.max(0.0);
        self.pending_input.begin_frame(input);
        // Keyboard/text input is sourced from winit events so focused widgets do
        // not receive Backspace/Enter/arrows twice through the raw Input mirror.
        self.capture = ui_capture_from_platform(self.runtime.platform_capture_state());
    }

    pub fn frame(&mut self, build: impl FnOnce(&mut Ui, Screen)) {
        let input = self.frame_input();
        self.runtime.frame(input, build);
        self.capture = ui_capture_from_platform(self.runtime.platform_capture_state());
    }

    pub(crate) fn apply_platform_effects(&mut self, window: Option<&Window>) {
        apply_window_platform_effects(
            &mut self.runtime,
            window,
            &mut self.capture,
            &mut self.clipboard,
        );
    }

    fn frame_input(&mut self) -> FrameInput {
        self.pending_input
            .drain_frame_input(self.screen, self.delta_seconds)
    }
}

impl UiBackend for SereinUiBackend {
    fn id(&self) -> UiBackendId {
        Self::ID
    }

    fn name(&self) -> &'static str {
        "serein"
    }

    fn handle_event(&mut self, ctx: UiEventContext<'_>) -> UiEventResponse {
        apply_window_platform_effects(
            &mut self.runtime,
            ctx.window,
            &mut self.capture,
            &mut self.clipboard,
        );
        match ctx.event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                UiEventResponse::ignored()
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    merge_keyboard_event(
                        &mut self.pending_input.keyboard,
                        keyboard_from_key_event(event, self.modifiers),
                    );
                }
                if self.runtime.has_keyboard_capture() {
                    UiEventResponse::consumed()
                } else {
                    UiEventResponse::ignored()
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.pending_input.keyboard.text.push_str(text);
                if self.runtime.has_keyboard_capture() {
                    UiEventResponse::consumed()
                } else {
                    UiEventResponse::ignored()
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let next = physical_cursor_to_logical(*position, ctx.scale_factor);
                let previous = self.pending_input.event_pointer_position;
                let delta = previous
                    .map(|previous| [next[0] - previous[0], next[1] - previous[1]])
                    .unwrap_or([0.0, 0.0]);
                self.pending_input.event_pointer_position = Some(next);
                self.pending_input.pointer_events.push(PointerEvent {
                    x: next[0],
                    y: next[1],
                    delta_x: delta[0],
                    delta_y: delta[1],
                    position: Some(next),
                    delta,
                    down: self.pending_input.event_left_down,
                    right_down: self.pending_input.event_right_down,
                    ..PointerEvent::default()
                });
                UiEventResponse::ignored()
            }
            WindowEvent::CursorLeft { .. } => {
                self.pending_input.event_pointer_position = None;
                self.pending_input
                    .pointer_events
                    .push(PointerEvent::default());
                UiEventResponse::ignored()
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(position) = self.pending_input.event_pointer_position else {
                    return UiEventResponse::ignored();
                };
                let mut event = PointerEvent {
                    x: position[0],
                    y: position[1],
                    position: Some(position),
                    down: self.pending_input.event_left_down,
                    right_down: self.pending_input.event_right_down,
                    ..PointerEvent::default()
                };
                match button {
                    winit::event::MouseButton::Left => match state {
                        ElementState::Pressed => {
                            self.pending_input.event_left_down = true;
                            event.down = true;
                            event.pressed_this_frame = true;
                        }
                        ElementState::Released => {
                            self.pending_input.event_left_down = false;
                            event.down = false;
                            event.released_this_frame = true;
                        }
                    },
                    winit::event::MouseButton::Right => match state {
                        ElementState::Pressed => {
                            self.pending_input.event_right_down = true;
                            event.right_down = true;
                            event.right_pressed_this_frame = true;
                        }
                        ElementState::Released => {
                            self.pending_input.event_right_down = false;
                            event.right_down = false;
                            event.right_released_this_frame = true;
                        }
                    },
                    _ => return UiEventResponse::ignored(),
                }
                event.down = self.pending_input.event_left_down;
                event.right_down = self.pending_input.event_right_down;
                self.pending_input.pointer_events.push(event);
                UiEventResponse::ignored()
            }
            _ => UiEventResponse::ignored(),
        }
    }

    fn begin_frame(&mut self, ctx: UiBeginFrameContext<'_>) {
        self.begin_frame_snapshot(
            ctx.input,
            ctx.logical_surface_size,
            ctx.world.time.frame_delta(),
        );
        apply_window_platform_effects(
            &mut self.runtime,
            ctx.window,
            &mut self.capture,
            &mut self.clipboard,
        );
    }

    fn render_overlay(&mut self, ctx: UiRenderContext<'_>) -> Result<(), UiError> {
        if ctx.gpu.has_surface() && ctx.gpu.has_active_frame() {
            #[cfg(feature = "profile")]
            let _overlay_scope = sky_profile::profile_scope!("ui_serein", "render_overlay");
            if self.renderer.is_none()
                || !self
                    .renderer
                    .as_ref()
                    .is_some_and(|renderer| renderer.matches_surface(ctx.gpu.surface_format()))
            {
                #[cfg(feature = "profile")]
                let _renderer_create_scope =
                    sky_profile::profile_scope!("ui_serein", "create_renderer");
                let renderer = SereinRenderer::new(ctx.gpu);
                self.renderer = Some(renderer);
            }
            let asset_server = ctx.world.get_resource::<Assets>().cloned();
            let pending_resources = self.renderer.as_mut().is_some_and(|renderer| {
                #[cfg(feature = "profile")]
                let _render_scope = sky_profile::profile_scope!("ui_serein", "renderer_render");
                let status = renderer.render(
                    ctx.gpu,
                    &mut self.runtime,
                    asset_server.as_ref(),
                    ctx.render_assets,
                );
                record_render_resource_status(&mut self.runtime, status)
            });
            if pending_resources {
                return Ok(());
            }
        }
        self.runtime.mark_rendered();
        Ok(())
    }

    fn capture(&self) -> UiCaptureState {
        self.capture
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn ui_capture_from_platform(state: PlatformCaptureState) -> UiCaptureState {
    UiCaptureState::new(state.wants_pointer, state.wants_keyboard)
}

fn apply_window_platform_effects(
    runtime: &mut Runtime,
    window: Option<&Window>,
    capture: &mut UiCaptureState,
    clipboard: &mut SereinClipboard,
) {
    for effect in runtime.take_platform_effects() {
        match effect {
            PlatformEffect::ImeStart { rect } | PlatformEffect::ImeMove { rect } => {
                if let Some(window) = window {
                    window.set_ime_allowed(true);
                    window.set_ime_cursor_area(
                        LogicalPosition::new(rect.x as f64, rect.y as f64),
                        LogicalSize::new(rect.width.max(1.0) as f64, rect.height.max(1.0) as f64),
                    );
                }
            }
            PlatformEffect::ImeEnd => {
                if let Some(window) = window {
                    window.set_ime_allowed(false);
                }
            }
            PlatformEffect::CursorShape { shape } => {
                if let Some(window) = window {
                    window.set_cursor(cursor_icon_for(shape));
                }
            }
            PlatformEffect::Capture { state } => {
                *capture = ui_capture_from_platform(state);
            }
            PlatformEffect::ClipboardWrite { text } => clipboard.set_text(text),
        }
    }
}

fn cursor_icon_for(shape: CursorShape) -> CursorIcon {
    match shape {
        CursorShape::Arrow => CursorIcon::Default,
        CursorShape::Hand => CursorIcon::Pointer,
    }
}

fn record_render_resource_status(
    runtime: &mut Runtime,
    status: SereinRenderResourceStatus,
) -> bool {
    for dirty in status.resource_dirty() {
        runtime.request_resource_dirty(dirty);
    }
    status.has_pending_resources()
}

fn physical_cursor_to_logical(
    position: winit::dpi::PhysicalPosition<f64>,
    scale_factor: f32,
) -> [f32; 2] {
    let scale = scale_factor.max(0.0001) as f64;
    [(position.x / scale) as f32, (position.y / scale) as f32]
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::super::renderer::SereinRenderStatus;
    use super::*;
    #[test]
    fn backend_dispatches_pending_keyboard_before_next_frame() {
        let text = Rc::new(RefCell::new(String::new()));
        let observed_during_frame = Rc::new(RefCell::new(String::new()));
        let mut backend = SereinUiBackend::new(SereinUiConfig {
            page_id: "page".to_string(),
        });
        backend.screen = Screen {
            width: 100.0,
            height: 50.0,
        };

        let text_for_callback = text.clone();
        backend.frame(move |ui, _| {
            let text_for_callback = text_for_callback.clone();
            ui.rect("input")
                .size(80.0, 24.0)
                .on_text_input(move |event| {
                    text_for_callback.borrow_mut().push_str(&event.text);
                })
                .build();
        });
        backend.runtime.dispatch_frame_input(
            FrameInput::new(backend.screen, 0.0).pointer(PointerEvent::pressed_at(8.0, 8.0)),
        );
        backend.pending_input.keyboard = KeyboardEvent {
            text: "A".to_string(),
            ..KeyboardEvent::default()
        };

        let text_for_frame = text.clone();
        let observed = observed_during_frame.clone();
        backend.frame(move |ui, _| {
            observed
                .borrow_mut()
                .push_str(text_for_frame.borrow().as_str());
            let text_for_callback = text_for_frame.clone();
            ui.rect("input")
                .size(80.0, 24.0)
                .on_text_input(move |event| {
                    text_for_callback.borrow_mut().push_str(&event.text);
                })
                .build();
        });

        assert_eq!(text.borrow().as_str(), "A");
        assert_eq!(observed_during_frame.borrow().as_str(), "A");
        assert!(backend.pending_input.keyboard.text.is_empty());
    }

    #[test]
    fn backend_dispatches_queued_pointer_clicks_in_order_before_frame() {
        let clicks = Rc::new(RefCell::new(Vec::new()));
        let mut backend = SereinUiBackend::new(SereinUiConfig {
            page_id: "page".to_string(),
        });
        backend.screen = Screen {
            width: 160.0,
            height: 80.0,
        };

        let first_clicks = clicks.clone();
        let second_clicks = clicks.clone();
        backend.frame(move |ui, _| {
            let first_clicks = first_clicks.clone();
            ui.rect("first")
                .position(0.0, 0.0)
                .size(60.0, 30.0)
                .on_click(move || first_clicks.borrow_mut().push(1))
                .build();

            let second_clicks = second_clicks.clone();
            ui.rect("second")
                .position(0.0, 40.0)
                .size(60.0, 30.0)
                .on_click(move || second_clicks.borrow_mut().push(2))
                .build();
        });

        backend
            .pending_input
            .pointer_events
            .push(PointerEvent::pressed_at(10.0, 10.0));
        backend
            .pending_input
            .pointer_events
            .push(PointerEvent::released_at(10.0, 10.0));
        backend
            .pending_input
            .pointer_events
            .push(PointerEvent::pressed_at(10.0, 50.0));
        backend
            .pending_input
            .pointer_events
            .push(PointerEvent::released_at(10.0, 50.0));

        backend.frame(|ui, _| {
            ui.rect("first").position(0.0, 0.0).size(60.0, 30.0).build();
            ui.rect("second")
                .position(0.0, 40.0)
                .size(60.0, 30.0)
                .build();
        });

        assert_eq!(&*clicks.borrow(), &[1, 2]);
    }

    #[test]
    fn backend_incremental_frame_drains_signal_dirty_records_after_event_callbacks() {
        #[derive(Default)]
        struct Model {
            page: i32,
        }

        let state = serein::State::new(Model::default());
        let builds = Rc::new(Cell::new(0));
        let mut backend = SereinUiBackend::new(SereinUiConfig {
            page_id: "page".to_string(),
        });
        backend.screen = Screen {
            width: 160.0,
            height: 80.0,
        };

        let run_nav_frame = |backend: &mut SereinUiBackend| {
            let frame_state = state.clone();
            let builds = builds.clone();
            backend.frame(move |ui, _| {
                ui.column("nav").size(160.0, 80.0).content(|ui| {
                    builds.set(builds.get() + 1);
                    let page = frame_state.signal(
                        "page",
                        |model| model.page,
                        |model, value| model.page = value,
                    );
                    let selected = page.watch(ui);
                    serein::widgets::button(ui, "nav.button")
                        .size(120.0, 40.0)
                        .text(format!("Page {selected}"))
                        .on_click(move || page.set(1))
                        .build();
                });
            });
        };

        run_nav_frame(&mut backend);
        assert_eq!(builds.get(), 1);
        let frame = backend
            .runtime
            .diagnostics()
            .find("nav.button.bg")
            .unwrap()
            .frame;
        backend
            .pending_input
            .pointer_events
            .push(PointerEvent::pressed_at(frame.x + 1.0, frame.y + 1.0));
        backend
            .pending_input
            .pointer_events
            .push(PointerEvent::released_at(frame.x + 1.0, frame.y + 1.0));

        run_nav_frame(&mut backend);

        assert_eq!(state.read(|model| model.page), 1);
        assert_eq!(builds.get(), 2);
        assert_eq!(
            backend
                .runtime
                .diagnostics()
                .find("nav.button.text")
                .unwrap()
                .text,
            "Page 1"
        );
    }

    #[test]
    fn backend_incremental_frame_flushes_real_window_click_before_dirty_records() {
        #[derive(Default)]
        struct Model {
            page: i32,
        }

        let state = serein::State::new(Model::default());
        let mut backend = SereinUiBackend::new(SereinUiConfig {
            page_id: "page".to_string(),
        });
        backend.screen = Screen {
            width: 240.0,
            height: 80.0,
        };

        let run_nav_frame = |backend: &mut SereinUiBackend| {
            let frame_state = state.clone();
            backend.frame(move |ui, _| {
                ui.column("nav").size(240.0, 80.0).content(|ui| {
                    let page = frame_state.signal(
                        "page",
                        |model| model.page,
                        |model, value| model.page = value,
                    );
                    let selected = page.watch(ui);
                    serein::widgets::tabs(ui, "tabs")
                        .size(180.0, 40.0)
                        .items(["One", "Two", "Three"])
                        .selected(selected)
                        .on_change(move |next| page.set(next))
                        .build();
                });
            });
        };

        run_nav_frame(&mut backend);
        let label = backend
            .runtime
            .diagnostics()
            .find("tabs.label.1")
            .unwrap()
            .frame;
        let point = [label.x + label.width * 0.5, label.y + label.height * 0.5];

        backend.pending_input.event_pointer_position = Some(point);
        backend
            .pending_input
            .pointer_events
            .push(PointerEvent::pressed_at(point[0], point[1]));
        backend
            .pending_input
            .pointer_events
            .push(PointerEvent::released_at(point[0], point[1]));

        run_nav_frame(&mut backend);

        assert_eq!(state.read(|model| model.page), 1);
        assert_eq!(
            backend
                .runtime
                .diagnostics()
                .find("tabs.indicator")
                .unwrap()
                .frame
                .x,
            backend
                .runtime
                .diagnostics()
                .find("tabs.hit.1")
                .unwrap()
                .frame
                .x
                + 10.0
        );
    }

    #[test]
    fn begin_frame_snapshot_preserves_queued_window_pointer_click() {
        #[derive(Default)]
        struct Model {
            page: i32,
        }

        let state = serein::State::new(Model::default());
        let mut backend = SereinUiBackend::new(SereinUiConfig {
            page_id: "page".to_string(),
        });
        backend.screen = Screen {
            width: 240.0,
            height: 80.0,
        };

        let run_nav_frame = |backend: &mut SereinUiBackend| {
            let frame_state = state.clone();
            backend.frame(move |ui, _| {
                ui.column("nav").size(240.0, 80.0).content(|ui| {
                    let page = frame_state.signal(
                        "page",
                        |model| model.page,
                        |model, value| model.page = value,
                    );
                    let selected = page.watch(ui);
                    serein::widgets::tabs(ui, "tabs")
                        .size(180.0, 40.0)
                        .items(["One", "Two", "Three"])
                        .selected(selected)
                        .on_change(move |next| page.set(next))
                        .build();
                });
            });
        };

        run_nav_frame(&mut backend);
        let label = backend
            .runtime
            .diagnostics()
            .find("tabs.label.1")
            .unwrap()
            .frame;
        let point = [label.x + label.width * 0.5, label.y + label.height * 0.5];

        backend.pending_input.event_pointer_position = Some(point);
        backend.pending_input.pointer_events.push(PointerEvent {
            x: point[0],
            y: point[1],
            position: Some(point),
            down: true,
            pressed_this_frame: true,
            ..PointerEvent::default()
        });
        backend.pending_input.event_left_down = true;
        backend.pending_input.pointer_events.push(PointerEvent {
            x: point[0],
            y: point[1],
            position: Some(point),
            released_this_frame: true,
            ..PointerEvent::default()
        });
        backend.pending_input.event_left_down = false;

        let input = Input::new();
        backend.begin_frame_snapshot(&input, [240.0, 80.0], 1.0 / 60.0);
        run_nav_frame(&mut backend);

        assert_eq!(state.read(|model| model.page), 1);
        assert_eq!(backend.pending_input.event_pointer_position, Some(point));
    }

    #[test]
    fn backend_incremental_frame_rebuilds_fully_after_untracked_click_callback() {
        let page = Rc::new(Cell::new(0));
        let builds = Rc::new(Cell::new(0));
        let mut backend = SereinUiBackend::new(SereinUiConfig {
            page_id: "page".to_string(),
        });
        backend.screen = Screen {
            width: 160.0,
            height: 80.0,
        };

        let run_button_frame = |backend: &mut SereinUiBackend| {
            let page_for_frame = page.clone();
            let page_for_click = page.clone();
            let builds = builds.clone();
            backend.frame(move |ui, _| {
                ui.column("panel").size(160.0, 80.0).content(|ui| {
                    builds.set(builds.get() + 1);
                    let selected = page_for_frame.get();
                    let page_for_click = page_for_click.clone();
                    serein::widgets::button(ui, "panel.button")
                        .size(120.0, 40.0)
                        .text(format!("Page {selected}"))
                        .on_click(move || page_for_click.set(1))
                        .build();
                });
            });
        };

        run_button_frame(&mut backend);
        assert_eq!(builds.get(), 1);
        let frame = backend
            .runtime
            .diagnostics()
            .find("panel.button.bg")
            .unwrap()
            .frame;
        let point = [frame.x + frame.width * 0.5, frame.y + frame.height * 0.5];
        backend
            .pending_input
            .pointer_events
            .push(PointerEvent::pressed_at(point[0], point[1]));
        backend
            .pending_input
            .pointer_events
            .push(PointerEvent::released_at(point[0], point[1]));

        run_button_frame(&mut backend);

        assert_eq!(page.get(), 1);
        assert_eq!(builds.get(), 2);
        assert_eq!(
            backend
                .runtime
                .diagnostics()
                .committed_snapshot()
                .layout_mode,
            serein::expert::LayoutMode::Full(
                serein::expert::FullLayoutReason::RetainedReuseUnavailable
            )
        );
        assert_eq!(
            backend
                .runtime
                .diagnostics()
                .find("panel.button.text")
                .unwrap()
                .text,
            "Page 1"
        );
    }

    #[test]
    fn renderer_resources_request_precise_dirty() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();

        let pending = record_render_resource_status(
            &mut runtime,
            SereinRenderResourceStatus {
                render: SereinRenderStatus {
                    pending_images: true,
                    pending_fonts: true,
                    ..SereinRenderStatus::default()
                },
                ready_images: true,
                ready_fonts: true,
            },
        );

        assert!(pending);
        assert!(runtime.needs_render());
        assert!(runtime.needs_compose());
        assert!(!runtime.full_redraw());
        let snapshot = runtime.diagnostics().current_snapshot();
        let draw_sources = [
            serein::RendererResourceDirty::PendingImages,
            serein::RendererResourceDirty::PendingFonts,
            serein::RendererResourceDirty::ReadyImages,
        ];
        for source in draw_sources {
            let invalidation = snapshot
                .invalidations
                .iter()
                .find(|record| match &record.source {
                    serein::expert::InvalidationSource::Resource(source_id) => {
                        source_id.renderer_kind() == Some(source)
                    }
                    _ => false,
                })
                .unwrap_or_else(|| panic!("missing resource invalidation for {}", source.label()));
            assert_eq!(invalidation.source.kind(), "resource");
            assert_eq!(invalidation.source.label(), source.label());
            assert_eq!(invalidation.flags, serein::DirtyFlags::DRAW);
            assert!(invalidation.pass_flags.request_draw);
            assert!(!invalidation.pass_flags.request_compose_ui);
            assert!(!invalidation.pass_flags.request_layout);
        }
        let font_ready = snapshot
            .invalidations
            .iter()
            .find(|record| match &record.source {
                serein::expert::InvalidationSource::Resource(source_id) => {
                    source_id.renderer_kind() == Some(serein::RendererResourceDirty::ReadyFonts)
                }
                _ => false,
            })
            .expect("missing ready font resource invalidation");
        assert_eq!(font_ready.source.kind(), "resource");
        assert_eq!(
            font_ready.source.label(),
            serein::RendererResourceDirty::ReadyFonts.label()
        );
        assert_eq!(
            font_ready.flags,
            serein::DirtyFlags::COMPOSE | serein::DirtyFlags::LAYOUT | serein::DirtyFlags::DRAW
        );
        assert!(font_ready.pass_flags.request_compose_ui);
        assert!(font_ready.pass_flags.request_layout);
        assert!(font_ready.pass_flags.request_hit);
        assert!(font_ready.pass_flags.request_draw);
    }
}
