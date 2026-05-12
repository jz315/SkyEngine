use std::f32::consts::{PI, TAU};

use crate::ecs::{EntityId, System, World};
use crate::math::{Transform, Vec2};
use crate::plugin::{Plugin, PluginResult};
use crate::render::{Color, SortingLayer, SpriteRenderer};

use super::components::{BodyType2D, Collider2D, ColliderShape2D, RigidBody2D};

/// Rendering options for the physics debug overlay.
///
/// Available behind `app + physics`. The overlay is implemented with regular
/// `SpriteRenderer` line segments so it works with the existing 2D renderer.
#[derive(Clone, Copy, Debug)]
pub struct PhysicsDebugDrawOptions2D {
    /// Whether debug line entities are visible.
    pub enabled: bool,
    /// Line thickness in render/world units.
    pub line_thickness: f32,
    /// Z value used for all debug line transforms.
    pub z: f32,
    /// Sorting layer assigned to debug line sprites.
    pub sorting_layer: SortingLayer,
    /// Color for enabled static solid colliders.
    pub static_color: Color,
    /// Color for enabled kinematic solid colliders.
    pub kinematic_color: Color,
    /// Color for enabled dynamic solid colliders.
    pub dynamic_color: Color,
    /// Color for enabled sensor/trigger colliders.
    pub trigger_color: Color,
    /// Color for disabled bodies or colliders.
    pub disabled_color: Color,
    /// Segment count used for circles and capsule arcs.
    pub circle_segments: usize,
}

impl Default for PhysicsDebugDrawOptions2D {
    fn default() -> Self {
        Self {
            enabled: true,
            line_thickness: 2.0,
            z: 0.8,
            sorting_layer: SortingLayer(10_000),
            static_color: Color::new(0.20, 0.82, 1.0, 0.78),
            kinematic_color: Color::new(1.0, 0.86, 0.18, 0.84),
            dynamic_color: Color::new(0.42, 1.0, 0.34, 0.78),
            trigger_color: Color::new(1.0, 0.22, 0.92, 0.72),
            disabled_color: Color::new(0.62, 0.64, 0.68, 0.42),
            circle_segments: 32,
        }
    }
}

/// Resource that owns reusable entities for the physics debug overlay.
///
/// You normally install this with [`PhysicsDebugPlugin`]. The resource
/// keeps a reusable pool of sprite-line entities and hides unused lines instead
/// of respawning every frame.
#[derive(Default, Debug)]
pub struct PhysicsDebugDraw2D {
    options: PhysicsDebugDrawOptions2D,
    segments: Vec<EntityId>,
}

impl PhysicsDebugDraw2D {
    /// Creates a debug draw resource with explicit options.
    pub fn new(options: PhysicsDebugDrawOptions2D) -> Self {
        Self {
            options,
            segments: Vec::new(),
        }
    }

    /// Returns current debug draw options.
    #[inline]
    pub fn options(&self) -> PhysicsDebugDrawOptions2D {
        self.options
    }

    /// Replaces all debug draw options.
    #[inline]
    pub fn set_options(&mut self, options: PhysicsDebugDrawOptions2D) {
        self.options = options;
    }

    /// Returns mutable access to debug draw options.
    #[inline]
    pub fn options_mut(&mut self) -> &mut PhysicsDebugDrawOptions2D {
        &mut self.options
    }

    /// Enables or disables the overlay.
    #[inline]
    pub fn set_enabled(&mut self, enabled: bool) {
        self.options.enabled = enabled;
    }

    /// Number of line entities currently owned by the pool.
    #[inline]
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Despawns all debug line entities owned by this resource.
    pub fn clear(&mut self, world: &mut World) {
        for entity in self.segments.drain(..) {
            let _ = world.despawn(entity);
        }
    }

