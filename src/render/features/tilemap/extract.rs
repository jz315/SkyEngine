use std::hash::{Hash, Hasher};

use crate::ecs::{PreparedQuery, World};
use crate::math::{Vec2, Vec3};
use crate::render::features::tilemap::{
    SharedTilemapFrameCache, TileChunkBounds, TileFlags, Tilemap, TilemapChunkState,
    TilemapDepthSort, TilemapDrawData, TilemapGpuChunkKey, TilemapInstance, TilemapInstanceSpan,
    TilemapOrientation, TilemapPreparedInstances, TilemapRenderOrder, TilemapRenderer,
    TilemapStorage,
};
use crate::render::phase::{
    transparent_ordered_2d_sort_key, transparent_sort_key, DrawFunctionId, PhaseItem,
};
use crate::render::view::{ResolvedSceneTransforms, SceneView};
use crate::render::Color;
use crate::render::Texture;
use crate::render::{RenderLayerMask, SortingLayer, Transform};

use crate::render::extract::{ExtractContext, ExtractError, Extractor, ExtractorViewKinds};

pub struct ExtractTilemaps {
    draw_function_id: DrawFunctionId,
    cache: SharedTilemapFrameCache,
    query: PreparedQuery<(
        &'static Transform,
        &'static TilemapRenderer,
        Option<&'static SortingLayer>,
        Option<&'static RenderLayerMask>,
    )>,
}

impl ExtractTilemaps {
    pub(crate) fn new(draw_function_id: DrawFunctionId, cache: SharedTilemapFrameCache) -> Self {
        Self {
            draw_function_id,
            cache,
            query: PreparedQuery::new(),
        }
    }
}

impl Extractor for ExtractTilemaps {
    fn supported_view_kinds(&self) -> ExtractorViewKinds {
        ExtractorViewKinds::MAIN
    }

    fn begin_frame(&mut self) {
        self.cache
            .lock()
            .expect("tilemap frame cache poisoned")
            .begin_frame();
    }

    fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        view: &SceneView,
        ctx: &mut ExtractContext<'_>,
    ) -> Result<(), ExtractError> {
        let mut cache = self.cache.lock().expect("tilemap frame cache poisoned");

        let Some(storage) = world.get_resource::<TilemapStorage>() else {
            return Ok(());
        };
        let animation_time = world.time.elapsed;

        self.query.for_each_with_entity(
            world,
            |entity, (transform, renderer, sorting_layer, layer_mask)| {
                let draw_visible = renderer.visible;
                let prewarm_only =
                    !draw_visible && renderer.cache_prewarm && view.execution_order() == 0;
                if !draw_visible && !prewarm_only {
                    return;
                }

                let effective_layer_mask = layer_mask.map_or(renderer.layer_mask, |mask| mask.0);
                if draw_visible && view.layer_mask & effective_layer_mask == 0 {
                    return;
                }

                let Some(map) = storage.get(renderer.map) else {
                    return;
                };

                let transform = transforms.get(entity).unwrap_or(*transform);
                let texture = resolve_tileset_texture(renderer, ctx);
                let texture_key = texture_key(texture.as_ref());
                let batch_key = tilemap_batch_key_for(self.draw_function_id, texture_key);
                let renderer_hash = renderer_hash_for(renderer);
                let layer = renderer.layer;
                if layer >= map.layer_count() {
                    return;
                }

                let sort_key = transparent_sort_key(
                    sorting_layer.copied().unwrap_or_default(),
                    batch_key,
                    transform,
                    view,
                );
                let ordered_chunks =
                    ordered_chunk_coords(map, renderer.orientation, renderer.render_order);

                for (chunk_x, chunk_y) in ordered_chunks {
                    if map
                        .chunk_non_empty_tiles(layer, chunk_x, chunk_y)
                        .unwrap_or_default()
                        == 0
                    {
                        continue;
                    }
                    let Some(bounds) = map.chunk_bounds(chunk_x, chunk_y) else {
                        continue;
                    };

                    let draw_chunk = draw_visible
                        && chunk_intersects_view(
                            map,
                            layer,
                            bounds,
                            renderer,
                            transform,
                            view,
                            animation_time,
                        );
                    if !draw_chunk && !prewarm_only {
                        continue;
                    }

                    let animation_frame_key =
                        chunk_animation_frame_key(map, layer, bounds, renderer, animation_time);
                    let key = TilemapGpuChunkKey {
                        map: renderer.map,
                        layer,
                        chunk_x,
                        chunk_y,
                        texture_key,
                    };
                    let state = TilemapChunkState {
                        chunk_version: map
                            .chunk_version(layer, chunk_x, chunk_y)
                            .unwrap_or_default(),
                        renderer_hash,
                        animation_frame_key,
                    };

                    let chunk_index = if let Some(index) =
                        cache.cached_chunk_index(key, state, texture.clone(), texture_key)
                    {
                        index
                    } else {
                        if prewarm_only && !draw_chunk && !cache.can_prepare_background_chunk() {
                            continue;
                        }
                        let prepared_instances =
                            build_chunk_instances(map, layer, bounds, renderer, animation_time);
                        if prepared_instances.instances.is_empty() {
                            continue;
                        }
                        cache.insert_or_update_chunk(
                            ctx.gpu.device(),
                            ctx.gpu.queue(),
                            key,
                            state,
                            texture.clone(),
                            texture_key,
                            prepared_instances,
                        )
                    };

                    if !draw_chunk {
                        continue;
                    }

                    let spans = cache
                        .chunk(chunk_index)
                        .map(|(_, chunk)| chunk.spans.clone())
                        .unwrap_or_default();
                    for span in spans {
                        let item_sort_key = if renderer.depth_sort == TilemapDepthSort::YThenLayer {
                            transparent_ordered_2d_sort_key(
                                sorting_layer.copied().unwrap_or_default(),
                                batch_key,
                                span.sort_order,
                            )
                        } else {
                            sort_key
                        };
                        ctx.transparent_phase.add_item(PhaseItem::new(
                            item_sort_key,
                            self.draw_function_id,
                            entity,
                            batch_key,
                            TilemapDrawData::new_chunk_range(
                                chunk_index,
                                span.first_instance,
                                span.instance_count,
                            ),
                        ));
                    }
                }
            },
        );

        Ok(())
    }
}

fn resolve_tileset_texture(
    renderer: &TilemapRenderer,
    ctx: &mut ExtractContext<'_>,
) -> Option<Texture> {
    match (ctx.asset_server, ctx.render_assets) {
        (Some(server), Some(cache)) => {
            cache
                .borrow_mut()
                .texture(ctx.gpu, server, &renderer.tileset.texture)
        }
        (_, Some(cache)) => {
            cache
                .borrow_mut()
                .mark_texture_missing(&renderer.tileset.texture);
            None
        }
        _ => None,
    }
}

