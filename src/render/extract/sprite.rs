use crate::ecs::{PreparedQuery, World};
use crate::render::component::{
    OrderInLayer, RenderLayerMask, SortingLayer, SpriteRenderer, Transform,
};
use crate::render::phase::{transparent_sort_key, DrawFunctionId, PhaseItem, SpriteDrawData};
use crate::render::resources::material::{MaterialError, SpriteMaterial};
use crate::render::view::{ResolvedSceneTransforms, SceneView};
use rustc_hash::FxHashMap;

use super::{ExtractContext, ExtractError, Extractor};

pub struct ExtractSprites {
    draw_function_id: DrawFunctionId,
    query: PreparedQuery<(
        &'static Transform,
        &'static SpriteRenderer,
        Option<&'static SortingLayer>,
        Option<&'static OrderInLayer>,
        Option<&'static RenderLayerMask>,
    )>,
}

impl ExtractSprites {
    pub fn new(draw_function_id: DrawFunctionId) -> Self {
        Self {
            draw_function_id,
            query: PreparedQuery::new(),
        }
    }
}

impl Extractor for ExtractSprites {
    fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        view: &SceneView,
        ctx: &mut ExtractContext<'_>,
    ) -> Result<(), ExtractError> {
        let material_storage = ctx
            .material_registry
            .try_materials_mut::<SpriteMaterial>()
            .ok_or(MaterialError::UnregisteredMaterialType {
                type_name: std::any::type_name::<SpriteMaterial>(),
            })?;
        material_storage.clear();
        let mut texture_materials = FxHashMap::default();

        let transparent_phase = &mut *ctx.transparent_phase;
        self.query.for_each_with_entity(
            world,
            |entity, (transform, sprite, sorting_layer, order_in_layer, layer_mask)| {
                if !sprite.visible {
                    return;
                }

                let effective_layer_mask = layer_mask.map_or(sprite.layer_mask, |mask| mask.0);
                if view.layer_mask & effective_layer_mask == 0 {
                    return;
                }

                let transform = transforms.get(entity).unwrap_or(*transform);
                let texture_key = sprite.texture.as_ref().map_or(usize::MAX, |texture| {
                    std::ptr::from_ref(texture.texture()) as usize
                });
                let handle = if let Some(handle) = texture_materials.get(&texture_key).copied() {
                    handle
                } else {
                    let mut material = SpriteMaterial::default();
                    if let Some(texture) = sprite.texture.clone() {
                        material = material.texture(texture);
                    }
                    let handle = material_storage.insert(material);
                    texture_materials.insert(texture_key, handle);
                    handle
                };
                let batch_key = sprite_batch_key_for(self.draw_function_id, handle);
                let sort_key = transparent_sort_key(
                    sorting_layer.copied().unwrap_or_default(),
                    order_in_layer.copied().unwrap_or_default(),
                    batch_key,
                    transform,
                    view,
                );

                transparent_phase.add_item(PhaseItem::new(
                    sort_key,
                    self.draw_function_id,
                    entity,
                    batch_key,
                    SpriteDrawData::new(
                        handle,
                        [sprite.width, sprite.height],
                        sprite.color.to_array(),
                        sprite.uv,
                    ),
                ));
            },
        );

        Ok(())
    }
}

fn sprite_batch_key_for(
    draw_function_id: DrawFunctionId,
    material_handle: crate::render::MaterialHandle,
) -> u64 {
    (((draw_function_id.index() as u64) & 0xff) << 56)
        | (((material_handle.index() as u64) & 0xff) << 48)
        | material_handle.index() as u64
}
