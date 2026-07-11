# `render/core` Dependency Rules

- `core` provides renderer-family-neutral protocols, execution, graph, GPU helpers,
  resources, scene/view state, and pipeline abstractions.
- Production `core` code must not import `render::features`, `render::integration`,
  a backend, Tilemap, GI, shadows, or Live2D.
- Cross-family data travels through neutral payloads/resources and the crate-private
  `FrameExtension` protocol; do not add feature-specific fields to frame state.
- Keep hot graph, extraction, draw, and runtime paths allocation-conscious.

