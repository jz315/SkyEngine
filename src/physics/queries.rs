use rapier2d::prelude::{Pose, QueryFilter, QueryFilterFlags, Ray};

use crate::ecs::EntityId;
use crate::math::Vec2;

use super::components::{ColliderShape2D, CollisionGroups2D};
use super::conversion::{collider_builder_for_shape, physics_vec};
use super::world::PhysicsWorld2D;

/// SkyEngine-owned filter for physics scene queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhysicsQueryFilter2D {
    /// Entity to exclude from the query.
    ///
    /// If the entity has both a body and collider, both are excluded.
    pub exclude_entity: Option<EntityId>,
    /// Whether sensor/trigger colliders are returned.
    pub include_sensors: bool,
    /// Whether non-sensor solid colliders are returned.
    pub include_solids: bool,
    /// Optional collision group mask used by the query.
    pub collision_groups: Option<CollisionGroups2D>,
}

impl PhysicsQueryFilter2D {
    /// Creates a filter that includes all colliders.
    #[inline]
    pub const fn new() -> Self {
        Self {
            exclude_entity: None,
            include_sensors: true,
            include_solids: true,
            collision_groups: None,
        }
    }

    /// Excludes one entity's body/collider from the query.
    #[inline]
    pub const fn exclude_entity(mut self, entity: EntityId) -> Self {
        self.exclude_entity = Some(entity);
        self
    }

    /// Includes or excludes sensor/trigger colliders.
    #[inline]
    pub const fn include_sensors(mut self, include: bool) -> Self {
        self.include_sensors = include;
        self
    }

    /// Includes or excludes non-sensor solid colliders.
    #[inline]
    pub const fn include_solids(mut self, include: bool) -> Self {
        self.include_solids = include;
        self
    }

    /// Returns only solid colliders.
    #[inline]
    pub const fn solids_only(self) -> Self {
        self.include_sensors(false).include_solids(true)
    }

    /// Returns only sensor/trigger colliders.
    #[inline]
    pub const fn sensors_only(self) -> Self {
        self.include_sensors(true).include_solids(false)
    }

    /// Restricts the query to colliders compatible with the given groups.
    #[inline]
    pub const fn collision_groups(mut self, groups: CollisionGroups2D) -> Self {
        self.collision_groups = Some(groups);
        self
    }
}

impl Default for PhysicsQueryFilter2D {
    fn default() -> Self {
        Self::new()
    }
}

/// Hit result returned by [`PhysicsWorld2D::raycast`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RaycastHit2D {
    pub entity: EntityId,
    /// Hit point in render/world units.
    pub point: Vec2,
    /// Distance from the ray origin in render/world units.
    pub distance: f32,
}

impl PhysicsWorld2D {
    /// Returns the closest ray hit, if any.
    ///
    /// `origin`, `direction`, and `max_distance` use render/world units.
    /// `solid` follows Rapier ray semantics: when true, a ray starting inside a
    /// shape reports a zero-distance hit.
    pub fn raycast(
        &self,
        origin: Vec2,
        direction: Vec2,
        max_distance: f32,
        solid: bool,
    ) -> Option<RaycastHit2D> {
        self.raycast_with_filter(
            origin,
            direction,
            max_distance,
            solid,
            PhysicsQueryFilter2D::default(),
        )
    }

    /// Returns the closest ray hit using a SkyEngine query filter.
    pub fn raycast_with_filter(
        &self,
        origin: Vec2,
        direction: Vec2,
        max_distance: f32,
        solid: bool,
        filter: PhysicsQueryFilter2D,
    ) -> Option<RaycastHit2D> {
        let length = direction.length();
        if length <= f32::EPSILON {
            return None;
        }

        let ppm = self.pixels_per_meter();
        let origin = physics_vec(origin.x() / ppm, origin.y() / ppm);
        let direction = physics_vec(direction.x() / length, direction.y() / length);
        let max_toi = max_distance.max(0.0) / ppm;
        let ray = Ray::new(origin, direction);
        let query = self.backend.query_pipeline(self.to_rapier_filter(filter));
        let (collider, toi) = query.cast_ray(&ray, max_toi, solid)?;
        let entity = *self.handles.collider_entities.get(&collider)?;
        Some(raycast_hit(entity, &ray, toi, ppm))
    }

