//! Thin wgpu context — frame lifecycle, surface management, convenience helpers.
//!
//! `GpuContext` owns the device/queue/surface plus the active frame encoder.
//! It also provides a small mid-layer for frame-scoped render-pass recording
//! and transient uploads.

use std::borrow::Cow;
#[cfg(feature = "profile-gpu")]
use std::collections::VecDeque;
use std::num::NonZeroU64;
use std::ops::{Deref, DerefMut, Range};
use std::path::Path;
#[cfg(feature = "profile-gpu")]
use std::sync::mpsc::Receiver;
use std::sync::{mpsc, Arc, OnceLock};
use std::time::Instant;

const INITIAL_VERTEX_UPLOAD_BYTES: u64 = 256 * 1024;
const INITIAL_INDEX_UPLOAD_BYTES: u64 = 128 * 1024;
const INITIAL_DYNAMIC_UNIFORM_CAPACITY: u64 = 64;
#[cfg(feature = "profile-gpu")]
const GPU_TIMESTAMP_QUERY_COUNT: u32 = 2048;
#[cfg(feature = "profile-gpu")]
const GPU_TIMESTAMP_READBACKS: usize = 4;

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
    Timeout,
    Occluded,
    OutOfMemory,
    Other(String),
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SurfaceLost => write!(f, "Surface lost"),
            Self::Timeout => write!(f, "Surface acquisition timed out"),
            Self::Occluded => write!(f, "Surface is occluded"),
            Self::OutOfMemory => write!(f, "Out of GPU memory"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for GpuError {}

#[derive(Debug, Default)]
struct GpuInitProfileSamples {
    instance_ms: f32,
    surface_ms: f32,
    adapter_ms: f32,
    device_ms: f32,
    configure_ms: f32,
    samplers_ms: f32,
}

struct GpuInitProfile {
    enabled: bool,
    start: Instant,
    last: Instant,
}

impl GpuInitProfile {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            enabled: gpu_init_profile_enabled(),
            start: now,
            last: now,
        }
    }

    fn mark(&mut self) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let now = Instant::now();
        let elapsed_ms = now.duration_since(self.last).as_secs_f32() * 1000.0;
        self.last = now;
        elapsed_ms
    }

    fn print(
        &self,
        samples: &GpuInitProfileSamples,
        adapter_name: &str,
        backend_name: &str,
        format: wgpu::TextureFormat,
    ) {
        if !self.enabled {
            return;
        }
        let total_ms = self.start.elapsed().as_secs_f32() * 1000.0;
        eprintln!(
            concat!(
                "[SkyEngine][GpuInitProfile] total={:.3}ms ",
                "instance={:.3} surface={:.3} adapter={:.3} device={:.3} configure={:.3} samplers={:.3} ",
                "adapter_name=\"{}\" backend={} format={:?}"
            ),
            total_ms,
            samples.instance_ms,
            samples.surface_ms,
            samples.adapter_ms,
            samples.device_ms,
            samples.configure_ms,
            samples.samplers_ms,
            adapter_name,
            backend_name,
            format,
        );
    }
}

fn gpu_init_profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("SKY_GPU_PROFILE")
            .or_else(|| std::env::var_os("SKY_APP_STARTUP_PROFILE"))
            .or_else(|| std::env::var_os("SKY_APP_PROFILE"))
            .or_else(|| std::env::var_os("SKY_PROFILE"))
            .is_some_and(env_flag_enabled)
    })
}

fn env_flag_enabled(value: std::ffi::OsString) -> bool {
    let value = value.to_string_lossy();
    !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
}

#[cfg(feature = "profile")]
fn record_gpu_init_profile(
    start: Instant,
    samples: &GpuInitProfileSamples,
    adapter_name: &str,
    backend_name: &str,
    format: wgpu::TextureFormat,
) {
    if !sky_profile::enabled() {
        return;
    }
    let run_id = sky_profile::run_id();
    let mut cursor_ns = sky_profile::elapsed_ns_since_start(start);
    let segments = [
        ("instance", samples.instance_ms),
        ("surface", samples.surface_ms),
        ("adapter", samples.adapter_ms),
        ("device", samples.device_ms),
        ("configure", samples.configure_ms),
        ("samplers", samples.samplers_ms),
    ];
    for (name, elapsed_ms) in segments {
        let duration_ns = profile_ms_to_ns(elapsed_ms);
        let event = sky_profile::ProfileEvent::new(
            run_id.clone(),
            None,
            "gpu_init",
            name,
            cursor_ns,
            duration_ns,
        )
        .with_metadata("adapter_name", adapter_name.to_string())
        .with_metadata("backend", backend_name.to_string())
        .with_metadata("surface_format", format!("{format:?}"));
        sky_profile::record_event(event);
        cursor_ns = cursor_ns.saturating_add(duration_ns);
    }
    sky_profile::flush();
}

