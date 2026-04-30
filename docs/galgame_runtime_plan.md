# SkyEngine Galgame / Visual Novel Runtime Plan

## Summary

This plan turns SkyEngine into a complete Galgame / visual novel production runtime while keeping the engine's native ECS, renderer, app loop, input, audio, asset, and UI layers as the foundation.

The target is not to embed Ren'Py, WebGAL, TyranoScript, Naninovel, or Dialogic directly. The target is to learn from their proven user experience and build a SkyEngine-native solution:

- Ren'Py-style authoring concepts: story nodes, `scene`, `show`, `hide`, dialogue lines, menus, jumps, rollback, history, save/load.
- WebGAL-style readable script commands for Chinese visual novel authors.
- Naninovel-style runtime completeness: layered scene presentation, text reveal, choices, audio, variables, localization, backlog, save thumbnails, and production UI.
- SkyEngine-native execution: ECS components, `RenderComposer`, `SpriteFeature`, `Live2DFeature` when enabled, `sky_engine::ui`, `sky_engine::audio`, and app/input resources.

The final deliverable should be a first-class module, tentatively `sky_engine::vn`, plus examples and documentation:

```rust,no_run
use sky_engine::app::{App, AppConfig};
use sky_engine::ecs::World;
use sky_engine::vn::{VnLoader, VnPlugin, YarnProject};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = YarnProject::load("assets/vn/project.vn.toml")?;
    let mut world = World::new();
    VnPlugin::default().install(&mut world)?;
    world
        .get_resource_mut::<VnLoader>()
        .expect("VN loader should be installed")
        .load_project(project);

    App::new(AppConfig::new("Sky VN", 1280, 720), world).run(Game);
    Ok(())
}
```

The user-facing goal is that a small visual novel can be built without writing custom Rust gameplay code, while advanced games can still take over systems, UI, render features, or script commands.

## Current Implementation Status

The current codebase now has the renderer-agnostic VN core in `src/vn`:

- Official `yarnspinner` compiler boundary plus SkyEngine-owned IR conversion.
- `YarnProject` manifest loading and script validation.
- `VnRuntime` with nodes, jumps, calls/returns, variables, choices, waits, actions, snapshots, and restore.
- Dialogue, scene, audio intent, video intent, preferences, UI mode, debug, save/load, rollback, localization, command registry, and ECS marker component types.
- `VnPlugin` installs the core resources into `World`.
- `sync_runtime_scene_to_world` syncs VN scene state into ECS sprites behind `app`.
- `examples/vn/minimal.rs` demonstrates headless script execution, audio intent, rollback snapshots, and save slots.
- `examples/vn/sprite_demo.rs` demonstrates app-backed sprite presentation with placeholder colors.

Remaining production work is backend binding rather than core state design: retained UI screen construction, real audio/video playback, texture asset lookup policy, thumbnail capture, platform save locations, Live2D actor sync, and editor/debug overlays.

## Non-Goals

- Do not copy source code from existing engines.
- Do not make SkyEngine depend on Python, browser runtimes, Unity, Godot, or external editor stacks.
- Do not force all games into one rigid scene schema. Keep VN state typed and extensible.
- Do not block the rest of SkyEngine's renderer on VN-specific assumptions.
- Do not require Live2D. It should be an optional feature path.
- Do not require a visual editor in the first version. A script-first runtime is the correct core.

## Reference Model

Use existing engines as product references, not implementation sources.

### Ren'Py

Borrow:

- Simple node/jump script structure.
- Character declarations.
- Scene/show/hide mental model.
- Save/load, rollback, skip, auto mode, backlog.
- Mature visual novel menu structure.

Avoid:

- Python embedding as the core runtime.
- Direct compatibility as a hard requirement.
- Copying transform language or UI internals.

### WebGAL

Borrow:

- Friendly command syntax.
- Chinese-author-friendly workflow.
- Web-style asset organization and simple preview loop.
- Straightforward character/background/BGM commands.

Avoid:

- Browser runtime dependency.
- Reusing MPL-covered code inside SkyEngine.

### Naninovel

Borrow:

- Production completeness checklist.
- Service-style subsystems for script, actors, audio, state, localization, save/load.
- Clear separation between script commands and presentation implementation.

Avoid:

- Unity-specific service model.
- Editor-first architecture before the runtime is stable.

