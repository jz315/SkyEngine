use super::event_command::{UiEventCommand, UiEventCommandReport};
use super::layers::{
    layer_blocks_element_target, layer_pointer_policy, layer_requests_keyboard_capture,
};
use super::reconcile::ElementIdSet;
use super::tree::z_order_is_stable;
use super::*;

impl Runtime {
    pub(crate) fn diagnostic_focused_id(&self) -> Option<&str> {
        self.input.owners.keyboard_focus_id()
    }

    pub(crate) fn diagnostic_text_focused_id(&self) -> Option<&str> {
        self.input.owners.text_focus_id()
    }

    pub fn has_keyboard_capture(&self) -> bool {
        self.input.owners.keyboard_focus.is_some()
            || self.input.owners.text_focus.is_some()
            || self.input.owners.ime_owner.is_some()
            || layer_requests_keyboard_capture(&self.layers.intents)
    }

    pub fn focused_ime_rect(&self) -> Option<LayoutRect> {
        let element = self.find_node(&self.input.owners.ime_owner_node_id()?)?;
        if !element.has_ime_rect {
            return None;
        }
        Some(LayoutRect::new(
            element.frame.x + element.ime_rect.x,
            element.frame.y + element.ime_rect.y,
            element.ime_rect.width,
            element.ime_rect.height,
        ))
    }

    pub(crate) fn diagnostic_response(&self, id: &str) -> Response {
        let id = self.resolve_node_id(id);
        self.response_for_node(&id)
    }

    pub(crate) fn response_for_node(&self, id: &NodeId) -> Response {
        self.input.responses.get(id).copied().unwrap_or_default()
    }

    pub(crate) fn diagnostic_interaction(&self, id: &str) -> InteractionState {
        let id = self.resolve_node_id(id);
        self.interaction_for_node(&id)
    }

    pub(crate) fn interaction_for_node(&self, id: &NodeId) -> InteractionState {
        self.input.interactions.get(id).copied().unwrap_or_default()
    }

    pub(crate) fn update_pointer(&mut self, event: PointerEvent) -> bool {
        let pointer_pass = self.collect_pointer_event_pass(event);
        let report = self.commit_pointer_event_pass(pointer_pass);
        self.record_frame_input_pass(report).changed()
    }

