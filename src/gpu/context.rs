//! Thin wgpu context — frame lifecycle, surface management, convenience helpers.
//!
//! `GpuContext` owns the device/queue/surface plus the active frame encoder.
//! It also provides a small mid-layer for frame-scoped render-pass recording
//! and transient uploads.

use std::borrow::Cow;
use std::num::NonZeroU64;
use std::ops::{Deref, DerefMut, Range};
use std::sync::Arc;

const INITIAL_VERTEX_UPLOAD_BYTES: u64 = 256 * 1024;
const INITIAL_INDEX_UPLOAD_BYTES: u64 = 128 * 1024;
const INITIAL_DYNAMIC_UNIFORM_CAPACITY: u64 = 64;

/// GPU initialization errors.
#[derive(Debug)]
pub enum GpuInitError {
    SurfaceCreation(String),
    AdapterUnavailable,
    DeviceCreation(String),
}

impl std::fmt::Display for GpuInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurfaceCreation(msg) => write!(f, "Failed to create surface: {msg}"),
            Self::AdapterUnavailable => write!(f, "No suitable GPU adapter found"),
            Self::DeviceCreation(msg) => write!(f, "Failed to create GPU device: {msg}"),
        }
    }
}

impl std::error::Error for GpuInitError {}

/// Surface / frame errors.
#[derive(Debug)]
pub enum GpuError {
    SurfaceLost,
    OutOfMemory,
    Other(String),
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurfaceLost => write!(f, "Surface lost"),
            Self::OutOfMemory => write!(f, "Out of GPU memory"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for GpuError {}

/// Helper trait for any color attachment target that can provide a texture view.
pub trait ColorTargetView {
    fn color_target_view(&self) -> &wgpu::TextureView;
}

impl ColorTargetView for wgpu::TextureView {
    #[inline]
    fn color_target_view(&self) -> &wgpu::TextureView {
        self
    }
}

/// A frame-scoped upload result pointing at a sub-range of a GPU buffer.
#[derive(Clone)]
pub struct UploadSlice {
    buffer: wgpu::Buffer,
    offset: u64,
    size: u64,
}

impl UploadSlice {
    #[inline]
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    #[inline]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    #[inline]
    pub fn size(&self) -> u64 {
        self.size
    }

    #[inline]
    pub fn range(&self) -> Range<u64> {
        self.offset..self.offset + self.size
    }

    #[inline]
    pub fn slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(self.range())
    }
}

#[derive(Debug)]
struct UploadBuffer {
    label: &'static str,
    usage: wgpu::BufferUsages,
    initial_size: u64,
    current: Option<wgpu::Buffer>,
    current_size: u64,
    cursor: u64,
}

impl UploadBuffer {
    fn new(label: &'static str, usage: wgpu::BufferUsages, initial_size: u64) -> Self {
        Self {
            label,
            usage,
            initial_size,
            current: None,
            current_size: 0,
            cursor: 0,
        }
    }

    fn reset(&mut self) {
        self.cursor = 0;
    }

    fn allocate(
        &mut self,
        device: &wgpu::Device,
        required_bytes: u64,
        alignment: u64,
    ) -> (wgpu::Buffer, u64) {
        let aligned_offset = align_up(self.cursor, alignment);
        let current_required = aligned_offset + required_bytes;
        if self.current.is_none() || current_required > self.current_size {
            let new_size = grow_buffer_size(
                self.current_size.max(self.initial_size),
                required_bytes.max(self.initial_size),
            );
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: new_size,
                usage: self.usage,
                mapped_at_creation: false,
            });
            self.current = Some(buffer);
            self.current_size = new_size;
            self.cursor = 0;
        }

        let offset = align_up(self.cursor, alignment);
        self.cursor = offset + required_bytes;
        (
            self.current
                .as_ref()
                .expect("upload buffer should exist")
                .clone(),
            offset,
        )
    }
}