### Yarn Spinner / Ink

Use Yarn Spinner as the primary mature script model:

- `.yarn` is readable, node-based, and already built around dialogue, choices, variables, conditions, and commands.
- Yarn commands map cleanly to VN presentation commands such as `<<scene>>`, `<<show>>`, `<<hide>>`, `<<play_bgm>>`, and `<<voice>>`.
- Line IDs and string tables fit localization and voice workflows.
- The command-handler model lets SkyEngine own rendering, audio, save/load, UI, and asset loading.

Use Ink as an optional importer/backend later:

- Ink is excellent for complex branching prose and variable-heavy narrative logic.
- Ink can be supported by translating external commands/tags into SkyEngine VN intents.
- Ink should not be the first runtime target because Yarn's command and line model is closer to Galgame production needs.

## Target Feature Set

The complete SkyEngine VN solution should support:

- Script loading, parsing, validation, and diagnostics.
- Labels, jumps, calls/returns, choices, conditions, variables, and flags.
- Dialogue with speaker metadata, text effects, typewriter reveal, auto mode, skip mode, and backlog.
- Backgrounds, CGs, character sprites, expression changes, layered composition, transitions, and screen effects.
- OP/ED/cutscene video playback through `sky_engine::video`, including frame-sequence clips and the `video-ffmpeg` MP4 decoder path.
- Optional Live2D characters behind the existing Live2D feature.
- BGM, sound effects, voice lines, audio buses, fades, loops, and voice replay.
- Save/load with screenshots, metadata, story state, visible scene state, variables, history, and audio state.
- Rollback and forward replay for recent script states.
- Localization-ready line IDs and external text tables.
- A production UI shell: title, main menu, in-game menu, preferences, save/load, backlog, choices, confirmation dialogs.
- Asset preloading and release policies for smooth chapter transitions.
- Debug tools: script stepper, current node display, variable inspector, missing asset report, node jump.
- Examples that compile with `--features app ui audio` where applicable.

## Proposed Public API

The high-level runtime should be easy to install:

```rust,no_run
let vn = VnRuntime::from_project_path("assets/vn/project.vn.toml")?;
world.insert_resource(vn);

world.group("vn")
    .add(vn_input_system)
    .add(vn_script_system)
    .add(vn_scene_system)
    .add(vn_audio_system)
    .add(vn_ui_system);
```

For app users, prefer a plugin-style helper:

```rust,no_run
let project = YarnProject::load("assets/vn/project.vn.toml")?;
let mut world = World::new();
VnPlugin::default().install(&mut world)?;
world
    .get_resource_mut::<VnLoader>()
    .expect("VN loader should be installed")
    .load_project(project);
App::new(config, world).run(Game);
```

Core exports:

```rust,no_run
pub mod vn {
    pub use runtime::{VnRuntime, VnRuntimeConfig, VnStatus};
    pub use script::{VnCommand, VnDiagnostic, VnValue, YarnProject, YarnScript};
    pub use scene::{VnSceneState, VnActor, VnLayer, VnTransition};
    pub use dialogue::{VnDialogueState, VnLine, VnChoice, VnBacklog};
    pub use save::{VnSaveSlot, VnSaveData, VnSaveStore};
    pub use loader::{VnLoader, VnLoaderStatus};
    pub use plugin::VnPlugin;
}
```

## Script Format

Use Yarn Spinner's `.yarn` text format as the primary story format instead of inventing a SkyEngine-only language.

SkyEngine owns the VN command vocabulary, runtime state, renderer/audio/UI integration, and project manifest. Yarn owns story flow: nodes, lines, choices, variables, conditions, jumps, and command dispatch.

Example:

```text
title: Start
---
<<scene "bg/classroom.png" transition="fade" duration=0.4>>
<<play_bgm "audio/bgm/morning.ogg" loop=true fade=1.0 volume=0.8>>
<<show alice expression="smile" at="right" z=10 transition="dissolve">>

Alice: 早上好。 #line:start.alice.0001
今天的天空很亮。 #line:start.narrator.0001

-> 和 Alice 一起走
    <<set $route = "alice">>
    Alice: 那就出发吧。 #line:start.alice.0002
    <<jump Ending>>
-> 一个人去学校 <<if $can_go_alone>>
    <<set $route = "alone">>
    我决定绕一条更安静的路。 #line:start.narrator.0002
    <<jump Ending>>
===

title: Ending
---
<<stop_bgm fade=1.0>>
再见。 #line:ending.narrator.0001
===
```

