use crate::render::Color;

use super::element::{Border, LayoutRect, Shadow, Transform};

bitflags::bitflags! {
    /// Animatable property mask copied from EUI-NEO's `AnimProperty`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct AnimProperty: u32 {
        const NONE = 0;
        const FRAME = 1 << 0;
        const COLOR = 1 << 1;
        const TEXT_COLOR = 1 << 2;
        const OPACITY = 1 << 3;
        const RADIUS = 1 << 4;
        const BORDER = 1 << 5;
        const SHADOW = 1 << 6;
        const BLUR = 1 << 7;
        const TRANSFORM = 1 << 8;
        const ALL = u32::MAX;
    }
}

/// Easing functions ported from EUI-NEO's animation layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ease {
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    OutCubic,
    InOutCubic,
    OutBack,
}

impl Ease {
    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::InQuad => t * t,
            Self::OutQuad => 1.0 - (1.0 - t) * (1.0 - t),
            Self::InOutQuad => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) * 0.5
                }
            }
            Self::OutCubic => 1.0 - (1.0 - t).powi(3),
            Self::InOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) * 0.5
                }
            }
            Self::OutBack => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
            }
        }
    }
}

pub fn apply_ease(ease: Ease, t: f32) -> f32 {
    ease.sample(t)
}

pub fn applyEase(ease: Ease, t: f32) -> f32 {
    apply_ease(ease, t)
}

pub fn has_anim_property(mask: AnimProperty, property: AnimProperty) -> bool {
    mask.contains(property)
}

pub fn hasAnimProperty(mask: AnimProperty, property: AnimProperty) -> bool {
    has_anim_property(mask, property)
}

/// Transition metadata for target-state animation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    pub enabled: bool,
    pub duration_seconds: f32,
    pub delay_seconds: f32,
    pub ease: Ease,
    pub properties: AnimProperty,
}

impl Transition {
    pub const DISABLED: Self = Self {
        enabled: false,
        duration_seconds: 0.18,
        delay_seconds: 0.0,
        ease: Ease::OutCubic,
        properties: AnimProperty::ALL,
    };

    pub fn new(duration_seconds: f32, ease: Ease) -> Self {
        Self {
            enabled: true,
            duration_seconds: duration_seconds.max(0.0),
            delay_seconds: 0.0,
            ease,
            properties: AnimProperty::ALL,
        }
    }

    pub fn none() -> Self {
        Self::DISABLED
    }

    pub fn make(duration_seconds: f32, ease: Ease) -> Self {
        Self::new(duration_seconds, ease)
    }

    pub fn duration(mut self, duration_seconds: f32) -> Self {
        self.enabled = true;
        self.duration_seconds = duration_seconds.max(0.0);
        self
    }

    pub fn delay(mut self, delay_seconds: f32) -> Self {
        self.enabled = true;
        self.delay_seconds = delay_seconds.max(0.0);
        self
    }

    pub fn easing(mut self, ease: Ease) -> Self {
        self.enabled = true;
        self.ease = ease;
        self
    }

    pub fn animate(mut self, properties: AnimProperty) -> Self {
        self.enabled = true;
        self.properties = properties;
        self
    }

    pub fn durationSeconds(self, duration_seconds: f32) -> Self {
        self.duration(duration_seconds)
    }

    pub fn delaySeconds(self, delay_seconds: f32) -> Self {
        self.delay(delay_seconds)
    }
}

impl Default for Transition {
    fn default() -> Self {
        Self::DISABLED
    }
}

/// Values that can be interpolated by the neo runtime.
pub trait Lerp: Copy {
    fn lerp(from: Self, to: Self, amount: f32) -> Self;
    fn close_enough(left: Self, right: Self) -> bool;
}

impl Lerp for f32 {
    fn lerp(from: Self, to: Self, amount: f32) -> Self {
        from + (to - from) * amount
    }

    fn close_enough(left: Self, right: Self) -> bool {
        (left - right).abs() <= 0.001
    }
}

impl Lerp for [f32; 2] {
    fn lerp(from: Self, to: Self, amount: f32) -> Self {
        [
            <f32 as Lerp>::lerp(from[0], to[0], amount),
            <f32 as Lerp>::lerp(from[1], to[1], amount),
        ]
    }

    fn close_enough(left: Self, right: Self) -> bool {
        <f32 as Lerp>::close_enough(left[0], right[0])
            && <f32 as Lerp>::close_enough(left[1], right[1])
    }
}

