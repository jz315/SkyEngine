# eui-neo

`eui-neo` is a Rust-native port and extension of the C++ EUI-NEO UI model.

This crate is the host-agnostic core: declarative builders, retained interaction
state, layout, animation, widgets, and backend-neutral draw-list generation. It
does not depend on SkyEngine, winit, wgpu, or a native windowing stack.

```rust
use eui_neo::prelude::*;

let mut runtime = Runtime::new("hud");
let result = runtime.frame(FrameInput::new(Screen::new(1280.0, 720.0), 1.0 / 60.0), |ui, _| {
    widgets::button(ui, "play").text("Play").build()
});

let frame = result.frame;
```

Platform input, clipboard, image loading, native windows, and GPU rendering
belong in adapter crates or engine integration layers.