    /// Rebuilds the visible overlay from current `Transform + RigidBody2D + Collider2D` components.
    pub fn sync(&mut self, world: &mut World) {
        if !self.options.enabled {
            self.hide_all(world);
            return;
        }

        let segments = collect_debug_segments(world, self.options);
        self.sync_segments(world, &segments);
        self.hide_unused(world, segments.len());
    }

    fn sync_segments(&mut self, world: &mut World, segments: &[DebugSegment]) {
        for (index, segment) in segments.iter().copied().enumerate() {
            if index >= self.segments.len() {
                self.segments
                    .push(spawn_segment(world, segment, self.options));
                continue;
            }

            let entity = self.segments[index];
            if world.contains(entity) && update_segment(world, entity, segment, self.options) {
                continue;
            }

            let _ = world.despawn(entity);
            self.segments[index] = spawn_segment(world, segment, self.options);
        }
    }

    fn hide_unused(&mut self, world: &mut World, used: usize) {
        for &entity in self.segments.iter().skip(used) {
            if let Some(sprite) = world.get_mut::<SpriteRenderer>(entity) {
                sprite.visible = false;
            }
        }
    }

    fn hide_all(&mut self, world: &mut World) {
        self.hide_unused(world, 0);
    }
}

struct PhysicsDebugDrawSystem;

impl System for PhysicsDebugDrawSystem {
    fn run(&mut self, world: &mut World) {
        sync_physics_debug_draw(world);
    }

    fn teardown(&mut self, world: &mut World) {
        if let Some(mut debug) = world.remove_resource::<PhysicsDebugDraw2D>() {
            debug.clear(world);
        }
    }
}

struct PhysicsDebugDrawInstalled2D;

/// Plugin that installs an every-frame system mirroring physics colliders as sprite lines.
#[derive(Clone, Copy, Debug, Default)]
pub struct PhysicsDebugPlugin {
    pub options: PhysicsDebugDrawOptions2D,
}

impl PhysicsDebugPlugin {
    pub fn new(options: PhysicsDebugDrawOptions2D) -> Self {
        Self { options }
    }
}

impl Plugin for PhysicsDebugPlugin {
    fn name(&self) -> &'static str {
        "physics_debug"
    }

    fn install(self, world: &mut World) -> PluginResult {
        install_physics_debug_plugin(world, self.options);
        Ok(())
    }
}

/// Installs an every-frame system that mirrors physics colliders as sprite lines.
///
/// Reinstalling updates options without registering duplicate systems.
fn install_physics_debug_plugin(world: &mut World, options: PhysicsDebugDrawOptions2D) {
    if let Some(debug) = world.get_resource_mut::<PhysicsDebugDraw2D>() {
        debug.set_options(options);
    } else {
        world.insert_resource(PhysicsDebugDraw2D::new(options));
    }

    if world
        .get_resource::<PhysicsDebugDrawInstalled2D>()
        .is_some()
    {
        return;
    }
    world.group("physics_debug").add(PhysicsDebugDrawSystem);
    world.insert_resource(PhysicsDebugDrawInstalled2D);
}

/// Synchronizes the debug overlay immediately.
///
/// Useful after toggling [`PhysicsDebugDrawOptions2D::enabled`] from app code
/// when you want the current frame to reflect the change before the next tick.
pub fn sync_physics_debug_draw(world: &mut World) {
    let Some(mut debug) = world.remove_resource::<PhysicsDebugDraw2D>() else {
        return;
    };
    debug.sync(world);
    world.insert_resource(debug);
}

#[derive(Clone, Copy, Debug)]
struct PhysicsDebugLine2D;

#[derive(Clone, Copy, Debug)]
struct DebugSegment {
    center: Vec2,
    length: f32,
    rotation: f32,
    color: Color,
}

