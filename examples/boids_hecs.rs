//! # Boids — hecs version
//!
//! Same boids simulation as `boids.rs`, but using hecs for the ECS layer.
//! Compare FPS with the Sky Engine version.
//!
//! ```
//! cargo run --example boids_hecs --features demo --release
//! ```

use minifb::{Key, MouseButton, MouseMode, Window, WindowOptions};
use rand::Rng;
use std::f32::consts::TAU;

const W: usize = 1024;
const H: usize = 768;
const NUM_BOIDS: usize = 2000;
const MAX_SPEED: f32 = 220.0;
const MIN_SPEED: f32 = 40.0;
const VISUAL_RANGE: f32 = 55.0;
const SEPARATION_RANGE: f32 = 18.0;
const PREDATOR_RANGE: f32 = 120.0;
const ATTRACTOR_RANGE: f32 = 200.0;

#[derive(Clone, Copy)]
struct Pos { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Vel { x: f32, y: f32 }

#[derive(Clone, Copy)]
struct Boid;

#[derive(Clone, Copy)]
struct Attractor { life: f32 }

// === Rendering (identical to sky version) ===

fn speed_color(speed: f32) -> u32 {
    let t = ((speed - MIN_SPEED) / (MAX_SPEED - MIN_SPEED)).clamp(0.0, 1.0);
    let hue = 240.0 * (1.0 - t);
    hsv_to_u32(hue, 0.85, 1.0)
}

fn hsv_to_u32(h: f32, s: f32, v: f32) -> u32 {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h as u32) / 60 {
        0 => (c, x, 0.0), 1 => (x, c, 0.0), 2 => (0.0, c, x),
        3 => (0.0, x, c), 4 => (x, 0.0, c), _ => (c, 0.0, x),
    };
    (((r+m)*255.0) as u32) << 16 | (((g+m)*255.0) as u32) << 8 | ((b+m)*255.0) as u32
}

fn plot(buf: &mut [u32], x: i32, y: i32, col: u32) {
    if x >= 0 && x < W as i32 && y >= 0 && y < H as i32 {
        buf[y as usize * W + x as usize] = col;
    }
}

fn draw_line(buf: &mut [u32], x0: f32, y0: f32, x1: f32, y1: f32, col: u32) {
    let dx = x1 - x0; let dy = y1 - y0;
    let steps = dx.abs().max(dy.abs()).max(1.0) as usize;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        plot(buf, (x0+dx*t) as i32, (y0+dy*t) as i32, col);
    }
}

fn draw_boid(buf: &mut [u32], x: f32, y: f32, vx: f32, vy: f32, col: u32) {
    let a = vy.atan2(vx); let sz = 6.0;
    let (tx, ty) = (x + a.cos()*sz, y + a.sin()*sz);
    let (lx, ly) = (x + (a+2.5).cos()*sz*0.6, y + (a+2.5).sin()*sz*0.6);
    let (rx, ry) = (x + (a-2.5).cos()*sz*0.6, y + (a-2.5).sin()*sz*0.6);
    draw_line(buf, tx, ty, lx, ly, col);
    draw_line(buf, tx, ty, rx, ry, col);
    draw_line(buf, lx, ly, rx, ry, col);
}

fn draw_ring(buf: &mut [u32], cx: f32, cy: f32, r: f32, col: u32) {
    let segs = (r * 1.5).max(16.0) as usize;
    for i in 0..segs {
        let a = TAU * i as f32 / segs as f32;
        plot(buf, (cx + a.cos()*r) as i32, (cy + a.sin()*r) as i32, col);
    }
}

fn fade_buffer(buf: &mut [u32], factor: u32) {
    for pixel in buf.iter_mut() {
        let r = ((*pixel >> 16) & 0xFF).saturating_sub(factor);
        let g = ((*pixel >> 8) & 0xFF).saturating_sub(factor);
        let b = (*pixel & 0xFF).saturating_sub(factor);
        *pixel = (r << 16) | (g << 8) | b;
    }
}

