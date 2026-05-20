//! GPU-streamed video texture demo.
//!
//! This demo intentionally updates one stable GPU texture every frame. That is
//! the same rendering path an MP4 decoder should feed after producing RGBA or
//! shader-converted YUV frames.
//!
//! ```bash
//! cargo run --example video_demo --features video --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::expert::SpriteBatch;
use sky_engine::render::{Camera, Color, Sprite};
use sky_engine::video::GpuVideoFrameBuffer;

const VIDEO_W: u32 = 512;
const VIDEO_H: u32 = 288;

struct VideoDemo {
    elapsed: f32,
    camera: Camera,
    batch: Option<SpriteBatch>,
    frame: Option<GpuVideoFrameBuffer>,
    pixels: Vec<u8>,
}

impl VideoDemo {
    fn new() -> Self {
        Self {
            elapsed: 0.0,
            camera: Camera::new(960.0, 540.0),
            batch: None,
            frame: None,
            pixels: vec![0; (VIDEO_W * VIDEO_H * 4) as usize],
        }
    }
}

impl AppState for VideoDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let gpu = ctx.gpu();
        self.batch = Some(SpriteBatch::new(gpu));
        self.frame = Some(
            GpuVideoFrameBuffer::new(gpu, VIDEO_W, VIDEO_H, wgpu::TextureFormat::Rgba8UnormSrgb)
                .expect("streamed video texture should be valid"),
        );
    }

    fn update(&mut self, ctx: &mut FrameContext) {
        self.elapsed += ctx.dt();
        let [w, h] = ctx.logical_surface_size();
        self.camera = Camera::new(w, h);
        write_synthetic_video_frame(&mut self.pixels, VIDEO_W, VIDEO_H, self.elapsed);

        let frame = self
            .frame
            .as_ref()
            .expect("video frame buffer is initialized");
        let batch = self.batch.as_mut().expect("sprite batch is initialized");
        let gpu = ctx.gpu();

        frame
            .write_rgba8(gpu, &self.pixels)
            .expect("generated video frame must match texture dimensions");

        batch.set_texture(frame.texture());
        batch.draw(Sprite::new(
            0.0,
            0.0,
            w.min(800.0),
            w.min(800.0) * 9.0 / 16.0,
        ));
        batch.flush_to_surface(gpu, &self.camera, Some(Color::rgb(0.015, 0.017, 0.022)));

        ctx.set_title(&format!(
            "video_demo | GPU streamed texture | t={:.2}s",
            self.elapsed
        ));
    }
}

fn main() {
    let mut world = World::new();
    world
        .install(WindowPlugin::new("video_demo", 960, 540))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();

    App::new(world).run(VideoDemo::new());
}

fn write_synthetic_video_frame(pixels: &mut [u8], width: u32, height: u32, time: f32) {
    let cx = width as f32 * (0.5 + 0.28 * (time * 1.3).cos());
    let cy = height as f32 * (0.5 + 0.22 * (time * 1.7).sin());
    let scan = (time * 90.0).fract();

    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let uvx = x as f32 / width as f32;
            let uvy = y as f32 / height as f32;
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let glow = (1.0 - ((dx * dx + dy * dy).sqrt() / 90.0)).clamp(0.0, 1.0);
            let wave = ((uvx * 18.0 + time * 4.0).sin() * 0.5 + 0.5)
                * ((uvy * 11.0 - time * 3.2).cos() * 0.5 + 0.5);
            let line = if ((uvy * height as f32 + scan) as i32) % 5 == 0 {
                0.08
            } else {
                0.0
            };

            pixels[i] = ((0.04 + glow * 0.95 + wave * 0.15) * 255.0).min(255.0) as u8;
            pixels[i + 1] = ((0.08 + glow * 0.32 + wave * 0.45 + line) * 255.0).min(255.0) as u8;
            pixels[i + 2] = ((0.18 + (1.0 - uvx) * 0.24 + glow * 0.18) * 255.0).min(255.0) as u8;
            pixels[i + 3] = 255;
        }
    }
}
