# `render/features` Dependency Rules

- A feature owns its ECS components, extraction, GPU preparation/cache, passes,
  shaders, and focused regression tests.
- Features may depend on `render::core`; they must not depend on `integration` or
  make a feature-specific requirement part of a core frame/runtime type.
- Use `FrameExtension` for family-wide frame work (for example shadows and GI),
  and `RenderFeature` for draw/extractor/pipeline registration.
- Keep render-only Tiled data in `features/tilemap`; editable tile-scene behavior
  belongs in `src/tile`.