fn build_chunk_instances(
    map: &Tilemap,
    layer: u32,
    bounds: TileChunkBounds,
    renderer: &TilemapRenderer,
    animation_time: f32,
) -> TilemapPreparedInstances {
    let capacity = (bounds.width * bounds.height) as usize;
    let mut instances = Vec::with_capacity(capacity);
    let mut spans = Vec::new();

    if renderer.depth_sort == TilemapDepthSort::YThenLayer {
        let mut sorted = Vec::with_capacity(capacity);
        for y in bounds.y..bounds.y + bounds.height {
            for x in bounds.x..bounds.x + bounds.width {
                if let Some(instance) =
                    build_tile_instance(map, layer, x, y, renderer, animation_time)
                {
                    sorted.push((tile_sort_order(map, renderer, layer, x, y), instance));
                }
            }
        }
        sorted.sort_by_key(|(order, _)| *order);
        for (order, instance) in sorted {
            let first_instance = instances.len() as u32;
            instances.push(instance);
            spans.push(TilemapInstanceSpan {
                first_instance,
                instance_count: 1,
                sort_order: order,
            });
        }
    } else {
        match renderer.orientation {
            TilemapOrientation::Orthogonal
            | TilemapOrientation::Staggered
            | TilemapOrientation::Hexagonal => {
                for_each_ordered_cell(bounds, renderer.render_order, |x, y| {
                    if let Some(instance) =
                        build_tile_instance(map, layer, x, y, renderer, animation_time)
                    {
                        instances.push(instance);
                    }
                });
            }
            TilemapOrientation::Isometric => {
                let mut sorted = Vec::with_capacity(capacity);
                for y in bounds.y..bounds.y + bounds.height {
                    for x in bounds.x..bounds.x + bounds.width {
                        if let Some(instance) =
                            build_tile_instance(map, layer, x, y, renderer, animation_time)
                        {
                            sorted.push((tile_sort_order(map, renderer, layer, x, y), instance));
                        }
                    }
                }
                sorted.sort_by_key(|(order, _)| *order);
                instances.extend(sorted.into_iter().map(|(_, instance)| instance));
            }
        }
        if !instances.is_empty() {
            spans.push(TilemapInstanceSpan {
                first_instance: 0,
                instance_count: instances.len() as u32,
                sort_order: 0,
            });
        }
    }

    TilemapPreparedInstances { instances, spans }
}

fn build_tile_instance(
    map: &Tilemap,
    layer: u32,
    x: u32,
    y: u32,
    renderer: &TilemapRenderer,
    animation_time: f32,
) -> Option<TilemapInstance> {
    let tile = map.tile(layer, x, y)?;
    if tile.is_empty() {
        return None;
    }
    let tile_id = renderer.tileset.animated_tile_id(tile.id, animation_time);
    let uv_rect = renderer.tileset.uv_rect(tile_id)?;
    let tile_size = renderer
        .tileset
        .tile_draw_size(tile_id)
        .map(|size| [size[0] as f32, size[1] as f32])
        .unwrap_or(renderer.tile_draw_size);
    let axis_x = [tile_size[0], 0.0, 0.0];
    let axis_y = [0.0, tile_size[1], 0.0];
    let (uv_origin, uv_axis_x, uv_axis_y) = tile_uv_transform(uv_rect, tile.flags);
    let mut origin = renderer.cell_to_local_origin([x as i32, y as i32]);
    origin[0] += renderer.tile_offset[0];
    origin[1] += renderer.tile_offset[1];
    let color = multiply_color(renderer.color, tile.tint).to_array();
    Some(TilemapInstance {
        origin: [origin[0], origin[1], 0.0, 1.0],
        axis_x: [axis_x[0], axis_x[1], axis_x[2], 0.0],
        axis_y: [axis_y[0], axis_y[1], axis_y[2], 0.0],
        color,
        uv_origin,
        uv_axis_x,
        uv_axis_y,
    })
}

fn tile_uv_transform(uv_rect: [f32; 4], flags: TileFlags) -> ([f32; 4], [f32; 4], [f32; 4]) {
    let p00 = transformed_atlas_uv([0.0, 0.0], uv_rect, flags);
    let p10 = transformed_atlas_uv([1.0, 0.0], uv_rect, flags);
    let p01 = transformed_atlas_uv([0.0, 1.0], uv_rect, flags);
    (
        [p00[0], p00[1], 0.0, 0.0],
        [p10[0] - p00[0], p10[1] - p00[1], 0.0, 0.0],
        [p01[0] - p00[0], p01[1] - p00[1], 0.0, 0.0],
    )
}

