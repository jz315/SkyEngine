use sky_engine::ecs::{EntityId, With, World};
use sky_engine::render::{Color, SpriteRenderer, Transform};

use crate::components::{Adventurer, Condition, Follow, GridPos, PixelVisual, StatusPip};
use crate::layout::grid_to_world;
use crate::palette;

pub fn animate_adventurers(world: &mut World, dt: f32) {
    let mut query = world
        .query_filtered::<(&Condition, &mut PixelVisual, &mut SpriteRenderer), With<Adventurer>>();
    query.for_each(world, |(condition, visual, sprite)| {
        visual.pulse += dt * (2.5 + condition.stress as f32 * 0.25);
        let blink = (visual.pulse.sin() * 0.5 + 0.5) * 0.12;
        sprite.color = if condition.health <= 5 {
            mix(visual.hurt, visual.base, 0.35 + blink)
        } else if condition.stress >= 7 {
            mix(palette::STRESS, visual.base, 0.35 + blink)
        } else {
            visual.base
        };
    });

    let mut pips = world.query_filtered::<(&Follow, &mut SpriteRenderer), With<StatusPip>>();
    let mut updates = Vec::new();
    pips.for_each_with_entity(world, |entity, (follow, _)| {
        if let Some(condition) = world.get::<Condition>(follow.target) {
            updates.push((entity, *condition));
        }
    });

    for (entity, condition) in updates {
        if let Some(sprite) = world.get_mut::<SpriteRenderer>(entity) {
            sprite.color = if condition.health <= 5 {
                palette::HURT
            } else if condition.stress >= 7 {
                palette::STRESS
            } else {
                palette::HEALTHY
            };
        }
    }
}

pub fn sync_transforms(world: &mut World, dt: f32) {
    let mut query = world.query::<(&GridPos, &mut Transform)>();
    query.for_each(world, |(grid, transform)| {
        let next = grid_to_world(*grid, transform.position[2]);
        let dx = next.position[0] - transform.position[0];
        let dy = next.position[1] - transform.position[1];
        let distance = (dx * dx + dy * dy).sqrt();
        let max_step = 165.0 * dt;
        if distance <= max_step || distance <= 0.01 {
            transform.position[0] = next.position[0];
            transform.position[1] = next.position[1];
        } else {
            let scale = max_step / distance;
            transform.position[0] += dx * scale;
            transform.position[1] += dy * scale;
        }
    });
}

pub fn entities_settled(world: &World, entities: &[EntityId]) -> bool {
    entities.iter().all(|entity| {
        let Some(grid) = world.get::<GridPos>(*entity) else {
            return false;
        };
        let Some(transform) = world.get::<Transform>(*entity) else {
            return false;
        };
        let target = grid_to_world(*grid, transform.position[2]);
        let dx = target.position[0] - transform.position[0];
        let dy = target.position[1] - transform.position[1];
        dx * dx + dy * dy <= 4.0
    })
}

pub fn sync_followers(world: &mut World) {
    let mut followers = Vec::new();
    let mut query = world.query::<&Follow>();
    query.for_each_with_entity(world, |entity, follow| {
        followers.push((entity, *follow));
    });

    for (entity, follow) in followers {
        let Some(target_transform) = world.get::<Transform>(follow.target).copied() else {
            continue;
        };
        if let Some(transform) = world.get_mut::<Transform>(entity) {
            transform.position[0] = target_transform.position[0] + follow.offset_x;
            transform.position[1] = target_transform.position[1] + follow.offset_y;
        }
    }
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}
