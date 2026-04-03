//! GPU texture loading and management.

use std::borrow::Cow;

use crate::gpu::{
    AddressMode, FilterMode, Gpu, Image, ImageCopyLayout, ImageDesc, ImageUsage, Sampler,
    SamplerDesc, TextureFormat,
};

/// A sampled texture handle used by render passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Texture {
    image: Image,
    sampler: Sampler,
    width: u32,
    height: u32,
    format: TextureFormat,
}

impl Texture {
    /// Create a texture from raw RGBA8 pixel data.
    ///
    /// `data` must be exactly `width * height * 4` bytes (RGBA, 1 byte each).
    pub fn from_rgba8(gpu: &mut impl Gpu, width: u32, height: u32, data: &[u8]) -> Self {
        Self::from_rgba8_with_format(
            gpu,
            width,
            height,
            data,
            TextureFormat::Rgba8UnormSrgb,
            Cow::Borrowed("sprite_texture"),
        )
    }

    /// Create a texture from raw RGBA8 pixel data with a custom label.
    pub fn from_rgba8_with_label(
        gpu: &mut impl Gpu,
        width: u32,
        height: u32,
        data: &[u8],
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::from_rgba8_with_format(
            gpu,
            width,
            height,
            data,
            TextureFormat::Rgba8UnormSrgb,
            label,
        )
    }

    pub fn from_rgba8_with_format(
        gpu: &mut impl Gpu,
        width: u32,
        height: u32,
        data: &[u8],
        format: TextureFormat,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        assert_eq!(
            data.len(),
            (width * height * 4) as usize,
            "RGBA8 data length mismatch"
        );

        let label = label.into();
        let image = gpu.create_image(&ImageDesc {
            label: label.clone(),
            width,
            height,
            depth: 1,
            format,
            usage: ImageUsage::SAMPLED | ImageUsage::COPY_DST,
            mip_levels: 1,
        });
        let sampler = gpu.create_sampler(&SamplerDesc {
            label,
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Nearest,
        });

        gpu.write_image(
            image,
            data,
            &ImageCopyLayout {
                offset: 0,
                bytes_per_row: 4 * width,
                rows_per_image: height,
            },
        );

        Self {
            image,
            sampler,
            width,
            height,
            format,
        }
    }

    /// Load a texture from a PNG file.
    #[cfg(feature = "asset")]
    pub fn from_png(gpu: &mut impl Gpu, path: &std::path::Path) -> Self {
        let img = image::open(path)
            .unwrap_or_else(|e| panic!("Failed to load texture {:?}: {}", path, e))
            .to_rgba8();
        let (w, h) = img.dimensions();
        Self::from_rgba8_with_label(gpu, w, h, &img, path.to_string_lossy().to_string())
    }

    /// Raw sampled image handle.
    #[inline]
    pub fn image(&self) -> Image {
        self.image
    }

    /// Raw sampler handle.
    #[inline]
    pub fn sampler(&self) -> Sampler {
        self.sampler
    }

    /// Texture width in pixels.
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Texture height in pixels.
    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Texture pixel format.
    #[inline]
    pub fn format(&self) -> TextureFormat {
        self.format
    }

    /// Generate a 1×1 white pixel texture (used as default/fallback).
    pub fn white_pixel(gpu: &mut impl Gpu) -> Self {
        Self::from_rgba8(gpu, 1, 1, &[255, 255, 255, 255])
    }

    /// Destroy the underlying GPU resources.
    ///
    /// After calling this, the `Texture` should not be used for rendering.
    pub fn destroy(&self, gpu: &mut impl Gpu) {
        gpu.destroy_image(self.image);
        gpu.destroy_sampler(self.sampler);
    }

    /// Generate a procedural checkerboard texture for testing.
    pub fn checkerboard(
        gpu: &mut impl Gpu,
        size: u32,
        tile_size: u32,
        color_a: [u8; 4],
        color_b: [u8; 4],
    ) -> Self {
        let mut data = vec![0u8; (size * size * 4) as usize];
        for y in 0..size {
            for x in 0..size {
                let is_a = ((x / tile_size) + (y / tile_size)) % 2 == 0;
                let color = if is_a { color_a } else { color_b };
                let i = ((y * size + x) * 4) as usize;
                data[i..i + 4].copy_from_slice(&color);
            }
        }
        Self::from_rgba8(gpu, size, size, &data)
    }

    /// Generate a procedural circle/dot texture for particles.
    pub fn circle(gpu: &mut impl Gpu, size: u32) -> Self {
        let mut data = vec![0u8; (size * size * 4) as usize];
        let center = size as f32 * 0.5;
        let radius = center - 1.0;
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - center;
                let dy = y as f32 + 0.5 - center;
                let dist = (dx * dx + dy * dy).sqrt();
                let alpha = ((radius - dist).max(0.0).min(1.0) * 255.0) as u8;
                let i = ((y * size + x) * 4) as usize;
                data[i] = 255;
                data[i + 1] = 255;
                data[i + 2] = 255;
                data[i + 3] = alpha;
            }
        }
        Self::from_rgba8(gpu, size, size, &data)
    }

    /// Generate a flat +Z normal map used as a default lighting input.
    pub fn flat_normal(gpu: &mut impl Gpu) -> Self {
        Self::from_rgba8_with_format(
            gpu,
            1,
            1,
            &[128, 128, 255, 255],
            TextureFormat::Rgba8Unorm,
            Cow::Borrowed("flat_normal"),
        )
    }

    /// Generate a spherical normal map inside a circular sprite.
    pub fn circle_normal(gpu: &mut impl Gpu, size: u32) -> Self {
        let mut data = vec![0u8; (size * size * 4) as usize];
        let center = size as f32 * 0.5;
        let radius = center - 1.0;

        for y in 0..size {
            for x in 0..size {
                let dx = (x as f32 + 0.5 - center) / radius.max(1.0);
                let dy = (y as f32 + 0.5 - center) / radius.max(1.0);
                let len_sq = dx * dx + dy * dy;
                let i = ((y * size + x) * 4) as usize;

                if len_sq > 1.0 {
                    data[i] = 128;
                    data[i + 1] = 128;
                    data[i + 2] = 255;
                    data[i + 3] = 255;
                    continue;
                }

                let dz = (1.0 - len_sq).sqrt();
                let nx = ((dx * 0.5 + 0.5) * 255.0) as u8;
                let ny = (((-dy) * 0.5 + 0.5) * 255.0) as u8;
                let nz = ((dz * 0.5 + 0.5) * 255.0) as u8;
                data[i] = nx;
                data[i + 1] = ny;
                data[i + 2] = nz;
                data[i + 3] = 255;
            }
        }

        Self::from_rgba8_with_format(
            gpu,
            size,
            size,
            &data,
            TextureFormat::Rgba8Unorm,
            Cow::Borrowed("circle_normal"),
        )
    }
}