impl Lerp for Color {
    fn lerp(from: Self, to: Self, amount: f32) -> Self {
        Self {
            r: <f32 as Lerp>::lerp(from.r, to.r, amount),
            g: <f32 as Lerp>::lerp(from.g, to.g, amount),
            b: <f32 as Lerp>::lerp(from.b, to.b, amount),
            a: <f32 as Lerp>::lerp(from.a, to.a, amount),
        }
    }

    fn close_enough(left: Self, right: Self) -> bool {
        <f32 as Lerp>::close_enough(left.r, right.r)
            && <f32 as Lerp>::close_enough(left.g, right.g)
            && <f32 as Lerp>::close_enough(left.b, right.b)
            && <f32 as Lerp>::close_enough(left.a, right.a)
    }
}

impl Lerp for LayoutRect {
    fn lerp(from: Self, to: Self, amount: f32) -> Self {
        Self {
            x: <f32 as Lerp>::lerp(from.x, to.x, amount),
            y: <f32 as Lerp>::lerp(from.y, to.y, amount),
            width: <f32 as Lerp>::lerp(from.width, to.width, amount),
            height: <f32 as Lerp>::lerp(from.height, to.height, amount),
        }
    }

    fn close_enough(left: Self, right: Self) -> bool {
        <f32 as Lerp>::close_enough(left.x, right.x)
            && <f32 as Lerp>::close_enough(left.y, right.y)
            && <f32 as Lerp>::close_enough(left.width, right.width)
            && <f32 as Lerp>::close_enough(left.height, right.height)
    }
}

impl Lerp for Border {
    fn lerp(from: Self, to: Self, amount: f32) -> Self {
        Self {
            width: <f32 as Lerp>::lerp(from.width, to.width, amount),
            color: Color::lerp(from.color, to.color, amount),
        }
    }

    fn close_enough(left: Self, right: Self) -> bool {
        <f32 as Lerp>::close_enough(left.width, right.width)
            && Color::close_enough(left.color, right.color)
    }
}

impl Lerp for Shadow {
    fn lerp(mut from: Self, mut to: Self, amount: f32) -> Self {
        if !from.enabled {
            from.color.a = 0.0;
        }
        if !to.enabled {
            to.color.a = 0.0;
        }
        Self {
            enabled: from.enabled || to.enabled,
            offset: <[f32; 2] as Lerp>::lerp(from.offset, to.offset, amount),
            blur: <f32 as Lerp>::lerp(from.blur, to.blur, amount),
            spread: <f32 as Lerp>::lerp(from.spread, to.spread, amount),
            color: Color::lerp(from.color, to.color, amount),
        }
    }

    fn close_enough(left: Self, right: Self) -> bool {
        left.enabled == right.enabled
            && <[f32; 2] as Lerp>::close_enough(left.offset, right.offset)
            && <f32 as Lerp>::close_enough(left.blur, right.blur)
            && <f32 as Lerp>::close_enough(left.spread, right.spread)
            && Color::close_enough(left.color, right.color)
    }
}

impl Lerp for Transform {
    fn lerp(from: Self, to: Self, amount: f32) -> Self {
        Self {
            translate: <[f32; 2] as Lerp>::lerp(from.translate, to.translate, amount),
            scale: <[f32; 2] as Lerp>::lerp(from.scale, to.scale, amount),
            rotation: <f32 as Lerp>::lerp(from.rotation, to.rotation, amount),
            origin: <[f32; 2] as Lerp>::lerp(from.origin, to.origin, amount),
        }
    }

    fn close_enough(left: Self, right: Self) -> bool {
        <[f32; 2] as Lerp>::close_enough(left.translate, right.translate)
            && <[f32; 2] as Lerp>::close_enough(left.scale, right.scale)
            && <f32 as Lerp>::close_enough(left.rotation, right.rotation)
            && <[f32; 2] as Lerp>::close_enough(left.origin, right.origin)
    }
}

/// Target-state animated value.
#[derive(Debug, Clone, Copy)]
pub struct AnimatedValue<T>
where
    T: Lerp,
{
    current: T,
    start: T,
    target: T,
    elapsed: f32,
    transition: Transition,
    animating: bool,
}

