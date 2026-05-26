# SkyEngine GPU

`sky_engine::gpu` 是 `wgpu` 的引擎封装层，在 `app` feature 下启用。它不是完整渲染器，而是负责 GPU 初始化、surface/frame 生命周期、frame-local upload、dynamic uniform buffer 和低层 pass recording。

```rust
use sky_engine::gpu::{
    DynamicUniformBuffer, FrameUploadArena, GpuComputePass, GpuContext, GpuError, GpuFrame,
    GpuInitError, GpuRenderPass, UploadSlice,
};
```

## GpuContext

`GpuContext` 拥有：

- `wgpu::Device`
- `wgpu::Queue`
- optional `wgpu::Surface`
- active frame encoder
- surface texture/view
- default samplers
- frame upload arena

构造：

```rust,no_run
GpuContext::new(window, vsync) -> GpuContext
GpuContext::try_new(window, vsync) -> Result<GpuContext, GpuInitError>
GpuContext::new_headless(device, queue, format, size) -> GpuContext
```

App runner 通常会替你创建 `GpuContext`。直接使用只适合测试、工具或自定义 runner。

## Accessors

```rust,no_run
gpu.device() -> &wgpu::Device
gpu.queue() -> &wgpu::Queue
gpu.surface_size() -> [u32; 2]
gpu.surface_format() -> wgpu::TextureFormat
gpu.has_surface() -> bool
gpu.adapter_name() -> &str
gpu.backend_name() -> &str
gpu.sampler_linear() -> &wgpu::Sampler
gpu.sampler_nearest() -> &wgpu::Sampler
```

## Frame Lifecycle

基本顺序：

```rust,no_run
gpu.begin_frame()?;

{
    let mut frame = gpu.frame();
    let mut pass = frame.begin_surface_pass_loaded("main");
    // record draw calls
}

gpu.end_frame();
# Ok::<(), sky_engine::gpu::GpuError>(())
```

规则：

- surface-backed context 在 `begin_frame` 获取 swapchain texture。
- headless context 也可以 `begin_frame`，用于 offscreen/test。
- `end_frame` 提交 command encoder；只有有 surface 时才 present。
- active frame 期间可以通过 `gpu.frame()` 获取 `GpuFrame`。

## GpuFrame

`GpuFrame` 是当前 frame 的显式 recorder。

Render pass：

```rust,no_run
frame.begin_surface_pass(label, clear)
frame.begin_surface_pass_loaded(label)
frame.begin_target_pass(label, target, load)
frame.begin_target_pass_loaded(label, target)
frame.begin_render_pass(desc)
```

Compute pass：

```rust,no_run
frame.begin_compute_pass(desc)
```

返回：

- `GpuRenderPass<'_>`
- `GpuComputePass<'_>`

这些类型包装 wgpu pass，并可继续调用 wgpu 的 `set_pipeline`、`set_bind_group`、`draw` 等方法。

## Frame Upload Arena

`FrameUploadArena` 提供 frame-local vertex/index upload。通常通过 `GpuContext` 或 `GpuFrame` 使用：

```rust,no_run
let vertices = gpu.upload_vertices(&vertex_data);
let indices = gpu.upload_indices_u16(&index_data);

let mut frame = gpu.frame();
let mut pass = frame.begin_surface_pass_loaded("draw");
pass.set_vertex_buffer(0, vertices.slice());
pass.set_index_buffer(indices.slice(), wgpu::IndexFormat::Uint16);
```

`UploadSlice` 可读：

```rust,no_run
slice.buffer()
slice.offset()
slice.size()
slice.range()
slice.slice()
```

行为：

- vertex 和 index 有独立 stream。
- buffer 不够会增长。
- frame reset 只重置 cursor，尽量复用 buffer。
- index upload 会处理底层写入对齐。

## DynamicUniformBuffer

`DynamicUniformBuffer<T>` 处理 uniform buffer alignment、动态 offset、buffer growth 和 bind group rebuild。

```rust,no_run
let mut uniforms = DynamicUniformBuffer::<ViewUniform>::new(
    gpu,
    "view_uniforms",
    wgpu::ShaderStages::VERTEX,
);

uniforms.clear();
let offset = uniforms.push(gpu, view_uniform);

pass.set_bind_group(0, uniforms.bind_group(), &[offset]);
```

常用方法：

```rust,no_run
uniforms.clear()
uniforms.push(ctx, value) -> u32
uniforms.push_staged(ctx, value) -> u32
uniforms.upload_all(ctx)
uniforms.bind_group_layout()
uniforms.bind_group()
uniforms.buffer()
uniforms.stride()
```

推荐 renderers 使用它，而不是各自手写 uniform alignment。

## flush

```rust,no_run
gpu.flush("next_encoder_label");
```

`flush` 会提交当前 encoder 并创建新 encoder。它不是普通 draw hazard 的默认解决方案。

适合：

- render graph copy/upload pass 需要 submit boundary。
- 特殊 workflow 必须把一个逻辑 frame 分成多个 queue submit。

普通 draw path 应通过不同 offset/suballocation 避免 hazard。

## Headless

`GpuContext::new_headless` 用于测试 render graph、resource allocation、offscreen pass 等无需 window 的逻辑。

```rust,no_run
let gpu = GpuContext::new_headless(device, queue, format, [640, 480]);
```

## 和 Render 的关系

- `gpu` 管 frame/device/queue/surface。
- `render` 管 sprite、mesh、phase、render graph、高层 pipeline。
- 应用通常不直接写 wgpu pass，除非在做 custom renderer 或低层工具。

## 测试和检查

```bash
cargo test --features app gpu
cargo test --features app graph
cargo check --features app
```
