use rustc_hash::{FxHashMap, FxHashSet, FxHasher};

use super::dsl::UiCallbacks;
use super::event::InteractionState;
use super::fonts::FontRef;
use super::layout::layout_roots_with_text_system;
use super::skin::{NeoSkin, SkinRegistry};
use super::text_measure::{DefaultTextSystem, TextSystem};
use super::Color;
use super::{
    AnimProperty, AnimatedValue, Border, DragEvent, Element, ElementKind, KeyboardEvent,
    LayoutRect, Lerp, Motion, PointerEvent, Response, Screen, ScrollEvent, Shadow, SmoothedValue,
    Transform, Transition, Ui,
};
use std::hash::{Hash, Hasher};

/// Compact structure snapshot used to detect tree-level changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementSnapshot {
    pub id: String,
    pub kind: ElementKind,
    pub z_index: i32,
    pub clip: bool,
    pub child_count: usize,
    pub signature: u64,
}

/// Complete host-provided input snapshot for one UI frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameInput {
    pub screen: Screen,
    pub delta_seconds: f32,
    pub pointer: PointerEvent,
    pub scroll: ScrollEvent,
    pub keyboard: KeyboardEvent,
}

impl FrameInput {
    pub fn new(screen: Screen, delta_seconds: f32) -> Self {
        Self {
            screen,
            delta_seconds,
            ..Self::default()
        }
    }

    pub fn pointer(mut self, value: PointerEvent) -> Self {
        self.pointer = value;
        self
    }

    pub fn scroll(mut self, value: ScrollEvent) -> Self {
        self.scroll = value;
        self
    }

    pub fn keyboard(mut self, value: KeyboardEvent) -> Self {
        self.keyboard = value;
        self
    }
}

/// Host-facing result of a completed UI frame.
#[derive(Debug, Clone)]
pub struct Frame {
    pub screen: Screen,
    pub draw_list: super::draw::UiDrawList,
    pub needs_render: bool,
    pub needs_compose: bool,
    pub full_redraw: bool,
    pub focused_ime_rect: Option<LayoutRect>,
}

impl Frame {
    pub fn draw_list(&self) -> &super::draw::UiDrawList {
        &self.draw_list
    }
}

/// Return value from [`Runtime::frame`].
#[derive(Debug, Clone)]
pub struct FrameResult<R = ()> {
    pub value: R,
    pub frame: Frame,
}

impl<R> FrameResult<R> {
    pub fn into_parts(self) -> (R, Frame) {
        (self.value, self.frame)
    }
}

/// EUI-NEO-style runtime shell.
///
/// This first slice owns composition, layout, and structure tracking. Event,
/// renderer, and backend integration are layered on top in later milestones.
pub struct Runtime {
    page_id: String,
    roots: Vec<Element>,
    structure: Vec<ElementSnapshot>,
    screen: Screen,
    interactions: FxHashMap<String, InteractionState>,
    responses: FxHashMap<String, Response>,
    callbacks: UiCallbacks,
    active_id: Option<String>,
    focused_id: Option<String>,
    drag_origin: Option<[f32; 2]>,
    pointer_position: Option<[f32; 2]>,
    timers: FxHashMap<String, TimerState>,
    animations: FxHashMap<String, ElementAnimation>,
    frame_targets: FxHashMap<String, FrameTargetState>,
    skins: SkinRegistry,
    needs_render: bool,
    needs_compose: bool,
    full_redraw: bool,
    text_system: Box<dyn TextSystem>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("page_id", &self.page_id)
            .field("roots", &self.roots)
            .field("structure", &self.structure)
            .field("screen", &self.screen)
            .field("needs_render", &self.needs_render)
            .field("needs_compose", &self.needs_compose)
            .field("full_redraw", &self.full_redraw)
            .finish_non_exhaustive()
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new("")
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct TimerState {
    seconds: f32,
    elapsed: f32,
    seen: bool,
    active: bool,
}

#[derive(Debug, Clone, Copy)]
struct FrameTargetState {
    frame: LayoutRect,
    seen: bool,
}

#[derive(Debug, Default)]
struct ElementAnimation {
    seen: bool,
    hover_blend: SmoothedValue,
    press_blend: SmoothedValue,
    frame: Option<AnimatedValue<LayoutRect>>,
    color: Option<AnimatedValue<Color>>,
    text_color: Option<AnimatedValue<Color>>,
    radius: Option<AnimatedValue<f32>>,
    blur: Option<AnimatedValue<f32>>,
    opacity: Option<AnimatedValue<f32>>,
    border: Option<AnimatedValue<Border>>,
    shadow: Option<AnimatedValue<Shadow>>,
    transform: Option<AnimatedValue<Transform>>,
}

impl ElementAnimation {
    fn is_active(&self) -> bool {
        self.hover_blend.is_moving()
            || self.press_blend.is_moving()
            || self.frame.as_ref().is_some_and(AnimatedValue::is_animating)
            || self.color.as_ref().is_some_and(AnimatedValue::is_animating)
            || self
                .text_color
                .as_ref()
                .is_some_and(AnimatedValue::is_animating)
            || self
                .radius
                .as_ref()
                .is_some_and(AnimatedValue::is_animating)
            || self.blur.as_ref().is_some_and(AnimatedValue::is_animating)
            || self
                .opacity
                .as_ref()
                .is_some_and(AnimatedValue::is_animating)
            || self
                .border
                .as_ref()
                .is_some_and(AnimatedValue::is_animating)
            || self
                .shadow
                .as_ref()
                .is_some_and(AnimatedValue::is_animating)
            || self
                .transform
                .as_ref()
                .is_some_and(AnimatedValue::is_animating)
    }
}

impl Runtime {
    pub fn new(page_id: impl Into<String>) -> Self {
        Self::with_text_system(page_id, DefaultTextSystem::new())
    }

    pub fn with_text_system(
        page_id: impl Into<String>,
        text_system: impl TextSystem + 'static,
    ) -> Self {
        Self {
            page_id: page_id.into(),
            roots: Vec::new(),
            structure: Vec::new(),
            screen: Screen::default(),
            interactions: FxHashMap::default(),
            responses: FxHashMap::default(),
            callbacks: UiCallbacks::default(),
            active_id: None,
            focused_id: None,
            drag_origin: None,
            pointer_position: None,
            timers: FxHashMap::default(),
            animations: FxHashMap::default(),
            frame_targets: FxHashMap::default(),
            skins: SkinRegistry::default(),
            needs_render: true,
            needs_compose: false,
            full_redraw: true,
            text_system: Box::new(text_system),
        }
    }

    pub fn text_system_mut(&mut self) -> &mut dyn TextSystem {
        self.text_system.as_mut()
    }