impl<T> AnimatedValue<T>
where
    T: Lerp,
{
    pub fn new(value: T) -> Self {
        Self {
            current: value,
            start: value,
            target: value,
            elapsed: 0.0,
            transition: Transition::DISABLED,
            animating: false,
        }
    }

    pub fn current(&self) -> T {
        self.current
    }

    pub fn target(&self) -> T {
        self.target
    }

    pub fn is_animating(&self) -> bool {
        self.animating
    }

    pub fn set_target(&mut self, target: T, transition: Transition) -> bool {
        self.set_target_if(target, transition, true)
    }

    pub fn set_target_if(
        &mut self,
        target: T,
        transition: Transition,
        animate_property: bool,
    ) -> bool {
        if T::close_enough(self.target, target) {
            return false;
        }
        self.start = self.current;
        self.target = target;
        self.elapsed = 0.0;
        self.transition = transition;
        self.animating =
            transition.enabled && animate_property && transition.duration_seconds > 0.0;
        if !self.animating {
            self.current = target;
        }
        true
    }

    pub fn update(&mut self, dt: f32) -> bool {
        if !self.animating {
            return false;
        }
        let previous = self.current;
        self.elapsed += dt.max(0.0);
        if self.elapsed - self.transition.delay_seconds <= 0.0 {
            return false;
        }
        let duration = self.transition.duration_seconds.max(0.0001);
        let t = ((self.elapsed - self.transition.delay_seconds) / duration).clamp(0.0, 1.0);
        self.current = T::lerp(self.start, self.target, self.transition.ease.sample(t));
        if t >= 1.0 || T::close_enough(self.current, self.target) {
            self.current = self.target;
            self.animating = false;
        }
        !T::close_enough(previous, self.current)
    }
}

/// Smoothly approaches a target value, useful for hover/press blends.
#[derive(Debug, Clone, Copy)]
pub struct SmoothedValue {
    current: f32,
    target: f32,
    speed: f32,
    initialized: bool,
}

impl SmoothedValue {
    pub fn new(value: f32) -> Self {
        Self {
            current: value,
            target: value,
            speed: 14.0,
            initialized: true,
        }
    }

    pub fn current(&self) -> f32 {
        self.current
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target.clamp(0.0, 1.0);
    }

    pub fn update(&mut self, dt: f32) -> bool {
        self.update_to(self.target, self.speed, dt)
    }

    pub fn update_to(&mut self, target: f32, speed: f32, dt: f32) -> bool {
        let target = target.clamp(0.0, 1.0);
        if !self.initialized {
            self.initialized = true;
            self.current = target;
            self.target = target;
            return true;
        }
        let before = self.current;
        self.target = target;
        if dt <= 0.0 || speed <= 0.0 {
            self.current = target;
        } else {
            let amount = 1.0 - (-speed * dt.max(0.0)).exp();
            self.current = <f32 as Lerp>::lerp(self.current, target, amount);
        }
        if <f32 as Lerp>::close_enough(self.current, target) {
            self.current = target;
        }
        !<f32 as Lerp>::close_enough(before, self.current)
    }

    pub fn is_moving(&self) -> bool {
        !<f32 as Lerp>::close_enough(self.current, self.target)
    }
}

impl Default for SmoothedValue {
    fn default() -> Self {
        Self {
            current: 0.0,
            target: 0.0,
            speed: 14.0,
            initialized: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AnimatedValue, Ease, SmoothedValue, Transition};

    #[test]
    fn easing_curves_stay_in_expected_range() {
        assert_eq!(Ease::Linear.sample(0.5), 0.5);
        assert!(Ease::OutCubic.sample(0.5) > 0.5);
        assert_eq!(Ease::InQuad.sample(-1.0), 0.0);
        assert_eq!(Ease::InQuad.sample(2.0), 1.0);
    }

    #[test]
    fn animated_value_reaches_target() {
        let mut value = AnimatedValue::new(0.0);
        value.set_target(10.0, Transition::new(1.0, Ease::Linear));

        assert!(value.is_animating());
        value.update(0.5);
        assert!((value.current() - 5.0).abs() < 0.001);
        value.update(0.5);
        assert_eq!(value.current(), 10.0);
        assert!(!value.is_animating());
    }

    #[test]
    fn smoothed_value_moves_toward_target() {
        let mut value = SmoothedValue::new(0.0);
        value.set_target(1.0);

        assert!(value.update(0.016));
        assert!(value.current() > 0.0);
        assert!(value.current() < 1.0);
    }
}
