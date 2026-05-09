use std::any::{Any, TypeId};
use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;
use crate::render::gpu::Texture;

use super::instance::MaterialInstanceRecord;
use super::{
    ErasedMaterialHandle, MaterialDebugSummary, MaterialError, MaterialInstanceDebugInfo,
    MaterialInstanceId, MaterialInstanceInfo, MaterialInstanceVersion, MaterialInterface,
    MaterialModel, MaterialModelDebugInfo, MaterialModelId, MaterialPipelineKey,
    MaterialPrepareContext, MaterialVariantContext, PipelineCache, PreparedMaterial,
    ShaderVariantKey, TypedMaterialHandle,
};

struct ModelRecord {
    id: MaterialModelId,
    type_id: TypeId,
    type_name: &'static str,
    interface: MaterialInterface,
    bind_group_layout: wgpu::BindGroupLayout,
    prepare: PrepareFn,
    variant: VariantFn,
}

type PrepareFn =
    fn(&dyn Any, &mut MaterialPrepareContext<'_>) -> Result<PreparedMaterial, MaterialError>;
type VariantFn =
    fn(&dyn Any, &MaterialVariantContext<'_>) -> Result<ShaderVariantKey, MaterialError>;

/// Registry owning material models, instances, prepared state, and pipeline
/// cache coordination.
pub struct MaterialRegistry {
    models: Vec<Option<ModelRecord>>,
    model_generations: Vec<u32>,
    model_by_type: FxHashMap<TypeId, MaterialModelId>,
    instances: Vec<Option<MaterialInstanceRecord>>,
    instance_generations: Vec<u32>,
    free_instances: Vec<u32>,
    dirty: Vec<MaterialInstanceId>,
    pipeline_cache: PipelineCache,
}

impl MaterialRegistry {
    pub fn new() -> Self {
        Self {
            models: Vec::new(),
            model_generations: Vec::new(),
            model_by_type: FxHashMap::default(),
            instances: Vec::new(),
            instance_generations: Vec::new(),
            free_instances: Vec::new(),
            dirty: Vec::new(),
            pipeline_cache: PipelineCache::new(),
        }
    }

    pub fn register_model<M: MaterialModel>(
        &mut self,
        device: &wgpu::Device,
    ) -> Result<MaterialModelId, MaterialError> {
        let type_id = TypeId::of::<M>();
        if let Some(id) = self.model_by_type.get(&type_id).copied() {
            return Ok(id);
        }

        let interface = M::interface();
        interface.validate()?;

        let index = self.models.len() as u32;
        let id = MaterialModelId::new(index, 0);
        let bind_group_layout = interface
            .bindings
            .create_bind_group_layout(device, format!("{}_material_bgl", interface.name));
        self.pipeline_cache.register_layout::<M>(device);
        self.models.push(Some(ModelRecord {
            id,
            type_id,
            type_name: std::any::type_name::<M>(),
            interface,
            bind_group_layout,
            prepare: prepare_erased::<M>,
            variant: variant_erased::<M>,
        }));
        self.model_generations.push(0);
        self.model_by_type.insert(type_id, id);
        Ok(id)
    }

    #[inline]
    pub fn model_id<M: MaterialModel>(&self) -> Option<MaterialModelId> {
        self.model_by_type.get(&TypeId::of::<M>()).copied()
    }

    #[inline]
    pub fn is_registered<M: MaterialModel>(&self) -> bool {
        self.model_id::<M>().is_some()
    }

    pub fn interface<M: MaterialModel>(&self) -> Option<&MaterialInterface> {
        let id = self.model_id::<M>()?;
        self.model_record(id).ok().map(|record| &record.interface)
    }

    pub fn insert<M: MaterialModel>(
        &mut self,
        data: M::Data,
    ) -> Result<TypedMaterialHandle<M>, MaterialError> {
        let model = self
            .model_id::<M>()
            .ok_or(MaterialError::UnregisteredMaterialModel {
                type_name: std::any::type_name::<M>(),
            })?;
        let index = if let Some(index) = self.free_instances.pop() {
            index
        } else {
            let index = self.instances.len() as u32;
            self.instances.push(None);
            self.instance_generations.push(0);
            index
        };
        let id = MaterialInstanceId::new(index, self.instance_generations[index as usize]);
        let record = MaterialInstanceRecord {
            id,
            model,
            model_type: TypeId::of::<M>(),
            version: MaterialInstanceVersion::new(1),
            data: Box::new(data),
            prepared: None,
            last_prepared_version: None,
            last_variant: None,
            debug_label: None,
        };
        self.instances[index as usize] = Some(record);
        self.mark_dirty(id);
        Ok(TypedMaterialHandle::new(model, id))
    }

    pub fn insert_with_label<M: MaterialModel>(
        &mut self,
        data: M::Data,
        label: impl Into<Cow<'static, str>>,
    ) -> Result<TypedMaterialHandle<M>, MaterialError> {
        let handle = self.insert::<M>(data)?;
        if let Some(record) = self.instance_record_mut(handle.erased())? {
            record.debug_label = Some(label.into());
        }
        Ok(handle)
    }

    #[inline]
    pub fn try_materials<M: MaterialModel>(&self) -> Option<super::MaterialStorage<'_, M>> {
        self.is_registered::<M>()
            .then(|| super::MaterialStorage::new(self))
    }

    #[inline]
    pub fn materials<M: MaterialModel>(&self) -> super::MaterialStorage<'_, M> {
        self.try_materials::<M>()
            .expect("requested material model has not been registered")
    }

    #[inline]
    pub fn try_materials_mut<M: MaterialModel>(
        &mut self,
    ) -> Option<super::MaterialStorageMut<'_, M>> {
        self.is_registered::<M>()
            .then(|| super::MaterialStorageMut::new(self))
    }

    #[inline]
    pub fn materials_mut<M: MaterialModel>(&mut self) -> super::MaterialStorageMut<'_, M> {
        self.try_materials_mut::<M>()
            .expect("requested material model has not been registered")
    }

    pub fn get<M: MaterialModel>(
        &self,
        handle: TypedMaterialHandle<M>,
    ) -> Result<&M::Data, MaterialError> {
        let record = self.instance_record(handle.erased())?;
        if handle.model_id().index() == u32::MAX {
            return record.data.downcast_ref::<M::Data>().ok_or(
                MaterialError::DowncastMaterialData {
                    model: std::any::type_name::<M>(),
                },
            );
        }
        record
            .data
            .downcast_ref::<M::Data>()
            .ok_or(MaterialError::DowncastMaterialData {
                model: std::any::type_name::<M>(),
            })
    }

    pub fn set<M, F>(
        &mut self,
        handle: TypedMaterialHandle<M>,
        update: F,
    ) -> Result<(), MaterialError>
    where
        M: MaterialModel,
        F: FnOnce(&mut M::Data),
    {
        let id = handle.id();
        let record = self.instance_record_mut_required(handle.erased())?;
        let data =
            record
                .data
                .downcast_mut::<M::Data>()
                .ok_or(MaterialError::DowncastMaterialData {
                    model: std::any::type_name::<M>(),
                })?;
        update(data);
        record.version.bump();
        self.mark_dirty(id);
        Ok(())
    }

    pub fn remove<M: MaterialModel>(
        &mut self,
        handle: TypedMaterialHandle<M>,
    ) -> Result<M::Data, MaterialError> {
        let erased = handle.erased();
        self.validate_erased_handle(erased)?;
        let slot = erased.id().index() as usize;
        let record = self.instances[slot]
            .take()
            .ok_or(MaterialError::StaleMaterialHandle { id: erased.id() })?;
        self.instance_generations[slot] = self.instance_generations[slot].wrapping_add(1);
        self.free_instances.push(erased.id().index());
        self.dirty.retain(|id| *id != erased.id());
        record
            .data
            .downcast::<M::Data>()
            .map(|boxed| *boxed)
            .map_err(|_| MaterialError::DowncastMaterialData {
                model: std::any::type_name::<M>(),
            })
    }

    pub(crate) fn clear_model_instances<M: MaterialModel>(&mut self) {
        let Some(model) = self.model_id::<M>() else {
            return;
        };
        for index in 0..self.instances.len() {
            let remove = self.instances[index]
                .as_ref()
                .is_some_and(|record| record.model == model);
            if remove {
                self.instances[index] = None;
                self.instance_generations[index] = self.instance_generations[index].wrapping_add(1);
                self.free_instances.push(index as u32);
            }
        }
        self.dirty.retain(|id| {
            self.instances
                .get(id.index() as usize)
                .and_then(Option::as_ref)
                .is_some()
        });
    }

    pub fn prepared(
        &self,
        handle: ErasedMaterialHandle,
    ) -> Result<&PreparedMaterial, MaterialError> {
        let record = self.instance_record(handle)?;
        record
            .prepared
            .as_ref()
            .ok_or(MaterialError::MissingPreparedMaterial { id: handle.id() })
    }

    pub fn prepare_dirty(
        &mut self,
        gpu: &GpuContext,
        fallback_texture: Option<&Texture>,
    ) -> Result<(), MaterialError> {
        let dirty = std::mem::take(&mut self.dirty);
        for id in dirty {
            let Some(slot) = self.instances.get(id.index() as usize) else {
                continue;
            };
            let Some(record) = slot.as_ref() else {
                continue;
            };
            if record.id != id || !record.is_dirty() {
                continue;
            }

            let model_index = record.model.index() as usize;
            let model = self
                .models
                .get(model_index)
                .and_then(Option::as_ref)
                .ok_or(MaterialError::UnregisteredMaterialModelId { id: record.model })?;
            let variant_ctx = MaterialVariantContext {
                interface: &model.interface,
            };
            let variant = (model.variant)(record.data.as_ref(), &variant_ctx)?;
            let mut prepare_ctx = MaterialPrepareContext::new(
                gpu.device(),
                gpu.sampler_linear(),
                gpu.sampler_nearest(),
                &model.bind_group_layout,
                fallback_texture,
                &model.interface,
                record.version,
                variant.clone(),
                record.debug_label.clone(),
            );
            let prepared = (model.prepare)(record.data.as_ref(), &mut prepare_ctx)?;
            let record = self
                .instances
                .get_mut(id.index() as usize)
                .and_then(Option::as_mut)
                .ok_or(MaterialError::StaleMaterialHandle { id })?;
            record.last_variant = Some(variant);
            record.last_prepared_version = Some(record.version);
            record.prepared = Some(prepared);
        }
        Ok(())
    }

    pub fn pipeline_key_for<M: MaterialModel>(
        &self,
        handle: TypedMaterialHandle<M>,
        mesh_layout: &crate::render::resources::mesh::VertexLayout,
        scene_layout: u64,
        target_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        sample_count: u32,
    ) -> Result<MaterialPipelineKey, MaterialError> {
        let record = self.instance_record(handle.erased())?;
        let model = self.model_record(record.model)?;
        let variant = record.last_variant.clone().unwrap_or_else(|| {
            M::variant(
                record
                    .data
                    .downcast_ref::<M::Data>()
                    .expect("typed handle matched material data"),
                &MaterialVariantContext {
                    interface: &model.interface,
                },
            )
        });
        Ok(MaterialPipelineKey::new(
            model.id,
            &model.interface,
            &variant,
            model.interface.passes.main as u64,
            mesh_layout,
            scene_layout,
            target_format,
            depth_format,
            sample_count,
        ))
    }

    #[inline]
    pub fn pipeline_cache(&self) -> &PipelineCache {
        &self.pipeline_cache
    }

    #[inline]
    pub fn pipeline_cache_mut(&mut self) -> &mut PipelineCache {
        &mut self.pipeline_cache
    }

    pub fn instance_info(
        &self,
        handle: ErasedMaterialHandle,
    ) -> Result<MaterialInstanceInfo, MaterialError> {
        let record = self.instance_record(handle)?;
        Ok(MaterialInstanceInfo {
            id: record.id,
            model: record.model,
            version: record.version,
            prepared_version: record.last_prepared_version,
            selected_variant: record.last_variant.clone(),
            debug_label: record.debug_label.clone(),
        })
    }

    pub fn debug_summary(&self) -> MaterialDebugSummary {
        let models = self
            .models
            .iter()
            .filter_map(|model| model.as_ref())
            .map(|model| MaterialModelDebugInfo {
                id: model.id,
                name: model.interface.name,
                interface: model.interface.clone(),
                instance_count: self
                    .instances
                    .iter()
                    .filter_map(|record| record.as_ref())
                    .filter(|record| record.model == model.id)
                    .count(),
            })
            .collect();
        let instances = self
            .instances
            .iter()
            .filter_map(|record| record.as_ref())
            .map(|record| MaterialInstanceDebugInfo {
                id: record.id,
                model: record.model,
                version: record.version,
                prepared_version: record.last_prepared_version,
                selected_variant: record.last_variant.clone(),
                debug_label: record.debug_label.clone(),
            })
            .collect();
        MaterialDebugSummary {
            models,
            instances,
            pipeline_count: self.pipeline_cache.pipeline_count(),
        }
    }

    fn model_record(&self, id: MaterialModelId) -> Result<&ModelRecord, MaterialError> {
        if self.model_generations.get(id.index() as usize).copied() != Some(id.generation()) {
            return Err(MaterialError::UnregisteredMaterialModelId { id });
        }
        self.models
            .get(id.index() as usize)
            .and_then(Option::as_ref)
            .ok_or(MaterialError::UnregisteredMaterialModelId { id })
    }

    fn validate_erased_handle(&self, handle: ErasedMaterialHandle) -> Result<(), MaterialError> {
        let record = self.instance_record(handle)?;
        if record.model_type != handle.model_type() {
            return Err(MaterialError::WrongMaterialModel {
                expected: record.model,
                actual: handle.model_id(),
            });
        }
        Ok(())
    }

    fn instance_record(
        &self,
        handle: ErasedMaterialHandle,
    ) -> Result<&MaterialInstanceRecord, MaterialError> {
        let Some(generation) = self.instance_generations.get(handle.id().index() as usize) else {
            return Err(MaterialError::StaleMaterialHandle { id: handle.id() });
        };
        if *generation != handle.id().generation() {
            return Err(MaterialError::StaleMaterialHandle { id: handle.id() });
        }
        let record = self
            .instances
            .get(handle.id().index() as usize)
            .and_then(Option::as_ref)
            .ok_or(MaterialError::StaleMaterialHandle { id: handle.id() })?;
        if handle.model_id().index() != u32::MAX && record.model != handle.model_id() {
            return Err(MaterialError::WrongMaterialModel {
                expected: record.model,
                actual: handle.model_id(),
            });
        }
        if record.model_type != handle.model_type() {
            return Err(MaterialError::WrongMaterialModel {
                expected: record.model,
                actual: handle.model_id(),
            });
        }
        Ok(record)
    }

    fn instance_record_mut(
        &mut self,
        handle: ErasedMaterialHandle,
    ) -> Result<Option<&mut MaterialInstanceRecord>, MaterialError> {
        match self.instance_record(handle) {
            Ok(_) => Ok(self
                .instances
                .get_mut(handle.id().index() as usize)
                .and_then(Option::as_mut)),
            Err(error) => Err(error),
        }
    }

    fn instance_record_mut_required(
        &mut self,
        handle: ErasedMaterialHandle,
    ) -> Result<&mut MaterialInstanceRecord, MaterialError> {
        self.instance_record(handle)?;
        self.instances
            .get_mut(handle.id().index() as usize)
            .and_then(Option::as_mut)
            .ok_or(MaterialError::StaleMaterialHandle { id: handle.id() })
    }

    fn mark_dirty(&mut self, id: MaterialInstanceId) {
        if !self.dirty.contains(&id) {
            self.dirty.push(id);
        }
    }
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn prepare_erased<M: MaterialModel>(
    data: &dyn Any,
    ctx: &mut MaterialPrepareContext<'_>,
) -> Result<PreparedMaterial, MaterialError> {
    let data = data
        .downcast_ref::<M::Data>()
        .ok_or(MaterialError::DowncastMaterialData {
            model: std::any::type_name::<M>(),
        })?;
    M::prepare(data, ctx)
}