    pub(super) fn collect_pointer_event_pass(&mut self, event: PointerEvent) -> PointerEventPass {
        let position = event.position();
        let delta = event.delta();
        if position.is_some() {
            self.input.pointer_position = position;
        }
        let mut commands = Vec::new();
        let layer_policy = layer_pointer_policy(&self.layers.intents, &self.tree.roots, position);
        if event.pressed_this_frame {
            self.layers.dismissal_records.clear();
            self.layers.pointer_records.clear();
            if let Some(record) = layer_policy.debug.clone() {
                self.layers.pointer_records.push(record);
            }
            if let Some(dismissal) = layer_policy.dismissal.clone() {
                commands.push(UiEventCommand::LayerDismiss {
                    target: dismissal.target(),
                });
                self.layers.dismissal_records.push(dismissal.into_record());
            }
        }
        let hit_id = (!layer_policy.block_pointer)
            .then(|| hit_test_interactive(&self.tree.roots, position))
            .flatten();
        let focus_target = event.pressed_this_frame.then(|| {
            (!layer_policy.block_pointer)
                .then(|| hit_test_focusable(&self.tree.roots, position))
                .flatten()
        });
        if event.pressed_this_frame {
            self.input.owners.set_pointer_press_target(hit_id.clone());
            self.input.drag_origin = position;
        }

        let captured_id = self.input.owners.pointer_capture_node_id();
        let hover_id = captured_id.clone().or(hit_id.clone());
        self.input.owners.set_pointer_hover(hover_id.clone());
        let mut ids = FxHashSet::default();
        ids.extend(self.input.interactions.keys().cloned());
        if let Some(id) = captured_id.as_ref() {
            ids.insert(id.clone());
        }
        if let Some(id) = hit_id.as_ref() {
            ids.insert(id.clone());
        }

        if event.right_pressed_this_frame {
            if let Some(target_id) = hit_id.as_ref() {
                let frame = self
                    .find_node(target_id)
                    .map(|element| element.frame)
                    .unwrap_or_default();
                commands.push(UiEventCommand::ContextMenu {
                    target: target_id.clone(),
                    event,
                    frame,
                });
            }
        }

        let mut changed = false;
        let mut next = FxHashMap::default();
        let mut responses = FxHashMap::default();
        for id in ids {
            let previous = self
                .input
                .interactions
                .get(&id)
                .copied()
                .unwrap_or_default();
            let active = captured_id.as_ref().is_some_and(|target| target == &id);
            let hovered = hover_id.as_ref().is_some_and(|target| target == &id);
            let pressed = active && event.down;
            let press_started = active && event.pressed_this_frame;
            let released = active && event.released_this_frame;
            let clicked = released && hit_id.as_ref().is_some_and(|target| target == &id);
            let drag_start = if press_started {
                position.unwrap_or(previous.drag_start)
            } else {
                previous.drag_start
            };
            let drag_total = if active {
                match (position, self.input.drag_origin) {
                    (Some(position), Some(origin)) => {
                        [position[0] - origin[0], position[1] - origin[1]]
                    }
                    _ => previous.drag_total,
                }
            } else {
                [0.0, 0.0]
            };
            let dragging =
                active && (drag_total[0] * drag_total[0] + drag_total[1] * drag_total[1]) > 4.0;
            let mut state = InteractionState {
                hovered,
                pressed,
                clicked,
                press_started,
                released,
                dragging,
                active,
                changed: false,
                drag_start,
                drag_delta: delta,
                drag_total,
            };
            state.changed = state_without_changed(state) != state_without_changed(previous);
            changed |= state.changed;
            if press_started {
                let frame = self
                    .find_node(&id)
                    .map(|element| element.frame)
                    .unwrap_or_default();
                commands.push(UiEventCommand::Press {
                    target: id.clone(),
                    event,
                    frame,
                });
            }
            if clicked {
                commands.push(UiEventCommand::Click { target: id.clone() });
            }
            if pressed && (delta != [0.0, 0.0] || dragging) && self.input.callbacks.has_drag(&id) {
                let [x, y] = position.unwrap_or_default();
                self.input.owners.set_drag_owner(id.clone());
                commands.push(UiEventCommand::Drag {
                    target: id.clone(),
                    event: DragEvent {
                        x,
                        y,
                        delta_x: delta[0],
                        delta_y: delta[1],
                        total_x: drag_total[0],
                        total_y: drag_total[1],
                    },
                });
            }
            if state != InteractionState::default() || state.changed {
                responses.insert(
                    id.clone(),
                    Response {
                        hovered,
                        pressed,
                        clicked,
                        focused: false,
                        changed: state.changed,
                    },
                );
            }
            if state != InteractionState::default() {
                next.insert(id, state);
            }
        }

        if event.released_this_frame {
            self.input.owners.clear_pointer_press();
            self.input.drag_origin = None;
        }

        PointerEventPass {
            interactions: next,
            responses,
            commands,
            changed,
            focus_target,
        }
    }

    pub(super) fn commit_pointer_event_pass(
        &mut self,
        pass: PointerEventPass,
    ) -> FrameInputPassReport {
        let mut input_state_changed = pass.changed;
        let mut commands = pass.commands;
        input_state_changed |= self.apply_focus_target(pass.focus_target, &mut commands);
        if input_state_changed {
            self.request_pointer_input_invalidation();
        }
        self.input.interactions = pass.interactions;
        self.input.responses = pass.responses;
        self.execute_frame_input_command_batch(FrameInputCommandBatch {
            commands,
            input_state_changed,
            timer_render_requested: false,
        })
    }