fn collect_debug_segments(world: &World, options: PhysicsDebugDrawOptions2D) -> Vec<DebugSegment> {
    let mut query = world.query::<(&Transform, &RigidBody2D, &Collider2D)>();
    let mut segments = Vec::new();
    query.for_each(world, |(transform, body, collider)| {
        let center = Vec2::new(transform.position.x(), transform.position.y())
            + rotate(collider.offset, transform.rotation_z());
        let rotation = transform.rotation_z() + collider.rotation;
        let color = collider_color(*body, *collider, options);

        match collider.shape {
            ColliderShape2D::Rectangle { width, height } => {
                push_rectangle(&mut segments, center, rotation, width, height, color);
            }
            ColliderShape2D::Circle { radius } => {
                push_circle(&mut segments, center, rotation, radius, options, color);
            }
            ColliderShape2D::CapsuleY {
                half_height,
                radius,
            } => {
                push_capsule_y(
                    &mut segments,
                    center,
                    rotation,
                    half_height,
                    radius,
                    options,
                    color,
                );
            }
        }
    });
    segments
}

fn collider_color(
    body: RigidBody2D,
    collider: Collider2D,
    options: PhysicsDebugDrawOptions2D,
) -> Color {
    if !body.enabled || !collider.enabled {
        return options.disabled_color;
    }
    if collider.sensor {
        return options.trigger_color;
    }
    match body.body_type {
        BodyType2D::Static => options.static_color,
        BodyType2D::Kinematic => options.kinematic_color,
        BodyType2D::Dynamic => options.dynamic_color,
    }
}

fn push_rectangle(
    segments: &mut Vec<DebugSegment>,
    center: Vec2,
    rotation: f32,
    width: f32,
    height: f32,
    color: Color,
) {
    let half_w = width.max(0.0) * 0.5;
    let half_h = height.max(0.0) * 0.5;
    let points = [
        Vec2::new(-half_w, -half_h),
        Vec2::new(half_w, -half_h),
        Vec2::new(half_w, half_h),
        Vec2::new(-half_w, half_h),
    ];
    push_polyline(segments, center, rotation, &points, true, color);
}

fn push_circle(
    segments: &mut Vec<DebugSegment>,
    center: Vec2,
    rotation: f32,
    radius: f32,
    options: PhysicsDebugDrawOptions2D,
    color: Color,
) {
    let radius = radius.max(0.0);
    let segment_count = options.circle_segments.max(8);
    let mut points = Vec::with_capacity(segment_count);
    for index in 0..segment_count {
        let angle = index as f32 / segment_count as f32 * TAU;
        points.push(Vec2::new(angle.cos() * radius, angle.sin() * radius));
    }
    push_polyline(segments, center, rotation, &points, true, color);
}

fn push_capsule_y(
    segments: &mut Vec<DebugSegment>,
    center: Vec2,
    rotation: f32,
    half_height: f32,
    radius: f32,
    options: PhysicsDebugDrawOptions2D,
    color: Color,
) {
    let half_height = half_height.max(0.0);
    let radius = radius.max(0.0);
    let arc_segments = (options.circle_segments.max(8) / 2).max(4);
    let mut points = Vec::with_capacity(arc_segments * 2 + 2);

    for index in 0..=arc_segments {
        let angle = index as f32 / arc_segments as f32 * PI;
        points.push(Vec2::new(
            angle.cos() * radius,
            half_height + angle.sin() * radius,
        ));
    }
    for index in 0..=arc_segments {
        let angle = PI + index as f32 / arc_segments as f32 * PI;
        points.push(Vec2::new(
            angle.cos() * radius,
            -half_height + angle.sin() * radius,
        ));
    }
    push_polyline(segments, center, rotation, &points, true, color);
}

fn push_polyline(
    segments: &mut Vec<DebugSegment>,
    center: Vec2,
    rotation: f32,
    points: &[Vec2],
    closed: bool,
    color: Color,
) {
    if points.len() < 2 {
        return;
    }

    for index in 0..points.len() - 1 {
        push_line(
            segments,
            transform_point(center, rotation, points[index]),
            transform_point(center, rotation, points[index + 1]),
            color,
        );
    }
    if closed {
        push_line(
            segments,
            transform_point(center, rotation, points[points.len() - 1]),
            transform_point(center, rotation, points[0]),
            color,
        );
    }
}

