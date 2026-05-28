use std::fmt;

use crate::{
    Element, KeyboardEvent, LayoutRect, PointerEvent, Response, Runtime, Screen, ScrollEvent, Ui,
    UiDebugSnapshot,
};

#[derive(Debug, Clone, PartialEq)]
pub enum UiTestError {
    MissingElement { id: String },
}

impl fmt::Display for UiTestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingElement { id } => write!(f, "missing UI element `{id}`"),
        }
    }
}

impl std::error::Error for UiTestError {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TargetPoint {
    Center,
    Relative { x: f32, y: f32 },
    Absolute { x: f32, y: f32 },
}

impl TargetPoint {
    pub fn center() -> Self {
        Self::Center
    }

    pub fn relative(x: f32, y: f32) -> Self {
        Self::Relative { x, y }
    }

    pub fn absolute(x: f32, y: f32) -> Self {
        Self::Absolute { x, y }
    }

    pub fn resolve(self, frame: LayoutRect) -> [f32; 2] {
        match self {
            Self::Center => [frame.x + frame.width * 0.5, frame.y + frame.height * 0.5],
            Self::Relative { x, y } => [frame.x + frame.width * x, frame.y + frame.height * y],
            Self::Absolute { x, y } => [x, y],
        }
    }
}

impl Default for TargetPoint {
    fn default() -> Self {
        Self::Center
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiActionTrace {
    pub action: &'static str,
    pub target_id: String,
    pub target_frame: Option<LayoutRect>,
    pub point: Option<[f32; 2]>,
    pub before: UiDebugSnapshot,
    pub after_input: UiDebugSnapshot,
    pub input_changed: bool,
    pub needs_compose_after_input: bool,
}

impl fmt::Display for UiActionTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} {}", self.action, self.target_id)?;
        match self.target_frame {
            Some(frame) => writeln!(
                f,
                "  target frame: x={:.1}, y={:.1}, w={:.1}, h={:.1}",
                frame.x, frame.y, frame.width, frame.height
            )?,
            None => writeln!(f, "  target frame: <none>")?,
        }
        match self.point {
            Some([x, y]) => writeln!(f, "  point: x={x:.1}, y={y:.1}")?,
            None => writeln!(f, "  point: <none>")?,
        }
        writeln!(f, "  input changed: {}", self.input_changed)?;
        writeln!(f, "  needs compose: {}", self.needs_compose_after_input)?;
        writeln!(
            f,
            "  before: layout={:?} dirty={:?}",
            self.before.layout_mode, self.before.dirty_scopes
        )?;
        write!(
            f,
            "  after:  layout={:?} dirty={:?}",
            self.after_input.layout_mode, self.after_input.dirty_scopes
        )
    }
}

pub struct UiTestDriver {
    runtime: Runtime,
    screen: Screen,
    traces: Vec<UiActionTrace>,
}

impl UiTestDriver {
    pub fn new(page_id: impl Into<String>, width: f32, height: f32) -> Self {
        Self {
            runtime: Runtime::new(page_id),
            screen: Screen::new(width, height),
            traces: Vec::new(),
        }
    }

    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut Runtime {
        &mut self.runtime
    }

    pub fn compose(&mut self, build: impl FnOnce(&mut Ui, Screen)) {
        self.runtime
            .compose(self.screen.width, self.screen.height, build);
    }

    pub fn compose_scoped(
        &mut self,
        dirty_scopes: impl IntoIterator<Item = String>,
        build: impl FnOnce(&mut Ui, Screen),
    ) {
        self.runtime
            .compose_scoped(self.screen.width, self.screen.height, dirty_scopes, build);
    }

    pub fn click(&mut self, id: &str) -> Result<UiActionTrace, UiTestError> {
        self.click_at(id, TargetPoint::Center)
    }

    pub fn click_at(
        &mut self,
        id: &str,
        target: TargetPoint,
    ) -> Result<UiActionTrace, UiTestError> {
        let (frame, point) = self.resolve_target(id, target)?;
        let before = self.runtime.debug_snapshot_current();
        let pressed_changed = self
            .runtime
            .update_pointer(PointerEvent::pressed_at(point[0], point[1]));
        let released_changed = self
            .runtime
            .update_pointer(PointerEvent::released_at(point[0], point[1]));
        Ok(self.record_trace(UiActionTrace {
            action: "click",
            target_id: id.to_string(),
            target_frame: Some(frame),
            point: Some(point),
            before,
            after_input: self.runtime.debug_snapshot_current(),
            input_changed: pressed_changed || released_changed,
            needs_compose_after_input: self.runtime.needs_compose(),
        }))
    }

