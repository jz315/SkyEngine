# Sakura RenderGraph Cache Audit

Date: 2026-05-27

This audit supports `docs/plan/sakura_render_graph_adaptation_plan.md`
Milestone 6.  It records the current SkyEngine facts before adding any
Sakura-style texture-view, buffer-view, bind-table, or merged bind-table cache.

## Commands

Static call-site counts were collected with:

```text
rg "texture_subresource_view\(" src/render -g "*.rs"
rg "storage_texture_view\(" src/render -g "*.rs"
rg "render_attachment_view\(" src/render -g "*.rs"
rg "create_bind_group\(" src/render -g "*.rs"
```

Observed counts in this workspace:

- `texture_subresource_view(`: 17 matches, including the graph method, tests,
  `src/render/builtins/debug.rs`, and SSGI executor usage.
- `storage_texture_view(`: 9 matches, including the graph method, tests, and
  SSGI executor usage.
- `render_attachment_view(`: 2 matches, the graph helper and one graph test.
- `create_bind_group(`: 63 matches under `src/render`.

These are static source counts, not frame-time measurements.

## RenderGraph View Facts

- `RenderTarget` owns and reuses its default `wgpu::TextureView`.
- `PhysicalResources::view` and `PhysicalResources::texture_view` return the
  existing default view for graph textures.
- `PhysicalResources::texture_subresource_view` creates a new
  `wgpu::TextureView` for explicit mip/layer ranges.
- `PhysicalResources::storage_texture_view` and
  `PhysicalResources::render_attachment_view` are wrappers over
  `texture_subresource_view`.
- Current graph-level subresource view users are concentrated in
  `src/render/gi/providers/ssgi/executor.rs` and
  `src/render/builtins/debug.rs`.
- `RenderGraph::physical_resource_view_stats()` records the latest execution's
  default view resolves and subresource/storage/render-attachment view
  creations.  This is a measurement hook, not a cache.

## Bind Group Facts

- Bind group creation is outside the core `RenderGraph`.
- Repeated bind group creation exists in feature/runtime owners such as post-fx,
  SSGI, DDGI, material preparation, sprite/tile draw paths, presentation, and
  Live2D renderer code.
- `RenderGraph` does not own material interfaces, renderer-family bind layouts,
  or pass-local resource binding policy.

## Decision

Do not add a graph-level bind-table or merged bind-table cache now.

Do not add a core `RenderGraph` view cache now.  The current evidence identifies
possible call sites and adds per-execution view counters, but it does not prove
a measured frame-time or allocation hot spot in a real workload.

If a future profile shows repeated subresource view creation is measurable, the
lowest-coupling candidate is a small owner-local cache keyed by texture identity
and view descriptor.  Likely owners are `RenderTarget` or a renderer-family cache
that already owns the target lifecycle.  Any such cache must invalidate on
resize, format, usage, sample count, mip count, or array layer count changes.

If a future profile shows bind group creation is measurable, keep caching near
the concrete owner that knows the layout and resource tuple, such as material
pipeline caches, post-fx passes, GI provider state, sprite/tile renderers, or
Live2D renderer state.

## Open Measurement Work

No runtime allocation profile or frame-time profile was captured for a real
SkyEngine render workload in this audit.  That means this audit identifies no
measured hot spot today.  It records where the current source can create views
and bind groups, adds a graph-level view counter for future workload runs, and
blocks copying Sakura's bind-table pools into SkyEngine's core graph without
workload evidence.