fn push_line(segments: &mut Vec<DebugSegment>, start: Vec2, end: Vec2, color: Color) {
    let delta = end - start;
    let length = delta.length();
    if length <= f32::EPSILON {
        return;
    }
    segments.push(DebugSegment {
        center: (start + end) * 0.5,
        length,
        rotation: delta.y().atan2(delta.x()),
        color,
    });
}

fn spawn_segment(
    world: &mut World,
    segment: DebugSegment,
    options: PhysicsDebugDrawOptions2D,
) -> EntityId {
    world.spawn((
        segment_transform(segment, options),
        segment_sprite(segment, options),
        options.sorting_layer,
        PhysicsDebugLine2D,
    ))
}

fn update_segment(
    world: &mut World,
    entity: EntityId,
    segment: DebugSegment,
    options: PhysicsDebugDrawOptions2D,
) -> bool {
    let mut complete = true;

    if let Some(transform) = world.get_mut::<Transform>(entity) {
        *transform = segment_transform(segment, options);
    } else {
        complete = false;
    }

    if let Some(sprite) = world.get_mut::<SpriteRenderer>(entity) {
        *sprite = segment_sprite(segment, options);
    } else {
        complete = false;
    }

    if let Some(layer) = world.get_mut::<SortingLayer>(entity) {
        *layer = options.sorting_layer;
    } else {
        complete = false;
    }

    complete
}

fn segment_transform(segment: DebugSegment, options: PhysicsDebugDrawOptions2D) -> Transform {
    Transform::from_xyz(segment.center.x(), segment.center.y(), options.z)
        .with_rotation(segment.rotation)
}

fn segment_sprite(segment: DebugSegment, options: PhysicsDebugDrawOptions2D) -> SpriteRenderer {
    SpriteRenderer::new(segment.length, options.line_thickness.max(0.1)).color(segment.color)
}

fn transform_point(center: Vec2, rotation: f32, point: Vec2) -> Vec2 {
    center + rotate(point, rotation)
}

fn rotate(point: Vec2, radians: f32) -> Vec2 {
    let (sin, cos) = radians.sin_cos();
    Vec2::new(
        point.x() * cos - point.y() * sin,
        point.x() * sin + point.y() * cos,
    )
}

#[cfg(test)]
mod tests {
    use crate::ecs::World;
    use crate::physics::{Collider2D, RigidBody2D};

    use super::*;

    #[test]
    fn debug_draw_spawns_rectangle_segments() {
        let mut world = World::new();
        let mut debug = PhysicsDebugDraw2D::new(PhysicsDebugDrawOptions2D::default());
        world.spawn((
            Transform::from_xy(4.0, 8.0),
            RigidBody2D::static_body(),
            Collider2D::rectangle(16.0, 24.0),
        ));

        debug.sync(&mut world);

        assert_eq!(debug.segment_count(), 4);
        let mut query = world.query::<&PhysicsDebugLine2D>();
        assert_eq!(query.count(&world), 4);
    }

    #[test]
    fn debug_draw_reuses_and_hides_segment_pool() {
        let mut world = World::new();
        let mut debug = PhysicsDebugDraw2D::new(PhysicsDebugDrawOptions2D {
            circle_segments: 8,
            ..Default::default()
        });
        let entity = world.spawn((
            Transform::from_xy(0.0, 0.0),
            RigidBody2D::static_body(),
            Collider2D::circle(12.0),
        ));

        debug.sync(&mut world);
        assert_eq!(debug.segment_count(), 8);
        world.insert(entity, Collider2D::rectangle(8.0, 8.0));
        debug.sync(&mut world);

        let visible = debug
            .segments
            .iter()
            .filter(|entity| {
                world
                    .get::<SpriteRenderer>(**entity)
                    .map(|sprite| sprite.visible)
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(debug.segment_count(), 8);
        assert_eq!(visible, 4);
    }
}