SkyEngine command families:

- Yarn-native flow: nodes, jumps, choices, `<<if>>`, `<<elseif>>`, `<<else>>`, variables, expressions.
- Scene: `<<scene>>`, `<<bg>>`, `<<cg>>`, `<<show>>`, `<<hide>>`, `<<move>>`, `<<camera>>`.
- Audio: `<<play_bgm>>`, `<<stop_bgm>>`, `<<play_se>>`, `<<voice>>`, `<<stop_voice>>`.
- Video: `<<play_video>>`, `<<stop_video>>`, `<<pause_video>>`, `<<resume_video>>`, `<<seek_video>>`.
- UI/runtime: `<<wait>>`, `<<checkpoint>>`, `<<preload>>`, `<<release>>`, `<<notify>>`, `<<unlock_cg>>`.
- Extension: unknown Yarn commands can be routed to registered Rust command handlers.

The first implementation should not attempt full custom language design. It should load Yarn scripts, validate the subset SkyEngine supports, and expose clear diagnostics for unsupported commands or malformed project metadata.

Project metadata belongs in a small manifest, not inside story files:

```toml
title = "Sky VN"
start_node = "Start"
resolution = [1280, 720]
default_language = "zh-CN"

scripts = [
  "story/main.yarn",
  "story/routes/alice.yarn",
]

[characters.alice]
display_name = "Alice"
color = "#8fd3ff"
default_expression = "smile"
```

## Internal Architecture

Add a new module:

```text
src/vn/
  mod.rs
  plugin.rs
  runtime.rs
  script/
    mod.rs
    ast.rs
    parser.rs
    diagnostics.rs
    validate.rs
  dialogue.rs
  scene.rs
  actor.rs
  transitions.rs
  audio.rs
  save.rs
  rollback.rs
  localization.rs
  ui.rs
  debug.rs
```

Feature flags:

- `vn`: script/runtime/state without requiring app UI.
- `vn-ui`: production UI shell, depends on `ui`.
- `vn-audio`: BGM/SE/voice integration, depends on `audio`.
- `vn-live2d`: Live2D actor integration, depends on `live2d` and `app`.
- `vn-tools`: debug inspector and editor-facing helpers.

Suggested `Cargo.toml` feature shape:

```toml
vn = ["asset", "serde"]
vn-ui = ["vn", "ui"]
vn-audio = ["vn", "audio"]
vn-live2d = ["vn", "live2d"]
vn-tools = ["vn", "diagnostics"]
```

## Runtime State Model

The runtime should separate script state from presentation state.

Script state:

- current script asset;
- current node;
- instruction pointer;
- call stack;
- variables;
- active choice;
- command wait state;
- checkpoints;
- deterministic random seed if needed.

Presentation state:

- current background / CG;
- actors by ID;
- actor layer, position, expression, opacity, transform, transition;
- dialogue line, reveal cursor, line completion state;
- backlog;
- active UI mode;
- active audio handles;
- pending asset loads.

This split lets save/load and rollback restore exact story state while scene systems animate toward the desired presentation.

## ECS Integration

VN visible objects should use ordinary ECS entities so they compose with the renderer:

- `VnBackground`: desired background asset and transition state.
- `VnActorSprite`: actor ID, expression asset, layer, transform, opacity.
- `VnLive2DActor`: optional Live2D model binding.
- `VnSceneLayer`: ordering and blend metadata.
- `VnDialogueUi`: dialogue box root marker.
- `VnChoiceUi`: choice list root marker.

Runtime systems:

- `vn_input_system`: maps click/keyboard/gamepad to VN actions.
- `vn_script_system`: advances commands until blocked by wait/input/choice/loading.
- `vn_scene_apply_system`: turns desired scene state into ECS sprite/Live2D entities.
- `vn_transition_system`: updates fades, dissolves, moves, and opacity.
- `vn_dialogue_system`: typewriter reveal, line completion, backlog append.
- `vn_audio_system`: BGM/SE/voice commands through `AudioServer`.
- `vn_ui_system`: creates/updates UI nodes for dialogue, choices, menus.
- `vn_save_request_system`: handles save/load requests from UI.

## Rendering Plan

