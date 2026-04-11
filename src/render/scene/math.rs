use crate::render::ecs::Transform;
use crate::render::Quaternion;

#[inline]
pub(crate) fn column_major_mul(lhs: [f32; 16], rhs: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for row in 0..4 {
        for col in 0..4 {
            let mut value = 0.0;
            for k in 0..4 {
                value += lhs[k * 4 + row] * rhs[col * 4 + k];
            }
            out[col * 4 + row] = value;
        }
    }
    out
}

#[inline]
pub(crate) fn scene_view_matrix(transform: Transform) -> [f32; 16] {
    let translation = translation_matrix(-transform.x(), -transform.y(), -transform.z());
    let rotation = transpose_rotation_matrix(scene_rotation_matrix(transform.rotation));
    column_major_mul(rotation, translation)
}

#[inline]
pub(crate) fn scene_rotation_matrix(rotation: Quaternion) -> [f32; 16] {
    rotation.to_matrix4()
}

#[inline]
pub(crate) fn transform_direction(rotation: [f32; 16], direction: [f32; 3]) -> [f32; 3] {
    [
        rotation[0] * direction[0] + rotation[4] * direction[1] + rotation[8] * direction[2],
        rotation[1] * direction[0] + rotation[5] * direction[1] + rotation[9] * direction[2],
        rotation[2] * direction[0] + rotation[6] * direction[1] + rotation[10] * direction[2],
    ]
}

#[inline]
pub(crate) fn transform_point(matrix: [f32; 16], point: [f32; 3]) -> [f32; 3] {
    [
        matrix[0] * point[0] + matrix[4] * point[1] + matrix[8] * point[2] + matrix[12],
        matrix[1] * point[0] + matrix[5] * point[1] + matrix[9] * point[2] + matrix[13],
        matrix[2] * point[0] + matrix[6] * point[1] + matrix[10] * point[2] + matrix[14],
    ]
}

#[inline]
pub(crate) fn transform_local_point(
    rotation: [f32; 16],
    point: [f32; 3],
    transform: Transform,
) -> [f32; 3] {
    let rotated = transform_direction(rotation, point);
    [
        transform.x() + rotated[0],
        transform.y() + rotated[1],
        transform.z() + rotated[2],
    ]
}

#[inline]
fn translation_matrix(x: f32, y: f32, z: f32) -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0,
    ]
}

#[cfg(feature = "live2d")]
#[inline]
fn scale_matrix(x: f32, y: f32, z: f32) -> [f32; 16] {
    [
        x, 0.0, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

#[inline]
fn transpose_rotation_matrix(matrix: [f32; 16]) -> [f32; 16] {
    [
        matrix[0], matrix[4], matrix[8], 0.0, matrix[1], matrix[5], matrix[9], 0.0, matrix[2],
        matrix[6], matrix[10], 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

#[cfg(feature = "live2d")]
pub(crate) fn scene_transform_matrix(transform: Transform) -> [f32; 16] {
    column_major_mul(
        translation_matrix(transform.x(), transform.y(), transform.z()),
        column_major_mul(
            scene_rotation_matrix(transform.rotation),
            scale_matrix(
                transform.scale_x(),
                transform.scale_y(),
                transform.scale_z(),
            ),
        ),
    )
}