fn transformed_atlas_uv(uv: [f32; 2], uv_rect: [f32; 4], flags: TileFlags) -> [f32; 2] {
    let mut u = uv[0];
    let mut v = uv[1];
    if flags.contains(TileFlags::FLIP_DIAGONAL) {
        std::mem::swap(&mut u, &mut v);
    }
    if flags.contains(TileFlags::FLIP_X) {
        u = 1.0 - u;
    }
    if flags.contains(TileFlags::FLIP_Y) {
        v = 1.0 - v;
    }
    [
        uv_rect[0] + (uv_rect[2] - uv_rect[0]) * u,
        uv_rect[1] + (uv_rect[3] - uv_rect[1]) * v,
    ]
}

fn multiply_color(lhs: Color, rhs: Color) -> Color {
    Color::new(lhs.r * rhs.r, lhs.g * rhs.g, lhs.b * rhs.b, lhs.a * rhs.a)
}

fn chunk_intersects_view(
    map: &Tilemap,
    layer: u32,
    bounds: TileChunkBounds,
    renderer: &TilemapRenderer,
    transform: Transform,
    view: &SceneView,
    animation_time: f32,
) -> bool {
    if !view.is_planar_2d {
        return true;
    }
    let Some(size) = view.projection.orthographic_size(Vec2::new(
        view.target_size[0] as f32,
        view.target_size[1] as f32,
    )) else {
        return true;
    };

    let half_width = size.x() * 0.5;
    let half_height = size.y() * 0.5;
    let view_min_x = view.camera_transform.x() - half_width;
    let view_max_x = view.camera_transform.x() + half_width;
    let view_min_y = view.camera_transform.y() - half_height;
    let view_max_y = view.camera_transform.y() + half_height;

    let mut chunk_min_x = f32::INFINITY;
    let mut chunk_max_x = f32::NEG_INFINITY;
    let mut chunk_min_y = f32::INFINITY;
    let mut chunk_max_y = f32::NEG_INFINITY;
    for y in bounds.y..bounds.y + bounds.height {
        for x in bounds.x..bounds.x + bounds.width {
            let Some(tile) = map.tile(layer, x, y) else {
                continue;
            };
            if tile.is_empty() {
                continue;
            }
            let tile_id = renderer.tileset.animated_tile_id(tile.id, animation_time);
            if renderer.tileset.uv_rect(tile_id).is_none() {
                continue;
            }
            let tile_size = renderer
                .tileset
                .tile_draw_size(tile_id)
                .map(|size| [size[0] as f32, size[1] as f32])
                .unwrap_or(renderer.tile_draw_size);
            let origin = renderer.cell_to_local_origin([x as i32, y as i32]);
            let local_min = [
                origin[0] + renderer.tile_offset[0],
                origin[1] + renderer.tile_offset[1],
            ];
            let local_max = [local_min[0] + tile_size[0], local_min[1] + tile_size[1]];
            let local_corners = [
                [local_min[0], local_min[1]],
                [local_max[0], local_min[1]],
                [local_min[0], local_max[1]],
                [local_max[0], local_max[1]],
            ];
            for [local_x, local_y] in local_corners {
                let corner = transform.transform_point(Vec3::new(local_x, local_y, 0.0));
                chunk_min_x = chunk_min_x.min(corner.x());
                chunk_max_x = chunk_max_x.max(corner.x());
                chunk_min_y = chunk_min_y.min(corner.y());
                chunk_max_y = chunk_max_y.max(corner.y());
            }
        }
    }

    if !chunk_min_x.is_finite()
        || !chunk_max_x.is_finite()
        || !chunk_min_y.is_finite()
        || !chunk_max_y.is_finite()
    {
        return false;
    }

    chunk_max_x >= view_min_x
        && chunk_min_x <= view_max_x
        && chunk_max_y >= view_min_y
        && chunk_min_y <= view_max_y
}

