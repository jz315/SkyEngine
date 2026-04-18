use crate::gpu::GpuContext;

const INITIAL_MODEL_CAPACITY: usize = 64;

fn align_up(value: u32, alignment: u32) -> u32 {
    if alignment <= 1 {
        value
    } else {
        value.div_ceil(alignment) * alignment
    }
}

pub struct ModelMatrixTable {
    buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    matrices: Vec<[f32; 16]>,
    upload_scratch: Vec<u8>,
    stride: u32,
    capacity: usize,
    dirty: bool,
}

impl ModelMatrixTable {
    pub fn new(ctx: &GpuContext) -> Self {
        let stride = align_up(
            std::mem::size_of::<[f32; 16]>() as u32,
            ctx.device()
                .limits()
                .min_uniform_buffer_offset_alignment
                .max(1),
        );
        let bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("model_matrix_table_bgl"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: Some(
                                std::num::NonZeroU64::new(std::mem::size_of::<[f32; 16]>() as u64)
                                    .expect("model matrix uniform has non-zero size"),
                            ),
                        },
                        count: None,
                    }],
                });
        let buffer = create_model_matrix_buffer(ctx, INITIAL_MODEL_CAPACITY, stride);
        let bind_group = create_model_matrix_bind_group(ctx, &bind_group_layout, &buffer);

        Self {
            buffer,
            bind_group_layout,
            bind_group,
            matrices: Vec::with_capacity(INITIAL_MODEL_CAPACITY),
            upload_scratch: Vec::with_capacity(INITIAL_MODEL_CAPACITY * stride as usize),
            stride,
            capacity: INITIAL_MODEL_CAPACITY,
            dirty: false,
        }
    }

    pub fn set_all(&mut self, ctx: &GpuContext, matrices: &[[f32; 16]]) {
        self.ensure_capacity(ctx, matrices.len().max(1));
        self.matrices.clear();
        self.matrices.extend_from_slice(matrices);
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
    pub fn dynamic_offset(&self, slot: u32) -> u32 {
        slot * self.stride
    }

    #[inline]
    pub fn stride(&self) -> u32 {
        self.stride
    }

    fn ensure_capacity(&mut self, ctx: &GpuContext, required: usize) {
        if required <= self.capacity {
            return;
        }

        self.capacity = required.next_power_of_two().max(INITIAL_MODEL_CAPACITY);
        self.buffer = create_model_matrix_buffer(ctx, self.capacity, self.stride);
        self.bind_group =
            create_model_matrix_bind_group(ctx, &self.bind_group_layout, &self.buffer);
        self.dirty = true;
    }

    pub(crate) fn upload(&mut self, queue: &wgpu::Queue) {
        if !self.dirty || self.matrices.is_empty() {
            return;
        }

        let stride = self.stride as usize;
        let matrix_size = std::mem::size_of::<[f32; 16]>();
        self.upload_scratch.clear();
        self.upload_scratch.resize(self.matrices.len() * stride, 0);
        for (index, matrix) in self.matrices.iter().enumerate() {
            let start = index * stride;
            self.upload_scratch[start..start + matrix_size]
                .copy_from_slice(bytemuck::bytes_of(matrix));
        }
        queue.write_buffer(&self.buffer, 0, &self.upload_scratch);
        self.dirty = false;
    }
}

fn create_model_matrix_buffer(ctx: &GpuContext, capacity: usize, stride: u32) -> wgpu::Buffer {
    ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("model_matrix_table"),
        size: (capacity.max(1) as u64) * stride as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_model_matrix_bind_group(
    ctx: &GpuContext,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let matrix_size = std::num::NonZeroU64::new(std::mem::size_of::<[f32; 16]>() as u64)
        .expect("model matrix uniform has non-zero size");
    ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("model_matrix_table_bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: Some(matrix_size),
            }),
        }],
    })
}
