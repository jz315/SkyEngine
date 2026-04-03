//! Bloom post-processing.

use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::gpu::{
    BindGroup, BindGroupDesc, BindGroupEntry, BindGroupLayout, BindGroupLayoutDesc, BindingType,
    Buffer, BufferDesc, BufferUsage, ColorAttachment, ColorTarget, Gpu, Sampler, ShaderStages,
    TextureFormat,
};
use crate::render::fullscreen::{FullscreenPass, FullscreenPipeline};
use crate::render::postfx::PostFx;
use crate::render::target::RenderTarget;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BloomUniform {
    params: [f32; 4],    // threshold, intensity, radius, _
    texel_dir: [f32; 4], // texel_x, texel_y, dir_x, dir_y
}

const BLOOM_SHADER: &str = include_str!("../shaders/bloom.wgsl");
const BLOOM_LEVELS: usize = 4;

pub struct Bloom {
    bright_pipeline: FullscreenPipeline,
    downsample_pipeline: FullscreenPipeline,
    blur_pipeline: FullscreenPipeline,
    upsample_pipeline: FullscreenPipeline,
    combine_pipeline: FullscreenPipeline,
    sample_bgl: BindGroupLayout,
    dual_bgl: BindGroupLayout,
    params_bgl: BindGroupLayout,
    params_buffer: Buffer,
    params_bind_group: BindGroup,
    sample_bind_groups: FxHashMap<(crate::gpu::Image, Sampler), BindGroup>,
    dual_bind_groups:
        FxHashMap<(crate::gpu::Image, Sampler, crate::gpu::Image, Sampler), BindGroup>,
    mip_chain: Vec<RenderTarget>,
    temp_targets: Vec<RenderTarget>,
    pub threshold: f32,
    pub intensity: f32,
    pub radius: f32,
}

