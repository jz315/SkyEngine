# Neo UI Standalone Library Working Draft

Document status: normative planning draft.

This document is the single active specification and execution plan for Neo UI.
It replaces the earlier exploratory port and usability plans. Those plans were
useful while the idea was still forming; this file is the API contract we use
before extracting Neo UI into a standalone library.

This document intentionally uses standard-like wording. `shall` is a
requirement, `shall not` is a prohibition, `should` is the preferred design
unless there is a documented reason, and `may` is permitted but not required.
Code marked as exposition-only illustrates the intended API shape. Public API
changes that conflict with this document shall update this document in the same
change.

## 1. Scope

### 1.1 Purpose

Neo UI shall become a standalone Rust game UI library.

SkyEngine shall be the first official host of Neo UI.

Neo UI shall remain inspired by EUI-NEO, but it shall not be specified as a
strict source-compatible port. Neo UI is allowed to be a Rust-first superset
when that makes the API simpler, safer, or more suitable for games.

### 1.2 In Scope

This specification defines:

- public crate and module shape;
- common authoring API;
- runtime frame API;
- input, focus, text, IME, and capture behavior;
- renderer-neutral output;
- SkyEngine adapter API;
- migration and validation requirements.

### 1.3 Out Of Scope

This specification does not define:

- HTML, CSS, or DOM compatibility;
- a general desktop application framework;
- a required render-graph phase;
- 3D world-space UI;
- source compatibility with every EUI-NEO identifier;
- a requirement that SkyEngine can only ever ship one UI backend.

## 2. Design Law

Neo UI APIs shall optimize for the following order:

1. concise common-path authoring;
2. hard-to-misuse input and focus behavior;
3. standalone library boundaries;
4. predictable rendering output;
5. mechanical traceability to the current implementation where practical.

The ordinary user shall be able to build a pause menu, settings panel,
inventory screen, dialogue box, and debug overlay by learning only:

```rust
neo_ui::Neo
neo_ui::Ui
neo_ui::Response
neo_ui::State
neo_ui::Binding
neo_ui::widgets
```

Backend registration, event translation, draw lists, renderers, GPU objects,
and SkyEngine resources shall not be required knowledge for ordinary UI
authoring.

## 3. Terms

`Neo UI` denotes the UI product and programming model.

`neo_ui` denotes the future standalone core crate.

`neo_ui_wgpu` denotes the optional standalone wgpu renderer crate.

`SkyEngine adapter` denotes `sky_engine::ui::neo`.

`Host` denotes the application, engine, or framework that owns the window loop,
input source, clocks, asset loading, renderer, and presentation lifecycle.

`Frame` denotes one UI update cycle.

`Compose` denotes invoking a user closure with `&mut Ui`.

`Capture` denotes Neo UI's request that the host not route the same pointer,
keyboard, text, or navigation input to gameplay.

`Draw output` denotes renderer-neutral primitives produced by Neo UI.

`Exposition-only` denotes illustrative code that fixes API intent but may need
minor lifetime or module-path adjustment during implementation.

## 4. Library Shape

### 4.1 Crates

The final product shape shall be:

```text
neo_ui
neo_ui_wgpu
sky_engine::ui::neo
```

`neo_ui` shall be usable without SkyEngine.

`neo_ui` shall not depend on `winit`, `wgpu`, SkyEngine `app`, SkyEngine `ecs`,
SkyEngine `gpu`, SkyEngine `input`, SkyEngine `render`, or SkyEngine `ui`.

`neo_ui_wgpu` may depend on `neo_ui`, `wgpu`, text shaping crates, and image
upload helpers.

`sky_engine::ui::neo` shall depend on `neo_ui` and may adapt SkyEngine input,
UI host, asset, and rendering services.

### 4.2 Publication Rule

The repository shall first create a clean internal boundary.

Publishing crates to crates.io shall be a consequence of a stable boundary, not
the tool used to discover the boundary.

### 4.3 Module Names

The standalone core crate shall expose these top-level names:

```rust
pub struct Neo;
pub struct Ui<'ui>;
pub struct Frame<'a>;
pub struct FrameOutput<R = ()>;
pub struct Response;
pub struct State<T>;
pub struct Binding<T>;
pub struct Id;
pub struct Screen;
pub struct Rect;
pub struct Vec2;
pub struct Color;
pub struct Capture;
pub struct DrawList;
pub mod widgets;
pub mod prelude;
```

