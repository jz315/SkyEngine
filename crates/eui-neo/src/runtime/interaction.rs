use super::tree::z_order_is_stable;
use super::*;
use crate::DirtyFlags;

#[derive(Debug, Clone)]
pub(super) enum UiEventCommand {
    Press {
        id: String,
        event: PointerEvent,
        frame: LayoutRect,
    },
    Click {
        id: String,
    },
    ContextMenu {
        id: String,
        event: PointerEvent,
        frame: LayoutRect,
    },
    Drag {
        id: String,
        event: DragEvent,
    },
    TextInput {
        id: String,
        event: KeyboardEvent,
    },
    Scroll {
        id: String,
        event: ScrollEvent,
    },
    FocusChanged {
        id: String,
        focused: bool,
    },
}

impl Runtime {
    pub fn focused_id(&self) -> Option<&str> {
        self.input.owners.keyboard_focus.as_deref()
    }

    pub fn text_focused_id(&self) -> Option<&str> {
        self.input.owners.text_focus.as_deref()
    }

    pub fn has_keyboard_capture(&self) -> bool {
        self.input.owners.keyboard_focus.is_some()
            || self.input.owners.text_focus.is_some()
            || self.input.owners.ime_owner.is_some()
    }

    pub fn focused_ime_rect(&self) -> Option<LayoutRect> {
        let element = self.find(self.input.owners.ime_owner.as_deref()?)?;
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

    pub fn response(&self, id: &str) -> Response {
        self.input
            .responses
            .get(self.resolve_id_ref(id).as_ref())
            .copied()
            .unwrap_or_default()
    }

    pub fn interaction(&self, id: &str) -> InteractionState {
        self.input
            .interactions
            .get(self.resolve_id_ref(id).as_ref())
            .copied()
            .unwrap_or_default()
    }

    pub fn update_pointer(&mut self, event: PointerEvent) -> bool {
        let pointer_pass = self.collect_pointer_event_pass(event);
        self.commit_pointer_event_pass(pointer_pass)
    }

    pub(super) fn collect_pointer_event_pass(&mut self, event: PointerEvent) -> PointerEventPass {
        let position = event.position();
        let delta = event.delta();
        if position.is_some() {
            self.input.pointer_position = position;
        }
        let hit_id = hit_test_interactive(&self.tree.roots, position);
        let focus_target = event
            .pressed_this_frame
            .then(|| hit_test_focusable(&self.tree.roots, position));
        if event.pressed_this_frame {
            self.input.owners.pointer_active = hit_id.clone();
            self.input.owners.pointer_capture = hit_id.clone();
            self.input.drag_origin = position;
        }

        let captured_id = self.input.owners.pointer_capture.clone();
        let hover_id = captured_id.clone().or(hit_id.clone());
        self.input.owners.pointer_hover = hover_id.clone();
        let mut ids = FxHashSet::default();
        ids.extend(self.input.interactions.keys().cloned());
        if let Some(id) = captured_id.as_ref() {
            ids.insert(id.clone());
        }
        if let Some(id) = hit_id.as_ref() {
            ids.insert(id.clone());
        }

        let mut commands = Vec::new();
        if event.right_pressed_this_frame {
            if let Some(target_id) = hit_id.as_deref() {
                let frame = self
                    .find(target_id)
                    .map(|element| element.frame)
                    .unwrap_or_default();
                commands.push(UiEventCommand::ContextMenu {
                    id: target_id.to_string(),
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
            let active = captured_id.as_deref() == Some(id.as_ref());
            let hovered = hover_id.as_deref() == Some(id.as_ref());
            let pressed = active && event.down;
            let press_started = active && event.pressed_this_frame;
            let released = active && event.released_this_frame;
            let clicked = released && hit_id.as_deref() == Some(id.as_ref());
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
                    .find(&id)
                    .map(|element| element.frame)
                    .unwrap_or_default();
                commands.push(UiEventCommand::Press {
                    id: id.clone(),
                    event,
                    frame,
                });
            }
            if clicked {
                commands.push(UiEventCommand::Click { id: id.clone() });
            }
            if pressed
                && (delta != [0.0, 0.0] || dragging)
                && self.input.callbacks.on_drag.contains_key(&id)
            {
                let [x, y] = position.unwrap_or_default();
                self.input.owners.drag_owner = Some(id.clone());
                commands.push(UiEventCommand::Drag {
                    id: id.clone(),
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
            self.input.owners.pointer_active = None;
            self.input.owners.pointer_capture = None;
            self.input.owners.drag_owner = None;
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

    pub(super) fn commit_pointer_event_pass(&mut self, pass: PointerEventPass) -> bool {
        let mut changed = pass.changed;
        if let Some(focus_target) = pass.focus_target {
            changed |= self.set_focused_id(focus_target);
        }
        if changed {
            self.mark_render_dirty();
        }
        self.input.interactions = pass.interactions;
        self.input.responses = pass.responses;
        let callbacks_changed = self.execute_event_commands(pass.commands);
        changed |= callbacks_changed;
        changed
    }

    pub fn update_scroll(&mut self, event: ScrollEvent) -> bool {
        if !event.active() {
            return false;
        }
        let target = hit_test(&self.tree.roots, self.input.pointer_position, |element| {
            self.input.callbacks.on_scroll.contains_key(&element.id) && !element.disabled
        });
        let Some(target) = target else {
            return false;
        };
        self.input.owners.scroll_owner = Some(target.clone());
        self.execute_event_commands(vec![UiEventCommand::Scroll { id: target, event }])
    }

    pub fn update_keyboard(&mut self, event: KeyboardEvent) -> bool {
        if !event.has_input() {
            return false;
        }
        let Some(focused_id) = self.input.owners.text_focus.clone() else {
            return false;
        };
        self.execute_event_commands(vec![UiEventCommand::TextInput {
            id: focused_id,
            event,
        }])
    }

    pub(super) fn update_events_and_timers(
        &mut self,
        pointer: PointerEvent,
        scroll: ScrollEvent,
        keyboard: KeyboardEvent,
        delta_seconds: f32,
    ) -> bool {
        self.timing.clock_seconds += f64::from(delta_seconds.max(0.0));
        let mut changed = self.update_pointer(pointer);
        changed |= self.update_scroll(scroll);
        changed |= self.update_keyboard(keyboard);
        changed |= self.tick_timers(delta_seconds);
        changed
    }

    fn set_focused_id(&mut self, focused: Option<String>) -> bool {
        if self.input.owners.keyboard_focus == focused {
            return false;
        }
        let old = self.input.owners.keyboard_focus.clone();
        self.input.owners.keyboard_focus = focused.clone();
        self.input.owners.text_focus = focused
            .as_ref()
            .filter(|id| self.input.callbacks.on_text_input.contains_key(id.as_str()))
            .cloned();
        self.input.owners.ime_owner = self.input.owners.text_focus.clone();

        let mut commands = Vec::new();
        if let Some(old) = old {
            commands.push(UiEventCommand::FocusChanged {
                id: old,
                focused: false,
            });
        }
        if let Some(new) = focused {
            commands.push(UiEventCommand::FocusChanged {
                id: new,
                focused: true,
            });
        }
        self.execute_event_commands(commands)
    }

    fn execute_event_commands(&mut self, commands: Vec<UiEventCommand>) -> bool {
        let mut changed = false;
        for command in commands {
            match command {
                UiEventCommand::Press { id, event, frame } => {
                    if let Some(callback) = self.input.callbacks.on_press.get_mut(&id) {
                        callback(event, frame);
                        self.record_invalidation(Invalidation::event(
                            id,
                            "press",
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        ));
                        changed = true;
                    }
                }
                UiEventCommand::Click { id } => {
                    if let Some(callback) = self.input.callbacks.on_click.get_mut(&id) {
                        callback();
                        self.record_invalidation(Invalidation::event(
                            id,
                            "click",
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        ));
                        changed = true;
                    }
                }
                UiEventCommand::ContextMenu { id, event, frame } => {
                    if let Some(callback) = self.input.callbacks.on_context_menu.get_mut(&id) {
                        callback(event, frame);
                        self.record_invalidation(Invalidation::event(
                            id,
                            "context_menu",
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        ));
                        changed = true;
                    }
                }
                UiEventCommand::Drag { id, event } => {
                    if let Some(callback) = self.input.callbacks.on_drag.get_mut(&id) {
                        callback(event);
                        self.record_invalidation(Invalidation::event(
                            id,
                            "drag",
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        ));
                        changed = true;
                    }
                }
                UiEventCommand::TextInput { id, event } => {
                    if let Some(callback) = self.input.callbacks.on_text_input.get_mut(&id) {
                        callback(event);
                        self.record_invalidation(Invalidation::event(
                            id,
                            "text_input",
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        ));
                        changed = true;
                    }
                }
                UiEventCommand::Scroll { id, event } => {
                    if let Some(callback) = self.input.callbacks.on_scroll.get_mut(&id) {
                        callback(event);
                        self.record_invalidation(Invalidation::event(
                            id,
                            "scroll",
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        ));
                        changed = true;
                    }
                }
                UiEventCommand::FocusChanged { id, focused } => {
                    if let Some(callback) = self.input.callbacks.on_focus_changed.get_mut(&id) {
                        callback(focused);
                        self.record_invalidation(Invalidation::event(
                            id,
                            "focus",
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        ));
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.mark_compose_dirty();
        }
        changed
    }
}

pub(super) struct PointerEventPass {
    interactions: FxHashMap<String, InteractionState>,
    responses: FxHashMap<String, Response>,
    commands: Vec<UiEventCommand>,
    changed: bool,
    focus_target: Option<Option<String>>,
}

pub(super) fn hit_test_interactive(
    elements: &[Element],
    position: Option<[f32; 2]>,
) -> Option<String> {
    hit_test(elements, position, |element| {
        element.interactive && !element.disabled
    })
}

pub(super) fn hit_test_focusable(
    elements: &[Element],
    position: Option<[f32; 2]>,
) -> Option<String> {
    hit_test(elements, position, |element| {
        element.focusable && !element.disabled
    })
}

pub(super) fn hit_test(
    elements: &[Element],
    position: Option<[f32; 2]>,
    predicate: impl Fn(&Element) -> bool,
) -> Option<String> {
    let position = position?;
    hit_test_elements(elements, position, None, &predicate).map(|element| element.id.clone())
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

pub(super) fn state_without_changed(mut state: InteractionState) -> InteractionState {
    state.changed = false;
    state
}
