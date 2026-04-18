mod fullscreen;
pub(crate) mod helpers;
mod model_matrix;
mod scene;
mod target;
mod texture;

use std::any::{Any, TypeId};

use rustc_hash::FxHashMap;

pub use fullscreen::{compose_fullscreen_shader, FullscreenPass, FullscreenPipeline};
pub use model_matrix::ModelMatrixTable;
pub use scene::GpuScene;
pub use target::{is_depth_format, RenderTarget, RenderTargetDescriptor, DEFAULT_DEPTH_FORMAT};
pub use texture::{Texture, TextureCreateDesc, TextureError, TextureFileDesc, TextureUploadDesc};

pub trait GpuTable: Any + Send + Sync {
    fn name(&self) -> &'static str;
    fn upload(&mut self, queue: &wgpu::Queue);
    fn bind_group(&self) -> &wgpu::BindGroup;
    fn bind_group_layout(&self) -> &wgpu::BindGroupLayout;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl GpuTable for ModelMatrixTable {
    fn name(&self) -> &'static str {
        "model_matrices"
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

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Default)]
pub struct GpuTableManager {
    tables: FxHashMap<TypeId, Box<dyn GpuTable>>,
}

impl GpuTableManager {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T>(&mut self, table: T)
    where
        T: GpuTable + 'static,
    {
        self.tables.insert(TypeId::of::<T>(), Box::new(table));
    }

    pub fn register_boxed(&mut self, table: Box<dyn GpuTable>) {
        let type_id = (*table).as_any().type_id();
        self.tables.insert(type_id, table);
    }

    pub fn try_table<T>(&self) -> Option<&T>
    where
        T: GpuTable + 'static,
    {
        self.tables
            .get(&TypeId::of::<T>())
            .and_then(|table| table.as_any().downcast_ref::<T>())
    }

    pub fn try_table_by_type_id(&self, type_id: TypeId) -> Option<&dyn GpuTable> {
        self.tables.get(&type_id).map(|table| &**table)
    }

    pub fn table<T>(&self) -> &T
    where
        T: GpuTable + 'static,
    {
        self.try_table::<T>().unwrap_or_else(|| {
            panic!(
                "GPU table `{}` is not registered",
                std::any::type_name::<T>()
            )
        })
    }

    pub fn try_table_mut<T>(&mut self) -> Option<&mut T>
    where
        T: GpuTable + 'static,
    {
        self.tables
            .get_mut(&TypeId::of::<T>())
            .and_then(|table| table.as_any_mut().downcast_mut::<T>())
    }

    pub fn table_mut<T>(&mut self) -> &mut T
    where
        T: GpuTable + 'static,
    {
        self.try_table_mut::<T>().unwrap_or_else(|| {
            panic!(
                "GPU table `{}` is not registered",
                std::any::type_name::<T>()
            )
        })
    }

    pub fn upload_all(&mut self, queue: &wgpu::Queue) {
        for table in self.tables.values_mut() {
            table.upload(queue);
        }
    }
}
