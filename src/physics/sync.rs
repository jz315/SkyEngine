use std::sync::Mutex;

use rapier2d::control::KinematicCharacterController;
use rapier2d::prelude::{
    ColliderHandle, Pose, QueryFilter, QueryFilterFlags, RigidBodyBuilder, RigidBodyHandle,
    Rotation, Vector,
};

use crate::ecs::{EntityId, World};
use crate::math::{Transform, Vec2};

use super::components::{BodyType2D, Collider2D, RigidBody2D, Velocity2D};
use super::conversion::{
    active_collision_types, collider_builder, entity_user_data, physics_vec, transform_pose,
    transform_translation,
};
use super::events::{translate_collision_events, PhysicsEvents, StepEventCollector};
use super::world::PhysicsWorld2D;

#[derive(Clone, Copy, Debug)]
struct KinematicMotion {
    body: RigidBodyHandle,
    collider: Option<ColliderHandle>,
    desired_translation: Vector,
    target_rotation: f32,
}

impl PhysicsWorld2D {
    pub(crate) fn step_world(&mut self, world: &World, events: &mut PhysicsEvents, dt: f32) {
        let dt = dt.max(0.0);
        if dt <= f32::EPSILON {
            self.cleanup_removed(world);
            return;
        }

        self.backend.set_timestep(dt);
        self.cleanup_removed(world);
        self.sync_bodies(world, dt);
        self.sync_colliders(world);

        let raw_events = Mutex::new(Vec::new());
        let collector = StepEventCollector {
            events: &raw_events,
        };
        let gravity = physics_vec(
            self.config.gravity.x() / self.pixels_per_meter(),
            self.config.gravity.y() / self.pixels_per_meter(),
        );

        self.backend.step(gravity, &collector);

        if let Ok(raw_events) = raw_events.into_inner() {
            translate_collision_events(
                raw_events,
                &self.handles.collider_entities,
                &self.backend.colliders,
                events,
            );
        }
    }

    pub(crate) fn write_back(&self, world: &mut World) {
        let ppm = self.pixels_per_meter();
        let mut query =
            world.query_mut::<(&mut Transform, &RigidBody2D, Option<&mut Velocity2D>)>();
        query.for_each_with_entity(|entity, (transform, _body, velocity)| {
            let Some(handle) = self.handles.body_handles.get(&entity).copied() else {
                return;
            };
            let Some(rb) = self.backend.bodies.get(handle) else {
                return;
            };
            transform.position[0] = rb.translation().x * ppm;
            transform.position[1] = rb.translation().y * ppm;
            transform.set_rotation_z(rb.rotation().angle());

            if let Some(velocity) = velocity {
                velocity.linear = Vec2::new(rb.linvel().x * ppm, rb.linvel().y * ppm);
                velocity.angular = rb.angvel();
            }
        });
    }

    fn cleanup_removed(&mut self, world: &World) {
        let remove_bodies = self
            .handles
            .body_handles
            .keys()
            .copied()
            .filter(|entity| {
                !world.contains(*entity) || world.get::<RigidBody2D>(*entity).is_none()
            })
            .collect::<Vec<_>>();
        for entity in remove_bodies {
            self.remove_body(entity);
        }

        let remove_colliders = self
            .handles
            .collider_handles
            .keys()
            .copied()
            .filter(|entity| {
                !world.contains(*entity)
                    || world.get::<Collider2D>(*entity).is_none()
                    || world.get::<RigidBody2D>(*entity).is_none()
            })
            .collect::<Vec<_>>();
        for entity in remove_colliders {
            self.remove_collider(entity);
        }
    }

    fn sync_bodies(&mut self, world: &World, dt: f32) {
        let ppm = self.pixels_per_meter();
        let query = world.query::<(&RigidBody2D, &Transform, Option<&Velocity2D>)>();
        let mut kinematic = Vec::new();

        query.for_each_with_entity(|entity, (body, transform, velocity)| {
            let handle = self.ensure_body(entity, *body, *transform, velocity.copied());
            self.update_body(handle, entity, *body, *transform, velocity.copied());

            if body.body_type == BodyType2D::Kinematic {
                if let Some(rb) = self.backend.bodies.get(handle) {
                    let target = if let Some(velocity) = velocity {
                        rb.translation()
                            + physics_vec(velocity.linear.x() / ppm, velocity.linear.y() / ppm) * dt
                    } else {
                        transform_translation(*transform, ppm)
                    };
                    let desired_translation = target - rb.translation();
                    kinematic.push(KinematicMotion {
                        body: handle,
                        collider: self.handles.collider_handles.get(&entity).copied(),
                        desired_translation,
                        target_rotation: transform.rotation_z(),
                    });
                }
            }
        });

        self.apply_kinematic_motions(kinematic, dt);
    }

