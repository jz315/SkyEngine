# Release Process

SkyEngine releases must pass a clean quality gate before tagging. A release can
be GitHub-only or crates.io-backed; crates.io releases require the additional
packaging checks below.

## Required Gates

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features app
cargo check --examples --features app
```

Add focused checks for any feature touched by the release:

```bash
cargo test --features ui-legacy
cargo test --features yakui-ui
cargo test --features vn
cargo test --features app tile::
cargo test --manifest-path crates/eui-neo/Cargo.toml
cargo test --manifest-path crates/eui-neo-wgpu/Cargo.toml
cargo check --examples --features ui-neo
```

## Release Checklist

- Working tree is clean except for intentional release edits.
- `CHANGELOG.md` has a dated entry for the release.
- Public examples compile for changed APIs.
- Benchmark-sensitive ECS or render changes include a benchmark note.
- Optional native integrations are either verified or clearly excluded from the
  release notes.
- Version numbers are bumped consistently across crates that changed.
- GitHub release notes state supported feature flags and known limitations.

## crates.io Checklist

Before publishing to crates.io:

```bash
cargo publish --dry-run
```

Every published path dependency must have a `version` requirement and be
published in dependency order. Optional vendor-only backends that cannot be
resolved from crates.io must stay out of published packages or be split into
separate non-published integration crates.

Publish internal crates from leaf to root, for example:

```text
sky_type
sky_reflect_derive
sky_reflect
sky_profile
sky_ecs
sky_math
eui-neo
eui-neo-wgpu
eui-neo-winit
sky_engine
```

Only tag after the dry run succeeds.
