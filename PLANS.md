# SkyEngine 改进计划

尚未实施的改进项，按优先级分组。

---

## P1 — 近期改进

### Texture Usage 可配置

当前 `Texture` 硬编码 `TEXTURE_BINDING | COPY_DST`，无法用于 compute 写入或作为 render attachment。

**方案**：在 `try_from_rgba8_with_format` 中增加 `usage` 参数（或提供 builder API）。

**触发条件**：需要 compute shader 写入纹理、或纹理作为临时 render target 时。

### Texture / RenderTarget 统一

两个类型字段几乎一致（texture + view + width + height + format），区别仅在于 `RenderTarget` 支持 `resize()` 和 render attachment usage。

**方案**：`RenderTarget` 内部持有 `Texture` + resize 能力，或提取公共 `GpuTexture` trait。

**触发条件**：RenderGraph 资源管理需要统一抽象时。

---

## P2 — 中期改进

### Mipmap 支持

当前所有纹理 `mip_level_count: 1`。对于非像素风格的 3D/2.5D 场景，需要 mipmap 减少远处纹理的 aliasing。

**方案**：
- `TextureBuilder` 增加 `mip_levels(n)` 选项
- 创建后通过 compute shader 或 blit 链生成 mip 数据
- `sampler_linear` 已配置 `mipmap_filter: Linear`，无需改动采样器

### GpuContext 职责拆分

当前 `GpuContext` 同时承担 CGPU 层（device/queue）、RenderDevice 层（default samplers）、帧管理层（surface/swapchain）。对应 SakuraEngine 的三层分离。

**方案**：
```
GpuDevice   — device + queue + samplers（RenderDevice 概念）
FrameContext — surface + swapchain + encoder（帧生命周期）
```

**触发条件**：需要多窗口渲染、多 queue（compute/copy 分离）、或 headless 渲染时。

---

## P3 — 远期改进

### 异步纹理上传

当前 `write_texture` 是同步阻塞的，大纹理上传会卡帧。

**方案**：
- 引入 staging buffer + copy queue 的异步上传管线
- 参考 SakuraEngine 的 `IVRAMService` + DStorage 模式
- 上传完成后通过 fence/callback 通知

**触发条件**：大量或大尺寸纹理的流式加载场景。

### Bindless 纹理

当前每次纹理切换都需要新的 bind group。当纹理数量增长时(100+)，bind group 切换成为瓶颈。

**方案**：
- 使用 texture array 或 bindless descriptor 模式
- SpriteBatch 通过 texture index 而非 bind group 切换纹理
- 需要 wgpu `PARTIALLY_BOUND_BINDING_ARRAY` feature

**触发条件**：大量不同纹理的场景（tilemap、UI atlas 等）。