fn renderer_hash_for(renderer: &TilemapRenderer) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    renderer.layer.hash(&mut hasher);
    renderer.orientation.hash(&mut hasher);
    renderer.stagger_axis.hash(&mut hasher);
    renderer.stagger_index.hash(&mut hasher);
    renderer.hex_side_length.to_bits().hash(&mut hasher);
    renderer.render_order.hash(&mut hasher);
    renderer.depth_sort.hash(&mut hasher);
    renderer.tile_size[0].to_bits().hash(&mut hasher);
    renderer.tile_size[1].to_bits().hash(&mut hasher);
    renderer.tile_draw_size[0].to_bits().hash(&mut hasher);
    renderer.tile_draw_size[1].to_bits().hash(&mut hasher);
    renderer.tile_offset[0].to_bits().hash(&mut hasher);
    renderer.tile_offset[1].to_bits().hash(&mut hasher);
    for channel in renderer.color.to_array() {
        channel.to_bits().hash(&mut hasher);
    }
    renderer.tileset.columns.hash(&mut hasher);
    renderer.tileset.rows.hash(&mut hasher);
    renderer.tileset.tile_size.hash(&mut hasher);
    renderer.tileset.texture_size.hash(&mut hasher);
    renderer.tileset.margin.hash(&mut hasher);
    renderer.tileset.spacing.hash(&mut hasher);
    renderer.tileset.tile_rects.hash(&mut hasher);
    renderer.tileset.animations.hash(&mut hasher);
    renderer.tileset.texture.hash(&mut hasher);
    hasher.finish()
}

fn chunk_animation_frame_key(
    map: &Tilemap,
    layer: u32,
    bounds: TileChunkBounds,
    renderer: &TilemapRenderer,
    animation_time: f32,
) -> u64 {
    if renderer.tileset.animations.is_empty() {
        return 0;
    }

    let mut hasher = rustc_hash::FxHasher::default();
    let mut found = false;
    for y in bounds.y..bounds.y + bounds.height {
        for x in bounds.x..bounds.x + bounds.width {
            let Some(tile) = map.tile(layer, x, y) else {
                continue;
            };
            if tile.is_empty() {
                continue;
            }
            if renderer
                .tileset
                .animations
                .iter()
                .any(|animation| animation.tile_id == tile.id)
            {
                found = true;
                tile.id.hash(&mut hasher);
                renderer
                    .tileset
                    .animated_tile_id(tile.id, animation_time)
                    .hash(&mut hasher);
            }
        }
    }

    if found {
        hasher.finish()
    } else {
        0
    }
}

fn ordered_chunk_coords(
    map: &Tilemap,
    orientation: TilemapOrientation,
    render_order: TilemapRenderOrder,
) -> Vec<(u32, u32)> {
    let mut coords = Vec::with_capacity((map.chunk_columns() * map.chunk_rows()) as usize);
    match orientation {
        TilemapOrientation::Orthogonal
        | TilemapOrientation::Staggered
        | TilemapOrientation::Hexagonal => {
            for_each_ordered_chunk(map, render_order, |chunk_x, chunk_y| {
                coords.push((chunk_x, chunk_y));
            });
        }
        TilemapOrientation::Isometric => {
            for chunk_y in (0..map.chunk_rows()).rev() {
                for chunk_x in (0..map.chunk_columns()).rev() {
                    coords.push((chunk_x, chunk_y));
                }
            }
            coords.sort_by_key(|&(chunk_x, chunk_y)| {
                let max_diagonal = map
                    .chunk_columns()
                    .saturating_add(map.chunk_rows())
                    .saturating_sub(2);
                let diagonal = chunk_x.saturating_add(chunk_y);
                (
                    max_diagonal.saturating_sub(diagonal),
                    chunk_x as i64 - chunk_y as i64,
                )
            });
        }
    }
    coords
}

