//! MP4 playback demo backed by FFmpeg.
//!
//! ```bash
//! cargo run --example mp4_video_demo --features video-ffmpeg --release -- path/to/video.mp4
//! ```

use std::env;
use std::path::PathBuf;

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::expert::SpriteBatch;
use sky_engine::render::{Camera, Color, Sprite};
use sky_engine::video::{
    FfmpegVideoOptions, FfmpegVideoPlayer, FfmpegVideoUpdate, VideoPlaybackState,
};

struct Mp4VideoDemo {
    path: PathBuf,
    camera: Camera,
    batch: Option<SpriteBatch>,
    player: Option<FfmpegVideoPlayer>,
    last_update: Option<FfmpegVideoUpdate>,
}

impl Mp4VideoDemo {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            camera: Camera::new(1280.0, 720.0),
            batch: None,
            player: None,
            last_update: None,
        }
    }
}

impl AppState for Mp4VideoDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let gpu = ctx.gpu();
        self.batch = Some(SpriteBatch::new(gpu));
        self.player = Some(
            FfmpegVideoPlayer::open_with_options(
                gpu,
                &self.path,
                FfmpegVideoOptions::default().looped(true),
            )
            .expect("failed to open video with FFmpeg"),
        );
    }

    fn update(&mut self, ctx: &mut FrameContext) {
        let [surface_w, surface_h] = ctx.logical_surface_size();
        self.camera = Camera::new(surface_w, surface_h);
        let dt = ctx.dt();

        let gpu = ctx.gpu();
        let player = self.player.as_mut().expect("player is initialized");
        let update = player.update(gpu, dt).expect("video update failed");

        let video_w = player.width() as f32;
        let video_h = player.height() as f32;
        let scale = (surface_w / video_w).min(surface_h / video_h).min(1.0);
        let draw_w = video_w * scale;
        let draw_h = video_h * scale;

        let batch = self.batch.as_mut().expect("sprite batch is initialized");
        batch.set_texture(player.texture());
        batch.draw(Sprite::new(0.0, 0.0, draw_w, draw_h));
        batch.flush_to_surface(gpu, &self.camera, Some(Color::rgb(0.01, 0.012, 0.016)));

        let state = update.state;
        let pts = update
            .uploaded_pts_seconds
            .or_else(|| player.last_uploaded_pts_seconds())
            .unwrap_or(player.position_seconds());
        ctx.set_title(&format!(
            "mp4_video_demo | {:?} | t={:.2}s | queued={} dropped={}",
            state,
            pts,
            update.queued_frames,
            update.dropped_late_frames + update.dropped_over_capacity
        ));

        if state == VideoPlaybackState::Finished {
            let _ = player.play();
        }
        self.last_update = Some(update);
    }
}

fn main() {
    let Some(path) = env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!(
            "Usage: cargo run --example mp4_video_demo --features video-ffmpeg -- <video.mp4>"
        );
        return;
    };
    if !path.exists() {
        eprintln!("Video file does not exist: {}", path.display());
        return;
    }

    let mut world = World::new();
    world
        .install(WindowPlugin::new("mp4_video_demo", 1280, 720))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();

    App::new(world).run(Mp4VideoDemo::new(path));
}
