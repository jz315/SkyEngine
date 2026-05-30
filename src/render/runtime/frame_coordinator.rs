use crate::asset::Assets;
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::resources::texture_cache::SharedRenderAssetCache;
use crate::render::runtime::{FrameRenderOutcome, FrameSkipReason};

use super::frame::{
    begin_frame_inputs, execute_prepared_frame, extract_frame, finish_frame_stats,
    finish_render_assets, finish_skipped_frame_stats, prepare_frame_assets,
    prepare_global_illumination, prepare_shadows, remember_previous_models, upload_scene_data,
    FrameRuntimeParts,
};
use super::runtime::RenderRuntime;

pub(crate) struct FrameCoordinator;

impl FrameCoordinator {
    #[inline]
    pub(crate) fn new() -> Self {
        Self
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
                shadows: &mut self.shadows,
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
            begin_frame_inputs(&mut parts, gpu, world, Some(asset_cache))
        };
        let mut extracted = {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "extract_frame");
            extract_frame(
                &mut parts,
                gpu,
                world,
                &inputs,
                Some(asset_cache),
                asset_server.as_ref(),
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
            return FrameRenderOutcome::Skipped(FrameSkipReason::ResourcePreparationFailed);
        }
        let uploads = {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "upload_scene_data");
            upload_scene_data(&mut parts, gpu, world, &inputs, &mut extracted)
        };
        {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "prepare_global_illumination");
            prepare_global_illumination(&mut parts, gpu, &extracted, &uploads);
        }
        let shadows = {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "prepare_shadows");
            prepare_shadows(&mut parts, gpu, &extracted, &uploads)
        };
        let render_asset_stats = {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "finish_render_assets");
            finish_render_assets(Some(asset_cache))
        };
        let execution = {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "execute_prepared_frame");
            execute_prepared_frame(&mut parts, gpu, &extracted, &uploads, &shadows)
        };
        {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "finish_frame_stats");
            finish_frame_stats(
                &mut parts,
                &inputs,
                &extracted,
                &uploads,
                &shadows,
                render_asset_stats,
                &execution,
            );
        }
        {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("render", "remember_previous_models");
            remember_previous_models(&mut parts, &inputs);
        }
        FrameRenderOutcome::Rendered
    }
}
