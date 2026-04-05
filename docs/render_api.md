# SkyEngine Render API Reference

## Architecture Overview

```text
Application / AppRunner
    -> GpuContext
        -> active frame ownership
        -> compatibility facade (`with_*`)
        -> frame upload arena (`upload_vertices`, `upload_indices_u16`)
        -> samplers / device / queue
    -> GpuFrame
        -> begin_surface_pass / begin_target_pass
        -> one pass, many state changes and draws
    -> Render Modules
        -> SpriteBatch
        -> Live2DRenderer
        -> LightPass / CompositePass / PostFX
    -> RenderGraph
        -> declarative orchestration / copy passes / resource aliasing
```

`GpuContext` is no longer just a thin wgpu forwarder. It owns the active frame and exposes the mid-layer used by runtime renderers:

- explicit frame recording via `GpuFrame`
- frame-local buffer uploads via `FrameUploadArena`
- reusable dynamic uniform buffers via `DynamicUniformBuffer<T>`

`render::core::camera` also exposes a small view abstraction now:

- `ViewUniform` is the packed GPU view struct
- `CameraUniform` remains as a compatibility alias
- `RenderView` is the pass-facing trait implemented by `Camera2D`

`RenderGraph` still keeps its own copy/upload execution path and submit boundaries. It does not currently reuse the frame upload arena.

---

## GpuContext (`src/gpu/context.rs`)

`GpuContext` owns:

- `wgpu::Device`
- `wgpu::Queue`
- optional presentation surface
- the active frame encoder
- default linear / nearest samplers
- the frame-scoped upload arena

### Construction

| Method | Notes |
|--------|-------|
| `GpuContext::new(window, vsync)` | Panics on init failure |
| `GpuContext::try_new(window, vsync)` | Fallible initialization |
| `GpuContext::new_headless(device, queue, format, size)` | Test-only headless context |

### Accessors

| Method | Returns |
|--------|---------|
| `device()` | `&wgpu::Device` |
| `queue()` | `&wgpu::Queue` |
| `surface_size()` | `[u32; 2]` |
| `surface_format()` | `wgpu::TextureFormat` |
| `sampler_linear()` | `&wgpu::Sampler` |
| `sampler_nearest()` | `&wgpu::Sampler` |
| `has_active_frame()` | `bool` |
| `frame()` | `GpuFrame<'_>` |

### Frame lifecycle

```rust
ctx.begin_frame()?;
// upload + record passes
ctx.end_frame();
```

Notes:

- surface-backed contexts acquire the swapchain image in `begin_frame()`
- headless contexts still allow `begin_frame()` so target-only rendering can use the same frame API
- `end_frame()` always submits the encoder; presentation only happens when a surface exists

### Compatibility facade

Older scoped helpers remain available:

- `with_surface_pass(...)`
- `with_surface_pass_loaded(...)`
- `with_render_pass(...)`
- `with_compute_pass(...)`

These are compatibility wrappers on top of the explicit frame recorder. New render code should prefer `ctx.frame()`.

### `flush()`

`flush(next_encoder_label)` still exists, but it is **not** the normal solution for draw-time buffer hazards.

Use it only when you need an explicit submit boundary, for example:

- render-graph copy / upload passes
- unusual workflows that must split one logical frame across multiple queue submits

Normal draw paths should use unique offsets / sub-allocation instead.

---

## GpuFrame and Pass Recording

`GpuFrame<'_>` is the explicit frame recorder returned by `GpuContext::frame()`.

### Render passes

| Method | Purpose |
|--------|---------|
| `begin_surface_pass(label, clear)` | Draw to the current surface |
| `begin_surface_pass_loaded(label)` | Draw to the current surface with `LoadOp::Load` |
| `begin_target_pass(label, target, load)` | Draw to an off-screen target |
| `begin_target_pass_loaded(label, target)` | Off-screen target with `LoadOp::Load` |
| `begin_render_pass(desc)` | Raw descriptor path |
| `begin_compute_pass(desc)` | Raw compute pass path |

Returned types:

- `GpuRenderPass<'_>`
- `GpuComputePass<'_>`