    pub(crate) fn update_scroll(&mut self, event: ScrollEvent) -> bool {
        if !event.active() {
            return false;
        };
        let commands = self.collect_scroll_command(event).into_iter().collect();
        let report = self.execute_frame_input_command_batch(FrameInputCommandBatch {
            commands,
            input_state_changed: false,
            timer_render_requested: false,
        });
        self.record_frame_input_pass(report).changed()
    }

    pub(crate) fn update_keyboard(&mut self, event: KeyboardEvent) -> bool {
        if !event.has_input() {
            return false;
        };
        let commands = self.collect_keyboard_command(event).into_iter().collect();
        let report = self.execute_frame_input_command_batch(FrameInputCommandBatch {
            commands,
            input_state_changed: false,
            timer_render_requested: false,
        });
        self.record_frame_input_pass(report).changed()
    }

    pub(super) fn update_events_and_timers(
        &mut self,
        pointer: PointerEvent,
        scroll: ScrollEvent,
        keyboard: KeyboardEvent,
        delta_seconds: f32,
    ) -> FrameInputPassReport {
        let batch = self.collect_frame_input_commands([pointer], scroll, keyboard, delta_seconds);
        let report = self.execute_frame_input_command_batch(batch);
        self.record_frame_input_pass(report)
    }

    pub(super) fn update_events_and_timers_from_pointer_events(
        &mut self,
        pointer_events: &[PointerEvent],
        scroll: ScrollEvent,
        keyboard: KeyboardEvent,
        delta_seconds: f32,
    ) -> FrameInputPassReport {
        let batch = self.collect_frame_input_commands(
            pointer_events.iter().copied(),
            scroll,
            keyboard,
            delta_seconds,
        );
        let report = self.execute_frame_input_command_batch(batch);
        self.record_frame_input_pass(report)
    }

    fn collect_frame_input_commands(
        &mut self,
        pointer_events: impl IntoIterator<Item = PointerEvent>,
        scroll: ScrollEvent,
        keyboard: KeyboardEvent,
        delta_seconds: f32,
    ) -> FrameInputCommandBatch {
        self.timing.clock_seconds += f64::from(delta_seconds.max(0.0));
        let mut batch = FrameInputCommandBatch::default();
        let mut commands = Vec::new();
        batch.input_state_changed |=
            self.collect_frame_pointer_commands(pointer_events, &mut commands);
        if let Some(command) = self.collect_scroll_command(scroll) {
            commands.push(command);
        }
        if let Some(command) = self.collect_keyboard_command(keyboard) {
            commands.push(command);
        }
        let timer_collection = self.collect_timer_commands(delta_seconds);
        batch.timer_render_requested = timer_collection.render_requested;
        commands.extend(timer_collection.commands);
        batch.commands = commands;
        batch
    }

    pub(super) fn execute_frame_input_command_batch(
        &mut self,
        batch: FrameInputCommandBatch,
    ) -> FrameInputPassReport {
        let command_report = self.execute_event_commands(batch.commands);
        FrameInputPassReport {
            input_state_changed: batch.input_state_changed,
            timer_render_requested: batch.timer_render_requested,
            command_report,
        }
    }

    fn record_frame_input_pass(&mut self, report: FrameInputPassReport) -> FrameInputPassReport {
        self.record_input_pass_debug(&report);
        report
    }

    fn collect_frame_pointer_commands(
        &mut self,
        pointer_events: impl IntoIterator<Item = PointerEvent>,
        commands: &mut Vec<UiEventCommand>,
    ) -> bool {
        let mut changed = false;
        for event in pointer_events {
            let pointer_pass = self.collect_pointer_event_pass(event);
            changed |= self.apply_focus_target(pointer_pass.focus_target, commands);
            changed |= pointer_pass.changed;
            self.input.interactions = pointer_pass.interactions;
            self.input.responses = pointer_pass.responses;
            commands.extend(pointer_pass.commands);
        }
        if changed {
            self.request_pointer_input_invalidation();
        }
        changed
    }

