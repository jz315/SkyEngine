use std::any::TypeId;
use std::hash::{Hash, Hasher};

use crate::render::resources::mesh::VertexLayout;

use super::{
    MaterialError, MaterialModel, MaterialRegistry, MaterialRenderState, MaterialStorage,
    MaterialStorageMut, PreparedMaterial, ShaderSource, TypedMaterialHandle,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneBindingKind {
    GpuTable(TypeId),
    ShadowView,
    GlobalIllumination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneBindingDesc {
    pub slot: u32,
    pub kind: SceneBindingKind,
}

impl SceneBindingDesc {
    pub fn gpu_table<T>(slot: u32) -> Self
    where
        T: crate::render::GpuTable + 'static,
    {
        Self {
            slot,
            kind: SceneBindingKind::GpuTable(TypeId::of::<T>()),
        }
    }

    #[inline]
    pub const fn shadow_view(slot: u32) -> Self {
        Self {
            slot,
            kind: SceneBindingKind::ShadowView,
        }
    }

    #[inline]
    pub const fn global_illumination(slot: u32) -> Self {
        Self {
            slot,
            kind: SceneBindingKind::GlobalIllumination,
        }
    }
}

pub trait MaterialModelExt: MaterialModel {
    fn pipeline_key(data: &Self::Data) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        Self::shader_source(data).hash(&mut hasher);
        Self::vertex_layout(data).hash(&mut hasher);
        Self::render_state(data).hash(&mut hasher);
        Self::vertex_entry(data).hash(&mut hasher);
        Self::fragment_entry(data).hash(&mut hasher);
        Self::variant(
            data,
            &super::MaterialVariantContext {
                interface: &Self::interface(),
            },
        )
        .hash(&mut hasher);
        hasher.finish()
    }

    #[inline]
    fn is_transparent(data: &Self::Data) -> bool {
        Self::render_state(data).blend.is_some()
    }

    #[inline]
    fn scene_bindings(data: &Self::Data) -> Vec<SceneBindingDesc> {
        <Self as MaterialModel>::scene_bindings(data)
    }

    #[inline]
    fn scene_prepass_shader_source(data: &Self::Data) -> Option<ShaderSource> {
        <Self as MaterialModel>::scene_prepass_shader_source(data)
    }

    #[inline]
    fn scene_prepass_vertex_layout(data: &Self::Data) -> VertexLayout {
        <Self as MaterialModel>::scene_prepass_vertex_layout(data)
    }

    #[inline]
    fn scene_prepass_vertex_entry(data: &Self::Data) -> &'static str {
        <Self as MaterialModel>::scene_prepass_vertex_entry(data)
    }

    #[inline]
    fn scene_prepass_fragment_entry(data: &Self::Data) -> &'static str {
        <Self as MaterialModel>::scene_prepass_fragment_entry(data)
    }

    fn scene_prepass_pipeline_key(data: &Self::Data) -> Option<u64> {
        let shader = <Self as MaterialModelExt>::scene_prepass_shader_source(data)?;
        let mut hasher = rustc_hash::FxHasher::default();
        shader.hash(&mut hasher);
        <Self as MaterialModelExt>::scene_prepass_vertex_layout(data).hash(&mut hasher);
        <Self as MaterialModelExt>::scene_prepass_vertex_entry(data).hash(&mut hasher);
        <Self as MaterialModelExt>::scene_prepass_fragment_entry(data).hash(&mut hasher);
        Some(hasher.finish())
    }
}

impl<T: MaterialModel> MaterialModelExt for T {}

impl MaterialRegistry {
    pub fn register_material<M: MaterialModel>(
        &mut self,
        device: &wgpu::Device,
    ) -> Result<super::MaterialModelId, MaterialError> {
        self.register_model::<M>(device)
    }

    #[inline]
    pub fn ensure_storage<M: MaterialModel>(&mut self) -> MaterialStorageMut<'_, M> {
        self.materials_mut::<M>()
    }

    pub fn materials_and_pipeline_cache<M: MaterialModel>(
        &mut self,
    ) -> Result<(MaterialStorage<'_, M>, &mut super::PipelineCache), MaterialError> {
        if !self.is_registered::<M>() {
            return Err(MaterialError::UnregisteredMaterialModel {
                type_name: std::any::type_name::<M>(),
            });
        }
        let registry = self as *mut MaterialRegistry;
        // The typed storage view is immutable and only reads instance data while
        // callers mutate the independent pipeline cache. This mirrors the old
        // API shape during the migration away from typed storages.
        let storage = unsafe { MaterialStorage::new(&*registry) };
        Ok((storage, self.pipeline_cache_mut()))
    }

    pub fn prepared_for_typed<M: MaterialModel>(
        &self,
        handle: TypedMaterialHandle<M>,
    ) -> Result<&PreparedMaterial, MaterialError> {
        self.prepared(handle.erased())
    }
}

pub(crate) fn dynamic_render_state<M: MaterialModel>(data: &M::Data) -> MaterialRenderState {
    M::render_state(data)
}
