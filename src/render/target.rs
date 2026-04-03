//! Off-screen render target helpers.

use std::borrow::Cow;

use crate::gpu::{
    AddressMode, FilterMode, Gpu, Image, ImageDesc, ImageUsage, Sampler, SamplerDesc, TextureFormat,
};

/// A persistent off-screen render target.
#[derive(Debug, Clone)]
pub struct RenderTarget {
    image: Image,
    sampler: Sampler,
    width: u32,
    height: u32,
    format: TextureFormat,
    label: Cow<'static, str>,
}

impl RenderTarget {
    /// Create a new off-screen target.
    pub fn new(
        gpu: &mut impl Gpu,
        width: u32,
        height: u32,
        format: TextureFormat,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        let label = label.into();
        let image = gpu.create_image(&ImageDesc {
            label: label.clone(),
            width,
            height,
            depth: 1,
            format,
            usage: ImageUsage::RENDER_TARGET | ImageUsage::SAMPLED,
            mip_levels: 1,
        });
        let sampler = gpu.create_sampler(&SamplerDesc {
            label: Cow::Owned(format!("{}_sampler", label)),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
        });

        Self {
            image,
            sampler,
            width,
            height,
            format,
            label,
        }
    }

    /// Resize the target if the dimensions changed.
    pub fn resize(&mut self, gpu: &mut impl Gpu, width: u32, height: u32) {
        if self.width == width && self.height == height {
            return;
        }

        gpu.destroy_image(self.image);
        self.image = gpu.create_image(&ImageDesc {
            label: self.label.clone(),
            width,
            height,
            depth: 1,
            format: self.format,
            usage: ImageUsage::RENDER_TARGET | ImageUsage::SAMPLED,
            mip_levels: 1,
        });
        self.width = width;
        self.height = height;
    }

    #[inline]
    pub fn image(&self) -> Image {
        self.image
    }

    #[inline]
    pub fn sampler(&self) -> Sampler {
        self.sampler
    }

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[inline]
    pub fn format(&self) -> TextureFormat {
        self.format
    }

    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Destroy the underlying GPU resources.
    ///
    /// After calling this, the `RenderTarget` should not be used for rendering.
    pub fn destroy(&self, gpu: &mut impl Gpu) {
        gpu.destroy_image(self.image);
        gpu.destroy_sampler(self.sampler);
    }
}
