# eui-neo-winit

`eui-neo-winit` contains optional winit integration helpers for `eui-neo`.

The crate is intentionally narrow: it translates winit keyboard/text input into
`eui_neo::KeyboardEvent` and provides small event utilities. It does not own a
window, renderer, event loop, engine runtime, or clipboard backend. Hosts pass
paste text through `KeyboardOptions` when they have platform clipboard access.
