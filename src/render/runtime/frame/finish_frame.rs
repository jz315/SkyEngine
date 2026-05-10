use crate::render::resources::texture_cache::RenderAssetStats;
use crate::render::view::RenderStats;

use super::{
    ExtractedFrame, FrameExecutionSummary, FrameInputs, FrameRuntimeParts, SceneUploadFrame,
    ShadowFrameSummary,
};
use crate::render::runtime::{elapsed_ms, RenderTimingStats};

pub(crate) fn finish_frame_stats(
    parts: &mut FrameRuntimeParts<'_>,
    inputs: &FrameInputs,
    extracted: &ExtractedFrame,
    uploads: &SceneUploadFrame,
    shadows: &ShadowFrameSummary,
    render_asset_stats: RenderAssetStats,
    execution: &FrameExecutionSummary,
) {
    parts.runtime.last_stats = RenderStats {
        step_count: parts.plan.steps.len(),
        view_count: extracted.views.len(),
        light_count: uploads.lights.len(),
        draw_calls: execution.stats.draw_calls,
        shadow_cascade_count: shadows.stats.cascade_count,
        shadow_caster_count: shadows.stats.caster_count,
        shadow_caster_count_by_cascade: shadows.stats.caster_count_by_cascade,
        shadow_draw_calls: shadows.draw_calls,
        shadow_draw_calls_by_cascade: shadows.draw_calls_by_cascade,
        shadow_atlas_width: shadows.stats.atlas_size[0],
        shadow_atlas_height: shadows.stats.atlas_size[1],
        shadow_atlas_rect_count: shadows.stats.rect_count,
        shadow_atlas_used_pixel_ratio: shadows.stats.used_pixel_ratio,
        shadow_atlas_guard_band_texels: shadows.stats.guard_band_texels,
        passes: execution.stats.passes,
        resident_render_assets: render_asset_stats.resident_assets,
        uploaded_render_assets: render_asset_stats.uploaded_assets,
        uploaded_render_asset_bytes: render_asset_stats.uploaded_bytes,
        queued_render_assets: render_asset_stats.queued_assets,
        visible_queued_render_assets: render_asset_stats.visible_queued_assets,
        loading_render_assets: render_asset_stats.loading_assets,
        fallback_render_assets: render_asset_stats.fallback_assets,
        missing_render_assets: render_asset_stats.missing_assets,
        failed_render_assets: render_asset_stats.failed_assets,
        timings: RenderTimingStats {
            frame_ms: elapsed_ms(inputs.frame_start),
            execute_ms: execution.execute_ms,
            upload_ms: render_asset_stats.upload_ms,
            ..Default::default()
        },
        ..Default::default()
    };
}

pub(crate) fn remember_previous_models(parts: &mut FrameRuntimeParts<'_>, inputs: &FrameInputs) {
    parts.runtime.previous_model_by_entity.clear();
    parts.runtime.previous_model_by_entity.extend(
        inputs
            .resolved_transforms
            .iter()
            .map(|(entity, transform)| (entity, transform.to_matrix4().to_cols_array())),
    );
}
