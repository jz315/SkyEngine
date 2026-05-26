//! Small game-oriented geometry helpers.

use crate::{Vec2, Vec3};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb2 {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb2 {
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
    use super::{Aabb2, Aabb3, Ray2, Ray3};
    use crate::{Vec2, Vec3};

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
}
