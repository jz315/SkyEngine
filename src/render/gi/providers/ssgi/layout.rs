use super::constants::{
    SSGI_ATLAS_LAYERS, SSGI_COLOR_FORMAT, SSGI_COMPUTE_TEXTURE_USAGE, SSGI_DEPTH_FORMAT,
    SSGI_INTERNAL_ALIGNMENT, SSGI_MIP_COUNT, SSGI_NORMAL_FORMAT, SSGI_TEXTURE_ATLAS_COLOR,
    SSGI_TEXTURE_ATLAS_DEPTH, SSGI_TEXTURE_DEPTH_MIPS, SSGI_TEXTURE_DIFFUSE_MIPS,
    SSGI_TEXTURE_FILTERED_DIFFUSE_MIPS, SSGI_TEXTURE_NORMAL_MIPS,
};
use crate::render::pipeline::TextureSpec;

/// Small runtime resource tracker for the SSGI pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SsgiResources {
    target_size: [u32; 2],
    aligned_size: [u32; 2],
    atlas_size: [u32; 2],
    mip_chain: [SsgiMipLevel; SSGI_MIP_COUNT],
}

/// Wicked-style SSGI mip dimensions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SsgiMipLevel {
    pub scale: u32,
    pub atlas_size: [u32; 2],
    pub regular_size: [u32; 2],
}

/// Texture contract for the compute-backed SSGI path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SsgiComputeTextureLayout {
    pub atlas_size: [u32; 2],
    pub regular_mip_size: [u32; 2],
    pub mip_level_count: u32,
    pub atlas_layer_count: u32,
    pub atlas_color_format: wgpu::TextureFormat,
    pub atlas_depth_format: wgpu::TextureFormat,
    pub depth_mip_format: wgpu::TextureFormat,
    pub normal_mip_format: wgpu::TextureFormat,
    pub diffuse_mip_format: wgpu::TextureFormat,
    pub usage: wgpu::TextureUsages,
}

#[derive(Clone, Debug)]
pub(crate) struct SsgiComputeTextureSpecs {
    pub(crate) atlas_color: TextureSpec,
    pub(crate) atlas_depth: TextureSpec,
    pub(crate) depth_mips: TextureSpec,
    pub(crate) normal_mips: TextureSpec,
    pub(crate) diffuse_mips: TextureSpec,
    pub(crate) filtered_diffuse_mips: TextureSpec,
}

impl SsgiResources {
    #[inline]
    pub const fn target_size(&self) -> [u32; 2] {
        self.target_size
    }

    #[inline]
    pub const fn aligned_size(&self) -> [u32; 2] {
        self.aligned_size
    }

    #[inline]
    pub const fn atlas_size(&self) -> [u32; 2] {
        self.atlas_size
    }

    #[inline]
    pub const fn atlas_layers(&self) -> u32 {
        SSGI_ATLAS_LAYERS
    }

    #[inline]
    pub const fn mip_level(&self, index: usize) -> Option<SsgiMipLevel> {
        if index < SSGI_MIP_COUNT {
            Some(self.mip_chain[index])
        } else {
            None
        }
    }

    #[inline]
    pub fn compute_texture_layout(&self) -> SsgiComputeTextureLayout {
        let regular_mip_size = self
            .mip_chain
            .first()
            .map(|mip| [mip.regular_size[0].max(1), mip.regular_size[1].max(1)])
            .unwrap_or([1, 1]);
        SsgiComputeTextureLayout {
            atlas_size: [self.atlas_size[0].max(1), self.atlas_size[1].max(1)],
            regular_mip_size,
            mip_level_count: SSGI_MIP_COUNT as u32,
            atlas_layer_count: SSGI_ATLAS_LAYERS,
            atlas_color_format: SSGI_COLOR_FORMAT,
            atlas_depth_format: SSGI_DEPTH_FORMAT,
            depth_mip_format: SSGI_DEPTH_FORMAT,
            normal_mip_format: SSGI_NORMAL_FORMAT,
            diffuse_mip_format: SSGI_COLOR_FORMAT,
            usage: SSGI_COMPUTE_TEXTURE_USAGE,
        }
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) {
        self.target_size = [width.max(1), height.max(1)];
        self.aligned_size = [
            align_to(self.target_size[0], SSGI_INTERNAL_ALIGNMENT),
            align_to(self.target_size[1], SSGI_INTERNAL_ALIGNMENT),
        ];
        self.atlas_size = [
            self.aligned_size[0].div_ceil(8).max(1),
            self.aligned_size[1].div_ceil(8).max(1),
        ];
        let regular_base = [
            self.aligned_size[0].div_ceil(2).max(1),
            self.aligned_size[1].div_ceil(2).max(1),
        ];
        let mut levels = [SsgiMipLevel::default(); SSGI_MIP_COUNT];
        for (index, level) in levels.iter_mut().enumerate() {
            let scale = 1u32 << index;
            *level = SsgiMipLevel {
                scale: scale * 2,
                atlas_size: [
                    (self.atlas_size[0] / scale).max(1),
                    (self.atlas_size[1] / scale).max(1),
                ],
                regular_size: [
                    (regular_base[0] / scale).max(1),
                    (regular_base[1] / scale).max(1),
                ],
            };
        }
        self.mip_chain = levels;
    }
}

impl SsgiComputeTextureSpecs {
    pub(crate) fn from_layout(layout: SsgiComputeTextureLayout) -> Self {
        let usage = layout.usage;
        Self {
            atlas_color: TextureSpec::new(SSGI_TEXTURE_ATLAS_COLOR, layout.atlas_color_format)
                .exact(layout.atlas_size[0], layout.atlas_size[1])
                .usage(usage)
                .mips(layout.mip_level_count)
                .array_layers(layout.atlas_layer_count),
            atlas_depth: TextureSpec::new(SSGI_TEXTURE_ATLAS_DEPTH, layout.atlas_depth_format)
                .exact(layout.atlas_size[0], layout.atlas_size[1])
                .usage(usage)
                .mips(layout.mip_level_count)
                .array_layers(layout.atlas_layer_count),
            depth_mips: TextureSpec::new(SSGI_TEXTURE_DEPTH_MIPS, layout.depth_mip_format)
                .exact(layout.regular_mip_size[0], layout.regular_mip_size[1])
                .usage(usage)
                .mips(layout.mip_level_count),
            normal_mips: TextureSpec::new(SSGI_TEXTURE_NORMAL_MIPS, layout.normal_mip_format)
                .exact(layout.regular_mip_size[0], layout.regular_mip_size[1])
                .usage(usage)
                .mips(layout.mip_level_count),
            diffuse_mips: TextureSpec::new(SSGI_TEXTURE_DIFFUSE_MIPS, layout.diffuse_mip_format)
                .exact(layout.regular_mip_size[0], layout.regular_mip_size[1])
                .usage(usage)
                .mips(layout.mip_level_count),
            filtered_diffuse_mips: TextureSpec::new(
                SSGI_TEXTURE_FILTERED_DIFFUSE_MIPS,
                layout.diffuse_mip_format,
            )
            .exact(layout.regular_mip_size[0], layout.regular_mip_size[1])
            .usage(usage)
            .mips(layout.mip_level_count),
        }
    }
}

#[inline]
pub(crate) fn ssgi_compute_texture_specs(resources: SsgiResources) -> SsgiComputeTextureSpecs {
    SsgiComputeTextureSpecs::from_layout(resources.compute_texture_layout())
}

#[inline]
fn align_to(value: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return value.max(1);
    }
    value.max(1).div_ceil(alignment) * alignment
}