/// Frame-scoped transient upload allocator used for per-draw buffer data.
#[derive(Debug)]
pub struct FrameUploadArena {
    vertex: UploadBuffer,
    index: UploadBuffer,
    index_scratch: Vec<u8>,
}

impl Default for FrameUploadArena {
    fn default() -> Self {
        Self {
            vertex: UploadBuffer::new(
                "frame_upload_vertex",
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                INITIAL_VERTEX_UPLOAD_BYTES,
            ),
            index: UploadBuffer::new(
                "frame_upload_index",
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                INITIAL_INDEX_UPLOAD_BYTES,
            ),
            index_scratch: Vec::new(),
        }
    }
}

impl FrameUploadArena {
    /// Reset the frame allocator so the active upload buffers can be reused.
    pub fn reset(&mut self) {
        self.vertex.reset();
        self.index.reset();
    }

    /// Upload vertex data and return a slice suitable for `set_vertex_buffer`.
    pub fn write_vertices<T: bytemuck::Pod>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &[T],
    ) -> UploadSlice {
        let bytes = bytemuck::cast_slice(data);
        self.write_vertex_bytes(device, queue, bytes)
    }

    /// Upload raw vertex bytes and return a slice suitable for `set_vertex_buffer`.
    pub fn write_vertex_bytes(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
    ) -> UploadSlice {
        let logical_size = bytes.len() as u64;
        let (buffer, offset) = self.vertex.allocate(device, logical_size.max(1), 1);
        if !bytes.is_empty() {
            queue.write_buffer(&buffer, offset, bytes);
        }
        UploadSlice {
            buffer,
            offset,
            size: logical_size,
        }
    }

    /// Upload `u16` index data, padding the write to satisfy copy alignment.
    pub fn write_indices_u16(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        indices: &[u16],
    ) -> UploadSlice {
        let bytes = bytemuck::cast_slice::<u16, u8>(indices);
        let logical_size = bytes.len() as u64;
        let padded_size = align_up(logical_size.max(1), wgpu::COPY_BUFFER_ALIGNMENT);
        let (buffer, offset) =
            self.index
                .allocate(device, padded_size, wgpu::COPY_BUFFER_ALIGNMENT);

        if !bytes.is_empty() {
            self.index_scratch.clear();
            self.index_scratch.extend_from_slice(bytes);
            self.index_scratch.resize(padded_size as usize, 0);
            queue.write_buffer(&buffer, offset, &self.index_scratch);
        }

        UploadSlice {
            buffer,
            offset,
            size: logical_size,
        }
    }
}

/// A reusable dynamic uniform buffer with automatic stride alignment.
pub struct DynamicUniformBuffer<T: bytemuck::Pod> {
    label: Cow<'static, str>,
    buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    stride: u64,
    capacity: u64,
    values: Vec<T>,
}

