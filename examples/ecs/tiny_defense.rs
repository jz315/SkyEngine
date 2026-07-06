//! # Tiny Defense
//!
//! A small ASCII tower-defense style example focused on ECS usage rather
//! than rendering. It demonstrates:
//!
//! - `World` for entities and resources
//! - typed `PreparedQuery`
//! - `With<T>` filters
//! - chunk iteration for movement
//! - `Commands` for deferred spawn / despawn during queries
//!
//! ```
//! cargo run --example tiny_defense
//! ```

use sky_engine::ecs::{Commands, EntityId, With, World};

const WIDTH: i32 = 24;
const HEIGHT: i32 = 7;
const MAX_TICKS: u32 = 36;
const FIRE_COOLDOWN: u32 = 1;
const ENEMY_HP: i32 = 1;
const ENEMY_LANES: [i32; 8] = [3, 3, 2, 2, 4, 4, 3, 3];

#[derive(Clone, Copy, Debug)]
struct Position {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, Debug)]
struct Velocity {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy, Debug)]
struct Health {
    hp: i32,
}

#[derive(Clone, Copy, Debug)]
struct Turret {
    cooldown: u32,
}

#[derive(Clone, Copy, Debug)]
struct Enemy;

#[derive(Clone, Copy, Debug)]
struct Bullet {
    damage: i32,
}

#[derive(Debug)]
struct GameState {
    tick: u32,
    score: u32,
    base_hp: i32,
    wave_index: usize,
}

fn main() {
    let mut world = World::new();
    world.insert_resource(GameState {
        tick: 0,
        score: 0,
        base_hp: 4,
        wave_index: 0,
    });

    world.spawn((
        Position {
            x: 2,
            y: HEIGHT / 2,
        },
        Turret { cooldown: 0 },
    ));

    println!("=== Tiny Defense ===");
    println!("Legend: | = base, T = turret, E = enemy, * = bullet\n");
    render(&world);

    while !is_finished(&world) {
        step(&mut world);
        render(&world);
    }

    let state = world.get_resource::<GameState>().unwrap();
    let result = if state.base_hp > 0 {
        "Victory"
    } else {
        "Defeat"
    };
    println!(
        "{} after {} ticks. Final score: {}",
        result, state.tick, state.score
    );
}

fn step(world: &mut World) {
    spawn_enemy(world);
    turret_ai(world);
    move_entities(world);
    resolve_hits(world);
    cleanup(world);
    world.get_resource_mut::<GameState>().unwrap().tick += 1;
}

fn spawn_enemy(world: &mut World) {
    let lane = {
        let state = world.get_resource_mut::<GameState>().unwrap();
        if state.wave_index >= ENEMY_LANES.len() || !state.tick.is_multiple_of(3) {
            return;
        }
        let lane = ENEMY_LANES[state.wave_index];
        state.wave_index += 1;
        lane
    };

    world.spawn((
        Position {
            x: WIDTH - 2,
            y: lane,
        },
        Velocity { x: -1, y: 0 },
        Health { hp: ENEMY_HP },
        Enemy,
    ));
}

fn turret_ai(world: &mut World) {
    let Some(target_lane) = find_target_lane(world) else {
        let mut turrets = world.query::<&mut Turret>();
        turrets.for_each(&mut *world, |turret| {
            turret.cooldown = turret.cooldown.saturating_sub(1);
        });
        return;
    };

    let mut commands = Commands::new();
    let mut turrets = world.query::<(&mut Position, &mut Turret)>();
    turrets.for_each(&mut *world, |(pos, turret)| {
        turret.cooldown = turret.cooldown.saturating_sub(1);

        if pos.y < target_lane {
            pos.y += 1;
            return;
        }
        if pos.y > target_lane {
            pos.y -= 1;
            return;
        }
        if turret.cooldown != 0 {
            return;
        }

        commands.spawn((
            Position {
                x: pos.x + 1,
                y: pos.y,
            },
            Velocity { x: 1, y: 0 },
            Bullet { damage: 1 },
        ));
        turret.cooldown = FIRE_COOLDOWN;
    });
    commands.apply(world);
}

fn find_target_lane(world: &World) -> Option<i32> {
    let mut best: Option<(i32, i32)> = None;
    let mut enemies = world.query_filtered::<&Position, With<Enemy>>();
    enemies.for_each(world, |pos| match best {
        Some((best_x, _)) if pos.x >= best_x => {}
        _ => best = Some((pos.x, pos.y)),
    });
    best.map(|(_, lane)| lane)
}

fn move_entities(world: &mut World) {
    let mut moving = world.query::<(&mut Position, &Velocity)>();
    moving.for_each_chunk(&mut *world, |(positions, velocities)| {
        for i in 0..positions.len() {
            positions[i].x += velocities[i].x;
            positions[i].y += velocities[i].y;
        }
    });
}

