//! Small game-oriented geometry helpers.

use crate::{Affine2, Affine3, Mat4, Vec2, Vec3, Vec4};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb2 {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb2 {
    pub const EMPTY: Self = Self {
        min: Vec2::splat(f32::INFINITY),
        max: Vec2::splat(f32::NEG_INFINITY),
    };

    pub fn from_points(points: impl IntoIterator<Item = Vec2>) -> Option<Self> {
        let mut points = points.into_iter();
        let first = points.next()?;
        Some(points.fold(
            Self {
                min: first,
                max: first,
            },
            |bounds, point| bounds.union_point(point),
        ))
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.min.x() <= self.max.x()
            && self.min.y() <= self.max.y()
    }
    #[inline]
    pub fn from_min_max(min: Vec2, max: Vec2) -> Self {
        Self {
            min: min.min(max),
            max: min.max(max),
        }
    }

    #[inline]
    pub fn from_center_half_size(center: Vec2, half_size: Vec2) -> Self {
        let half_size = half_size.abs();
        Self {
            min: center - half_size,
            max: center + half_size,
        }
    }

    #[inline]
    pub fn center(self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    #[inline]
    pub fn size(self) -> Vec2 {
        self.max - self.min
    }

    #[inline]
    pub fn half_size(self) -> Vec2 {
        self.size() * 0.5
    }

    #[inline]
    pub fn area(self) -> f32 {
        if self.is_valid() {
            let s = self.size();
            s.x() * s.y()
        } else {
            0.0
        }
    }

    #[inline]
    pub fn contains_aabb(self, rhs: Self) -> bool {
        self.contains_point(rhs.min) && self.contains_point(rhs.max)
    }

    #[inline]
    pub fn contains_point(self, point: Vec2) -> bool {
        point.x() >= self.min.x()
            && point.x() <= self.max.x()
            && point.y() >= self.min.y()
            && point.y() <= self.max.y()
    }

    #[inline]
    pub fn intersects(self, rhs: Self) -> bool {
        self.min.x() <= rhs.max.x()
            && self.max.x() >= rhs.min.x()
            && self.min.y() <= rhs.max.y()
            && self.max.y() >= rhs.min.y()
    }

    #[inline]
    pub fn union(self, rhs: Self) -> Self {
        Self {
            min: self.min.min(rhs.min),
            max: self.max.max(rhs.max),
        }
    }

    #[inline]
    pub fn union_point(self, point: Vec2) -> Self {
        Self {
            min: self.min.min(point),
            max: self.max.max(point),
        }
    }

    #[inline]
    pub fn intersection(self, rhs: Self) -> Option<Self> {
        let result = Self {
            min: self.min.max(rhs.min),
            max: self.max.min(rhs.max),
        };
        result.is_valid().then_some(result)
    }

    #[inline]
    pub fn closest_point(self, point: Vec2) -> Vec2 {
        point.clamp(self.min, self.max)
    }

    #[inline]
    pub fn distance_squared_to_point(self, point: Vec2) -> f32 {
        self.closest_point(point).distance_squared(point)
    }

    #[inline]
    pub fn distance_to_point(self, point: Vec2) -> f32 {
        self.distance_squared_to_point(point).sqrt()
    }

    #[inline]
    pub fn transformed(self, transform: Affine2) -> Self {
        let corners = [
            self.min,
            Vec2::new(self.max.x(), self.min.y()),
            self.max,
            Vec2::new(self.min.x(), self.max.y()),
        ];
        Self::from_points(corners.map(|point| transform.transform_point2(point))).unwrap()
    }

    #[inline]
    pub fn expanded(self, padding: Vec2) -> Self {
        let padding = padding.abs();
        Self {
            min: self.min - padding,
            max: self.max + padding,
        }
    }

    #[inline]
    pub fn translated(self, offset: Vec2) -> Self {
        Self {
            min: self.min + offset,
            max: self.max + offset,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb3 {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb3 {
    pub const EMPTY: Self = Self {
        min: Vec3::splat(f32::INFINITY),
        max: Vec3::splat(f32::NEG_INFINITY),
    };

    pub fn from_points(points: impl IntoIterator<Item = Vec3>) -> Option<Self> {
        let mut points = points.into_iter();
        let first = points.next()?;
        Some(points.fold(
            Self {
                min: first,
                max: first,
            },
            |bounds, point| bounds.union_point(point),
        ))
    }

    #[inline]
    pub fn is_valid(self) -> bool {
        self.min.is_finite()
            && self.max.is_finite()
            && self.min.x() <= self.max.x()
            && self.min.y() <= self.max.y()
            && self.min.z() <= self.max.z()
    }
    #[inline]
    pub fn from_min_max(min: Vec3, max: Vec3) -> Self {
        Self {
            min: min.min(max),
            max: min.max(max),
        }
    }

    #[inline]
    pub fn from_center_half_size(center: Vec3, half_size: Vec3) -> Self {
        let half_size = half_size.abs();
        Self {
            min: center - half_size,
            max: center + half_size,
        }
    }

    #[inline]
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    #[inline]
    pub fn size(self) -> Vec3 {
        self.max - self.min
    }

    #[inline]
    pub fn half_size(self) -> Vec3 {
        self.size() * 0.5
    }

    #[inline]
    pub fn volume(self) -> f32 {
        if self.is_valid() {
            let s = self.size();
            s.x() * s.y() * s.z()
        } else {
            0.0
        }
    }

    #[inline]
    pub fn surface_area(self) -> f32 {
        if !self.is_valid() {
            return 0.0;
        }
        let s = self.size();
        2.0 * (s.x() * s.y() + s.y() * s.z() + s.z() * s.x())
    }

    #[inline]
    pub fn contains_aabb(self, rhs: Self) -> bool {
        self.contains_point(rhs.min) && self.contains_point(rhs.max)
    }

    #[inline]
    pub fn contains_point(self, point: Vec3) -> bool {
        point.x() >= self.min.x()
            && point.x() <= self.max.x()
            && point.y() >= self.min.y()
            && point.y() <= self.max.y()
            && point.z() >= self.min.z()
            && point.z() <= self.max.z()
    }

    #[inline]
    pub fn intersects(self, rhs: Self) -> bool {
        self.min.x() <= rhs.max.x()
            && self.max.x() >= rhs.min.x()
            && self.min.y() <= rhs.max.y()
            && self.max.y() >= rhs.min.y()
            && self.min.z() <= rhs.max.z()
            && self.max.z() >= rhs.min.z()
    }

    #[inline]
    pub fn union(self, rhs: Self) -> Self {
        Self {
            min: self.min.min(rhs.min),
            max: self.max.max(rhs.max),
        }
    }

    #[inline]
    pub fn union_point(self, point: Vec3) -> Self {
        Self {
            min: self.min.min(point),
            max: self.max.max(point),
        }
    }

    #[inline]
    pub fn intersection(self, rhs: Self) -> Option<Self> {
        let result = Self {
            min: self.min.max(rhs.min),
            max: self.max.min(rhs.max),
        };
        result.is_valid().then_some(result)
    }

    #[inline]
    pub fn closest_point(self, point: Vec3) -> Vec3 {
        point.clamp(self.min, self.max)
    }

    #[inline]
    pub fn distance_squared_to_point(self, point: Vec3) -> f32 {
        self.closest_point(point).distance_squared(point)
    }

    #[inline]
    pub fn distance_to_point(self, point: Vec3) -> f32 {
        self.distance_squared_to_point(point).sqrt()
    }

    pub fn transformed(self, transform: Affine3) -> Self {
        let mut points = [Vec3::ZERO; 8];
        let mut index = 0;
        for x in [self.min.x(), self.max.x()] {
            for y in [self.min.y(), self.max.y()] {
                for z in [self.min.z(), self.max.z()] {
                    points[index] = transform.transform_point3(Vec3::new(x, y, z));
                    index += 1;
                }
            }
        }
        Self::from_points(points).unwrap()
    }

    #[inline]
    pub fn expanded(self, padding: Vec3) -> Self {
        let padding = padding.abs();
        Self {
            min: self.min - padding,
            max: self.max + padding,
        }
    }

    #[inline]
    pub fn translated(self, offset: Vec3) -> Self {
        Self {
            min: self.min + offset,
            max: self.max + offset,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray2 {
    pub origin: Vec2,
    pub direction: Vec2,
}

impl Ray2 {
    #[inline]
    pub fn new(origin: Vec2, direction: Vec2) -> Self {
        Self { origin, direction }
    }

    #[inline]
    pub fn from_points(origin: Vec2, target: Vec2) -> Self {
        Self {
            origin,
            direction: target - origin,
        }
    }

    #[inline]
    pub fn at(self, t: f32) -> Vec2 {
        self.origin + self.direction * t
    }

    #[inline]
    pub fn normalized(self) -> Option<Self> {
        Some(Self {
            origin: self.origin,
            direction: self.direction.try_normalized()?,
        })
    }

    #[inline]
    pub fn intersect_aabb(self, aabb: Aabb2) -> Option<(f32, f32)> {
        let mut t_min = 0.0;
        let mut t_max = f32::INFINITY;
        if !slab(
            self.origin.x(),
            self.direction.x(),
            aabb.min.x(),
            aabb.max.x(),
            &mut t_min,
            &mut t_max,
        ) {
            return None;
        }
        if !slab(
            self.origin.y(),
            self.direction.y(),
            aabb.min.y(),
            aabb.max.y(),
            &mut t_min,
            &mut t_max,
        ) {
            return None;
        }
        Some((t_min, t_max))
    }

    pub fn intersect_circle(self, circle: Circle, max_t: f32) -> Option<f32> {
        quadratic_ray_hit(
            self.origin - circle.center,
            self.direction,
            circle.radius,
            max_t,
        )
    }

    pub fn intersect_triangle(self, triangle: Triangle2, max_t: f32) -> Option<f32> {
        let mut best = None;
        for (start, end) in [
            (triangle.a, triangle.b),
            (triangle.b, triangle.c),
            (triangle.c, triangle.a),
        ] {
            let edge = end - start;
            let denominator = self.direction.perp_dot(edge);
            if denominator.abs() <= 1e-7 {
                continue;
            }
            let offset = start - self.origin;
            let t = offset.perp_dot(edge) / denominator;
            let u = offset.perp_dot(self.direction) / denominator;
            if t >= 0.0 && t <= max_t && (0.0..=1.0).contains(&u) {
                best = Some(best.map_or(t, |current: f32| current.min(t)));
            }
        }
        best
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray3 {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray3 {
    #[inline]
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self { origin, direction }
    }

    #[inline]
    pub fn from_points(origin: Vec3, target: Vec3) -> Self {
        Self {
            origin,
            direction: target - origin,
        }
    }

    #[inline]
    pub fn at(self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }

    #[inline]
    pub fn normalized(self) -> Option<Self> {
        Some(Self {
            origin: self.origin,
            direction: self.direction.try_normalized()?,
        })
    }

    #[inline]
    pub fn intersect_aabb(self, aabb: Aabb3) -> Option<(f32, f32)> {
        let mut t_min = 0.0;
        let mut t_max = f32::INFINITY;
        if !slab(
            self.origin.x(),
            self.direction.x(),
            aabb.min.x(),
            aabb.max.x(),
            &mut t_min,
            &mut t_max,
        ) {
            return None;
        }
        if !slab(
            self.origin.y(),
            self.direction.y(),
            aabb.min.y(),
            aabb.max.y(),
            &mut t_min,
            &mut t_max,
        ) {
            return None;
        }
        if !slab(
            self.origin.z(),
            self.direction.z(),
            aabb.min.z(),
            aabb.max.z(),
            &mut t_min,
            &mut t_max,
        ) {
            return None;
        }
        Some((t_min, t_max))
    }

    pub fn intersect_sphere(self, sphere: Sphere, max_t: f32) -> Option<f32> {
        quadratic_ray_hit(
            self.origin - sphere.center,
            self.direction,
            sphere.radius,
            max_t,
        )
    }

    #[inline]
    pub fn intersect_plane(self, plane: Plane, max_t: f32) -> Option<f32> {
        plane.intersect_ray(self, max_t)
    }

    pub fn intersect_triangle(self, triangle: Triangle3, max_t: f32) -> Option<RayTriangleHit> {
        let edge1 = triangle.b - triangle.a;
        let edge2 = triangle.c - triangle.a;
        let p = self.direction.cross(edge2);
        let determinant = edge1.dot(p);
        if determinant.abs() <= 1e-7 {
            return None;
        }
        let inverse = determinant.recip();
        let offset = self.origin - triangle.a;
        let u = offset.dot(p) * inverse;
        if !(0.0..=1.0).contains(&u) {
            return None;
        }
        let q = offset.cross(edge1);
        let v = self.direction.dot(q) * inverse;
        if v < 0.0 || u + v > 1.0 {
            return None;
        }
        let t = edge2.dot(q) * inverse;
        (t >= 0.0 && t <= max_t).then_some(RayTriangleHit {
            t,
            barycentric: Vec3::new(1.0 - u - v, u, v),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    pub center: Vec2,
    pub radius: f32,
}
impl Circle {
    pub fn new(center: Vec2, radius: f32) -> Self {
        Self {
            center,
            radius: radius.max(0.0),
        }
    }
    pub fn contains_point(self, point: Vec2) -> bool {
        self.center.distance_squared(point) <= self.radius * self.radius
    }
    pub fn intersects(self, rhs: Self) -> bool {
        self.center.distance_squared(rhs.center) <= (self.radius + rhs.radius).powi(2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
}
impl Sphere {
    pub fn new(center: Vec3, radius: f32) -> Self {
        Self {
            center,
            radius: radius.max(0.0),
        }
    }
    pub fn contains_point(self, point: Vec3) -> bool {
        self.center.distance_squared(point) <= self.radius * self.radius
    }
    pub fn intersects(self, rhs: Self) -> bool {
        self.center.distance_squared(rhs.center) <= (self.radius + rhs.radius).powi(2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle2 {
    pub a: Vec2,
    pub b: Vec2,
    pub c: Vec2,
}
impl Triangle2 {
    pub const fn new(a: Vec2, b: Vec2, c: Vec2) -> Self {
        Self { a, b, c }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triangle3 {
    pub a: Vec3,
    pub b: Vec3,
    pub c: Vec3,
}
impl Triangle3 {
    pub const fn new(a: Vec3, b: Vec3, c: Vec3) -> Self {
        Self { a, b, c }
    }
    pub fn normal(self) -> Option<Vec3> {
        (self.b - self.a).cross(self.c - self.a).try_normalized()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayTriangleHit {
    pub t: f32,
    pub barycentric: Vec3,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    normal: Vec3,
    distance: f32,
}
impl Plane {
    pub fn from_normal_distance(normal: Vec3, distance: f32) -> Option<Self> {
        let length = normal.length();
        if !length.is_finite() || length <= f32::EPSILON || !distance.is_finite() {
            return None;
        }
        Some(Self {
            normal: normal / length,
            distance: distance / length,
        })
    }
    pub fn from_point_normal(point: Vec3, normal: Vec3) -> Option<Self> {
        let normal = normal.try_normalized()?;
        Some(Self {
            normal,
            distance: -normal.dot(point),
        })
    }
    pub const fn normal(self) -> Vec3 {
        self.normal
    }
    pub const fn distance(self) -> f32 {
        self.distance
    }
    pub fn signed_distance_to_point(self, point: Vec3) -> f32 {
        self.normal.dot(point) + self.distance
    }
    pub fn project_point(self, point: Vec3) -> Vec3 {
        point - self.normal * self.signed_distance_to_point(point)
    }
    pub fn intersect_ray(self, ray: Ray3, max_t: f32) -> Option<f32> {
        let denominator = self.normal.dot(ray.direction);
        if denominator.abs() <= 1e-7 {
            return None;
        }
        let t = -self.signed_distance_to_point(ray.origin) / denominator;
        (t >= 0.0 && t <= max_t).then_some(t)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frustum {
    planes: [Plane; 6],
}
impl Frustum {
    /// Compatibility constructor for column-major view-projection arrays.
    #[inline]
    pub fn from_view_proj(view_projection: [f32; 16]) -> Self {
        Self::from_view_projection(Mat4::from_cols_array(view_projection))
    }

    pub fn from_view_projection(view_projection: Mat4) -> Self {
        let m = view_projection.to_cols_array();
        let x = Vec4::new(m[0], m[4], m[8], m[12]);
        let y = Vec4::new(m[1], m[5], m[9], m[13]);
        let z = Vec4::new(m[2], m[6], m[10], m[14]);
        let w = Vec4::new(m[3], m[7], m[11], m[15]);
        Self {
            planes: [
                plane_from_vec4(w + x),
                plane_from_vec4(w - x),
                plane_from_vec4(w + y),
                plane_from_vec4(w - y),
                plane_from_vec4(z),
                plane_from_vec4(w - z),
            ],
        }
    }
    pub fn contains_point(&self, point: Vec3) -> bool {
        self.planes
            .iter()
            .all(|plane| plane.signed_distance_to_point(point) >= 0.0)
    }
    pub fn intersects_sphere_shape(&self, sphere: Sphere) -> bool {
        if !sphere.radius.is_finite() {
            return true;
        }
        self.planes
            .iter()
            .all(|plane| plane.signed_distance_to_point(sphere.center) >= -sphere.radius)
    }

    /// Compatibility entry point accepting either `Vec3` or `[f32; 3]` centers.
    #[inline]
    pub fn intersects_sphere(&self, center: impl Into<Vec3>, radius: f32) -> bool {
        self.intersects_sphere_shape(Sphere::new(center.into(), radius))
    }
    pub fn intersects_aabb(&self, aabb: Aabb3) -> bool {
        self.planes.iter().all(|plane| {
            let n = plane.normal;
            let positive = Vec3::new(
                if n.x() >= 0.0 {
                    aabb.max.x()
                } else {
                    aabb.min.x()
                },
                if n.y() >= 0.0 {
                    aabb.max.y()
                } else {
                    aabb.min.y()
                },
                if n.z() >= 0.0 {
                    aabb.max.z()
                } else {
                    aabb.min.z()
                },
            );
            plane.signed_distance_to_point(positive) >= 0.0
        })
    }
}

fn plane_from_vec4(value: Vec4) -> Plane {
    Plane::from_normal_distance(value.truncate(), value.w()).unwrap_or(Plane {
        normal: Vec3::ZERO,
        distance: f32::INFINITY,
    })
}

trait RayScalarVector: Copy {
    fn dot(self, rhs: Self) -> f32;
}
impl RayScalarVector for Vec2 {
    fn dot(self, rhs: Self) -> f32 {
        self.dot(rhs)
    }
}
impl RayScalarVector for Vec3 {
    fn dot(self, rhs: Self) -> f32 {
        self.dot(rhs)
    }
}
fn quadratic_ray_hit<V>(offset: V, direction: V, radius: f32, max_t: f32) -> Option<f32>
where
    V: RayScalarVector + std::ops::Add<Output = V> + std::ops::Mul<f32, Output = V>,
{
    let a = direction.dot(direction);
    if a <= f32::EPSILON {
        return None;
    }
    let half_b = offset.dot(direction);
    let c = offset.dot(offset) - radius * radius;
    let discriminant = half_b * half_b - a * c;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let near = (-half_b - root) / a;
    let far = (-half_b + root) / a;
    [near, far].into_iter().find(|t| *t >= 0.0 && *t <= max_t)
}

#[inline]
fn slab(origin: f32, direction: f32, min: f32, max: f32, t_min: &mut f32, t_max: &mut f32) -> bool {
    if direction.abs() <= f32::EPSILON {
        return origin >= min && origin <= max;
    }

    let inv_direction = direction.recip();
    let mut near = (min - origin) * inv_direction;
    let mut far = (max - origin) * inv_direction;
    if near > far {
        std::mem::swap(&mut near, &mut far);
    }

    *t_min = t_min.max(near);
    *t_max = t_max.min(far);
    *t_min <= *t_max
}

#[cfg(test)]
mod tests {
    use super::{Aabb2, Aabb3, Circle, Frustum, Plane, Ray2, Ray3, Sphere, Triangle2, Triangle3};
    use crate::{Mat4, Vec2, Vec3};

    #[test]
    fn aabb2_contains_intersects_and_unions() {
        let a = Aabb2::from_center_half_size(Vec2::ZERO, Vec2::new(2.0, 3.0));
        let b = Aabb2::from_min_max(Vec2::new(1.0, 2.0), Vec2::new(4.0, 5.0));

        assert!(a.contains_point(Vec2::new(1.5, -2.5)));
        assert!(!a.contains_point(Vec2::new(2.5, 0.0)));
        assert!(a.intersects(b));
        assert_eq!(a.union(b).max.to_array(), [4.0, 5.0]);
    }

    #[test]
    fn aabb3_contains_intersects_and_expands() {
        let a = Aabb3::from_min_max(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
        let b = a.translated(Vec3::new(1.5, 0.0, 0.0));

        assert!(a.contains_point(Vec3::ZERO));
        assert!(a.intersects(b));
        assert_eq!(
            a.expanded(Vec3::splat(1.0)).size().to_array(),
            [4.0, 4.0, 4.0]
        );
    }

    #[test]
    fn ray2_intersects_aabb_with_non_unit_direction() {
        let ray = Ray2::new(Vec2::new(-4.0, 0.0), Vec2::new(2.0, 0.0));
        let aabb = Aabb2::from_min_max(Vec2::new(-1.0, -1.0), Vec2::new(1.0, 1.0));
        let (near, far) = ray.intersect_aabb(aabb).unwrap();

        assert_eq!(near, 1.5);
        assert_eq!(far, 2.5);
    }

    #[test]
    fn ray3_parallel_miss_returns_none() {
        let ray = Ray3::new(Vec3::new(0.0, 2.0, 0.0), Vec3::X);
        let aabb = Aabb3::from_min_max(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));

        assert!(ray.intersect_aabb(aabb).is_none());
    }

    #[test]
    fn bounds_extensions_handle_empty_intersection_and_distance() {
        assert!(!Aabb2::EMPTY.is_valid());
        assert!(Aabb2::from_points([]).is_none());
        let a = Aabb2::from_min_max(Vec2::ZERO, Vec2::new(2.0, 3.0));
        let b = Aabb2::from_min_max(Vec2::new(2.0, 1.0), Vec2::new(4.0, 2.0));
        assert_eq!(a.area(), 6.0);
        assert!(a.intersection(b).is_some());
        assert_eq!(a.distance_squared_to_point(Vec2::new(5.0, 1.0)), 9.0);
        let volume = Aabb3::from_min_max(Vec3::ZERO, Vec3::new(2.0, 3.0, 4.0));
        assert_eq!(volume.volume(), 24.0);
        assert_eq!(volume.surface_area(), 52.0);
    }

    #[test]
    fn non_unit_rays_hit_tangent_shapes_and_reject_reverse_hits() {
        let ray2 = Ray2::new(Vec2::new(-2.0, 1.0), Vec2::new(2.0, 0.0));
        assert_eq!(
            ray2.intersect_circle(Circle::new(Vec2::ZERO, 1.0), 10.0),
            Some(1.0)
        );
        let ray3 = Ray3::new(Vec3::new(-2.0, 1.0, 0.0), Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(
            ray3.intersect_sphere(Sphere::new(Vec3::ZERO, 1.0), 10.0),
            Some(1.0)
        );
        assert!(Ray3::new(Vec3::new(2.0, 0.0, 0.0), Vec3::X)
            .intersect_sphere(Sphere::new(Vec3::ZERO, 1.0), 10.0)
            .is_none());
    }

    #[test]
    fn plane_and_triangles_cover_parallel_degenerate_and_barycentric_cases() {
        let plane = Plane::from_point_normal(Vec3::ZERO, Vec3::Z).unwrap();
        assert_eq!(
            Ray3::new(Vec3::Z, -Vec3::Z).intersect_plane(plane, 2.0),
            Some(1.0)
        );
        assert!(Ray3::new(Vec3::Z, Vec3::X)
            .intersect_plane(plane, 2.0)
            .is_none());
        let triangle = Triangle3::new(Vec3::ZERO, Vec3::X, Vec3::Y);
        let hit = Ray3::new(Vec3::new(0.25, 0.25, 1.0), -Vec3::Z)
            .intersect_triangle(triangle, 2.0)
            .unwrap();
        assert!((hit.barycentric.x() - 0.5).abs() < 1e-6);
        assert!(Triangle3::new(Vec3::ZERO, Vec3::X, Vec3::X)
            .normal()
            .is_none());
        let edge = Triangle2::new(Vec2::ZERO, Vec2::Y, Vec2::new(0.0, 2.0));
        assert!(Ray2::new(Vec2::new(-1.0, 0.5), Vec2::X)
            .intersect_triangle(edge, 2.0)
            .is_some());
    }

    #[test]
    fn frustum_uses_zero_to_one_depth_and_is_conservative_for_invalid_radius() {
        let frustum = Frustum::from_view_projection(Mat4::perspective_rh(
            60f32.to_radians(),
            1.0,
            0.1,
            100.0,
        ));
        assert!(frustum.contains_point(Vec3::new(0.0, 0.0, -2.0)));
        assert!(!frustum.contains_point(Vec3::new(0.0, 0.0, 2.0)));
        assert!(frustum
            .intersects_sphere_shape(Sphere::new(Vec3::new(1000.0, 0.0, 0.0), f32::INFINITY)));
        let bounds = Aabb3::from_center_half_size(Vec3::new(0.0, 0.0, -2.0), Vec3::splat(0.5));
        assert!(frustum.intersects_aabb(bounds));
    }
}
