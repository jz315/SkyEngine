mod actions;
mod aftertalk;
mod anomaly;
mod app;
mod case_dialogue;
mod case_file;
mod chapter;
mod companion;
mod content;
mod conversation;
mod departure;
mod dialogue;
mod dialogue_challenge;
mod dialogue_lead;
mod dialogue_question;
mod dialogue_relay;
mod dialogue_system;
mod ending_aftermath;
mod evidence;
mod final_debate;
mod final_interview;
mod final_prelude;
mod inner_voice;
mod lamp_focus;
mod memory;
mod model;
mod patrol;
mod resonance;
mod route_cost;
mod route_echo;
mod route_pressure;
mod route_witness;
mod route_witness_debrief;
mod save;
mod station_request;
mod station_whisper;
mod theme;
mod trial;
mod truth;
mod view;
mod vow;

use sky_engine::app::{App, AssetPlugin, InputPlugin, RenderPlugin, RunnerPlugin, WindowPlugin};
use sky_engine::ecs::World;

fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("雾灯站", 1440, 860)
                .with_vsync(true)
                .with_resizable(true),
        )
        .unwrap();
    world
        .install(RunnerPlugin::game().with_frame_rate_limit(60.0))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world.install(RenderPlugin::forward_2d()).unwrap();

    App::new(world).run(app::FogLanternStation::default());
}