#[cfg(feature = "profile")]
fn profile_ms_to_ns(ms: f32) -> u64 {
    if !ms.is_finite() || ms <= 0.0 {
        return 0;
    }
    (f64::from(ms) * 1_000_000.0).round().min(u64::MAX as f64) as u64
}

/// Errors returned while reading the current surface frame back to the CPU.
#[derive(Debug)]
pub enum GpuScreenshotError {
    NoActiveFrame,
    NoSurfaceFrame,
    SurfaceCopyUnsupported,
    UnsupportedFormat(wgpu::TextureFormat),
    MapFailed(String),
    Io(std::io::Error),
    Image(image::ImageError),
}

impl std::fmt::Display for GpuScreenshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoActiveFrame => write!(f, "screenshot requires an active GPU frame"),
            Self::NoSurfaceFrame => write!(f, "screenshot requires a surface-backed frame"),
            Self::SurfaceCopyUnsupported => {
                write!(
                    f,
                    "presentation surface does not support COPY_SRC screenshots"
                )
            }
            Self::UnsupportedFormat(format) => {
                write!(f, "unsupported screenshot surface format {format:?}")
            }
            Self::MapFailed(message) => write!(f, "screenshot readback failed: {message}"),
            Self::Io(error) => write!(f, "{error}"),
            Self::Image(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for GpuScreenshotError {}

impl From<std::io::Error> for GpuScreenshotError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<image::ImageError> for GpuScreenshotError {
    fn from(value: image::ImageError) -> Self {
        Self::Image(value)
    }
}

/// CPU copy of a captured surface frame, stored as tightly-packed RGBA8 pixels.
#[derive(Debug, Clone)]
pub struct GpuScreenshot {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl GpuScreenshot {
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Write the screenshot as an RGBA PNG.
    pub fn write_png(&self, path: impl AsRef<Path>) -> Result<(), GpuScreenshotError> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        image::save_buffer(
            path,
            &self.data,
            self.width,
            self.height,
            image::ColorType::Rgba8,
        )?;
        Ok(())
    }
}

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
    upload_scratch: Vec<u8>,
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
            upload_scratch: Vec::new(),
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

    pub fn push_staged(&mut self, ctx: &GpuContext, value: T) -> u32 {
        let index = self.values.len() as u64;
        self.values.push(value);
        let _ = self.ensure_capacity(ctx, self.values.len() as u64);
        let offset = index * self.stride;
        u32::try_from(offset).expect("dynamic uniform offset exceeds u32::MAX")
    }

    pub fn upload_all(&mut self, ctx: &GpuContext) {
        if self.values.is_empty() {
            return;
        }
        self.reupload_all(ctx);
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

    fn reupload_all(&mut self, ctx: &GpuContext) {
        let value_size = std::mem::size_of::<T>();
        let total_size = (self.values.len() as u64 * self.stride) as usize;
        self.upload_scratch.clear();
        self.upload_scratch.resize(total_size, 0);

        for (index, value) in self.values.iter().enumerate() {
            let offset = index * self.stride as usize;
            let bytes = bytemuck::bytes_of(value);
            self.upload_scratch[offset..offset + value_size].copy_from_slice(bytes);
        }

        ctx.queue()
            .write_buffer(&self.buffer, 0, &self.upload_scratch);
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

#[cfg(feature = "profile-gpu")]
#[derive(Debug, Clone)]
struct GpuTimestampSample {
    category: &'static str,
    name: String,
    cpu_start_ns: u64,
    cpu_duration_ns: u64,
    start_index: u32,
    end_index: u32,
}

#[cfg(feature = "profile-gpu")]
struct PendingGpuTimestampReadback {
    readback_index: usize,
    query_count: u32,
    samples: Vec<GpuTimestampSample>,
    receiver: Receiver<Result<(), wgpu::BufferAsyncError>>,
}

#[cfg(feature = "profile-gpu")]
struct GpuTimestampProfiler {
    enabled: bool,
    query_set: Option<wgpu::QuerySet>,
    resolve_buffer: Option<wgpu::Buffer>,
    readback_buffers: Vec<wgpu::Buffer>,
    next_readback: usize,
    query_count: u32,
    samples: Vec<GpuTimestampSample>,
    pending: VecDeque<PendingGpuTimestampReadback>,
    timestamp_period_ns: f64,
}

#[cfg(feature = "profile-gpu")]
pub struct GpuProfileScope {
    sample_index: usize,
    cpu_start: Instant,
    cpu_start_ns: u64,
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
    #[cfg(feature = "profile-gpu")]
    timestamps: GpuTimestampProfiler,
}

/// Explicit frame recorder backed by an active [`GpuContext`] frame.
pub struct GpuFrame<'a> {
    ctx: &'a mut GpuContext,
}

/// Borrowed parts of the active surface-backed frame.
pub struct GpuSurfaceFrameParts<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub surface_view: &'a wgpu::TextureView,
    pub surface_texture: &'a wgpu::Texture,
    pub surface_copy_supported: bool,
    pub surface_format: wgpu::TextureFormat,
    pub surface_size: [u32; 2],
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

#[cfg(feature = "profile-gpu")]
impl GpuTimestampProfiler {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let requested = sky_profile::config().gpu_enabled;
        let has_required_features = device.features().contains(profile_gpu_required_features());
        let enabled = requested && has_required_features;
        if !enabled {
            if requested {
                record_gpu_timestamp_capability(false);
            }
            return Self {
                enabled: false,
                query_set: None,
                resolve_buffer: None,
                readback_buffers: Vec::new(),
                next_readback: 0,
                query_count: 0,
                samples: Vec::new(),
                pending: VecDeque::new(),
                timestamp_period_ns: 1.0,
            };
        }
        record_gpu_timestamp_capability(true);

        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("sky_profile_timestamp_queries"),
            ty: wgpu::QueryType::Timestamp,
            count: GPU_TIMESTAMP_QUERY_COUNT,
        });
        let buffer_size = u64::from(GPU_TIMESTAMP_QUERY_COUNT) * u64::from(wgpu::QUERY_SIZE);
        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky_profile_timestamp_resolve"),
            size: buffer_size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback_buffers = (0..GPU_TIMESTAMP_READBACKS)
            .map(|index| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("sky_profile_timestamp_readback_{index}")),
                    size: buffer_size,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                })
            })
            .collect();

        Self {
            enabled: true,
            query_set: Some(query_set),
            resolve_buffer: Some(resolve_buffer),
            readback_buffers,
            next_readback: 0,
            query_count: 0,
            samples: Vec::new(),
            pending: VecDeque::new(),
            timestamp_period_ns: f64::from(queue.get_timestamp_period()),
        }
    }

    fn begin_frame(&mut self, device: &wgpu::Device) {
        self.poll_completed(device);
        self.query_count = 0;
        self.samples.clear();
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn begin_scope(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        category: &'static str,
        name: impl Into<String>,
    ) -> Option<GpuProfileScope> {
        if !self.enabled || self.query_count + 2 > GPU_TIMESTAMP_QUERY_COUNT {
            return None;
        }
        let query_set = self.query_set.as_ref()?;
        let start_index = self.query_count;
        let end_index = self.query_count + 1;
        self.query_count += 2;
        encoder.write_timestamp(query_set, start_index);
        let cpu_start = Instant::now();
        let cpu_start_ns = sky_profile::elapsed_ns_since_start(cpu_start);
        let sample_index = self.samples.len();
        self.samples.push(GpuTimestampSample {
            category,
            name: name.into(),
            cpu_start_ns,
            cpu_duration_ns: 0,
            start_index,
            end_index,
        });
        Some(GpuProfileScope {
            sample_index,
            cpu_start,
            cpu_start_ns,
        })
    }

    fn end_scope(&mut self, encoder: &mut wgpu::CommandEncoder, scope: GpuProfileScope) {
        if !self.enabled {
            return;
        }
        let Some(sample) = self.samples.get_mut(scope.sample_index) else {
            return;
        };
        if let Some(query_set) = self.query_set.as_ref() {
            encoder.write_timestamp(query_set, sample.end_index);
        }
        sample.cpu_start_ns = scope.cpu_start_ns;
        sample.cpu_duration_ns = scope
            .cpu_start
            .elapsed()
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
    }

    fn resolve_frame(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if !self.enabled || self.query_count == 0 || self.samples.is_empty() {
            return;
        }
        let Some(readback_index) = self.available_readback_index() else {
            self.samples.clear();
            self.query_count = 0;
            return;
        };
        let Some(query_set) = self.query_set.as_ref() else {
            return;
        };
        let Some(resolve_buffer) = self.resolve_buffer.as_ref() else {
            return;
        };
        let bytes = u64::from(self.query_count) * u64::from(wgpu::QUERY_SIZE);
        encoder.resolve_query_set(query_set, 0..self.query_count, resolve_buffer, 0);
        encoder.copy_buffer_to_buffer(
            resolve_buffer,
            0,
            &self.readback_buffers[readback_index],
            0,
            bytes,
        );

        let slice = self.readback_buffers[readback_index].slice(0..bytes);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.pending.push_back(PendingGpuTimestampReadback {
            readback_index,
            query_count: self.query_count,
            samples: std::mem::take(&mut self.samples),
            receiver,
        });
        self.next_readback = (readback_index + 1) % self.readback_buffers.len();
        self.query_count = 0;
    }

    fn poll_completed(&mut self, device: &wgpu::Device) {
        if !self.enabled {
            return;
        }
        let _ = device.poll(wgpu::PollType::Poll);
        let mut index = 0;
        while index < self.pending.len() {
            let ready = match self.pending[index].receiver.try_recv() {
                Ok(Ok(())) => true,
                Ok(Err(_)) => {
                    let pending = self.pending.remove(index).unwrap();
                    self.readback_buffers[pending.readback_index].unmap();
                    continue;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    index += 1;
                    continue;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let pending = self.pending.remove(index).unwrap();
                    self.readback_buffers[pending.readback_index].unmap();
                    continue;
                }
            };
            if ready {
                let pending = self.pending.remove(index).unwrap();
                self.record_pending(pending);
            }
        }
    }

    fn record_pending(&mut self, pending: PendingGpuTimestampReadback) {
        let bytes = u64::from(pending.query_count) * u64::from(wgpu::QUERY_SIZE);
        {
            let mapped = self.readback_buffers[pending.readback_index]
                .slice(0..bytes)
                .get_mapped_range();
            for sample in &pending.samples {
                let Some(start) = read_timestamp_value(&mapped, sample.start_index) else {
                    continue;
                };
                let Some(end) = read_timestamp_value(&mapped, sample.end_index) else {
                    continue;
                };
                if end < start {
                    continue;
                }
                let gpu_duration_ns =
                    ((end - start) as f64 * self.timestamp_period_ns).round() as u64;
                sky_profile::record_gpu_event(
                    sample.category,
                    sample.name.clone(),
                    sample.cpu_start_ns,
                    sample.cpu_duration_ns,
                    gpu_duration_ns,
                );
            }
        }
        self.readback_buffers[pending.readback_index].unmap();
    }

    fn available_readback_index(&self) -> Option<usize> {
        for offset in 0..self.readback_buffers.len() {
            let index = (self.next_readback + offset) % self.readback_buffers.len();
            if self
                .pending
                .iter()
                .all(|pending| pending.readback_index != index)
            {
                return Some(index);
            }
        }
        None
    }
}

