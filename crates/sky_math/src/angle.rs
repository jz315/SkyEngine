//! Angle helpers. Public angle-bearing APIs use radians unless stated otherwise.

pub const fn degrees_to_radians(degrees: f32) -> f32 {
    degrees * (core::f32::consts::PI / 180.0)
}

pub const fn radians_to_degrees(radians: f32) -> f32 {
    radians * (180.0 / core::f32::consts::PI)
}

/// Normalize an angle to the half-open interval `[-PI, PI)`.
#[inline]
pub fn normalize_angle_radians(radians: f32) -> f32 {
    (radians + core::f32::consts::PI).rem_euclid(core::f32::consts::TAU) - core::f32::consts::PI
}

/// Interpolate along the shortest angular path. `t` is intentionally unclamped.
#[inline]
pub fn lerp_angle_radians(start: f32, end: f32, t: f32) -> f32 {
    start + normalize_angle_radians(end - start) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_and_shortest_lerp_are_consistent() {
        assert!((degrees_to_radians(180.0) - core::f32::consts::PI).abs() < 1e-6);
        assert!((radians_to_degrees(core::f32::consts::PI) - 180.0).abs() < 1e-5);
        let value = lerp_angle_radians(170.0f32.to_radians(), -170.0f32.to_radians(), 0.5);
        assert!((normalize_angle_radians(value).abs() - core::f32::consts::PI).abs() < 1e-5);
    }
}