`NeoRuntime`, `NeoState`, and current EUI-style names may remain as temporary
compatibility aliases. New examples and docs shall prefer `Neo` and `State`.

## 5. Standalone Runtime API

### 5.1 Runtime Object

The primary standalone API shall be:

```rust
use neo_ui::prelude::*;

let mut neo = Neo::new();

let output = neo.frame(
    Frame::new(Screen::new(width, height))
        .dt(dt)
        .scale_factor(scale_factor)
        .events(events),
    |ui| {
        if ui.button("resume").text("Resume").clicked() {
            game.resume();
        }

        ui.slider("music", &mut settings.music)
            .range(0.0..=1.0)
            .label("Music");
    },
);

renderer.render(&output.draw);
```

This shape is normative:

- one long-lived `Neo`;
- one `frame` call per UI frame;
- one `&mut Ui` authoring closure;
- one `FrameOutput` returned to the host.

### 5.2 `Neo`

The core runtime shall provide an API equivalent to:

```rust
pub struct Neo { /* private */ }

impl Neo {
    pub fn new() -> Self;
    pub fn with_options(options: Options) -> Self;
    pub fn options(&self) -> &Options;
    pub fn set_options(&mut self, options: Options);

    pub fn frame<R>(
        &mut self,
        frame: Frame<'_>,
        compose: impl FnOnce(&mut Ui<'_>) -> R,
    ) -> FrameOutput<R>;

    pub fn clear(&mut self);
}
```

`Neo::new()` shall be enough for ordinary standalone use.

`Neo::frame` shall run input processing, composition, layout, interaction,
animation advancement, draw output generation, and capture calculation.

`Neo::clear` shall drop runtime UI state but shall not be required during
normal frame-to-frame operation.

### 5.3 `Frame`

`Frame` shall be host-created and shall not read platform state by itself.

The API shall be equivalent to:

```rust
pub struct Frame<'a> { /* private */ }

impl<'a> Frame<'a> {
    pub fn new(screen: Screen) -> Self;
    pub fn dt(self, seconds: f32) -> Self;
    pub fn time(self, seconds: f64) -> Self;
    pub fn scale_factor(self, scale: f32) -> Self;
    pub fn events(self, events: &'a [Event]) -> Self;
    pub fn modifiers(self, modifiers: Modifiers) -> Self;
    pub fn focused(self, focused: bool) -> Self;
}
```

`Frame` shall use logical pixels.

`Frame` shall carry committed text and IME composition separately from physical
key presses.

### 5.4 `FrameOutput`

`FrameOutput` shall expose:

```rust
pub struct FrameOutput<R = ()> {
    pub value: R,
    pub draw: DrawList,
    pub capture: Capture,
    pub cursor: Option<CursorIcon>,
    pub ime: Option<ImeRequest>,
    pub needs_redraw: bool,
}
```

`FrameOutput::value` shall be the return value of the compose closure.

`FrameOutput::capture` shall be the only required signal a host needs to avoid
gameplay/UI input conflicts.

`needs_redraw` shall be true when animation, cursor blink, pending image loads,
or other runtime activity requires another frame.

## 6. SkyEngine API

### 6.1 App-Level Rule

SkyEngine's app-facing common path shall remain generic:

```rust
ctx.ui()
```

SkyEngine shall not add a long-term app-level shortcut such as:

```rust
ctx.neo(...)
```

This prohibition exists because every backend-specific `ctx.x(...)` method
makes future UI backend coexistence worse.

### 6.2 Compose Entry Point

The normative SkyEngine authoring entry point shall be:

```rust
use sky_engine::ui::neo;

neo::compose(ctx.ui(), |ui| {
    if ui.button("resume").text("Resume").clicked() {
        game.resume();
    }
});
```

The function shall be equivalent to:

```rust
pub fn compose<R>(
    ui: sky_engine::app::UiFrame<'_, '_>,
    compose: impl FnOnce(&mut neo_ui::Ui<'_>) -> R,
) -> Option<R>;
```

The first parameter shall be the generic SkyEngine UI frame facade.

The first parameter shall not be `&mut FrameContext`.

