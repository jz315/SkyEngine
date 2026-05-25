use super::Color;

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

pub fn has_anim_property(mask: AnimProperty, property: AnimProperty) -> bool {
    mask.contains(property)
}

/// Spring parameters for interruptible target-state motion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpringMotion {
    pub response_seconds: f32,
    pub damping_ratio: f32,
}

impl SpringMotion {
    pub const fn new(response_seconds: f32, damping_ratio: f32) -> Self {
        Self {
            response_seconds,
            damping_ratio,
        }
    }
}

/// Apple-style motion presets for common UI state changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotionPreset {
    Smooth,
    Snappy,
    Gentle,
    Responsive,
}

impl MotionPreset {
    pub const fn spring(self) -> SpringMotion {
        match self {
            Self::Smooth => SpringMotion::new(0.32, 1.0),
            Self::Snappy => SpringMotion::new(0.22, 0.82),
            Self::Gentle => SpringMotion::new(0.44, 1.08),
            Self::Responsive => SpringMotion::new(0.18, 0.88),
        }
    }

    pub fn transition(self) -> Transition {
        Transition::spring_preset(self)
    }
}

/// Transition interpolation mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    Ease,
    Spring,
}

impl Motion {
    pub const EASE: Self = Self::Ease;
}

/// Transition metadata for target-state animation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    pub enabled: bool,
    pub duration_seconds: f32,
    pub delay_seconds: f32,
    pub ease: Ease,
    pub properties: AnimProperty,
    pub motion: Motion,
    pub damping_ratio: f32,
}

impl Transition {
    pub const DISABLED: Self = Self {
        enabled: false,
        duration_seconds: 0.18,
        delay_seconds: 0.0,
        ease: Ease::OutCubic,
        properties: AnimProperty::ALL,
        motion: Motion::EASE,
        damping_ratio: 1.0,
    };

    pub fn ease(duration_seconds: f32, ease: Ease) -> Self {
        Self {
            enabled: true,
            duration_seconds: duration_seconds.max(0.0),
            delay_seconds: 0.0,
            ease,
            properties: AnimProperty::ALL,
            motion: Motion::EASE,
            damping_ratio: 1.0,
        }
    }

    pub fn spring(response_seconds: f32, damping_ratio: f32) -> Self {
        let spring = SpringMotion {
            response_seconds: response_seconds.max(0.001),
            damping_ratio: damping_ratio.max(0.001),
        };
        Self {
            enabled: true,
            duration_seconds: spring.response_seconds,
            delay_seconds: 0.0,
            ease: Ease::OutCubic,
            properties: AnimProperty::ALL,
            motion: Motion::Spring,
            damping_ratio: spring.damping_ratio,
        }
    }

    pub fn spring_preset(preset: MotionPreset) -> Self {
        let spring = preset.spring();
        Self::spring(spring.response_seconds, spring.damping_ratio)
    }

    pub fn smooth() -> Self {
        Self::spring_preset(MotionPreset::Smooth)
    }

    pub fn snappy() -> Self {
        Self::spring_preset(MotionPreset::Snappy)
    }

    pub fn gentle() -> Self {
        Self::spring_preset(MotionPreset::Gentle)
    }

    pub fn responsive() -> Self {
        Self::spring_preset(MotionPreset::Responsive)
    }

    pub fn none() -> Self {
        Self::DISABLED
    }

    pub fn duration(mut self, duration_seconds: f32) -> Self {
        self.enabled = true;
        self.duration_seconds = duration_seconds.max(0.0);
        self.motion = Motion::EASE;
        self.damping_ratio = 1.0;
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
        self.motion = Motion::EASE;
        self.damping_ratio = 1.0;
        self
    }

    pub fn spring_motion(mut self, response_seconds: f32, damping_ratio: f32) -> Self {
        self.enabled = true;
        self.duration_seconds = response_seconds.max(0.001);
        self.motion = Motion::Spring;
        self.damping_ratio = damping_ratio.max(0.001);
        self
    }

    pub fn preset(mut self, preset: MotionPreset) -> Self {
        let spring = preset.spring();
        self.enabled = true;
        self.duration_seconds = spring.response_seconds;
        self.motion = Motion::Spring;
        self.damping_ratio = spring.damping_ratio;
        self
    }

    pub fn animate(mut self, properties: AnimProperty) -> Self {
        self.enabled = true;
        self.properties = properties;
        self
    }

    pub fn spring_motion_params(self) -> SpringMotion {
        SpringMotion {
            response_seconds: self.duration_seconds.max(0.001),
            damping_ratio: self.damping_ratio.max(0.001),
        }
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
    fn zero() -> Self;
    fn spring_step(
        current: Self,
        velocity: Self,
        target: Self,
        spring: SpringMotion,
        dt: f32,
    ) -> (Self, Self);
}

impl Lerp for f32 {
    fn lerp(from: Self, to: Self, amount: f32) -> Self {
        from + (to - from) * amount
    }

