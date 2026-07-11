# `render/integration` Dependency Rules

- Integration composes the public rendering experience: app-facing backends,
  `SceneSnapshot`, CPU asset bridges, and pipeline presets.
- It may depend on `features` and `core`, but must not host renderer-family
  implementation, per-frame rendering state, or a second execution runtime.
- `presets` assemble existing features and passes only; concrete pass behavior
  stays in its feature directory.

