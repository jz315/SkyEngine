use crate::asset::Assets;
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::extract::ExtractContext;
use crate::render::lighting::shadow::append_directional_shadow_views;
use crate::render::phase::{OpaquePhase, TransparentPhase};
use crate::render::resources::texture_cache::SharedRenderAssetCache;
use crate::render::view::{fallback_scene_view, SceneView};

use super::{ExtractedFrame, FrameInputs, FrameRuntimeParts};

pub(crate) fn extract_frame(
    parts: &mut FrameRuntimeParts<'_>,
    gpu: &mut GpuContext,
    world: &World,
    inputs: &FrameInputs,
    asset_cache: Option<&SharedRenderAssetCache>,
    asset_server: Option<&Assets>,
) -> ExtractedFrame {
    let mut views = parts.runtime.view_collector.collect_world_views(
        world,
        &inputs.resolved_transforms,
        parts.runtime.surface_size,
    );
    for feature in &mut parts.plan.runtime_features {
        feature.extract(
            world,
            &inputs.resolved_transforms,
            parts.runtime.surface_size,
        );
    }
    for feature in &parts.plan.runtime_features {
        feature.collect_views(&mut views);
    }
    let shadow_setups = append_directional_shadow_views(world, &mut views);
    let mut views = finalize_frame_views(views, parts.runtime.surface_size);
    parts.runtime.temporal.update_views(
        &mut views,
        parts.runtime.frame_settings.temporal_aa.enabled,
        parts.runtime.frame_settings.temporal_aa.jitter_scale,
    );

    for feature in &mut parts.plan.runtime_features {
        feature.prepare(gpu, &views);
    }

    let quad_mesh_handle = parts.resources.mesh_registry.ensure_builtin_quad(gpu);
    if let Some(asset_server) = asset_server {
        for event in asset_server.events_since(&mut parts.runtime.asset_event_cursor) {
            if let Some(asset_cache) = asset_cache {
                asset_cache
                    .borrow_mut()
                    .handle_asset_event(event, Some(asset_server));
            }
        }
    }

    let mut opaque_phases = Vec::with_capacity(views.len());
    let mut transparent_phases = Vec::with_capacity(views.len());
    for extractor in &mut parts.plan.extractors {
        extractor.begin_frame();
    }
    for (view_index, view) in views.iter().enumerate() {
        let mut opaque_phase = OpaquePhase::new();
        let mut transparent_phase = TransparentPhase::new();
        for extractor in &mut parts.plan.extractors {
            if !extractor.supported_view_kinds().contains(view.kind) {
                continue;
            }
            let extractor_name = extractor.name();
            if let Err(error) = extractor.extract(
                world,
                &inputs.resolved_transforms,
                view,
                &mut ExtractContext {
                    gpu,
                    asset_server,
                    render_assets: asset_cache,
                    material_registry: &mut parts.resources.material_registry,
                    mesh_registry: &parts.resources.mesh_registry,
                    opaque_phase: &mut opaque_phase,
                    transparent_phase: &mut transparent_phase,
                    quad_mesh_handle,
                },
            ) {
                eprintln!(
                    "[SkyEngine] Render extractor `{extractor_name}` failed for view \
                     {view_index}; skipping extractor output: {error}"
                );
            }
        }
        for feature in &parts.plan.runtime_features {
            feature.append_phase_items(view_index, &mut opaque_phase, &mut transparent_phase);
        }
        opaque_phase.sort();
        transparent_phase.sort();
        opaque_phases.push(opaque_phase);
        transparent_phases.push(transparent_phase);
    }

    ExtractedFrame {
        views,
        opaque_phases,
        transparent_phases,
        shadow_setups,
    }
}

fn finalize_frame_views(mut views: Vec<SceneView>, surface_size: [u32; 2]) -> Vec<SceneView> {
    if views.is_empty() {
        views.push(fallback_scene_view(surface_size));
    }
    views.sort_by_key(|view| (view.order, if view.presents_to_surface() { 1 } else { 0 }));
    let mut cleared_surface = false;
    for (index, view) in views.iter_mut().enumerate() {
        view.set_execution_order(index as i32);
        view.clear_surface = !cleared_surface && view.presents_to_surface();
        if view.clear_surface {
            cleared_surface = true;
        }
        if view.target_size[0] == 0 || view.target_size[1] == 0 {
            view.target_size = view.viewport.size();
        }
    }
    views
}
