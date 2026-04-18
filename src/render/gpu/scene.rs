use std::any::TypeId;

use crate::gpu::GpuContext;
use crate::render::view::ViewUniform;
use crate::render::{GpuTable, GpuTableManager, LightTable, ModelMatrixTable};

pub struct GpuScene {
    table_manager: GpuTableManager,
    view_buffer: wgpu::Buffer,
    view_bind_group_layout: wgpu::BindGroupLayout,
    view_bind_group: wgpu::BindGroup,
}

impl GpuScene {
    pub fn new(ctx: &GpuContext) -> Self {
        let view_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_scene_view_uniforms"),
            size: std::mem::size_of::<ViewUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view_bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("gpu_scene_view_bgl"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size:
                                Some(
                                    std::num::NonZeroU64::new(
                                        std::mem::size_of::<ViewUniform>() as u64
                                    )
                                    .expect("ViewUniform has non-zero size"),
                                ),
                        },
                        count: None,
                    }],
                });
        let view_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu_scene_view_bg"),
            layout: &view_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_buffer.as_entire_binding(),
            }],
        });

        let mut table_manager = GpuTableManager::new();
        table_manager.register(ModelMatrixTable::new(ctx));
        table_manager.register(LightTable::new(ctx));

        Self {
            table_manager,
            view_buffer,
            view_bind_group_layout,
            view_bind_group,
        }
    }

    pub fn register<T>(&mut self, table: T)
    where
        T: GpuTable + 'static,
    {
        self.table_manager.register(table);
    }

    pub fn register_boxed(&mut self, table: Box<dyn GpuTable>) {
        self.table_manager.register_boxed(table);
    }

    pub fn table<T>(&self) -> &T
    where
        T: GpuTable + 'static,
    {
        self.table_manager.table::<T>()
    }

    pub(crate) fn try_table_by_type_id(&self, type_id: TypeId) -> Option<&dyn GpuTable> {
        self.table_manager.try_table_by_type_id(type_id)
    }

    pub fn table_mut<T>(&mut self) -> &mut T
    where
        T: GpuTable + 'static,
    {
        self.table_manager.table_mut::<T>()
    }

    pub fn write_view_uniform(&self, queue: &wgpu::Queue, uniform: &ViewUniform) {
        queue.write_buffer(&self.view_buffer, 0, bytemuck::bytes_of(uniform));
    }

    pub fn upload_all(&mut self, queue: &wgpu::Queue) {
        self.table_manager.upload_all(queue);
    }

    #[inline]
    pub fn view_bind_group(&self) -> &wgpu::BindGroup {
        &self.view_bind_group
    }

    #[inline]
    pub fn view_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.view_bind_group_layout
    }

    #[inline]
    pub fn model_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        self.table::<ModelMatrixTable>().bind_group_layout()
    }
}
