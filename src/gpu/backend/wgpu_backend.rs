//! wgpu-based GPU backend implementation.
//!
//! This backend translates the [`Gpu`] trait into wgpu API calls.  On Windows
//! it defaults to the Vulkan backend; on macOS to Metal; on Linux to Vulkan.
//!
//! # Resource management
//!
//! Each resource type is stored in a [`HandlePool`] that maps our
//! [`Handle<T>`] to the corresponding `wgpu::*` object.  The generation
//! counter in the handle prevents use-after-free.

use std::sync::Arc;

use crate::gpu::desc::*;
use crate::gpu::handle::*;
use crate::gpu::types::*;
use crate::gpu::{ComputePassEncoder, Gpu, RenderPassEncoder};

// ── Type conversion helpers ─────────────────────────────────────────────────

fn to_wgpu_texture_format(f: TextureFormat) -> wgpu::TextureFormat {
    match f {
        TextureFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
        TextureFormat::Rgba8UnormSrgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        TextureFormat::Bgra8Unorm => wgpu::TextureFormat::Bgra8Unorm,
        TextureFormat::Bgra8UnormSrgb => wgpu::TextureFormat::Bgra8UnormSrgb,
        TextureFormat::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
        TextureFormat::R32Float => wgpu::TextureFormat::R32Float,
        TextureFormat::Rg32Float => wgpu::TextureFormat::Rg32Float,
        TextureFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
        TextureFormat::Depth32Float => wgpu::TextureFormat::Depth32Float,
        TextureFormat::Depth24PlusStencil8 => wgpu::TextureFormat::Depth24PlusStencil8,
    }
}

fn from_wgpu_texture_format(f: wgpu::TextureFormat) -> TextureFormat {
    match f {
        wgpu::TextureFormat::Rgba8Unorm => TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb => TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8Unorm => TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Bgra8UnormSrgb => TextureFormat::Bgra8UnormSrgb,
        wgpu::TextureFormat::Rgba16Float => TextureFormat::Rgba16Float,
        wgpu::TextureFormat::R32Float => TextureFormat::R32Float,
        wgpu::TextureFormat::Rg32Float => TextureFormat::Rg32Float,
        wgpu::TextureFormat::Rgba32Float => TextureFormat::Rgba32Float,
        wgpu::TextureFormat::Depth32Float => TextureFormat::Depth32Float,
        wgpu::TextureFormat::Depth24PlusStencil8 => TextureFormat::Depth24PlusStencil8,
        _ => panic!(
            "Unsupported wgpu texture format {:?} — add it to TextureFormat and the conversion functions",
            f
        ),
    }
}

fn to_wgpu_buffer_usages(u: BufferUsage) -> wgpu::BufferUsages {
    let mut out = wgpu::BufferUsages::empty();
    let bits = u.bits();
    if bits & BufferUsage::VERTEX.bits() != 0 {
        out |= wgpu::BufferUsages::VERTEX;
    }
    if bits & BufferUsage::INDEX.bits() != 0 {
        out |= wgpu::BufferUsages::INDEX;
    }
    if bits & BufferUsage::UNIFORM.bits() != 0 {
        out |= wgpu::BufferUsages::UNIFORM;
    }
    if bits & BufferUsage::STORAGE.bits() != 0 {
        out |= wgpu::BufferUsages::STORAGE;
    }
    if bits & BufferUsage::COPY_SRC.bits() != 0 {
        out |= wgpu::BufferUsages::COPY_SRC;
    }
    if bits & BufferUsage::COPY_DST.bits() != 0 {
        out |= wgpu::BufferUsages::COPY_DST;
    }
    if bits & BufferUsage::INDIRECT.bits() != 0 {
        out |= wgpu::BufferUsages::INDIRECT;
    }
    if bits & BufferUsage::QUERY_RESOLVE.bits() != 0 {
        out |= wgpu::BufferUsages::QUERY_RESOLVE;
    }
    // All buffers need COPY_DST for write_buffer
    out |= wgpu::BufferUsages::COPY_DST;
    out
}

fn to_wgpu_texture_usages(u: ImageUsage) -> wgpu::TextureUsages {
    let mut out = wgpu::TextureUsages::empty();
    let bits = u.bits();
    if bits & ImageUsage::SAMPLED.bits() != 0 {
        out |= wgpu::TextureUsages::TEXTURE_BINDING;
    }
    if bits & ImageUsage::STORAGE.bits() != 0 {
        out |= wgpu::TextureUsages::STORAGE_BINDING;
    }
    if bits & ImageUsage::RENDER_TARGET.bits() != 0 {
        out |= wgpu::TextureUsages::RENDER_ATTACHMENT;
    }
    if bits & ImageUsage::COPY_SRC.bits() != 0 {
        out |= wgpu::TextureUsages::COPY_SRC;
    }
    if bits & ImageUsage::COPY_DST.bits() != 0 {
        out |= wgpu::TextureUsages::COPY_DST;
    }
    out
}

