use super::*;

impl Runtime {
    pub fn take_platform_effects(&mut self) -> Vec<PlatformEffect> {
        std::mem::take(&mut self.platform.effects)
    }

    pub(super) fn run_platform_effects_pass(&mut self) {
        let ime_rect = self.focused_ime_rect();
        let ime_effect = match (self.platform.ime_rect, ime_rect) {
            (None, Some(rect)) => Some(PlatformEffect::ImeStart { rect }),
            (Some(previous), Some(rect)) if previous != rect => {
                Some(PlatformEffect::ImeMove { rect })
            }
            (Some(_), None) => Some(PlatformEffect::ImeEnd),
            _ => None,
        };
        let cursor_shape = self.platform_cursor_shape();
        let cursor_effect =
            (self.platform.cursor_shape != cursor_shape).then_some(PlatformEffect::CursorShape {
                shape: cursor_shape,
            });
        let capture = self.platform_capture_state();
        let capture_effect = (self.platform.capture != capture)
            .then_some(PlatformEffect::Capture { state: capture });

        self.platform.ime_rect = ime_rect;
        self.platform.cursor_shape = cursor_shape;
        self.platform.capture = capture;
        self.platform.effects.clear();
        self.debug.platform_effects.clear();
        for effect in [ime_effect, cursor_effect, capture_effect]
            .into_iter()
            .flatten()
        {
            self.platform.effects.push(effect);
            self.debug.platform_effects.push(effect);
        }
    }

    pub fn platform_capture_state(&self) -> PlatformCaptureState {
        PlatformCaptureState::new(self.platform_wants_pointer(), self.has_keyboard_capture())
    }

    fn platform_cursor_shape(&self) -> CursorShape {
        self.input
            .owners
            .pointer_hover_node_id()
            .and_then(|id| self.find_node(&id))
            .filter(|element| element.interactive && !element.disabled)
            .map_or(CursorShape::Arrow, |element| element.cursor)
    }

    fn platform_wants_pointer(&self) -> bool {
        let screen = LayoutRect::new(0.0, 0.0, self.tree.screen.width, self.tree.screen.height);
        self.tree.roots.iter().any(|root| {
            self.tree_has_hover_or_active(root) || tree_has_fullscreen_interactive(root, screen)
        })
    }

    fn tree_has_hover_or_active(&self, element: &Element) -> bool {
        let id = NodeId::new(element.id.as_str());
        let state = self.interaction_for_node(&id);
        state.hovered
            || state.active
            || element
                .children
                .iter()
                .any(|child| self.tree_has_hover_or_active(child))
    }
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
    use super::*;
    use crate::{FrameInput, LayoutRect, PointerEvent, Screen, Size};

    #[test]
    fn frame_platform_pass_tracks_ime_start_move_and_end() {
        let mut runtime = Runtime::new("page");

        runtime.frame(FrameInput::new(Screen::new(160.0, 100.0), 0.0), |ui, _| {
            ui.rect("input")
                .position(10.0, 20.0)
                .size(80.0, 24.0)
                .ime_rect(0.0, 0.0, 80.0, 24.0)
                .on_text_input(|_| {})
                .build();
        });
        assert!(runtime.take_platform_effects().is_empty());

        runtime.update_pointer(PointerEvent::pressed_at(12.0, 22.0));
        runtime.frame(FrameInput::new(Screen::new(160.0, 100.0), 0.0), |ui, _| {
            ui.rect("input")
                .position(10.0, 20.0)
                .size(80.0, 24.0)
                .ime_rect(0.0, 0.0, 80.0, 24.0)
                .on_text_input(|_| {})
                .build();
        });

        assert_eq!(
            runtime.take_platform_effects(),
            vec![
                PlatformEffect::ImeStart {
                    rect: LayoutRect::new(10.0, 20.0, 80.0, 24.0)
                },
                PlatformEffect::Capture {
                    state: PlatformCaptureState::new(true, true)
                },
            ]
        );
        assert!(runtime.take_platform_effects().is_empty());

        runtime.frame(FrameInput::new(Screen::new(160.0, 100.0), 0.0), |ui, _| {
            ui.rect("input")
                .position(16.0, 24.0)
                .size(80.0, 24.0)
                .ime_rect(0.0, 0.0, 80.0, 24.0)
                .on_text_input(|_| {})
                .build();
        });
        assert_eq!(
            runtime.take_platform_effects(),
            vec![PlatformEffect::ImeMove {
                rect: LayoutRect::new(16.0, 24.0, 80.0, 24.0)
            }]
        );

        runtime.frame(FrameInput::new(Screen::new(160.0, 100.0), 0.0), |ui, _| {
            ui.rect("outside")
                .size(Size::fixed(40.0), Size::fixed(24.0))
                .build();
        });
        assert_eq!(
            runtime.take_platform_effects(),
            vec![
                PlatformEffect::ImeEnd,
                PlatformEffect::Capture {
                    state: PlatformCaptureState::new(false, false)
                },
            ]
        );
    }

    #[test]
    fn frame_platform_pass_tracks_cursor_shape_changes() {
        let mut runtime = Runtime::new("page");

        runtime.frame(FrameInput::new(Screen::new(160.0, 100.0), 0.0), |ui, _| {
            ui.rect("button")
                .position(10.0, 10.0)
                .size(80.0, 24.0)
                .interactive(true)
                .cursor(CursorShape::Hand)
                .build();
        });
        assert!(runtime.take_platform_effects().is_empty());

        runtime.frame(
            FrameInput::new(Screen::new(160.0, 100.0), 0.0).pointer(PointerEvent::at(12.0, 12.0)),
            |ui, _| {
                ui.rect("button")
                    .position(10.0, 10.0)
                    .size(80.0, 24.0)
                    .interactive(true)
                    .cursor(CursorShape::Hand)
                    .build();
            },
        );
        assert_eq!(
            runtime.take_platform_effects(),
            vec![
                PlatformEffect::CursorShape {
                    shape: CursorShape::Hand
                },
                PlatformEffect::Capture {
                    state: PlatformCaptureState::new(true, false)
                },
            ]
        );
        assert!(runtime.take_platform_effects().is_empty());

        runtime.frame(
            FrameInput::new(Screen::new(160.0, 100.0), 0.0).pointer(PointerEvent::at(12.0, 12.0)),
            |ui, _| {
                ui.rect("button")
                    .position(10.0, 10.0)
                    .size(80.0, 24.0)
                    .interactive(true)
                    .cursor(CursorShape::Hand)
                    .build();
            },
        );
        assert!(runtime.take_platform_effects().is_empty());

        runtime.frame(
            FrameInput::new(Screen::new(160.0, 100.0), 0.0).pointer(PointerEvent::at(140.0, 80.0)),
            |ui, _| {
                ui.rect("button")
                    .position(10.0, 10.0)
                    .size(80.0, 24.0)
                    .interactive(true)
                    .cursor(CursorShape::Hand)
                    .build();
            },
        );
        assert_eq!(
            runtime.take_platform_effects(),
            vec![
                PlatformEffect::CursorShape {
                    shape: CursorShape::Arrow
                },
                PlatformEffect::Capture {
                    state: PlatformCaptureState::new(false, false)
                },
            ]
        );
    }
}
