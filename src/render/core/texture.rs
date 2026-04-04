//! GPU texture loading and management.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::gpu::GpuContext;

/// Errors returned by fallible texture creation APIs.
#[derive(Debug)]
pub enum TextureError {
    InvalidRgba8Length { expected: usize, actual: usize },
    ImageLoad { path: PathBuf, message: String },
}

impl std::fmt::Display for TextureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRgba8Length { expected, actual } => {
                write!(
                    f,
                    "RGBA8 data length mismatch: expected {expected} bytes, got {actual}"
                )
            }
            Self::ImageLoad { path, message } => {
                write!(f, "Failed to load texture {:?}: {message}", path)
            }
        }
    }
}

impl std::error::Error for TextureError {}

/// Explicit descriptor for uploading a raw RGBA8 texture.
pub struct TextureUploadDesc<'a> {
    pub width: u32,
    pub height: u32,
    pub data: &'a [u8],
    pub format: wgpu::TextureFormat,
    pub label: Cow<'static, str>,
}

impl<'a> TextureUploadDesc<'a> {
    pub fn new(width: u32, height: u32, data: &'a [u8]) -> Self {
        Self {
            width,
            height,
            data,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            label: Cow::Borrowed("sprite_texture"),
        }
    }

    #[inline]
    pub fn format(mut self, format: wgpu::TextureFormat) -> Self {
        self.format = format;
        self
    }

    #[inline]
    pub fn label(mut self, label: impl Into<Cow<'static, str>>) -> Self {
        self.label = label.into();
        self
    }
}

/// Explicit descriptor for loading a texture file.
pub struct TextureFileDesc<'a> {
    pub path: &'a Path,
    pub format: wgpu::TextureFormat,
    pub label: Option<Cow<'static, str>>,
}

impl<'a> TextureFileDesc<'a> {
    pub fn new(path: &'a Path) -> Self {
        Self {
            path,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            label: None,
        }
    }

    #[inline]
    pub fn format(mut self, format: wgpu::TextureFormat) -> Self {
        self.format = format;
        self
    }

