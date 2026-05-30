use super::tree::{sorted_z_indices, z_order_is_stable};
use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct FrameTargetState {
    frame: LayoutRect,
    seen: bool,
}

#[derive(Debug, Default)]
pub(super) struct ElementAnimation {
    pub(super) seen: bool,
    pub(super) hover_blend: SmoothedValue,
    pub(super) press_blend: SmoothedValue,
    pub(super) frame: Option<AnimatedValue<LayoutRect>>,
    pub(super) color: Option<AnimatedValue<Color>>,
    pub(super) text_color: Option<AnimatedValue<Color>>,
    pub(super) radius: Option<AnimatedValue<f32>>,
    pub(super) blur: Option<AnimatedValue<f32>>,
    pub(super) opacity: Option<AnimatedValue<f32>>,
    pub(super) border: Option<AnimatedValue<Border>>,
    pub(super) shadow: Option<AnimatedValue<Shadow>>,
    pub(super) transform: Option<AnimatedValue<Transform>>,
}

impl ElementAnimation {
    pub(super) fn is_active(&self) -> bool {
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
    pub(crate) fn animated_frame(&self, element: &Element) -> LayoutRect {
        self.animation
            .animations
            .get(&element.id)
            .and_then(|animation| animation.frame.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.frame)
    }

    pub(crate) fn animated_color(&self, element: &Element) -> Color {
        self.animation
            .animations
            .get(&element.id)
            .and_then(|animation| animation.color.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or_else(|| self.state_color_target(element))
    }

    pub(crate) fn animated_text_color(&self, element: &Element) -> Color {
        self.animation
            .animations
            .get(&element.id)
            .and_then(|animation| animation.text_color.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.text_color)
    }

    pub(crate) fn animated_radius(&self, element: &Element) -> f32 {
        self.animation
            .animations
            .get(&element.id)
            .and_then(|animation| animation.radius.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.radius)
    }

    pub(crate) fn animated_blur(&self, element: &Element) -> f32 {
        self.animation
            .animations
            .get(&element.id)
            .and_then(|animation| animation.blur.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.blur)
    }

    pub(crate) fn animated_opacity(&self, element: &Element) -> f32 {
        self.animation
            .animations
            .get(&element.id)
            .and_then(|animation| animation.opacity.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.opacity)
    }

    pub(crate) fn animated_border(&self, element: &Element) -> Border {
        self.animation
            .animations
            .get(&element.id)
            .and_then(|animation| animation.border.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.border)
    }

    pub(crate) fn animated_shadow(&self, element: &Element) -> Shadow {
        self.animation
            .animations
            .get(&element.id)
            .and_then(|animation| animation.shadow.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.shadow)
    }

    pub(crate) fn animated_transform(&self, element: &Element) -> Transform {
        self.animation
            .animations
            .get(&element.id)
            .and_then(|animation| animation.transform.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.transform)
    }

    pub(crate) fn hover_blend_for_source(&self, id: &str) -> Option<f32> {
        let id = self.resolve_id_ref(id);
        let element = self.find_resolved(id.as_ref())?;
        if !matches!(
            element.kind,
            ElementKind::Rect | ElementKind::Polygon | ElementKind::Image | ElementKind::NineSlice
        ) {
            return None;
        }
        self.animation
            .animations
            .get(id.as_ref())
            .map(|animation| animation.hover_blend.current())
    }

    pub(crate) fn press_blend_for_source(&self, id: &str) -> Option<(f32, LayoutRect)> {
        let id = self.resolve_id_ref(id);
        let element = self.find_resolved(id.as_ref())?;
        if !matches!(
            element.kind,
            ElementKind::Rect | ElementKind::Polygon | ElementKind::Image | ElementKind::NineSlice
        ) {
            return None;
        }
        let animation = self.animation.animations.get(id.as_ref())?;
        let frame = animation
            .frame
            .as_ref()
            .map(AnimatedValue::current)
            .unwrap_or(element.frame);
        Some((animation.press_blend.current(), frame))
    }

    pub(crate) fn tick_animations(&mut self, delta_seconds: f32) -> bool {
        for animation in self.animation.animations.values_mut() {
            animation.seen = false;
        }
        for target in self.animation.frame_targets.values_mut() {
            target.seen = false;
        }

        let mut changed = false;
        let roots = std::mem::take(&mut self.tree.roots);
        if z_order_is_stable(&roots) {
            for root in &roots {
                changed |= self.tick_element_animation_tree(root, delta_seconds, false);
            }
        } else {
            for index in sorted_z_indices(&roots) {
                changed |= self.tick_element_animation_tree(&roots[index], delta_seconds, false);
            }
        }
        self.tree.roots = roots;
        self.animation
            .animations
            .retain(|_, animation| animation.seen);
        self.animation.frame_targets.retain(|_, target| target.seen);

        let active = self
            .animation
            .animations
            .values()
            .any(ElementAnimation::is_active);
        if changed {
            self.mark_render_dirty();
        } else if active {
            self.request_render();
        }
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
        if z_order_is_stable(&element.children) {
            for child in &element.children {
                changed |= self.tick_element_animation_tree(
                    child,
                    delta_seconds,
                    child_ancestor_frame_changed,
                );
            }
        } else {
            for index in sorted_z_indices(&element.children) {
                changed |= self.tick_element_animation_tree(
                    &element.children[index],
                    delta_seconds,
                    child_ancestor_frame_changed,
                );
            }
        }
        changed
    }

    fn update_frame_target(&mut self, element: &Element) -> bool {
        let entry = self
            .animation
            .frame_targets
            .entry(element.id.clone())
            .or_insert(FrameTargetState {
                frame: element.frame,
                seen: true,
            });
        let moved = (entry.frame.x - element.frame.x).abs() > 0.001
            || (entry.frame.y - element.frame.y).abs() > 0.001;
        entry.frame = element.frame;
        entry.seen = true;
        moved
    }

    fn tick_element_animation(
        &mut self,
        element: &Element,
        delta_seconds: f32,
        snap_frame: bool,
    ) -> bool {
        let interaction = self.interaction(&element.id);
        let animation = self
            .animation
            .animations
            .entry(element.id.clone())
            .or_default();
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
        let animation = self.animation.animations.get(&element.id);
        let blends = animation.map(|animation| {
            (
                animation.hover_blend.current(),
                animation.press_blend.current(),
            )
        });
        state_color_target(element, self.interaction(&element.id), blends)
    }
}

pub(super) fn sync_animated<T>(
    slot: &mut Option<AnimatedValue<T>>,
    target: T,
    transition: Transition,
    animate_property: bool,
    delta_seconds: f32,
) -> bool
where
    T: crate::Lerp,
{
    let Some(value) = slot.as_mut() else {
        *slot = Some(AnimatedValue::new(target));
        return true;
    };
    let mut changed = value.set_target_if(target, transition, animate_property);
    changed |= value.update(delta_seconds);
    changed
}

pub(super) fn update_state_blends(
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

pub(super) fn should_animate(element: &Element, property: AnimProperty) -> bool {
    element.transition.enabled && element.transition.properties.contains(property)
}

pub(super) fn should_animate_frame(element: &Element) -> bool {
    should_animate(element, AnimProperty::FRAME) && element.explicit_frame_animation
}

pub(super) fn state_color_target(
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

pub(super) fn mix_color(from: Color, to: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    let inverse = 1.0 - amount;
    Color {
        r: from.r * inverse + to.r * amount,
        g: from.g * inverse + to.g * amount,
        b: from.b * inverse + to.b * amount,
        a: from.a * inverse + to.a * amount,
    }
}

pub(super) fn color_close_enough(left: Color, right: Color) -> bool {
    (left.r - right.r).abs() <= 0.001
        && (left.g - right.g).abs() <= 0.001
        && (left.b - right.b).abs() <= 0.001
        && (left.a - right.a).abs() <= 0.001
}