    fn apply_focus_target(
        &mut self,
        focused: Option<Option<NodeId>>,
        commands: &mut Vec<UiEventCommand>,
    ) -> bool {
        let Some(focused) = focused else {
            return false;
        };
        if self.input.owners.keyboard_focus_id() == focused.as_ref().map(NodeId::as_str) {
            return false;
        }
        let old = self.input.owners.keyboard_focus_node_id();
        let text_enabled = focused
            .as_ref()
            .filter(|id| self.input.callbacks.has_text_input(id))
            .is_some();
        self.input
            .owners
            .set_keyboard_focus(focused.clone(), text_enabled);

        if let Some(old) = old {
            commands.push(UiEventCommand::FocusChanged {
                target: old,
                focused: false,
            });
        }
        if let Some(new) = focused {
            commands.push(UiEventCommand::FocusChanged {
                target: new,
                focused: true,
            });
        }
        true
    }

    fn collect_scroll_command(&mut self, event: ScrollEvent) -> Option<UiEventCommand> {
        if !event.active() {
            return None;
        }
        let layer_policy = layer_pointer_policy(
            &self.layers.intents,
            &self.tree.roots,
            self.input.pointer_position,
        );
        self.layers.pointer_records.clear();
        if let Some(record) = layer_policy.debug.clone() {
            self.layers.pointer_records.push(record);
        }
        if layer_policy.block_pointer {
            return None;
        }
        let target = hit_test(&self.tree.roots, self.input.pointer_position, |element| {
            let id = NodeId::new(element.id.as_str());
            self.input.callbacks.has_scroll(&id) && !element.disabled
        })?;
        self.input.owners.set_scroll_owner(target.clone());
        Some(UiEventCommand::Scroll { target, event })
    }

    fn collect_keyboard_command(&self, event: KeyboardEvent) -> Option<UiEventCommand> {
        if !event.has_input() {
            return None;
        }
        let focused_id = self.input.owners.text_focus_node_id()?;
        if layer_blocks_element_target(&self.layers.intents, &self.tree.roots, &focused_id) {
            return None;
        }
        Some(UiEventCommand::TextInput {
            target: focused_id,
            event,
        })
    }

    fn request_pointer_input_invalidation(&mut self) {
        self.request_invalidation(Invalidation::runtime(
            self.runtime_invalidation_target(),
            "pointer_input",
            crate::DirtyFlags::DRAW,
        ));
    }
}

pub(super) fn replay_frame_responses(runtime: &Runtime, ui: &mut Ui) {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.replay_responses");
    for (id, response) in &runtime.input.responses {
        ui.set_response_node(id.clone(), *response);
    }
}

pub(super) struct PointerEventPass {
    interactions: FxHashMap<NodeId, InteractionState>,
    responses: FxHashMap<NodeId, Response>,
    commands: Vec<UiEventCommand>,
    changed: bool,
    focus_target: Option<Option<NodeId>>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct FrameInputCommandBatch {
    pub(super) commands: Vec<UiEventCommand>,
    pub(super) input_state_changed: bool,
    pub(super) timer_render_requested: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct FrameInputPassReport {
    pub(super) input_state_changed: bool,
    pub(super) timer_render_requested: bool,
    pub(super) command_report: UiEventCommandReport,
}

impl FrameInputPassReport {
    pub(super) fn changed(&self) -> bool {
        self.input_state_changed || self.command_report.changed()
    }
}

pub(super) fn hit_test_interactive(
    elements: &[Element],
    position: Option<[f32; 2]>,
) -> Option<NodeId> {
    hit_test(elements, position, |element| {
        element.interactive && !element.disabled
    })
}

pub(super) fn hit_test_focusable(
    elements: &[Element],
    position: Option<[f32; 2]>,
) -> Option<NodeId> {
    hit_test(elements, position, |element| {
        element.focusable && !element.disabled
    })
}

pub(super) fn hit_test(
    elements: &[Element],
    position: Option<[f32; 2]>,
    predicate: impl Fn(&Element) -> bool,
) -> Option<NodeId> {
    let position = position?;
    hit_test_elements(elements, position, None, &predicate).map(|element| NodeId::new(&element.id))
}

pub(super) fn hit_test_elements<'a>(
    elements: &'a [Element],
    position: [f32; 2],
    clip: Option<UiClip>,
    predicate: &impl Fn(&Element) -> bool,
) -> Option<&'a Element> {
    if elements.len() <= 1 {
        for element in elements.iter().rev() {
            if let Some(target) = hit_test_element(element, position, clip, predicate) {
                return Some(target);
            }
        }
        return None;
    }

