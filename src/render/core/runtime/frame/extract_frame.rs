use crate::asset::Assets;
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::extract::ExtractContext;
use crate::render::phase::{OpaquePhase, TransparentPhase};
use crate::render::resources::texture_cache::SharedRenderAssetCache;
use crate::render::runtime::FrameExtensionViewContext;
use crate::render::view::{fallback_scene_view, SceneView};

use super::{ExtractedFrame, FrameInputs, FrameRuntimeParts};

pub(crate) fn extract_frame(
    parts: &mut FrameRuntimeParts<'_>,
    gpu: &mut GpuContext,
    world: &World,
    inputs: &FrameInputs,
    asset_cache: Option<&SharedRenderAssetCache>,
    asset_server: Option<&Assets>,
    mut extracted: ExtractedFrame,
) -> ExtractedFrame {
    parts.runtime.view_collector.collect_world_views_into(
        world,
        &inputs.resolved_transforms,
        parts.runtime.surface_size,
        &mut extracted.views,
    );
    for feature in &mut parts.plan.runtime_features {
        feature.extract(
            world,
            &inputs.resolved_transforms,
            parts.runtime.surface_size,
        );
    }
    for feature in &parts.plan.runtime_features {
        feature.collect_views(&mut extracted.views);
    }
    for extension in &mut parts.plan.frame_extensions {
        extension.collect_views(FrameExtensionViewContext {
            world,
            views: &mut extracted.views,
        });
    }
    finalize_frame_views(&mut extracted.views, parts.runtime.surface_size);
    parts.runtime.temporal.update_views(
        &mut extracted.views,
        parts.runtime.frame_settings.temporal_aa.enabled,
        parts.runtime.frame_settings.temporal_aa.jitter_scale,
    );

    for feature in &mut parts.plan.runtime_features {
        feature.prepare(gpu, &extracted.views);
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

    extracted
        .opaque_phases
        .resize_with(extracted.views.len(), OpaquePhase::new);
    extracted
        .transparent_phases
        .resize_with(extracted.views.len(), TransparentPhase::new);
    for phase in &mut extracted.opaque_phases {
        phase.clear();
    }
    for phase in &mut extracted.transparent_phases {
        phase.clear();
    }
    for extractor in &mut parts.plan.extractors {
        extractor.begin_frame();
    }
    for (view_index, view) in extracted.views.iter().enumerate() {
        let opaque_phase = &mut extracted.opaque_phases[view_index];
        let transparent_phase = &mut extracted.transparent_phases[view_index];
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
                    opaque_phase,
                    transparent_phase,
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
            feature.append_phase_items(view_index, opaque_phase, transparent_phase);
        }
        opaque_phase.sort();
        transparent_phase.sort();
    }

    extracted
}

fn finalize_frame_views(views: &mut Vec<SceneView>, surface_size: [u32; 2]) {
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
}