fn variant_erased<M: MaterialModel>(
    data: &dyn Any,
    ctx: &MaterialVariantContext<'_>,
) -> Result<ShaderVariantKey, MaterialError> {
    let data = data
        .downcast_ref::<M::Data>()
        .ok_or(MaterialError::DowncastMaterialData {
            model: std::any::type_name::<M>(),
        })?;
    Ok(M::variant(data, ctx))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use super::*;
    use crate::render::resources::material::{
        MainPassMode, MaterialBinding, MaterialInterface, MaterialRenderState, MaterialShaderSet,
        SceneResourceRequirements,
    };

    #[derive(Clone)]
    struct TestMaterial {
        value: f32,
        variant: u64,
    }

    struct TestModel;

    impl MaterialModel for TestModel {
        type Data = TestMaterial;

        fn interface() -> MaterialInterface {
            MaterialInterface::builder("test")
                .shader(MaterialShaderSet::wgsl("@vertex fn vs_main() -> @builtin(position) vec4<f32> { return vec4<f32>(); }\n@fragment fn fs_main() -> @location(0) vec4<f32> { return vec4<f32>(); }"))
                .scene(SceneResourceRequirements::new().camera())
                .binding(MaterialBinding::uniform(
                    0,
                    NonZeroU64::new(16).expect("non-zero"),
                ))
                .main_pass(MainPassMode::Opaque)
                .render_state(MaterialRenderState::opaque())
                .build()
        }

        fn variant(data: &Self::Data, _ctx: &MaterialVariantContext<'_>) -> ShaderVariantKey {
            ShaderVariantKey::new().with("variant", data.variant)
        }

        fn prepare(
            _data: &Self::Data,
            ctx: &mut MaterialPrepareContext<'_>,
        ) -> Result<PreparedMaterial, MaterialError> {
            #[repr(C)]
            #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
            struct Uniform {
                value: [f32; 4],
            }
            ctx.bindings()
                .uniform(0, "test_uniform", &Uniform { value: [1.0; 4] })
                .build()
        }
    }

    #[derive(Clone)]
    struct OtherMaterial;

    struct OtherModel;

    impl MaterialModel for OtherModel {
        type Data = OtherMaterial;

        fn interface() -> MaterialInterface {
            MaterialInterface::builder("other")
                .shader(MaterialShaderSet::wgsl("@vertex fn vs_main() -> @builtin(position) vec4<f32> { return vec4<f32>(); }\n@fragment fn fs_main() -> @location(0) vec4<f32> { return vec4<f32>(); }"))
                .build()
        }

        fn prepare(
            _data: &Self::Data,
            ctx: &mut MaterialPrepareContext<'_>,
        ) -> Result<PreparedMaterial, MaterialError> {
            let bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("other_bg"),
                layout: ctx.layout(),
                entries: &[],
            });
            Ok(PreparedMaterial::new(
                bind_group,
                Vec::new(),
                ShaderVariantKey::default(),
                MaterialInstanceVersion::new(1),
                None,
            ))
        }
    }

    fn create_test_device() -> GpuContext {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for material tests");

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("material_registry_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device");
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [16, 16])
    }

    #[test]
    fn interface_validation_rejects_duplicate_bindings() {
        let interface = MaterialInterface::builder("bad")
            .shader(MaterialShaderSet::wgsl("@vertex fn vs_main() -> @builtin(position) vec4<f32> { return vec4<f32>(); }\n@fragment fn fs_main() -> @location(0) vec4<f32> { return vec4<f32>(); }"))
            .binding(MaterialBinding::sampler(0))
            .binding(MaterialBinding::texture_2d(0))
            .build();

        assert!(matches!(
            interface.validate(),
            Err(MaterialError::DuplicateBinding {
                model: "bad",
                binding: 0
            })
        ));
    }

    #[test]
    fn typed_handles_reject_wrong_model_use() {
        let gpu = create_test_device();
        let mut registry = MaterialRegistry::new();
        registry.register_model::<TestModel>(gpu.device()).unwrap();
        registry.register_model::<OtherModel>(gpu.device()).unwrap();
        let test = registry
            .insert::<TestModel>(TestMaterial {
                value: 1.0,
                variant: 0,
            })
            .unwrap();
        let other_model = registry.model_id::<OtherModel>().unwrap();
        let wrong = ErasedMaterialHandle::from_ids::<TestModel>(other_model, test.id());

        assert!(matches!(
            registry.prepared(wrong),
            Err(MaterialError::WrongMaterialModel { .. })
        ));
    }

    #[test]
    fn stale_handles_fail_after_removal() {
        let gpu = create_test_device();
        let mut registry = MaterialRegistry::new();
        registry.register_model::<TestModel>(gpu.device()).unwrap();
        let handle = registry
            .insert::<TestModel>(TestMaterial {
                value: 1.0,
                variant: 0,
            })
            .unwrap();
        let _ = registry.remove(handle).unwrap();

        assert!(matches!(
            registry.get(handle),
            Err(MaterialError::StaleMaterialHandle { .. })
        ));
    }

    #[test]
    fn dirty_prepare_reuses_state_when_unchanged() {
        let gpu = create_test_device();
        let fallback = Texture::white_pixel(&gpu);
        let mut registry = MaterialRegistry::new();
        registry.register_model::<TestModel>(gpu.device()).unwrap();
        let handle = registry
            .insert::<TestModel>(TestMaterial {
                value: 1.0,
                variant: 0,
            })
            .unwrap();

        registry.prepare_dirty(&gpu, Some(&fallback)).unwrap();
        let first = registry.instance_info(handle.erased()).unwrap();
        registry.prepare_dirty(&gpu, Some(&fallback)).unwrap();
        let second = registry.instance_info(handle.erased()).unwrap();

        assert_eq!(first.prepared_version, second.prepared_version);
        assert_eq!(first.selected_variant, second.selected_variant);
    }

    #[test]
    fn uniform_change_keeps_pipeline_key_but_variant_change_updates_it() {
        let gpu = create_test_device();
        let mut registry = MaterialRegistry::new();
        registry.register_model::<TestModel>(gpu.device()).unwrap();
        let handle = registry
            .insert::<TestModel>(TestMaterial {
                value: 1.0,
                variant: 0,
            })
            .unwrap();
        let mesh = crate::render::resources::mesh::Mesh::vertex_layout_position_uv();
        let first = registry
            .pipeline_key_for(handle, &mesh, 0, wgpu::TextureFormat::Bgra8Unorm, None, 1)
            .unwrap();
        registry.set(handle, |data| data.value = 2.0).unwrap();
        let second = registry
            .pipeline_key_for(handle, &mesh, 0, wgpu::TextureFormat::Bgra8Unorm, None, 1)
            .unwrap();
        registry.set(handle, |data| data.variant = 1).unwrap();
        let third = registry
            .pipeline_key_for(handle, &mesh, 0, wgpu::TextureFormat::Bgra8Unorm, None, 1)
            .unwrap();

        assert_eq!(first, second);
        assert_ne!(second, third);
    }
}
