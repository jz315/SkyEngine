//! # Live2D Demo
//!
//! Focused integration example for loading and rendering a Cubism model
//! through SkyEngine's Live2D pipeline.
//!
//! ```bash
//! cargo run --example live2d_demo --features live2d --release -- <path-to-model3.json>
//! cargo run --example live2d_demo --features live2d --release -- assets/Haru/Haru.model3.json
//! ```

use sky_engine::app::{App, AppConfig, KeyCode};
use sky_engine::ecs::World;
use sky_engine::render::expert::live2d::clipping::ClippingManager;
use sky_engine::render::expert::live2d::{Live2DModelResource, Live2DRenderer};

fn main() {
    let model_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: live2d_demo <path-to-model3.json>");
        eprintln!("  e.g. live2d_demo assets/Haru/Haru.model3.json");
        std::process::exit(1);
    });

    let config = AppConfig {
        title: format!("SkyEngine — Live2D: {}", model_path),
        width: 1280,
        height: 720,
        vsync: false,
        resizable: true,
    };

    let mut resource: Option<Live2DModelResource> = None;
    let mut renderer: Option<Live2DRenderer> = None;
    let mut clipping: Option<ClippingManager> = None;
    let mut expression_names: Vec<String> = Vec::new();
    let mut initialized = false;

    // FPS tracking
    let mut fps_accum = 0.0_f32;
    let mut fps_frames = 0u32;
    let mut fps_display = 0.0_f32;
    let base_title = config.title.clone();

    let world = World::new();

    App::new(config, world)
        .run(move |ctx| {
            ctx.world.tick();
            let dt = ctx.dt;

            // FPS counter
            fps_accum += dt;
            fps_frames += 1;
            if fps_accum >= 0.5 {
                fps_display = fps_frames as f32 / fps_accum;
                fps_accum = 0.0;
                fps_frames = 0;
                ctx.set_title(&format!("{} | {:.0} FPS", base_title, fps_display));
            }

            // Read input before gpu borrow
            let expression_index = pressed_expression_index(ctx.input);

            // Lazy init (need GpuContext for texture uploads)
            if !initialized {
                eprintln!("[Live2D] Loading model: {}", model_path);
                match Live2DModelResource::load(ctx.gpu(), &model_path) {
                    Ok(res) => {
                        eprintln!(
                            "[Live2D] Loaded: {} drawables, {} textures, motion {}, blink {}, expression {}, breath {}, physics {}, pose {}, canvas {:?}",
                            res.model.drawable_count(),
                            res.textures.len(),
                            if res.motion_player.is_some() { "yes" } else { "no" },
                            if res.eye_blink.is_some() { "yes" } else { "no" },
                            if res.expression_player.is_some() { "yes" } else { "no" },
                            if res.breath.is_some() { "yes" } else { "no" },
                            if res.physics.is_some() { "yes" } else { "no" },
                            if res.pose.is_some() { "yes" } else { "no" },
                            res.model.canvas_info(),
                        );

                        let clip_mgr = ClippingManager::new(&res.model);
                        if clip_mgr.has_masks() {
                            eprintln!("[Live2D] {} clipping contexts", clip_mgr.contexts.len());
                            clipping = Some(clip_mgr);
                        }

                        if let Some(player) = res.expression_player.as_ref() {
                            expression_names = player
                                .expression_names()
                                .map(|name| name.to_string())
                                .collect();
                            if !expression_names.is_empty() {
                                let hotkeys = expression_names
                                    .iter()
                                    .take(9)
                                    .enumerate()
                                    .map(|(index, name)| format!("{}={name}", index + 1))
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                eprintln!("[Live2D] Expressions: {hotkeys}");
                            }
                        }

                        resource = Some(res);
                        renderer = Some(Live2DRenderer::new(ctx.gpu()));
                    }
                    Err(e) => {
                        eprintln!("[Live2D] Load failed: {e}");
                        std::process::exit(1);
                    }
                }
                initialized = true;
            }

            if let (Some(ref mut res), Some(ref mut rend)) = (&mut resource, &mut renderer) {
                if let Some(idx) = expression_index {
                    if let Some(name) = expression_names.get(idx) {
                        if res.set_expression(name) {
                            eprintln!("[Live2D] Expression -> {name}");
                        }
                    }
                }

                // Update runtime state (pose -> model)
                res.update(dt);

                let gpu = ctx.gpu();

                // Clear surface
                gpu.with_surface_pass(
                    "live2d_clear",
                    Some(wgpu::Color {
                        r: 0.12,
                        g: 0.12,
                        b: 0.18,
                        a: 1.0,
                    }),
                    |_| {},
                );

                // Draw model
                rend.draw_to_surface(gpu, &res.model, &res.textures, &mut clipping);
            }
        });
}

fn pressed_expression_index(input: &sky_engine::app::Input) -> Option<usize> {
    const HOTKEYS: [KeyCode; 9] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];

    HOTKEYS.iter().position(|&key| input.key_pressed(key))
}
