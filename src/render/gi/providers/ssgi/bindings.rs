use super::constants::{SSGI_COLOR_FORMAT, SSGI_DEPTH_FORMAT, SSGI_NORMAL_FORMAT};
use super::contract::SsgiBindingRole;
use crate::gpu::GpuContext;
use crate::render::gpu::RenderTarget;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SsgiBindingLocation {
    pub(crate) group: u32,
    pub(crate) binding: u32,
}

#[derive(Default)]
pub(crate) struct SsgiBindGroupLayouts {
    final_bgl: Option<wgpu::BindGroupLayout>,
    composite_bgl: Option<wgpu::BindGroupLayout>,
    uniform_bgl: Option<wgpu::BindGroupLayout>,
    compute_scene_bgl: Option<wgpu::BindGroupLayout>,
    compute_deinterleave_output_bgl: Option<wgpu::BindGroupLayout>,
    compute_diffuse_input_bgl: Option<wgpu::BindGroupLayout>,
    compute_diffuse_output_bgl: Option<wgpu::BindGroupLayout>,
    compute_upsample_input_bgl: Option<wgpu::BindGroupLayout>,
    compute_upsample_output_bgl: Option<wgpu::BindGroupLayout>,
}

impl SsgiBindGroupLayouts {
    pub(crate) fn ensure_final(&mut self, gpu: &GpuContext) {
        self.ensure_uniform(gpu);
        if self.final_bgl.is_none() {
            self.final_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_final_bgl"),
                    entries: &[
                        texture_entry(0, wgpu::TextureSampleType::Float { filterable: false }),
                        texture_entry(1, wgpu::TextureSampleType::Float { filterable: false }),
                        texture_entry(2, wgpu::TextureSampleType::Float { filterable: false }),
                        texture_entry(3, wgpu::TextureSampleType::Depth),
                        texture_entry(4, wgpu::TextureSampleType::Float { filterable: false }),
                        texture_entry(5, wgpu::TextureSampleType::Float { filterable: false }),
                    ],
                },
            ));
        }
        if self.composite_bgl.is_none() {
            self.composite_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_composite_bgl"),
                    entries: &[
                        texture_entry(0, wgpu::TextureSampleType::Float { filterable: false }),
                        texture_entry(1, wgpu::TextureSampleType::Float { filterable: false }),
                    ],
                },
            ));
        }
    }

    pub(crate) fn ensure_compute(&mut self, gpu: &GpuContext) {
        self.ensure_uniform(gpu);
        if self.compute_scene_bgl.is_none() {
            self.compute_scene_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_compute_scene_bgl"),
                    entries: &[
                        compute_texture_entry(
                            0,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            1,
                            wgpu::TextureSampleType::Depth,
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            2,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            3,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                    ],
                },
            ));
        }

        if self.compute_deinterleave_output_bgl.is_none() {
            self.compute_deinterleave_output_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_compute_deinterleave_output_bgl"),
                    entries: &[
                        compute_storage_texture_entry(
                            0,
                            SSGI_DEPTH_FORMAT,
                            wgpu::TextureViewDimension::D2Array,
                        ),
                        compute_storage_texture_entry(
                            1,
                            SSGI_COLOR_FORMAT,
                            wgpu::TextureViewDimension::D2Array,
                        ),
                        compute_storage_texture_entry(
                            2,
                            SSGI_DEPTH_FORMAT,
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_storage_texture_entry(
                            3,
                            SSGI_NORMAL_FORMAT,
                            wgpu::TextureViewDimension::D2,
                        ),
                    ],
                },
            ));
        }

        if self.compute_diffuse_input_bgl.is_none() {
            self.compute_diffuse_input_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_compute_diffuse_input_bgl"),
                    entries: &[
                        compute_texture_entry(
                            0,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2Array,
                        ),
                        compute_texture_entry(
                            1,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2Array,
                        ),
                        compute_texture_entry(
                            2,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                    ],
                },
            ));
        }

        if self.compute_diffuse_output_bgl.is_none() {
            self.compute_diffuse_output_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_compute_diffuse_output_bgl"),
                    entries: &[compute_storage_texture_entry(
                        0,
                        SSGI_COLOR_FORMAT,
                        wgpu::TextureViewDimension::D2,
                    )],
                },
            ));
        }

        if self.compute_upsample_input_bgl.is_none() {
            self.compute_upsample_input_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_compute_upsample_input_bgl"),
                    entries: &[
                        compute_texture_entry(
                            0,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            1,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            2,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            3,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            4,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                        compute_texture_entry(
                            5,
                            wgpu::TextureSampleType::Float { filterable: false },
                            wgpu::TextureViewDimension::D2,
                        ),
                    ],
                },
            ));
        }

        if self.compute_upsample_output_bgl.is_none() {
            self.compute_upsample_output_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_compute_upsample_output_bgl"),
                    entries: &[compute_storage_texture_entry(
                        0,
                        SSGI_COLOR_FORMAT,
                        wgpu::TextureViewDimension::D2,
                    )],
                },
            ));
        }
    }

    pub(crate) fn uniform(&self) -> &wgpu::BindGroupLayout {
        self.uniform_bgl
            .as_ref()
            .expect("SSGI uniform bind group layout should exist")
    }

    pub(crate) fn final_textures(&self) -> &wgpu::BindGroupLayout {
        self.final_bgl
            .as_ref()
            .expect("SSGI final bind group layout should exist")
    }

    pub(crate) fn composite_textures(&self) -> &wgpu::BindGroupLayout {
        self.composite_bgl
            .as_ref()
            .expect("SSGI composite bind group layout should exist")
    }

    pub(crate) fn compute_scene(&self) -> &wgpu::BindGroupLayout {
        self.compute_scene_bgl
            .as_ref()
            .expect("SSGI compute scene bind group layout should exist")
    }

    pub(crate) fn deinterleave_output(&self) -> &wgpu::BindGroupLayout {
        self.compute_deinterleave_output_bgl
            .as_ref()
            .expect("SSGI deinterleave output bind group layout should exist")
    }

    pub(crate) fn diffuse_input(&self) -> &wgpu::BindGroupLayout {
        self.compute_diffuse_input_bgl
            .as_ref()
            .expect("SSGI diffuse input bind group layout should exist")
    }

    pub(crate) fn diffuse_output(&self) -> &wgpu::BindGroupLayout {
        self.compute_diffuse_output_bgl
            .as_ref()
            .expect("SSGI diffuse output bind group layout should exist")
    }

    pub(crate) fn upsample_input(&self) -> &wgpu::BindGroupLayout {
        self.compute_upsample_input_bgl
            .as_ref()
            .expect("SSGI upsample input bind group layout should exist")
    }

    pub(crate) fn upsample_output(&self) -> &wgpu::BindGroupLayout {
        self.compute_upsample_output_bgl
            .as_ref()
            .expect("SSGI upsample output bind group layout should exist")
    }

    fn ensure_uniform(&mut self, gpu: &GpuContext) {
        if self.uniform_bgl.is_none() {
            self.uniform_bgl = Some(gpu.device().create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("ssgi_uniform_bgl"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                },
            ));
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn binding_location(role: SsgiBindingRole) -> SsgiBindingLocation {
    match role {
        SsgiBindingRole::SceneColor
        | SsgiBindingRole::DiffuseAtlasDepthInput
        | SsgiBindingRole::UpsampleLowDepthInput
        | SsgiBindingRole::FinalLowDepth => SsgiBindingLocation {
            group: 0,
            binding: 0,
        },
        SsgiBindingRole::SceneDepth
        | SsgiBindingRole::DiffuseAtlasColorInput
        | SsgiBindingRole::UpsampleLowNormalInput
        | SsgiBindingRole::FinalLowNormal => SsgiBindingLocation {
            group: 0,
            binding: 1,
        },
        SsgiBindingRole::SceneNormal
        | SsgiBindingRole::DiffuseNormalInput
        | SsgiBindingRole::UpsampleLowDiffuseInput
        | SsgiBindingRole::FinalLowDiffuse => SsgiBindingLocation {
            group: 0,
            binding: 2,
        },
        SsgiBindingRole::SceneVelocity => SsgiBindingLocation {
            group: 0,
            binding: 3,
        },
        SsgiBindingRole::UpsampleHighDepthInput | SsgiBindingRole::FinalSceneDepth => {
            SsgiBindingLocation {
                group: 0,
                binding: 3,
            }
        }
        SsgiBindingRole::UpsampleHighNormalInput | SsgiBindingRole::FinalSceneNormal => {
            SsgiBindingLocation {
                group: 0,
                binding: 4,
            }
        }
        SsgiBindingRole::UpsampleHighDiffuseInput | SsgiBindingRole::FinalSceneColor => {
            SsgiBindingLocation {
                group: 0,
                binding: 5,
            }
        }
        SsgiBindingRole::CompositeIndirectDiffuse => SsgiBindingLocation {
            group: 0,
            binding: 0,
        },
        SsgiBindingRole::CompositeSceneColor => SsgiBindingLocation {
            group: 0,
            binding: 1,
        },
        SsgiBindingRole::Uniform => SsgiBindingLocation {
            group: 1,
            binding: 0,
        },
        SsgiBindingRole::DeinterleaveAtlasDepthOutput
        | SsgiBindingRole::DiffuseOutput
        | SsgiBindingRole::UpsampleOutput => SsgiBindingLocation {
            group: 2,
            binding: 0,
        },
        SsgiBindingRole::DeinterleaveAtlasColorOutput => SsgiBindingLocation {
            group: 2,
            binding: 1,
        },
        SsgiBindingRole::DeinterleaveDepthOutput => SsgiBindingLocation {
            group: 2,
            binding: 2,
        },
        SsgiBindingRole::DeinterleaveNormalOutput => SsgiBindingLocation {
            group: 2,
            binding: 3,
        },
    }
}

pub(crate) fn create_final_bind_group(
    gpu: &GpuContext,
    layouts: &SsgiBindGroupLayouts,
    low_depth: &wgpu::TextureView,
    low_normal: &wgpu::TextureView,
    low_diffuse: &wgpu::TextureView,
    scene_depth: &wgpu::TextureView,
    scene_normal: &wgpu::TextureView,
    scene_color: &wgpu::TextureView,
) -> wgpu::BindGroup {
    gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ssgi_final_bg"),
        layout: layouts.final_textures(),
        entries: &[
            texture_view_binding(0, low_depth),
            texture_view_binding(1, low_normal),
            texture_view_binding(2, low_diffuse),
            texture_view_binding(3, scene_depth),
            texture_view_binding(4, scene_normal),
            texture_view_binding(5, scene_color),
        ],
    })
}