fn tile_sort_order(map: &Tilemap, renderer: &TilemapRenderer, layer: u32, x: u32, y: u32) -> u64 {
    let layer_count = map.layer_count().max(1) as u64;
    let layer_rank = layer.min(map.layer_count().saturating_sub(1)) as u64;
    match renderer.orientation {
        TilemapOrientation::Orthogonal
        | TilemapOrientation::Staggered
        | TilemapOrientation::Hexagonal => {
            let height = map.height().max(1);
            let width = map.width().max(1);
            let tiled_y = height.saturating_sub(1).saturating_sub(y.min(height - 1));
            let row_rank = match renderer.render_order {
                TilemapRenderOrder::RightDown | TilemapRenderOrder::LeftDown => tiled_y,
                TilemapRenderOrder::RightUp | TilemapRenderOrder::LeftUp => {
                    height.saturating_sub(1).saturating_sub(tiled_y)
                }
            } as u64;
            let col_rank = match renderer.render_order {
                TilemapRenderOrder::RightDown | TilemapRenderOrder::RightUp => x.min(width - 1),
                TilemapRenderOrder::LeftDown | TilemapRenderOrder::LeftUp => {
                    width.saturating_sub(1).saturating_sub(x.min(width - 1))
                }
            } as u64;
            ((row_rank * width as u64 + col_rank) * layer_count) + layer_rank
        }
        TilemapOrientation::Isometric => {
            let height = map.height().max(1);
            let width = map.width().max(1);
            let y = y.min(height - 1);
            let x = x.min(width - 1);
            let max_diagonal = width.saturating_add(height).saturating_sub(2);
            let diagonal_rank = max_diagonal.saturating_sub(x.saturating_add(y)) as u64;
            let screen_x_rank = x as u64 + height.saturating_sub(1).saturating_sub(y) as u64;
            let diagonal_width = width as u64 + height as u64;
            ((diagonal_rank * diagonal_width + screen_x_rank) * layer_count) + layer_rank
        }
    }
}

fn for_each_ordered_cell(
    bounds: TileChunkBounds,
    render_order: TilemapRenderOrder,
    mut visit: impl FnMut(u32, u32),
) {
    let x_start = bounds.x;
    let x_end = bounds.x + bounds.width;
    let y_start = bounds.y;
    let y_end = bounds.y + bounds.height;
    match render_order {
        TilemapRenderOrder::RightDown => {
            for y in (y_start..y_end).rev() {
                for x in x_start..x_end {
                    visit(x, y);
                }
            }
        }
        TilemapRenderOrder::RightUp => {
            for y in y_start..y_end {
                for x in x_start..x_end {
                    visit(x, y);
                }
            }
        }
        TilemapRenderOrder::LeftDown => {
            for y in (y_start..y_end).rev() {
                for x in (x_start..x_end).rev() {
                    visit(x, y);
                }
            }
        }
        TilemapRenderOrder::LeftUp => {
            for y in y_start..y_end {
                for x in (x_start..x_end).rev() {
                    visit(x, y);
                }
            }
        }
    }
}

fn for_each_ordered_chunk(
    map: &Tilemap,
    render_order: TilemapRenderOrder,
    mut visit: impl FnMut(u32, u32),
) {
    let chunk_columns = map.chunk_columns();
    let chunk_rows = map.chunk_rows();
    match render_order {
        TilemapRenderOrder::RightDown => {
            for y in (0..chunk_rows).rev() {
                for x in 0..chunk_columns {
                    visit(x, y);
                }
            }
        }
        TilemapRenderOrder::RightUp => {
            for y in 0..chunk_rows {
                for x in 0..chunk_columns {
                    visit(x, y);
                }
            }
        }
        TilemapRenderOrder::LeftDown => {
            for y in (0..chunk_rows).rev() {
                for x in (0..chunk_columns).rev() {
                    visit(x, y);
                }
            }
        }
        TilemapRenderOrder::LeftUp => {
            for y in 0..chunk_rows {
                for x in (0..chunk_columns).rev() {
                    visit(x, y);
                }
            }
        }
    }
}

fn texture_key(texture: Option<&Texture>) -> u64 {
    texture.map_or(u64::MAX, |texture| {
        std::ptr::from_ref(texture.texture()) as usize as u64
    })
}