fn to_wgpu_vertex_format(f: VertexFormat) -> wgpu::VertexFormat {
    match f {
        VertexFormat::Float32 => wgpu::VertexFormat::Float32,
        VertexFormat::Float32x2 => wgpu::VertexFormat::Float32x2,
        VertexFormat::Float32x3 => wgpu::VertexFormat::Float32x3,
        VertexFormat::Float32x4 => wgpu::VertexFormat::Float32x4,
        VertexFormat::Uint32 => wgpu::VertexFormat::Uint32,
        VertexFormat::Uint8x4 => wgpu::VertexFormat::Uint8x4,
        VertexFormat::Unorm8x4 => wgpu::VertexFormat::Unorm8x4,
    }
}

fn to_wgpu_address_mode(m: AddressMode) -> wgpu::AddressMode {
    match m {
        AddressMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        AddressMode::Repeat => wgpu::AddressMode::Repeat,
        AddressMode::MirrorRepeat => wgpu::AddressMode::MirrorRepeat,
    }
}

fn to_wgpu_filter_mode(m: FilterMode) -> wgpu::FilterMode {
    match m {
        FilterMode::Nearest => wgpu::FilterMode::Nearest,
        FilterMode::Linear => wgpu::FilterMode::Linear,
    }
}

fn to_wgpu_blend_factor(f: BlendFactor) -> wgpu::BlendFactor {
    match f {
        BlendFactor::Zero => wgpu::BlendFactor::Zero,
        BlendFactor::One => wgpu::BlendFactor::One,
        BlendFactor::SrcAlpha => wgpu::BlendFactor::SrcAlpha,
        BlendFactor::OneMinusSrcAlpha => wgpu::BlendFactor::OneMinusSrcAlpha,
        BlendFactor::DstAlpha => wgpu::BlendFactor::DstAlpha,
        BlendFactor::OneMinusDstAlpha => wgpu::BlendFactor::OneMinusDstAlpha,
        BlendFactor::SrcColor => wgpu::BlendFactor::Src,
        BlendFactor::OneMinusSrcColor => wgpu::BlendFactor::OneMinusSrc,
        BlendFactor::DstColor => wgpu::BlendFactor::Dst,
        BlendFactor::OneMinusDstColor => wgpu::BlendFactor::OneMinusDst,
    }
}

fn to_wgpu_blend_op(op: BlendOp) -> wgpu::BlendOperation {
    match op {
        BlendOp::Add => wgpu::BlendOperation::Add,
        BlendOp::Subtract => wgpu::BlendOperation::Subtract,
        BlendOp::ReverseSubtract => wgpu::BlendOperation::ReverseSubtract,
        BlendOp::Min => wgpu::BlendOperation::Min,
        BlendOp::Max => wgpu::BlendOperation::Max,
    }
}

fn to_wgpu_blend_state(b: &BlendState) -> wgpu::BlendState {
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: to_wgpu_blend_factor(b.src_color),
            dst_factor: to_wgpu_blend_factor(b.dst_color),
            operation: to_wgpu_blend_op(b.color_op),
        },
        alpha: wgpu::BlendComponent {
            src_factor: to_wgpu_blend_factor(b.src_alpha),
            dst_factor: to_wgpu_blend_factor(b.dst_alpha),
            operation: to_wgpu_blend_op(b.alpha_op),
        },
    }
}

fn to_wgpu_primitive_topology(t: PrimitiveTopology) -> wgpu::PrimitiveTopology {
    match t {
        PrimitiveTopology::PointList => wgpu::PrimitiveTopology::PointList,
        PrimitiveTopology::LineList => wgpu::PrimitiveTopology::LineList,
        PrimitiveTopology::LineStrip => wgpu::PrimitiveTopology::LineStrip,
        PrimitiveTopology::TriangleList => wgpu::PrimitiveTopology::TriangleList,
        PrimitiveTopology::TriangleStrip => wgpu::PrimitiveTopology::TriangleStrip,
    }
}

fn to_wgpu_front_face(f: FrontFace) -> wgpu::FrontFace {
    match f {
        FrontFace::Ccw => wgpu::FrontFace::Ccw,
        FrontFace::Cw => wgpu::FrontFace::Cw,
    }
}

fn to_wgpu_cull_mode(c: CullMode) -> Option<wgpu::Face> {
    match c {
        CullMode::None => None,
        CullMode::Front => Some(wgpu::Face::Front),
        CullMode::Back => Some(wgpu::Face::Back),
    }
}