    if z_order_is_stable(elements) {
        for element in elements.iter().rev() {
            if let Some(target) = hit_test_element(element, position, clip, predicate) {
                return Some(target);
            }
        }
        return None;
    }

    let mut order: SmallVec<[usize; 16]> = (0..elements.len()).collect();
    order.sort_by_key(|&index| (elements[index].z_index, index));
    for index in order.into_iter().rev() {
        if let Some(target) = hit_test_element(&elements[index], position, clip, predicate) {
            return Some(target);
        }
    }
    None
}

pub(super) fn hit_test_element<'a>(
    element: &'a Element,
    position: [f32; 2],
    clip: Option<UiClip>,
    predicate: &impl Fn(&Element) -> bool,
) -> Option<&'a Element> {
    if clip.is_some_and(|clip| !clip.contains(position)) {
        return None;
    }
    let next_clip = if element.clip {
        let radius = element.clip_radius;
        let current = UiClip::new(element.frame, radius);
        let clip = match clip {
            Some(parent) => intersect_clip(parent, current)?,
            None => current,
        };
        if !clip.contains(position) {
            return None;
        }
        Some(clip)
    } else {
        clip
    };

    let element_hit = if predicate(element)
        && hit_contains(element, position)
        && next_clip.is_none_or(|clip| clip.contains(position))
    {
        Some(element)
    } else {
        None
    };

    hit_test_elements(&element.children, position, next_clip, predicate).or(element_hit)
}

pub(super) fn hit_contains(element: &Element, position: [f32; 2]) -> bool {
    if element.kind == ElementKind::Polygon {
        return polygon_contains(element, position);
    }
    element.frame.contains(position)
}

