use minifb::{Key, Window, WindowOptions};
use rand::Rng;
use sky_engine::ecs::{EntityId, World};
use std::time::Instant;

const WIDTH: usize = 960;
const HEIGHT: usize = 640;
const MAX_PARTICLES: usize = 80_0000;
const SPAWN_RATE: usize = 400;
const GRAVITY: f32 = 120.0;

#[derive(Clone, Copy)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Velocity {
    vx: f32,
    vy: f32,
}

/// `pool_state`: 0 = in free list, 1 = alive, 2 = just died (needs recycle)
#[derive(Clone, Copy)]
struct Particle {
    lifetime: f32,
    max_lifetime: f32,
    pool_state: u8,
    r: f32,
    g: f32,
    b: f32,
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h as u32) / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

fn color_to_u32(r: f32, g: f32, b: f32, alpha: f32) -> u32 {
    let r = (r * alpha * 255.0).min(255.0) as u32;
    let g = (g * alpha * 255.0).min(255.0) as u32;
    let b = (b * alpha * 255.0).min(255.0) as u32;
    (r << 16) | (g << 8) | b
}

fn main() {
    let mut window = Window::new(
        "SkyEngine — Particle Simulation",
        WIDTH,
        HEIGHT,
        WindowOptions {
            resize: false,
            ..WindowOptions::default()
        },
    )
    .expect("failed to create window");

    window.set_target_fps(0);

    let mut world = World::new();
    let mut buffer = vec![0u32; WIDTH * HEIGHT];
    let mut rng = rand::thread_rng();
    let mut hue_offset: f32 = 0.0;

    // Pre-allocate entire pool
    let mut free_list: Vec<EntityId> = Vec::with_capacity(MAX_PARTICLES);
    for _ in 0..MAX_PARTICLES {
        let e = world.spawn((
            Position { x: 0.0, y: 0.0 },
            Velocity { vx: 0.0, vy: 0.0 },
            Particle {
                lifetime: 0.0,
                max_lifetime: 1.0,
                pool_state: 0,
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
        ));
        free_list.push(e);
    }
    let mut alive_count: usize = 0;
    let mut recycle_buf: Vec<EntityId> = Vec::new();

    // FPS
    let mut last_frame = Instant::now();
    let mut fps_accum = 0.0f64;
    let mut fps_count = 0u32;
    let mut display_fps = 0.0f64;
    let mut display_ecs_us = 0.0f64;
    let mut display_render_us = 0.0f64;
    let mut fps_timer = Instant::now();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = Instant::now();
        let dt = (now - last_frame).as_secs_f32().min(0.05);
        last_frame = now;
        hue_offset = (hue_offset + 15.0 * dt) % 360.0;

        // === SPAWN from pool ===
        let to_spawn = SPAWN_RATE.min(free_list.len());
        for _ in 0..to_spawn {
            let entity = free_list.pop().unwrap();

            let angle = rng.gen_range(0.0..std::f32::consts::TAU);
            let speed = rng.gen_range(40.0..280.0);
            let lifetime = rng.gen_range(1.5..4.0f32);
            let hue = (hue_offset + rng.gen_range(-30.0..30.0f32)).rem_euclid(360.0);
            let (r, g, b) = hsv_to_rgb(hue, 0.9, 1.0);

            {
                let p = world.get_mut::<Position>(entity).unwrap();
                p.x = WIDTH as f32 / 2.0;
                p.y = HEIGHT as f32 * 0.35;
            }
            {
                let v = world.get_mut::<Velocity>(entity).unwrap();
                v.vx = angle.cos() * speed;
                v.vy = angle.sin() * speed - 80.0;
            }
            {
                let pt = world.get_mut::<Particle>(entity).unwrap();
                pt.lifetime = lifetime;
                pt.max_lifetime = lifetime;
                pt.pool_state = 1;
                pt.r = r;
                pt.g = g;
                pt.b = b;
            }

            alive_count += 1;
        }

        // === ECS: PHYSICS ===
        let t_ecs = Instant::now();
        recycle_buf.clear();
        {
            let mut query = world.query::<(&mut Position, &mut Velocity, &mut Particle)>();
            query.for_each_chunk_with_entities(
                &world,
                |entities, (positions, velocities, particles)| {
                    for i in 0..positions.len() {
                        let particle = &mut particles[i];
                        if particle.pool_state != 1 {
                            continue;
                        }

                        let vel = &mut velocities[i];
                        vel.vy += GRAVITY * dt;
                        vel.vx *= 0.999;
                        vel.vy *= 0.999;

                        let pos = &mut positions[i];
                        pos.x += vel.vx * dt;
                        pos.y += vel.vy * dt;

                        if pos.x < 0.0 {
                            pos.x = 0.0;
                            vel.vx = vel.vx.abs() * 0.6;
                        }
                        if pos.x >= WIDTH as f32 {
                            pos.x = WIDTH as f32 - 1.0;
                            vel.vx = -vel.vx.abs() * 0.6;
                        }
                        if pos.y >= HEIGHT as f32 {
                            pos.y = HEIGHT as f32 - 1.0;
                            vel.vy = -vel.vy.abs() * 0.5;
                            vel.vx *= 0.9;
                        }

                        particle.lifetime -= dt;
                        if particle.lifetime <= 0.0 {
                            particle.pool_state = 2;
                            recycle_buf.push(entities[i]);
                        }
                    }
                },
            );
        }
        // Return dead to pool
        for &entity in &recycle_buf {
            world.get_mut::<Particle>(entity).unwrap().pool_state = 0;
            free_list.push(entity);
            alive_count -= 1;
        }
        let ecs_us = t_ecs.elapsed().as_micros() as f64;

        // === RENDER ===
        let t_render = Instant::now();

        for pixel in buffer.iter_mut() {
            let r = ((*pixel >> 16) & 0xFF) as u32;
            let g = ((*pixel >> 8) & 0xFF) as u32;
            let b = (*pixel & 0xFF) as u32;
            *pixel = ((r * 88 / 100) << 16) | ((g * 86 / 100) << 8) | (b * 84 / 100);
        }

        {
            let mut query = world.query::<(&Position, &Particle)>();
            query.for_each(&world, |(pos, particle)| {
                if particle.pool_state != 1 {
                    return;
                }

                let alpha = (particle.lifetime / particle.max_lifetime).clamp(0.0, 1.0);
                let px = pos.x as usize;
                let py = pos.y as usize;

                if px < WIDTH && py < HEIGHT {
                    let color = color_to_u32(particle.r, particle.g, particle.b, alpha);
                    buffer[py * WIDTH + px] = color;
                    if px + 1 < WIDTH {
                        buffer[py * WIDTH + px + 1] = color;
                    }
                    if py + 1 < HEIGHT {
                        buffer[(py + 1) * WIDTH + px] = color;
                    }
                    if px + 1 < WIDTH && py + 1 < HEIGHT {
                        buffer[(py + 1) * WIDTH + px + 1] = color;
                    }
                }
            });
        }
        let render_us = t_render.elapsed().as_micros() as f64;

        // === PRESENT ===
        window
            .update_with_buffer(&buffer, WIDTH, HEIGHT)
            .expect("failed to update window");

        fps_accum += 1.0 / dt as f64;
        fps_count += 1;
        if fps_timer.elapsed().as_secs_f64() >= 0.5 {
            display_fps = fps_accum / fps_count as f64;
            display_ecs_us = ecs_us;
            display_render_us = render_us;
            fps_accum = 0.0;
            fps_count = 0;
            fps_timer = Instant::now();
        }

        window.set_title(&format!(
            "SkyEngine | {} alive / {} pool | {:.0} FPS | ECS {:.0}µs | Render {:.0}µs",
            alive_count,
            free_list.len(),
            display_fps,
            display_ecs_us,
            display_render_us
        ));
    }
}