The closure shall not receive `Screen` as a second argument in the preferred
API. Code needing screen information shall call:

```rust
let screen = ui.screen();
```

### 6.3 Backend Access

The SkyEngine adapter may expose advanced backend access for debugging and
tooling, but it shall not be the common path:

```rust
ctx.ui().with_backend_mut::<neo::NeoUiBackend, _>(|backend| {
    /* advanced integration only */
});
```

Examples intended for ordinary users shall use `neo::compose(ctx.ui(), ...)`.

### 6.4 Installation

SkyEngine shall keep an installable backend:

```rust
neo::NeoUiPlugin::install(world);
```

or the current plugin-equivalent shape.

`neo::compose(ctx.ui(), ...)` may lazily install the backend during migration,
but final examples should install the backend explicitly when the app setup API
is clear.

## 7. Authoring API

### 7.1 `Ui`

`Ui` shall represent the current composition context.

`Ui` shall expose:

```rust
impl Ui<'_> {
    pub fn screen(&self) -> Screen;
    pub fn dt(&self) -> f32;

    pub fn scope<R>(&mut self, id: impl Into<Id>, f: impl FnOnce(&mut Ui<'_>) -> R) -> R;

    pub fn row(&mut self, id: impl Into<Id>) -> widgets::Row<'_>;
    pub fn column(&mut self, id: impl Into<Id>) -> widgets::Column<'_>;
    pub fn stack(&mut self, id: impl Into<Id>) -> widgets::Stack<'_>;

    pub fn text(&mut self, id: impl Into<Id>) -> widgets::Text<'_>;
    pub fn button(&mut self, id: impl Into<Id>) -> widgets::Button<'_>;
    pub fn checkbox(&mut self, id: impl Into<Id>, value: &mut bool) -> widgets::Checkbox<'_>;
    pub fn slider(&mut self, id: impl Into<Id>, value: &mut f32) -> widgets::Slider<'_>;
    pub fn input(&mut self, id: impl Into<Id>, value: &mut String) -> widgets::Input<'_>;
    pub fn image(&mut self, id: impl Into<Id>, image: impl Into<ImageSource>) -> widgets::Image<'_>;

    pub fn modal<R>(&mut self, id: impl Into<Id>, f: impl FnOnce(&mut Ui<'_>) -> R) -> R;
}
```

This list is the preferred common API. Extra widgets may live under
`neo_ui::widgets`.

### 7.2 Response-First Interaction

Interactive widgets shall return a response or provide response-query methods.

The common style shall be:

```rust
if ui.button("apply").text("Apply").clicked() {
    apply();
}

if ui.input("name", &mut name).placeholder("Name").changed() {
    save_name(&name);
}
```

Callbacks may be supported, but callbacks shall not be required for ordinary
buttons, sliders, inputs, toggles, or menu actions.

### 7.3 Builder Consumption

Interactive builders shall be consumed by response queries.

The following style shall be valid:

```rust
let clicked = ui.button("start").text("Start").clicked();
```

The following style may be available for explicitness:

```rust
let response = ui.button("start").text("Start").build();
if response.clicked() {
    start();
}
```

Widget builders shall not require users to remember a hidden finalization step
that can silently drop the widget.

### 7.4 Layout

The common layout API shall be flow-first:

```rust
ui.column("settings")
    .padding(24.0)
    .gap(12.0)
    .content(|ui| {
        ui.text("title").content("Settings");
        ui.slider("music", &mut music).range(0.0..=1.0);

        ui.row("actions").gap(8.0).content(|ui| {
            ui.button("cancel").text("Cancel");
            ui.button("save").text("Save");
        });
    });
```

`row`, `column`, and `stack` shall be first-class primitives.

Absolute positioning may exist, but it shall not be the default page-building
path.

### 7.5 Identifiers

Every interactive element shall have a stable ID.

String literals shall be accepted as IDs.

Scoped IDs shall be supported:

```rust
ui.scope("inventory", |ui| {
    ui.input("search", &mut query);
    ui.button("close").text("Close");
});
```

The implementation may internally resolve those IDs as hierarchical IDs.

Users shall not be required to manually concatenate long IDs for ordinary UI.

### 7.6 Naming

New public Rust API shall use `snake_case`.

CamelCase aliases may remain temporarily for EUI-NEO familiarity.