pub(super) fn polygon_contains(element: &Element, position: [f32; 2]) -> bool {
    if element.polygon_points.len() < 3 || !element.frame.contains(position) {
        return false;
    }
    let local_x = position[0] - element.frame.x;
    let local_y = position[1] - element.frame.y;
    let mut inside = false;
    let mut previous = element.polygon_points.len() - 1;
    for current in 0..element.polygon_points.len() {
        let a = element.polygon_points[current];
        let b = element.polygon_points[previous];
        let denominator = b[1] - a[1];
        let crosses = (a[1] > local_y) != (b[1] > local_y)
            && local_x < (b[0] - a[0]) * (local_y - a[1]) / denominator + a[0];
        if crosses {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

pub(super) fn intersect_rect(left: LayoutRect, right: LayoutRect) -> Option<LayoutRect> {
    let x0 = left.x.max(right.x);
    let y0 = left.y.max(right.y);
    let x1 = left.right().min(right.right());
    let y1 = left.bottom().min(right.bottom());
    (x1 > x0 && y1 > y0).then(|| LayoutRect::new(x0, y0, x1 - x0, y1 - y0))
}

pub(super) fn intersect_clip(left: UiClip, right: UiClip) -> Option<UiClip> {
    let rect = intersect_rect(left.rect, right.rect)?;
    let radius = if same_rect(rect, right.rect) {
        right.radius
    } else if same_rect(rect, left.rect) {
        left.radius
    } else {
        left.radius.min(right.radius)
    };
    Some(UiClip::new(rect, radius))
}

pub(super) fn same_rect(left: LayoutRect, right: LayoutRect) -> bool {
    (left.x - right.x).abs() <= 0.001
        && (left.y - right.y).abs() <= 0.001
        && (left.width - right.width).abs() <= 0.001
        && (left.height - right.height).abs() <= 0.001
}

pub(super) fn cleanup_stale_input_state(
    runtime: &mut Runtime,
    existing_ids: &ElementIdSet,
) -> bool {
    let mut changed = runtime
        .input
        .owners
        .retain_existing(|id| existing_ids.contains(id));
    let previous_interactions = runtime.input.interactions.len();
    runtime
        .input
        .interactions
        .retain(|id, _| existing_ids.contains(id));
    changed |= runtime.input.interactions.len() != previous_interactions;
    let previous_responses = runtime.input.responses.len();
    runtime
        .input
        .responses
        .retain(|id, _| existing_ids.contains(id));
    changed |= runtime.input.responses.len() != previous_responses;
    changed
}

pub(super) fn state_without_changed(mut state: InteractionState) -> InteractionState {
    state.changed = false;
    state
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use crate::callbacks::{
        ClickCallbackId, ScrollCallbackId, TextInputCallbackId, TimerCallbackId,
    };

    use super::*;

    #[test]
    fn cleanup_stale_input_state_keeps_only_existing_ids() {
        let mut runtime = Runtime::new("page");
        runtime
            .input
            .owners
            .set_keyboard_focus(Some(NodeId::new("page.removed")), true);
        runtime.input.interactions.insert(
            NodeId::new("page.kept"),
            InteractionState {
                hovered: true,
                ..InteractionState::default()
            },
        );
        runtime.input.interactions.insert(
            NodeId::new("page.removed"),
            InteractionState {
                pressed: true,
                ..InteractionState::default()
            },
        );
        runtime.input.responses.insert(
            NodeId::new("page.kept"),
            Response {
                hovered: true,
                ..Response::default()
            },
        );
        runtime.input.responses.insert(
            NodeId::new("page.removed"),
            Response {
                pressed: true,
                ..Response::default()
            },
        );
        let existing_ids = element_ids(&["page.kept"]);

        assert!(cleanup_stale_input_state(&mut runtime, &existing_ids));

        assert_eq!(runtime.input.owners.keyboard_focus_id(), None);
        assert_eq!(runtime.input.owners.text_focus_id(), None);
        assert!(runtime
            .input
            .interactions
            .contains_key(&NodeId::new("page.kept")));
        assert!(!runtime
            .input
            .interactions
            .contains_key(&NodeId::new("page.removed")));
        assert!(runtime
            .input
            .responses
            .contains_key(&NodeId::new("page.kept")));
        assert!(!runtime
            .input
            .responses
            .contains_key(&NodeId::new("page.removed")));
    }

    #[test]
    fn replay_frame_responses_seeds_builder_responses() {
        let mut runtime = Runtime::new("page");
        runtime.input.responses.insert(
            NodeId::new("page.hit"),
            Response {
                hovered: true,
                clicked: true,
                ..Response::default()
            },
        );
        let mut ui = Ui::new("page");

        replay_frame_responses(&runtime, &mut ui);

        let response = ui.response("hit");
        assert!(response.hovered);
        assert!(response.clicked);
        assert!(!response.pressed);
    }

    #[test]
    fn collect_frame_input_commands_defers_callback_execution() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();
        let clicked = Rc::new(Cell::new(false));
        let timer_fired = Rc::new(Cell::new(false));
        let clicked_callback = clicked.clone();
        let timer_callback = timer_fired.clone();
        runtime.input.callbacks.on_click.insert(
            ClickCallbackId::new(NodeId::new("page.button")),
            Box::new(move || clicked_callback.set(true)),
        );
        runtime.input.callbacks.on_timer.insert(
            TimerCallbackId::new(NodeId::new("page.timer")),
            Box::new(move || timer_callback.set(true)),
        );
        let mut button = Element::new(ElementKind::Rect, "page.button");
        button.interactive = true;
        button.frame = LayoutRect::new(0.0, 0.0, 40.0, 30.0);
        let mut timer = Element::new(ElementKind::Rect, "page.timer");
        timer.timer_seconds = 0.1;
        runtime.tree.roots = vec![button, timer];

        let batch = runtime.collect_frame_input_commands(
            [
                PointerEvent::pressed_at(10.0, 10.0),
                PointerEvent::released_at(10.0, 10.0),
            ],
            ScrollEvent::default(),
            KeyboardEvent::default(),
            0.1,
        );

        assert!(batch.input_state_changed);
        assert!(!batch.timer_render_requested);
        assert_eq!(batch.commands.len(), 3);
        assert!(!clicked.get());
        assert!(!timer_fired.get());
        assert!(runtime.diagnostics().current_snapshot().events.is_empty());

        let report = runtime.execute_frame_input_command_batch(batch);

        assert!(clicked.get());
        assert!(timer_fired.get());
        assert!(report.input_state_changed);
        assert_eq!(report.command_report.command_count, 3);
        assert_eq!(report.command_report.callback_count, 2);
        assert_eq!(report.command_report.invalidation_count, 2);
        assert!(report.command_report.pass_flags.request_compose_ui);
    }

    #[test]
    fn frame_input_helpers_record_input_pass_debug() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();
        let clicked = Rc::new(Cell::new(false));
        let clicked_callback = clicked.clone();
        runtime.input.callbacks.on_click.insert(
            ClickCallbackId::new(NodeId::new("page.button")),
            Box::new(move || clicked_callback.set(true)),
        );
        let mut button = Element::new(ElementKind::Rect, "page.button");
        button.interactive = true;
        button.frame = LayoutRect::new(0.0, 0.0, 40.0, 30.0);
        runtime.tree.roots = vec![button];

        let report = runtime.update_events_and_timers_from_pointer_events(
            &[
                PointerEvent::pressed_at(10.0, 10.0),
                PointerEvent::released_at(10.0, 10.0),
            ],
            ScrollEvent::default(),
            KeyboardEvent::default(),
            0.0,
        );

        assert!(clicked.get());
        assert_eq!(report.command_report.command_count, 2);
        assert_eq!(report.command_report.callback_count, 1);
        let snapshot = runtime.diagnostics().current_snapshot();
        let input_pass = snapshot
            .input_pass
            .as_ref()
            .expect("frame input helper should publish input pass debug");
        assert!(input_pass.input_state_changed);
        assert_eq!(input_pass.command_count, 2);
        assert_eq!(input_pass.callback_count, 1);
        assert_eq!(input_pass.invalidation_count, 1);
        assert!(input_pass.pass_flags.request_compose_ui);
    }

    #[test]
    fn direct_pointer_update_records_input_pass_debug() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();
        let clicked = Rc::new(Cell::new(false));
        let clicked_callback = clicked.clone();
        runtime.input.callbacks.on_click.insert(
            ClickCallbackId::new(NodeId::new("page.button")),
            Box::new(move || clicked_callback.set(true)),
        );
        let mut button = Element::new(ElementKind::Rect, "page.button");
        button.interactive = true;
        button.frame = LayoutRect::new(0.0, 0.0, 40.0, 30.0);
        runtime.tree.roots = vec![button];

        runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
        assert_eq!(
            runtime
                .diagnostics()
                .current_snapshot()
                .input_pass
                .as_ref()
                .map(|record| (record.command_count, record.callback_count)),
            Some((1, 0))
        );
        assert!(!clicked.get());

        runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));

        assert!(clicked.get());
        let snapshot = runtime.diagnostics().current_snapshot();
        let input_pass = snapshot
            .input_pass
            .as_ref()
            .expect("direct pointer update should record input pass debug");
        assert!(input_pass.input_state_changed);
        assert_eq!(input_pass.command_count, 1);
        assert_eq!(input_pass.callback_count, 1);
        assert_eq!(input_pass.invalidation_count, 1);
        assert!(input_pass.pass_flags.request_compose_ui);
    }

    #[test]
    fn direct_scroll_and_keyboard_updates_record_input_pass_debug() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();
        let scrolled = Rc::new(Cell::new(false));
        let typed = Rc::new(Cell::new(false));
        let scrolled_callback = scrolled.clone();
        let typed_callback = typed.clone();
        runtime.input.callbacks.on_scroll.insert(
            ScrollCallbackId::new(NodeId::new("page.scroll")),
            Box::new(move |_| scrolled_callback.set(true)),
        );
        runtime.input.callbacks.on_text_input.insert(
            TextInputCallbackId::new(NodeId::new("page.input")),
            Box::new(move |_| typed_callback.set(true)),
        );
        let mut scroll = Element::new(ElementKind::Rect, "page.scroll");
        scroll.frame = LayoutRect::new(0.0, 0.0, 40.0, 30.0);
        let input = Element::new(ElementKind::Rect, "page.input");
        runtime.tree.roots = vec![scroll, input];
        runtime.input.pointer_position = Some([10.0, 10.0]);
        runtime
            .input
            .owners
            .set_keyboard_focus(Some(NodeId::new("page.input")), true);

        assert!(runtime.update_scroll(ScrollEvent { x: 0.0, y: 8.0 }));
        assert!(scrolled.get());
        let snapshot = runtime.diagnostics().current_snapshot();
        let input_pass = snapshot
            .input_pass
            .as_ref()
            .expect("direct scroll update should record input pass debug");
        assert_eq!(input_pass.command_count, 1);
        assert_eq!(input_pass.callback_count, 1);
        assert!(input_pass.pass_flags.request_compose_ui);

        assert!(runtime.update_keyboard(KeyboardEvent {
            text: "a".to_string(),
            ..KeyboardEvent::default()
        }));
        assert!(typed.get());
        let snapshot = runtime.diagnostics().current_snapshot();
        let input_pass = snapshot
            .input_pass
            .as_ref()
            .expect("direct keyboard update should record input pass debug");
        assert_eq!(input_pass.command_count, 1);
        assert_eq!(input_pass.callback_count, 1);
        assert!(input_pass.pass_flags.request_compose_ui);
    }

    #[test]
    fn direct_scroll_and_keyboard_no_command_paths_record_empty_input_pass_debug() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();
        let clicked = Rc::new(Cell::new(false));
        let clicked_callback = clicked.clone();
        runtime.input.callbacks.on_click.insert(
            ClickCallbackId::new(NodeId::new("page.button")),
            Box::new(move || clicked_callback.set(true)),
        );
        let mut button = Element::new(ElementKind::Rect, "page.button");
        button.interactive = true;
        button.frame = LayoutRect::new(0.0, 0.0, 40.0, 30.0);
        runtime.tree.roots = vec![button];

        runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));
        assert!(clicked.get());
        assert_eq!(
            runtime
                .diagnostics()
                .current_snapshot()
                .input_pass
                .as_ref()
                .map(|record| record.callback_count),
            Some(1)
        );

        assert!(!runtime.update_scroll(ScrollEvent { x: 0.0, y: 4.0 }));
        let snapshot = runtime.diagnostics().current_snapshot();
        let input_pass = snapshot
            .input_pass
            .as_ref()
            .expect("active scroll without a target should still publish an input pass");
        assert!(!input_pass.input_state_changed);
        assert_eq!(input_pass.command_count, 0);
        assert_eq!(input_pass.callback_count, 0);
        assert_eq!(input_pass.invalidation_count, 0);

        assert!(!runtime.update_keyboard(KeyboardEvent {
            text: "a".to_string(),
            ..KeyboardEvent::default()
        }));
        let snapshot = runtime.diagnostics().current_snapshot();
        let input_pass = snapshot
            .input_pass
            .as_ref()
            .expect("keyboard input without a text focus should still publish an input pass");
        assert!(!input_pass.input_state_changed);
        assert_eq!(input_pass.command_count, 0);
        assert_eq!(input_pass.callback_count, 0);
        assert_eq!(input_pass.invalidation_count, 0);
    }

    fn element_ids(ids: &[&str]) -> ElementIdSet {
        ids.iter().map(|id| NodeId::new(*id)).collect()
    }
}