fn to_wgpu_index_format(f: IndexFormat) -> wgpu::IndexFormat {
    match f {
        IndexFormat::Uint16 => wgpu::IndexFormat::Uint16,
        IndexFormat::Uint32 => wgpu::IndexFormat::Uint32,
    }
}

fn to_wgpu_compare_function(c: CompareFunction) -> wgpu::CompareFunction {
    match c {
        CompareFunction::Never => wgpu::CompareFunction::Never,
        CompareFunction::Less => wgpu::CompareFunction::Less,
        CompareFunction::LessEqual => wgpu::CompareFunction::LessEqual,
        CompareFunction::Greater => wgpu::CompareFunction::Greater,
        CompareFunction::GreaterEqual => wgpu::CompareFunction::GreaterEqual,
        CompareFunction::Equal => wgpu::CompareFunction::Equal,
        CompareFunction::NotEqual => wgpu::CompareFunction::NotEqual,
        CompareFunction::Always => wgpu::CompareFunction::Always,
    }
}

fn to_wgpu_stencil_operation(op: StencilOperation) -> wgpu::StencilOperation {
    match op {
        StencilOperation::Keep => wgpu::StencilOperation::Keep,
        StencilOperation::Zero => wgpu::StencilOperation::Zero,
        StencilOperation::Replace => wgpu::StencilOperation::Replace,
        StencilOperation::Invert => wgpu::StencilOperation::Invert,
        StencilOperation::IncrementClamp => wgpu::StencilOperation::IncrementClamp,
        StencilOperation::DecrementClamp => wgpu::StencilOperation::DecrementClamp,
        StencilOperation::IncrementWrap => wgpu::StencilOperation::IncrementWrap,
        StencilOperation::DecrementWrap => wgpu::StencilOperation::DecrementWrap,
    }
}

fn to_wgpu_stencil_face_state(s: &StencilFaceState) -> wgpu::StencilFaceState {
    wgpu::StencilFaceState {
        compare: to_wgpu_compare_function(s.compare),
        fail_op: to_wgpu_stencil_operation(s.fail_op),
        depth_fail_op: to_wgpu_stencil_operation(s.depth_fail_op),
        pass_op: to_wgpu_stencil_operation(s.pass_op),
    }
}

// ── WgpuBackend ─────────────────────────────────────────────────────────────

/// Image pool stores texture + default view.
struct WgpuImage {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// Per-frame state.
struct FrameState {
    surface_texture: wgpu::SurfaceTexture,
    surface_view: wgpu::TextureView,
    encoder: wgpu::CommandEncoder,
}

/// wgpu-based implementation of the [`Gpu`] trait.
///
/// On Windows this defaults to the Vulkan backend. Set `WGPU_BACKEND=vulkan`
/// to force it explicitly (handy for debugging).
pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    adapter_name: String,
    backend_name: String,

    // Resource pools
    buffers: HandlePool<BufferTag, wgpu::Buffer>,
    images: HandlePool<ImageTag, WgpuImage>,
    image_views: HandlePool<ImageViewTag, wgpu::TextureView>,
    samplers: HandlePool<SamplerTag, wgpu::Sampler>,
    shaders: HandlePool<ShaderTag, wgpu::ShaderModule>,
    bind_group_layouts: HandlePool<BindGroupLayoutTag, wgpu::BindGroupLayout>,
    bind_groups: HandlePool<BindGroupTag, wgpu::BindGroup>,
    pipelines: HandlePool<PipelineTag, wgpu::RenderPipeline>,
    compute_pipelines: HandlePool<ComputePipelineTag, wgpu::ComputePipeline>,

    // Frame state
    frame: Option<FrameState>,
}

impl WgpuBackend {
    /// Create a new wgpu backend attached to the given window.
    ///
    /// This blocks on adapter/device creation using `pollster`.
    pub fn new(window: Arc<winit::window::Window>, vsync: bool) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::METAL | wgpu::Backends::DX12,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found");

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
        .expect("Failed to create GPU device");

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

        eprintln!(
            "[SkyEngine GPU] Adapter: {} | Backend: {} | Format: {:?}",
            adapter_name, backend_name, format
        );

        Self {
            device,
            queue,
            surface,
            surface_config,
            adapter_name,
            backend_name,

            buffers: HandlePool::new(),
            images: HandlePool::new(),
            image_views: HandlePool::new(),
            samplers: HandlePool::new(),
            shaders: HandlePool::new(),
            bind_group_layouts: HandlePool::new(),
            bind_groups: HandlePool::new(),
            pipelines: HandlePool::new(),
            compute_pipelines: HandlePool::new(),

            frame: None,
        }
    }
}

