use std::any::Any;

use crate::input::Input;
use crate::ui::{
    UiBackend, UiBackendId, UiBeginFrameContext, UiCaptureState, UiError, UiEventContext,
    UiEventResponse, UiRenderContext,
};
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Ime, WindowEvent};
use winit::keyboard::ModifiersState;
use winit::window::Window;

use super::config::NeoUiConfig;
use super::input_bridge::{
    keyboard_from_key_event, merge_keyboard_event, pointer_from_input, scroll_from_input,
};
use super::{
    Element, KeyboardEvent, LayoutRect, NeoRenderer, NeoRuntime, PointerEvent, Screen, ScrollEvent,
    Ui,
};

/// Pluggable backend for the neo runtime.
#[derive(Debug)]
pub struct NeoUiBackend {
    runtime: NeoRuntime,
    renderer: Option<NeoRenderer>,
    pending_pointer: PointerEvent,
    pending_scroll: ScrollEvent,
    pending_keyboard: KeyboardEvent,
    modifiers: ModifiersState,
    capture: UiCaptureState,
    screen: Screen,
    delta_seconds: f32,
}

impl NeoUiBackend {
    pub const ID: UiBackendId = UiBackendId::new("neo");

    pub fn new(config: NeoUiConfig) -> Self {
        Self {
            runtime: NeoRuntime::new(config.page_id),
            renderer: None,
            pending_pointer: PointerEvent::default(),
            pending_scroll: ScrollEvent::default(),
            pending_keyboard: KeyboardEvent::default(),
            modifiers: ModifiersState::default(),
            capture: UiCaptureState::default(),
            screen: Screen::default(),
            delta_seconds: 0.0,
        }
    }

    pub fn runtime(&self) -> &NeoRuntime {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut NeoRuntime {
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
        self.pending_scroll = scroll_from_input(input);
        // Keyboard/text input is sourced from winit events so focused widgets do
        // not receive Backspace/Enter/arrows twice through the raw Input mirror.
        self.refresh_capture();
    }

    pub fn compose(&mut self, compose: impl FnOnce(&mut Ui, Screen)) {
        let screen = self.screen;
        self.runtime.update_events_and_timers(
            self.pending_pointer,
            self.pending_scroll,
            std::mem::take(&mut self.pending_keyboard),
            self.delta_seconds,
        );
        self.pending_scroll = ScrollEvent::default();
        self.runtime.compose(screen.width, screen.height, compose);
        self.runtime.tick_animations(self.delta_seconds);
        self.refresh_capture();
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
            let draw_list = self.runtime.draw_list();
            let pending_images = self.renderer.as_mut().is_some_and(|renderer| {
                renderer
                    .render(ctx.gpu, &draw_list, self.screen)
                    .pending_images
            });
            if pending_images {
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

fn tree_has_hover_or_active(element: &Element, runtime: &NeoRuntime) -> bool {
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

#[cfg(test)]
mod tests {
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
}