impl<T: bytemuck::Pod> DynamicUniformBuffer<T> {
    pub fn new(
        ctx: &GpuContext,
        label: impl Into<Cow<'static, str>>,
        visibility: wgpu::ShaderStages,
    ) -> Self {
        let label = label.into();
        let min_binding_size =
            NonZeroU64::new(std::mem::size_of::<T>() as u64).expect("uniform T must be non-empty");
        let stride = align_up(
            std::mem::size_of::<T>() as u64,
            ctx.device().limits().min_uniform_buffer_offset_alignment as u64,
        );
        let capacity = INITIAL_DYNAMIC_UNIFORM_CAPACITY;
        let buffer = Self::create_buffer(ctx.device(), label.as_ref(), stride * capacity);
        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some(&format!("{}_bgl", label)),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: Some(min_binding_size),
                        },
                        count: None,
                    }],
                });
        let bind_group = Self::create_bind_group(
            ctx.device(),
            label.as_ref(),
            &bind_group_layout,
            &buffer,
            min_binding_size,
        );

        Self {
            label,
            buffer,
            bind_group_layout,
            bind_group,
            stride,
            capacity,
            values: Vec::with_capacity(capacity as usize),
        }
    }

    fn create_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn create_bind_group(
        device: &wgpu::Device,
        label: &str,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
        min_binding_size: NonZeroU64,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("{}_bg", label)),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: 0,
                    size: Some(min_binding_size),
                }),
            }],
        })
    }

    pub fn clear(&mut self) {
        self.values.clear();
    }

    pub fn push(&mut self, ctx: &GpuContext, value: T) -> u32 {
        let index = self.values.len() as u64;
        self.values.push(value);

        let grew = self.ensure_capacity(ctx, self.values.len() as u64);
        let offset = index * self.stride;
        if grew {
            self.reupload_all(ctx);
        } else {
            ctx.queue()
                .write_buffer(&self.buffer, offset, bytemuck::bytes_of(&value));
        }

        u32::try_from(offset).expect("dynamic uniform offset exceeds u32::MAX")
    }

    fn ensure_capacity(&mut self, ctx: &GpuContext, required: u64) -> bool {
        if required <= self.capacity {
            return false;
        }

        self.capacity = grow_buffer_size(self.capacity.max(1), required);
        self.buffer = Self::create_buffer(
            ctx.device(),
            self.label.as_ref(),
            self.stride * self.capacity,
        );
        let min_binding_size =
            NonZeroU64::new(std::mem::size_of::<T>() as u64).expect("uniform T must be non-empty");
        self.bind_group = Self::create_bind_group(
            ctx.device(),
            self.label.as_ref(),
            &self.bind_group_layout,
            &self.buffer,
            min_binding_size,
        );
        true
    }

    fn reupload_all(&self, ctx: &GpuContext) {
        for (index, value) in self.values.iter().enumerate() {
            let offset = index as u64 * self.stride;
            ctx.queue()
                .write_buffer(&self.buffer, offset, bytemuck::bytes_of(value));
        }
    }

    #[inline]
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    #[inline]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    #[inline]
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    #[inline]
    pub fn stride(&self) -> u64 {
        self.stride
    }
}

/// Per-frame state (optional surface texture + encoder + upload arena).
struct FrameState {
    surface_texture: Option<wgpu::SurfaceTexture>,
    surface_view: Option<wgpu::TextureView>,
    encoder: wgpu::CommandEncoder,
}

/// Lightweight wrapper around wgpu device, queue, and optional surface.
///
/// Owns the frame lifecycle (`begin_frame` / `end_frame`) and surface
/// configuration. All other GPU operations go through the publicly-exposed
/// device/queue, or through frame-scoped helpers such as [`GpuFrame`].
pub struct GpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: wgpu::SurfaceConfiguration,
    adapter_name: String,
    backend_name: String,
    frame: Option<FrameState>,
    uploads: FrameUploadArena,
    sampler_linear: wgpu::Sampler,
    sampler_nearest: wgpu::Sampler,
}

/// Explicit frame recorder backed by an active [`GpuContext`] frame.
pub struct GpuFrame<'a> {
    ctx: &'a mut GpuContext,
}

/// Wrapper around `wgpu::RenderPass` that keeps the API close to raw wgpu.
pub struct GpuRenderPass<'a> {
    inner: wgpu::RenderPass<'a>,
}

impl<'a> Deref for GpuRenderPass<'a> {
    type Target = wgpu::RenderPass<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> DerefMut for GpuRenderPass<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Wrapper around `wgpu::ComputePass`.
pub struct GpuComputePass<'a> {
    inner: wgpu::ComputePass<'a>,
}

impl<'a> Deref for GpuComputePass<'a> {
    type Target = wgpu::ComputePass<'a>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a> DerefMut for GpuComputePass<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<'a> GpuFrame<'a> {
    fn frame_state(&mut self) -> &mut FrameState {
        self.ctx
            .frame
            .as_mut()
            .expect("GpuFrame requires an active frame")
    }