These wrap the corresponding wgpu pass objects and `DerefMut` to them, so code inside a pass still uses the normal wgpu API:

```rust
let mut frame = ctx.frame();
let mut pass = frame.begin_surface_pass_loaded("sprites");
pass.set_pipeline(&pipeline);
pass.set_bind_group(0, &bind_group, &[]);
pass.draw_indexed(0..6, 0, 0..1);
```

This is the intended fix for cases like Live2D where many drawables need to share one render pass while changing pipeline / bind groups / buffers between draws.

---

## Frame Upload Arena

`FrameUploadArena` is the engine-provided transient upload facility used by `GpuContext`.

Public types:

- `FrameUploadArena`
- `UploadSlice`

Typical usage goes through `GpuContext`:

```rust
let vertex_upload = ctx.upload_vertices(&vertices);
let index_upload = ctx.upload_indices_u16(indices);
```

Then bind directly:

```rust
pass.set_vertex_buffer(0, vertex_upload.slice());
pass.set_index_buffer(index_upload.slice(), wgpu::IndexFormat::Uint16);
```

### Behavior

- vertex and index uploads use separate streams
- index uploads pad the underlying `queue.write_buffer()` to satisfy copy alignment
- `UploadSlice` stores `buffer + offset + size`
- frame reset only resets the active cursors; buffers are reused across frames
- buffers grow automatically when a stream runs out of room

This replaces renderer-local solutions like Live2D’s former `FrameUploadCursor`.

---

## DynamicUniformBuffer<T>

`DynamicUniformBuffer<T>` is the reusable dynamic-offset uniform helper.

What it handles:

- `min_uniform_buffer_offset_alignment`
- per-frame offset calculation
- buffer growth
- bind group rebuild after reallocation

Core API:

| Method | Purpose |
|--------|---------|
| `DynamicUniformBuffer::new(ctx, label, visibility)` | Create buffer + BGL + BG |
| `clear()` | Reset logical contents for a new frame |
| `push(ctx, value)` | Upload one value and return its dynamic offset |
| `bind_group_layout()` | Reuse in pipeline layout creation |
| `bind_group()` | Bind for draw calls |
| `stride()` | Aligned per-element stride |

Typical usage:

```rust
uniforms.clear();
let offset = uniforms.push(ctx, my_uniforms);

pass.set_bind_group(0, uniforms.bind_group(), &[offset]);
```

This is the intended engine-level solution for per-draw uniforms. Renderers should not hand-roll stride/alignment logic.

---

## Texture API (`src/render/core/texture.rs`)

`Texture` remains an `Arc`-backed texture + default view wrapper, but creation is now descriptor-first.

Additional low-level entry point:

- `TextureCreateDesc` for creating empty/custom textures with explicit usage, dimension, mip count, and sample count

### Explicit descriptors

- `TextureUploadDesc<'a>` for raw RGBA8 uploads
- `TextureFileDesc<'a>` for file loads

Examples:

```rust
let color_tex = Texture::from_upload_desc(
    &ctx,
    TextureUploadDesc::new(w, h, pixels),
);

let linear_tex = Texture::from_upload_desc(
    &ctx,
    TextureUploadDesc::new(w, h, pixels)
        .format(wgpu::TextureFormat::Rgba8Unorm)
        .label("normal_map"),
);
```

```rust
let tex = Texture::from_file_desc(
    &ctx,
    TextureFileDesc::new(path)
        .format(wgpu::TextureFormat::Rgba8Unorm),
);
```

### Convenience wrappers

These are still kept for compatibility:

- `from_rgba8(...)`
- `from_rgba8_with_label(...)`
- `from_rgba8_with_format(...)`
- `from_png(...)`
- `try_from_png(...)`

Defaults:

- `from_rgba8` / `from_png` still mean **sRGB color texture**
- non-color data must opt into an explicit format

This keeps old call sites stable while removing the assumption that every uploaded texture is sRGB.

---

## RenderTarget (`src/render/core/target.rs`)

`RenderTarget` is the persistent off-screen color target wrapper.

Additional low-level entry point:

- `RenderTargetDescriptor` for non-default sample/mip configuration

Key methods:

- `RenderTarget::new(...)`
- `resize(...)`
- `view()`
- `texture()`
- `width() / height() / format()`

`RenderTarget` implements the `gpu::ColorTargetView` trait, so it can be passed directly to `GpuFrame::begin_target_pass(...)`.

---

## SpriteBatch (`src/render/passes/batch.rs`)

`SpriteBatch` still uses persistent quad / instance / camera buffers and its own draw-plan batching.

What changed:

- pass recording now uses `GpuFrame`
- one batch still records as one pass
- batching behavior and persistent instance-buffer strategy are unchanged

Typical flow:

```rust
batch.set_texture(&texture);
batch.draw(Sprite::new(x, y, w, h));
batch.clear_texture(); // optional: return to solid-color mode
batch.flush_to_surface(&mut ctx, &camera, Some(clear));
```

---

## Mesh / MeshPass (`src/render/resources/mesh.rs`, `src/render/passes/mesh_pass.rs`)

`Mesh` is the persistent custom-geometry counterpart to `SpriteBatch`'s internal quad buffers.

- `Mesh::from_vertices(...)` for non-indexed geometry
- `Mesh::from_vertices_indices(...)` for indexed geometry via `MeshIndexData::U16` / `MeshIndexData::U32`
- vertex and index buffers are created with `COPY_DST`, so the resource shape is future-proof for dynamic updates

`MeshPass` is the runtime renderer for those meshes:

- shader contract reserves bind group `0` for the shared `ViewUniform`
- `create_pipeline_cache(...)` builds a `MaterialPipelineCache` with that reserved view slot
- `MeshDraw` configures the mesh, optional material, draw ranges, base vertex, and instance range
- `render_to_target_with_depth(...)` validates color/depth sample-count and format compatibility before encoding draw calls
- `render_to_surface(...)` currently supports single-sample, depthless surface rendering

Typical flow:

```rust
let mesh = Mesh::from_vertices_indices(&ctx, &vertices, MeshIndexData::U16(&indices), "tri");
let mut mesh_pass = MeshPass::new(&ctx);
let mut pipeline = mesh_pass.create_pipeline_cache(&ctx, desc, None, None)?;
let mut draws = [MeshDraw::new(&mesh, &mut pipeline)];

mesh_pass.render_to_target(&mut ctx, &target, &camera, Some(Color::BLACK), &mut draws)?;
```

---

## Live2D (`src/render/live2d/`)

Live2D is the first renderer fully migrated to the new mid-layer.

Current design:

- per-draw geometry uploads use `ctx.upload_vertices(...)` and `ctx.upload_indices_u16(...)`
- per-draw uniforms use `DynamicUniformBuffer<Live2DUniforms>`
- mask drawables are prepared first, then rendered in a single mask pass
- model drawables are prepared next, then rendered in a single model pass
- surface and target paths each open at most one model pass per frame

This fixes both major issues:

- shared-buffer overwrite from repeated offset-0 writes
- excessive pass churn from opening one render pass per drawable

Important notes:

- Live2D textures load as `Rgba8Unorm` (linear), not the default sRGB path
- `LIVE2D_CUBISM_SDK_NATIVE_DIR` must be set at build time

---

## RenderGraph boundary

`RenderGraph` remains the declarative orchestration layer for:

- pass dependency analysis
- execution ordering
- physical resource allocation / aliasing
- copy and upload passes

It still owns its own submit-boundary rules and may call `ctx.flush(...)` around copy/upload work. That is intentional and separate from runtime renderer uploads.

For full render-graph details, see:

- [src/render/graph/AGENTS.md](C:/Coding/SkyEngine/src/render/graph/AGENTS.md)

---

## Recommended usage

- Use `GpuFrame` for new runtime renderers that need one pass with many draw/state changes.
- Use `ctx.upload_vertices(...)` / `ctx.upload_indices_u16(...)` for transient per-frame geometry.
- Use `DynamicUniformBuffer<T>` for per-draw uniforms with dynamic offsets.
- Use `TextureUploadDesc` / `TextureFileDesc` whenever format matters.
- Do not use `flush()` as a substitute for sub-allocation in normal draw code.
