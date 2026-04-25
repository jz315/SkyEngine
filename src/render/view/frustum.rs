#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frustum {
    planes: [Plane; 6],
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Plane {
    normal: [f32; 3],
    distance: f32,
}

impl Frustum {
    pub fn from_view_proj(view_proj: [f32; 16]) -> Self {
        let row_x = [view_proj[0], view_proj[4], view_proj[8], view_proj[12]];
        let row_y = [view_proj[1], view_proj[5], view_proj[9], view_proj[13]];
        let row_z = [view_proj[2], view_proj[6], view_proj[10], view_proj[14]];
        let row_w = [view_proj[3], view_proj[7], view_proj[11], view_proj[15]];

        Self {
            planes: [
                Plane::from_vec4(add_vec4(row_w, row_x)),
                Plane::from_vec4(sub_vec4(row_w, row_x)),
                Plane::from_vec4(add_vec4(row_w, row_y)),
                Plane::from_vec4(sub_vec4(row_w, row_y)),
                Plane::from_vec4(row_z),
                Plane::from_vec4(sub_vec4(row_w, row_z)),
            ],
        }
    }

    #[inline]
    pub fn intersects_sphere(&self, center: [f32; 3], radius: f32) -> bool {
        if !radius.is_finite() {
            return true;
        }

        self.planes
            .iter()
            .all(|plane| plane.distance_to_point(center) >= -radius)
    }
}

impl Plane {
    fn from_vec4(plane: [f32; 4]) -> Self {
        let length = (plane[0] * plane[0] + plane[1] * plane[1] + plane[2] * plane[2]).sqrt();
        if length <= f32::EPSILON {
            return Self {
                normal: [0.0, 0.0, 0.0],
                distance: plane[3],
            };
        }

        let inv = length.recip();
        Self {
            normal: [plane[0] * inv, plane[1] * inv, plane[2] * inv],
            distance: plane[3] * inv,
        }
    }

    #[inline]
    fn distance_to_point(&self, point: [f32; 3]) -> f32 {
        self.normal[0] * point[0]
            + self.normal[1] * point[1]
            + self.normal[2] * point[2]
            + self.distance
    }
}

#[inline]
fn add_vec4(lhs: [f32; 4], rhs: [f32; 4]) -> [f32; 4] {
    [
        lhs[0] + rhs[0],
        lhs[1] + rhs[1],
        lhs[2] + rhs[2],
        lhs[3] + rhs[3],
    ]
}

#[inline]
fn sub_vec4(lhs: [f32; 4], rhs: [f32; 4]) -> [f32; 4] {
    [
        lhs[0] - rhs[0],
        lhs[1] - rhs[1],
        lhs[2] - rhs[2],
        lhs[3] - rhs[3],
    ]
}

#[cfg(test)]
mod tests {
    use super::Frustum;
    use crate::render::view::Projection;
    use crate::render::Transform;

    #[test]
    fn orthographic_frustum_accepts_origin_and_rejects_far_x() {
        let projection = Projection::orthographic(64.0);
        let frustum = Frustum::from_view_proj(
            projection
                .view_uniform(Transform::default(), [64, 64])
                .view_proj,
        );

        assert!(frustum.intersects_sphere([0.0, 0.0, 0.0], 0.5));
        assert!(!frustum.intersects_sphere([100.0, 0.0, 0.0], 0.5));
    }

    #[test]
    fn perspective_frustum_rejects_points_behind_camera() {
        let projection = Projection::perspective(60.0f32.to_radians(), 0.1, 100.0);
        let frustum = Frustum::from_view_proj(
            projection
                .view_uniform(Transform::default(), [128, 128])
                .view_proj,
        );

        assert!(frustum.intersects_sphere([0.0, 0.0, -2.0], 0.25));
        assert!(!frustum.intersects_sphere([0.0, 0.0, 2.0], 0.25));
    }
}