#[cfg(feature = "profile-gpu")]
fn read_timestamp_value(mapped: &[u8], index: u32) -> Option<u64> {
    let offset = index as usize * wgpu::QUERY_SIZE as usize;
    let bytes = mapped.get(offset..offset + wgpu::QUERY_SIZE as usize)?;
    Some(u64::from_ne_bytes(bytes.try_into().ok()?))
}

#[cfg(feature = "profile-gpu")]
fn record_gpu_timestamp_capability(supported: bool) {
    if !sky_profile::enabled() {
        return;
    }
    let now = Instant::now();
    let event = sky_profile::ProfileEvent::new(
        sky_profile::run_id(),
        sky_profile::current_frame(),
        "gpu",
        "timestamp_profile_capability",
        sky_profile::elapsed_ns_since_start(now),
        0,
    )
    .with_metadata("gpu_supported", supported);
    sky_profile::record_event(event);
}

#[cfg(feature = "profile-gpu")]
fn profile_gpu_required_features() -> wgpu::Features {
    wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
}

#[cfg(feature = "profile-gpu")]
fn profile_gpu_adapter_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    let required = profile_gpu_required_features();
    if sky_profile::config().gpu_enabled && adapter.features().contains(required) {
        required
    } else {
        if sky_profile::config().gpu_enabled && !adapter.features().contains(required) {
            eprintln!(
                "[SkyEngine][Profile] GPU timestamp profiling disabled: adapter lacks {required:?}"
            );
        }
        wgpu::Features::empty()
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
                    depth_slice: None,
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
                    depth_slice: None,
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
        let mut init_profile = GpuInitProfile::new();
        let mut init_samples = GpuInitProfileSamples::default();
        #[cfg(feature = "profile")]
        let _profile_scope = sky_profile::profile_scope!("gpu", "GpuContext::try_new");

        let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
        instance_desc.backends =
            wgpu::Backends::VULKAN | wgpu::Backends::METAL | wgpu::Backends::DX12;
        let instance = wgpu::Instance::new(instance_desc);
        init_samples.instance_ms = init_profile.mark();

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| GpuInitError::SurfaceCreation(e.to_string()))?;
        init_samples.surface_ms = init_profile.mark();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .map_err(|_| GpuInitError::AdapterUnavailable)?;
        init_samples.adapter_ms = init_profile.mark();

        let adapter_info = adapter.get_info();
        let adapter_name = adapter_info.name.clone();
        let backend_name = format!("{:?}", adapter_info.backend);
        #[cfg(feature = "profile-gpu")]
        let required_features = profile_gpu_adapter_features(&adapter);
        #[cfg(not(feature = "profile-gpu"))]
        let required_features = wgpu::Features::empty();

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("SkyEngine Device"),
            required_features,
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .map_err(|e| GpuInitError::DeviceCreation(e.to_string()))?;
        init_samples.device_ms = init_profile.mark();

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let mut surface_usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        if caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            surface_usage |= wgpu::TextureUsages::COPY_SRC;
        }

        let surface_config = wgpu::SurfaceConfiguration {
            usage: surface_usage,
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
        init_samples.configure_ms = init_profile.mark();

        let sampler_linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("default_linear_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let sampler_nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("default_nearest_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        init_samples.samplers_ms = init_profile.mark();

        eprintln!(
            "[SkyEngine GPU] Adapter: {} | Backend: {} | Format: {:?}",
            adapter_name, backend_name, format
        );
        init_profile.print(&init_samples, &adapter_name, &backend_name, format);
        #[cfg(feature = "profile")]
        record_gpu_init_profile(
            init_profile.start,
            &init_samples,
            &adapter_name,
            &backend_name,
            format,
        );
        #[cfg(feature = "profile-gpu")]
        let timestamps = GpuTimestampProfiler::new(&device, &queue);

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
            #[cfg(feature = "profile-gpu")]
            timestamps,
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
                wgpu::CurrentSurfaceTexture::Success(st)
                | wgpu::CurrentSurfaceTexture::Suboptimal(st) => st,
                wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                    surface.configure(&self.device, &self.surface_config);
                    return Err(GpuError::SurfaceLost);
                }
                wgpu::CurrentSurfaceTexture::Timeout => {
                    return Err(GpuError::Timeout);
                }
                wgpu::CurrentSurfaceTexture::Occluded => return Err(GpuError::Occluded),
                wgpu::CurrentSurfaceTexture::Validation => {
                    return Err(GpuError::Other(
                        "surface texture acquisition failed validation".into(),
                    ));
                }
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
        #[cfg(feature = "profile-gpu")]
        self.timestamps.begin_frame(&self.device);
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

    /// Borrow the active surface-backed frame's raw wgpu parts in one shot.
    ///
    /// This is useful for adapter renderers that operate directly on wgpu
    /// without knowing about `GpuContext`.
    pub fn with_surface_frame_parts<R>(
        &mut self,
        f: impl FnOnce(GpuSurfaceFrameParts<'_>) -> R,
    ) -> R {
        let device = &self.device;
        let queue = &self.queue;
        let surface_format = self.surface_config.format;
        let surface_size = [self.surface_config.width, self.surface_config.height];
        let surface_copy_supported = self
            .surface_config
            .usage
            .contains(wgpu::TextureUsages::COPY_SRC);
        let frame = self
            .frame
            .as_mut()
            .expect("with_surface_frame_parts requires active frame");
        let surface_view = frame
            .surface_view
            .as_ref()
            .expect("with_surface_frame_parts requires a surface-backed frame");
        let surface_texture = &frame
            .surface_texture
            .as_ref()
            .expect("with_surface_frame_parts requires a surface-backed frame")
            .texture;

        f(GpuSurfaceFrameParts {
            device,
            queue,
            encoder: &mut frame.encoder,
            surface_view,
            surface_texture,
            surface_copy_supported,
            surface_format,
            surface_size,
        })
    }

    /// Copy the current presentation surface into a texture for later sampling.
    ///
    /// The caller must ensure no render pass is active, and the target texture
    /// must be created with `COPY_DST` usage and the active surface format.
    pub fn copy_current_surface_to_texture(
        &mut self,
        target: &wgpu::Texture,
    ) -> Result<(), GpuScreenshotError> {
        if self.frame.is_none() {
            return Err(GpuScreenshotError::NoActiveFrame);
        }
        if !self
            .surface_config
            .usage
            .contains(wgpu::TextureUsages::COPY_SRC)
        {
            return Err(GpuScreenshotError::SurfaceCopyUnsupported);
        }

        let width = self.surface_config.width.max(1);
        let height = self.surface_config.height.max(1);
        let frame = self
            .frame
            .as_mut()
            .ok_or(GpuScreenshotError::NoActiveFrame)?;
        let surface_texture = frame
            .surface_texture
            .as_ref()
            .ok_or(GpuScreenshotError::NoSurfaceFrame)?;
        frame.encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &surface_texture.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    /// Copy the current presentation surface into a caller-owned texture.
    ///
    /// This variant allows the destination extent to be smaller than the full
    /// surface; callers can then render/blit from that snapshot into another
    /// target without sampling the swapchain image directly.
    pub fn copy_current_surface_to_texture_extent(
        &mut self,
        target: &wgpu::Texture,
        extent: wgpu::Extent3d,
    ) -> Result<(), GpuScreenshotError> {
        if self.frame.is_none() {
            return Err(GpuScreenshotError::NoActiveFrame);
        }
        if !self
            .surface_config
            .usage
            .contains(wgpu::TextureUsages::COPY_SRC)
        {
            return Err(GpuScreenshotError::SurfaceCopyUnsupported);
        }

        let width = extent.width.min(self.surface_config.width.max(1)).max(1);
        let height = extent.height.min(self.surface_config.height.max(1)).max(1);
        let frame = self
            .frame
            .as_mut()
            .ok_or(GpuScreenshotError::NoActiveFrame)?;
        let surface_texture = frame
            .surface_texture
            .as_ref()
            .ok_or(GpuScreenshotError::NoSurfaceFrame)?;
        frame.encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &surface_texture.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    /// Whether a frame is currently active.
    #[inline]
    pub fn has_active_frame(&self) -> bool {
        self.frame.is_some()
    }

    #[cfg(feature = "profile-gpu")]
    pub fn gpu_profile_supported(&self) -> bool {
        self.timestamps.is_enabled()
    }

    #[cfg(feature = "profile-gpu")]
    pub fn begin_gpu_profile_scope(
        &mut self,
        category: &'static str,
        name: impl Into<String>,
    ) -> Option<GpuProfileScope> {
        let frame = self.frame.as_mut()?;
        self.timestamps
            .begin_scope(&mut frame.encoder, category, name)
    }

    #[cfg(feature = "profile-gpu")]
    pub fn end_gpu_profile_scope(&mut self, scope: GpuProfileScope) {
        let Some(frame) = self.frame.as_mut() else {
            return;
        };
        self.timestamps.end_scope(&mut frame.encoder, scope);
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

    /// Capture the current surface frame as RGBA8 pixels.
    ///
    /// Call this after rendering the frame contents and before [`end_frame`](Self::end_frame).
    /// The method flushes the active encoder so the copy can be mapped immediately.
    pub fn capture_surface_screenshot(&mut self) -> Result<GpuScreenshot, GpuScreenshotError> {
        if self.frame.is_none() {
            return Err(GpuScreenshotError::NoActiveFrame);
        }
        if !self
            .surface_config
            .usage
            .contains(wgpu::TextureUsages::COPY_SRC)
        {
            return Err(GpuScreenshotError::SurfaceCopyUnsupported);
        }

        let format = self.surface_config.format;
        let convert = match format {
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => {
                ScreenshotFormatConvert::Rgba
            }
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => {
                ScreenshotFormatConvert::Bgra
            }
            _ => return Err(GpuScreenshotError::UnsupportedFormat(format)),
        };

        let width = self.surface_config.width.max(1);
        let height = self.surface_config.height.max(1);
        let tight_row_bytes = width * 4;
        let padded_row_bytes = align_to_u32(tight_row_bytes, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer_size = padded_row_bytes as u64 * height as u64;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("surface_screenshot_readback_buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        {
            let frame = self
                .frame
                .as_mut()
                .ok_or(GpuScreenshotError::NoActiveFrame)?;
            let surface_texture = frame
                .surface_texture
                .as_ref()
                .ok_or(GpuScreenshotError::NoSurfaceFrame)?;
            frame.encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &surface_texture.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row_bytes),
                        rows_per_image: Some(height),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }

        self.flush("surface_screenshot_after_copy");

        let slice = buffer.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result.map(|_| ()));
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        receiver
            .recv()
            .map_err(|error| GpuScreenshotError::MapFailed(error.to_string()))?
            .map_err(|error| GpuScreenshotError::MapFailed(error.to_string()))?;

        let mapped = slice.get_mapped_range();
        let mut data = vec![0; (tight_row_bytes * height) as usize];
        for y in 0..height as usize {
            let src_row = y * padded_row_bytes as usize;
            let dst_row = y * tight_row_bytes as usize;
            let src = &mapped[src_row..src_row + tight_row_bytes as usize];
            let dst = &mut data[dst_row..dst_row + tight_row_bytes as usize];
            match convert {
                ScreenshotFormatConvert::Rgba => dst.copy_from_slice(src),
                ScreenshotFormatConvert::Bgra => {
                    for (src, dst) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
                        dst[0] = src[2];
                        dst[1] = src[1];
                        dst[2] = src[0];
                        dst[3] = src[3];
                    }
                }
            }
        }
        drop(mapped);
        buffer.unmap();

        Ok(GpuScreenshot {
            width,
            height,
            data,
        })
    }

    /// Capture the current surface frame and write it as a PNG.
    pub fn capture_surface_screenshot_png(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<(), GpuScreenshotError> {
        self.capture_surface_screenshot()?.write_png(path)
    }

    /// Finish the frame: submit the command encoder and present the surface.
    pub fn end_frame(&mut self) {
        let mut frame = self
            .frame
            .take()
            .expect("end_frame called without begin_frame");
        #[cfg(feature = "profile-gpu")]
        self.timestamps.resolve_frame(&mut frame.encoder);
        self.queue.submit(std::iter::once(frame.encoder.finish()));
        if let Some(surface_texture) = frame.surface_texture {
            surface_texture.present();
        }
        #[cfg(feature = "profile-gpu")]
        self.timestamps.poll_completed(&self.device);
    }
}

impl GpuContext {
    /// Create a headless GpuContext with no window or presentation surface.
    ///
    /// This is useful for render tests, offline tools, and performance probes
    /// that want to drive the full renderer without vsync or OS windowing.
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
        #[cfg(feature = "profile-gpu")]
        let timestamps = GpuTimestampProfiler::new(&device, &queue);
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
            #[cfg(feature = "profile-gpu")]
            timestamps,
        }
    }
}

fn align_up(value: u64, alignment: u64) -> u64 {
    debug_assert!(alignment > 0);
    ((value + alignment - 1) / alignment) * alignment
}

#[inline]
fn align_to_u32(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

#[derive(Clone, Copy)]
enum ScreenshotFormatConvert {
    Rgba,
    Bgra,
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
    use crate::render::gpu::RenderTarget;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for gpu::context tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gpu_context_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
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
