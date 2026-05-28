use std::any::Any;

use crate::asset::Assets;
use crate::input::Input;
use crate::ui::{
    UiBackend, UiBackendId, UiBeginFrameContext, UiCaptureState, UiError, UiEventContext,
    UiEventResponse, UiRenderContext,
};
use eui_neo::Element;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Ime, WindowEvent};
use winit::keyboard::ModifiersState;
use winit::window::Window;

use super::config::NeoUiConfig;
use super::input_bridge::{
    keyboard_from_key_event, merge_keyboard_event, pointer_from_input, scroll_from_input,
};
use super::renderer::NeoRenderer;
use super::{KeyboardEvent, LayoutRect, PointerEvent, Runtime, Screen, ScrollEvent, Ui};

/// Pluggable backend for the neo runtime.
#[derive(Debug)]
pub struct NeoUiBackend {
    runtime: Runtime,
    renderer: Option<NeoRenderer>,
    pending_pointer: PointerEvent,
    pending_pointer_events: Vec<PointerEvent>,
    pending_scroll: ScrollEvent,
    pending_keyboard: KeyboardEvent,
    event_pointer_position: Option<[f32; 2]>,
    event_left_down: bool,
    event_right_down: bool,
    modifiers: ModifiersState,
    capture: UiCaptureState,
    screen: Screen,
    delta_seconds: f32,
}

impl NeoUiBackend {
    pub const ID: UiBackendId = UiBackendId::new("neo");

    pub fn new(config: NeoUiConfig) -> Self {
        Self {
            runtime: Runtime::new(config.page_id),
            renderer: None,
            pending_pointer: PointerEvent::default(),
            pending_pointer_events: Vec::new(),
            pending_scroll: ScrollEvent::default(),
            pending_keyboard: KeyboardEvent::default(),
            event_pointer_position: None,
            event_left_down: false,
            event_right_down: false,
            modifiers: ModifiersState::default(),
            capture: UiCaptureState::default(),
            screen: Screen::default(),
            delta_seconds: 0.0,
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
        self.pending_pointer = pointer_from_input(input);
        self.event_pointer_position = self.pending_pointer.position();
        self.event_left_down = self.pending_pointer.down;
        self.event_right_down = self.pending_pointer.right_down;
        self.pending_scroll = scroll_from_input(input);
        // Keyboard/text input is sourced from winit events so focused widgets do
        // not receive Backspace/Enter/arrows twice through the raw Input mirror.
        self.refresh_capture();
    }

    pub fn compose(&mut self, compose: impl FnOnce(&mut Ui, Screen)) {
        let screen = self.screen;
        self.update_pending_events();
        self.runtime.compose(screen.width, screen.height, compose);
        self.runtime.tick_animations(self.delta_seconds);
        self.refresh_capture();
    }

    pub fn compose_scoped(
        &mut self,
        dirty_scopes: impl FnOnce() -> Vec<String>,
        compose: impl FnOnce(&mut Ui, Screen),
    ) {
        let screen = self.screen;
        self.update_pending_events();
        let dirty_scopes = dirty_scopes();
        self.runtime
            .compose_scoped(screen.width, screen.height, dirty_scopes, compose);
        self.runtime.tick_animations(self.delta_seconds);
        self.refresh_capture();
    }

    fn update_pending_events(&mut self) {
        let pointer_events = std::mem::take(&mut self.pending_pointer_events);
        let keyboard = std::mem::take(&mut self.pending_keyboard);
        if let Some((last, leading)) = pointer_events.split_last() {
            for event in leading {
                self.runtime.update_pointer(*event);
            }
            self.runtime.update_events_and_timers(
                *last,
                self.pending_scroll,
                keyboard,
                self.delta_seconds,
            );
        } else {
            self.runtime.update_events_and_timers(
                self.pending_pointer,
                self.pending_scroll,
                keyboard,
                self.delta_seconds,
            );
        }
        self.pending_scroll = ScrollEvent::default();
    }

    fn refresh_capture(&mut self) {
        let screen_rect = LayoutRect::new(0.0, 0.0, self.screen.width, self.screen.height);
        let wants_pointer = self.runtime.roots().iter().any(|root| {
            tree_has_hover_or_active(root, &self.runtime)
                || tree_has_fullscreen_interactive(root, screen_rect)
        });
        let wants_keyboard = self.runtime.focused_id().is_some();
        self.capture = UiCaptureState::new(wants_pointer, wants_keyboard);
    }

    fn sync_window_ime(&self, window: Option<&Window>) {
        let Some(window) = window else {
            return;
        };
        let Some(rect) = self.runtime.focused_ime_rect() else {
            window.set_ime_allowed(false);
            return;
        };
        window.set_ime_allowed(true);
        window.set_ime_cursor_area(
            LogicalPosition::new(rect.x as f64, rect.y as f64),
            LogicalSize::new(rect.width.max(1.0) as f64, rect.height.max(1.0) as f64),
        );
    }
}

impl UiBackend for NeoUiBackend {
    fn id(&self) -> UiBackendId {
        Self::ID
    }