    pub fn right_click(&mut self, id: &str) -> Result<UiActionTrace, UiTestError> {
        let (frame, point) = self.resolve_target(id, TargetPoint::Center)?;
        let before = self.runtime.debug_snapshot_current();
        let changed = self
            .runtime
            .update_pointer(PointerEvent::right_pressed_at(point[0], point[1]));
        Ok(self.record_trace(UiActionTrace {
            action: "right_click",
            target_id: id.to_string(),
            target_frame: Some(frame),
            point: Some(point),
            before,
            after_input: self.runtime.debug_snapshot_current(),
            input_changed: changed,
            needs_compose_after_input: self.runtime.needs_compose(),
        }))
    }

    pub fn hover(&mut self, id: &str) -> Result<UiActionTrace, UiTestError> {
        let (frame, point) = self.resolve_target(id, TargetPoint::Center)?;
        let before = self.runtime.debug_snapshot_current();
        let changed = self
            .runtime
            .update_pointer(PointerEvent::at(point[0], point[1]));
        Ok(self.record_trace(UiActionTrace {
            action: "hover",
            target_id: id.to_string(),
            target_frame: Some(frame),
            point: Some(point),
            before,
            after_input: self.runtime.debug_snapshot_current(),
            input_changed: changed,
            needs_compose_after_input: self.runtime.needs_compose(),
        }))
    }

    pub fn drag_by(&mut self, id: &str, dx: f32, dy: f32) -> Result<UiActionTrace, UiTestError> {
        let (frame, point) = self.resolve_target(id, TargetPoint::Center)?;
        let before = self.runtime.debug_snapshot_current();
        let pressed_changed = self
            .runtime
            .update_pointer(PointerEvent::pressed_at(point[0], point[1]));
        let dragged_changed = self.runtime.update_pointer(PointerEvent::dragged_to(
            point[0] + dx,
            point[1] + dy,
            dx,
            dy,
        ));
        let released_changed = self
            .runtime
            .update_pointer(PointerEvent::released_at(point[0] + dx, point[1] + dy));
        Ok(self.record_trace(UiActionTrace {
            action: "drag_by",
            target_id: id.to_string(),
            target_frame: Some(frame),
            point: Some(point),
            before,
            after_input: self.runtime.debug_snapshot_current(),
            input_changed: pressed_changed || dragged_changed || released_changed,
            needs_compose_after_input: self.runtime.needs_compose(),
        }))
    }

    pub fn scroll(&mut self, id: &str, x: f32, y: f32) -> Result<UiActionTrace, UiTestError> {
        let (frame, point) = self.resolve_target(id, TargetPoint::Center)?;
        let before = self.runtime.debug_snapshot_current();
        let hover_changed = self
            .runtime
            .update_pointer(PointerEvent::at(point[0], point[1]));
        let scroll_changed = self.runtime.update_scroll(ScrollEvent { x, y });
        Ok(self.record_trace(UiActionTrace {
            action: "scroll",
            target_id: id.to_string(),
            target_frame: Some(frame),
            point: Some(point),
            before,
            after_input: self.runtime.debug_snapshot_current(),
            input_changed: hover_changed || scroll_changed,
            needs_compose_after_input: self.runtime.needs_compose(),
        }))
    }

    pub fn type_text(&mut self, text: impl Into<String>) -> Result<UiActionTrace, UiTestError> {
        self.keyboard(
            "type_text",
            KeyboardEvent {
                text: text.into(),
                ..KeyboardEvent::default()
            },
        )
    }

    pub fn press_backspace(&mut self) -> Result<UiActionTrace, UiTestError> {
        self.keyboard(
            "press_backspace",
            KeyboardEvent {
                backspace: true,
                ..KeyboardEvent::default()
            },
        )
    }

    pub fn press_enter(&mut self) -> Result<UiActionTrace, UiTestError> {
        self.keyboard(
            "press_enter",
            KeyboardEvent {
                enter: true,
                ..KeyboardEvent::default()
            },
        )
    }

    pub fn find(&self, id: &str) -> Option<&Element> {
        self.runtime.find(id)
    }

    pub fn frame(&self, id: &str) -> Result<LayoutRect, UiTestError> {
        self.runtime
            .find(id)
            .map(|element| element.frame)
            .ok_or_else(|| UiTestError::MissingElement { id: id.to_string() })
    }

    pub fn response(&self, id: &str) -> Response {
        self.runtime.response(id)
    }

    pub fn debug_snapshot(&self) -> &UiDebugSnapshot {
        self.runtime.debug_snapshot()
    }