fn tilemap_batch_key_for(draw_function_id: DrawFunctionId, texture_key: u64) -> u64 {
    (((draw_function_id.index() as u64) & 0xff) << 56) | (texture_key & 0x00ff_ffff_ffff_ffff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{AssetId, Handle, TextureAsset};
    use crate::render::features::tilemap::{
        Tile, TileId, TilemapDescriptor, TilemapFrameCache, TilemapHandle,
    };
    use crate::render::features::tilemap::{
        TileAnimation, TileAnimationFrame, TilesetGrid, TilesetTileRect,
    };
    use crate::render::view::SceneViewKind;
    use crate::render::view::{Projection, ProjectionViewUniformExt, ViewportRect};
    use std::sync::{Arc, Mutex};

    #[test]
    fn tilemap_extractor_declares_main_views_only() {
        let extractor = ExtractTilemaps::new(
            DrawFunctionId::from_raw(0),
            Arc::new(Mutex::new(TilemapFrameCache::default())),
        );
        let view_kinds = extractor.supported_view_kinds();

        assert!(view_kinds.contains(SceneViewKind::Main));
        assert!(!view_kinds.contains(SceneViewKind::DirectionalShadow));
    }

    #[test]
    fn tile_uv_transform_keeps_plain_rect() {
        let (origin, axis_x, axis_y) =
            tile_uv_transform([0.25, 0.5, 0.5, 0.75], TileFlags::empty());

        assert_eq!(origin, [0.25, 0.5, 0.0, 0.0]);
        assert_eq!(axis_x, [0.25, 0.0, 0.0, 0.0]);
        assert_eq!(axis_y, [0.0, 0.25, 0.0, 0.0]);
    }

    #[test]
    fn tile_uv_transform_supports_tiled_diagonal_flip() {
        let (origin, axis_x, axis_y) =
            tile_uv_transform([0.25, 0.5, 0.5, 0.75], TileFlags::FLIP_DIAGONAL);

        assert_eq!(origin, [0.25, 0.5, 0.0, 0.0]);
        assert_eq!(axis_x, [0.0, 0.25, 0.0, 0.0]);
        assert_eq!(axis_y, [0.25, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn orthogonal_right_down_draws_high_engine_y_first() {
        let mut cells = Vec::new();
        for_each_ordered_cell(
            TileChunkBounds {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            TilemapRenderOrder::RightDown,
            |x, y| cells.push((x, y)),
        );

        assert_eq!(cells, [(0, 1), (1, 1), (0, 0), (1, 0)]);
    }

    #[test]
    fn orthogonal_chunk_order_matches_tiled_render_order() {
        let map = Tilemap::new(crate::render::features::tilemap::TilemapDescriptor::new(
            64, 64, 1,
        ));

        assert_eq!(
            ordered_chunk_coords(
                &map,
                TilemapOrientation::Orthogonal,
                TilemapRenderOrder::RightDown,
            ),
            [(0, 1), (1, 1), (0, 0), (1, 0)]
        );
        assert_eq!(
            ordered_chunk_coords(
                &map,
                TilemapOrientation::Orthogonal,
                TilemapRenderOrder::LeftUp,
            ),
            [(1, 0), (0, 0), (1, 1), (0, 1)]
        );
    }

    #[test]
    fn y_then_layer_sort_interleaves_layers_by_tiled_cell_order() {
        let map = Tilemap::new(TilemapDescriptor::new(4, 4, 3));
        let renderer = TilemapRenderer::new(
            TilemapHandle::new(0, 0),
            crate::render::features::tilemap::TilesetGrid::new(
                Handle::<TextureAsset>::new(AssetId::new()),
                [64, 64],
                1,
                1,
            ),
        )
        .tile_size([31.0, 31.0])
        .render_order(TilemapRenderOrder::RightDown)
        .depth_sort(TilemapDepthSort::YThenLayer);

        let back_upper_layer = tile_sort_order(&map, &renderer, 2, 1, 3);
        let same_cell_lower_layer = tile_sort_order(&map, &renderer, 0, 1, 3);
        let front_lower_layer = tile_sort_order(&map, &renderer, 0, 1, 2);

        assert!(same_cell_lower_layer < back_upper_layer);
        assert!(back_upper_layer < front_lower_layer);
    }

    #[test]
    fn renderer_hash_ignores_transform_sensitive_state() {
        let texture = Handle::<TextureAsset>::new(AssetId::new());
        let renderer = TilemapRenderer::new(
            TilemapHandle::new(0, 0),
            TilesetGrid::new(texture, [16, 16], 1, 1),
        );

        assert_eq!(renderer_hash_for(&renderer), renderer_hash_for(&renderer));
    }

    #[test]
    fn chunk_culling_uses_tileset_draw_extents() {
        let texture = Handle::<TextureAsset>::new(AssetId::new());
        let tileset = TilesetGrid::new(texture, [16, 16], 1, 1)
            .texture_size([64, 64])
            .tile_rects([Some(TilesetTileRect::new(0, 0, 64, 64))]);
        let renderer = TilemapRenderer::new(TilemapHandle::new(0, 0), tileset)
            .tile_size([16.0, 16.0])
            .tile_draw_size([16.0, 16.0]);
        let mut map = Tilemap::new(TilemapDescriptor::new(1, 1, 1));
        assert!(map.set_tile(0, 0, 0, Tile::new(TileId(0))).is_some());
        let bounds = map.chunk_bounds(0, 0).unwrap();
        let projection = Projection::orthographic_fixed(16.0, 16.0);
        let camera = Transform::from_xy(58.0, 8.0);
        let view = SceneView::new(
            0,
            ViewportRect::from_surface_size([16, 16]),
            [16, 16],
            false,
            u32::MAX,
            camera,
            projection,
            projection.view_uniform(camera, [16, 16]),
            true,
        );

        assert!(chunk_intersects_view(
            &map,
            0,
            bounds,
            &renderer,
            Transform::default(),
            &view,
            0.0,
        ));
    }

    #[test]
    fn non_animated_chunks_ignore_animation_time() {
        let texture = Handle::<TextureAsset>::new(AssetId::new());
        let renderer = TilemapRenderer::new(
            TilemapHandle::new(0, 0),
            TilesetGrid::new(texture, [16, 16], 2, 1),
        );
        let mut map = Tilemap::new(TilemapDescriptor::new(2, 1, 1));
        assert!(map.set_tile(0, 0, 0, Tile::new(TileId(0))).is_some());
        let bounds = map.chunk_bounds(0, 0).unwrap();

        assert_eq!(
            chunk_animation_frame_key(&map, 0, bounds, &renderer, 0.05),
            chunk_animation_frame_key(&map, 0, bounds, &renderer, 0.25)
        );
    }

    #[test]
    fn animated_chunks_change_key_only_when_frame_changes() {
        let texture = Handle::<TextureAsset>::new(AssetId::new());
        let renderer = TilemapRenderer::new(
            TilemapHandle::new(0, 0),
            TilesetGrid::new(texture, [16, 16], 4, 1).animations([TileAnimation::new(
                TileId(1),
                [
                    TileAnimationFrame::new(TileId(1), 100),
                    TileAnimationFrame::new(TileId(2), 100),
                ],
            )]),
        );
        let mut map = Tilemap::new(TilemapDescriptor::new(2, 1, 1));
        assert!(map.set_tile(0, 0, 0, Tile::new(TileId(1))).is_some());
        assert!(map.set_tile(0, 1, 0, Tile::new(TileId(0))).is_some());
        let bounds = map.chunk_bounds(0, 0).unwrap();

        let early = chunk_animation_frame_key(&map, 0, bounds, &renderer, 0.05);
        let same_frame = chunk_animation_frame_key(&map, 0, bounds, &renderer, 0.08);
        let next_frame = chunk_animation_frame_key(&map, 0, bounds, &renderer, 0.15);

        assert_eq!(early, same_frame);
        assert_ne!(early, next_frame);
    }
}
