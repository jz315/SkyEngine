//! Native UI-backed VN scene demo using local example assets.
//!
//! ```bash
//! cargo run --example vn_ui_demo --features vn-ui
//! ```

use std::path::Path;

use sky_engine::app::{App, AppConfig, AppState, FrameContext};
use sky_engine::ecs::World;
use sky_engine::render::{RenderPipelineAsset, RenderSettings, SpriteFeature, TransparentPhase};
use sky_engine::vn::{VnLoader, VnPlugin, YarnProject, YarnScript};

const CORRIDOR_CG: &str = "在校园走廊的长椅上坐着_4K_202604121803.png";
const ROOM_EDIT_CG: &str = "把图2_的_人换成图一的_202604112345.png";
const STANDING_POSE: &str = "图2人物站着半身照_202604142337.png";

struct VnUiDemo;

impl AppState for VnUiDemo {
    fn update(&mut self, ctx: &mut FrameContext) {
        ctx.update_ui();
        ctx.render();
        ctx.render_ui();
    }
}

fn main() {
    let script = YarnScript::parse_str(&format!(
        r#"
title: Start
---
<<scene "{CORRIDOR_CG}" transition="fade">>
<<show alice "{STANDING_POSE}" at="right" layer=20>>
Alice: 走廊里的光刚刚好，像是下午第一节课前的安静。 #line:start.alice.0001
-> 留在走廊长椅
    Alice: 那就坐一会儿吧。书页翻过去的时候，窗外的树影也在动。 #line:route.corridor.0001
    <<jump Ending>>
-> 看之前的人物替换图
    <<cg "{ROOM_EDIT_CG}" layer=40>>
    Alice: 这一张可以当回忆 CG，用来检查人物和场景融合。 #line:route.room.0001
    <<jump Ending>>
-> 让立绘站到中间
    <<move alice to="center">>
    Alice: 现在这张半身图作为 UI 立绘层显示，位置和窗口尺寸不会再互相打架。 #line:route.pose.0001
    <<jump Ending>>
===

title: Ending
---
Alice: 这套素材已经接进 SkyEngine 的 VN runtime 和 UI 展示了。 #line:end.alice.0001
===
"#
    ))
    .expect("demo script should parse");

    let project = YarnProject::new("Start", script).expect("demo project should build");
    let initial_window_size = project.manifest.resolution;

    let mut world = World::new();
    world.insert_resource(RenderSettings {
        clear_color: sky_engine::render::Color::rgb(0.02, 0.024, 0.032),
        ..Default::default()
    });
    VnPlugin::default()
        .install(&mut world)
        .expect("VN plugin should install");
    world
        .get_resource_mut::<VnLoader>()
        .expect("VN loader should be installed")
        .set_asset_root(example_asset_root())
        .load_project(project);

    App::new(
        AppConfig::new(
            "SkyEngine VN Asset Scene",
            initial_window_size[0],
            initial_window_size[1],
        ),
        world,
    )
    .with_render_pipeline(
        RenderPipelineAsset::builder()
            .add_feature(SpriteFeature::unlit())
            .add_phase(TransparentPhase::new())
            .build(),
    )
    .run(VnUiDemo);
}

fn example_asset_root() -> impl AsRef<Path> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets")
}
