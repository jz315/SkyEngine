use std::any::TypeId;
use std::collections::BTreeSet;
use std::marker::PhantomData;

use rustc_hash::FxHashMap;

use crate::ecs::EntityId;
use crate::render::resources::material::{Material, MaterialError, SceneBindingKind};
use crate::render::view::ResolvedSceneTransforms;

use super::mesh_instance::mesh_phase_instance_buffer;
use super::scene_bindings::IDENTITY_MODEL;
use super::{
    DrawContext, DrawError, DrawFunction, MeshDrawData, PhaseItem, SceneMaterialPrepassContext,
};

pub struct DrawMesh<M> {
    marker: PhantomData<fn() -> M>,
}

impl<M> DrawMesh<M> {
    #[inline]
    pub fn new() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<M> Default for DrawMesh<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> DrawFunction for DrawMesh<M>
where
    M: Material,
{
    #[inline]
    fn material_type_id(&self) -> Option<TypeId> {
        Some(TypeId::of::<M>())
    }

    #[inline]
    fn supports_scene_material_prepass(&self) -> bool {
        true
    }

    #[allow(private_interfaces)]
    fn draw_scene_material_prepass_batch(
        &mut self,
        ctx: &mut SceneMaterialPrepassContext<'_, '_>,
        items: &[PhaseItem],
    ) -> Result<(), DrawError> {
        if items.is_empty() {
            return Ok(());
        }

        if !ctx.material_registry.is_registered::<M>() {
            return Ok(());
        }
        let mut bind_group_keepalive = Vec::new();
        let mut cursor = 0usize;

        while cursor < items.len() {
            let base = *items[cursor].data::<MeshDrawData>();
            let mesh_handle = base.mesh_handle();
            let material_handle = base.material_handle::<M>();
            let sub_mesh_index = base.sub_mesh_index();
            let mut batch_end = cursor + 1;
            while batch_end < items.len() {
                let next = *items[batch_end].data::<MeshDrawData>();
                if next.mesh_handle() != mesh_handle
                    || next.material_handle::<M>() != material_handle
                    || next.sub_mesh_index() != sub_mesh_index
                {
                    break;
                }
                batch_end += 1;
            }

            let mesh = ctx
                .mesh_registry
                .get(mesh_handle)
                .ok_or(DrawError::MissingMesh {
                    handle: mesh_handle,
                })?;
            let material = ctx
                .material_registry
                .get_erased::<M>(material_handle)
                .map_err(|_| DrawError::MissingMaterial {
                    type_name: std::any::type_name::<M>(),
                })?;
            let Some(pipeline) = ctx.pipeline_cache.ensure_pipeline::<M>(
                ctx.device,
                material,
                ctx.view_bind_group_layout,
                ctx.material_registry.pipeline_cache().layout::<M>().ok_or(
                    MaterialError::UnregisteredMaterialType {
                        type_name: std::any::type_name::<M>(),
                    },
                )?,
                mesh.vertex_layout(),
                ctx.albedo_format,
                ctx.material_format,
                ctx.emissive_format,
                ctx.normal_format,
                ctx.depth_format,
            )?
            else {
                cursor = batch_end;
                continue;
            };
            bind_group_keepalive.push(
                ctx.material_registry
                    .prepared(material_handle)?
                    .bind_group()
                    .clone(),
            );

            let models: Vec<[f32; 16]> = items[cursor..batch_end]
                .iter()
                .map(|item| {
                    let mesh_data = *item.data::<MeshDrawData>();
                    *ctx.cpu_model_matrices
                        .and_then(|matrices| matrices.get(mesh_data.model_slot() as usize))
                        .unwrap_or(&IDENTITY_MODEL)
                })
                .collect();
            let instance_buffer = mesh_phase_instance_buffer(
                ctx.device,
                "scene_material_prepass_instance_buffer",
                &models,
            );

            ctx.pass.set_pipeline(&pipeline);
            ctx.pass.set_bind_group(0, ctx.view_bind_group, &[]);
            ctx.pass.set_bind_group(
                1,
                bind_group_keepalive
                    .last()
                    .expect("scene material prepass should keep a material bind group alive"),
                &[],
            );
            ctx.pass
                .set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
            ctx.pass.set_vertex_buffer(1, instance_buffer.slice(..));

            let Some(sub_mesh) = mesh.sub_meshes().get(sub_mesh_index as usize) else {
                return Err(DrawError::InvalidSubMeshIndex {
                    mesh: mesh.label().to_string(),
                    sub_mesh_index,
                });
            };
            if mesh.has_indices() {
                let Some(index_buffer) = mesh.index_buffer() else {
                    return Err(DrawError::MissingIndexBuffer {
                        mesh: mesh.label().to_string(),
                        sub_mesh_index,
                    });
                };
                let index_count = if sub_mesh.index_count == 0 {
                    mesh.index_count()
                } else {
                    sub_mesh.index_count
                };
                let index_offset = if sub_mesh.index_count == 0 {
                    0
                } else {
                    sub_mesh.index_offset
                };
                ctx.pass.set_index_buffer(
                    index_buffer.slice(..),
                    mesh.index_format()
                        .expect("indexed meshes provide an index format"),
                );
                ctx.pass.draw_indexed(
                    index_offset..(index_offset + index_count),
                    sub_mesh.vertex_offset,
                    0..models.len() as u32,
                );
            } else {
                ctx.pass
                    .draw(0..mesh.vertex_count(), 0..models.len() as u32);
            }

            cursor = batch_end;
        }

        Ok(())
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext<'_, '_, '_>,
        item: &PhaseItem,
    ) -> Result<(), DrawError> {
        self.draw_batch(ctx, std::slice::from_ref(item))
    }

    fn draw_batch(
        &mut self,
        ctx: &mut DrawContext<'_, '_, '_>,
        items: &[PhaseItem],
    ) -> Result<(), DrawError> {
        if items.is_empty() {
            return Ok(());
        }

        if !ctx.material_registry.is_registered::<M>() {
            return Err(MaterialError::UnregisteredMaterialType {
                type_name: std::any::type_name::<M>(),
            }
            .into());
        }
        let mut bind_group_keepalive = Vec::new();
        let cpu_model_matrices = ctx.cpu_model_matrices;
        let mut cursor = 0usize;

        while cursor < items.len() {
            let base = *items[cursor].data::<MeshDrawData>();
            let mesh_handle = base.mesh_handle();
            let material_handle = base.material_handle::<M>();
            let sub_mesh_index = base.sub_mesh_index();
            let mut batch_end = cursor + 1;
            while batch_end < items.len() {
                let next = *items[batch_end].data::<MeshDrawData>();
                if next.mesh_handle() != mesh_handle
                    || next.material_handle::<M>() != material_handle
                    || next.sub_mesh_index() != sub_mesh_index
                {
                    break;
                }
                batch_end += 1;
            }

            let mesh = ctx
                .mesh_registry
                .get(mesh_handle)
                .ok_or(DrawError::MissingMesh {
                    handle: mesh_handle,
                })?;
            let typed_material_handle =
                material_handle
                    .typed::<M>()
                    .ok_or(DrawError::MissingMaterial {
                        type_name: std::any::type_name::<M>(),
                    })?;
            let material = ctx
                .material_registry
                .get(typed_material_handle)
                .cloned()
                .map_err(|_| DrawError::MissingMaterial {
                    type_name: std::any::type_name::<M>(),
                })?;
            let scene_bindings = M::scene_bindings(&material);

            let mut fixed_layouts = vec![(0, ctx.view_bind_group_layout)];
            for binding in &scene_bindings {
                match binding.kind {
                    SceneBindingKind::GpuTable(type_id) => {
                        let Some(gpu_scene) = ctx.gpu_scene else {
                            return Err(DrawError::MissingSceneBinding {
                                type_name: std::any::type_name::<M>(),
                                kind: binding.kind,
                            });
                        };
                        let Some(table) = gpu_scene.try_table_by_type_id(type_id) else {
                            return Err(DrawError::MissingSceneBinding {
                                type_name: std::any::type_name::<M>(),
                                kind: binding.kind,
                            });
                        };
                        fixed_layouts.push((binding.slot, table.bind_group_layout()));
                    }
                    SceneBindingKind::ShadowView => {
                        let Some(scene_shadows) = ctx.scene_shadows.as_ref() else {
                            return Err(DrawError::MissingSceneBinding {
                                type_name: std::any::type_name::<M>(),
                                kind: binding.kind,
                            });
                        };
                        let Some(layout) = scene_shadows.bind_group_layout() else {
                            return Err(DrawError::MissingSceneBinding {
                                type_name: std::any::type_name::<M>(),
                                kind: binding.kind,
                            });
                        };
                        fixed_layouts.push((binding.slot, layout));
                    }
                    SceneBindingKind::GlobalIllumination => {
                        let Some(gi_sampling) = ctx.gi_sampling.as_ref() else {
                            return Err(DrawError::MissingSceneBinding {
                                type_name: std::any::type_name::<M>(),
                                kind: binding.kind,
                            });
                        };
                        fixed_layouts.push((binding.slot, &gi_sampling.layout));
                    }
                }
            }

            let pipeline = ctx
                .material_registry
                .pipeline_cache_mut()
                .get_or_create::<M>(
                    ctx.device,
                    &material,
                    mesh.vertex_layout(),
                    &fixed_layouts,
                    ctx.gi_shader_source,
                    ctx.gi_shader_key,
                    ctx.target_format,
                    ctx.depth_format,
                )?
                .clone();
            bind_group_keepalive.push(
                ctx.material_registry
                    .prepared(material_handle)?
                    .bind_group()
                    .clone(),
            );
            let empty_bind_group = ctx
                .material_registry
                .pipeline_cache_mut()
                .shared_empty_bind_group(ctx.device)
                .clone();

            let models: Vec<[f32; 16]> = items[cursor..batch_end]
                .iter()
                .map(|item| {
                    let mesh_data = *item.data::<MeshDrawData>();
                    *cpu_model_matrices
                        .and_then(|matrices| matrices.get(mesh_data.model_slot() as usize))
                        .unwrap_or(&IDENTITY_MODEL)
                })
                .collect();
            let instance_buffer =
                mesh_phase_instance_buffer(ctx.device, "mesh_phase_instance_buffer", &models);

            ctx.pass.set_pipeline(&pipeline);
            ctx.pass.set_bind_group(0, ctx.view_bind_group, &[]);
            ctx.pass.set_bind_group(
                1,
                bind_group_keepalive
                    .last()
                    .expect("mesh draw should cache current material bind group"),
                &[],
            );
            for binding in &scene_bindings {
                match binding.kind {
                    SceneBindingKind::GpuTable(type_id) => {
                        let Some(gpu_scene) = ctx.gpu_scene else {
                            return Err(DrawError::MissingSceneBinding {
                                type_name: std::any::type_name::<M>(),
                                kind: binding.kind,
                            });
                        };
                        let Some(table) = gpu_scene.try_table_by_type_id(type_id) else {
                            return Err(DrawError::MissingSceneBinding {
                                type_name: std::any::type_name::<M>(),
                                kind: binding.kind,
                            });
                        };
                        ctx.pass
                            .set_bind_group(binding.slot, table.bind_group(), &[]);
                    }
                    SceneBindingKind::ShadowView => {
                        let Some(scene_shadows) = ctx.scene_shadows.as_ref() else {
                            return Err(DrawError::MissingSceneBinding {
                                type_name: std::any::type_name::<M>(),
                                kind: binding.kind,
                            });
                        };
                        let Some(bind_group) = scene_shadows.bind_group() else {
                            return Err(DrawError::MissingSceneBinding {
                                type_name: std::any::type_name::<M>(),
                                kind: binding.kind,
                            });
                        };
                        ctx.pass.set_bind_group(binding.slot, bind_group, &[]);
                    }
                    SceneBindingKind::GlobalIllumination => {
                        let Some(gi_sampling) = ctx.gi_sampling.as_ref() else {
                            return Err(DrawError::MissingSceneBinding {
                                type_name: std::any::type_name::<M>(),
                                kind: binding.kind,
                            });
                        };
                        ctx.pass
                            .set_bind_group(binding.slot, &gi_sampling.bind_group, &[]);
                    }
                }
            }
            let occupied_slots: BTreeSet<u32> = std::iter::once(0u32)
                .chain(std::iter::once(1u32))
                .chain(scene_bindings.iter().map(|binding| binding.slot))
                .collect();
            let max_slot = occupied_slots.iter().copied().max().unwrap_or(1);
            for slot in 0..=max_slot {
                if !occupied_slots.contains(&slot) {
                    ctx.pass.set_bind_group(slot, &empty_bind_group, &[]);
                }
            }

            ctx.pass
                .set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
            ctx.pass.set_vertex_buffer(1, instance_buffer.slice(..));

            let Some(sub_mesh) = mesh.sub_meshes().get(sub_mesh_index as usize) else {
                return Err(DrawError::InvalidSubMeshIndex {
                    mesh: mesh.label().to_string(),
                    sub_mesh_index,
                });
            };
            if mesh.has_indices() {
                let Some(index_buffer) = mesh.index_buffer() else {
                    return Err(DrawError::MissingIndexBuffer {
                        mesh: mesh.label().to_string(),
                        sub_mesh_index,
                    });
                };
                let index_count = if sub_mesh.index_count == 0 {
                    mesh.index_count()
                } else {
                    sub_mesh.index_count
                };
                let index_offset = if sub_mesh.index_count == 0 {
                    0
                } else {
                    sub_mesh.index_offset
                };
                ctx.pass.set_index_buffer(
                    index_buffer.slice(..),
                    mesh.index_format()
                        .expect("indexed meshes provide an index format"),
                );
                ctx.pass.draw_indexed(
                    index_offset..(index_offset + index_count),
                    sub_mesh.vertex_offset,
                    0..models.len() as u32,
                );
            } else {
                ctx.pass
                    .draw(0..mesh.vertex_count(), 0..models.len() as u32);
            }
            cursor = batch_end;
        }

        Ok(())
    }

    fn assign_model_matrix(
        &mut self,
        item: &mut PhaseItem,
        transforms: &ResolvedSceneTransforms,
        entity_to_slot: &mut FxHashMap<EntityId, u32>,
        model_matrices: &mut Vec<[f32; 16]>,
    ) {
        let slot = if let Some(slot) = entity_to_slot.get(&item.entity).copied() {
            slot
        } else if let Some(transform) = transforms.get(item.entity) {
            let slot = model_matrices.len() as u32;
            model_matrices.push(transform.to_matrix4().to_cols_array());
            entity_to_slot.insert(item.entity, slot);
            slot
        } else {
            0
        };
        item.data_mut::<MeshDrawData>().set_model_slot(slot);
    }

    fn draw_call_count(&self, items: &[PhaseItem]) -> usize {
        if items.is_empty() {
            return 0;
        }

        let mut draws = 0usize;
        let mut cursor = 0usize;
        while cursor < items.len() {
            let base = *items[cursor].data::<MeshDrawData>();
            let mut batch_end = cursor + 1;
            while batch_end < items.len() {
                let next = *items[batch_end].data::<MeshDrawData>();
                if next.mesh_handle() != base.mesh_handle()
                    || next.sub_mesh_index() != base.sub_mesh_index()
                    || next.material_handle::<M>() != base.material_handle::<M>()
                {
                    break;
                }
                batch_end += 1;
            }
            draws += 1;
            cursor = batch_end;
        }
        draws
    }
}
