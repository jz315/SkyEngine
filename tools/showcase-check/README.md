# Local showcase compile coverage

The root package intentionally keeps `autoexamples = false` so only polished,
published examples appear in its Cargo surface. This package registers every
remaining runnable showcase as a non-publishable binary, preventing local game,
renderer, and UI demos from silently falling out of compile coverage.

```bash
cargo check --manifest-path tools/showcase-check/Cargo.toml --all-targets --all-features
```

Individual feature families can be checked faster, for example:

```bash
cargo check --manifest-path tools/showcase-check/Cargo.toml --all-targets --features app
cargo check --manifest-path tools/showcase-check/Cargo.toml --all-targets --features ui-neo
```
