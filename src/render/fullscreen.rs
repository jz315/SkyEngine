//! Fullscreen pass helpers.

use std::borrow::Cow;

use crate::gpu::{
    BindGroupLayout, BlendState, ColorTargetState, Gpu, Pipeline, PrimitiveState,
    RenderPassEncoder, RenderPipelineDesc, Shader, ShaderDesc, TextureFormat,
};

const FULLSCREEN_COMMON: &str = include_str!("shaders/fullscreen.wgsl");

/// Stateless fullscreen triangle drawer.
pub struct FullscreenPass;

impl FullscreenPass {
    #[inline]
    pub fn draw(pass: &mut dyn RenderPassEncoder) {
        pass.draw(0..3, 0..1);
    }
}

/// A compiled fullscreen pipeline.
#[derive(Clone, Copy)]
pub struct FullscreenPipeline {
    shader: Shader,
    pipeline: Pipeline,
}

impl FullscreenPipeline {
    pub fn new(
        gpu: &mut impl Gpu,
        fragment_source: &str,
        fs_entry: &'static str,
        bind_group_layouts: &[BindGroupLayout],
        target_format: TextureFormat,
        blend: Option<BlendState>,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        let label = label.into();
        let shader = gpu.create_shader(&ShaderDesc {
            label: Cow::Owned(format!("{}_shader", label)),
            source: compose_fullscreen_shader(fragment_source),
        });
        let pipeline = gpu.create_render_pipeline(&RenderPipelineDesc {
            label,
            shader,
            vs_entry: "vs_fullscreen",
            fs_entry,
            vertex_layouts: vec![],
            bind_group_layouts: bind_group_layouts.to_vec(),
            color_targets: vec![ColorTargetState {
                format: target_format,
                blend,
            }],
            depth_stencil: None,
            primitive: PrimitiveState::default(),
        });

        Self { shader, pipeline }
    }

    #[inline]
    pub fn shader(&self) -> Shader {
        self.shader
    }

    #[inline]
    pub fn pipeline(&self) -> Pipeline {
        self.pipeline
    }

    pub fn destroy(&self, gpu: &mut impl Gpu) {
        gpu.destroy_pipeline(self.pipeline);
        gpu.destroy_shader(self.shader);
    }
}

pub fn compose_fullscreen_shader(fragment_source: &str) -> Cow<'static, str> {
    Cow::Owned(format!("{FULLSCREEN_COMMON}\n{fragment_source}"))
}