    pub fn begin_surface_pass<'b>(
        &'b mut self,
        label: &str,
        clear: Option<wgpu::Color>,
    ) -> GpuRenderPass<'b> {
        let load = match clear {
            Some(color) => wgpu::LoadOp::Clear(color),
            None => wgpu::LoadOp::Load,
        };
        let frame = self.frame_state();
        let view = frame
            .surface_view
            .as_ref()
            .expect("begin_surface_pass requires a surface-backed frame");
        let pass = frame
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
        GpuRenderPass { inner: pass }
    }

    pub fn begin_surface_pass_loaded<'b>(&'b mut self, label: &str) -> GpuRenderPass<'b> {
        self.begin_surface_pass(label, None)
    }

    pub fn begin_target_pass<'b, T: ColorTargetView + ?Sized>(
        &'b mut self,
        label: &str,
        target: &T,
        load: wgpu::LoadOp<wgpu::Color>,
    ) -> GpuRenderPass<'b> {
        let pass = self
            .frame_state()
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.color_target_view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
        GpuRenderPass { inner: pass }
    }

    pub fn begin_target_pass_loaded<'b, T: ColorTargetView + ?Sized>(
        &'b mut self,
        label: &str,
        target: &T,
    ) -> GpuRenderPass<'b> {
        self.begin_target_pass(label, target, wgpu::LoadOp::Load)
    }

    pub fn begin_render_pass<'b>(
        &'b mut self,
        desc: &wgpu::RenderPassDescriptor<'b>,
    ) -> GpuRenderPass<'b> {
        let pass = self.frame_state().encoder.begin_render_pass(desc);
        GpuRenderPass { inner: pass }
    }

    pub fn begin_compute_pass<'b>(
        &'b mut self,
        desc: &wgpu::ComputePassDescriptor<'b>,
    ) -> GpuComputePass<'b> {
        let pass = self.frame_state().encoder.begin_compute_pass(desc);
        GpuComputePass { inner: pass }
    }

    #[inline]
    pub fn upload_vertices<T: bytemuck::Pod>(&mut self, data: &[T]) -> UploadSlice {
        self.ctx.upload_vertices(data)
    }

    #[inline]
    pub fn upload_indices_u16(&mut self, indices: &[u16]) -> UploadSlice {
        self.ctx.upload_indices_u16(indices)
    }
}

impl GpuContext {
    /// Create a new GPU context attached to the given window.
    ///
    /// Blocks on adapter/device creation via `pollster`.
    pub fn new(window: Arc<winit::window::Window>, vsync: bool) -> Self {
        Self::try_new(window, vsync).expect("GpuContext::new failed")
    }