    pub fn register_font(&mut self, font: &FontRef, bytes: &[u8]) {
        self.text_system.register_font(font, bytes);
        self.needs_render = true;
        self.needs_compose = true;
    }

    pub fn register_skin(&mut self, skin: NeoSkin) {
        self.skins.register(skin);
        self.needs_render = true;
        self.needs_compose = true;
    }

    pub fn skins(&self) -> &SkinRegistry {
        &self.skins
    }

    pub fn skins_mut(&mut self) -> &mut SkinRegistry {
        self.needs_render = true;
        self.needs_compose = true;
        &mut self.skins
    }

    pub(crate) fn resolve_image_ref(&self, image: &super::ImageRef) -> super::ImageRef {
        self.skins.resolve_image(image)
    }

    pub(crate) fn resolve_font_ref(&self, font: &FontRef) -> FontRef {
        self.skins.resolve_font(font)
    }

    pub fn page_id(&self) -> &str {
        &self.page_id
    }

    pub fn screen(&self) -> Screen {
        self.screen
    }

    pub fn roots(&self) -> &[Element] {
        &self.roots
    }

    pub fn draw_list(&self) -> super::draw::UiDrawList {
        super::draw::build_draw_list(self)
    }

    pub fn needs_render(&self) -> bool {
        self.needs_render
    }

    pub fn full_redraw(&self) -> bool {
        self.full_redraw
    }

    pub fn needs_compose(&self) -> bool {
        self.needs_compose
    }

    pub fn clear_needs_compose(&mut self) {
        self.needs_compose = false;
    }

    pub fn focused_id(&self) -> Option<&str> {
        self.focused_id.as_deref()
    }

    pub fn focused_ime_rect(&self) -> Option<LayoutRect> {
        let element = self.find(self.focused_id.as_deref()?)?;
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

    pub fn mark_rendered(&mut self) {
        self.needs_render = false;
        self.full_redraw = false;
    }

    pub fn mark_full_redraw(&mut self) {
        self.needs_render = true;
        self.full_redraw = true;
    }

    pub fn compose(&mut self, width: f32, height: f32, compose: impl FnOnce(&mut Ui, Screen)) {
        self.needs_compose = false;
        let screen = Screen { width, height };
        let mut ui = Ui::new(self.page_id.clone());
        ui.set_skins(self.skins.clone());
        ui.set_focused_id(self.focused_id.clone());
        for (id, response) in &self.responses {
            ui.set_response(id.clone(), *response);
        }
        compose(&mut ui, screen);
        let (mut roots, callbacks) = ui.into_parts();
        layout_roots_with_text_system(&mut roots, width, height, self.text_system.as_mut());
        let next_structure = collect_structure(&roots, self.structure.len());
        if next_structure != self.structure || self.screen != screen {
            self.needs_render = true;
            self.full_redraw = true;
        }
        self.structure = next_structure;
        self.screen = screen;
        self.roots = roots;
        self.callbacks = callbacks;
    }

    pub fn current_frame(&self) -> Frame {
        Frame {
            screen: self.screen,
            draw_list: self.draw_list(),
            needs_render: self.needs_render,
            needs_compose: self.needs_compose,
            full_redraw: self.full_redraw,
            focused_ime_rect: self.focused_ime_rect(),
        }
    }

    pub fn frame<R>(
        &mut self,
        input: FrameInput,
        compose: impl FnOnce(&mut Ui, Screen) -> R,
    ) -> FrameResult<R> {
        self.update_events_and_timers(
            input.pointer,
            input.scroll,
            input.keyboard,
            input.delta_seconds,
        );
        let mut value = None;
        self.compose(input.screen.width, input.screen.height, |ui, screen| {
            value = Some(compose(ui, screen));
        });
        self.tick_animations(input.delta_seconds);
        FrameResult {
            value: value.expect("frame compose closure did not run"),
            frame: self.current_frame(),
        }
    }

    pub fn find(&self, id: &str) -> Option<&Element> {
        let id = self.resolve_id(id);
        self.roots.iter().find_map(|root| find_element(root, &id))
    }

    pub fn response(&self, id: &str) -> Response {
        self.responses
            .get(&self.resolve_id(id))
            .copied()
            .unwrap_or_default()
    }

    pub fn interaction(&self, id: &str) -> InteractionState {
        self.interactions
            .get(&self.resolve_id(id))
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn animated_frame(&self, element: &Element) -> LayoutRect {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.frame.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.frame)
    }

    pub(crate) fn animated_color(&self, element: &Element) -> Color {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.color.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or_else(|| self.state_color_target(element))
    }

    pub(crate) fn animated_text_color(&self, element: &Element) -> Color {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.text_color.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.text_color)
    }

    pub(crate) fn animated_radius(&self, element: &Element) -> f32 {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.radius.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.radius)
    }