Use the existing high-level renderer first:

- Backgrounds and CGs: full-screen sprites using `SpriteFeature`.
- Character sprites: ordered sprites with layer/z metadata.
- UI: `sky_engine::ui` overlay, not world camera sprites.
- Transitions: alpha fade, dissolve shader later, movement and transform interpolation first.
- Screen effects: post-fx hooks once VN needs bloom/vignette/blur.

Layer model:

```text
0   background
10  far characters
20  normal characters
30  foreground props
40  CG overlays
90  screen effects
100 UI overlay
```

Initial transitions:

- `none`
- `fade`
- `dissolve` as alpha fade first, shader dissolve later
- `slide_left`, `slide_right`, `slide_up`, `slide_down`
- actor `move`, `scale`, `rotate`, `tint`, `shake`

Live2D integration should appear only after sprite actors are stable. It should reuse actor IDs and script commands:

```text
show alice live2d "alice.model3.json" at right expression=smile motion=greeting
motion alice "tap_body"
```

## UI Plan

Production VN UI should be built on `sky_engine::ui`.

Required screens:

- Title screen: start, load, preferences, gallery, quit.
- Dialogue HUD: name box, text box, quick buttons, voice replay.
- Choice menu: keyboard/mouse/gamepad selectable options.
- Backlog: scrollable previous lines with speaker, text, voice replay.
- Save/load: slots with timestamp, node, line preview, screenshot thumbnail.
- Preferences: text speed, auto speed, skip behavior, BGM/SE/voice volumes, fullscreen/windowed.
- Confirm dialog: overwrite save, return to title, quit.
- Debug overlay under `vn-tools`.

Required input actions:

- advance;
- skip hold/toggle;
- auto toggle;
- hide UI;
- open menu;
- open backlog;
- quick save;
- quick load;
- choice up/down/confirm/cancel.

The runtime should expose logical VN actions, not raw keys:

```rust,no_run
pub enum VnAction {
    Advance,
    Skip,
    Auto,
    HideUi,
    Menu,
    Backlog,
    QuickSave,
    QuickLoad,
    Confirm,
    Cancel,
    Up,
    Down,
}
```

## Audio Plan

Use `sky_engine::audio` behind `vn-audio`.

Audio buses:

- `master`
- `bgm`
- `sfx`
- `voice`
- `ui`

Script commands:

```text
<<play_bgm "audio/theme.ogg" loop=true fade=1.0 volume=0.8>>
<<stop_bgm fade=0.5>>
<<play_se "audio/door.ogg" volume=0.9>>
<<voice alice "voice/alice_0001.ogg">>
```

Runtime requirements:

- BGM loop and fade.
- Crossfade between tracks.
- SE one-shots.
- Voice playback tied to current line.
- Stop voice on line advance by default.
- Voice replay from current line and backlog.
- Per-bus volume saved in preferences.

## Asset Plan

Recommended asset layout:

```text
assets/vn/
  project.vn.toml
  story/main.yarn
  story/routes/alice.yarn
  lang/zh_cn.toml
  images/bg/classroom.png
  images/characters/alice/smile.png
  images/cg/opening.png
  audio/bgm/morning.ogg
  audio/se/door.ogg
  audio/voice/alice/0001.ogg
```

Add a `VnManifest` later for larger projects:

```toml
title = "Sky VN"
start_node = "Start"
resolution = [1280, 720]
default_language = "zh-CN"

scripts = [
  "story/main.yarn",
  "story/routes/alice.yarn",
]

[characters.alice]
display_name = "Alice"
color = "#8fd3ff"
```

Preloading rules:

- Preload current node's immediate visible assets.
- Preload choice branch first command assets once choices are visible.
- Allow explicit `preload` commands for expensive CG/Live2D moments.
- Release previous chapter assets at node boundaries when safe.

## Save, Load, And Rollback

Save data must be serde-friendly and versioned:

```rust,no_run
pub struct VnSaveData {
    pub version: u32,
    pub script_id: String,
    pub node: String,
    pub instruction: u32,
    pub call_stack: Vec<VnStackFrame>,
    pub variables: VnVariables,
    pub scene: VnSceneSnapshot,
    pub dialogue: VnDialogueSnapshot,
    pub audio: VnAudioSnapshot,
    pub backlog: Vec<VnBacklogLine>,
    pub thumbnail: Option<VnSaveThumbnail>,
    pub created_at_unix_ms: u64,
}
```

