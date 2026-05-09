use std::hash::{Hash, Hasher};

use crate::ecs::{PreparedQuery, World};
use crate::render::component::{RenderLayerMask, SortingLayer, Transform, WgpuMeshRenderer};
use crate::render::phase::{
    opaque_sort_key, transparent_sort_key, DrawFunctionId, MeshDrawData, PhaseItem,
};
use crate::render::resources::{
    material::{Material, MaterialError, MaterialHandle, MaterialModelExt},
    mesh::{BoundingSphere, MeshHandle},
};
use crate::render::view::{ResolvedSceneTransforms, SceneView};

use super::{ExtractContext, ExtractError, Extractor};

pub struct ExtractMeshes<M> {
    draw_function_id: DrawFunctionId,
    query: PreparedQuery<(
        &'static Transform,
        &'static WgpuMeshRenderer,
        Option<&'static SortingLayer>,
        Option<&'static RenderLayerMask>,
    )>,
    marker: std::marker::PhantomData<fn() -> M>,
}

impl<M> ExtractMeshes<M> {
    pub fn new(draw_function_id: DrawFunctionId) -> Self {
        Self {
            draw_function_id,
            query: PreparedQuery::new(),
            marker: std::marker::PhantomData,
        }
    }
}

impl<M> Extractor for ExtractMeshes<M>
where
    M: Material,
{
    fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        view: &SceneView,
        ctx: &mut ExtractContext<'_>,
    ) -> Result<(), ExtractError> {
        let material_storage = ctx.material_registry.try_materials::<M>().ok_or(
            MaterialError::UnregisteredMaterialType {
                type_name: std::any::type_name::<M>(),
            },
        )?;
        let mesh_registry = ctx.mesh_registry;
        let opaque_phase = &mut *ctx.opaque_phase;
        let transparent_phase = &mut *ctx.transparent_phase;

        self.query.for_each_with_entity(
            world,
            |entity, (transform, mesh_renderer, sorting_layer, layer_mask)| {
                if !mesh_renderer.visible {
                    return;
                }
                if view.is_shadow()
                    && !mesh_renderer.casts_shadows_in_cascade(view.shadow_cascade())
                {
                    return;
                }

                let effective_layer_mask =
                    layer_mask.map_or(mesh_renderer.layer_mask, |mask| mask.0);
                if view.layer_mask & effective_layer_mask == 0 {
                    return;
                }

                let transform = transforms.get(entity).unwrap_or(*transform);
                let Some(mesh) = mesh_registry.get(mesh_renderer.mesh) else {
                    return;
                };

                if !sphere_visible(view, transformed_sphere(transform, mesh.bounding_sphere())) {
                    return;
                }

                for (sub_mesh_index, sub_mesh) in mesh.sub_meshes().iter().enumerate() {
                    let Some(material_handle) =
                        resolve_sub_mesh_material(mesh_renderer, sub_mesh.material_index)
                    else {
                        continue;
                    };
                    if !material_handle.is::<M>() {
                        continue;
                    }
                    let Some(material) = material_storage.get(material_handle) else {
                        continue;
                    };
                    if !sphere_visible(
                        view,
                        transformed_sphere(transform, sub_mesh.bounding_sphere),
                    ) {
                        continue;
                    }

                    let batch_key = batch_key_for(
                        self.draw_function_id,
                        M::pipeline_key(material),
                        material_handle,
                        mesh_renderer.mesh,
                        sub_mesh_index as u32,
                    );
                    let item = PhaseItem::new(
                        if M::is_transparent(material) {
                            transparent_sort_key(
                                sorting_layer.copied().unwrap_or_default(),
                                batch_key,
                                transform,
                                view,
                            )
                        } else {
                            opaque_sort_key(batch_key, entity, transform, view)
                        },
                        self.draw_function_id,
                        entity,
                        batch_key,
                        MeshDrawData::new(
                            mesh_renderer.mesh,
                            material_handle,
                            sub_mesh_index as u32,
                        ),
                    );

                    if M::is_transparent(material) {
                        transparent_phase.add_item(item);
                    } else {
                        opaque_phase.add_item(item);
                    }
                }
            },
        );

        Ok(())
    }
}

fn resolve_sub_mesh_material(
    mesh_renderer: &WgpuMeshRenderer,
    material_index: u32,
) -> Option<MaterialHandle> {
    mesh_renderer
        .materials
        .get(material_index as usize)
        .copied()
        .or_else(|| mesh_renderer.materials.first().copied())
}

fn batch_key_for(
    draw_function_id: DrawFunctionId,
    pipeline_key: u64,
    material_handle: MaterialHandle,
    mesh_handle: MeshHandle,
    sub_mesh_index: u32,
) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    material_handle.hash(&mut hasher);
    mesh_handle.hash(&mut hasher);
    sub_mesh_index.hash(&mut hasher);
    let exact_batch = hasher.finish() & 0x0000_ffff_ffff_ffff;
    (((draw_function_id.index() as u64) & 0xff) << 56) | ((pipeline_key & 0xff) << 48) | exact_batch
}

fn transformed_sphere(transform: Transform, sphere: BoundingSphere) -> BoundingSphere {
    if !sphere.radius.is_finite() {
        return BoundingSphere::UNBOUNDED;
    }

    let max_scale = transform
        .scale_x()
        .abs()
        .max(transform.scale_y().abs())
        .max(transform.scale_z().abs());
    BoundingSphere::new(
        transform
            .transform_point(crate::math::Vec3::from_array(sphere.center))
            .to_array(),
        sphere.radius * max_scale.max(f32::EPSILON),
    )
}

fn sphere_visible(view: &SceneView, sphere: BoundingSphere) -> bool {
    if !sphere.radius.is_finite() {
        return true;
    }
    view.frustum()
        .intersects_sphere(sphere.center, sphere.radius)
}
