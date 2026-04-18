//! Type-erased registry for registered [`Material`] types, combining typed
//! storage with the pipeline cache.

use std::any::{Any, TypeId};

use rustc_hash::FxHashMap;

use crate::render::gpu::Texture;
use super::traits::{Material, MaterialBindContext};
use super::storage::MaterialStorage;
use super::cache::PipelineCache;
use super::MaterialError;

/// Type-erased registry for registered [`Material`] types.
pub struct MaterialRegistry {
    storages: FxHashMap<TypeId, Box<dyn Any>>,
    pipeline_cache: PipelineCache,
}

impl MaterialRegistry {
    #[inline]
    pub fn new() -> Self {
        Self {
            storages: FxHashMap::default(),
            pipeline_cache: PipelineCache::new(),
        }
    }

    pub fn register_material<M: Material>(&mut self, device: &wgpu::Device) {
        self.storages
            .entry(TypeId::of::<M>())
            .or_insert_with(|| Box::new(MaterialStorage::<M>::new()));
        self.pipeline_cache.register_layout::<M>(device);
    }

    #[inline]
    pub fn is_registered<M: Material>(&self) -> bool {
        self.storages.contains_key(&TypeId::of::<M>())
    }

    pub fn try_materials<M: Material>(&self) -> Option<&MaterialStorage<M>> {
        self.storages
            .get(&TypeId::of::<M>())
            .and_then(|storage| storage.downcast_ref::<MaterialStorage<M>>())
    }

    pub fn materials<M: Material>(&self) -> &MaterialStorage<M> {
        self.try_materials::<M>()
            .expect("requested material storage has not been registered")
    }

    pub fn try_materials_mut<M: Material>(&mut self) -> Option<&mut MaterialStorage<M>> {
        self.storages
            .get_mut(&TypeId::of::<M>())
            .and_then(|storage| storage.downcast_mut::<MaterialStorage<M>>())
    }

    pub fn materials_mut<M: Material>(&mut self) -> &mut MaterialStorage<M> {
        self.try_materials_mut::<M>()
            .expect("requested material storage has not been registered")
    }

    pub fn bind_context<'a, M: Material>(
        &'a self,
        device: &'a wgpu::Device,
        sampler_linear: &'a wgpu::Sampler,
        sampler_nearest: &'a wgpu::Sampler,
        fallback_texture: Option<&'a Texture>,
    ) -> Result<MaterialBindContext<'a>, MaterialError> {
        let layout =
            self.pipeline_cache
                .layout::<M>()
                .ok_or(MaterialError::UnregisteredMaterialType {
                    type_name: std::any::type_name::<M>(),
                })?;
        Ok(MaterialBindContext::new(
            device,
            sampler_linear,
            sampler_nearest,
            layout,
            fallback_texture,
        ))
    }

    pub fn materials_and_pipeline_cache<M: Material>(
        &mut self,
    ) -> Result<(&MaterialStorage<M>, &mut PipelineCache), MaterialError> {
        let storage = self
            .storages
            .get(&TypeId::of::<M>())
            .and_then(|storage| storage.downcast_ref::<MaterialStorage<M>>())
            .ok_or(MaterialError::UnregisteredMaterialType {
                type_name: std::any::type_name::<M>(),
            })?;
        Ok((storage, &mut self.pipeline_cache))
    }

    #[inline]
    pub fn pipeline_cache(&self) -> &PipelineCache {
        &self.pipeline_cache
    }

    #[inline]
    pub fn pipeline_cache_mut(&mut self) -> &mut PipelineCache {
        &mut self.pipeline_cache
    }

    pub fn sync_material_storage<M>(&mut self, source: &MaterialRegistry, device: &wgpu::Device)
    where
        M: Material + Clone,
    {
        self.register_material::<M>(device);
        let dst = self.materials_mut::<M>();
        if let Some(src) = source.try_materials::<M>() {
            dst.clone_from_storage(src);
        } else {
            dst.clear();
        }
    }
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}