    pub(crate) fn animated_blur(&self, element: &Element) -> f32 {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.blur.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.blur)
    }

    pub(crate) fn animated_opacity(&self, element: &Element) -> f32 {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.opacity.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.opacity)
    }

    pub(crate) fn animated_border(&self, element: &Element) -> Border {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.border.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.border)
    }

    pub(crate) fn animated_shadow(&self, element: &Element) -> Shadow {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.shadow.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.shadow)
    }

    pub(crate) fn animated_transform(&self, element: &Element) -> Transform {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.transform.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.transform)
    }

    pub(crate) fn hover_blend_for_source(&self, id: &str) -> Option<f32> {
        let id = self.resolve_id(id);
        let element = self.find(&id)?;
        if !matches!(
            element.kind,
            ElementKind::Rect | ElementKind::Polygon | ElementKind::Image | ElementKind::NineSlice
        ) {
            return None;
        }
        self.animations
            .get(&id)
            .map(|animation| animation.hover_blend.current())
    }

    pub(crate) fn press_blend_for_source(&self, id: &str) -> Option<(f32, LayoutRect)> {
        let id = self.resolve_id(id);
        let element = self.find(&id)?;
        if !matches!(
            element.kind,
            ElementKind::Rect | ElementKind::Polygon | ElementKind::Image | ElementKind::NineSlice
        ) {
            return None;
        }
        let animation = self.animations.get(&id)?;
        let frame = animation
            .frame
            .as_ref()
            .map(AnimatedValue::current)
            .unwrap_or(element.frame);
        Some((animation.press_blend.current(), frame))
    }

    pub fn update_pointer(&mut self, event: PointerEvent) -> bool {
        let position = event.position();
        let delta = event.delta();
        if position.is_some() {
            self.pointer_position = position;
        }
        let hit_id = hit_test_interactive(&self.roots, position);
        if event.pressed_this_frame {
            self.set_focused_id(hit_test_focusable(&self.roots, position));
        }
        if event.right_pressed_this_frame {
            if let Some(target_id) = hit_id.as_deref() {
                let frame = self
                    .find(target_id)
                    .map(|element| element.frame)
                    .unwrap_or_default();
                if let Some(callback) = self.callbacks.on_context_menu.get_mut(target_id) {
                    callback(event, frame);
                    self.needs_compose = true;
                    self.needs_render = true;
                }
            }
        }
        if event.pressed_this_frame {
            self.active_id = hit_id.clone();
            self.drag_origin = position;
        }

        let captured_id = self.active_id.clone();
        let hover_id = captured_id.clone().or(hit_id.clone());
        let mut ids = FxHashSet::default();
        ids.extend(self.interactions.keys().cloned());
        if let Some(id) = captured_id.as_ref() {
            ids.insert(id.clone());
        }
        if let Some(id) = hit_id.as_ref() {
            ids.insert(id.clone());
        }

        let mut changed = false;
        let mut next = FxHashMap::default();
        let mut responses = FxHashMap::default();
        for id in ids {
            let previous = self.interactions.get(&id).copied().unwrap_or_default();
            let active = captured_id.as_deref() == Some(id.as_str());
            let hovered = hover_id.as_deref() == Some(id.as_str());
            let pressed = active && event.down;
            let press_started = active && event.pressed_this_frame;
            let released = active && event.released_this_frame;
            let clicked = released && hit_id.as_deref() == Some(id.as_str());
            let drag_start = if press_started {
                position.unwrap_or(previous.drag_start)
            } else {
                previous.drag_start
            };
            let drag_total = if active {
                match (position, self.drag_origin) {
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
                if let Some(callback) = self.callbacks.on_press.get_mut(&id) {
                    callback(event, frame);
                    self.needs_compose = true;
                    self.needs_render = true;
                }
            }
            if clicked {
                if let Some(callback) = self.callbacks.on_click.get_mut(&id) {
                    callback();
                    self.needs_compose = true;
                    self.needs_render = true;
                }
            }
            if pressed
                && (delta != [0.0, 0.0] || dragging)
                && self.callbacks.on_drag.contains_key(&id)
            {
                if let Some(callback) = self.callbacks.on_drag.get_mut(&id) {
                    let [x, y] = position.unwrap_or_default();
                    callback(DragEvent {
                        x,
                        y,
                        delta_x: delta[0],
                        delta_y: delta[1],
                        total_x: drag_total[0],
                        total_y: drag_total[1],
                    });
                    self.needs_compose = true;
                    self.needs_render = true;
                }
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
            self.active_id = None;
            self.drag_origin = None;
        }
        if changed {
            self.needs_render = true;
        }
        self.interactions = next;
        self.responses = responses;
        changed
    }

    pub fn update_scroll(&mut self, event: ScrollEvent) -> bool {
        if !event.active() {
            return false;
        }
        let target = hit_test(&self.roots, self.pointer_position, |element| {
            self.callbacks.on_scroll.contains_key(&element.id) && !element.disabled
        });
        let Some(target) = target else {
            return false;
        };
        let Some(callback) = self.callbacks.on_scroll.get_mut(&target) else {
            return false;
        };
        callback(event);
        self.needs_compose = true;
        self.needs_render = true;
        true
    }

    pub fn update_keyboard(&mut self, event: KeyboardEvent) -> bool {
        if !event.has_input() {
            return false;
        }
        let Some(focused_id) = self.focused_id.clone() else {
            return false;
        };
        let Some(callback) = self.callbacks.on_text_input.get_mut(&focused_id) else {
            return false;
        };
        callback(event);
        self.needs_compose = true;
        self.needs_render = true;
        true
    }

    pub fn tick_timers(&mut self, delta_seconds: f32) -> bool {
        for state in self.timers.values_mut() {
            state.seen = false;
        }
        let mut fired = Vec::new();
        collect_timer_ids(&self.roots, &mut fired);
        let mut changed = false;
        for (id, seconds) in fired {
            let state = self.timers.entry(id.clone()).or_default();
            state.seen = true;
            if !state.active || (state.seconds - seconds).abs() > 0.001 {
                state.seconds = seconds;
                state.elapsed = 0.0;
                state.active = true;
            }
            state.elapsed += delta_seconds.max(0.0);
            if state.active && state.elapsed >= state.seconds {
                state.active = false;
                if let Some(callback) = self.callbacks.on_timer.get_mut(&id) {
                    callback();
                    self.needs_compose = true;
                    self.needs_render = true;
                    changed = true;
                }
            } else if state.active {
                self.needs_render = true;
            }
        }
        self.timers.retain(|_, state| state.seen);
        changed
    }

    pub fn tick_animations(&mut self, delta_seconds: f32) -> bool {
        for animation in self.animations.values_mut() {
            animation.seen = false;
        }
        for target in self.frame_targets.values_mut() {
            target.seen = false;
        }

        let mut changed = false;
        let roots = self.roots.clone();
        for root in ordered_element_clones(&roots) {
            changed |= self.tick_element_animation_tree(&root, delta_seconds, false);
        }
        self.animations.retain(|_, animation| animation.seen);
        self.frame_targets.retain(|_, target| target.seen);

        let active = self.animations.values().any(ElementAnimation::is_active);
        if changed || active {
            self.needs_render = true;
        }
        changed
    }

    pub fn update_input_snapshot(
        &mut self,
        pointer: PointerEvent,
        scroll: ScrollEvent,
        keyboard: KeyboardEvent,
        delta_seconds: f32,
    ) -> bool {
        let mut changed = self.update_events_and_timers(pointer, scroll, keyboard, delta_seconds);
        changed |= self.tick_animations(delta_seconds);
        changed
    }

    pub fn update_events_and_timers(
        &mut self,
        pointer: PointerEvent,
        scroll: ScrollEvent,
        keyboard: KeyboardEvent,
        delta_seconds: f32,
    ) -> bool {
        let mut changed = self.update_pointer(pointer);
        changed |= self.update_scroll(scroll);
        changed |= self.update_keyboard(keyboard);
        changed |= self.tick_timers(delta_seconds);
        changed
    }

    fn tick_element_animation_tree(
        &mut self,
        element: &Element,
        delta_seconds: f32,
        ancestor_frame_changed: bool,
    ) -> bool {
        let frame_target_changed = self.update_frame_target(element);
        let mut changed =
            self.tick_element_animation(element, delta_seconds, ancestor_frame_changed);
        let child_ancestor_frame_changed = ancestor_frame_changed || frame_target_changed;
        for child in ordered_element_clones(&element.children) {
            changed |= self.tick_element_animation_tree(
                &child,
                delta_seconds,
                child_ancestor_frame_changed,
            );
        }
        changed
    }

    fn update_frame_target(&mut self, element: &Element) -> bool {
        let entry = self
            .frame_targets
            .entry(element.id.clone())
            .or_insert(FrameTargetState {
                frame: element.frame,
                seen: true,
            });
        let changed = !LayoutRect::close_enough(entry.frame, element.frame);
        entry.frame = element.frame;
        entry.seen = true;
        changed
    }

    fn tick_element_animation(
        &mut self,
        element: &Element,
        delta_seconds: f32,
        snap_frame: bool,
    ) -> bool {
        let interaction = self.interaction(&element.id);
        let animation = self.animations.entry(element.id.clone()).or_default();
        animation.seen = true;

        let mut changed = false;
        let transition = element.transition;
        let animate_frame = !snap_frame && should_animate_frame(element);
        let animate_color = should_animate(element, AnimProperty::COLOR);
        let animate_text_color = should_animate(element, AnimProperty::TEXT_COLOR);
        let animate_opacity = should_animate(element, AnimProperty::OPACITY);
        let animate_radius = should_animate(element, AnimProperty::RADIUS);
        let animate_border = should_animate(element, AnimProperty::BORDER);
        let animate_shadow = should_animate(element, AnimProperty::SHADOW);
        let animate_blur = should_animate(element, AnimProperty::BLUR);
        let animate_transform = should_animate(element, AnimProperty::TRANSFORM);

        match element.kind {
            ElementKind::Row | ElementKind::Column | ElementKind::Stack => {
                changed |= sync_animated(
                    &mut animation.opacity,
                    element.opacity,
                    transition,
                    animate_opacity,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.transform,
                    element.transform,
                    transition,
                    animate_transform,
                    delta_seconds,
                );
            }
            ElementKind::Rect => {
                changed |= update_state_blends(animation, element, interaction, delta_seconds);
                let target_color = state_color_target(
                    element,
                    interaction,
                    Some((
                        animation.hover_blend.current(),
                        animation.press_blend.current(),
                    )),
                );
                changed |= sync_animated(
                    &mut animation.frame,
                    element.frame,
                    transition,
                    animate_frame,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.color,
                    target_color,
                    transition,
                    animate_color,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.radius,
                    element.radius,
                    transition,
                    animate_radius,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.blur,
                    element.blur,
                    transition,
                    animate_blur,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.opacity,
                    element.opacity,
                    transition,
                    animate_opacity,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.border,
                    element.border,
                    transition,
                    animate_border,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.shadow,
                    element.shadow,
                    transition,
                    animate_shadow,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.transform,
                    element.transform,
                    transition,
                    animate_transform,
                    delta_seconds,
                );
            }
            ElementKind::Polygon => {
                changed |= update_state_blends(animation, element, interaction, delta_seconds);
                let target_color = state_color_target(
                    element,
                    interaction,
                    Some((
                        animation.hover_blend.current(),
                        animation.press_blend.current(),
                    )),
                );
                changed |= sync_animated(
                    &mut animation.frame,
                    element.frame,
                    transition,
                    animate_frame,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.color,
                    target_color,
                    transition,
                    animate_color,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.opacity,
                    element.opacity,
                    transition,
                    animate_opacity,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.transform,
                    element.transform,
                    transition,
                    animate_transform,
                    delta_seconds,
                );
            }
            ElementKind::Text => {
                changed |= sync_animated(
                    &mut animation.frame,
                    element.frame,
                    transition,
                    animate_frame,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.text_color,
                    element.text_color,
                    transition,
                    animate_text_color,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.opacity,
                    element.opacity,
                    transition,
                    animate_opacity,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.transform,
                    element.transform,
                    transition,
                    animate_transform,
                    delta_seconds,
                );
            }
            ElementKind::Image | ElementKind::NineSlice => {
                changed |= update_state_blends(animation, element, interaction, delta_seconds);
                let target_color = state_color_target(
                    element,
                    interaction,
                    Some((
                        animation.hover_blend.current(),
                        animation.press_blend.current(),
                    )),
                );
                changed |= sync_animated(
                    &mut animation.frame,
                    element.frame,
                    transition,
                    animate_frame,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.color,
                    target_color,
                    transition,
                    animate_color,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.radius,
                    element.radius,
                    transition,
                    animate_radius,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.opacity,
                    element.opacity,
                    transition,
                    animate_opacity,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.transform,
                    element.transform,
                    transition,
                    animate_transform,
                    delta_seconds,
                );
            }
        }

        changed
    }

    fn state_color_target(&self, element: &Element) -> Color {
        let animation = self.animations.get(&element.id);
        let blends = animation.map(|animation| {
            (
                animation.hover_blend.current(),
                animation.press_blend.current(),
            )
        });
        state_color_target(element, self.interaction(&element.id), blends)
    }

    fn set_focused_id(&mut self, focused: Option<String>) {
        if self.focused_id == focused {
            return;
        }
        let old = self.focused_id.clone();
        self.focused_id = focused.clone();
        if let Some(old) = old {
            if let Some(callback) = self.callbacks.on_focus_changed.get_mut(&old) {
                callback(false);
            }
        }
        if let Some(new) = focused {
            if let Some(callback) = self.callbacks.on_focus_changed.get_mut(&new) {
                callback(true);
            }
        }
        self.needs_compose = true;
        self.needs_render = true;
    }

    fn resolve_id(&self, id: &str) -> String {
        if id.is_empty() || self.page_id.is_empty() {
            return id.to_string();
        }
        if is_resolved_id(id, &self.page_id) {
            id.to_string()
        } else {
            let mut resolved = String::with_capacity(self.page_id.len() + 1 + id.len());
            resolved.push_str(&self.page_id);
            resolved.push('.');
            resolved.push_str(id);
            resolved
        }
    }
}

fn is_resolved_id(id: &str, page_id: &str) -> bool {
    id.len() > page_id.len()
        && id.as_bytes().get(page_id.len()) == Some(&b'.')
        && id.as_bytes().starts_with(page_id.as_bytes())
}

fn ordered_element_clones(elements: &[Element]) -> Vec<Element> {
    let mut ordered: Vec<_> = elements.iter().cloned().enumerate().collect();
    ordered.sort_by_key(|(index, element)| (element.z_index, *index));
    ordered.into_iter().map(|(_, element)| element).collect()
}

fn sync_animated<T>(
    slot: &mut Option<AnimatedValue<T>>,
    target: T,
    transition: Transition,
    animate_property: bool,
    delta_seconds: f32,
) -> bool
where
    T: super::Lerp,
{
    let Some(value) = slot.as_mut() else {
        *slot = Some(AnimatedValue::new(target));
        return true;
    };
    let mut changed = value.set_target_if(target, transition, animate_property);
    changed |= value.update(delta_seconds);
    changed
}

fn update_state_blends(
    animation: &mut ElementAnimation,
    element: &Element,
    interaction: InteractionState,
    delta_seconds: f32,
) -> bool {
    let interactive = element.interactive && !element.disabled;
    let state_colors_visible = element.has_state_colors
        && (!color_close_enough(element.color, element.hover_color)
            || !color_close_enough(element.color, element.pressed_color));
    let hover_speed = if element.smooth_state_colors {
        9.0
    } else {
        0.0
    };
    let press_speed = if element.smooth_state_colors {
        16.0
    } else {
        0.0
    };
    let hover_target = (interactive && state_colors_visible && interaction.hovered) as u8 as f32;
    let press_target = (interactive && state_colors_visible && interaction.pressed) as u8 as f32;
    let mut changed = animation
        .hover_blend
        .update_to(hover_target, hover_speed, delta_seconds);
    changed |= animation
        .press_blend
        .update_to(press_target, press_speed, delta_seconds);
    changed
}

fn should_animate(element: &Element, property: AnimProperty) -> bool {
    element.transition.enabled && element.transition.properties.contains(property)
}

fn should_animate_frame(element: &Element) -> bool {
    should_animate(element, AnimProperty::FRAME) && element.explicit_frame_animation
}

fn state_color_target(
    element: &Element,
    interaction: InteractionState,
    blends: Option<(f32, f32)>,
) -> Color {
    let interactive = element.interactive && !element.disabled;
    let state_colors_visible = element.has_state_colors
        && (!color_close_enough(element.color, element.hover_color)
            || !color_close_enough(element.color, element.pressed_color));
    if !interactive || !state_colors_visible {
        return element.color;
    }

    let (hover, press) = blends.unwrap_or((
        interaction.hovered as u8 as f32,
        interaction.pressed as u8 as f32,
    ));
    let hover_color = mix_color(element.color, element.hover_color, hover);
    mix_color(hover_color, element.pressed_color, press)
}

fn mix_color(from: Color, to: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    let inverse = 1.0 - amount;
    Color {
        r: from.r * inverse + to.r * amount,
        g: from.g * inverse + to.g * amount,
        b: from.b * inverse + to.b * amount,
        a: from.a * inverse + to.a * amount,
    }
}

fn color_close_enough(left: Color, right: Color) -> bool {
    (left.r - right.r).abs() <= 0.001
        && (left.g - right.g).abs() <= 0.001
        && (left.b - right.b).abs() <= 0.001
        && (left.a - right.a).abs() <= 0.001
}

fn collect_structure(roots: &[Element], previous_len: usize) -> Vec<ElementSnapshot> {
    let mut snapshots = Vec::with_capacity(previous_len);
    for root in roots {
        collect_element_structure(root, &mut snapshots);
    }
    snapshots
}

fn collect_element_structure(element: &Element, snapshots: &mut Vec<ElementSnapshot>) {
    snapshots.push(ElementSnapshot {
        id: element.id.clone(),
        kind: element.kind,
        z_index: element.z_index,
        clip: element.clip,
        child_count: element.children.len(),
        signature: element_signature(element),
    });
    for child in &element.children {
        collect_element_structure(child, snapshots);
    }
}

fn find_element<'a>(element: &'a Element, id: &str) -> Option<&'a Element> {
    if element.id == id {
        return Some(element);
    }
    element
        .children
        .iter()
        .find_map(|child| find_element(child, id))
}

