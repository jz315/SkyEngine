use crate::gpu::GpuContext;
use crate::render::core::camera::{CameraUniform, RenderView};

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PositionQuadVertex {
    pos: [f32; 2],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TexturedQuadVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

const POSITION_QUAD_VERTICES: [PositionQuadVertex; 4] = [
    PositionQuadVertex { pos: [0.0, 0.0] },
    PositionQuadVertex { pos: [1.0, 0.0] },
    PositionQuadVertex { pos: [1.0, 1.0] },
    PositionQuadVertex { pos: [0.0, 1.0] },
];

const TEXTURED_QUAD_VERTICES: [TexturedQuadVertex; 4] = [
    TexturedQuadVertex {
        position: [0.0, 0.0],
        uv: [0.0, 1.0],
    },
    TexturedQuadVertex {
        position: [1.0, 0.0],
        uv: [1.0, 1.0],
    },
    TexturedQuadVertex {
        position: [1.0, 1.0],
        uv: [1.0, 0.0],
    },
    TexturedQuadVertex {
        position: [0.0, 1.0],
        uv: [0.0, 0.0],
    },
];

const QUAD_INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

pub(crate) struct QuadGeometry {
    pub(crate) vertex_buffer: wgpu::Buffer,
    pub(crate) index_buffer: wgpu::Buffer,
}

pub(crate) struct CameraBinding {
    pub(crate) buffer: wgpu::Buffer,
    pub(crate) bind_group: wgpu::BindGroup,
    pub(crate) layout: wgpu::BindGroupLayout,
}

impl CameraBinding {
    pub(crate) fn new(ctx: &GpuContext, label_prefix: &str) -> Self {
        let buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label_prefix}_camera_buf")),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(&format!("{label_prefix}_camera_bgl")),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("{label_prefix}_camera_bg")),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            buffer,
            bind_group,
            layout,
        }
    }

    #[inline]
    pub(crate) fn upload_view(&self, ctx: &GpuContext, view: &impl RenderView) {
        ctx.queue()
            .write_buffer(&self.buffer, 0, bytemuck::bytes_of(&view.view_uniform()));
    }
}

pub(crate) fn create_position_quad_geometry(ctx: &GpuContext, label_prefix: &str) -> QuadGeometry {
    create_quad_geometry(
        ctx,
        &format!("{label_prefix}_quad_vb"),
        bytemuck::cast_slice(&POSITION_QUAD_VERTICES),
        &format!("{label_prefix}_quad_ib"),
    )
}

pub(crate) fn create_textured_quad_geometry(ctx: &GpuContext, label_prefix: &str) -> QuadGeometry {
    create_quad_geometry(
        ctx,
        &format!("{label_prefix}_quad_vb"),
        bytemuck::cast_slice(&TEXTURED_QUAD_VERTICES),
        &format!("{label_prefix}_quad_ib"),
    )
}

fn create_quad_geometry(
    ctx: &GpuContext,
    vertex_label: &str,
    vertex_bytes: &[u8],
    index_label: &str,
) -> QuadGeometry {
    let vertex_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some(vertex_label),
        size: vertex_bytes.len() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    ctx.queue().write_buffer(&vertex_buffer, 0, vertex_bytes);

    let index_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some(index_label),
        size: std::mem::size_of_val(&QUAD_INDICES) as u64,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    ctx.queue()
        .write_buffer(&index_buffer, 0, bytemuck::cast_slice(&QUAD_INDICES));

    QuadGeometry {
        vertex_buffer,
        index_buffer,
    }
}