Save slots:

- Manual slots.
- Quick save slot.
- Auto save slots.
- Optional chapter checkpoint slots.

Rollback:

- Store compact snapshots at every completed line and choice.
- Limit by count and memory budget.
- Replaying forward should be deterministic.
- Rollback should restore dialogue, variables, scene, and audio intent.

## Localization

Every dialogue line should have a stable line ID:

```text
Alice: 早上好。 #line:start.alice.0001
```

Yarn's line IDs should be preserved and surfaced to localization, voice matching, backlog, and save metadata. If omitted, the compiler can generate deterministic IDs, but production scripts should prefer explicit IDs.

Localization table:

```toml
[start.alice.0001]
speaker = "Alice"
text = "Good morning."
voice = "audio/voice/en/alice/0001.ogg"
```

Runtime requirements:

- Language switch in preferences.
- Fallback to source text.
- Per-language voice paths.
- Font selection through UI config.

## Diagnostics And Tooling

The script compiler should report:

- duplicate nodes;
- missing nodes;
- missing assets;
- unknown characters;
- malformed commands;
- invalid conditions;
- unreachable nodes as warnings;
- missing localization IDs as warnings in strict mode.

Debug overlay under `vn-tools`:

- current script path;
- current node and instruction index;
- current command;
- variables;
- call stack;
- visible actors;
- pending waits;
- loaded assets;
- buttons for step, continue, jump to node, reload script.

Later tooling:

- CLI validator: `cargo run --bin sky-vn-check -- assets/vn/project.vn.toml`.
- Script-to-graph visualizer.
- Minimal desktop preview app.
- Optional editor integration after runtime is stable.

## Implementation Phases

## Phase 0: Contract And Spike

Purpose: lock the design and prove SkyEngine can display a VN scene through existing systems.

Tasks:

- Add this plan and a smaller `docs/vn.md` API page once the first API exists.
- Create `examples/vn/minimal.rs` or `examples/render/vn_demo.rs`.
- Hard-code one background, one character sprite, dialogue UI, and click-to-advance.
- Verify current renderer/UI/audio feature combinations.
- Decide final feature flag names.

Deliverables:

- A non-scripted prototype with background, sprite actor, text box, and advance input.
- A short API note showing intended `VnPlugin` usage.

Validation:

- `cargo check --examples --features "app ui"`
- Manual run of the prototype.

Exit criteria:

- The team agrees VN runtime should live as `src/vn`.
- The prototype proves that no renderer rewrite is needed for phase 1.

## Phase 1: Yarn Project Loader, Compiler Boundary, And Validator

Purpose: establish a mature authoring core before building runtime behavior.

Tasks:

- Add `src/vn/script`.
- Define `YarnProject`, `YarnScript`, `YarnNode`, `YarnLine`, `YarnCommand`, `YarnChoice`, and `VnValue`.
- Use the official Rust `yarnspinner` compiler as the first parser/compiler boundary, then convert the supported `.yarn` surface into SkyEngine-owned IR for runtime and presentation state.
- Implement project manifest loading.
- Implement strict validation for the supported Yarn subset.
- Implement diagnostics with line/column spans.
- Implement validation for nodes, jumps, characters, SkyEngine commands, and basic asset references.
- Add unit tests for loader/compiler boundary and validator.

Initial supported syntax:

- Yarn nodes with `title: ...`, `---`, and `===`.
- Dialogue lines with optional speaker prefix.
- Options using `->`.
- `<<jump NodeName>>`.
- `<<set $name = value>>`.
- Yarn `<<if>>`, `<<elseif>>`, `<<else>>`, `<<endif>>`.
- SkyEngine commands: `<<scene>>`, `<<show>>`, `<<hide>>`, `<<play_bgm>>`, `<<stop_bgm>>`, `<<play_se>>`, `<<voice>>`, `<<wait>>`, `<<checkpoint>>`.

Deliverables:

- `YarnProject::load`.
- `YarnScript::parse_str` or adapter around the chosen Yarn parser.
- `VnDiagnostic`.
- Yarn fixture tests.

Validation:

- `cargo test vn::script`

Exit criteria:

- Invalid Yarn scripts produce useful errors.
- A small branching Yarn project loads and validates.