fn resolve_hits(world: &mut World) {
    let mut bullets = Vec::new();
    let mut bullet_query = world.query::<(&Position, &Bullet)>();
    bullet_query.for_each_with_entity(&*world, |entity, (pos, bullet)| {
        bullets.push((entity, *pos, bullet.damage));
    });

    let mut enemies = Vec::new();
    let mut enemy_query = world.query_filtered::<(&Position, &Health), With<Enemy>>();
    enemy_query.for_each_with_entity(&*world, |entity, (pos, health)| {
        enemies.push((entity, *pos, health.hp));
    });

    let mut bullets_to_remove = Vec::new();
    let mut enemy_damage = Vec::new();

    for (bullet_entity, bullet_pos, damage) in bullets {
        if let Some((enemy_entity, _, _)) = enemies
            .iter()
            .find(|(_, enemy_pos, _)| enemy_pos.x == bullet_pos.x && enemy_pos.y == bullet_pos.y)
            .copied()
        {
            bullets_to_remove.push(bullet_entity);
            add_damage(&mut enemy_damage, enemy_entity, damage);
        }
    }

    let mut defeated = 0;
    let mut commands = Commands::new();

    for bullet in bullets_to_remove {
        commands.despawn(bullet);
    }

    for (enemy, damage) in enemy_damage {
        let dead = {
            let Some(health) = world.get_mut::<Health>(enemy) else {
                continue;
            };
            health.hp -= damage;
            health.hp <= 0
        };

        if dead {
            defeated += 1;
            commands.despawn(enemy);
        }
    }

    commands.apply(world);

    if defeated > 0 {
        world.get_resource_mut::<GameState>().unwrap().score += defeated * 10;
    }
}

fn add_damage(damages: &mut Vec<(EntityId, i32)>, entity: EntityId, amount: i32) {
    for (target, damage) in damages.iter_mut() {
        if *target == entity {
            *damage += amount;
            return;
        }
    }
    damages.push((entity, amount));
}

fn cleanup(world: &mut World) {
    let mut leaked = 0;
    let mut commands = Commands::new();

    let mut bullets = world.query::<(&Position, &Bullet)>();
    bullets.for_each_with_entity(&*world, |entity, (pos, _)| {
        if pos.x >= WIDTH - 1 {
            commands.despawn(entity);
        }
    });

    let mut enemies = world.query_filtered::<&Position, With<Enemy>>();
    enemies.for_each_with_entity(&*world, |entity, pos| {
        if pos.x <= 0 {
            leaked += 1;
            commands.despawn(entity);
        }
    });

    commands.apply(world);

    if leaked > 0 {
        world.get_resource_mut::<GameState>().unwrap().base_hp -= leaked;
    }
}

fn is_finished(world: &World) -> bool {
    let state = world.get_resource::<GameState>().unwrap();
    if state.base_hp <= 0 || state.tick >= MAX_TICKS {
        return true;
    }

    if state.wave_index < ENEMY_LANES.len() {
        return false;
    }

    let mut enemies = world.query_filtered::<&Position, With<Enemy>>();
    enemies.is_empty(world)
}

fn render(world: &World) {
    let mut grid = vec![vec!['.'; WIDTH as usize]; HEIGHT as usize];
    for row in &mut grid {
        row[0] = '|';
    }

    let mut turrets = world.query_filtered::<&Position, With<Turret>>();
    turrets.for_each(world, |pos| {
        if pos.x >= 0 && pos.x < WIDTH && pos.y >= 0 && pos.y < HEIGHT {
            grid[pos.y as usize][pos.x as usize] = 'T';
        }
    });

    let mut enemies = world.query_filtered::<&Position, With<Enemy>>();
    enemies.for_each(world, |pos| {
        if pos.x >= 0 && pos.x < WIDTH && pos.y >= 0 && pos.y < HEIGHT {
            grid[pos.y as usize][pos.x as usize] = 'E';
        }
    });

    let mut bullets = world.query::<(&Position, &Bullet)>();
    bullets.for_each(world, |(pos, _)| {
        if pos.x >= 0 && pos.x < WIDTH && pos.y >= 0 && pos.y < HEIGHT {
            grid[pos.y as usize][pos.x as usize] = '*';
        }
    });

    let state = world.get_resource::<GameState>().unwrap();
    println!(
        "tick={:02}  base_hp={}  score={}  entities={}",
        state.tick,
        state.base_hp,
        state.score,
        world.entity_count()
    );
    for row in grid {
        let line: String = row.into_iter().collect();
        println!("{line}");
    }
    println!();
}
