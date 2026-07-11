use crate::ecs::{PreparedQuery, World};
use crate::render::phase::{transparent_sort_key, DrawFunctionId, PhaseItem, SpriteDrawData};
use crate::render::resources::material::{MaterialError, SpriteMaterial};
use crate::render::view::{ResolvedSceneTransforms, SceneView};
use crate::render::{RenderLayerMask, SortingLayer, SpriteRenderer, Transform};
use rustc_hash::FxHashMap;

use super::{ExtractContext, ExtractError, Extractor, ExtractorViewKinds};

pub struct ExtractSprites {
    draw_function_id: DrawFunctionId,
    query: PreparedQuery<(
        &'static Transform,
        &'static SpriteRenderer,
        Option<&'static SortingLayer>,
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
    fn supported_view_kinds(&self) -> ExtractorViewKinds {
        ExtractorViewKinds::MAIN
    }

    fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        view: &SceneView,
        ctx: &mut ExtractContext<'_>,
    ) -> Result<(), ExtractError> {
        let gpu = ctx.gpu;
        let asset_server = ctx.asset_server;
        let render_assets = ctx.render_assets;
        let transparent_phase = &mut *ctx.transparent_phase;
        if !ctx.material_registry.is_registered::<SpriteMaterial>() {
            return Err(MaterialError::UnregisteredMaterialType {
                type_name: std::any::type_name::<SpriteMaterial>(),
            }
            .into());
        }
        let mut texture_materials = FxHashMap::default();

        self.query.for_each_with_entity(
            world,
            |entity, (transform, sprite, sorting_layer, layer_mask)| {
                if !sprite.visible {
                    return;
                }

                let effective_layer_mask = layer_mask.map_or(sprite.layer_mask, |mask| mask.0);
                if view.layer_mask & effective_layer_mask == 0 {
                    return;
                }

                let transform = transforms.get(entity).unwrap_or(*transform);
                let texture = sprite.texture.as_ref().and_then(|handle| {
                    resolve_sprite_texture(gpu, asset_server, render_assets, handle)
                });
                let texture_key = texture.as_ref().map_or(usize::MAX, |texture| {
                    std::ptr::from_ref(texture.texture()) as usize
                });
                let handle = if let Some(handle) = texture_materials.get(&texture_key).copied() {
                    handle
                } else {
                    let mut material = SpriteMaterial::default();
                    if let Some(texture) = texture {
                        material = material.texture(texture);
                    }
                    let handle = ctx
                        .material_registry
                        .insert_material::<SpriteMaterial>(material)
                        .expect("SpriteMaterial registration was checked before extraction")
                        .erased();
                    texture_materials.insert(texture_key, handle);
                    handle
                };
                let batch_key = sprite_batch_key_for(self.draw_function_id, handle);
                let sort_key = transparent_sort_key(
                    sorting_layer.copied().unwrap_or_default(),
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

fn resolve_sprite_texture(
    gpu: &crate::gpu::GpuContext,
    asset_server: Option<&crate::asset::Assets>,
    render_assets: Option<&crate::render::resources::texture_cache::SharedRenderAssetCache>,
    handle: &crate::asset::Handle<crate::asset::TextureAsset>,
) -> Option<crate::render::Texture> {
    match (asset_server, render_assets) {
        (Some(server), Some(cache)) => cache.borrow_mut().texture(gpu, server, handle),
        (_, Some(cache)) => {
            cache.borrow_mut().mark_texture_missing(handle);
            None
        }
        _ => None,
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
