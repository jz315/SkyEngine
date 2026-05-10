use crate::gpu::GpuContext;
use crate::render::lighting::shadow::{sync_shadow_views, ShadowDebugResources, ShadowViewBinding};
use crate::render::phase::{MeshDrawData, OpaquePhase, PhaseItem, TransparentPhase};
use crate::render::view::SceneView;
use crate::render::LightTable;

use super::{
    ExtractedFrame, FrameRuntimeParts, SceneUploadFrame, ShadowFrameStats, ShadowFrameSummary,
};

pub(crate) fn prepare_shadows(
    parts: &mut FrameRuntimeParts<'_>,
    gpu: &mut GpuContext,
    extracted: &ExtractedFrame,
    uploads: &SceneUploadFrame,
) -> ShadowFrameSummary {
    sync_shadow_views(
        &mut parts.shadows.views,
        gpu,
        &extracted.views,
        &extracted.opaque_phases,
        &extracted.transparent_phases,
        &uploads.model_matrices,
        &extracted.shadow_setups,
        parts
            .shadows
            .layout
            .as_ref()
            .expect("shadow runtime should initialize a shared shadow layout"),
        parts
            .shadows
            .pass_layout
            .as_ref()
            .expect("shadow runtime should initialize a shared shadow-pass layout"),
        parts
            .shadows
            .compare_sampler
            .as_ref()
            .expect("shadow runtime should initialize a shared shadow sampler"),
        parts
            .runtime
            .gpu_scene
            .as_ref()
            .expect("phase runtime should initialize a GpuScene")
            .table::<LightTable>(),
        parts.runtime.frame_settings.debug_view,
    );
    summarize_shadow_frame(extracted, &parts.shadows.views)
}

pub(crate) fn summarize_shadow_frame(
    extracted: &ExtractedFrame,
    shadow_views: &[ShadowViewBinding],
) -> ShadowFrameSummary {
    ShadowFrameSummary {
        debug_resources: shadow_views
            .iter()
            .find_map(ShadowDebugResources::from_directional_shadow),
        stats: collect_shadow_stats(shadow_views),
        draw_calls: count_shadow_draw_calls(
            &extracted.views,
            &extracted.opaque_phases,
            &extracted.transparent_phases,
            shadow_views,
        ),
        draw_calls_by_cascade: count_shadow_draw_calls_by_cascade(
            &extracted.views,
            &extracted.opaque_phases,
            &extracted.transparent_phases,
            shadow_views,
        ),
    }
}

fn collect_shadow_stats(shadow_views: &[ShadowViewBinding]) -> ShadowFrameStats {
    let mut stats = ShadowFrameStats::default();
    let mut atlas_area = 0u64;
    let mut used_area = 0f64;

    for shadow_view in shadow_views.iter().filter(|shadow| shadow.enabled()) {
        let atlas = shadow_view.atlas_stats();
        stats.cascade_count += atlas.active_cascade_count as usize;
        stats.caster_count += shadow_view.caster_count();
        for (index, count) in shadow_view
            .caster_count_by_cascade()
            .into_iter()
            .enumerate()
        {
            stats.caster_count_by_cascade[index] += count;
        }
        stats.rect_count += atlas.used_rect_count as usize;
        stats.atlas_size[0] = stats.atlas_size[0].max(atlas.atlas_size[0]);
        stats.atlas_size[1] = stats.atlas_size[1].max(atlas.atlas_size[1]);
        stats.guard_band_texels = stats.guard_band_texels.max(atlas.guard_band_texels);
        let area = atlas.atlas_size[0] as u64 * atlas.atlas_size[1] as u64;
        atlas_area += area;
        used_area += atlas.used_pixel_ratio as f64 * area as f64;
    }

    if atlas_area > 0 {
        stats.used_pixel_ratio = (used_area / atlas_area as f64) as f32;
    }

    stats
}

fn count_shadow_draw_calls(
    views: &[SceneView],
    opaque_phases: &[OpaquePhase],
    transparent_phases: &[TransparentPhase],
    shadow_views: &[ShadowViewBinding],
) -> usize {
    views
        .iter()
        .enumerate()
        .filter(|(_, view)| view.is_shadow())
        .filter(|(_, view)| {
            view.shadow_binding()
                .and_then(|binding| shadow_views.get(binding))
                .is_some_and(|shadow| {
                    shadow.enabled() && shadow.should_update_cascade(view.shadow_cascade())
                })
        })
        .map(|(index, _)| {
            opaque_phases
                .get(index)
                .map_or(0, |phase| count_shadow_batches(phase.items()))
                + transparent_phases
                    .get(index)
                    .map_or(0, |phase| count_shadow_batches(phase.items()))
        })
        .sum()
}

fn count_shadow_draw_calls_by_cascade(
    views: &[SceneView],
    opaque_phases: &[OpaquePhase],
    transparent_phases: &[TransparentPhase],
    shadow_views: &[ShadowViewBinding],
) -> [usize; crate::render::component::MAX_DIRECTIONAL_SHADOW_CASCADES] {
    let mut draw_calls = [0usize; crate::render::component::MAX_DIRECTIONAL_SHADOW_CASCADES];
    for (view_index, view) in views
        .iter()
        .enumerate()
        .filter(|(_, view)| view.is_shadow())
    {
        let cascade = view.shadow_cascade() as usize;
        if cascade >= draw_calls.len() {
            continue;
        }
        let Some(shadow_view) = view
            .shadow_binding()
            .and_then(|binding| shadow_views.get(binding))
        else {
            continue;
        };
        if !shadow_view.enabled() || !shadow_view.should_update_cascade(view.shadow_cascade()) {
            continue;
        }
        draw_calls[cascade] += opaque_phases
            .get(view_index)
            .map_or(0, |phase| count_shadow_batches(phase.items()))
            + transparent_phases
                .get(view_index)
                .map_or(0, |phase| count_shadow_batches(phase.items()));
    }
    draw_calls
}

fn count_shadow_batches(items: &[PhaseItem]) -> usize {
    let mut draws = 0usize;
    let mut cursor = 0usize;
    while cursor < items.len() {
        let base = *items[cursor].data::<MeshDrawData>();
        let base_draw_function = items[cursor].draw_function_id;
        let base_material = base.material_handle::<crate::render::StandardMaterial>();
        let mut batch_end = cursor + 1;
        while batch_end < items.len() {
            let next = *items[batch_end].data::<MeshDrawData>();
            let next_draw_function = items[batch_end].draw_function_id;
            if next.mesh_handle() != base.mesh_handle()
                || next.sub_mesh_index() != base.sub_mesh_index()
                || next_draw_function != base_draw_function
                || next.material_handle::<crate::render::StandardMaterial>() != base_material
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