struct WgpuRenderPassEncoder<'a, 'b> {
    pass: &'a mut wgpu::RenderPass<'b>,
    pipelines: &'a HandlePool<PipelineTag, wgpu::RenderPipeline>,
    bind_groups: &'a HandlePool<BindGroupTag, wgpu::BindGroup>,
    buffers: &'a HandlePool<BufferTag, wgpu::Buffer>,
}

struct WgpuComputePassEncoder<'a, 'b> {
    pass: &'a mut wgpu::ComputePass<'b>,
    compute_pipelines: &'a HandlePool<ComputePipelineTag, wgpu::ComputePipeline>,
    bind_groups: &'a HandlePool<BindGroupTag, wgpu::BindGroup>,
    buffers: &'a HandlePool<BufferTag, wgpu::Buffer>,
}

impl RenderPassEncoder for WgpuRenderPassEncoder<'_, '_> {
    fn set_pipeline(&mut self, pip: Pipeline) {
        let pipeline = self.pipelines.get(pip).expect("Invalid pipeline handle");
        self.pass.set_pipeline(pipeline);
    }

    fn set_bind_group(&mut self, slot: u32, bg: BindGroup) {
        let bind_group = self.bind_groups.get(bg).expect("Invalid bind group handle");
        self.pass.set_bind_group(slot, bind_group, &[]);
    }

    fn set_vertex_buffer(&mut self, slot: u32, buf: Buffer) {
        let buffer = self.buffers.get(buf).expect("Invalid buffer handle");
        self.pass.set_vertex_buffer(slot, buffer.slice(..));
    }

    fn set_index_buffer(&mut self, buf: Buffer, format: IndexFormat) {
        let buffer = self.buffers.get(buf).expect("Invalid buffer handle");
        self.pass
            .set_index_buffer(buffer.slice(..), to_wgpu_index_format(format));
    }

    fn set_viewport(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.pass.set_viewport(x, y, w, h, 0.0, 1.0);
    }

    fn set_scissor(&mut self, x: u32, y: u32, w: u32, h: u32) {
        self.pass.set_scissor_rect(x, y, w, h);
    }

    fn draw(&mut self, vertices: std::ops::Range<u32>, instances: std::ops::Range<u32>) {
        self.pass.draw(vertices, instances);
    }

    fn draw_indexed(
        &mut self,
        indices: std::ops::Range<u32>,
        base_vertex: i32,
        instances: std::ops::Range<u32>,
    ) {
        self.pass.draw_indexed(indices, base_vertex, instances);
    }

    fn draw_indirect(&mut self, buffer: Buffer, offset: u64) {
        let buf = self.buffers.get(buffer).expect("Invalid buffer handle");
        self.pass.draw_indirect(buf, offset);
    }

    fn draw_indexed_indirect(&mut self, buffer: Buffer, offset: u64) {
        let buf = self.buffers.get(buffer).expect("Invalid buffer handle");
        self.pass.draw_indexed_indirect(buf, offset);
    }
}

impl ComputePassEncoder for WgpuComputePassEncoder<'_, '_> {
    fn set_pipeline(&mut self, pip: ComputePipeline) {
        let pipeline = self
            .compute_pipelines
            .get(pip)
            .expect("Invalid compute pipeline handle");
        self.pass.set_pipeline(pipeline);
    }

    fn set_bind_group(&mut self, slot: u32, bg: BindGroup) {
        let bind_group = self.bind_groups.get(bg).expect("Invalid bind group handle");
        self.pass.set_bind_group(slot, bind_group, &[]);
    }

    fn dispatch(&mut self, x: u32, y: u32, z: u32) {
        self.pass.dispatch_workgroups(x, y, z);
    }

    fn dispatch_indirect(&mut self, buffer: Buffer, offset: u64) {
        let buf = self.buffers.get(buffer).expect("Invalid buffer handle");
        self.pass.dispatch_workgroups_indirect(buf, offset);
    }
}

impl Gpu for WgpuBackend {
    // ── Resource creation ───────────────────────────────────────────────