## Phase 2: Runtime Interpreter

Purpose: execute story flow independently from rendering.

Tasks:

- Add `VnRuntime`.
- Implement instruction pointer, nodes, call stack, variables, choices, and wait states.
- Implement `advance`, `choose`, `jump_to_node`, `can_advance`, and status inspection.
- Add deterministic runtime tests with no renderer or app dependency.
- Add extension command placeholder handling.

Deliverables:

- Runtime can execute dialogue, scene commands, choices, variable sets, and jumps.
- Runtime exposes presentation intents but does not yet create ECS entities.

Validation:

- `cargo test vn::runtime`

Exit criteria:

- A test script can branch and reach different endings.
- Runtime state can be serialized enough for phase 5 save/load.

## Phase 3: Scene And Dialogue Presentation

Purpose: make script execution visible in a real app.

Tasks:

- Add `VnSceneState`, `VnDialogueState`, `VnActor`, and `VnTransition`.
- Implement sprite background and actor entity sync.
- Implement text reveal/typewriter behavior.
- Implement backlog append.
- Implement click-to-complete-line and click-to-next-line.
- Implement simple fade/move transitions.
- Add `VnPlugin` or installation helper for app examples.

Deliverables:

- Example plays a small script with background, actor show/hide, dialogue, and choices.
- VN state is represented in ECS-friendly resources/components.

Validation:

- `cargo check --examples --features "app ui"`
- Manual run of `vn_demo`.

Exit criteria:

- A five-minute script can be played from start to finish with no custom game code.

## Phase 4: Production UI Shell

Purpose: move from demo UI to usable Galgame UI.

Tasks:

- Build reusable dialogue HUD.
- Build choice menu with keyboard/mouse/gamepad navigation.
- Build title screen.
- Build in-game menu.
- Build preferences screen.
- Build backlog screen.
- Build save/load slot UI without persistence first.
- Add theme/config structs for colors, sizes, fonts, and layout.

Deliverables:

- `VnUiTheme`.
- `VnUiState`.
- Production-quality default UI.
- Example with title screen and in-game menu.

Validation:

- `cargo check --examples --features "app ui"`
- Manual viewport checks at 1280x720, 1920x1080, and a small window.

Exit criteria:

- A player can start, read, choose, open menu, inspect backlog, adjust preferences, and return.

## Phase 5: Save, Load, Auto Save, And Rollback

Purpose: support the features players expect from a real visual novel.

Tasks:

- Define versioned save schema.
- Serialize script state, variables, scene state, dialogue state, backlog, preferences, and audio intent.
- Implement manual save/load slots.
- Implement quick save/load.
- Implement auto save on node/choice/checkpoint.
- Capture save thumbnails from current frame if supported; otherwise use metadata-only fallback.
- Implement rollback snapshots per line/choice.
- Add migration hook for future save versions.

Deliverables:

- `VnSaveStore`.
- `VnSaveData`.
- Save/load UI connected to persistence.
- Rollback controls.

Validation:

- Unit tests for save round trip.
- Manual tests: save during line, save at choice, load after restart, rollback across choice boundary.

Exit criteria:

- A player can close the app and resume the same scene and story state.

## Phase 6: Audio And Voice

Purpose: make the runtime emotionally complete.

Tasks:

- Add `vn-audio` feature.
- Map script commands to `AudioServer`.
- Implement BGM play/stop/fade/crossfade.
- Implement SE one-shots.
- Implement voice line playback.
- Implement voice replay from current line and backlog.
- Persist audio preferences and restore audio intent on load.

Deliverables:

- `VnAudioState`.
- Script audio commands.
- Audio-enabled example.

Validation:

- `cargo check --examples --features "app ui audio vn-audio"`
- Manual BGM crossfade, SE, voice replay, load restore.

Exit criteria:

- A normal VN scene can use BGM, SE, and voice lines without custom Rust code.

## Phase 7: Localization, Asset Pipeline, And Project Structure

Purpose: support real projects with many scripts and languages.

Tasks:

- Add `VnManifest`.
- Support multiple script files and `include`.
- Add localization tables.
- Add explicit line IDs and generated ID warnings.
- Add asset preloading/release policies.
- Add CLI validator.
- Add missing asset report and localization coverage report.

Deliverables:

- `assets/vn` sample project.
- `sky-vn-check` CLI or cargo example.
- Localization example.

