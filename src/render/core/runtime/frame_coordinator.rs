use crate::asset::Assets;
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::resources::texture_cache::SharedRenderAssetCache;
use crate::render::runtime::{FrameRenderOutcome, FrameSkipReason};
use crate::render::view::ResolvedSceneTransforms;

use super::engine_runtime::RenderRuntime;
use super::frame::{
    begin_frame_inputs, execute_prepared_frame, extract_frame, finish_frame_stats,
    finish_render_assets, finish_skipped_frame_stats, prepare_frame_assets,
    prepare_frame_extensions, remember_previous_models, upload_scene_data, ExtractedFrame,
    FrameRuntimeParts, SceneUploadFrame,
};

pub(crate) struct FrameCoordinator {
    resolved_transforms: ResolvedSceneTransforms,
    extracted_frame: ExtractedFrame,
    scene_upload: SceneUploadFrame,
}

impl FrameCoordinator {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            resolved_transforms: ResolvedSceneTransforms::default(),
            extracted_frame: ExtractedFrame::default(),
            scene_upload: SceneUploadFrame::default(),
        }
    }
}

impl Default for FrameCoordinator {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl RenderRuntime {
    pub fn render_world(&mut self, gpu: &mut GpuContext, world: &World) -> FrameRenderOutcome {
        let asset_cache = &self.asset_cache;
        self.frame.render_world(
            gpu,
            world,
            asset_cache,
            FrameRuntimeParts {
                plan: &mut self.plan,
                resources: &mut self.resources,
                runtime: &mut self.runtime,
                executor: &mut self.executor,
            },
        )
    }
}

impl FrameCoordinator {
    fn render_world(
        &mut self,
        gpu: &mut GpuContext,
        world: &World,
        asset_cache: &SharedRenderAssetCache,
        mut parts: FrameRuntimeParts<'_>,
    ) -> FrameRenderOutcome {
        #[cfg(feature = "profile")]
        let _render_scope = sky_profile::profile_scope!("render", "RenderRuntime::render_world");

        let asset_server = world.get_resource::<Assets>().cloned();
        let inputs = {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "begin_frame_inputs");
            let reusable_transforms = std::mem::take(&mut self.resolved_transforms);
            begin_frame_inputs(
                &mut parts,
                gpu,
                world,
                Some(asset_cache),
                reusable_transforms,
            )
        };
        let mut extracted = {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "extract_frame");
            let reusable_extracted = std::mem::take(&mut self.extracted_frame);
            extract_frame(
                &mut parts,
                gpu,
                world,
                &inputs,
                Some(asset_cache),
                asset_server.as_ref(),
                reusable_extracted,
            )
        };
        if !{
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "prepare_frame_assets");
            prepare_frame_assets(&mut parts, gpu, Some(asset_cache))
        } {
            let render_asset_stats = finish_render_assets(Some(asset_cache));
            finish_skipped_frame_stats(&mut parts, &inputs, &extracted, render_asset_stats);
            remember_previous_models(&mut parts, &inputs);
            invalidate_temporal_history(&mut parts);
            self.resolved_transforms = inputs.resolved_transforms;
            self.extracted_frame = extracted;
            return FrameRenderOutcome::Skipped(FrameSkipReason::ResourcePreparationFailed);
        }
        let uploads = {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "upload_scene_data");
            let reusable_upload = std::mem::take(&mut self.scene_upload);
            upload_scene_data(
                &mut parts,
                gpu,
                world,
                &inputs,
                &mut extracted,
                reusable_upload,
            )
        };
        if let Err(error) = {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "prepare_frame_extensions");
            prepare_frame_extensions(&mut parts, gpu, &extracted, &uploads)
        } {
            eprintln!(
                "[SkyEngine] Render frame extension `{}` failed; skipping frame: {}",
                error.extension, error.message
            );
            let render_asset_stats = finish_render_assets(Some(asset_cache));
            finish_skipped_frame_stats(&mut parts, &inputs, &extracted, render_asset_stats);
            remember_previous_models(&mut parts, &inputs);
            invalidate_temporal_history(&mut parts);
            self.resolved_transforms = inputs.resolved_transforms;
            self.extracted_frame = extracted;
            self.scene_upload = uploads;
            return FrameRenderOutcome::Skipped(FrameSkipReason::ResourcePreparationFailed);
        }
        let render_asset_stats = {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "finish_render_assets");
            finish_render_assets(Some(asset_cache))
        };
        let execution = {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "execute_prepared_frame");
            execute_prepared_frame(&mut parts, gpu, &extracted, &uploads)
        };
        {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "finish_frame_stats");
            finish_frame_stats(
                &mut parts,
                &inputs,
                &extracted,
                &uploads,
                render_asset_stats,
                &execution,
            );
        }
        {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "remember_previous_models");
            remember_previous_models(&mut parts, &inputs);
        }
        self.resolved_transforms = inputs.resolved_transforms;
        self.extracted_frame = extracted;
        self.scene_upload = uploads;
        if execution.succeeded {
            FrameRenderOutcome::Rendered
        } else {
            invalidate_temporal_history(&mut parts);
            FrameRenderOutcome::Skipped(FrameSkipReason::RenderGraphFailed)
        }
    }
}

fn invalidate_temporal_history(parts: &mut FrameRuntimeParts<'_>) {
    parts.runtime.temporal.invalidate();
    parts.runtime.history.invalidate();
    for extension in &mut parts.plan.frame_extensions {
        extension.invalidate();
    }
}