    fn create_buffer(&mut self, desc: &BufferDesc) -> Buffer {
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(desc.label.as_ref()),
            size: desc.size,
            usage: to_wgpu_buffer_usages(desc.usage),
            mapped_at_creation: false,
        });
        self.buffers.insert(buf)
    }

    fn create_image(&mut self, desc: &ImageDesc) -> Image {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(desc.label.as_ref()),
            size: wgpu::Extent3d {
                width: desc.width,
                height: desc.height,
                depth_or_array_layers: desc.depth,
            },
            mip_level_count: desc.mip_levels,
            sample_count: 1,
            dimension: if desc.depth > 1 {
                wgpu::TextureDimension::D3
            } else {
                wgpu::TextureDimension::D2
            },
            format: to_wgpu_texture_format(desc.format),
            usage: to_wgpu_texture_usages(desc.usage),
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.images.insert(WgpuImage { texture, view })
    }

    fn create_image_view(&mut self, desc: &ImageViewDesc) -> ImageView {
        let img = self.images.get(desc.image).expect("Invalid image handle");
        let view = img.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(to_wgpu_texture_format(desc.format)),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: desc.base_mip_level,
            mip_level_count: desc.mip_level_count,
            base_array_layer: desc.base_array_layer,
            array_layer_count: desc.array_layer_count,
            ..Default::default()
        });
        self.image_views.insert(view)
    }

    fn create_sampler(&mut self, desc: &SamplerDesc) -> Sampler {
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(desc.label.as_ref()),
            address_mode_u: to_wgpu_address_mode(desc.address_mode_u),
            address_mode_v: to_wgpu_address_mode(desc.address_mode_v),
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: to_wgpu_filter_mode(desc.mag_filter),
            min_filter: to_wgpu_filter_mode(desc.min_filter),
            mipmap_filter: to_wgpu_filter_mode(desc.mipmap_filter),
            ..Default::default()
        });
        self.samplers.insert(sampler)
    }

    fn create_shader(&mut self, desc: &ShaderDesc) -> Shader {
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(desc.label.as_ref()),
                source: wgpu::ShaderSource::Wgsl(desc.source.clone()),
            });
        self.shaders.insert(module)
    }

    fn create_bind_group_layout(&mut self, desc: &BindGroupLayoutDesc) -> BindGroupLayout {
        let entries: Vec<wgpu::BindGroupLayoutEntry> = desc
            .entries
            .iter()
            .map(|e| {
                let visibility = {
                    let mut v = wgpu::ShaderStages::empty();
                    let bits = e.visibility.bits();
                    if bits & ShaderStages::VERTEX.bits() != 0 {
                        v |= wgpu::ShaderStages::VERTEX;
                    }
                    if bits & ShaderStages::FRAGMENT.bits() != 0 {
                        v |= wgpu::ShaderStages::FRAGMENT;
                    }
                    if bits & ShaderStages::COMPUTE.bits() != 0 {
                        v |= wgpu::ShaderStages::COMPUTE;
                    }
                    v
                };
                wgpu::BindGroupLayoutEntry {
                    binding: e.binding,
                    visibility,
                    ty: match e.ty {
                        BindingType::UniformBuffer => wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        BindingType::StorageBuffer => wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        BindingType::StorageBufferReadOnly => wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        BindingType::Texture => wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        BindingType::TextureNonFiltering => wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        BindingType::Sampler => {
                            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)
                        }
                        BindingType::SamplerNonFiltering => {
                            wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering)
                        }
                        BindingType::StorageTexture { access, format } => {
                            wgpu::BindingType::StorageTexture {
                                access: match access {
                                    StorageTextureAccess::ReadOnly => {
                                        wgpu::StorageTextureAccess::ReadOnly
                                    }
                                    StorageTextureAccess::WriteOnly => {
                                        wgpu::StorageTextureAccess::WriteOnly
                                    }
                                    StorageTextureAccess::ReadWrite => {
                                        wgpu::StorageTextureAccess::ReadWrite
                                    }
                                },
                                format: to_wgpu_texture_format(format),
                                view_dimension: wgpu::TextureViewDimension::D2,
                            }
                        }
                    },
                    count: None,
                }
            })
            .collect();

        let layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(desc.label.as_ref()),
                entries: &entries,
            });
        self.bind_group_layouts.insert(layout)
    }

    fn create_bind_group(&mut self, desc: &BindGroupDesc) -> BindGroup {
        let wgpu_layout = self
            .bind_group_layouts
            .get(desc.layout)
            .expect("Invalid bind group layout handle");

        let entries: Vec<wgpu::BindGroupEntry> = desc
            .entries
            .iter()
            .map(|e| match e {
                BindGroupEntry::Buffer {
                    binding,
                    buffer,
                    offset,
                    size,
                } => {
                    let buf = self.buffers.get(*buffer).expect("Invalid buffer handle");
                    wgpu::BindGroupEntry {
                        binding: *binding,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: buf,
                            offset: *offset,
                            size: std::num::NonZeroU64::new(*size),
                        }),
                    }
                }
                BindGroupEntry::Texture { binding, image } => {
                    let img = self.images.get(*image).expect("Invalid image handle");
                    wgpu::BindGroupEntry {
                        binding: *binding,
                        resource: wgpu::BindingResource::TextureView(&img.view),
                    }
                }
                BindGroupEntry::TextureView { binding, view } => {
                    let v = self
                        .image_views
                        .get(*view)
                        .expect("Invalid image view handle");
                    wgpu::BindGroupEntry {
                        binding: *binding,
                        resource: wgpu::BindingResource::TextureView(v),
                    }
                }
                BindGroupEntry::Sampler { binding, sampler } => {
                    let s = self.samplers.get(*sampler).expect("Invalid sampler handle");
                    wgpu::BindGroupEntry {
                        binding: *binding,
                        resource: wgpu::BindingResource::Sampler(s),
                    }
                }
            })
            .collect();

        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(desc.label.as_ref()),
            layout: wgpu_layout,
            entries: &entries,
        });
        self.bind_groups.insert(bg)
    }

    fn create_render_pipeline(&mut self, desc: &RenderPipelineDesc) -> Pipeline {
        let shader = self
            .shaders
            .get(desc.shader)
            .expect("Invalid shader handle");

        // Build wgpu bind group layouts reference
        let bgl_refs: Vec<&wgpu::BindGroupLayout> = desc
            .bind_group_layouts
            .iter()
            .map(|h| {
                self.bind_group_layouts
                    .get(*h)
                    .expect("Invalid bind group layout handle")
            })
            .collect();

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(desc.label.as_ref()),
                bind_group_layouts: &bgl_refs,
                push_constant_ranges: &[],
            });

        // Build vertex buffer layouts
        // We need to keep the attribute arrays alive, so collect them first
        let wgpu_attrs: Vec<Vec<wgpu::VertexAttribute>> = desc
            .vertex_layouts
            .iter()
            .map(|vl| {
                vl.attributes
                    .iter()
                    .map(|a| wgpu::VertexAttribute {
                        format: to_wgpu_vertex_format(a.format),
                        offset: a.offset,
                        shader_location: a.shader_location,
                    })
                    .collect()
            })
            .collect();

        let wgpu_vbls: Vec<wgpu::VertexBufferLayout> = desc
            .vertex_layouts
            .iter()
            .zip(wgpu_attrs.iter())
            .map(|(vl, attrs)| wgpu::VertexBufferLayout {
                array_stride: vl.stride,
                step_mode: match vl.step_mode {
                    VertexStepMode::Vertex => wgpu::VertexStepMode::Vertex,
                    VertexStepMode::Instance => wgpu::VertexStepMode::Instance,
                },
                attributes: attrs,
            })
            .collect();

        let color_targets: Vec<Option<wgpu::ColorTargetState>> = desc
            .color_targets
            .iter()
            .map(|ct| {
                Some(wgpu::ColorTargetState {
                    format: to_wgpu_texture_format(ct.format),
                    blend: ct.blend.as_ref().map(to_wgpu_blend_state),
                    write_mask: wgpu::ColorWrites::ALL,
                })
            })
            .collect();

        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(desc.label.as_ref()),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some(desc.vs_entry),
                    buffers: &wgpu_vbls,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some(desc.fs_entry),
                    targets: &color_targets,
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: to_wgpu_primitive_topology(desc.primitive.topology),
                    strip_index_format: None,
                    front_face: to_wgpu_front_face(desc.primitive.front_face),
                    cull_mode: to_wgpu_cull_mode(desc.primitive.cull_mode),
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: desc
                    .depth_stencil
                    .as_ref()
                    .map(|ds| wgpu::DepthStencilState {
                        format: to_wgpu_texture_format(ds.format),
                        depth_write_enabled: ds.depth_write,
                        depth_compare: to_wgpu_compare_function(ds.depth_compare),
                        stencil: wgpu::StencilState {
                            front: to_wgpu_stencil_face_state(&ds.stencil_front),
                            back: to_wgpu_stencil_face_state(&ds.stencil_back),
                            read_mask: ds.stencil_read_mask,
                            write_mask: ds.stencil_write_mask,
                        },
                        bias: wgpu::DepthBiasState::default(),
                    }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        self.pipelines.insert(pipeline)
    }

    fn create_compute_pipeline(&mut self, desc: &ComputePipelineDesc) -> ComputePipeline {
        let shader = self
            .shaders
            .get(desc.shader)
            .expect("Invalid shader handle");

        let bgl_refs: Vec<&wgpu::BindGroupLayout> = desc
            .bind_group_layouts
            .iter()
            .map(|h| {
                self.bind_group_layouts
                    .get(*h)
                    .expect("Invalid bind group layout handle")
            })
            .collect();

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(desc.label.as_ref()),
                bind_group_layouts: &bgl_refs,
                push_constant_ranges: &[],
            });

        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(desc.label.as_ref()),
                layout: Some(&pipeline_layout),
                module: shader,
                entry_point: Some(desc.entry_point),
                compilation_options: Default::default(),
                cache: None,
            });

        self.compute_pipelines.insert(pipeline)
    }

    // ── Resource destruction ────────────────────────────────────────────

    fn destroy_buffer(&mut self, buf: Buffer) {
        if let Some(b) = self.buffers.remove(buf) {
            b.destroy();
        }
    }

    fn destroy_image(&mut self, img: Image) {
        if let Some(i) = self.images.remove(img) {
            i.texture.destroy();
        }
    }

    fn destroy_image_view(&mut self, view: ImageView) {
        self.image_views.remove(view);
    }

    fn destroy_sampler(&mut self, s: Sampler) {
        self.samplers.remove(s);
    }

    fn destroy_shader(&mut self, s: Shader) {
        self.shaders.remove(s);
    }

    fn destroy_bind_group_layout(&mut self, l: BindGroupLayout) {
        self.bind_group_layouts.remove(l);
    }

    fn destroy_bind_group(&mut self, bg: BindGroup) {
        self.bind_groups.remove(bg);
    }

    fn destroy_pipeline(&mut self, p: Pipeline) {
        self.pipelines.remove(p);
    }

    fn destroy_compute_pipeline(&mut self, p: ComputePipeline) {
        self.compute_pipelines.remove(p);
    }

    // ── Data upload ─────────────────────────────────────────────────────

    fn write_buffer(&self, buf: Buffer, offset: u64, data: &[u8]) {
        let b = self.buffers.get(buf).expect("Invalid buffer handle");
        self.queue.write_buffer(b, offset, data);
    }

    fn write_image(&self, img: Image, data: &[u8], layout: &ImageCopyLayout) {
        let i = self.images.get(img).expect("Invalid image handle");
        let size = i.texture.size();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &i.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: layout.offset,
                bytes_per_row: Some(layout.bytes_per_row),
                rows_per_image: Some(layout.rows_per_image),
            },
            size,
        );
    }

    fn copy_buffer_to_buffer(&mut self, desc: &BufferCopyDesc) {
        let mut frame = self
            .frame
            .take()
            .expect("copy_buffer_to_buffer requires active frame");

        let src = self
            .buffers
            .get(desc.src)
            .expect("Invalid source buffer handle");
        let dst = self
            .buffers
            .get(desc.dst)
            .expect("Invalid destination buffer handle");
        frame
            .encoder
            .copy_buffer_to_buffer(src, desc.src_offset, dst, desc.dst_offset, desc.size);

        self.frame = Some(frame);
    }

    fn copy_buffer_to_image(&mut self, desc: &BufferToImageCopyDesc) {
        let mut frame = self
            .frame
            .take()
            .expect("copy_buffer_to_image requires active frame");

        let src = self
            .buffers
            .get(desc.src)
            .expect("Invalid source buffer handle");
        let dst = self
            .images
            .get(desc.dst)
            .expect("Invalid destination image handle");
        frame.encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: src,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: desc.src_offset,
                    bytes_per_row: Some(desc.bytes_per_row),
                    rows_per_image: Some(desc.rows_per_image),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: &dst.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: desc.width,
                height: desc.height,
                depth_or_array_layers: 1,
            },
        );

        self.frame = Some(frame);
    }

    fn copy_image_to_image(&mut self, desc: &ImageCopyDesc) {
        let mut frame = self
            .frame
            .take()
            .expect("copy_image_to_image requires active frame");

        let src = self
            .images
            .get(desc.src)
            .expect("Invalid source image handle");
        let dst = self
            .images
            .get(desc.dst)
            .expect("Invalid destination image handle");
        frame.encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &src.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &dst.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: desc.width,
                height: desc.height,
                depth_or_array_layers: 1,
            },
        );

        self.frame = Some(frame);
    }

    // ── Frame rendering ─────────────────────────────────────────────────

    fn begin_frame(&mut self) -> Result<(), GpuError> {
        let surface_texture = match self.surface.get_current_texture() {
            Ok(st) => st,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.surface_config);
                return Err(GpuError::SurfaceLost);
            }
            Err(wgpu::SurfaceError::OutOfMemory) => return Err(GpuError::OutOfMemory),
            Err(e) => return Err(GpuError::Other(e.to_string())),
        };
        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame_encoder"),
            });
        self.frame = Some(FrameState {
            surface_texture,
            surface_view,
            encoder,
        });
        Ok(())
    }

    fn with_render_pass<F>(&mut self, desc: &RenderPassDesc, f: F)
    where
        F: FnOnce(&mut dyn RenderPassEncoder),
    {
        let mut frame = self
            .frame
            .take()
            .expect("with_render_pass requires active frame");

        let attachments: Vec<Option<wgpu::RenderPassColorAttachment<'_>>> = desc
            .color_attachments
            .iter()
            .map(|attachment| {
                let view = match attachment.target {
                    ColorTarget::Surface => &frame.surface_view,
                    ColorTarget::Image(image) => {
                        &self.images.get(image).expect("Invalid image handle").view
                    }
                };
                let load = match attachment.clear {
                    Some(c) => wgpu::LoadOp::Clear(wgpu::Color {
                        r: c[0] as f64,
                        g: c[1] as f64,
                        b: c[2] as f64,
                        a: c[3] as f64,
                    }),
                    None => wgpu::LoadOp::Load,
                };

                Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })
            })
            .collect();

        {
            let depth_stencil_attachment = desc.depth_stencil.as_ref().map(|ds| {
                let view = &self
                    .images
                    .get(ds.image)
                    .expect("Invalid depth image handle")
                    .view;
                wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: match ds.clear_depth {
                            Some(val) => wgpu::LoadOp::Clear(val),
                            None => wgpu::LoadOp::Load,
                        },
                        store: if ds.depth_store {
                            wgpu::StoreOp::Store
                        } else {
                            wgpu::StoreOp::Discard
                        },
                    }),
                    stencil_ops: if ds.clear_stencil.is_some() || ds.stencil_store {
                        Some(wgpu::Operations {
                            load: match ds.clear_stencil {
                                Some(val) => wgpu::LoadOp::Clear(val),
                                None => wgpu::LoadOp::Load,
                            },
                            store: if ds.stencil_store {
                                wgpu::StoreOp::Store
                            } else {
                                wgpu::StoreOp::Discard
                            },
                        })
                    } else {
                        None
                    },
                }
            });

            let mut pass = frame
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(desc.label.as_ref()),
                    color_attachments: &attachments,
                    depth_stencil_attachment,
                    ..Default::default()
                });
            let mut encoder = WgpuRenderPassEncoder {
                pass: &mut pass,
                pipelines: &self.pipelines,
                bind_groups: &self.bind_groups,
                buffers: &self.buffers,
            };
            f(&mut encoder);
        }

        self.frame = Some(frame);
    }

    fn with_compute_pass<F>(&mut self, desc: &ComputePassDesc, f: F)
    where
        F: FnOnce(&mut dyn ComputePassEncoder),
    {
        let mut frame = self
            .frame
            .take()
            .expect("with_compute_pass requires active frame");

        {
            let mut pass = frame
                .encoder
                .begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some(desc.label.as_ref()),
                    timestamp_writes: None,
                });
            let mut encoder = WgpuComputePassEncoder {
                pass: &mut pass,
                compute_pipelines: &self.compute_pipelines,
                bind_groups: &self.bind_groups,
                buffers: &self.buffers,
            };
            f(&mut encoder);
        }

        self.frame = Some(frame);
    }

    fn end_frame(&mut self) {
        let frame = self
            .frame
            .take()
            .expect("end_frame called without begin_frame");
        self.queue.submit(std::iter::once(frame.encoder.finish()));
        frame.surface_texture.present();
    }

    // ── Surface / info ──────────────────────────────────────────────────

    fn resize_surface(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.surface_config.width = width;
            self.surface_config.height = height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }

    fn surface_size(&self) -> [u32; 2] {
        [self.surface_config.width, self.surface_config.height]
    }

    fn surface_format(&self) -> TextureFormat {
        from_wgpu_texture_format(self.surface_config.format)
    }

    fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    fn backend_name(&self) -> &str {
        &self.backend_name
    }
}