    pub fn traces(&self) -> &[UiActionTrace] {
        &self.traces
    }

    fn keyboard(
        &mut self,
        action: &'static str,
        event: KeyboardEvent,
    ) -> Result<UiActionTrace, UiTestError> {
        let before = self.runtime.debug_snapshot_current();
        let focused_id = self
            .runtime
            .focused_id()
            .ok_or_else(|| UiTestError::MissingElement {
                id: "<focused>".to_string(),
            })?
            .to_string();
        let changed = self.runtime.update_keyboard(event);
        Ok(self.record_trace(UiActionTrace {
            action,
            target_id: focused_id,
            target_frame: None,
            point: None,
            before,
            after_input: self.runtime.debug_snapshot_current(),
            input_changed: changed,
            needs_compose_after_input: self.runtime.needs_compose(),
        }))
    }

    fn resolve_target(
        &self,
        id: &str,
        target: TargetPoint,
    ) -> Result<(LayoutRect, [f32; 2]), UiTestError> {
        let frame = self.frame(id)?;
        Ok((frame, target.resolve(frame)))
    }

    fn record_trace(&mut self, trace: UiActionTrace) -> UiActionTrace {
        self.traces.push(trace.clone());
        trace
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use crate::widgets;
    use crate::{DragEvent, Size};

    use super::*;

    #[test]
    fn click_triggers_button_callback() {
        let clicks = Rc::new(Cell::new(0));
        let mut driver = UiTestDriver::new("page", 200.0, 100.0);

        driver.compose({
            let clicks = clicks.clone();
            move |ui, _| {
                ui.rect("button")
                    .position(10.0, 10.0)
                    .size(80.0, 32.0)
                    .on_click(move || clicks.set(clicks.get() + 1))
                    .build();
            }
        });

        let trace = driver.click("button").expect("button should be clickable");

        assert_eq!(clicks.get(), 1, "{trace}");
        assert!(trace.input_changed, "{trace}");
        assert!(trace.needs_compose_after_input, "{trace}");
    }

    #[test]
    fn click_missing_element_returns_error() {
        let mut driver = UiTestDriver::new("page", 200.0, 100.0);
        driver.compose(|ui, _| {
            ui.rect("button").size(80.0, 32.0).build();
        });

        assert_eq!(
            driver.click("missing"),
            Err(UiTestError::MissingElement {
                id: "missing".to_string()
            })
        );
    }

    #[test]
    fn hover_updates_response_state() {
        let mut driver = UiTestDriver::new("page", 200.0, 100.0);
        driver.compose(|ui, _| {
            ui.rect("button")
                .position(10.0, 10.0)
                .size(80.0, 32.0)
                .on_click(|| {})
                .build();
        });

        let trace = driver.hover("button").expect("button should exist");

        assert!(driver.response("button").hovered(), "{trace}");
        assert!(trace.input_changed, "{trace}");
    }

    #[test]
    fn drag_by_sends_drag_callback_values() {
        let drag = Rc::new(Cell::new(DragEvent::default()));
        let mut driver = UiTestDriver::new("page", 200.0, 100.0);

        driver.compose({
            let drag = drag.clone();
            move |ui, _| {
                ui.rect("slider")
                    .position(10.0, 10.0)
                    .size(80.0, 32.0)
                    .on_drag(move |event| drag.set(event))
                    .build();
            }
        });

        let trace = driver
            .drag_by("slider", 24.0, 6.0)
            .expect("slider should be draggable");

        assert_eq!(drag.get().delta_x, 24.0, "{trace}");
        assert_eq!(drag.get().delta_y, 6.0, "{trace}");
        assert_eq!(drag.get().total_x, 24.0, "{trace}");
        assert_eq!(drag.get().total_y, 6.0, "{trace}");
    }

    #[test]
    fn type_text_routes_to_focused_input() {
        let value = Rc::new(RefCell::new(String::new()));
        let mut driver = UiTestDriver::new("page", 240.0, 100.0);

        driver.compose({
            let value = value.clone();
            move |ui, _| {
                widgets::input(ui, "field")
                    .size(Size::fixed(180.0), Size::fixed(36.0))
                    .on_change(move |text| *value.borrow_mut() = text.to_string())
                    .build();
            }
        });

        driver.click("field.hit").expect("input should focus");
        let trace = driver
            .type_text("abc")
            .expect("focused input should accept text");

        assert_eq!(value.borrow().as_str(), "abc", "{trace}");
        assert!(trace.needs_compose_after_input, "{trace}");
    }
}
