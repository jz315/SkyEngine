//! Minimal app-backed VN sprite presentation demo.
//!
//! This uses the VN runtime and presentation sync with placeholder colors, so
//! it does not require external image assets.
//!
//! ```bash
//! cargo run --example vn_sprite_demo --features "vn app"
//! ```

use sky_engine::app::{App, AppConfig, AppState, FrameContext};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, MainCamera, Projection, RenderPipelineAsset, SpriteFeature, Transform,
    TransparentPhase,
};
use sky_engine::vn::{
    sync_runtime_scene_to_world, VnAction, VnLoader, VnPlugin, VnRuntime, VnRuntimeEvent,
    VnSpritePresentationConfig, YarnScript,
};

struct VnSpriteDemo {
    timer: f32,
}

impl AppState for VnSpriteDemo {
    fn update(&mut self, ctx: &mut FrameContext) {
        self.timer += ctx.dt;
        if self.timer >= 1.0 {
            self.timer = 0.0;
            advance_vn(ctx.world);
        }

        sync_runtime_scene_to_world(ctx.world);
        ctx.render();
    }
}

fn advance_vn(world: &mut World) {
    let Some(runtime) = world.get_resource_mut::<VnRuntime>() else {
        return;
    };

    if runtime.status() == &sky_engine::vn::VnStatus::Ended {
        return;
    }

    if runtime.status() == &sky_engine::vn::VnStatus::Choice {
        let _ = runtime.choose(0);
    }

    match runtime.apply_action(VnAction::Advance) {
        Ok(Some(VnRuntimeEvent::Line(_))) => {
            runtime.dialogue_mut().complete_line();
        }
        Ok(Some(VnRuntimeEvent::Wait(_))) => {
            runtime.complete_wait();
        }
        Ok(_) => {}
        Err(error) => eprintln!("VN runtime error: {error}"),
    }
}

fn main() {
    let script = YarnScript::parse_str(
        r#"
title: Start
---
<<scene "bg/classroom.png" transition="fade">>
<<show alice "characters/alice/smile.png" at="left" layer=20>>
<<show bob "characters/bob/neutral.png" at="right" layer=20>>
Alice: The scene is live. #line:start.alice.0001
Bob: Even without textures, the state is going through ECS sprites. #line:start.bob.0001
-> Show CG
    <<cg "cg/opening.png" layer=40>>
    Alice: This would be a CG layer. #line:start.alice.0002
    <<jump Ending>>
===

title: Ending
---
<<hide bob>>
Alice: Done. #line:ending.alice.0001
===
"#,
    )
    .expect("demo script should parse");

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(1280.0, 720.0),
        MainCamera,
    ));
    VnPlugin::default()
        .install(&mut world)
        .expect("VN plugin should install");
    world
        .get_resource_mut::<VnLoader>()
        .expect("VN loader should be installed")
        .load_script(script, "Start");
    world.insert_resource(VnSpritePresentationConfig::default());

    App::new(AppConfig::new("SkyEngine VN Sprite Demo", 1280, 720), world)
        .with_render_pipeline(
            RenderPipelineAsset::builder()
                .add_feature(SpriteFeature::unlit())
                .add_phase(TransparentPhase::new())
                .build(),
        )
        .run(VnSpriteDemo { timer: 0.0 });
}
