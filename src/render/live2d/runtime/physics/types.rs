// Live2D physics (`physics3.json`) runtime support.

pub(super) const AIR_RESISTANCE: f32 = 5.0;
pub(super) const MAX_WEIGHT: f32 = 100.0;
pub(super) const MOVEMENT_THRESHOLD: f32 = 0.001;
pub(super) const MAX_DELTA_TIME: f32 = 5.0;
pub(super) const DEFAULT_GRAVITY: [f32; 2] = [0.0, -1.0];
pub(super) const DEFAULT_WIND: [f32; 2] = [0.0, 0.0];

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Vec2 {
    pub(super) x: f32,
    pub(super) y: f32,
}

impl Vec2 {
    pub(super) fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub(super) fn normalize(&mut self) {
        let length = (self.x * self.x + self.y * self.y).sqrt();
        if length > f32::EPSILON {
            self.x /= length;
            self.y /= length;
        }
    }
}

impl std::ops::Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl std::ops::Mul<f32> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

impl std::ops::MulAssign<f32> for Vec2 {
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

impl std::ops::DivAssign<f32> for Vec2 {
    fn div_assign(&mut self, rhs: f32) {
        self.x /= rhs;
        self.y /= rhs;
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Normalization {
    pub(super) minimum: f32,
    pub(super) maximum: f32,
    pub(super) default: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Source {
    X,
    Y,
    Angle,
}

#[derive(Debug, Clone)]
pub(super) struct Input {
    pub(super) parameter_index: Option<usize>,
    pub(super) weight: f32,
    pub(super) reflect: bool,
    pub(super) source: Source,
}

#[derive(Debug, Clone)]
pub(super) struct Output {
    pub(super) parameter_index: Option<usize>,
    pub(super) vertex_index: usize,
    pub(super) scale: f32,
    pub(super) weight: f32,
    pub(super) reflect: bool,
    pub(super) source: Source,
    pub(super) value_below_minimum: f32,
    pub(super) value_exceeded_maximum: f32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Particle {
    pub(super) initial_position: Vec2,
    pub(super) position: Vec2,
    pub(super) last_position: Vec2,
    pub(super) last_gravity: Vec2,
    pub(super) velocity: Vec2,
    pub(super) force: Vec2,
    pub(super) mobility: f32,
    pub(super) delay: f32,
    pub(super) acceleration: f32,
    pub(super) radius: f32,
}

#[derive(Debug, Clone)]
pub(super) struct SubRig {
    pub(super) normalization_position: Normalization,
    pub(super) normalization_angle: Normalization,
    pub(super) inputs: Vec<Input>,
    pub(super) outputs: Vec<Output>,
    pub(super) particles: Vec<Particle>,
}

/// Runtime physics state loaded from a `physics3.json` file.
#[derive(Debug, Clone)]
pub struct Live2DPhysics {
    pub(super) gravity: Vec2,
    pub(super) wind: Vec2,
    pub(super) fps: f32,
    pub(super) current_remain_time: f32,
    pub(super) sub_rigs: Vec<SubRig>,
    pub(super) current_rig_outputs: Vec<Vec<f32>>,
    pub(super) previous_rig_outputs: Vec<Vec<f32>>,
    pub(super) parameter_caches: Vec<f32>,
    pub(super) parameter_input_caches: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Live2DPhysicsOptions {
    pub gravity: [f32; 2],
    pub wind: [f32; 2],
}

impl Default for Live2DPhysicsOptions {
    fn default() -> Self {
        Self {
            gravity: DEFAULT_GRAVITY,
            wind: DEFAULT_WIND,
        }
    }
}
