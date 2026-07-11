#[inline]
pub(super) fn add3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] + rhs[0], lhs[1] + rhs[1], lhs[2] + rhs[2]]
}

#[inline]
pub(super) fn sub3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [lhs[0] - rhs[0], lhs[1] - rhs[1], lhs[2] - rhs[2]]
}

#[inline]
pub(super) fn mul3(value: [f32; 3], scalar: f32) -> [f32; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

#[inline]
pub(super) fn dot3(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    lhs[0] * rhs[0] + lhs[1] * rhs[1] + lhs[2] * rhs[2]
}

#[inline]
pub(super) fn cross3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [
        lhs[1] * rhs[2] - lhs[2] * rhs[1],
        lhs[2] * rhs[0] - lhs[0] * rhs[2],
        lhs[0] * rhs[1] - lhs[1] * rhs[0],
    ]
}

#[inline]
pub(super) fn length_sq3(value: [f32; 3]) -> f32 {
    dot3(value, value)
}

#[inline]
pub(super) fn normalize3(value: [f32; 3]) -> [f32; 3] {
    let len_sq = length_sq3(value);
    if len_sq <= f32::EPSILON {
        [0.0, 0.0, 1.0]
    } else {
        mul3(value, len_sq.sqrt().recip())
    }
}

pub(super) fn fallback_tangent(normal: [f32; 3]) -> [f32; 3] {
    let axis = if normal[2].abs() < 0.999 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    normalize3(cross3(axis, normal))
}
