use crate::ecs::World;
use crate::math::{Transform, Vec2};

use super::*;

fn run_fixed(world: &mut World, dt: f32) {
    world.tick_with_delta(dt);
}

#[test]
fn physics_feature_does_not_require_app() {
    let mut world = World::new();
    install_physics(&mut world, PhysicsConfig2D::default());
    assert!(world.get_resource::<PhysicsWorld2D>().is_some());
}

#[test]
fn reinstall_updates_fixed_step() {
    let mut world = World::new();
    install_physics(&mut world, PhysicsConfig2D::default());
    install_physics(
        &mut world,
        PhysicsConfig2D {
            fixed_dt: 1.0 / 30.0,
            ..Default::default()
        },
    );
    world.spawn((
        Transform::from_xy(0.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(16.0, 16.0),
    ));

    run_fixed(&mut world, 1.0 / 45.0);
    assert_eq!(
        world.get_resource::<PhysicsWorld2D>().unwrap().body_count(),
        0
    );

    run_fixed(&mut world, 1.0 / 45.0);
    assert_eq!(
        world.get_resource::<PhysicsWorld2D>().unwrap().body_count(),
        1
    );
}

#[test]
fn dynamic_body_syncs_transform_and_velocity() {
    let mut world = World::new();
    install_physics(
        &mut world,
        PhysicsConfig2D {
            gravity: Vec2::new(0.0, -32.0),
            ..Default::default()
        },
    );
    let entity = world.spawn((
        Transform::from_xy(0.0, 32.0),
        RigidBody2D::dynamic().lock_rotation(),
        Collider2D::circle(8.0),
        Velocity2D::default(),
    ));

    run_fixed(&mut world, 1.0 / 60.0);

    let transform = world.get::<Transform>(entity).unwrap();
    let velocity = world.get::<Velocity2D>(entity).unwrap();
    assert!(transform.position.y() < 32.0);
    assert!(velocity.linear.y() < 0.0);
}

#[test]
fn kinematic_body_is_blocked_by_static_collider() {
    let mut world = World::new();
    install_physics(&mut world, PhysicsConfig2D::default());
    world.spawn((
        Transform::from_xy(32.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(32.0, 32.0),
    ));
    let player = world.spawn((
        Transform::from_xy(0.0, 0.0),
        RigidBody2D::kinematic(),
        Collider2D::rectangle(16.0, 16.0),
        Velocity2D::new(640.0, 0.0),
    ));

    for _ in 0..20 {
        run_fixed(&mut world, 1.0 / 60.0);
    }

    let transform = world.get::<Transform>(player).unwrap();
    assert!(transform.position.x() < 24.0);
}

#[test]
fn trigger_events_are_queued() {
    let mut world = World::new();
    install_physics(&mut world, PhysicsConfig2D::default());
    let trigger = world.spawn((
        Transform::from_xy(16.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(16.0, 16.0).sensor(true),
    ));
    let player = world.spawn((
        Transform::from_xy(0.0, 0.0),
        RigidBody2D::kinematic(),
        Collider2D::rectangle(8.0, 8.0),
        Velocity2D::new(64.0, 0.0),
    ));

    for _ in 0..20 {
        run_fixed(&mut world, 1.0 / 60.0);
    }

    let events = world.get_resource::<PhysicsEvents>().unwrap();
    assert!(events.iter().any(|event| {
        matches!(
            event,
            PhysicsEvent2D::TriggerEntered { trigger: t, other: o }
                if *t == trigger && *o == player
        )
    }));
}

#[test]
fn raycast_and_overlap_return_entities() {
    let mut world = World::new();
    install_physics(&mut world, PhysicsConfig2D::default());
    let wall = world.spawn((
        Transform::from_xy(32.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(16.0, 16.0),
    ));
    run_fixed(&mut world, 1.0 / 60.0);

    let physics = world.get_resource::<PhysicsWorld2D>().unwrap();
    let hit = physics
        .raycast(Vec2::ZERO, Vec2::new(1.0, 0.0), 128.0, true)
        .unwrap();
    assert_eq!(hit.entity, wall);

    let overlaps = physics.overlap_shape(Vec2::new(32.0, 0.0), ColliderShape2D::circle(16.0));
    assert!(overlaps.contains(&wall));
}

#[test]
fn raycast_filter_can_exclude_entity_and_sensors() {
    let mut world = World::new();
    install_physics(&mut world, PhysicsConfig2D::default());
    let trigger = world.spawn((
        Transform::from_xy(16.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(8.0, 8.0).sensor(true),
    ));
    let near_wall = world.spawn((
        Transform::from_xy(32.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(8.0, 8.0),
    ));
    let far_wall = world.spawn((
        Transform::from_xy(64.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(8.0, 8.0),
    ));
    run_fixed(&mut world, 1.0 / 60.0);

    let physics = world.get_resource::<PhysicsWorld2D>().unwrap();
    let first = physics
        .raycast(Vec2::ZERO, Vec2::new(1.0, 0.0), 128.0, true)
        .unwrap();
    assert_eq!(first.entity, trigger);

    let solid_hit = physics
        .raycast_with_filter(
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            128.0,
            true,
            PhysicsQueryFilter2D::default().include_sensors(false),
        )
        .unwrap();
    assert_eq!(solid_hit.entity, near_wall);

    let excluded_hit = physics
        .raycast_with_filter(
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            128.0,
            true,
            PhysicsQueryFilter2D::default()
                .include_sensors(false)
                .exclude_entity(near_wall),
        )
        .unwrap();
    assert_eq!(excluded_hit.entity, far_wall);
}

#[test]
fn raycast_all_returns_hits_sorted_by_distance() {
    let mut world = World::new();
    install_physics(&mut world, PhysicsConfig2D::default());
    let first = world.spawn((
        Transform::from_xy(24.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(8.0, 8.0),
    ));
    let second = world.spawn((
        Transform::from_xy(48.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(8.0, 8.0),
    ));
    let third = world.spawn((
        Transform::from_xy(72.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(8.0, 8.0),
    ));
    run_fixed(&mut world, 1.0 / 60.0);

    let hits = world.get_resource::<PhysicsWorld2D>().unwrap().raycast_all(
        Vec2::ZERO,
        Vec2::new(1.0, 0.0),
        128.0,
        true,
    );

    let entities = hits.iter().map(|hit| hit.entity).collect::<Vec<_>>();
    assert_eq!(entities, vec![first, second, third]);
    assert!(hits
        .windows(2)
        .all(|pair| pair[0].distance <= pair[1].distance));
}

#[test]
fn overlap_filter_splits_sensors_and_solids() {
    let mut world = World::new();
    install_physics(&mut world, PhysicsConfig2D::default());
    let solid = world.spawn((
        Transform::from_xy(0.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(16.0, 16.0),
    ));
    let trigger = world.spawn((
        Transform::from_xy(0.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(16.0, 16.0).sensor(true),
    ));
    run_fixed(&mut world, 1.0 / 60.0);

    let physics = world.get_resource::<PhysicsWorld2D>().unwrap();
    let solids = physics.overlap_shape_with_filter(
        Vec2::ZERO,
        ColliderShape2D::circle(16.0),
        PhysicsQueryFilter2D::default().solids_only(),
    );
    let sensors = physics.overlap_shape_with_filter(
        Vec2::ZERO,
        ColliderShape2D::circle(16.0),
        PhysicsQueryFilter2D::default().sensors_only(),
    );

    assert_eq!(solids, vec![solid]);
    assert_eq!(sensors, vec![trigger]);
}

#[test]
fn query_filter_respects_collision_groups() {
    let mut world = World::new();
    install_physics(&mut world, PhysicsConfig2D::default());
    let blue_group = CollisionGroups2D::new(0b0001, 0b0010);
    let red_group = CollisionGroups2D::new(0b0100, 0b0010);
    let query_group = CollisionGroups2D::new(0b0010, 0b0001);
    let blue = world.spawn((
        Transform::from_xy(24.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(8.0, 8.0).collision_groups(blue_group),
    ));
    world.spawn((
        Transform::from_xy(48.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(8.0, 8.0).collision_groups(red_group),
    ));
    run_fixed(&mut world, 1.0 / 60.0);

    let hits = world
        .get_resource::<PhysicsWorld2D>()
        .unwrap()
        .raycast_all_with_filter(
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            128.0,
            true,
            PhysicsQueryFilter2D::default().collision_groups(query_group),
        );

    let entities = hits.iter().map(|hit| hit.entity).collect::<Vec<_>>();
    assert_eq!(entities, vec![blue]);
}

#[test]
fn despawn_cleans_internal_handles() {
    let mut world = World::new();
    install_physics(&mut world, PhysicsConfig2D::default());
    let entity = world.spawn((
        Transform::from_xy(0.0, 0.0),
        RigidBody2D::static_body(),
        Collider2D::rectangle(16.0, 16.0),
    ));
    run_fixed(&mut world, 1.0 / 60.0);
    assert_eq!(
        world.get_resource::<PhysicsWorld2D>().unwrap().body_count(),
        1
    );

    assert!(world.despawn(entity));
    run_fixed(&mut world, 1.0 / 60.0);

    let physics = world.get_resource::<PhysicsWorld2D>().unwrap();
    assert_eq!(physics.body_count(), 0);
    assert_eq!(physics.collider_count(), 0);
}
