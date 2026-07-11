use crate::render::resources::texture_cache::RenderAssetStats;
use crate::render::view::RenderStats;

use super::{
    ExtractedFrame, FrameExecutionSummary, FrameInputs, FrameRuntimeParts, SceneUploadFrame,
};
use crate::render::runtime::{elapsed_ms, RenderTimingStats};

pub(crate) fn finish_frame_stats(
    parts: &mut FrameRuntimeParts<'_>,
    inputs: &FrameInputs,
    extracted: &ExtractedFrame,
    uploads: &SceneUploadFrame,
    render_asset_stats: RenderAssetStats,
    execution: &FrameExecutionSummary,
) {
    let mut stats = RenderStats {
        step_count: parts.plan.steps.len(),
        view_count: extracted.views.len(),
        light_count: uploads.lights.len(),
        draw_calls: execution.stats.draw_calls,
        passes: execution.stats.passes,
        resident_render_assets: render_asset_stats.resident_assets,
        resident_render_asset_bytes: render_asset_stats.resident_bytes,
        uploaded_render_assets: render_asset_stats.uploaded_assets,
        uploaded_render_asset_bytes: render_asset_stats.uploaded_bytes,
        evicted_render_assets: render_asset_stats.evicted_assets,
        evicted_render_asset_bytes: render_asset_stats.evicted_bytes,
        cached_failed_render_assets: render_asset_stats.cached_failed_assets,
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
    for extension in &parts.plan.frame_extensions {
        extension.update_render_stats(&mut stats);
    }
    parts.runtime.last_stats = stats;
}

pub(crate) fn finish_skipped_frame_stats(
    parts: &mut FrameRuntimeParts<'_>,
    inputs: &FrameInputs,
    extracted: &ExtractedFrame,
    render_asset_stats: RenderAssetStats,
) {
    let mut stats = RenderStats {
        step_count: parts.plan.steps.len(),
        view_count: extracted.views.len(),
        resident_render_assets: render_asset_stats.resident_assets,
        resident_render_asset_bytes: render_asset_stats.resident_bytes,
        uploaded_render_assets: render_asset_stats.uploaded_assets,
        uploaded_render_asset_bytes: render_asset_stats.uploaded_bytes,
        evicted_render_assets: render_asset_stats.evicted_assets,
        evicted_render_asset_bytes: render_asset_stats.evicted_bytes,
        cached_failed_render_assets: render_asset_stats.cached_failed_assets,
        queued_render_assets: render_asset_stats.queued_assets,
        visible_queued_render_assets: render_asset_stats.visible_queued_assets,
        loading_render_assets: render_asset_stats.loading_assets,
        fallback_render_assets: render_asset_stats.fallback_assets,
        missing_render_assets: render_asset_stats.missing_assets,
        failed_render_assets: render_asset_stats.failed_assets,
        timings: RenderTimingStats {
            frame_ms: elapsed_ms(inputs.frame_start),
            upload_ms: render_asset_stats.upload_ms,
            ..Default::default()
        },
        ..Default::default()
    };
    for extension in &parts.plan.frame_extensions {
        extension.update_render_stats(&mut stats);
    }
    parts.runtime.last_stats = stats;
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