impl Drop for WgpuBackend {
    fn drop(&mut self) {
        // Deterministic GPU resource cleanup in reverse-dependency order.
        //
        // Bind groups reference pipelines, buffers, textures, and samplers.
        // Pipelines reference shaders and bind group layouts.
        // Image views reference images.
        //
        // wgpu objects are internally Arc-refcounted, so drop order doesn't
        // strictly matter for correctness — but explicit teardown in
        // dependency order keeps the validation layer quiet and makes the
        // intent clear.

        // 1. Bind groups (reference everything else)
        let _: usize = self.bind_groups.drain().count();

        // 2. Pipelines (reference shaders, bind group layouts)
        let _: usize = self.pipelines.drain().count();
        let _: usize = self.compute_pipelines.drain().count();

        // 3. Bind group layouts
        let _: usize = self.bind_group_layouts.drain().count();

        // 4. Shaders
        let _: usize = self.shaders.drain().count();

        // 5. Image views (reference images)
        let _: usize = self.image_views.drain().count();

        // 6. Samplers
        let _: usize = self.samplers.drain().count();

        // 7. Images — explicit destroy releases GPU memory immediately
        for img in self.images.drain() {
            img.texture.destroy();
        }

        // 8. Buffers — explicit destroy releases GPU memory immediately
        for buf in self.buffers.drain() {
            buf.destroy();
        }
    }
}
