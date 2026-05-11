use crate::render::component::MAX_DIRECTIONAL_SHADOW_CASCADES;
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ShadowUniform {
    pub(crate) light_view_proj: [[f32; 16]; MAX_DIRECTIONAL_SHADOW_CASCADES],
    pub(crate) light_direction: [f32; 4],
    pub(crate) cascade_splits: [f32; MAX_DIRECTIONAL_SHADOW_CASCADES],
    // x compare bias, y world units per shadow texel, z Wicked-style filter radius,
    // w light-space depth range in world units (0 disables the cascade).
    pub(crate) cascade_params: [[f32; 4]; MAX_DIRECTIONAL_SHADOW_CASCADES],
    pub(crate) shadow_atlas_mul_add: [f32; 4],
    pub(crate) shadow_atlas_resolution_rcp: [f32; 4], // xy atlas reciprocal, z guard band texels, w sampling mode
    // x cascade count, y blend, z material debug mode, w enabled plus temporal rotation seed
    pub(crate) shadow_params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ShadowPassUniform {
    pub(crate) raster_view_proj: [f32; 16],
    pub(crate) depth_view_proj: [f32; 16],
}

pub(crate) struct ShadowSceneBindingLayout {
    bind_group_layout: wgpu::BindGroupLayout,
}

impl ShadowSceneBindingLayout {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        Self {
            bind_group_layout: create_shadow_scene_bind_group_layout(device),
        }
    }

    #[inline]
    pub(crate) fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }
}

pub(crate) struct ShadowPassBindingLayout {
    bind_group_layout: wgpu::BindGroupLayout,
}

impl ShadowPassBindingLayout {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        Self {
            bind_group_layout: create_shadow_pass_bind_group_layout(device),
        }
    }

    #[inline]
    pub(crate) fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }
}

pub(crate) fn create_shadow_scene_bind_group_layout(
    device: &wgpu::Device,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow_scene_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<ShadowUniform>() as u64)
                            .expect("ShadowUniform has non-zero size"),
                    ),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

pub(crate) fn create_shadow_compare_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("shadow_compare_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        compare: Some(wgpu::CompareFunction::LessEqual),
        ..Default::default()
    })
}

fn create_shadow_pass_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shadow_pass_bgl"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: Some(
                    std::num::NonZeroU64::new(std::mem::size_of::<ShadowPassUniform>() as u64)
                        .expect("ShadowPassUniform has non-zero size"),
                ),
            },
            count: None,
        }],
    })
}

pub(crate) fn create_shadow_scene_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    light_buffer: &wgpu::Buffer,
    light_meta_buffer: &wgpu::Buffer,
    uniform_buffer: &wgpu::Buffer,
    shadow_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    transparent_shadow_view: &wgpu::TextureView,
    transparent_shadow_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shadow_scene_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: light_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: light_meta_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(transparent_shadow_view),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(transparent_shadow_sampler),
            },
        ],
    })
}

pub(crate) fn create_shadow_pass_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("shadow_pass_bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    #[test]
    fn shadow_uniform_layout_matches_wgsl_contract() {
        assert_eq!(size_of::<ShadowUniform>(), 400);
        assert_eq!(align_of::<ShadowUniform>(), align_of::<f32>());
        assert_eq!(offset_of!(ShadowUniform, light_view_proj), 0);
        assert_eq!(offset_of!(ShadowUniform, light_direction), 256);
        assert_eq!(offset_of!(ShadowUniform, cascade_splits), 272);
        assert_eq!(offset_of!(ShadowUniform, cascade_params), 288);
        assert_eq!(offset_of!(ShadowUniform, shadow_atlas_mul_add), 352);
        assert_eq!(offset_of!(ShadowUniform, shadow_atlas_resolution_rcp), 368);
        assert_eq!(offset_of!(ShadowUniform, shadow_params), 384);
    }

    #[test]
    fn shadow_pass_uniform_layout_matches_wgsl_contract() {
        assert_eq!(size_of::<ShadowPassUniform>(), 128);
        assert_eq!(align_of::<ShadowPassUniform>(), align_of::<f32>());
        assert_eq!(offset_of!(ShadowPassUniform, raster_view_proj), 0);
        assert_eq!(offset_of!(ShadowPassUniform, depth_view_proj), 64);
    }

    #[test]
    fn shadow_uniform_params_match_current_cascade_contract() {
        let uniform = ShadowUniform {
            light_view_proj: [[0.0; 16]; MAX_DIRECTIONAL_SHADOW_CASCADES],
            light_direction: [0.0, -1.0, 0.0, 0.02],
            cascade_splits: [8.0, 32.0, 128.0, 512.0],
            cascade_params: [
                [0.003, 0.005, 0.05, 64.0],
                [0.004, 0.012, 0.06, 128.0],
                [0.005, 0.026, 0.07, 256.0],
                [0.006, 0.052, 0.08, 512.0],
            ],
            shadow_atlas_mul_add: [0.25, 1.0, 0.0, 0.0],
            shadow_atlas_resolution_rcp: [1.0 / 8192.0, 1.0 / 2048.0, 1.0, 2.0],
            shadow_params: [4.0, 0.1, 1.0, 1.0],
        };

        assert_eq!(uniform.light_direction[3], 0.02);
        assert_eq!(uniform.cascade_splits[2], 128.0);
        assert_eq!(uniform.cascade_params[0][0], 0.003);
        assert_eq!(uniform.cascade_params[1][1], 0.012);
        assert_eq!(uniform.cascade_params[3][2], 0.08);
        assert_eq!(uniform.cascade_params[3][3], 512.0);
        assert_eq!(uniform.shadow_atlas_mul_add, [0.25, 1.0, 0.0, 0.0]);
        assert_eq!(
            uniform.shadow_atlas_resolution_rcp,
            [1.0 / 8192.0, 1.0 / 2048.0, 1.0, 2.0]
        );
        assert_eq!(uniform.shadow_params[0], 4.0);
        assert_eq!(uniform.shadow_params[1], 0.1);
        assert_eq!(uniform.shadow_params[2], 1.0);
        assert_eq!(uniform.shadow_params[3], 1.0);
    }
}