    fn name(&self) -> &'static str {
        "neo"
    }

    fn handle_event(&mut self, ctx: UiEventContext<'_>) -> UiEventResponse {
        self.sync_window_ime(ctx.window);
        match ctx.event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                UiEventResponse::ignored()
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    merge_keyboard_event(
                        &mut self.pending_keyboard,
                        keyboard_from_key_event(event, self.modifiers),
                    );
                }
                if self.runtime.focused_id().is_some() {
                    UiEventResponse::consumed()
                } else {
                    UiEventResponse::ignored()
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.pending_keyboard.text.push_str(text);
                if self.runtime.focused_id().is_some() {
                    UiEventResponse::consumed()
                } else {
                    UiEventResponse::ignored()
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let next = physical_cursor_to_logical(*position, ctx.scale_factor);
                let previous = self.event_pointer_position;
                let delta = previous
                    .map(|previous| [next[0] - previous[0], next[1] - previous[1]])
                    .unwrap_or([0.0, 0.0]);
                self.event_pointer_position = Some(next);
                self.pending_pointer_events.push(PointerEvent {
                    x: next[0],
                    y: next[1],
                    delta_x: delta[0],
                    delta_y: delta[1],
                    position: Some(next),
                    delta,
                    down: self.event_left_down,
                    right_down: self.event_right_down,
                    ..PointerEvent::default()
                });
                UiEventResponse::ignored()
            }
            WindowEvent::CursorLeft { .. } => {
                self.event_pointer_position = None;
                self.pending_pointer_events.push(PointerEvent::default());
                UiEventResponse::ignored()
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(position) = self.event_pointer_position else {
                    return UiEventResponse::ignored();
                };
                let mut event = PointerEvent {
                    x: position[0],
                    y: position[1],
                    position: Some(position),
                    down: self.event_left_down,
                    right_down: self.event_right_down,
                    ..PointerEvent::default()
                };
                match button {
                    winit::event::MouseButton::Left => match state {
                        ElementState::Pressed => {
                            self.event_left_down = true;
                            event.down = true;
                            event.pressed_this_frame = true;
                        }
                        ElementState::Released => {
                            self.event_left_down = false;
                            event.down = false;
                            event.released_this_frame = true;
                        }
                    },
                    winit::event::MouseButton::Right => match state {
                        ElementState::Pressed => {
                            self.event_right_down = true;
                            event.right_down = true;
                            event.right_pressed_this_frame = true;
                        }
                        ElementState::Released => {
                            self.event_right_down = false;
                            event.right_down = false;
                            event.right_released_this_frame = true;
                        }
                    },
                    _ => return UiEventResponse::ignored(),
                }
                event.down = self.event_left_down;
                event.right_down = self.event_right_down;
                self.pending_pointer_events.push(event);
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
        self.sync_window_ime(ctx.window);
    }

    fn render_overlay(&mut self, ctx: UiRenderContext<'_>) -> Result<(), UiError> {
        if ctx.gpu.has_surface() && ctx.gpu.has_active_frame() {
            if self.renderer.is_none()
                || !self
                    .renderer
                    .as_ref()
                    .is_some_and(|renderer| renderer.matches_surface(ctx.gpu.surface_format()))
            {
                self.renderer = Some(NeoRenderer::new(ctx.gpu));
            }
            let asset_server = ctx.world.get_resource::<Assets>().cloned();
            let pending_resources = self.renderer.as_mut().is_some_and(|renderer| {
                let status = renderer.render(
                    ctx.gpu,
                    &mut self.runtime,
                    asset_server.as_ref(),
                    ctx.render_assets,
                );
                status.pending_images || status.pending_fonts
            });
            if pending_resources {
                self.runtime.mark_full_redraw();
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

fn tree_has_hover_or_active(element: &Element, runtime: &Runtime) -> bool {
    let state = runtime.interaction(&element.id);
    state.hovered
        || state.active
        || element
            .children
            .iter()
            .any(|child| tree_has_hover_or_active(child, runtime))
}

fn tree_has_fullscreen_interactive(element: &Element, screen: LayoutRect) -> bool {
    (element.interactive && !element.disabled && rect_covers(element.frame, screen))
        || element
            .children
            .iter()
            .any(|child| tree_has_fullscreen_interactive(child, screen))
}

fn rect_covers(rect: LayoutRect, screen: LayoutRect) -> bool {
    const EPSILON: f32 = 0.5;
    rect.x <= screen.x + EPSILON
        && rect.y <= screen.y + EPSILON
        && rect.right() >= screen.right() - EPSILON
        && rect.bottom() >= screen.bottom() - EPSILON
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

    use super::*;

    #[test]
    fn backend_dispatches_pending_keyboard_before_next_compose() {
        let text = Rc::new(RefCell::new(String::new()));
        let observed_during_compose = Rc::new(RefCell::new(String::new()));
        let mut backend = NeoUiBackend::new(NeoUiConfig {
            page_id: "page".to_string(),
        });
        backend.screen = Screen {
            width: 100.0,
            height: 50.0,
        };

        let text_for_callback = text.clone();
        backend.compose(move |ui, _| {
            let text_for_callback = text_for_callback.clone();
            ui.rect("input")
                .size(80.0, 24.0)
                .on_text_input(move |event| {
                    text_for_callback.borrow_mut().push_str(&event.text);
                })
                .build();
        });
        backend
            .runtime
            .update_pointer(PointerEvent::pressed_at(8.0, 8.0));
        backend.pending_keyboard = KeyboardEvent {
            text: "A".to_string(),
            ..KeyboardEvent::default()
        };

        let text_for_compose = text.clone();
        let observed = observed_during_compose.clone();
        backend.compose(move |ui, _| {
            observed
                .borrow_mut()
                .push_str(text_for_compose.borrow().as_str());
            let text_for_callback = text_for_compose.clone();
            ui.rect("input")
                .size(80.0, 24.0)
                .on_text_input(move |event| {
                    text_for_callback.borrow_mut().push_str(&event.text);
                })
                .build();
        });

        assert_eq!(text.borrow().as_str(), "A");
        assert_eq!(observed_during_compose.borrow().as_str(), "A");
        assert!(backend.pending_keyboard.text.is_empty());
    }

    #[test]
    fn backend_dispatches_queued_pointer_clicks_in_order_before_compose() {
        let clicks = Rc::new(RefCell::new(Vec::new()));
        let mut backend = NeoUiBackend::new(NeoUiConfig {
            page_id: "page".to_string(),
        });
        backend.screen = Screen {
            width: 160.0,
            height: 80.0,
        };

        let first_clicks = clicks.clone();
        let second_clicks = clicks.clone();
        backend.compose(move |ui, _| {
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
            .pending_pointer_events
            .push(PointerEvent::pressed_at(10.0, 10.0));
        backend
            .pending_pointer_events
            .push(PointerEvent::released_at(10.0, 10.0));
        backend
            .pending_pointer_events
            .push(PointerEvent::pressed_at(10.0, 50.0));
        backend
            .pending_pointer_events
            .push(PointerEvent::released_at(10.0, 50.0));

        backend.compose(|ui, _| {
            ui.rect("first").position(0.0, 0.0).size(60.0, 30.0).build();
            ui.rect("second")
                .position(0.0, 40.0)
                .size(60.0, 30.0)
                .build();
        });

        assert_eq!(&*clicks.borrow(), &[1, 2]);
    }

    #[test]
    fn backend_scoped_compose_drains_signal_dirty_scopes_after_event_callbacks() {
        #[derive(Default)]
        struct Model {
            page: i32,
        }

        let state = eui_neo::State::new(Model::default());
        let builds = Rc::new(Cell::new(0));
        let mut backend = NeoUiBackend::new(NeoUiConfig {
            page_id: "page".to_string(),
        });
        backend.screen = Screen {
            width: 160.0,
            height: 80.0,
        };

        let compose_nav = |backend: &mut NeoUiBackend| {
            let dirty_state = state.clone();
            let compose_state = state.clone();
            let builds = builds.clone();
            backend.compose_scoped(
                move || dirty_state.take_dirty_scopes(),
                move |ui, _| {
                    ui.column("nav").size(160.0, 80.0).content(|ui| {
                        builds.set(builds.get() + 1);
                        let page = compose_state.signal(
                            "page",
                            |model| model.page,
                            |model, value| model.page = value,
                        );
                        let selected = page.watch(ui);
                        eui_neo::widgets::button(ui, "nav.button")
                            .size(120.0, 40.0)
                            .text(format!("Page {selected}"))
                            .on_click(move || page.set(1))
                            .build();
                    });
                },
            );
        };

        compose_nav(&mut backend);
        assert_eq!(builds.get(), 1);
        let frame = backend.runtime.find("nav.button.bg").unwrap().frame;
        backend
            .pending_pointer_events
            .push(PointerEvent::pressed_at(frame.x + 1.0, frame.y + 1.0));
        backend
            .pending_pointer_events
            .push(PointerEvent::released_at(frame.x + 1.0, frame.y + 1.0));

        compose_nav(&mut backend);

        assert_eq!(state.read(|model| model.page), 1);
        assert_eq!(builds.get(), 2);
        assert_eq!(
            backend.runtime.find("nav.button.text").unwrap().text,
            "Page 1"
        );
    }

    #[test]
    fn backend_scoped_compose_flushes_real_window_click_before_dirty_scopes() {
        #[derive(Default)]
        struct Model {
            page: i32,
        }

        let state = eui_neo::State::new(Model::default());
        let mut backend = NeoUiBackend::new(NeoUiConfig {
            page_id: "page".to_string(),
        });
        backend.screen = Screen {
            width: 240.0,
            height: 80.0,
        };

        let compose_nav = |backend: &mut NeoUiBackend| {
            let dirty_state = state.clone();
            let compose_state = state.clone();
            backend.compose_scoped(
                move || dirty_state.take_dirty_scopes(),
                move |ui, _| {
                    ui.column("nav").size(240.0, 80.0).content(|ui| {
                        let page = compose_state.signal(
                            "page",
                            |model| model.page,
                            |model, value| model.page = value,
                        );
                        let selected = page.watch(ui);
                        eui_neo::widgets::tabs(ui, "tabs")
                            .size(180.0, 40.0)
                            .items(["One", "Two", "Three"])
                            .selected(selected)
                            .on_change(move |next| page.set(next))
                            .build();
                    });
                },
            );
        };

        compose_nav(&mut backend);
        let label = backend.runtime.find("tabs.label.1").unwrap().frame;
        let point = [label.x + label.width * 0.5, label.y + label.height * 0.5];

        backend.event_pointer_position = Some(point);
        backend
            .pending_pointer_events
            .push(PointerEvent::pressed_at(point[0], point[1]));
        backend
            .pending_pointer_events
            .push(PointerEvent::released_at(point[0], point[1]));

        compose_nav(&mut backend);

        assert_eq!(state.read(|model| model.page), 1);
        assert_eq!(
            backend.runtime.find("tabs.indicator").unwrap().frame.x,
            backend.runtime.find("tabs.hit.1").unwrap().frame.x + 10.0
        );
    }
}
