//! Scene/light composite pass.

use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::gpu::{
    BindGroup, BindGroupDesc, BindGroupEntry, BindGroupLayout, BindGroupLayoutDesc, BindingType,
    ColorAttachment, ColorTarget, Gpu, Sampler, ShaderStages, TextureFormat,
};
use crate::render::fullscreen::{FullscreenPass, FullscreenPipeline};
use crate::render::target::RenderTarget;

const COMPOSITE_SHADER: &str = include_str!("shaders/composite.wgsl");

/// scene_color * lightmap_color -> output
pub struct CompositePass {
    pipeline: FullscreenPipeline,
    bind_group_layout: BindGroupLayout,
    bind_groups: FxHashMap<(crate::gpu::Image, Sampler, crate::gpu::Image, Sampler), BindGroup>,
}

impl CompositePass {
    pub fn new(gpu: &mut impl Gpu, target_format: TextureFormat) -> Self {
        let bind_group_layout = gpu.create_bind_group_layout(&BindGroupLayoutDesc {
            label: Cow::Borrowed("composite_bgl"),
            entries: vec![
                crate::gpu::BindGroupLayoutEntry {
                    binding: 0,
                    ty: BindingType::Texture,
                    visibility: ShaderStages::FRAGMENT,
                },
                crate::gpu::BindGroupLayoutEntry {
                    binding: 1,
                    ty: BindingType::Sampler,
                    visibility: ShaderStages::FRAGMENT,
                },
                crate::gpu::BindGroupLayoutEntry {
                    binding: 2,
                    ty: BindingType::Texture,
                    visibility: ShaderStages::FRAGMENT,
                },
                crate::gpu::BindGroupLayoutEntry {
                    binding: 3,
                    ty: BindingType::Sampler,
                    visibility: ShaderStages::FRAGMENT,
                },
            ],
        });
        let pipeline = FullscreenPipeline::new(
            gpu,
            COMPOSITE_SHADER,
            "fs_main",
            &[bind_group_layout],
            target_format,
            None,
            "composite_pipeline",
        );

        Self {
            pipeline,
            bind_group_layout,
            bind_groups: FxHashMap::default(),
        }
    }

    pub fn render_to_target(
        &mut self,
        gpu: &mut impl Gpu,
        scene: &RenderTarget,
        lightmap: &RenderTarget,
        output: &RenderTarget,
    ) {
        self.render_inner(
            gpu,
            scene,
            lightmap,
            ColorTarget::Image(output.image()),
            Some([0.0, 0.0, 0.0, 1.0]),
        );
    }

    pub fn render_to_surface(
        &mut self,
        gpu: &mut impl Gpu,
        scene: &RenderTarget,
        lightmap: &RenderTarget,
    ) {
        self.render_inner(
            gpu,
            scene,
            lightmap,
            ColorTarget::Surface,
            Some([0.0, 0.0, 0.0, 1.0]),
        );
    }

    fn render_inner(
        &mut self,
        gpu: &mut impl Gpu,
        scene: &RenderTarget,
        lightmap: &RenderTarget,
        output: ColorTarget,
        clear: Option<[f32; 4]>,
    ) {
        let bind_group = self.bind_group(
            gpu,
            scene.image(),
            scene.sampler(),
            lightmap.image(),
            lightmap.sampler(),
        );

        gpu.with_render_pass(
            &crate::gpu::RenderPassDesc {
                label: Cow::Borrowed("composite_pass"),
                color_attachments: vec![ColorAttachment {
                    target: output,
                    clear,
                }],
                depth_stencil: None,
            },
            |pass| {
                pass.set_pipeline(self.pipeline.pipeline());
                pass.set_bind_group(0, bind_group);
                FullscreenPass::draw(pass);
            },
        );
    }

    fn bind_group(
        &mut self,
        gpu: &mut impl Gpu,
        scene_image: crate::gpu::Image,
        scene_sampler: Sampler,
        light_image: crate::gpu::Image,
        light_sampler: Sampler,
    ) -> BindGroup {
        let key = (scene_image, scene_sampler, light_image, light_sampler);
        if let Some(bind_group) = self.bind_groups.get(&key) {
            return *bind_group;
        }

        let bind_group = gpu.create_bind_group(&BindGroupDesc {
            label: Cow::Borrowed("composite_bg"),
            layout: self.bind_group_layout,
            entries: vec![
                BindGroupEntry::Texture {
                    binding: 0,
                    image: scene_image,
                },
                BindGroupEntry::Sampler {
                    binding: 1,
                    sampler: scene_sampler,
                },
                BindGroupEntry::Texture {
                    binding: 2,
                    image: light_image,
                },
                BindGroupEntry::Sampler {
                    binding: 3,
                    sampler: light_sampler,
                },
            ],
        });
        self.bind_groups.insert(key, bind_group);
        bind_group
    }

    pub fn invalidate_cache(&mut self, gpu: &mut impl Gpu) {
        for bind_group in self.bind_groups.drain().map(|(_, bind_group)| bind_group) {
            gpu.destroy_bind_group(bind_group);
        }
    }

    pub fn destroy(&mut self, gpu: &mut impl Gpu) {
        self.invalidate_cache(gpu);
        gpu.destroy_bind_group_layout(self.bind_group_layout);
        self.pipeline.destroy(gpu);
    }
}