pub(crate) fn create_composite_bind_group(
    gpu: &GpuContext,
    layouts: &SsgiBindGroupLayouts,
    indirect_diffuse: &wgpu::TextureView,
    scene_color: &wgpu::TextureView,
) -> wgpu::BindGroup {
    gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ssgi_composite_bg"),
        layout: layouts.composite_textures(),
        entries: &[
            texture_view_binding(0, indirect_diffuse),
            texture_view_binding(1, scene_color),
        ],
    })
}

pub(crate) fn create_uniform_bind_group(
    gpu: &GpuContext,
    layouts: &SsgiBindGroupLayouts,
    uniform_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ssgi_uniform_bg"),
        layout: layouts.uniform(),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    })
}

pub(crate) fn create_compute_scene_bind_group(
    gpu: &GpuContext,
    layouts: &SsgiBindGroupLayouts,
    current: &RenderTarget,
    depth: &RenderTarget,
    normal: &RenderTarget,
    velocity: &RenderTarget,
) -> wgpu::BindGroup {
    gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ssgi_compute_scene_bg"),
        layout: layouts.compute_scene(),
        entries: &[
            texture_binding(0, current),
            texture_binding(1, depth),
            texture_binding(2, normal),
            texture_binding(3, velocity),
        ],
    })
}

