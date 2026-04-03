//! # Asteroids
//!
//! A minimal Asteroids-like game built on Sky Engine ECS + minifb.
//! Demonstrates entity spawning/despawning, component queries, and
//! simple collision detection — all driven by the ECS.
//!
//! Controls: ← → rotate, ↑ thrust, Space shoot
//!
//! ```
//! cargo run --example asteroids --features demo
//! ```

use minifb::{Key, Window, WindowOptions};
use rand::Rng;
use sky_engine::ecs::{EntityId, World};
use std::f32::consts::TAU;

const W: usize = 800;
const H: usize = 600;

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Pos {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Vel {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Rot {
    angle: f32,
} // radians

#[derive(Clone, Copy)]
struct Ship;

#[derive(Clone, Copy)]
struct Bullet {
    ttl: f32,
}

#[derive(Clone, Copy)]
struct Asteroid {
    radius: f32,
}

// ---------------------------------------------------------------------------
// Drawing helpers (software rasteriser)
// ---------------------------------------------------------------------------

fn plot(buf: &mut [u32], x: i32, y: i32, col: u32) {
    if x >= 0 && x < W as i32 && y >= 0 && y < H as i32 {
        buf[y as usize * W + x as usize] = col;
    }
}

fn draw_line(buf: &mut [u32], x0: f32, y0: f32, x1: f32, y1: f32, col: u32) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let steps = dx.abs().max(dy.abs()).max(1.0) as usize;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        plot(buf, (x0 + dx * t) as i32, (y0 + dy * t) as i32, col);
    }
}

fn draw_circle(buf: &mut [u32], cx: f32, cy: f32, r: f32, col: u32) {
    let segs = (r * 2.0).max(12.0) as usize;
    for i in 0..segs {
        let a0 = TAU * i as f32 / segs as f32;
        let a1 = TAU * (i + 1) as f32 / segs as f32;
        draw_line(
            buf,
            cx + a0.cos() * r,
            cy + a0.sin() * r,
            cx + a1.cos() * r,
            cy + a1.sin() * r,
            col,
        );
    }
}

fn draw_ship(buf: &mut [u32], cx: f32, cy: f32, angle: f32, col: u32) {
    let sz = 12.0;
    let pts = [
        (angle, sz),
        (angle + 2.4, sz * 0.7),
        (angle - 2.4, sz * 0.7),
    ];
    let verts: Vec<(f32, f32)> = pts
        .iter()
        .map(|(a, r)| (cx + a.cos() * r, cy + a.sin() * r))
        .collect();
    for i in 0..3 {
        let j = (i + 1) % 3;
        draw_line(buf, verts[i].0, verts[i].1, verts[j].0, verts[j].1, col);
    }
}