fn hit_test_interactive(elements: &[Element], position: Option<[f32; 2]>) -> Option<String> {
    hit_test(elements, position, |element| {
        element.interactive && !element.disabled
    })
}

fn hit_test_focusable(elements: &[Element], position: Option<[f32; 2]>) -> Option<String> {
    hit_test(elements, position, |element| {
        element.focusable && !element.disabled
    })
}

fn hit_test(
    elements: &[Element],
    position: Option<[f32; 2]>,
    predicate: impl Fn(&Element) -> bool,
) -> Option<String> {
    let position = position?;
    hit_test_elements(elements, position, None, &predicate).map(|element| element.id.clone())
}

fn hit_test_elements<'a>(
    elements: &'a [Element],
    position: [f32; 2],
    clip: Option<LayoutRect>,
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

    let mut order: Vec<usize> = (0..elements.len()).collect();
    order.sort_by_key(|&index| (elements[index].z_index, index));
    for index in order.into_iter().rev() {
        if let Some(target) = hit_test_element(&elements[index], position, clip, predicate) {
            return Some(target);
        }
    }
    None
}

fn z_order_is_stable(elements: &[Element]) -> bool {
    elements
        .windows(2)
        .all(|pair| pair[0].z_index <= pair[1].z_index)
}

fn hit_test_element<'a>(
    element: &'a Element,
    position: [f32; 2],
    clip: Option<LayoutRect>,
    predicate: &impl Fn(&Element) -> bool,
) -> Option<&'a Element> {
    if clip.is_some_and(|clip| !clip.contains(position)) {
        return None;
    }
    let next_clip = if element.clip {
        let clip = match clip {
            Some(parent) => intersect_rect(parent, element.frame)?,
            None => element.frame,
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

fn hit_contains(element: &Element, position: [f32; 2]) -> bool {
    if element.kind == ElementKind::Polygon {
        return polygon_contains(element, position);
    }
    element.frame.contains(position)
}

fn polygon_contains(element: &Element, position: [f32; 2]) -> bool {
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

fn intersect_rect(left: LayoutRect, right: LayoutRect) -> Option<LayoutRect> {
    let x0 = left.x.max(right.x);
    let y0 = left.y.max(right.y);
    let x1 = left.right().min(right.right());
    let y1 = left.bottom().min(right.bottom());
    (x1 > x0 && y1 > y0).then(|| LayoutRect::new(x0, y0, x1 - x0, y1 - y0))
}

fn state_without_changed(mut state: InteractionState) -> InteractionState {
    state.changed = false;
    state
}

fn collect_timer_ids(elements: &[Element], timers: &mut Vec<(String, f32)>) {
    for element in elements {
        if element.timer_seconds > 0.0 {
            timers.push((element.id.clone(), element.timer_seconds));
        }
        collect_timer_ids(&element.children, timers);
    }
}

fn element_signature(element: &Element) -> u64 {
    let mut hasher = FxHasher::default();
    element.kind.hash(&mut hasher);
    element.id.hash(&mut hasher);
    element.has_x.hash(&mut hasher);
    element.has_y.hash(&mut hasher);
    hash_f32(element.x, &mut hasher);
    hash_f32(element.y, &mut hasher);
    hash_size(element.width, &mut hasher);
    hash_size(element.height, &mut hasher);
    hash_f32(element.margin.left, &mut hasher);
    hash_f32(element.margin.top, &mut hasher);
    hash_f32(element.margin.right, &mut hasher);
    hash_f32(element.margin.bottom, &mut hasher);
    hash_f32(element.spacing, &mut hasher);
    element.main_align.hash(&mut hasher);
    element.cross_align.hash(&mut hasher);
    element.z_index.hash(&mut hasher);
    element.clip.hash(&mut hasher);
    hash_color(element.color, &mut hasher);
    hash_color(element.text_color, &mut hasher);
    hash_f32(element.radius, &mut hasher);
    hash_f32(element.opacity, &mut hasher);
    element.text.hash(&mut hasher);
    element.font.hash(&mut hasher);
    hash_f32(element.font_size, &mut hasher);
    hash_f32(element.line_height, &mut hasher);
    element.image.hash(&mut hasher);
    element.image_fit.hash(&mut hasher);
    hash_f32(element.slice.left, &mut hasher);
    hash_f32(element.slice.top, &mut hasher);
    hash_f32(element.slice.right, &mut hasher);
    hash_f32(element.slice.bottom, &mut hasher);
    hash_f32(element.content_inset.left, &mut hasher);
    hash_f32(element.content_inset.top, &mut hasher);
    hash_f32(element.content_inset.right, &mut hasher);
    hash_f32(element.content_inset.bottom, &mut hasher);
    element.center_mode.hash(&mut hasher);
    element.edge_mode.hash(&mut hasher);
    element.interactive.hash(&mut hasher);
    element.focusable.hash(&mut hasher);
    element.disabled.hash(&mut hasher);
    element.visual_state_source_id.hash(&mut hasher);
    element.hover_opacity_source_id.hash(&mut hasher);
    hash_f32(element.pressed_scale, &mut hasher);
    hash_f32(element.hover_hidden_opacity, &mut hasher);
    hash_f32(element.hover_visible_opacity, &mut hasher);
    element.transition.enabled.hash(&mut hasher);
    hash_f32(element.transition.duration_seconds, &mut hasher);
    hash_f32(element.transition.delay_seconds, &mut hasher);
    element.transition.ease.hash(&mut hasher);
    element.transition.properties.hash(&mut hasher);
    hash_motion(element.transition.motion, &mut hasher);
    hash_f32(element.transition.damping_ratio, &mut hasher);
    element.explicit_frame_animation.hash(&mut hasher);
    hash_f32(element.timer_seconds, &mut hasher);
    hasher.finish()
}

fn hash_motion(motion: Motion, hasher: &mut impl Hasher) {
    match motion {
        Motion::Ease => 0_u8.hash(hasher),
        Motion::Spring => 1_u8.hash(hasher),
    }
}

fn hash_size(size: super::Size, hasher: &mut impl Hasher) {
    match size {
        super::Size::Fixed(value) => {
            0_u8.hash(hasher);
            hash_f32(value, hasher);
        }
        super::Size::WrapContent => 1_u8.hash(hasher),
        super::Size::Fill => 2_u8.hash(hasher),
    }
}

fn hash_color(color: Color, hasher: &mut impl Hasher) {
    hash_f32(color.r, hasher);
    hash_f32(color.g, hasher);
    hash_f32(color.b, hasher);
    hash_f32(color.a, hasher);
}

fn hash_f32(value: f32, hasher: &mut impl Hasher) {
    value.to_bits().hash(hasher);
}

#[cfg(test)]
mod tests {
    use super::Color;
    use super::Runtime;
    use crate::expert::{UiDrawCommand, UiRectDraw};
    use crate::widgets::{button, panel, text};
    use crate::{
        Align, AnimProperty, Ease, FontRef, FrameInput, HorizontalAlign, KeyboardEvent, LayoutRect,
        PointerEvent, Screen, ScrollEvent, Size, TextMeasure, TextMeasureRequest, TextSystem,
        Transition,
    };
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[test]
    fn runtime_composes_and_lays_out_tree() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(800.0, 600.0, |ui, screen| {
            ui.stack("root")
                .size(screen.width, screen.height)
                .content(|ui| {
                    ui.text("title").text("Hello").font_size(20.0).build();
                });
        });

        let root = runtime.find("root").unwrap();
        assert_eq!(root.frame.width, 800.0);
        assert_eq!(root.frame.height, 600.0);
        assert!(runtime.find("title").is_some());
        assert!(runtime.needs_render());
        assert!(runtime.full_redraw());
    }

    #[test]
    fn font_source_reaches_draw_list() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(200.0, 80.0, |ui, _| {
            ui.text("title")
                .text("Hello")
                .font_source("ui/fonts/title.ttf")
                .build();
        });

        let draw = runtime.draw_list();
        let text = draw
            .commands()
            .iter()
            .find_map(|command| match command {
                UiDrawCommand::Text(text) => Some(text),
                _ => None,
            })
            .expect("text draw should exist");
        assert_eq!(text.font, FontRef::source("ui/fonts/title.ttf"));
    }

    #[test]
    fn injected_text_system_controls_layout_measurement() {
        #[derive(Default)]
        struct FixedTextSystem;

        impl TextSystem for FixedTextSystem {
            fn register_font(&mut self, _font: &FontRef, _bytes: &[u8]) {}

            fn measure(&mut self, _request: TextMeasureRequest<'_>) -> TextMeasure {
                TextMeasure {
                    width: 77.0,
                    height: 19.0,
                }
            }
        }

        let mut runtime = Runtime::with_text_system("demo", FixedTextSystem);
        runtime.compose(200.0, 80.0, |ui, _| {
            ui.text("title")
                .text("Hello")
                .size(Size::wrap_content(), Size::wrap_content())
                .build();
        });

        let title = runtime.find("title").expect("title should exist");
        assert_eq!(title.frame.width, 77.0);
        assert_eq!(title.frame.height, 19.0);
    }

    #[test]
    fn frame_updates_input_composes_and_returns_draw_list() {
        let mut runtime = Runtime::new("demo");
        let result = runtime.frame(
            FrameInput::new(Screen::new(320.0, 180.0), 1.0 / 60.0),
            |ui, _| {
                ui.rect("root").size(64.0, 32.0).build();
                42
            },
        );

        assert_eq!(result.value, 42);
        assert_eq!(result.frame.screen.width, 320.0);
        assert!(!result.frame.draw_list().is_empty());
        assert!(result.frame.needs_render);
    }

    #[test]
    fn eui_demo_layout_matches_original_frames() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(800.0, 600.0, |ui, screen| {
            ui.stack("root")
                .size(screen.width, screen.height)
                .align(Align::Center, Align::Center)
                .content(|ui| {
                    panel(ui, "card")
                        .size(360.0, 260.0)
                        .radius(18.0)
                        .gradient(
                            Color::new(0.10, 0.12, 0.16, 1.0),
                            Color::new(0.05, 0.07, 0.10, 1.0),
                        )
                        .border(1.0, Color::new(0.23, 0.29, 0.38, 1.0))
                        .shadow(26.0, 0.0, 8.0, Color::new(0.0, 0.0, 0.0, 0.26))
                        .build();

                    ui.column("content")
                        .size(360.0, 260.0)
                        .gap(8.0)
                        .justify_content(Align::Center)
                        .align_items(Align::Center)
                        .content(|ui| {
                            text(ui, "title")
                                .size(300.0, 38.0)
                                .text("Hello EUI")
                                .font_size(30.0)
                                .line_height(38.0)
                                .color(Color::new(0.94, 0.97, 1.0, 1.0))
                                .horizontal_align(HorizontalAlign::Center)
                                .build();

                            text(ui, "subtitle")
                                .size(300.0, 30.0)
                                .margin_each(0.0, 0.0, 0.0, 16.0)
                                .text("Text Button Component")
                                .font_size(24.0)
                                .line_height(30.0)
                                .color(Color::new(0.62, 0.70, 0.82, 1.0))
                                .horizontal_align(HorizontalAlign::Center)
                                .build();

                            button(ui, "primary")
                                .size(240.0, 70.0)
                                .text("Click Me")
                                .build();
                        });
                });
        });

        assert_frame(runtime.find("root").unwrap().frame, 0.0, 0.0, 800.0, 600.0);
        assert_frame(
            runtime.find("card").unwrap().frame,
            220.0,
            170.0,
            360.0,
            260.0,
        );
        assert_frame(
            runtime.find("content").unwrap().frame,
            220.0,
            170.0,
            360.0,
            260.0,
        );
        assert_frame(
            runtime.find("title").unwrap().frame,
            250.0,
            215.0,
            300.0,
            38.0,
        );
        assert_frame(
            runtime.find("subtitle").unwrap().frame,
            250.0,
            261.0,
            300.0,
            30.0,
        );
        assert_frame(
            runtime.find("primary").unwrap().frame,
            280.0,
            315.0,
            240.0,
            70.0,
        );
        assert_frame(
            runtime.find("primary.bg").unwrap().frame,
            280.0,
            315.0,
            240.0,
            70.0,
        );
        assert_frame(
            runtime.find("primary.content").unwrap().frame,
            280.0,
            315.0,
            240.0,
            70.0,
        );
        assert_frame(
            runtime.find("primary.text").unwrap().frame,
            280.0,
            315.0,
            240.0,
            70.0,
        );
    }

    #[test]
    fn runtime_marks_no_redraw_for_same_structure_and_size_after_rendered() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("a").size(10.0, 10.0).build();
        });
        runtime.mark_rendered();

        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("a").size(10.0, 10.0).build();
        });

        assert!(!runtime.needs_render());
        assert!(!runtime.full_redraw());
    }

    #[test]
    fn runtime_detects_structure_changes() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("a").size(10.0, 10.0).build();
        });
        runtime.mark_rendered();

        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("a").size(10.0, 10.0).build();
            ui.rect("b").size(10.0, 10.0).build();
        });

        assert!(runtime.needs_render());
        assert!(runtime.full_redraw());
    }

    #[test]
    fn runtime_detects_visual_changes_with_same_structure() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.text("title").text("A").build();
        });
        runtime.mark_rendered();

        runtime.compose(100.0, 100.0, |ui, _| {
            ui.text("title").text("B").build();
        });

        assert!(runtime.needs_render());
        assert!(runtime.full_redraw());
    }

    #[test]
    fn frame_transition_interpolates_draw_list_frame() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(200.0, 100.0, |ui, _| {
            ui.rect("bar")
                .size(10.0, 10.0)
                .transition(Transition::ease(1.0, Ease::Linear))
                .animate(AnimProperty::FRAME)
                .build();
        });
        runtime.tick_animations(0.0);
        runtime.mark_rendered();

        runtime.compose(200.0, 100.0, |ui, _| {
            ui.rect("bar")
                .size(110.0, 10.0)
                .transition(Transition::ease(1.0, Ease::Linear))
                .animate(AnimProperty::FRAME)
                .build();
        });
        assert!(runtime.tick_animations(0.5));

        let draw = runtime.draw_list();
        let bar = draw
            .commands()
            .iter()
            .filter_map(rect_draw)
            .find(|draw| draw.id == "demo.bar")
            .unwrap();

        assert!((bar.frame.width - 60.0).abs() < 0.001);
        assert!(runtime.needs_render());
    }

    #[test]
    fn ancestor_frame_change_snaps_descendant_frame_animation() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(200.0, 100.0, |ui, _| {
            ui.stack("viewport").size(100.0, 80.0).clip().content(|ui| {
                ui.stack("content").size(100.0, 160.0).content(|ui| {
                    ui.rect("indicator")
                        .size(20.0, 10.0)
                        .transition(Transition::ease(1.0, Ease::Linear))
                        .animate(AnimProperty::FRAME)
                        .build();
                });
            });
        });
        runtime.tick_animations(0.0);
        runtime.mark_rendered();

        runtime.compose(200.0, 100.0, |ui, _| {
            ui.stack("viewport").size(100.0, 80.0).clip().content(|ui| {
                ui.stack("content")
                    .y(-40.0)
                    .size(100.0, 160.0)
                    .content(|ui| {
                        ui.rect("indicator")
                            .size(20.0, 10.0)
                            .transition(Transition::ease(1.0, Ease::Linear))
                            .animate(AnimProperty::FRAME)
                            .build();
                    });
            });
        });

        assert!(runtime.tick_animations(0.016));

        let draw = runtime.draw_list();
        let indicator = draw
            .commands()
            .iter()
            .filter_map(rect_draw)
            .find(|draw| draw.id == "demo.indicator")
            .unwrap();

        assert_frame(indicator.frame, 0.0, -40.0, 20.0, 10.0);
    }

    #[test]
    fn state_color_uses_smoothed_hover_blend() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("button")
                .size(40.0, 40.0)
                .states(Color::BLACK, Color::RED, Color::GREEN)
                .build();
        });
        runtime.tick_animations(0.0);

        runtime.update_pointer(PointerEvent::at(5.0, 5.0));
        assert!(runtime.tick_animations(0.016));

        let draw = runtime.draw_list();
        let button = draw
            .commands()
            .iter()
            .filter_map(rect_draw)
            .find(|draw| draw.id == "demo.button")
            .unwrap();

        assert!(button.color.r > 0.0);
        assert!(button.color.r < 1.0);
    }

    #[test]
    fn pointer_press_and_release_produces_click_response() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("button").size(40.0, 30.0).interactive(true).build();
        });
        runtime.mark_rendered();

        runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));

        assert!(runtime.response("button").clicked());
        assert!(runtime.needs_render());
    }

    #[test]
    fn pointer_hover_leave_reports_changed_response() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("button").size(40.0, 30.0).interactive(true).build();
        });

        runtime.update_pointer(PointerEvent::at(10.0, 10.0));
        assert!(runtime.response("button").hovered());

        runtime.update_pointer(PointerEvent::at(90.0, 90.0));
        let response = runtime.response("button");
        assert!(!response.hovered());
        assert!(response.changed());
    }

    #[test]
    fn press_capture_prevents_other_element_click() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.stack("root").size(100.0, 100.0).content(|ui| {
                ui.rect("a")
                    .position(0.0, 0.0)
                    .size(40.0, 40.0)
                    .interactive(true)
                    .build();
                ui.rect("b")
                    .position(50.0, 0.0)
                    .size(40.0, 40.0)
                    .interactive(true)
                    .build();
            });
        });

        runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
        runtime.update_pointer(PointerEvent::dragged_to(60.0, 10.0, 50.0, 0.0));
        runtime.update_pointer(PointerEvent::released_at(60.0, 10.0));

        assert!(!runtime.response("a").clicked());
        assert!(!runtime.response("b").clicked());
        assert!(runtime.interaction("a").released);
    }

    #[test]
    fn z_index_controls_topmost_hit_test() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.stack("root").size(100.0, 100.0).content(|ui| {
                ui.rect("low")
                    .size(50.0, 50.0)
                    .interactive(true)
                    .z_index(0)
                    .build();
                ui.rect("high")
                    .size(50.0, 50.0)
                    .interactive(true)
                    .z_index(10)
                    .build();
            });
        });

        runtime.update_pointer(PointerEvent::at(10.0, 10.0));

        assert!(!runtime.response("low").hovered());
        assert!(runtime.response("high").hovered());
    }

    #[test]
    fn click_callback_runs_from_runtime_dispatch() {
        let clicks = Rc::new(Cell::new(0));
        let callback_clicks = clicks.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, move |ui, _| {
            let callback_clicks = callback_clicks.clone();
            ui.rect("button")
                .size(40.0, 30.0)
                .on_click(move || {
                    callback_clicks.set(callback_clicks.get() + 1);
                })
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));

        assert_eq!(clicks.get(), 1);
        assert!(runtime.needs_compose());
    }

    #[test]
    fn right_click_dispatches_context_menu_callback() {
        let opened = Rc::new(Cell::new(false));
        let callback_opened = opened.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, move |ui, _| {
            let callback_opened = callback_opened.clone();
            ui.rect("target")
                .size(40.0, 30.0)
                .on_context_menu(move |_, _| {
                    callback_opened.set(true);
                })
                .build();
        });

        runtime.update_pointer(PointerEvent::right_pressed_at(10.0, 10.0));

        assert!(opened.get());
    }

    #[test]
    fn focused_element_receives_keyboard_input() {
        let text = Rc::new(RefCell::new(String::new()));
        let callback_text = text.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, move |ui, _| {
            let callback_text = callback_text.clone();
            ui.rect("input")
                .size(80.0, 24.0)
                .on_text_input(move |event| {
                    callback_text.borrow_mut().push_str(&event.text);
                })
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(5.0, 5.0));
        runtime.update_keyboard(KeyboardEvent {
            text: "A".to_string(),
            ..KeyboardEvent::default()
        });

        assert_eq!(text.borrow().as_str(), "A");
        assert_eq!(runtime.focused_id(), Some("demo.input"));
    }

    #[test]
    fn scroll_dispatches_to_topmost_scrollable_element() {
        let amount = Rc::new(Cell::new(0.0));
        let callback_amount = amount.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, move |ui, _| {
            let callback_amount = callback_amount.clone();
            ui.rect("scroll")
                .size(80.0, 80.0)
                .on_scroll(move |event| {
                    callback_amount.set(callback_amount.get() + event.y);
                })
                .build();
        });

        runtime.update_pointer(PointerEvent::at(5.0, 5.0));
        runtime.update_scroll(ScrollEvent { x: 0.0, y: 3.0 });

        assert_eq!(amount.get(), 3.0);
    }

    #[test]
    fn fullscreen_modal_layer_blocks_underlying_hits() {
        let under_clicks = Rc::new(Cell::new(0));
        let modal_clicks = Rc::new(Cell::new(0));
        let under = under_clicks.clone();
        let modal = modal_clicks.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(200.0, 120.0, move |ui, _| {
            let under = under.clone();
            let modal = modal.clone();
            ui.rect("under")
                .size(200.0, 120.0)
                .z_index(0)
                .on_click(move || under.set(under.get() + 1))
                .build();
            ui.stack("modal")
                .size(200.0, 120.0)
                .z_index(1000)
                .content(|ui| {
                    ui.rect("modal.backdrop")
                        .size(200.0, 120.0)
                        .on_click(move || modal.set(modal.get() + 1))
                        .build();
                });
        });

        runtime.update_pointer(PointerEvent::pressed_at(20.0, 20.0));
        runtime.update_pointer(PointerEvent::released_at(20.0, 20.0));

        assert_eq!(under_clicks.get(), 0);
        assert_eq!(modal_clicks.get(), 1);
        assert!(!runtime.response("under").hovered());
        assert!(runtime.response("modal.backdrop").clicked());
    }

    #[test]
    fn timer_callback_runs_after_elapsed_duration() {
        let fired = Rc::new(Cell::new(false));
        let callback_fired = fired.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, move |ui, _| {
            let callback_fired = callback_fired.clone();
            ui.rect("timer")
                .size(1.0, 1.0)
                .on_timer(0.10, move || {
                    callback_fired.set(true);
                })
                .build();
        });

        assert!(!runtime.tick_timers(0.05));
        assert!(runtime.tick_timers(0.05));
        assert!(fired.get());
    }

    fn assert_frame(frame: LayoutRect, x: f32, y: f32, width: f32, height: f32) {
        const EPSILON: f32 = 0.001;
        assert!(
            (frame.x - x).abs() < EPSILON
                && (frame.y - y).abs() < EPSILON
                && (frame.width - width).abs() < EPSILON
                && (frame.height - height).abs() < EPSILON,
            "frame mismatch: got {:?}, expected ({x}, {y}, {width}, {height})",
            frame
        );
    }

    fn rect_draw(command: &UiDrawCommand) -> Option<&UiRectDraw> {
        match command {
            UiDrawCommand::Rect(draw) => Some(draw),
            _ => None,
        }
    }
}