    /// Returns all ray hits sorted by distance from nearest to farthest.
    pub fn raycast_all(
        &self,
        origin: Vec2,
        direction: Vec2,
        max_distance: f32,
        solid: bool,
    ) -> Vec<RaycastHit2D> {
        self.raycast_all_with_filter(
            origin,
            direction,
            max_distance,
            solid,
            PhysicsQueryFilter2D::default(),
        )
    }

    /// Returns all filtered ray hits sorted by distance from nearest to farthest.
    pub fn raycast_all_with_filter(
        &self,
        origin: Vec2,
        direction: Vec2,
        max_distance: f32,
        solid: bool,
        filter: PhysicsQueryFilter2D,
    ) -> Vec<RaycastHit2D> {
        let length = direction.length();
        if length <= f32::EPSILON {
            return Vec::new();
        }

        let ppm = self.pixels_per_meter();
        let origin = physics_vec(origin.x() / ppm, origin.y() / ppm);
        let direction = physics_vec(direction.x() / length, direction.y() / length);
        let max_toi = max_distance.max(0.0) / ppm;
        let ray = Ray::new(origin, direction);
        let query = self.backend.query_pipeline(self.to_rapier_filter(filter));
        let mut hits = query
            .intersect_ray(ray, max_toi, solid)
            .filter_map(|(handle, _collider, hit)| {
                let entity = self.handles.collider_entities.get(&handle).copied()?;
                Some(raycast_hit(entity, &ray, hit.time_of_impact, ppm))
            })
            .collect::<Vec<_>>();
        hits.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        hits
    }

    /// Returns entities whose colliders overlap the shape at `center`.
    pub fn overlap_shape(&self, center: Vec2, shape: ColliderShape2D) -> Vec<EntityId> {
        self.overlap_shape_with_filter(center, shape, PhysicsQueryFilter2D::default())
    }

    /// Returns filtered entities whose colliders overlap the shape at `center`.
    pub fn overlap_shape_with_filter(
        &self,
        center: Vec2,
        shape: ColliderShape2D,
        filter: PhysicsQueryFilter2D,
    ) -> Vec<EntityId> {
        let ppm = self.pixels_per_meter();
        let collider = collider_builder_for_shape(shape, ppm).build();
        let pose = Pose::new(physics_vec(center.x() / ppm, center.y() / ppm), 0.0);
        let query = self.backend.query_pipeline(self.to_rapier_filter(filter));
        query
            .intersect_shape(pose, collider.shape())
            .filter_map(|(handle, _)| self.handles.collider_entities.get(&handle).copied())
            .collect()
    }

    fn to_rapier_filter(&self, filter: PhysicsQueryFilter2D) -> QueryFilter<'_> {
        let mut flags = QueryFilterFlags::default();
        if !filter.include_sensors {
            flags |= QueryFilterFlags::EXCLUDE_SENSORS;
        }
        if !filter.include_solids {
            flags |= QueryFilterFlags::EXCLUDE_SOLIDS;
        }

        let mut query = QueryFilter {
            flags,
            groups: filter.collision_groups.map(CollisionGroups2D::to_rapier),
            ..QueryFilter::default()
        };
        if let Some(entity) = filter.exclude_entity {
            query.exclude_rigid_body = self.handles.body_handles.get(&entity).copied();
            query.exclude_collider = self.handles.collider_handles.get(&entity).copied();
        }
        query
    }
}

fn raycast_hit(entity: EntityId, ray: &Ray, toi: f32, pixels_per_meter: f32) -> RaycastHit2D {
    let point = ray.origin + ray.dir * toi;
    RaycastHit2D {
        entity,
        point: Vec2::new(point.x * pixels_per_meter, point.y * pixels_per_meter),
        distance: toi * pixels_per_meter,
    }
}
