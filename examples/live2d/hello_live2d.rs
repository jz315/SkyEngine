//! Minimal Live2D ECS + render example.
//!
//! ```bash
//! cargo run --example hello_live2d --features live2d --release
//! cargo run --example hello_live2d --features live2d --release -- path/to/model.model3.json
//! ```

use std::path::PathBuf;

use sky_engine::app::{App, AppConfig, AppState, FrameContext};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, Live2DAnimator, Live2DModelInstance, MainCamera, Projection, RenderPipelineAsset,
    SortingLayer, Transform,
};

const DEFAULT_MODEL_PATH: &str =
    "CubismSdkForNative/CubismSdkForNative-5-r.5/Samples/Resources/Haru/Haru.model3.json";

struct HelloLive2D;

impl AppState for HelloLive2D {
    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        ctx.render();
    }
}

fn main() {
    let model_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MODEL_PATH));

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic(720.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        SortingLayer(1),
        Live2DModelInstance::new(model_path).with_height(360.0),
        Live2DAnimator::default(),
    ));

    App::new(AppConfig::new("SkyEngine - Hello Live2D", 1280, 720), world)
        .with_render_pipeline(RenderPipelineAsset::live2d_2d())
        .run(HelloLive2D);
}