pub(crate) fn create_compute_deinterleave_output_bind_group(
    gpu: &GpuContext,
    layouts: &SsgiBindGroupLayouts,
    atlas_depth: &wgpu::TextureView,
    atlas_color: &wgpu::TextureView,
    regular_depth: &wgpu::TextureView,
    regular_normal: &wgpu::TextureView,
) -> wgpu::BindGroup {
    gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ssgi_compute_deinterleave_output_bg"),
        layout: layouts.deinterleave_output(),
        entries: &[
            texture_view_binding(0, atlas_depth),
            texture_view_binding(1, atlas_color),
            texture_view_binding(2, regular_depth),
            texture_view_binding(3, regular_normal),
        ],
    })
}

pub(crate) fn create_compute_diffuse_input_bind_group(
    gpu: &GpuContext,
    layouts: &SsgiBindGroupLayouts,
    atlas_depth: &wgpu::TextureView,
    atlas_color: &wgpu::TextureView,
    normal: &wgpu::TextureView,
) -> wgpu::BindGroup {
    gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ssgi_compute_diffuse_input_bg"),
        layout: layouts.diffuse_input(),
        entries: &[
            texture_view_binding(0, atlas_depth),
            texture_view_binding(1, atlas_color),
            texture_view_binding(2, normal),
        ],
    })
}

