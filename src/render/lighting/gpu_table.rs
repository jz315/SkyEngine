use crate::gpu::GpuContext;
use crate::render::gpu::GpuTable;

const INITIAL_LIGHT_CAPACITY: usize = 32;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuLight {
    /// Point lights store world-space position.xyz and radius in w.
    /// Directional lights store normalized light direction.xyz and zero radius.
    pub pos_radius: [f32; 4],
    pub color: [f32; 4],
    /// x = falloff for point lights
    /// y = kind (0.0 = point, 1.0 = directional)
    pub falloff: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
struct LightTableMeta {
    count: u32,
    _pad: [u32; 3],
}

pub struct LightTable {
    buffer: wgpu::Buffer,
    meta_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    lights: Vec<GpuLight>,
    capacity: usize,
    dirty: bool,
}

impl LightTable {
    pub fn new(ctx: &GpuContext) -> Self {
        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("light_table_bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: Some(
                                    std::num::NonZeroU64::new(
                                        std::mem::size_of::<LightTableMeta>() as u64,
                                    )
                                    .expect("LightTableMeta has non-zero size"),
                                ),
                            },
                            count: None,
                        },
                    ],
                });
        let buffer = create_light_buffer(ctx, INITIAL_LIGHT_CAPACITY);
        let meta_buffer = create_light_meta_buffer(ctx);
        let bind_group = create_light_bind_group(ctx, &bind_group_layout, &buffer, &meta_buffer);

        Self {
            buffer,
            meta_buffer,
            bind_group_layout,
            bind_group,
            lights: Vec::with_capacity(INITIAL_LIGHT_CAPACITY),
            capacity: INITIAL_LIGHT_CAPACITY,
            dirty: true,
        }
    }

    pub fn set_all(&mut self, ctx: &GpuContext, lights: &[GpuLight]) {
        self.ensure_capacity(ctx, lights.len().max(1));
        self.lights.clear();
        self.lights.extend_from_slice(lights);
        self.dirty = true;
    }

    #[inline]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    #[inline]
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    #[inline]
    pub(crate) fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    #[inline]
    pub(crate) fn meta_buffer(&self) -> &wgpu::Buffer {
        &self.meta_buffer
    }

    fn ensure_capacity(&mut self, ctx: &GpuContext, required: usize) {
        if required <= self.capacity {
            return;
        }

        self.capacity = required.next_power_of_two().max(INITIAL_LIGHT_CAPACITY);
        self.buffer = create_light_buffer(ctx, self.capacity);
        self.bind_group = create_light_bind_group(
            ctx,
            &self.bind_group_layout,
            &self.buffer,
            &self.meta_buffer,
        );
        self.dirty = true;
    }

    pub(crate) fn upload(&mut self, queue: &wgpu::Queue) {
        if !self.dirty {
            return;
        }

        if !self.lights.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.lights));
        }
        let meta = LightTableMeta {
            count: self.lights.len() as u32,
            _pad: [0; 3],
        };
        queue.write_buffer(&self.meta_buffer, 0, bytemuck::bytes_of(&meta));
        self.dirty = false;
    }
}

impl GpuTable for LightTable {
    fn name(&self) -> &'static str {
        "lights"
    }

    fn upload(&mut self, queue: &wgpu::Queue) {
        self.upload(queue);
    }

    fn bind_group(&self) -> &wgpu::BindGroup {
        self.bind_group()
    }

    fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.bind_group_layout()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn create_light_buffer(ctx: &GpuContext, capacity: usize) -> wgpu::Buffer {
    ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("gpu_light_table"),
        size: (capacity.max(1) * std::mem::size_of::<GpuLight>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_light_bind_group(
    ctx: &GpuContext,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    meta_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gpu_light_table_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: meta_buffer.as_entire_binding(),
            },
        ],
    })
}

fn create_light_meta_buffer(ctx: &GpuContext) -> wgpu::Buffer {
    ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("gpu_light_table_meta"),
        size: std::mem::size_of::<LightTableMeta>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
