# Visual Novel / Galgame

`sky_engine::vn` is the renderer-agnostic visual novel core. It uses the
official `yarnspinner` Rust compiler as the first parser/compiler boundary,
then converts the supported script surface into SkyEngine's save-friendly IR.
The runtime executes branching flow and maintains dialogue, scene, audio,
video, save, rollback, localization, UI, and debug state that can later be
bound to the app/render/audio layers.

Enable it with:

```toml
sky_engine = { version = "...", features = ["vn"] }
```

## Quick Start

The app-facing path is plugin install plus one VN resource:

```rust,no_run
use sky_engine::app::{App, AppConfig};
use sky_engine::ecs::World;
use sky_engine::vn::{VnPlugin, VnResource};

let mut world = World::new();
VnPlugin::default().install(&mut world)?;

world
    .get_resource_mut::<VnResource>()
    .expect("VnPlugin installs VnResource")
    .load_project_path("assets/vn/project.vn.toml")?;

App::new(AppConfig::new("Sky VN", 1280, 720), world).run(|ctx| {
    ctx.render();
});
# Ok::<(), Box<dyn std::error::Error>>(())
```

For headless tests or tools, use `VnResource` directly:

```rust,no_run
use sky_engine::ecs::World;
use sky_engine::vn::{vn_load_system, VnResource, VnRuntimeEvent, YarnScript};

let script = YarnScript::parse_str(r#"
title: Start
---
Alice: Morning. #line:start.alice.0001
-> Continue
    <<jump Ending>>
===

title: Ending
---
See you. #line:ending.narrator.0001
===
"#)?;

let mut world = World::new();
world.insert_resource(VnResource::default());
world
    .get_resource_mut::<VnResource>()
    .unwrap()
    .load_script(script, "Start")?;
vn_load_system(&mut world);

let vn = world.get_resource_mut::<VnResource>().unwrap();
if let Some(VnRuntimeEvent::Line(line)) = vn.advance()? {
    println!("{}", line.text);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

For the lowest-level runtime API, `VnRuntime` remains public for expert code:

```rust,no_run
# use sky_engine::vn::{VnAction, VnRuntime, VnRuntimeEvent, YarnScript};
# let script = YarnScript::parse_str("title: Start\n---\nDone. #line:start.1\n===")?;
let mut runtime = VnRuntime::from_script(script, "Start")?;
while !matches!(runtime.status(), sky_engine::vn::VnStatus::Ended) {
    let Some(event) = runtime.apply_action(VnAction::Advance)? else {
        continue;
    };
    match event {
        VnRuntimeEvent::Line(line) => println!("{}", line.text),
        VnRuntimeEvent::Choices(_) => runtime.choose(0)?,
        VnRuntimeEvent::Command(command) => println!("{}", command.raw),
        VnRuntimeEvent::Wait(_) => runtime.complete_wait(),
        VnRuntimeEvent::End => break,
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Current Surface

- `YarnScript::parse_str` / `parse_source`: parse and validate one script.
- `YarnProject::load`: load `project.vn.toml` plus listed `.yarn` files.
- `VnResource`: the normal app-facing control surface for loading, runtime access, actions, save/load, preferences, UI state, rollback, and presentation caches.
- `VnPlugin`: installs `VnResource` and VN systems into an ECS `World`.
- `VnRuntime`: execute nodes, jumps, choices, variables, waits, and commands.
- `VnDialogueState`: current line, reveal cursor, choices, and backlog.
- `VnSceneState`: background, CG, sprite actor intents, camera placeholder.
- `VnAudioState` / `VnVideoState`: backend-neutral BGM, SFX, voice, and video intents.
- `VnAssetState` / `VnProgressState`: preload/release intents, checkpoints, notifications, and CG unlocks.
- `VnRuntimeSnapshot`: serializable script, dialogue, scene, audio, video, asset, progress, and wait state.
- `VnSaveStore`: versioned save slots with TOML round trip support.
- `VnRollbackStack`: bounded rollback snapshots for line/choice checkpoints.
- `VnLocalizationTable`: line-id based text, speaker, and voice replacement.
- `VnUiState`, `VnPreferences`, `VnCommandRegistry`, and marker components remain public expert types, but `VnPlugin` no longer installs them as separate resources.
- `sync_runtime_scene_to_world` behind `app`: syncs `VnSceneState` into ECS sprites using `SpriteRenderer`.

Supported script syntax currently includes node headers, dialogue lines,
`->` choices, `<<jump>>`, `<<call>>`, `<<return>>`, `<<set>>`, conditionals,
`<<wait>>`, and presentation commands such as `<<scene>>`, `<<show>>`,
`<<hide>>`, `<<move>>`, `<<cg>>`, `<<play_bgm>>`, `<<play_se>>`,
`<<voice>>`, `<<play_video>>`, `<<checkpoint>>`, `<<preload>>`, and
extension commands routed through `VnCommandRegistry`.

YarnSpinner diagnostics are mapped into `VnDiagnostic` before SkyEngine's own
validator runs. The adapter also auto-declares condition-only variables as
booleans so small VN scripts can stay lightweight while still passing the
official compiler.

## Validation

Use:

```bash
cargo test --features vn vn
cargo run --example vn_minimal --features vn
cargo check --example vn_sprite_demo --features "vn app"
cargo test --features "vn app" vn::presentation
```

The VN core has no GPU, UI, or audio backend dependency. Sprite presentation is
available when `app` is enabled. The remaining production bindings are retained
UI screens, real audio playback, video texture presentation, thumbnail capture,
platform save locations, and Live2D actor sync.