New documentation and examples shall not teach camelCase aliases.

## 8. State And Binding

### 8.1 Direct Mutable State

Simple widgets shall accept direct mutable references:

```rust
ui.checkbox("fullscreen", &mut settings.fullscreen);
ui.slider("volume", &mut settings.volume).range(0.0..=1.0);
ui.input("player_name", &mut profile.name);
```

This is the preferred API for local game state.

### 8.2 `State<T>`

`State<T>` shall be a built-in shared state container:

```rust
pub struct State<T> { /* private */ }

impl<T> State<T> {
    pub fn new(value: T) -> Self;
    pub fn get(&self) -> T where T: Clone;
    pub fn set(&self, value: T);
    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R;
    pub fn write<R>(&self, f: impl FnOnce(&mut T) -> R) -> R;
    pub fn binding<U>(&self, get: impl Fn(&T) -> U + 'static) -> Binding<U>
    where
        U: Clone + 'static;
}
```

`State<T>` shall be useful, but ordinary widgets shall not force all state into
`State<T>`.

### 8.3 `Binding<T>`

`Binding<T>` shall represent reusable read/write access to state:

```rust
pub struct Binding<T> { /* private */ }

impl<T> Binding<T> {
    pub fn get(&self) -> T where T: Clone;
    pub fn set(&self, value: T);
    pub fn update(&self, f: impl FnOnce(&mut T));
}
```

Widgets may accept `Binding<T>` where shared callback-driven state is more
convenient than `&mut T`.

## 9. Input, Text, And Capture

### 9.1 Event Model

Core input shall use Neo-owned event types:

```rust
pub enum Event {
    Pointer(PointerEvent),
    Scroll(ScrollEvent),
    Key(KeyEvent),
    Text(TextEvent),
    Ime(ImeEvent),
    Focus(bool),
    Navigate(NavEvent),
}
```

`neo_ui` shall not expose `winit::event::WindowEvent` in its public core API.

### 9.2 Text Rules

Physical key presses and committed text shall be distinct.

Typing `b` into a focused input shall not be represented only as `KeyB`.

IME preedit text, committed text, and IME candidate rectangle requests shall be
separate concepts.

### 9.3 Capture

`Capture` shall be equivalent to:

```rust
pub struct Capture {
    pub pointer: bool,
    pub keyboard: bool,
    pub text: bool,
    pub navigation: bool,
    pub modal: bool,
}
```

Hosts shall suppress gameplay pointer input when `capture.pointer` is true.

Hosts shall suppress gameplay keyboard input when `capture.keyboard` is true.

Hosts shall suppress gameplay text input when `capture.text` is true.

Hosts shall suppress gameplay navigation input when `capture.navigation` is
true.

### 9.4 Modal Rules

Modal UI shall:

- draw above normal content;
- block pointer hits on underlying UI;
- own keyboard focus while active;
- report capture through `FrameOutput`;
- define escape and dismiss behavior.

Common modal widgets shall not require users to hand-author z-index or capture
logic.

## 10. Draw And Rendering

### 10.1 Draw Output

`neo_ui` shall emit renderer-neutral draw output.

`DrawList` shall be capable of expressing:

- rectangles;
- text;
- images;
- vector paths or polygons;
- clipping;
- opacity;
- transforms;
- gradients;
- shadows;
- backdrop effects where supported.

`DrawList` shall not contain `wgpu` handles.

### 10.2 Renderer API

A renderer shall consume draw output and a render target supplied by the host.

An exposition-only renderer shape is:

```rust
pub trait Renderer {
    type Target<'a>;
    type Error;

    fn render(
        &mut self,
        draw: &DrawList,
        target: Self::Target<'_>,
    ) -> Result<(), Self::Error>;
}
```

Concrete renderers may use a more practical API, but the dependency direction
shall remain renderer depends on `neo_ui`, not `neo_ui` depends on renderer.

### 10.3 SkyEngine Renderer

SkyEngine may keep the current wgpu overlay renderer during migration.

The renderer shall move toward consuming Neo-owned draw output and shall stop
requiring SkyEngine app state in core draw preparation.

## 11. Example Scenarios

### 11.1 Pause Menu

The intended SkyEngine common path shall be:

```rust
neo::compose(ctx.ui(), |ui| {
    ui.modal("pause", |ui| {
        ui.column("panel").padding(24.0).gap(12.0).content(|ui| {
            ui.text("title").content("Paused");

            if ui.button("resume").text("Resume").clicked() {
                commands.resume_game();
            }

            if ui.button("quit").text("Quit").clicked() {
                commands.quit_to_menu();
            }
        });
    });
});
```

Gameplay shall not receive click, escape, enter, or navigation events already
captured by the modal.

### 11.2 Settings Panel

The intended standalone common path shall be:

```rust
let output = neo.frame(frame, |ui| {
    ui.column("settings").padding(24.0).gap(16.0).content(|ui| {
        ui.checkbox("fullscreen", &mut settings.fullscreen).text("Fullscreen");
        ui.slider("master_volume", &mut settings.master_volume)
            .range(0.0..=1.0)
            .label("Master Volume");

        if ui.button("apply").text("Apply").clicked() {
            settings.apply();
        }
    });
});
```

The UI shall remain readable without helper layers or app-specific wrappers.

### 11.3 Inventory Search

Text input in an inventory search box shall not trigger gameplay hotkeys while
the input is focused.

The host shall route committed text to Neo and use `output.capture` to suppress
gameplay interpretation.

## 12. Migration Plan

### 12.1 Phase A: Collapse Planning Documents

This file shall be the single active Neo UI plan.

Older Neo-specific plan files shall be deleted or merged into this file.

### 12.2 Phase B: Introduce Neo-Owned Core Types

Move or introduce:

```text
Color
Vec2
Rect
Screen
Id
Event
Capture
Frame
FrameOutput
DrawList
```

Core modules shall use these types instead of SkyEngine render, app, input, or
UI host types.

### 12.3 Phase C: Add New SkyEngine Compose API

Add:

```rust
neo::compose(ctx.ui(), |ui| {
    /* author UI */
});
```

The existing `neo::compose(ctx, |ui, screen| ...)` API may remain as a temporary
wrapper.

The old API shall be deprecated after examples are migrated.

### 12.4 Phase D: Separate Host Adapter From Core

Modules that may remain SkyEngine-specific:

```text
plugin
backend
input_bridge
window
renderer, until neo_ui_wgpu exists
```

Modules that shall become host-free core:

```text
animation
binding
builder
color
element
event
layout
runtime
text editing model
widgets
draw output
```

Host-free modules shall not import `crate::app`, `crate::ecs`, `crate::gpu`,
`crate::input`, `crate::render`, or `crate::ui`.

### 12.5 Phase E: Internal Crate

Create:

```text
crates/neo-ui
```

SkyEngine shall consume it by path dependency.

The SkyEngine adapter shall re-export the common authoring surface where useful:

```rust
pub use neo_ui::{Binding, Color, FrameOutput, Neo, Response, State, Ui, widgets};
```

### 12.6 Phase F: Optional Renderer Crate

Create only when the core boundary is stable enough:

```text
crates/neo-ui-wgpu
```

This crate should make Neo UI usable by non-SkyEngine users without requiring
them to write a renderer first.

### 12.7 Phase G: Publication Readiness

Publication may be considered only when:

- SkyEngine consumes Neo through the public `neo_ui` API;
- examples use `neo::compose(ctx.ui(), ...)`;
- core tests run without SkyEngine app/render features;
- input capture works in pause menu, settings, inventory search, modal, and IME scenarios;
- the public API has survived at least one real example migration.

## 13. Compatibility

During migration, existing examples should continue to compile unless a break
is deliberate and recorded here.

Old APIs should first become wrappers around new APIs.

Removed APIs shall be removed from examples and docs in the same change.

CamelCase aliases may stay temporarily but shall not appear in new examples.

## 14. Validation

After changing Neo core runtime, widgets, layout, animation, state, or draw
output, run:

```powershell
cargo test --features ui-neo ui::neo
```

After changing public Neo authoring API, run:

```powershell
cargo check --examples --features ui-neo
```

After changing `UiHost`, `FrameContext::ui()`, input routing, capture, or UI
overlay rendering, run:

```powershell
cargo test --features app
cargo check --examples --features app
```

Visual validation shall use SkyEngine's screenshot path for Neo examples rather
than browser screenshots.

