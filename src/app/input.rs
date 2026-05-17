//! winit-to-Sky input conversion for the app runner.

use winit::event::{ElementState, WindowEvent};

use crate::input::raw::{Input, KeyCode, MouseButton};

pub(crate) fn update_from_window_event(
    input: &mut Input,
    event: &WindowEvent,
    suppressed: bool,
    scale_factor: f32,
) {
    match event {
        WindowEvent::KeyboardInput { event, .. } => {
            if let winit::keyboard::PhysicalKey::Code(code) = event.physical_key {
                let key = KeyCode::from_winit(code);
                match (event.state, suppressed) {
                    (ElementState::Pressed, false) => input.key_down(key),
                    (ElementState::Released, false) => input.key_up(key),
                    (ElementState::Pressed, true) => input.suppress_key_down(key),
                    (ElementState::Released, true) => input.suppress_key_up(key),
                }
            }
        }
        WindowEvent::CursorEntered { .. } => {
            if suppressed {
                input.set_cursor_in_window(false);
            } else {
                input.set_cursor_in_window(true);
            }
        }
        WindowEvent::CursorLeft { .. } => {
            input.set_cursor_in_window(false);
        }
        WindowEvent::CursorMoved { position, .. } => {
            let [x, y] = physical_cursor_to_logical(*position, scale_factor);
            if suppressed {
                input.set_mouse_position_suppressed(x, y);
            } else {
                input.set_mouse_position(x, y);
            }
        }
        WindowEvent::MouseInput {
            state: button_state,
            button,
            ..
        } => {
            let Some(mb) = MouseButton::from_winit(*button) else {
                return;
            };
            let index = mb.index();
            match (button_state, suppressed) {
                (ElementState::Pressed, false) => input.mouse_button_down(index),
                (ElementState::Released, false) => input.mouse_button_up(index),
                (ElementState::Pressed, true) => {
                    input.set_cursor_in_window(false);
                    input.suppress_mouse_button_down(index);
                }
                (ElementState::Released, true) => {
                    input.set_cursor_in_window(false);
                    input.suppress_mouse_button_up(index);
                }
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            if suppressed {
                input.set_cursor_in_window(false);
                return;
            }
            let (dx, dy) = match delta {
                winit::event::MouseScrollDelta::LineDelta(x, y) => (*x, *y),
                winit::event::MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
            };
            input.add_scroll_delta(dx, dy);
        }
        _ => {}
    }
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
    use super::*;

    #[test]
    fn cursor_position_is_converted_to_logical_pixels() {
        let converted =
            physical_cursor_to_logical(winit::dpi::PhysicalPosition::new(300.0, 150.0), 1.5);
        assert_eq!(converted, [200.0, 100.0]);
    }
}
