//! # Snake
//!
//! Classic Snake game built on Sky Engine ECS.  Each body segment is
//! a separate entity with its own Position component.  Food spawning
//! and collision are handled through ECS queries.
//!
//! Controls: WASD or Arrow keys
//!
//! ```
//! cargo run --example snake --features demo-legacy
//! ```

use minifb::{Key, Window, WindowOptions};
use rand::Rng;
use sky_engine::ecs::{EntityId, World};
use std::collections::VecDeque;

const W: usize = 640;
const H: usize = 480;
const GRID: usize = 16;
const COLS: usize = W / GRID;
const ROWS: usize = H / GRID;
const TICK_RATE: f32 = 0.10; // seconds per step

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct GridPos {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy)]
struct Food;

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct SnakeSegment {
    order: u32,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fill_cell(buf: &mut [u32], gx: i32, gy: i32, col: u32) {
    if gx < 0 || gx >= COLS as i32 || gy < 0 || gy >= ROWS as i32 {
        return;
    }
    let px = gx as usize * GRID;
    let py = gy as usize * GRID;
    for dy in 1..GRID - 1 {
        for dx in 1..GRID - 1 {
            let idx = (py + dy) * W + (px + dx);
            if idx < buf.len() {
                buf[idx] = col;
            }
        }
    }
}

fn fill_cell_round(buf: &mut [u32], gx: i32, gy: i32, col: u32) {
    if gx < 0 || gx >= COLS as i32 || gy < 0 || gy >= ROWS as i32 {
        return;
    }
    let cx = gx as f32 * GRID as f32 + GRID as f32 / 2.0;
    let cy = gy as f32 * GRID as f32 + GRID as f32 / 2.0;
    let r = GRID as f32 / 2.0 - 1.5;
    let px_start = gx as usize * GRID;
    let py_start = gy as usize * GRID;
    for dy in 0..GRID {
        for dx in 0..GRID {
            let fx = px_start as f32 + dx as f32 + 0.5;
            let fy = py_start as f32 + dy as f32 + 0.5;
            if (fx - cx) * (fx - cx) + (fy - cy) * (fy - cy) <= r * r {
                let idx = (py_start + dy) * W + (px_start + dx);
                if idx < buf.len() {
                    buf[idx] = col;
                }
            }
        }
    }
}

fn main() {
    let mut window = Window::new(
        "SkyEngine — Snake",
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

    // Spawn initial snake (3 segments)
    let mut snake: VecDeque<EntityId> = VecDeque::new();
    let start_x = COLS as i32 / 2;
    let start_y = ROWS as i32 / 2;
    for i in 0..3 {
        let e = world.spawn((
            GridPos {
                x: start_x - i,
                y: start_y,
            },
            SnakeSegment { order: i as u32 },
        ));
        snake.push_back(e);
    }

    // Spawn initial food
    spawn_food(&mut world, &mut rng);

    let mut dir: (i32, i32) = (1, 0);
    let mut next_dir = dir;
    let mut timer: f32 = 0.0;
    let mut score: u32 = 0;
    let mut game_over = false;
    let mut last = std::time::Instant::now();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let now = std::time::Instant::now();
        let dt = (now - last).as_secs_f32().min(0.1);
        last = now;

        // --- Input (queue direction change) ---
        if !game_over {
            if (window.is_key_down(Key::W) || window.is_key_down(Key::Up)) && dir.1 != 1 {
                next_dir = (0, -1);
            }
            if (window.is_key_down(Key::S) || window.is_key_down(Key::Down)) && dir.1 != -1 {
                next_dir = (0, 1);
            }
            if (window.is_key_down(Key::A) || window.is_key_down(Key::Left)) && dir.0 != 1 {
                next_dir = (-1, 0);
            }
            if (window.is_key_down(Key::D) || window.is_key_down(Key::Right)) && dir.0 != -1 {
                next_dir = (1, 0);
            }
        }

        // --- Tick ---
        timer += dt;
        if timer >= TICK_RATE && !game_over {
            timer -= TICK_RATE;
            dir = next_dir;

            let head_entity = snake[0];
            let head_pos = *world.get::<GridPos>(head_entity).unwrap();
            let new_x = head_pos.x + dir.0;
            let new_y = head_pos.y + dir.1;

            // Wall collision
            if new_x < 0 || new_x >= COLS as i32 || new_y < 0 || new_y >= ROWS as i32 {
                game_over = true;
            }

            // Self collision
            if !game_over {
                for i in 0..snake.len() {
                    let seg_pos = world.get::<GridPos>(snake[i]).unwrap();
                    if seg_pos.x == new_x && seg_pos.y == new_y {
                        game_over = true;
                        break;
                    }
                }
            }

            if !game_over {
                // Check food
                let mut ate = false;
                let mut food_to_remove: Option<EntityId> = None;
                {
                    let mut q = world.query::<(&GridPos, &Food)>();
                    q.for_each_with_entity(&world, |food_entity, (fpos, _)| {
                        if fpos.x == new_x && fpos.y == new_y {
                            food_to_remove = Some(food_entity);
                        }
                    });
                }
                if let Some(e) = food_to_remove {
                    world.despawn(e);
                    ate = true;
                }

                // Spawn new head
                let new_head =
                    world.spawn((GridPos { x: new_x, y: new_y }, SnakeSegment { order: 0 }));
                snake.push_front(new_head);

                if ate {
                    score += 1;
                    spawn_food(&mut world, &mut rng);
                } else {
                    // Remove tail
                    let tail = snake.pop_back().unwrap();
                    world.despawn(tail);
                }
            }
        }

        // --- Render ---
        // Background: dark checkerboard
        for gy in 0..ROWS {
            for gx in 0..COLS {
                let col = if (gx + gy) % 2 == 0 {
                    0x1A1A2E
                } else {
                    0x16162A
                };
                fill_cell(&mut buf, gx as i32, gy as i32, col);
            }
        }

        // Draw food
        {
            let mut q = world.query::<(&GridPos, &Food)>();
            q.for_each(&world, |(pos, _)| {
                fill_cell_round(&mut buf, pos.x, pos.y, 0xFF3344);
            });
        }

        // Draw snake
        for (i, &entity) in snake.iter().enumerate() {
            if let Some(pos) = world.get::<GridPos>(entity) {
                let t = i as f32 / snake.len().max(1) as f32;
                let g = (255.0 * (1.0 - t * 0.6)) as u32;
                let col = if i == 0 {
                    0x44FF88
                } else {
                    (0x20 << 16) | (g << 8) | 0x40
                };
                fill_cell(&mut buf, pos.x, pos.y, col);
            }
        }

        // Game over overlay
        if game_over {
            for y in H / 2 - 20..H / 2 + 20 {
                for x in W / 2 - 80..W / 2 + 80 {
                    if x < W && y < H {
                        buf[y * W + x] = 0xCC2222;
                    }
                }
            }
        }

        let title = if game_over {
            format!("SkyEngine Snake | GAME OVER | Score: {}", score)
        } else {
            format!(
                "SkyEngine Snake | Score: {} | Length: {}",
                score,
                snake.len()
            )
        };
        window.set_title(&title);
        window.update_with_buffer(&buf, W, H).unwrap();
    }
}

fn spawn_food(world: &mut World, rng: &mut impl Rng) {
    let x = rng.gen_range(0..COLS as i32);
    let y = rng.gen_range(0..ROWS as i32);
    world.spawn((GridPos { x, y }, Food));
}