Validation:

- Unit tests for includes and localization fallback.
- CLI validation against sample project.

Exit criteria:

- A multi-chapter, multi-language VN project can be organized cleanly.

## Phase 8: Advanced Presentation And Extensibility

Purpose: make the runtime expressive enough for commercial-style games.

Tasks:

- Add Live2D actor support behind `vn-live2d`.
- Add shader-backed dissolve once render pipeline support is ready.
- Add camera shake, zoom, pan, and CG gallery unlocks.
- Add custom command registration API.
- Add custom actor backend registration API.
- Add custom UI screen hooks.
- Add debug stepper and hot reload under `vn-tools`.

Deliverables:

- Live2D VN example.
- Advanced transition example.
- Custom command example.
- Debug overlay.

Validation:

- `cargo check --examples --features "app ui audio live2d vn-live2d vn-tools"`
- Manual hot-reload and debug stepping.

Exit criteria:

- Users can extend the VN runtime without forking `src/vn`.

## Phase 9: Polish, Documentation, And Stability

Purpose: make the feature maintainable and pleasant.

Tasks:

- Write `docs/vn.md`.
- Add examples: minimal, choices, audio, save_load, localization, live2d.
- Add script reference.
- Add migration notes.
- Add performance budget notes.
- Add CI test coverage for Yarn loading/runtime/save.
- Add example compile checks to development checklist.

Deliverables:

- Stable public API for `sky_engine::vn`.
- Complete docs and examples.
- Release checklist.

Validation:

- `cargo test`
- `cargo test --features "app ui audio vn vn-ui vn-audio"`
- `cargo check --examples --features "app ui audio vn-ui vn-audio"`

Exit criteria:

- The VN runtime is ready to be advertised as a complete SkyEngine solution.

## Milestone Roadmap

Suggested milestone grouping:

- MVP: Phases 0-3. Play a small visual novel script.
- Usable Alpha: Phases 4-5. Real UI, save/load, rollback.
- Production Beta: Phases 6-7. Audio, localization, project structure, validator.
- Complete Runtime: Phases 8-9. Live2D, extensibility, tools, stable docs.

The MVP should be small and strict. Avoid adding every Ren'Py-like feature before the runtime loop is solid.

## Testing Strategy

Parser:

- valid script fixtures;
- invalid script fixtures;
- diagnostic span tests;
- whitespace/comment tests.

Runtime:

- linear story;
- branch story;
- nested call/return;
- variable condition;
- choice gating;
- wait states;
- deterministic replay.

Presentation:

- scene state snapshots;
- transition interpolation;
- actor ordering;
- dialogue reveal behavior;
- backlog append.

Persistence:

- save/load round trip;
- old save version migration;
- rollback snapshot budget;
- load at choice;
- load during transition.

Examples:

- compile with expected feature sets;
- manual smoke test for rendering/UI/audio.

## Performance And Reliability Notes

- Script execution is not a hot path; clarity and diagnostics matter more than micro-optimization.
- Scene sync should avoid respawning entities every frame. Diff desired state against existing actor entities.
- Typewriter/UI updates should allocate minimally during normal playback.
- Asset preloading should prevent frame spikes at visible transitions.
- Save/load should never store raw ECS entity IDs as durable state.
- Rollback snapshots should be compact and bounded.
- Runtime tests should not require a GPU.

## Open Decisions

- Final manifest extension: `project.vn.toml` versus a shorter `vn.toml`.
- Whether `vn-ui` should be folded into `vn` once UI is considered core.
- Whether to use RON/TOML for manifest and localization, or a stricter custom format.
- How much Ren'Py syntax compatibility to intentionally support.
- Whether Ink import belongs in core or tools.
- Save file location policy per platform.
- Thumbnail capture API shape.

## First Implementation Slice

The best first coding slice is:

1. Create `src/vn/mod.rs` behind `vn`.
2. Add `YarnProject` manifest loading and a small `.yarn` adapter for nodes, lines, options, variables, jumps, and command dispatch.
3. Add runtime tests for a branching Yarn script.
4. Add `examples/render/vn_demo.rs` that manually feeds Yarn-derived runtime intents into the current sprite/UI renderer.
5. Only after the demo works, promote the integration into `VnPlugin`.

This keeps the first PR small enough to review while proving the whole direction.