    fn close_enough(left: Self, right: Self) -> bool {
        (left - right).abs() <= 0.001
    }

    fn zero() -> Self {
        0.0
    }

    fn spring_step(
        current: Self,
        velocity: Self,
        target: Self,
        spring: SpringMotion,
        dt: f32,
    ) -> (Self, Self) {
        spring_step_f32(current, velocity, target, spring, dt)
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

    fn zero() -> Self {
        [0.0, 0.0]
    }

    fn spring_step(
        current: Self,
        velocity: Self,
        target: Self,
        spring: SpringMotion,
        dt: f32,
    ) -> (Self, Self) {
        let (x, vx) = <f32 as Lerp>::spring_step(current[0], velocity[0], target[0], spring, dt);
        let (y, vy) = <f32 as Lerp>::spring_step(current[1], velocity[1], target[1], spring, dt);
        ([x, y], [vx, vy])
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

    fn zero() -> Self {
        Self {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        }
    }

    fn spring_step(
        current: Self,
        velocity: Self,
        target: Self,
        spring: SpringMotion,
        dt: f32,
    ) -> (Self, Self) {
        let (r, vr) = <f32 as Lerp>::spring_step(current.r, velocity.r, target.r, spring, dt);
        let (g, vg) = <f32 as Lerp>::spring_step(current.g, velocity.g, target.g, spring, dt);
        let (b, vb) = <f32 as Lerp>::spring_step(current.b, velocity.b, target.b, spring, dt);
        let (a, va) = <f32 as Lerp>::spring_step(current.a, velocity.a, target.a, spring, dt);
        (
            Self { r, g, b, a },
            Self {
                r: vr,
                g: vg,
                b: vb,
                a: va,
            },
        )
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

    fn zero() -> Self {
        Self::ZERO
    }

    fn spring_step(
        current: Self,
        velocity: Self,
        target: Self,
        spring: SpringMotion,
        dt: f32,
    ) -> (Self, Self) {
        let (x, vx) = <f32 as Lerp>::spring_step(current.x, velocity.x, target.x, spring, dt);
        let (y, vy) = <f32 as Lerp>::spring_step(current.y, velocity.y, target.y, spring, dt);
        let (width, vwidth) =
            <f32 as Lerp>::spring_step(current.width, velocity.width, target.width, spring, dt);
        let (height, vheight) =
            <f32 as Lerp>::spring_step(current.height, velocity.height, target.height, spring, dt);
        (
            Self {
                x,
                y,
                width,
                height,
            },
            Self {
                x: vx,
                y: vy,
                width: vwidth,
                height: vheight,
            },
        )
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

    fn zero() -> Self {
        Self {
            width: 0.0,
            color: Color::zero(),
        }
    }

    fn spring_step(
        current: Self,
        velocity: Self,
        target: Self,
        spring: SpringMotion,
        dt: f32,
    ) -> (Self, Self) {
        let (width, vwidth) =
            <f32 as Lerp>::spring_step(current.width, velocity.width, target.width, spring, dt);
        let (color, vcolor) =
            Color::spring_step(current.color, velocity.color, target.color, spring, dt);
        (
            Self { width, color },
            Self {
                width: vwidth,
                color: vcolor,
            },
        )
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

    fn zero() -> Self {
        Self {
            enabled: false,
            offset: [0.0, 0.0],
            blur: 0.0,
            spread: 0.0,
            color: Color::zero(),
        }
    }

    fn spring_step(
        mut current: Self,
        velocity: Self,
        mut target: Self,
        spring: SpringMotion,
        dt: f32,
    ) -> (Self, Self) {
        if !current.enabled {
            current.color.a = 0.0;
        }
        if !target.enabled {
            target.color.a = 0.0;
        }
        let (offset, voffset) = <[f32; 2] as Lerp>::spring_step(
            current.offset,
            velocity.offset,
            target.offset,
            spring,
            dt,
        );
        let (blur, vblur) =
            <f32 as Lerp>::spring_step(current.blur, velocity.blur, target.blur, spring, dt);
        let (spread, vspread) =
            <f32 as Lerp>::spring_step(current.spread, velocity.spread, target.spread, spring, dt);
        let (color, vcolor) =
            Color::spring_step(current.color, velocity.color, target.color, spring, dt);
        (
            Self {
                enabled: current.enabled || target.enabled,
                offset,
                blur,
                spread,
                color,
            },
            Self {
                enabled: false,
                offset: voffset,
                blur: vblur,
                spread: vspread,
                color: vcolor,
            },
        )
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

    fn zero() -> Self {
        Self {
            translate: [0.0, 0.0],
            scale: [0.0, 0.0],
            rotation: 0.0,
            origin: [0.0, 0.0],
        }
    }

    fn spring_step(
        current: Self,
        velocity: Self,
        target: Self,
        spring: SpringMotion,
        dt: f32,
    ) -> (Self, Self) {
        let (translate, vtranslate) = <[f32; 2] as Lerp>::spring_step(
            current.translate,
            velocity.translate,
            target.translate,
            spring,
            dt,
        );
        let (scale, vscale) = <[f32; 2] as Lerp>::spring_step(
            current.scale,
            velocity.scale,
            target.scale,
            spring,
            dt,
        );
        let (rotation, vrotation) = <f32 as Lerp>::spring_step(
            current.rotation,
            velocity.rotation,
            target.rotation,
            spring,
            dt,
        );
        let (origin, vorigin) = <[f32; 2] as Lerp>::spring_step(
            current.origin,
            velocity.origin,
            target.origin,
            spring,
            dt,
        );
        (
            Self {
                translate,
                scale,
                rotation,
                origin,
            },
            Self {
                translate: vtranslate,
                scale: vscale,
                rotation: vrotation,
                origin: vorigin,
            },
        )
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
    velocity: T,
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
            velocity: T::zero(),
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
        let keep_velocity = matches!(self.transition.motion, Motion::Spring)
            && matches!(transition.motion, Motion::Spring)
            && self.animating;
        self.start = self.current;
        self.target = target;
        self.elapsed = 0.0;
        if !keep_velocity {
            self.velocity = T::zero();
        }
        self.transition = transition;
        self.animating = transition.enabled
            && animate_property
            && (transition.duration_seconds > 0.0 || matches!(transition.motion, Motion::Spring));
        if !self.animating {
            self.current = target;
            self.velocity = T::zero();
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
        match self.transition.motion {
            Motion::Ease => {
                let duration = self.transition.duration_seconds.max(0.0001);
                let t = ((self.elapsed - self.transition.delay_seconds) / duration).clamp(0.0, 1.0);
                self.current = T::lerp(self.start, self.target, self.transition.ease.sample(t));
                if t >= 1.0 || T::close_enough(self.current, self.target) {
                    self.current = self.target;
                    self.velocity = T::zero();
                    self.animating = false;
                }
            }
            Motion::Spring => {
                let step_dt = dt.max(0.0);
                let (current, velocity) = T::spring_step(
                    self.current,
                    self.velocity,
                    self.target,
                    self.transition.spring_motion_params(),
                    step_dt,
                );
                self.current = current;
                self.velocity = velocity;
                if T::close_enough(self.current, self.target)
                    && T::close_enough(self.velocity, T::zero())
                {
                    self.current = self.target;
                    self.velocity = T::zero();
                    self.animating = false;
                }
            }
        }
        !T::close_enough(previous, self.current)
    }
}

fn spring_step_f32(
    mut current: f32,
    mut velocity: f32,
    target: f32,
    spring: SpringMotion,
    dt: f32,
) -> (f32, f32) {
    let response = spring.response_seconds.max(0.001);
    let damping_ratio = spring.damping_ratio.max(0.001);
    let omega = std::f32::consts::TAU / response;
    let stiffness = omega * omega;
    let damping = 2.0 * damping_ratio * omega;
    let mut remaining = dt.clamp(0.0, 0.25);

    while remaining > 0.0 {
        let step = remaining.min(1.0 / 120.0);
        let acceleration = stiffness * (target - current) - damping * velocity;
        velocity += acceleration * step;
        current += velocity * step;
        remaining -= step;
    }

    (current, velocity)
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
    use super::{AnimatedValue, Ease, Motion, MotionPreset, SmoothedValue, Transition};

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
        value.set_target(10.0, Transition::ease(1.0, Ease::Linear));

        assert!(value.is_animating());
        value.update(0.5);
        assert!((value.current() - 5.0).abs() < 0.001);
        value.update(0.5);
        assert_eq!(value.current(), 10.0);
        assert!(!value.is_animating());
    }

    #[test]
    fn spring_value_reaches_target() {
        let mut value = AnimatedValue::new(0.0);
        value.set_target(10.0, Transition::smooth());

        for _ in 0..80 {
            value.update(1.0 / 60.0);
        }

        assert!((value.current() - 10.0).abs() < 0.01);
        assert!(!value.is_animating());
    }

    #[test]
    fn spring_retarget_keeps_velocity() {
        let mut value = AnimatedValue::new(0.0);
        value.set_target(100.0, Transition::snappy());
        value.update(0.08);
        let velocity_before = value.velocity;
        assert!(velocity_before > 0.0);

        value.set_target(0.0, Transition::snappy());

        assert_eq!(value.velocity, velocity_before);
        assert!(value.is_animating());
    }

    #[test]
    fn motion_preset_builds_spring_transition() {
        let transition = MotionPreset::Responsive.transition();

        assert!(matches!(transition.motion, Motion::Spring));
        assert!(transition.enabled);
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