pub(crate) fn create_compute_diffuse_output_bind_group(
    gpu: &GpuContext,
    layouts: &SsgiBindGroupLayouts,
    output: &wgpu::TextureView,
) -> wgpu::BindGroup {
    gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ssgi_compute_diffuse_output_bg"),
        layout: layouts.diffuse_output(),
        entries: &[texture_view_binding(0, output)],
    })
}

pub(crate) fn create_compute_upsample_input_bind_group(
    gpu: &GpuContext,
    layouts: &SsgiBindGroupLayouts,
    depth_low: &wgpu::TextureView,
    normal_low: &wgpu::TextureView,
    diffuse_low: &wgpu::TextureView,
    depth_high: &wgpu::TextureView,
    normal_high: &wgpu::TextureView,
    diffuse_high: &wgpu::TextureView,
) -> wgpu::BindGroup {
    gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ssgi_compute_upsample_input_bg"),
        layout: layouts.upsample_input(),
        entries: &[
            texture_view_binding(0, depth_low),
            texture_view_binding(1, normal_low),
            texture_view_binding(2, diffuse_low),
            texture_view_binding(3, depth_high),
            texture_view_binding(4, normal_high),
            texture_view_binding(5, diffuse_high),
        ],
    })
}

pub(crate) fn create_compute_upsample_output_bind_group(
    gpu: &GpuContext,
    layouts: &SsgiBindGroupLayouts,
    output: &wgpu::TextureView,
) -> wgpu::BindGroup {
    gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ssgi_compute_upsample_output_bg"),
        layout: layouts.upsample_output(),
        entries: &[texture_view_binding(0, output)],
    })
}

pub(crate) fn color_attachment(target: &RenderTarget) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view: target.view(),
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
        },
    }
}

fn texture_entry(binding: u32, sample_type: wgpu::TextureSampleType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn texture_binding<'a>(binding: u32, target: &'a RenderTarget) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(target.view()),
    }
}

fn texture_view_binding<'a>(binding: u32, view: &'a wgpu::TextureView) -> wgpu::BindGroupEntry<'a> {
    wgpu::BindGroupEntry {
        binding,
        resource: wgpu::BindingResource::TextureView(view),
    }
}

fn compute_texture_entry(
    binding: u32,
    sample_type: wgpu::TextureSampleType,
    view_dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension,
            multisampled: false,
        },
        count: None,
    }
}

fn compute_storage_texture_entry(
    binding: u32,
    format: wgpu::TextureFormat,
    view_dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format,
            view_dimension,
        },
        count: None,
    }
}
