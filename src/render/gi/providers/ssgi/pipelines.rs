use super::bindings::SsgiBindGroupLayouts;
use super::constants::{
    SSGI_COMPOSITE_SHADER, SSGI_COMPUTE_SHADER, SSGI_DEINTERLEAVE_COMPUTE_SHADER,
    SSGI_FINAL_SHADER, SSGI_UPSAMPLE_COMPUTE_SHADER,
};
use super::contract::SsgiShaderStage;
use crate::gpu::GpuContext;
use crate::render::gpu::{ComputePipelineCache, FullscreenPipeline};
use std::sync::Arc;

#[derive(Default)]
pub(crate) struct SsgiPipelineCache {
    final_pipeline: Option<FullscreenPipeline>,
    composite_pipeline: Option<FullscreenPipeline>,
    compute_deinterleave_pipeline: Option<ComputePipelineCache>,
    compute_diffuse_pipeline: Option<ComputePipelineCache>,
    compute_upsample_pipeline: Option<ComputePipelineCache>,
}

impl SsgiPipelineCache {
    pub(crate) fn ensure_final(
        &mut self,
        gpu: &GpuContext,
        layouts: &SsgiBindGroupLayouts,
        target_format: wgpu::TextureFormat,
    ) {
        if self.final_pipeline.is_none() {
            self.final_pipeline = Some(FullscreenPipeline::new(
                gpu,
                SSGI_FINAL_SHADER,
                "fs_final_upsample",
                &[layouts.final_textures(), layouts.uniform()],
                target_format,
                None,
                "ssgi_final_pipeline",
            ));
        }
        if self.composite_pipeline.is_none() {
            self.composite_pipeline = Some(FullscreenPipeline::new(
                gpu,
                SSGI_COMPOSITE_SHADER,
                "fs_scene_composite",
                &[layouts.composite_textures(), layouts.uniform()],
                target_format,
                None,
                "ssgi_composite_pipeline",
            ));
        }
    }

    pub(crate) fn ensure_compute(&mut self, gpu: &GpuContext, layouts: &SsgiBindGroupLayouts) {
        if self.compute_deinterleave_pipeline.is_none() {
            self.compute_deinterleave_pipeline = Some(ComputePipelineCache::new(
                gpu,
                SSGI_DEINTERLEAVE_COMPUTE_SHADER,
                "cs_main",
                &[
                    layouts.compute_scene(),
                    layouts.uniform(),
                    layouts.deinterleave_output(),
                ],
                "ssgi_compute_deinterleave",
            ));
        }

        if self.compute_diffuse_pipeline.is_none() {
            self.compute_diffuse_pipeline = Some(ComputePipelineCache::new(
                gpu,
                SSGI_COMPUTE_SHADER,
                "cs_main",
                &[
                    layouts.diffuse_input(),
                    layouts.uniform(),
                    layouts.diffuse_output(),
                ],
                "ssgi_compute_diffuse",
            ));
        }

        if self.compute_upsample_pipeline.is_none() {
            self.compute_upsample_pipeline = Some(ComputePipelineCache::new(
                gpu,
                SSGI_UPSAMPLE_COMPUTE_SHADER,
                "cs_main",
                &[
                    layouts.upsample_input(),
                    layouts.uniform(),
                    layouts.upsample_output(),
                ],
                "ssgi_compute_upsample",
            ));
        }
    }

    pub(crate) fn compute_pipeline(
        &mut self,
        gpu: &GpuContext,
        stage: SsgiShaderStage,
    ) -> Arc<wgpu::ComputePipeline> {
        match stage {
            SsgiShaderStage::DeinterleaveCompute => self
                .compute_deinterleave_pipeline
                .as_mut()
                .expect("SSGI deinterleave compute pipeline should exist")
                .pipeline(gpu),
            SsgiShaderStage::DiffuseCompute => self
                .compute_diffuse_pipeline
                .as_mut()
                .expect("SSGI diffuse compute pipeline should exist")
                .pipeline(gpu),
            SsgiShaderStage::UpsampleCompute => self
                .compute_upsample_pipeline
                .as_mut()
                .expect("SSGI upsample compute pipeline should exist")
                .pipeline(gpu),
            SsgiShaderStage::FinalFullscreen => {
                panic!("SSGI final shader stage is not a compute pipeline")
            }
            SsgiShaderStage::CompositeFullscreen => {
                panic!("SSGI composite shader stage is not a compute pipeline")
            }
        }
    }

    pub(crate) fn fullscreen_pipeline(
        &mut self,
        gpu: &GpuContext,
        stage: SsgiShaderStage,
        target_format: wgpu::TextureFormat,
    ) -> Arc<wgpu::RenderPipeline> {
        match stage {
            SsgiShaderStage::FinalFullscreen => self
                .final_pipeline
                .as_mut()
                .expect("SSGI final pipeline should exist")
                .pipeline(gpu, target_format),
            SsgiShaderStage::CompositeFullscreen => self
                .composite_pipeline
                .as_mut()
                .expect("SSGI composite pipeline should exist")
                .pipeline(gpu, target_format),
            _ => panic!("SSGI shader stage is not a fullscreen pipeline"),
        }
    }
}

#[cfg(test)]
pub(crate) fn ssgi_shader_source(stage: SsgiShaderStage) -> &'static str {
    match stage {
        SsgiShaderStage::DeinterleaveCompute => SSGI_DEINTERLEAVE_COMPUTE_SHADER,
        SsgiShaderStage::DiffuseCompute => SSGI_COMPUTE_SHADER,
        SsgiShaderStage::UpsampleCompute => SSGI_UPSAMPLE_COMPUTE_SHADER,
        SsgiShaderStage::FinalFullscreen => SSGI_FINAL_SHADER,
        SsgiShaderStage::CompositeFullscreen => SSGI_COMPOSITE_SHADER,
    }
}