impl Bloom {
    pub fn new(gpu: &mut impl Gpu, width: u32, height: u32, target_format: TextureFormat) -> Self {
        let sample_bgl = gpu.create_bind_group_layout(&BindGroupLayoutDesc {
            label: Cow::Borrowed("bloom_sample_bgl"),
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
        let dual_bgl = gpu.create_bind_group_layout(&BindGroupLayoutDesc {
            label: Cow::Borrowed("bloom_dual_bgl"),
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
        let params_bgl = gpu.create_bind_group_layout(&BindGroupLayoutDesc {
            label: Cow::Borrowed("bloom_params_bgl"),
            entries: vec![crate::gpu::BindGroupLayoutEntry {
                binding: 0,
                ty: BindingType::UniformBuffer,
                visibility: ShaderStages::FRAGMENT,
            }],
        });
        let params_buffer = gpu.create_buffer(&BufferDesc {
            label: Cow::Borrowed("bloom_params_buf"),
            size: std::mem::size_of::<BloomUniform>() as u64,
            usage: BufferUsage::UNIFORM | BufferUsage::COPY_DST,
        });
        let params_bind_group = gpu.create_bind_group(&BindGroupDesc {
            label: Cow::Borrowed("bloom_params_bg"),
            layout: params_bgl,
            entries: vec![BindGroupEntry::Buffer {
                binding: 0,
                buffer: params_buffer,
                offset: 0,
                size: std::mem::size_of::<BloomUniform>() as u64,
            }],
        });

        let bright_pipeline = FullscreenPipeline::new(
            gpu,
            BLOOM_SHADER,
            "fs_bright",
            &[sample_bgl, params_bgl],
            target_format,
            None,
            "bloom_bright_pipeline",
        );
        let downsample_pipeline = FullscreenPipeline::new(
            gpu,
            BLOOM_SHADER,
            "fs_downsample",
            &[sample_bgl, params_bgl],
            target_format,
            None,
            "bloom_downsample_pipeline",
        );
        let blur_pipeline = FullscreenPipeline::new(
            gpu,
            BLOOM_SHADER,
            "fs_blur",
            &[sample_bgl, params_bgl],
            target_format,
            None,
            "bloom_blur_pipeline",
        );
        let upsample_pipeline = FullscreenPipeline::new(
            gpu,
            BLOOM_SHADER,
            "fs_upsample",
            &[sample_bgl, params_bgl],
            target_format,
            Some(crate::gpu::BlendState::ADDITIVE),
            "bloom_upsample_pipeline",
        );
        let combine_pipeline = FullscreenPipeline::new(
            gpu,
            BLOOM_SHADER,
            "fs_combine",
            &[dual_bgl, params_bgl],
            target_format,
            None,
            "bloom_combine_pipeline",
        );

        let mut bloom = Self {
            bright_pipeline,
            downsample_pipeline,
            blur_pipeline,
            upsample_pipeline,
            combine_pipeline,
            sample_bgl,
            dual_bgl,
            params_bgl,
            params_buffer,
            params_bind_group,
            sample_bind_groups: FxHashMap::default(),
            dual_bind_groups: FxHashMap::default(),
            mip_chain: Vec::new(),
            temp_targets: Vec::new(),
            threshold: 0.8,
            intensity: 0.3,
            radius: 1.0,
        };
        bloom.resize(gpu, width, height, target_format);
        bloom
    }

    pub fn resize(
        &mut self,
        gpu: &mut impl Gpu,
        width: u32,
        height: u32,
        target_format: TextureFormat,
    ) {
        if self.mip_chain.len() != BLOOM_LEVELS {
            self.mip_chain.clear();
            self.temp_targets.clear();
            for level in 0..BLOOM_LEVELS {
                let scale = 1u32 << (level as u32 + 1);
                let level_width = (width / scale).max(1);
                let level_height = (height / scale).max(1);
                self.mip_chain.push(RenderTarget::new(
                    gpu,
                    level_width,
                    level_height,
                    target_format,
                    format!("bloom_mip_{level}"),
                ));
                self.temp_targets.push(RenderTarget::new(
                    gpu,
                    level_width,
                    level_height,
                    target_format,
                    format!("bloom_tmp_{level}"),
                ));
            }
            self.sample_bind_groups.clear();
            self.dual_bind_groups.clear();
            return;
        }

        for level in 0..BLOOM_LEVELS {
            let scale = 1u32 << (level as u32 + 1);
            let level_width = (width / scale).max(1);
            let level_height = (height / scale).max(1);
            self.mip_chain[level].resize(gpu, level_width, level_height);
            self.temp_targets[level].resize(gpu, level_width, level_height);
        }
        self.invalidate_cache(gpu);
    }

    fn update_uniform(
        &self,
        gpu: &mut impl Gpu,
        threshold: f32,
        intensity: f32,
        texel: [f32; 2],
        dir: [f32; 2],
    ) {
        let uniform = BloomUniform {
            params: [threshold, intensity, self.radius, 0.0],
            texel_dir: [texel[0], texel[1], dir[0], dir[1]],
        };
        gpu.write_buffer(self.params_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn apply(&mut self, gpu: &mut impl Gpu, input: &RenderTarget, output: &RenderTarget) {
        self.resize(gpu, input.width(), input.height(), output.format());

        self.update_uniform(
            gpu,
            self.threshold,
            self.intensity,
            [1.0 / input.width() as f32, 1.0 / input.height() as f32],
            [0.0, 0.0],
        );
        let bright_bg = self.sample_bind_group(gpu, input.image(), input.sampler());
        self.run_single_input(
            gpu,
            self.bright_pipeline.pipeline(),
            bright_bg,
            self.mip_chain[0].image(),
            Some([0.0, 0.0, 0.0, 1.0]),
        );

        for level in 1..BLOOM_LEVELS {
            let source = &self.mip_chain[level - 1];
            let source_image = source.image();
            let source_sampler = source.sampler();
            let source_width = source.width();
            let source_height = source.height();
            let target_image = self.mip_chain[level].image();
            self.update_uniform(
                gpu,
                self.threshold,
                self.intensity,
                [1.0 / source_width as f32, 1.0 / source_height as f32],
                [0.0, 0.0],
            );
            let bind_group = self.sample_bind_group(gpu, source_image, source_sampler);
            self.run_single_input(
                gpu,
                self.downsample_pipeline.pipeline(),
                bind_group,
                target_image,
                Some([0.0, 0.0, 0.0, 1.0]),
            );
        }

        for level in 0..BLOOM_LEVELS {
            let source = &self.mip_chain[level];
            let source_image = source.image();
            let source_sampler = source.sampler();
            let texel = [1.0 / source.width() as f32, 1.0 / source.height() as f32];
            let temp_image = self.temp_targets[level].image();

            self.update_uniform(gpu, self.threshold, self.intensity, texel, [1.0, 0.0]);
            let h_bg = self.sample_bind_group(gpu, source_image, source_sampler);
            self.run_single_input(
                gpu,
                self.blur_pipeline.pipeline(),
                h_bg,
                temp_image,
                Some([0.0, 0.0, 0.0, 1.0]),
            );

            self.update_uniform(gpu, self.threshold, self.intensity, texel, [0.0, 1.0]);
            let temp_sampler = self.temp_targets[level].sampler();
            let v_bg = self.sample_bind_group(gpu, temp_image, temp_sampler);
            self.run_single_input(
                gpu,
                self.blur_pipeline.pipeline(),
                v_bg,
                self.mip_chain[level].image(),
                Some([0.0, 0.0, 0.0, 1.0]),
            );
        }

        for level in (1..BLOOM_LEVELS).rev() {
            let source = &self.mip_chain[level];
            let source_image = source.image();
            let source_sampler = source.sampler();
            let source_width = source.width();
            let source_height = source.height();
            let target_image = self.mip_chain[level - 1].image();
            self.update_uniform(
                gpu,
                self.threshold,
                self.intensity,
                [1.0 / source_width as f32, 1.0 / source_height as f32],
                [0.0, 0.0],
            );
            let bind_group = self.sample_bind_group(gpu, source_image, source_sampler);
            self.run_single_input(
                gpu,
                self.upsample_pipeline.pipeline(),
                bind_group,
                target_image,
                None,
            );
        }

        self.update_uniform(gpu, self.threshold, self.intensity, [0.0, 0.0], [0.0, 0.0]);
        let combine_bg = self.dual_bind_group(
            gpu,
            input.image(),
            input.sampler(),
            self.mip_chain[0].image(),
            self.mip_chain[0].sampler(),
        );
        gpu.with_render_pass(
            &crate::gpu::RenderPassDesc {
                label: Cow::Borrowed("bloom_combine_pass"),
                color_attachments: vec![ColorAttachment {
                    target: ColorTarget::Image(output.image()),
                    clear: Some([0.0, 0.0, 0.0, 1.0]),
                }],
                depth_stencil: None,
            },
            |pass| {
                pass.set_pipeline(self.combine_pipeline.pipeline());
                pass.set_bind_group(0, combine_bg);
                pass.set_bind_group(1, self.params_bind_group);
                FullscreenPass::draw(pass);
            },
        );
    }

    fn run_single_input(
        &self,
        gpu: &mut impl Gpu,
        pipeline: crate::gpu::Pipeline,
        input_bind_group: BindGroup,
        output_image: crate::gpu::Image,
        clear: Option<[f32; 4]>,
    ) {
        gpu.with_render_pass(
            &crate::gpu::RenderPassDesc {
                label: Cow::Borrowed("bloom_single_pass"),
                color_attachments: vec![ColorAttachment {
                    target: ColorTarget::Image(output_image),
                    clear,
                }],
                depth_stencil: None,
            },
            |pass| {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, input_bind_group);
                pass.set_bind_group(1, self.params_bind_group);
                FullscreenPass::draw(pass);
            },
        );
    }

    fn sample_bind_group(
        &mut self,
        gpu: &mut impl Gpu,
        image: crate::gpu::Image,
        sampler: Sampler,
    ) -> BindGroup {
        if let Some(bind_group) = self.sample_bind_groups.get(&(image, sampler)) {
            return *bind_group;
        }

        let bind_group = gpu.create_bind_group(&BindGroupDesc {
            label: Cow::Borrowed("bloom_sample_bg"),
            layout: self.sample_bgl,
            entries: vec![
                BindGroupEntry::Texture { binding: 0, image },
                BindGroupEntry::Sampler {
                    binding: 1,
                    sampler,
                },
            ],
        });
        self.sample_bind_groups.insert((image, sampler), bind_group);
        bind_group
    }

    fn dual_bind_group(
        &mut self,
        gpu: &mut impl Gpu,
        image_a: crate::gpu::Image,
        sampler_a: Sampler,
        image_b: crate::gpu::Image,
        sampler_b: Sampler,
    ) -> BindGroup {
        let key = (image_a, sampler_a, image_b, sampler_b);
        if let Some(bind_group) = self.dual_bind_groups.get(&key) {
            return *bind_group;
        }

        let bind_group = gpu.create_bind_group(&BindGroupDesc {
            label: Cow::Borrowed("bloom_dual_bg"),
            layout: self.dual_bgl,
            entries: vec![
                BindGroupEntry::Texture {
                    binding: 0,
                    image: image_a,
                },
                BindGroupEntry::Sampler {
                    binding: 1,
                    sampler: sampler_a,
                },
                BindGroupEntry::Texture {
                    binding: 2,
                    image: image_b,
                },
                BindGroupEntry::Sampler {
                    binding: 3,
                    sampler: sampler_b,
                },
            ],
        });
        self.dual_bind_groups.insert(key, bind_group);
        bind_group
    }

    pub fn invalidate_cache(&mut self, gpu: &mut impl Gpu) {
        for bind_group in self
            .sample_bind_groups
            .drain()
            .map(|(_, bind_group)| bind_group)
        {
            gpu.destroy_bind_group(bind_group);
        }
        for bind_group in self
            .dual_bind_groups
            .drain()
            .map(|(_, bind_group)| bind_group)
        {
            gpu.destroy_bind_group(bind_group);
        }
    }

    pub fn destroy(&mut self, gpu: &mut impl Gpu) {
        self.invalidate_cache(gpu);
        for target in &self.mip_chain {
            target.destroy(gpu);
        }
        for target in &self.temp_targets {
            target.destroy(gpu);
        }
        gpu.destroy_bind_group(self.params_bind_group);
        gpu.destroy_buffer(self.params_buffer);
        gpu.destroy_bind_group_layout(self.params_bgl);
        gpu.destroy_bind_group_layout(self.dual_bgl);
        gpu.destroy_bind_group_layout(self.sample_bgl);
        self.combine_pipeline.destroy(gpu);
        self.upsample_pipeline.destroy(gpu);
        self.blur_pipeline.destroy(gpu);
        self.downsample_pipeline.destroy(gpu);
        self.bright_pipeline.destroy(gpu);
    }
}

impl PostFx for Bloom {
    fn apply_to_target<G: Gpu>(
        &mut self,
        gpu: &mut G,
        input: &RenderTarget,
        output: &RenderTarget,
    ) {
        Bloom::apply(self, gpu, input, output);
    }
}
