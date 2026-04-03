//! HDR tonemapping.

use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::gpu::{
    BindGroup, BindGroupDesc, BindGroupEntry, BindGroupLayout, BindGroupLayoutDesc, BindingType,
    Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, Gpu, ShaderStages,
    TextureFormat,
};
use crate::render::fullscreen::{FullscreenPass, FullscreenPipeline};
use crate::render::postfx::PostFx;
use crate::render::target::RenderTarget;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ToneMapUniform {
    params: [f32; 4], // exposure, gamma, _, _
}

const TONEMAP_SHADER: &str = include_str!("../shaders/tonemap.wgsl");

pub struct ToneMap {
    pipeline: FullscreenPipeline,
    texture_bgl: BindGroupLayout,
    params_bgl: BindGroupLayout,
    params_buffer: Buffer,
    params_bind_group: BindGroup,
    bind_groups: FxHashMap<(crate::gpu::Image, crate::gpu::Sampler), BindGroup>,
    pub exposure: f32,
    pub gamma: f32,
}

impl ToneMap {
    pub fn new(gpu: &mut impl Gpu, target_format: TextureFormat) -> Self {
        let texture_bgl = gpu.create_bind_group_layout(&BindGroupLayoutDesc {
            label: Cow::Borrowed("tonemap_texture_bgl"),
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
            ],
        });
        let params_bgl = gpu.create_bind_group_layout(&BindGroupLayoutDesc {
            label: Cow::Borrowed("tonemap_params_bgl"),
            entries: vec![crate::gpu::BindGroupLayoutEntry {
                binding: 0,
                ty: BindingType::UniformBuffer,
                visibility: ShaderStages::FRAGMENT,
            }],
        });
        let params_buffer = gpu.create_buffer(&BufferDesc {
            label: Cow::Borrowed("tonemap_params_buf"),
            size: std::mem::size_of::<ToneMapUniform>() as u64,
            usage: BufferUsage::UNIFORM | BufferUsage::COPY_DST,
        });
        let params_bind_group = gpu.create_bind_group(&BindGroupDesc {
            label: Cow::Borrowed("tonemap_params_bg"),
            layout: params_bgl,
            entries: vec![BindGroupEntry::Buffer {
                binding: 0,
                buffer: params_buffer,
                offset: 0,
                size: std::mem::size_of::<ToneMapUniform>() as u64,
            }],
        });
        let pipeline = FullscreenPipeline::new(
            gpu,
            TONEMAP_SHADER,
            "fs_main",
            &[texture_bgl, params_bgl],
            target_format,
            None,
            "tonemap_pipeline",
        );

        Self {
            pipeline,
            texture_bgl,
            params_bgl,
            params_buffer,
            params_bind_group,
            bind_groups: FxHashMap::default(),
            exposure: 1.0,
            gamma: 2.2,
        }
    }

    pub fn apply_to_surface(&mut self, gpu: &mut impl Gpu, input: &RenderTarget) {
        self.apply_inner(gpu, input, ColorTarget::Surface);
    }

    pub fn apply_to_target(
        &mut self,
        gpu: &mut impl Gpu,
        input: &RenderTarget,
        output: &RenderTarget,
    ) {
        self.apply_inner(gpu, input, ColorTarget::Image(output.image()));
    }

    fn apply_inner(&mut self, gpu: &mut impl Gpu, input: &RenderTarget, output: ColorTarget) {
        let bind_group = self.texture_bind_group(gpu, input.image(), input.sampler());
        let uniform = ToneMapUniform {
            params: [self.exposure, self.gamma.max(0.001), 0.0, 0.0],
        };
        gpu.write_buffer(self.params_buffer, 0, bytemuck::bytes_of(&uniform));

        gpu.with_render_pass(
            &crate::gpu::RenderPassDesc {
                label: Cow::Borrowed("tonemap_pass"),
                color_attachments: vec![ColorAttachment {
                    target: output,
                    clear: Some([0.0, 0.0, 0.0, 1.0]),
                }],
                depth_stencil: None,
            },
            |pass| {
                pass.set_pipeline(self.pipeline.pipeline());
                pass.set_bind_group(0, bind_group);
                pass.set_bind_group(1, self.params_bind_group);
                FullscreenPass::draw(pass);
            },
        );
    }

    fn texture_bind_group(
        &mut self,
        gpu: &mut impl Gpu,
        image: crate::gpu::Image,
        sampler: crate::gpu::Sampler,
    ) -> BindGroup {
        if let Some(bind_group) = self.bind_groups.get(&(image, sampler)) {
            return *bind_group;
        }

        let bind_group = gpu.create_bind_group(&BindGroupDesc {
            label: Cow::Borrowed("tonemap_texture_bg"),
            layout: self.texture_bgl,
            entries: vec![
                BindGroupEntry::Texture { binding: 0, image },
                BindGroupEntry::Sampler {
                    binding: 1,
                    sampler,
                },
            ],
        });
        self.bind_groups.insert((image, sampler), bind_group);
        bind_group
    }

    pub fn invalidate_cache(&mut self, gpu: &mut impl Gpu) {
        for bind_group in self.bind_groups.drain().map(|(_, bind_group)| bind_group) {
            gpu.destroy_bind_group(bind_group);
        }
    }

    pub fn destroy(&mut self, gpu: &mut impl Gpu) {
        self.invalidate_cache(gpu);
        gpu.destroy_bind_group(self.params_bind_group);
        gpu.destroy_buffer(self.params_buffer);
        gpu.destroy_bind_group_layout(self.params_bgl);
        gpu.destroy_bind_group_layout(self.texture_bgl);
        self.pipeline.destroy(gpu);
    }
}

impl PostFx for ToneMap {
    fn apply_to_target<G: Gpu>(
        &mut self,
        gpu: &mut G,
        input: &RenderTarget,
        output: &RenderTarget,
    ) {
        ToneMap::apply_to_target(self, gpu, input, output);
    }
}