fn main() {
    let mut window = Window::new("hecs — Boids", W, H,
        WindowOptions { resize: false, ..Default::default() }).unwrap();
    window.set_target_fps(0);

    let mut world = hecs::World::new();
    let mut buf = vec![0u32; W * H];
    let mut rng = rand::thread_rng();

    for _ in 0..NUM_BOIDS {
        let angle = rng.gen_range(0.0..TAU);
        let speed = rng.gen_range(MIN_SPEED..MAX_SPEED);
        world.spawn((
            Pos { x: rng.gen_range(0.0..W as f32), y: rng.gen_range(0.0..H as f32) },
            Vel { x: angle.cos() * speed, y: angle.sin() * speed },
            Boid,
        ));
    }

    let mut last = std::time::Instant::now();
    let mut fps_timer = std::time::Instant::now();
    let mut fps_count = 0u32;
    let mut display_fps = 0.0f64;

    let mut positions: Vec<(f32, f32)> = Vec::with_capacity(NUM_BOIDS);
    let mut velocities: Vec<(f32, f32)> = Vec::with_capacity(NUM_BOIDS);

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = std::time::Instant::now();
        let dt = (now - last).as_secs_f32().min(0.05);
        last = now;

        let mouse_pos = window.get_mouse_pos(MouseMode::Clamp).unwrap_or((-1000.0, -1000.0));

        if window.get_mouse_down(MouseButton::Left) {
            world.spawn((Pos { x: mouse_pos.0, y: mouse_pos.1 }, Attractor { life: 5.0 }));
        }

        let panic_mode = window.is_key_down(Key::Space);

        // Collect attractors
        let mut attractors: Vec<(f32, f32)> = Vec::new();
        for (_, (p, _)) in world.query_mut::<(&Pos, &Attractor)>() {
            attractors.push((p.x, p.y));
        }

        // Decay attractors
        let mut dead: Vec<hecs::Entity> = Vec::new();
        for (e, attr) in world.query_mut::<&mut Attractor>() {
            attr.life -= dt;
            if attr.life <= 0.0 { dead.push(e); }
        }
        for e in dead { let _ = world.despawn(e); }

        // Collect positions/velocities
        positions.clear();
        velocities.clear();
        for (_, (p, v, _)) in world.query_mut::<(&Pos, &Vel, &Boid)>() {
            positions.push((p.x, p.y));
            velocities.push((v.x, v.y));
        }

        // Boid rules
        {
            let mut idx = 0usize;
            for (_, (pos, vel, _)) in world.query_mut::<(&Pos, &mut Vel, &Boid)>() {
                let mut sep_x = 0.0f32; let mut sep_y = 0.0f32;
                let mut align_x = 0.0f32; let mut align_y = 0.0f32;
                let mut coh_x = 0.0f32; let mut coh_y = 0.0f32;
                let mut neighbours = 0u32;

                for j in 0..positions.len() {
                    if j == idx { continue; }
                    let dx = positions[j].0 - pos.x;
                    let dy = positions[j].1 - pos.y;
                    let dist_sq = dx * dx + dy * dy;
                    if dist_sq < VISUAL_RANGE * VISUAL_RANGE {
                        let dist = dist_sq.sqrt().max(0.01);
                        align_x += velocities[j].0; align_y += velocities[j].1;
                        coh_x += positions[j].0; coh_y += positions[j].1;
                        neighbours += 1;
                        if dist_sq < SEPARATION_RANGE * SEPARATION_RANGE {
                            sep_x -= dx / dist; sep_y -= dy / dist;
                        }
                    }
                }

                if neighbours > 0 {
                    let n = neighbours as f32;
                    vel.x += (align_x / n - vel.x) * 0.05;
                    vel.y += (align_y / n - vel.y) * 0.05;
                    vel.x += (coh_x / n - pos.x) * 0.005;
                    vel.y += (coh_y / n - pos.y) * 0.005;
                    vel.x += sep_x * 2.0; vel.y += sep_y * 2.0;
                }

                let pdx = pos.x - mouse_pos.0; let pdy = pos.y - mouse_pos.1;
                let pdist_sq = pdx * pdx + pdy * pdy;
                if pdist_sq < PREDATOR_RANGE * PREDATOR_RANGE && pdist_sq > 0.01 {
                    let pdist = pdist_sq.sqrt();
                    let strength = (1.0 - pdist / PREDATOR_RANGE) * 600.0;
                    vel.x += pdx / pdist * strength * dt;
                    vel.y += pdy / pdist * strength * dt;
                }

                for &(ax, ay) in &attractors {
                    let adx = ax - pos.x; let ady = ay - pos.y;
                    let adist_sq = adx * adx + ady * ady;
                    if adist_sq < ATTRACTOR_RANGE * ATTRACTOR_RANGE && adist_sq > 1.0 {
                        let adist = adist_sq.sqrt();
                        vel.x += adx / adist * 80.0 * dt;
                        vel.y += ady / adist * 80.0 * dt;
                    }
                }

                if panic_mode {
                    let scatter_angle = (idx as f32 * 2.399) % TAU;
                    vel.x += scatter_angle.cos() * 500.0 * dt;
                    vel.y += scatter_angle.sin() * 500.0 * dt;
                }

                let margin = 60.0; let turn = 150.0;
                if pos.x < margin { vel.x += turn * dt; }
                if pos.x > W as f32 - margin { vel.x -= turn * dt; }
                if pos.y < margin { vel.y += turn * dt; }
                if pos.y > H as f32 - margin { vel.y -= turn * dt; }

                let speed = (vel.x * vel.x + vel.y * vel.y).sqrt();
                let target_max = if panic_mode { MAX_SPEED * 1.5 } else { MAX_SPEED };
                if speed > target_max { vel.x = vel.x/speed*target_max; vel.y = vel.y/speed*target_max; }
                if speed < MIN_SPEED && speed > 0.01 { vel.x = vel.x/speed*MIN_SPEED; vel.y = vel.y/speed*MIN_SPEED; }

                idx += 1;
            }
        }

        // Move
        for (_, (pos, vel, _)) in world.query_mut::<(&mut Pos, &Vel, &Boid)>() {
            pos.x += vel.x * dt; pos.y += vel.y * dt;
            if !(pos.x >= 0.0) { pos.x = 0.0; }
            if !(pos.x <= W as f32 - 1.0) { pos.x = W as f32 - 1.0; }
            if !(pos.y >= 0.0) { pos.y = 0.0; }
            if !(pos.y <= H as f32 - 1.0) { pos.y = H as f32 - 1.0; }
        }

        // Render
        fade_buffer(&mut buf, 18);

        for (_, (pos, attr)) in world.query_mut::<(&Pos, &Attractor)>() {
            let alpha = (attr.life / 5.0).clamp(0.0, 1.0);
            let pulse = (attr.life * 4.0).sin() * 0.3 + 0.7;
            let g = (200.0 * alpha * pulse) as u32;
            draw_ring(&mut buf, pos.x, pos.y, ATTRACTOR_RANGE * 0.3, (g << 8) | 0x44);
        }

        if mouse_pos.0 >= 0.0 && mouse_pos.0 < W as f32 {
            draw_ring(&mut buf, mouse_pos.0, mouse_pos.1, PREDATOR_RANGE * 0.5, 0x442222);
        }

        for (_, (pos, vel, _)) in world.query_mut::<(&Pos, &Vel, &Boid)>() {
            let speed = (vel.x * vel.x + vel.y * vel.y).sqrt();
            draw_boid(&mut buf, pos.x, pos.y, vel.x, vel.y, speed_color(speed));
        }

        fps_count += 1;
        if fps_timer.elapsed().as_secs_f64() >= 0.5 {
            display_fps = fps_count as f64 / fps_timer.elapsed().as_secs_f64();
            fps_count = 0; fps_timer = std::time::Instant::now();
        }

        window.set_title(&format!("hecs Boids | {} boids | {:.0} FPS", NUM_BOIDS, display_fps));
        window.update_with_buffer(&buf, W, H).unwrap();
    }
}
