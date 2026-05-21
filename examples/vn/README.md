# Visual Novel Examples

This directory contains the Rust entry points for the VN runtime examples.
The runnable story project and image assets live under `examples/assets/vn`.

## Examples

- `vn_minimal`: headless runtime and save-store smoke example.
- `vn_sprite_demo`: app-backed sprite presentation using placeholder colors.
- `vn_ui_demo`: Neo UI galgame slice backed by `examples/assets/vn/project.vn.toml`.

## Commands

```bash
cargo run --example vn_minimal --features vn
cargo run --example vn_sprite_demo --features "vn app"
cargo run --example vn_ui_demo --features vn-ui
```

Local long-form Chinese outline drafts can live under
`examples/assets/vn/_source/`. They are source material, not Yarn scripts, and
are ignored by git. The compiled demo script is
`examples/assets/vn/scripts/after_school_promise.yarn`.