fn wrap(v: &mut f32, lo: f32, hi: f32) {
    if *v < lo {
        *v += hi - lo;
    }
    if *v > hi {
        *v -= hi - lo;
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let mut window = Window::new(
        "SkyEngine — Asteroids",
        W,
        H,
        WindowOptions {
            resize: false,
            ..Default::default()
        },
    )
    .unwrap();
    window.set_target_fps(60);

    let mut world = World::new();
    let mut buf = vec![0u32; W * H];
    let mut rng = rand::thread_rng();

    // Spawn ship
    let ship = world.spawn((
        Pos {
            x: W as f32 / 2.0,
            y: H as f32 / 2.0,
        },
        Vel { x: 0.0, y: 0.0 },
        Rot {
            angle: -std::f32::consts::FRAC_PI_2,
        },
        Ship,
    ));

    // Spawn initial asteroids
    for _ in 0..8 {
        spawn_asteroid(&mut world, &mut rng, None);
    }

    let mut shoot_cooldown: f32 = 0.0;
    let mut score: u32 = 0;
    let mut last = std::time::Instant::now();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = std::time::Instant::now();
        let dt = (now - last).as_secs_f32().min(0.05);
        last = now;
        shoot_cooldown = (shoot_cooldown - dt).max(0.0);

        // --- Input ---
        if world.contains(ship) {
            if let Some(rot) = world.get_mut::<Rot>(ship) {
                if window.is_key_down(Key::Left) {
                    rot.angle -= 4.0 * dt;
                }
                if window.is_key_down(Key::Right) {
                    rot.angle += 4.0 * dt;
                }
            }
            if window.is_key_down(Key::Up) {
                let angle = world.get::<Rot>(ship).unwrap().angle;
                let vel = world.get_mut::<Vel>(ship).unwrap();
                vel.x += angle.cos() * 200.0 * dt;
                vel.y += angle.sin() * 200.0 * dt;
            }
            if window.is_key_down(Key::Space) && shoot_cooldown <= 0.0 {
                let pos = *world.get::<Pos>(ship).unwrap();
                let rot = *world.get::<Rot>(ship).unwrap();
                let vel = *world.get::<Vel>(ship).unwrap();
                let speed = 350.0;
                world.spawn((
                    Pos {
                        x: pos.x + rot.angle.cos() * 14.0,
                        y: pos.y + rot.angle.sin() * 14.0,
                    },
                    Vel {
                        x: vel.x + rot.angle.cos() * speed,
                        y: vel.y + rot.angle.sin() * speed,
                    },
                    Bullet { ttl: 1.5 },
                ));
                shoot_cooldown = 0.15;
            }
        }

        // --- Physics: move everything with Pos+Vel ---
        {
            let mut q = world.query::<(&mut Pos, &mut Vel)>();
            q.for_each(&world, |(pos, vel)| {
                pos.x += vel.x * dt;
                pos.y += vel.y * dt;
                wrap(&mut pos.x, 0.0, W as f32);
                wrap(&mut pos.y, 0.0, H as f32);
            });
        }

        // Ship drag
        if let Some(vel) = world.get_mut::<Vel>(ship) {
            vel.x *= 0.995;
            vel.y *= 0.995;
        }

        // --- Bullet lifetime ---
        let mut dead_bullets: Vec<EntityId> = Vec::new();
        {
            let mut q = world.query::<&mut Bullet>();
            q.for_each_with_entity(&world, |entity, bullet| {
                bullet.ttl -= dt;
                if bullet.ttl <= 0.0 {
                    dead_bullets.push(entity);
                }
            });
        }
        for e in &dead_bullets {
            world.despawn(*e);
        }

        // --- Collision: bullets vs asteroids ---
        let mut hits: Vec<(EntityId, EntityId, Pos, f32)> = Vec::new(); // (bullet, asteroid, pos, radius)
        {
            let mut bq = world.query::<(&Pos, &Bullet)>();
            let mut aq = world.query::<(&Pos, &Asteroid)>();
            // Collect bullet positions
            let mut bullets: Vec<(EntityId, f32, f32)> = Vec::new();
            bq.for_each_with_entity(&world, |e, (p, _)| {
                bullets.push((e, p.x, p.y));
            });
            // Check against asteroids
            aq.for_each_with_entity(&world, |ae, (ap, ast)| {
                for &(be, bx, by) in &bullets {
                    let dx = ap.x - bx;
                    let dy = ap.y - by;
                    if dx * dx + dy * dy < ast.radius * ast.radius {
                        hits.push((be, ae, *ap, ast.radius));
                    }
                }
            });
        }
        for (be, ae, apos, arad) in &hits {
            if world.contains(*be) {
                world.despawn(*be);
            }
            if world.contains(*ae) {
                world.despawn(*ae);
                score += 1;
                // Split into smaller asteroids
                if *arad > 15.0 {
                    for _ in 0..2 {
                        spawn_asteroid_at(&mut world, &mut rng, apos.x, apos.y, arad * 0.55);
                    }
                }
            }
        }

        // Respawn asteroids if too few
        let mut asteroid_count = {
            let mut q = world.query::<&Asteroid>();
            q.count(&world)
        };
        while asteroid_count < 5 {
            spawn_asteroid(&mut world, &mut rng, None);
            asteroid_count += 1;
        }

        // --- Render ---
        buf.fill(0x0A0A1A); // dark blue

        // Draw asteroids
        {
            let mut q = world.query::<(&Pos, &Asteroid)>();
            q.for_each(&world, |(pos, ast)| {
                draw_circle(&mut buf, pos.x, pos.y, ast.radius, 0x888888);
            });
        }

        // Draw bullets
        {
            let mut q = world.query::<(&Pos, &Bullet)>();
            q.for_each(&world, |(pos, _)| {
                for dy in -1..=1i32 {
                    for dx in -1..=1i32 {
                        plot(&mut buf, pos.x as i32 + dx, pos.y as i32 + dy, 0xFFFF44);
                    }
                }
            });
        }

        // Draw ship
        if world.contains(ship) {
            let pos = *world.get::<Pos>(ship).unwrap();
            let rot = *world.get::<Rot>(ship).unwrap();
            draw_ship(&mut buf, pos.x, pos.y, rot.angle, 0x44FF88);
        }

        window.set_title(&format!(
            "SkyEngine Asteroids | Score: {} | Entities: {}",
            score,
            world.entity_count()
        ));
        window.update_with_buffer(&buf, W, H).unwrap();
    }
}

fn spawn_asteroid(world: &mut World, rng: &mut impl Rng, radius: Option<f32>) {
    let r = radius.unwrap_or(rng.gen_range(20.0..50.0));
    let x = rng.gen_range(0.0..W as f32);
    let y = rng.gen_range(0.0..H as f32);
    spawn_asteroid_at(world, rng, x, y, r);
}

fn spawn_asteroid_at(world: &mut World, rng: &mut impl Rng, x: f32, y: f32, radius: f32) {
    let angle = rng.gen_range(0.0..TAU);
    let speed = rng.gen_range(20.0..80.0);
    world.spawn((
        Pos { x, y },
        Vel {
            x: angle.cos() * speed,
            y: angle.sin() * speed,
        },
        Asteroid { radius },
    ));
}