## 15. Current Implementation Gap Analysis

This section records the current gap between the implementation under
`src/ui/neo/` and the standalone-library target. It is descriptive, but each
gap should drive migration work.

### 15.1 Core Boundary

The following modules are already close to standalone core shape:

```text
animation
binding
builder
dsl
draw
element
event
layout
runtime
widgets
```

These modules mostly use Neo-owned types and ordinary Rust dependencies.

The following modules are host adapters and shall not move into `neo_ui`:

```text
api
backend
input_bridge
plugin
renderer
window
```

`config` and `color` currently straddle the boundary because they mention
SkyEngine render types. `neo_ui` shall own host-free options and color types;
SkyEngine-specific conversions shall live in `sky_engine::ui::neo`.

### 15.2 Missing Runtime Facade

The current runtime API is centered on:

```rust
NeoRuntime::compose(width, height, |ui, screen| { ... })
NeoRuntime::update_events_and_timers(...)
NeoRuntime::draw_list()
```

This is useful internally, but it is not the standalone API. The missing facade
is:

```rust
Neo::frame(Frame::new(screen).events(events).dt(dt), |ui| { ... })
    -> FrameOutput<R>
```

`NeoRuntime` may remain an internal implementation detail or a compatibility
alias, but ordinary users shall not need to call event update, layout, animation,
capture, and draw-list methods separately.

### 15.3 Input And Capture

The current core event model uses separate pointer, scroll, and keyboard
snapshots. Winit event translation, modifier tracking, IME cursor updates, and
SkyEngine input snapshots are handled in `NeoUiBackend` and `input_bridge`.

Standalone Neo needs a single Neo-owned event stream:

```rust
Event::Pointer(...)
Event::Scroll(...)
Event::Key(...)
Event::Text(...)
Event::Ime(...)
Event::Focus(...)
Event::Navigate(...)
```

The runtime shall compute `Capture` directly and return it through
`FrameOutput`. SkyEngine's `UiCaptureState` shall become an adapter projection
from `neo_ui::Capture`, not the core capture type.

### 15.4 Widget State

Several widgets currently keep internal state in `thread_local!` maps. This
works inside one process and one UI runtime, but it is not a clean standalone
library boundary. Widget state shall move into `Neo` runtime storage keyed by
`Id`.

Acceptable temporary exceptions shall be documented per widget. Final standalone
widgets shall not use process-global state for editable text, picker state,
scroll state, chart interaction state, focus, or animation.

### 15.5 Text, Clipboard, And IME

Text measurement currently depends on `glyphon`, and text input widgets call
`arboard` directly for clipboard operations. A standalone core shall not own
platform clipboard or renderer font state.

Neo core shall define:

```rust
TextMeasurer
Clipboard
ImeRequest
```

as host service boundaries. Default no-op services may exist for tests, but
platform access shall be supplied by adapters.

### 15.6 Rendering And Assets

The current renderer mixes draw-list consumption, wgpu pipelines, glyphon text,
image loading, remote/Bing image helpers, and SkyEngine `GpuContext` upload
helpers.

The split shall be:

```text
neo_ui        owns DrawList, ImageSource identifiers, text/image draw commands
neo_ui_wgpu   owns wgpu pipelines, glyphon integration, texture cache
SkyEngine     owns adapter glue to GpuContext, UiHost, assets, screenshots
```

Remote image loading, Bing image helpers, and file/asset resolution shall not be
hard-coded into element builders. They may exist as optional adapters or example
helpers layered above `ImageSource`.

### 15.7 Authoring Ergonomics

Current widgets support builder APIs, but common widgets still prefer callback
and binding styles in several places. The standalone API shall make direct
mutable state and response-first interaction the easiest path:

```rust
if ui.button("apply").text("Apply").clicked() {
    apply();
}

ui.input("name", &mut profile.name).placeholder("Name");
ui.slider("volume", &mut settings.volume).range(0.0..=1.0);
```

Callbacks and `Binding<T>` remain useful for advanced shared state, but they
shall not be required for ordinary buttons, inputs, sliders, toggles, menus, or
settings panels.

## 16. Target Architecture

### 16.1 Layer Diagram

The target dependency direction shall be:

