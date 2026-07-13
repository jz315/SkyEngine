# Contributing

SkyEngine accepts changes that move the engine toward a stable, production-ready
runtime. Keep patches focused, documented, and covered by the smallest useful
test or example check.

## Development Gates

Run the relevant checks before opening a pull request:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

For render, app, UI, tile, or asset-facing changes, also run the matching
feature checks from `AGENTS.md`. Common examples:

```bash
cargo test --features app
cargo check --examples --features app
cargo test --features ui-legacy
cargo test --manifest-path ../Serein/crates/serein/Cargo.toml
cargo test --manifest-path ../Serein/crates/serein-wgpu/Cargo.toml
cargo check --examples --features ui-serein
```

## API Changes

- Prefer existing public entry points and module boundaries.
- Keep breaking changes deliberate and documented in `CHANGELOG.md`.
- Do not expose low-level internals unless they belong under an existing
  `expert` namespace.
- Add or update examples when public usage changes.

## Release Discipline

- Do not commit profiler artifacts, local scratch output, or temporary files.
- Keep vendored and optional backend changes isolated behind feature gates.
- Update release notes for user-visible behavior, public API, feature flags,
  or packaging changes.
