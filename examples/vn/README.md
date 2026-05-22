# Visual Novel Examples

This directory contains the Rust entry points for the VN runtime examples.
The runnable story project and image assets live under
`examples/assets/vn/after_school_promise`.

## Examples

- `vn_runtime_minimal`: headless runtime and save-store smoke example.
- `vn_sprite_presentation`: app-backed sprite presentation using placeholder colors.
- `vn_after_school_promise`: Neo UI galgame slice backed by `examples/assets/vn/after_school_promise/project.vn.toml`.

## Commands

```bash
cargo run --example vn_runtime_minimal --features vn
cargo run --example vn_sprite_presentation --features "vn app"
cargo run --example vn_after_school_promise --features vn-ui
```

Local long-form Chinese outline drafts can live under
`examples/assets/vn/after_school_promise/_source/`. They are source material,
not Yarn scripts, and are ignored by git. The compiled demo script is
`examples/assets/vn/after_school_promise/scripts/after_school_promise.yarn`.