```text
Application / Game
        |
        v
Host adapter: sky_engine::ui::neo, winit adapter, custom host adapter
        |
        v
Renderer adapter: neo_ui_wgpu or custom renderer
        |
        v
Core library: neo_ui
```

No lower layer shall depend on a higher layer.

### 16.2 `neo_ui` Core

`neo_ui` shall contain:

```text
app-neutral runtime: Neo, Frame, FrameOutput, Options
authoring: Ui, Id, Response, State, Binding, widgets
input: Event, PointerEvent, KeyEvent, TextEvent, ImeEvent, Capture
layout: Screen, Vec2, Rect, Size, EdgeInsets, alignment
visual model: Element, DrawList, draw commands, ImageSource, Color
services: TextMeasurer, Clipboard, ImageResolver hooks
tests: headless runtime, layout, input, focus, capture, widget state
```

`neo_ui` shall not contain `winit`, `wgpu`, SkyEngine, network clients, or
platform clipboard implementations.

### 16.3 `neo_ui_wgpu`

`neo_ui_wgpu` shall contain:

```text
wgpu renderer
glyphon text implementation
texture/image cache
optional built-in image upload helpers
renderer conformance tests where practical
```

This crate consumes `neo_ui::DrawList`. It may define practical target types for
wgpu command encoders and texture views, but it shall not require SkyEngine
`GpuContext`.

### 16.4 SkyEngine Adapter

`sky_engine::ui::neo` shall contain:

```text
NeoUiPlugin
NeoUiBackend
compose(ctx.ui(), |ui| ...)
winit/SkyEngine input translation
UiHost capture projection
SkyEngine GpuContext renderer bridge
optional compatibility wrappers for old examples
```

The adapter shall re-export the common `neo_ui` authoring surface, but it shall
not fork or wrap ordinary widgets unless SkyEngine-specific assets or services
are required.

### 16.5 Host Service Boundary

Neo core shall receive host services through explicit options or frame context,
not through globals:

```rust
pub struct Services<'a> {
    pub text: &'a mut dyn TextMeasurer,
    pub clipboard: Option<&'a mut dyn Clipboard>,
    pub images: Option<&'a mut dyn ImageResolver>,
}
```

The exact lifetime shape may change, but the ownership rule is normative:
platform effects are provided by the host, requested by Neo, and never hidden in
core widget code.

### 16.6 ID And State Ownership

`Id` shall be a first-class type. String literals are accepted at the public
API boundary, but runtime storage shall key by normalized `Id`, not by raw
concatenated strings scattered across widgets.

`Ui::scope` shall be the standard way to build hierarchical IDs. Widget-internal
parts may derive child IDs from a parent `Id`, but those derived IDs shall remain
stable and inspectable for debugging.

All persistent widget state shall live in the `Neo` runtime. This includes text
cursor state, selections, scroll offsets, open picker menus, active popups,
dragging, focus, and animation targets.

## 17. API Lock

Before the crate extraction begins, the project locks these API decisions:

- The standalone root type is `Neo`, not `NeoRuntime`.
- The standalone per-frame call is `Neo::frame(Frame, compose)`.
- The SkyEngine common path is `neo::compose(ctx.ui(), |ui| { ... })`.
- The compose closure receives only `&mut Ui`; screen data is read from
  `ui.screen()`.
- Widgets are response-first. Callback APIs are additive.
- Direct `&mut T` widget state is the common path. `State` and `Binding` are
  optional tools.
- Core input uses Neo-owned events, not `winit` or SkyEngine input types.
- Core rendering output is `DrawList`, not `wgpu` commands or SkyEngine GPU
  resources.
- CamelCase aliases are compatibility only and shall not appear in new examples.

## 18. Decision Record

The project currently decides:

- Neo UI is worth extracting as a standalone-library-shaped product;
- SkyEngine remains the first official host;
- the app-level integration point is `ctx.ui()`;
- the SkyEngine authoring API is `neo::compose(ctx.ui(), |ui| { ... })`;
- `ctx.neo(...)` is rejected for the common path;
- response-first widgets are preferred;
- direct `&mut` state is preferred for simple state;
- `State` and `Binding` are provided for shared state;
- Neo-owned core types are required before crate extraction;
- renderer-neutral draw output is required before a clean standalone core exists.

This decision record shall not be changed by accidental implementation drift.
