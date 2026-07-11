use crate::math::Vec2;

/// Configuration for the 2D physics world.
///
/// Defaults are tuned for top-down 2D games: no gravity, a 60 Hz fixed step,
/// and 32 render/world units per physics meter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsConfig2D {
    /// World gravity in render/world units per second squared.
    ///
    /// This is converted internally using [`pixels_per_meter`](Self::pixels_per_meter).
    pub gravity: Vec2,
    /// Fixed simulation step in seconds.
    ///
    /// [`PhysicsPlugin`](crate::physics::PhysicsPlugin) configures the
    /// [`FixedUpdate`](crate::ecs::FixedUpdate) stage with this value.
    pub fixed_dt: f32,
    /// Conversion factor between render/world units and Rapier meters.
    ///
    /// For tile-based 2D games, `32.0` or `64.0` is usually a good starting
    /// point. The physics API always accepts and returns render/world units.
    pub pixels_per_meter: f32,
}

impl Default for PhysicsConfig2D {
    fn default() -> Self {
        Self {
            gravity: Vec2::ZERO,
            fixed_dt: 1.0 / 60.0,
            pixels_per_meter: 32.0,
        }
    }
}