    #[inline]
    pub fn label(mut self, label: impl Into<Cow<'static, str>>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// Shared interior for a GPU texture.
struct TextureInner {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
}

/// A GPU texture — holds a wgpu texture and its default view.
///
/// Uses `Arc` internally so it can be cheaply cloned and shared
/// (e.g. across SpriteBatch draw calls). Resources are released
/// when the last reference is dropped.
///
/// Samplers are **not** bundled with textures. Use
/// `GpuContext::sampler_linear()` or `sampler_nearest()` when
/// creating bind groups.
#[derive(Clone)]
pub struct Texture(Arc<TextureInner>);

impl Texture {
    pub fn from_upload_desc(ctx: &GpuContext, desc: TextureUploadDesc<'_>) -> Self {
        Self::try_from_upload_desc(ctx, desc).expect("Texture::from_upload_desc failed")
    }

    pub fn try_from_upload_desc(
        ctx: &GpuContext,
        desc: TextureUploadDesc<'_>,
    ) -> Result<Self, TextureError> {
        let expected = (desc.width * desc.height * 4) as usize;
        if desc.data.len() != expected {
            return Err(TextureError::InvalidRgba8Length {
                expected,
                actual: desc.data.len(),
            });
        }

        let texture = ctx.device().create_texture(&wgpu::TextureDescriptor {
            label: Some(desc.label.as_ref()),
            size: wgpu::Extent3d {
                width: desc.width,
                height: desc.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: desc.format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        ctx.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            desc.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * desc.width),
                rows_per_image: Some(desc.height),
            },
            wgpu::Extent3d {
                width: desc.width,
                height: desc.height,
                depth_or_array_layers: 1,
            },
        );

        Ok(Self(Arc::new(TextureInner {
            texture,
            view,
            width: desc.width,
            height: desc.height,
            format: desc.format,
        })))
    }

    /// Create a texture from raw RGBA8 pixel data.
    ///
    /// `data` must be exactly `width * height * 4` bytes (RGBA, 1 byte each).
    pub fn from_rgba8(ctx: &GpuContext, width: u32, height: u32, data: &[u8]) -> Self {
        Self::from_upload_desc(ctx, TextureUploadDesc::new(width, height, data))
    }

    /// Create a texture from raw RGBA8 pixel data with a custom label.
    pub fn from_rgba8_with_label(
        ctx: &GpuContext,
        width: u32,
        height: u32,
        data: &[u8],
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::from_upload_desc(
            ctx,
            TextureUploadDesc::new(width, height, data).label(label),
        )
    }

    pub fn from_rgba8_with_format(
        ctx: &GpuContext,
        width: u32,
        height: u32,
        data: &[u8],
        format: wgpu::TextureFormat,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::try_from_rgba8_with_format(ctx, width, height, data, format, label)
            .expect("Texture::from_rgba8_with_format failed")
    }

    pub fn try_from_rgba8_with_format(
        ctx: &GpuContext,
        width: u32,
        height: u32,
        data: &[u8],
        format: wgpu::TextureFormat,
        label: impl Into<Cow<'static, str>>,
    ) -> Result<Self, TextureError> {
        Self::try_from_upload_desc(
            ctx,
            TextureUploadDesc::new(width, height, data)
                .format(format)
                .label(label),
        )
    }

    /// Load a texture from a PNG file.
    #[cfg(feature = "asset")]
    pub fn from_png(ctx: &GpuContext, path: &std::path::Path) -> Self {
        Self::try_from_png(ctx, path).expect("Texture::from_png failed")
    }

    #[cfg(feature = "asset")]
    pub fn from_file_desc(ctx: &GpuContext, desc: TextureFileDesc<'_>) -> Self {
        Self::try_from_file_desc(ctx, desc).expect("Texture::from_file_desc failed")
    }

    /// Load a texture from a PNG file.
    #[cfg(feature = "asset")]
    pub fn try_from_png(ctx: &GpuContext, path: &std::path::Path) -> Result<Self, TextureError> {
        Self::try_from_file_desc(ctx, TextureFileDesc::new(path))
    }

    /// Load a texture from a file with explicit format/label settings.
    #[cfg(feature = "asset")]
    pub fn try_from_file_desc(
        ctx: &GpuContext,
        desc: TextureFileDesc<'_>,
    ) -> Result<Self, TextureError> {
        let TextureFileDesc {
            path,
            format,
            label,
        } = desc;

        let img = image::open(path)
            .map_err(|e| TextureError::ImageLoad {
                path: path.to_path_buf(),
                message: e.to_string(),
            })?
            .to_rgba8();
        let (w, h) = img.dimensions();
        let label = label.unwrap_or_else(|| Cow::Owned(path.to_string_lossy().to_string()));
        Self::try_from_upload_desc(
            ctx,
            TextureUploadDesc::new(w, h, &img)
                .format(format)
                .label(label),
        )
    }

    /// The underlying wgpu texture.
    #[inline]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.0.texture
    }

    /// Whether two `Texture` handles point to the same underlying data.
    #[inline]
    pub fn ptr_eq(&self, other: &Texture) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// The default texture view.
    #[inline]
    pub fn view(&self) -> &wgpu::TextureView {
        &self.0.view
    }

    /// Texture width in pixels.
    #[inline]
    pub fn width(&self) -> u32 {
        self.0.width
    }

    /// Texture height in pixels.
    #[inline]
    pub fn height(&self) -> u32 {
        self.0.height
    }

    /// Texture pixel format.
    #[inline]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.0.format
    }

    /// Generate a 1×1 white pixel texture (used as default/fallback).
    pub fn white_pixel(ctx: &GpuContext) -> Self {
        Self::from_rgba8(ctx, 1, 1, &[255, 255, 255, 255])
    }

    /// Generate a procedural checkerboard texture for testing.
    pub fn checkerboard(
        ctx: &GpuContext,
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
        Self::from_rgba8(ctx, size, size, &data)
    }

    /// Generate a procedural circle/dot texture for particles.
    pub fn circle(ctx: &GpuContext, size: u32) -> Self {
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
        Self::from_rgba8(ctx, size, size, &data)
    }

    /// Generate a flat +Z normal map used as a default lighting input.
    pub fn flat_normal(ctx: &GpuContext) -> Self {
        Self::from_rgba8_with_format(
            ctx,
            1,
            1,
            &[128, 128, 255, 255],
            wgpu::TextureFormat::Rgba8Unorm,
            Cow::Borrowed("flat_normal"),
        )
    }

    /// Generate a spherical normal map inside a circular sprite.
    pub fn circle_normal(ctx: &GpuContext, size: u32) -> Self {
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
            ctx,
            size,
            size,
            &data,
            wgpu::TextureFormat::Rgba8Unorm,
            Cow::Borrowed("circle_normal"),
        )
    }
}