    fn ensure_body(
        &mut self,
        entity: EntityId,
        body: RigidBody2D,
        transform: Transform,
        velocity: Option<Velocity2D>,
    ) -> RigidBodyHandle {
        if let Some(handle) = self.handles.body_handles.get(&entity).copied() {
            return handle;
        }

        let ppm = self.pixels_per_meter();
        let mut builder = match body.body_type {
            BodyType2D::Static => RigidBodyBuilder::fixed(),
            BodyType2D::Kinematic => RigidBodyBuilder::kinematic_position_based(),
            BodyType2D::Dynamic => RigidBodyBuilder::dynamic(),
        }
        .translation(transform_translation(transform, ppm))
        .rotation(transform.rotation_z())
        .enabled(body.enabled)
        .gravity_scale(body.gravity_scale)
        .can_sleep(body.can_sleep)
        .ccd_enabled(body.ccd_enabled)
        .user_data(entity_user_data(entity));

        if body.lock_rotation {
            builder = builder.lock_rotations();
        }
        if let Some(velocity) = velocity {
            builder = builder
                .linvel(physics_vec(
                    velocity.linear.x() / ppm,
                    velocity.linear.y() / ppm,
                ))
                .angvel(velocity.angular);
        }

        let handle = self.backend.bodies.insert(builder.build());
        self.handles.bind_body(entity, handle, body);
        handle
    }

    fn update_body(
        &mut self,
        handle: RigidBodyHandle,
        entity: EntityId,
        body: RigidBody2D,
        transform: Transform,
        velocity: Option<Velocity2D>,
    ) {
        let ppm = self.pixels_per_meter();
        let Some(rb) = self.backend.bodies.get_mut(handle) else {
            self.handles.drop_body_mapping(entity, handle);
            return;
        };

        if self.handles.body_snapshots.get(&entity).copied() != Some(body) {
            rb.set_body_type(body.body_type.to_rapier(), true);
            rb.set_enabled(body.enabled);
            rb.set_gravity_scale(body.gravity_scale, true);
            rb.enable_ccd(body.ccd_enabled);
            rb.lock_rotations(body.lock_rotation, true);
            self.handles.body_snapshots.insert(entity, body);
        }

        match body.body_type {
            BodyType2D::Static => {
                rb.set_position(transform_pose(transform, ppm), true);
            }
            BodyType2D::Dynamic => {
                let target = transform_translation(transform, ppm);
                if (rb.translation() - target).length_squared() > 1.0e-8 {
                    rb.set_position(transform_pose(transform, ppm), true);
                }
                if let Some(velocity) = velocity {
                    rb.set_linvel(
                        physics_vec(velocity.linear.x() / ppm, velocity.linear.y() / ppm),
                        true,
                    );
                    rb.set_angvel(velocity.angular, true);
                }
            }
            BodyType2D::Kinematic => {
                rb.set_rotation(Rotation::new(transform.rotation_z()), true);
            }
        }
    }

    fn sync_colliders(&mut self, world: &World) {
        let query = world.query::<(&Collider2D, &RigidBody2D, &Transform)>();
        query.for_each_with_entity(|entity, (collider, _body, _transform)| {
            let body_handle = match self.handles.body_handles.get(&entity).copied() {
                Some(handle) => handle,
                None => return,
            };

            if self.handles.collider_snapshots.get(&entity).copied() != Some(*collider) {
                self.remove_collider(entity);
            }

            if self.handles.collider_handles.contains_key(&entity) {
                return;
            }

            let ppm = self.pixels_per_meter();
            let mut builder = collider_builder(*collider, ppm)
                .active_events(rapier2d::prelude::ActiveEvents::COLLISION_EVENTS)
                .active_collision_types(active_collision_types())
                .user_data(entity_user_data(entity));
            builder = builder.translation(physics_vec(
                collider.offset.x() / ppm,
                collider.offset.y() / ppm,
            ));
            builder = builder.rotation(collider.rotation);

            let handle = self.backend.colliders.insert_with_parent(
                builder.build(),
                body_handle,
                &mut self.backend.bodies,
            );
            self.handles.bind_collider(entity, handle, *collider);
        });
    }

    fn apply_kinematic_motions(&mut self, motions: Vec<KinematicMotion>, dt: f32) {
        if motions.is_empty() {
            return;
        }

        let controller = KinematicCharacterController {
            snap_to_ground: None,
            autostep: None,
            slide: true,
            ..Default::default()
        };

        for motion in motions {
            let Some(rb) = self.backend.bodies.get(motion.body) else {
                continue;
            };
            let start_translation = rb.translation();
            let Some(collider_handle) = motion.collider else {
                if let Some(rb) = self.backend.bodies.get_mut(motion.body) {
                    rb.set_next_kinematic_position(Pose::new(
                        start_translation + motion.desired_translation,
                        motion.target_rotation,
                    ));
                }
                continue;
            };

            let movement = {
                let Some(collider) = self.backend.colliders.get(collider_handle) else {
                    continue;
                };
                let character_pos = *collider.position();
                let shape = collider.shape();
                let query = self.backend.query_pipeline(QueryFilter {
                    flags: QueryFilterFlags::EXCLUDE_SENSORS,
                    exclude_rigid_body: Some(motion.body),
                    ..QueryFilter::default()
                });
                controller.move_shape(
                    dt,
                    &query,
                    shape,
                    &character_pos,
                    motion.desired_translation,
                    |_| {},
                )
            };

            let target = start_translation + movement.translation;
            if let Some(rb) = self.backend.bodies.get_mut(motion.body) {
                rb.set_next_kinematic_position(Pose::new(target, motion.target_rotation));
            }
        }
    }

    fn remove_body(&mut self, entity: EntityId) {
        let Some(handle) = self.handles.unbind_body(entity) else {
            return;
        };
        self.backend.remove_body(handle);
        self.handles.retain_live_colliders(&self.backend.colliders);
    }

    fn remove_collider(&mut self, entity: EntityId) {
        let Some(handle) = self.handles.unbind_collider(entity) else {
            return;
        };
        self.backend.remove_collider(handle);
    }
}