    /// Create a new GPU context attached to the given window.
    ///
    /// Blocks on adapter/device creation via `pollster`.
    pub fn try_new(window: Arc<winit::window::Window>, vsync: bool) -> Result<Self, GpuInitError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::METAL | wgpu::Backends::DX12,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| GpuInitError::SurfaceCreation(e.to_string()))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or(GpuInitError::AdapterUnavailable)?;

        let adapter_info = adapter.get_info();
        let adapter_name = adapter_info.name.clone();
        let backend_name = format!("{:?}", adapter_info.backend);

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("SkyEngine Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .map_err(|e| GpuInitError::DeviceCreation(e.to_string()))?;

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: if vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            },
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let sampler_linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("default_linear_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let sampler_nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("default_nearest_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        eprintln!(
            "[SkyEngine GPU] Adapter: {} | Backend: {} | Format: {:?}",
            adapter_name, backend_name, format
        );

        Ok(Self {
            device,
            queue,
            surface: Some(surface),
            surface_config,
            adapter_name,
            backend_name,
            frame: None,
            uploads: FrameUploadArena::default(),
            sampler_linear,
            sampler_nearest,
        })
    }

    // ── Accessors ────────────────────────────────────────────────────────

    /// The wgpu device — use this to create buffers, textures, pipelines, etc.
    #[inline]
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The wgpu queue — use this for `write_buffer`, `write_texture`, `submit`.
    #[inline]
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Current surface dimensions in pixels.
    #[inline]
    pub fn surface_size(&self) -> [u32; 2] {
        [self.surface_config.width, self.surface_config.height]
    }

    /// Pixel format of the presentation surface.
    #[inline]
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }

    /// Returns `true` when this context owns a presentation surface.
    #[inline]
    pub fn has_surface(&self) -> bool {
        self.surface.is_some()
    }

    /// Human-readable GPU adapter name (e.g. "NVIDIA GeForce RTX 4090").
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Graphics API backend name (e.g. "Vulkan", "Dx12", "Metal").
    pub fn backend_name(&self) -> &str {
        &self.backend_name
    }

    /// Linear-filtered sampler — PostFX, compositing, lighting.
    #[inline]
    pub fn sampler_linear(&self) -> &wgpu::Sampler {
        &self.sampler_linear
    }

    /// Nearest-filtered sampler — pixel sprites, data textures.
    #[inline]
    pub fn sampler_nearest(&self) -> &wgpu::Sampler {
        &self.sampler_nearest
    }

    // ── Surface management ───────────────────────────────────────────────

    /// Resize the presentation surface. Call on window resize events.
    pub fn resize_surface(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            if let Some(ref surface) = self.surface {
                surface.configure(&self.device, &self.surface_config);
            }
        }
    }

    // ── Frame lifecycle ──────────────────────────────────────────────────

    /// Begin a new frame.
    ///
    /// On surface-backed contexts this acquires the current surface texture.
    /// On headless contexts it creates an encoder-only frame so off-screen
    /// target rendering can still use the same frame API.
    pub fn begin_frame(&mut self) -> Result<(), GpuError> {
        if self.frame.is_some() {
            return Err(GpuError::Other(
                "begin_frame called while frame is active".into(),
            ));
        }

        let (surface_texture, surface_view) = if let Some(surface) = self.surface.as_ref() {
            let surface_texture = match surface.get_current_texture() {
                Ok(st) => st,
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    surface.configure(&self.device, &self.surface_config);
                    return Err(GpuError::SurfaceLost);
                }
                Err(wgpu::SurfaceError::OutOfMemory) => return Err(GpuError::OutOfMemory),
                Err(e) => return Err(GpuError::Other(e.to_string())),
            };
            let surface_view = surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            (Some(surface_texture), Some(surface_view))
        } else {
            (None, None)
        };

        let encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame_encoder"),
            });
        self.uploads.reset();
        self.frame = Some(FrameState {
            surface_texture,
            surface_view,
            encoder,
        });
        Ok(())
    }

    /// Access the explicit frame recorder.
    ///
    /// # Panics
    /// Panics if called outside a `begin_frame` / `end_frame` pair.
    pub fn frame(&mut self) -> GpuFrame<'_> {
        assert!(self.frame.is_some(), "frame() requires an active frame");
        GpuFrame { ctx: self }
    }

    /// Get the current frame's surface view.
    ///
    /// # Panics
    /// Panics if called outside a `begin_frame` / `end_frame` pair or if the
    /// active frame is headless.
    pub fn surface_view(&self) -> &wgpu::TextureView {
        self.frame
            .as_ref()
            .expect("surface_view requires active frame")
            .surface_view
            .as_ref()
            .expect("surface_view requires a surface-backed frame")
    }

    /// Get a mutable reference to the current frame's command encoder.
    ///
    /// # Panics
    /// Panics if called outside a `begin_frame` / `end_frame` pair.
    pub fn encoder(&mut self) -> &mut wgpu::CommandEncoder {
        &mut self
            .frame
            .as_mut()
            .expect("encoder requires active frame")
            .encoder
    }

    /// Whether a frame is currently active.
    #[inline]
    pub fn has_active_frame(&self) -> bool {
        self.frame.is_some()
    }

    /// Upload transient vertex data into the frame arena.
    pub fn upload_vertices<T: bytemuck::Pod>(&mut self, data: &[T]) -> UploadSlice {
        let device = &self.device;
        let queue = &self.queue;
        assert!(
            self.frame.is_some(),
            "upload_vertices requires active frame"
        );
        self.uploads.write_vertices(device, queue, data)
    }

    /// Upload transient index data into the frame arena.
    pub fn upload_indices_u16(&mut self, indices: &[u16]) -> UploadSlice {
        let device = &self.device;
        let queue = &self.queue;
        assert!(
            self.frame.is_some(),
            "upload_indices_u16 requires active frame"
        );
        self.uploads.write_indices_u16(device, queue, indices)
    }

    /// Submit the current frame encoder and replace it with a fresh one.
    ///
    /// Primarily used to establish explicit submit boundaries (for example,
    /// render-graph copy/upload operations). Normal draw paths should prefer
    /// frame-local sub-allocation instead of relying on `flush`.
    ///
    /// # Panics
    /// Panics if called outside a `begin_frame` / `end_frame` pair.
    pub fn flush(&mut self, next_encoder_label: &str) {
        let finished = {
            let new_encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some(next_encoder_label),
                });
            let frame = self.frame.as_mut().expect("flush requires active frame");
            std::mem::replace(&mut frame.encoder, new_encoder).finish()
        };
        self.queue.submit(std::iter::once(finished));
    }

    /// Execute a scoped render pass.
    ///
    /// # Panics
    /// Panics if called outside a `begin_frame` / `end_frame` pair.
    pub fn with_render_pass<F>(&mut self, desc: &wgpu::RenderPassDescriptor<'_>, f: F)
    where
        F: FnOnce(&mut wgpu::RenderPass<'_>),
    {
        let mut frame = self.frame();
        let mut pass = frame.begin_render_pass(desc);
        f(&mut *pass);
    }

    /// Execute a scoped compute pass.
    ///
    /// # Panics
    /// Panics if called outside a `begin_frame` / `end_frame` pair.
    pub fn with_compute_pass<F>(&mut self, desc: &wgpu::ComputePassDescriptor<'_>, f: F)
    where
        F: FnOnce(&mut wgpu::ComputePass<'_>),
    {
        let mut frame = self.frame();
        let mut pass = frame.begin_compute_pass(desc);
        f(&mut *pass);
    }

    /// Execute a scoped render pass targeting the current frame's surface.
    pub fn with_surface_pass<F>(&mut self, label: &str, clear: Option<wgpu::Color>, f: F)
    where
        F: FnOnce(&mut wgpu::RenderPass<'_>),
    {
        let mut frame = self.frame();
        let mut pass = frame.begin_surface_pass(label, Some(clear.unwrap_or(wgpu::Color::BLACK)));
        f(&mut *pass);
    }

    /// Execute a scoped render pass that preserves the current surface contents.
    pub fn with_surface_pass_loaded<F>(&mut self, label: &str, f: F)
    where
        F: FnOnce(&mut wgpu::RenderPass<'_>),
    {
        let mut frame = self.frame();
        let mut pass = frame.begin_surface_pass_loaded(label);
        f(&mut *pass);
    }

    /// Finish the frame: submit the command encoder and present the surface.
    pub fn end_frame(&mut self) {
        let frame = self
            .frame
            .take()
            .expect("end_frame called without begin_frame");
        self.queue.submit(std::iter::once(frame.encoder.finish()));
        if let Some(surface_texture) = frame.surface_texture {
            surface_texture.present();
        }
    }
}

#[cfg(test)]
impl GpuContext {
    /// Create a headless GpuContext for unit tests (no window/surface).
    pub fn new_headless(
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface_format: wgpu::TextureFormat,
        surface_size: [u32; 2],
    ) -> Self {
        let sampler_linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("test_linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let sampler_nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("test_nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            device,
            queue,
            surface: None,
            surface_config: wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                width: surface_size[0].max(1),
                height: surface_size[1].max(1),
                present_mode: wgpu::PresentMode::AutoNoVsync,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
            },
            adapter_name: "headless".into(),
            backend_name: "test".into(),
            frame: None,
            uploads: FrameUploadArena::default(),
            sampler_linear,
            sampler_nearest,
        }
    }
}

fn align_up(value: u64, alignment: u64) -> u64 {
    debug_assert!(alignment > 0);
    ((value + alignment - 1) / alignment) * alignment
}

fn grow_buffer_size(current: u64, required: u64) -> u64 {
    let mut size = current.max(1);
    while size < required {
        size = size.saturating_mul(2);
    }
    size
}

#[cfg(test)]
mod tests {
    use super::{
        align_up, DynamicUniformBuffer, FrameUploadArena, GpuContext, INITIAL_VERTEX_UPLOAD_BYTES,
    };
    use crate::render::core::target::RenderTarget;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for gpu::context tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("gpu_context_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn frame_upload_arena_reuses_buffers_across_frame_reset() {
        let (device, queue) = create_test_device();
        let mut arena = FrameUploadArena::default();
        let first = arena.write_vertices(&device, &queue, &[[1.0f32, 2.0]]);
        arena.reset();
        let second = arena.write_vertices(&device, &queue, &[[3.0f32, 4.0]]);

        assert_eq!(first.offset(), 0);
        assert_eq!(second.offset(), 0);
        assert_eq!(first.size(), second.size());
    }

    #[test]
    fn frame_upload_arena_aligns_index_uploads() {
        let (device, queue) = create_test_device();
        let mut arena = FrameUploadArena::default();
        let first = arena.write_indices_u16(&device, &queue, &[0, 1, 2]);
        let second = arena.write_indices_u16(&device, &queue, &[3, 4, 5, 6]);

        assert_eq!(first.offset(), 0);
        assert_eq!(first.size(), 6);
        assert_eq!(second.offset(), align_up(8, wgpu::COPY_BUFFER_ALIGNMENT));
        assert_eq!(second.size(), 8);
    }

    #[test]
    fn dynamic_uniform_buffer_grows_and_preserves_stride() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [16, 16]);
        let mut uniforms = DynamicUniformBuffer::<[f32; 4]>::new(
            &ctx,
            "test_uniforms",
            wgpu::ShaderStages::VERTEX_FRAGMENT,
        );

        let mut last_offset = 0;
        for i in 0..80 {
            last_offset = uniforms.push(&ctx, [i as f32, 0.0, 0.0, 1.0]);
        }

        assert!(uniforms.stride() >= std::mem::size_of::<[f32; 4]>() as u64);
        assert_eq!(last_offset as u64, 79 * uniforms.stride());
        assert!(uniforms.buffer().size() >= uniforms.stride() * 80);
    }

    #[test]
    fn headless_frame_supports_target_pass_recording() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [32, 32]);
        let target = RenderTarget::new(&ctx, 32, 32, wgpu::TextureFormat::Rgba8Unorm, "test");

        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        {
            let mut frame = ctx.frame();
            let mut pass = frame.begin_target_pass(
                "headless_target_pass",
                &target,
                wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            );
            pass.set_viewport(0.0, 0.0, 32.0, 32.0, 0.0, 1.0);
        }
        ctx.end_frame();
    }

    #[test]
    fn gpu_context_reuses_upload_capacity_across_frames() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [32, 32]);
        let large_upload = vec![[0.0f32, 1.0f32]; 40_000];

        ctx.begin_frame()
            .expect("first headless begin_frame should succeed");
        let first = ctx.upload_vertices(&large_upload);
        let grown_capacity = ctx.uploads.vertex.current_size;
        assert_eq!(first.offset(), 0);
        assert!(grown_capacity > INITIAL_VERTEX_UPLOAD_BYTES);
        ctx.end_frame();

        ctx.begin_frame()
            .expect("second headless begin_frame should succeed");
        let second = ctx.upload_vertices(&[[2.0f32, 3.0f32]]);
        assert_eq!(second.offset(), 0);
        assert_eq!(ctx.uploads.vertex.current_size, grown_capacity);
        ctx.end_frame();
    }
}
