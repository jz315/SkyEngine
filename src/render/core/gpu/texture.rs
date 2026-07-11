//! GPU texture loading and management.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::gpu::GpuContext;

/// Errors returned by fallible texture creation APIs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureError {
    InvalidRgba8Length { expected: usize, actual: usize },
    InvalidTextureSize { width: u32, height: u32 },
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
            Self::InvalidTextureSize { width, height } => {
                write!(f, "Invalid texture size: {width}x{height}")
            }
            Self::ImageLoad { path, message } => {
                write!(f, "Failed to load texture {:?}: {message}", path)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for texture tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("texture_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn texture_create_desc_tracks_metadata() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [16, 16]);

        let texture = Texture::create(
            &ctx,
            TextureCreateDesc::new_2d(16, 8, wgpu::TextureFormat::Rgba8Unorm)
                .mip_level_count(3)
                .label("mipped_texture"),
        );

        assert_eq!(texture.width(), 16);
        assert_eq!(texture.height(), 8);
        assert_eq!(texture.depth_or_array_layers(), 1);
        assert_eq!(texture.mip_level_count(), 3);
        assert_eq!(texture.sample_count(), 1);
        assert_eq!(texture.dimension(), wgpu::TextureDimension::D2);
        assert_eq!(texture.resident_bytes(), Some(672));
    }

    #[test]
    fn texture_resident_bytes_reports_common_format_sizes() {
        assert_eq!(
            texture_resident_bytes(2, 3, 1, wgpu::TextureFormat::Rgba8UnormSrgb),
            Some(24)
        );
        assert_eq!(
            texture_resident_bytes(2, 3, 1, wgpu::TextureFormat::Rgba16Float),
            Some(48)
        );
        assert_eq!(
            texture_resident_bytes(2, 3, 2, wgpu::TextureFormat::Rgba16Float),
            Some(96)
        );
        assert_eq!(
            texture_resident_bytes(
                u32::MAX,
                u32::MAX,
                u32::MAX,
                wgpu::TextureFormat::Rgba32Float
            ),
            None
        );
        assert_eq!(
            texture_resident_bytes_with_metadata(
                wgpu::Extent3d {
                    width: 8,
                    height: 4,
                    depth_or_array_layers: 2,
                },
                wgpu::TextureDimension::D2,
                3,
                4,
                wgpu::TextureFormat::Rgba8Unorm,
            ),
            Some((64 + 16 + 4) * 4 * 4)
        );
    }

    #[test]
    fn rgba8_upload_size_rejects_empty_dimensions() {
        assert_eq!(
            rgba8_len(0, 4),
            Err(TextureError::InvalidTextureSize {
                width: 0,
                height: 4,
            })
        );
        assert_eq!(
            rgba8_len(4, 0),
            Err(TextureError::InvalidTextureSize {
                width: 4,
                height: 0,
            })
        );
    }

    #[test]
    fn procedural_textures_clamp_empty_sizes_and_tiles() {
        let (device, queue) = create_test_device();
        let ctx = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [1, 1]);

        let checkerboard = Texture::checkerboard(&ctx, 0, 0, [0; 4], [255; 4]);
        let circle = Texture::circle(&ctx, 0);
        let normal = Texture::circle_normal(&ctx, 0);

        assert_eq!([checkerboard.width(), checkerboard.height()], [1, 1]);
        assert_eq!([circle.width(), circle.height()], [1, 1]);
        assert_eq!([normal.width(), normal.height()], [1, 1]);
    }
}

impl std::error::Error for TextureError {}

/// Explicit descriptor for creating an empty GPU texture.
pub struct TextureCreateDesc {
    pub size: wgpu::Extent3d,
    pub format: wgpu::TextureFormat,
    pub usage: wgpu::TextureUsages,
    pub dimension: wgpu::TextureDimension,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub label: Cow<'static, str>,
    pub view_dimension: Option<wgpu::TextureViewDimension>,
}

impl TextureCreateDesc {
    pub fn new_2d(width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        Self {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            dimension: wgpu::TextureDimension::D2,
            mip_level_count: 1,
            sample_count: 1,
            label: Cow::Borrowed("texture"),
            view_dimension: None,
        }
    }

    #[inline]
    pub fn usage(mut self, usage: wgpu::TextureUsages) -> Self {
        self.usage = usage;
        self
    }

    #[inline]
    pub fn dimension(mut self, dimension: wgpu::TextureDimension) -> Self {
        self.dimension = dimension;
        self
    }

    #[inline]
    pub fn mip_level_count(mut self, mip_level_count: u32) -> Self {
        self.mip_level_count = mip_level_count.max(1);
        self
    }

    #[inline]
    pub fn depth_or_array_layers(mut self, depth_or_array_layers: u32) -> Self {
        self.size.depth_or_array_layers = depth_or_array_layers.max(1);
        self
    }

    #[inline]
    pub fn sample_count(mut self, sample_count: u32) -> Self {
        self.sample_count = sample_count.max(1);
        self
    }

    #[inline]
    pub fn label(mut self, label: impl Into<Cow<'static, str>>) -> Self {
        self.label = label.into();
        self
    }

    #[inline]
    pub fn view_dimension(mut self, view_dimension: wgpu::TextureViewDimension) -> Self {
        self.view_dimension = Some(view_dimension);
        self
    }
}

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
    size: wgpu::Extent3d,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    dimension: wgpu::TextureDimension,
    mip_level_count: u32,
    sample_count: u32,
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
    pub fn create(ctx: &GpuContext, desc: TextureCreateDesc) -> Self {
        let size = normalize_texture_size(desc.size, desc.dimension);
        let mip_level_count = desc.mip_level_count.max(1);
        let sample_count = desc.sample_count.max(1);
        let texture = ctx.device().create_texture(&wgpu::TextureDescriptor {
            label: Some(desc.label.as_ref()),
            size,
            mip_level_count,
            sample_count,
            dimension: desc.dimension,
            format: desc.format,
            usage: desc.usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: desc.view_dimension,
            ..Default::default()
        });

        Self(Arc::new(TextureInner {
            texture,
            view,
            size,
            format: desc.format,
            usage: desc.usage,
            dimension: desc.dimension,
            mip_level_count,
            sample_count,
        }))
    }

    pub fn from_upload_desc(ctx: &GpuContext, desc: TextureUploadDesc<'_>) -> Self {
        Self::try_from_upload_desc(ctx, desc).expect("Texture::from_upload_desc failed")
    }

    pub fn try_from_upload_desc(
        ctx: &GpuContext,
        desc: TextureUploadDesc<'_>,
    ) -> Result<Self, TextureError> {
        let expected = rgba8_len(desc.width, desc.height)?;
        if desc.data.len() != expected {
            return Err(TextureError::InvalidRgba8Length {
                expected,
                actual: desc.data.len(),
            });
        }

        let create_desc = TextureCreateDesc::new_2d(desc.width, desc.height, desc.format)
            .usage(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST)
            .label(desc.label);
        let texture = Self::create(ctx, create_desc);

        ctx.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: texture.texture(),
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

        Ok(texture)
    }

    /// Upload raw RGBA8 pixels into this texture without reallocating it.
    ///
    /// This is the hot path for streamed video frames and other dynamic
    /// textures. The texture must have been created with `COPY_DST` usage.
    pub fn write_rgba8(&self, ctx: &GpuContext, data: &[u8]) -> Result<(), TextureError> {
        let expected = rgba8_len(self.width(), self.height())?;
        if data.len() != expected {
            return Err(TextureError::InvalidRgba8Length {
                expected,
                actual: data.len(),
            });
        }

        ctx.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: self.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * self.width()),
                rows_per_image: Some(self.height()),
            },
            wgpu::Extent3d {
                width: self.width(),
                height: self.height(),
                depth_or_array_layers: 1,
            },
        );
        Ok(())
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
        self.0.size.width
    }

    /// Texture height in pixels.
    #[inline]
    pub fn height(&self) -> u32 {
        self.0.size.height
    }

    /// Texture depth or array layer count.
    #[inline]
    pub fn depth_or_array_layers(&self) -> u32 {
        self.0.size.depth_or_array_layers
    }

    /// Full logical texture size.
    #[inline]
    pub fn size(&self) -> wgpu::Extent3d {
        self.0.size
    }

    /// Texture pixel format.
    #[inline]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.0.format
    }

    /// Texture usage flags used at creation.
    #[inline]
    pub fn usage(&self) -> wgpu::TextureUsages {
        self.0.usage
    }

    /// Texture dimension.
    #[inline]
    pub fn dimension(&self) -> wgpu::TextureDimension {
        self.0.dimension
    }

    /// Number of mip levels allocated for the texture.
    #[inline]
    pub fn mip_level_count(&self) -> u32 {
        self.0.mip_level_count
    }

    /// Number of samples per texel.
    #[inline]
    pub fn sample_count(&self) -> u32 {
        self.0.sample_count
    }

    /// Approximate bytes retained by this texture's allocated texels.
    ///
    /// Compressed, packed, multi-planar, and other uncommon formats return
    /// `None` until the engine has an explicit accounting rule for them.
    #[inline]
    pub fn resident_bytes(&self) -> Option<usize> {
        texture_resident_bytes_with_metadata(
            self.0.size,
            self.dimension(),
            self.mip_level_count(),
            self.sample_count(),
            self.format(),
        )
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
        let size = size.max(1);
        let tile_size = tile_size.max(1);
        let mut data = vec![
            0u8;
            rgba8_len(size, size)
                .expect("checkerboard texture dimensions are too large")
        ];
        for y in 0..size {
            for x in 0..size {
                let is_a = ((x / tile_size) + (y / tile_size)).is_multiple_of(2);
                let color = if is_a { color_a } else { color_b };
                let i = ((y * size + x) * 4) as usize;
                data[i..i + 4].copy_from_slice(&color);
            }
        }
        Self::from_rgba8(ctx, size, size, &data)
    }

    /// Generate a procedural circle/dot texture for particles.
    pub fn circle(ctx: &GpuContext, size: u32) -> Self {
        let size = size.max(1);
        let mut data =
            vec![0u8; rgba8_len(size, size).expect("circle texture dimensions are too large")];
        let center = size as f32 * 0.5;
        let radius = center - 1.0;
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - center;
                let dy = y as f32 + 0.5 - center;
                let dist = (dx * dx + dy * dy).sqrt();
                let alpha = ((radius - dist).clamp(0.0, 1.0) * 255.0) as u8;
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
        let size = size.max(1);
        let mut data = vec![
            0u8;
            rgba8_len(size, size)
                .expect("circle normal texture dimensions are too large")
        ];
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

fn rgba8_len(width: u32, height: u32) -> Result<usize, TextureError> {
    if width == 0 || height == 0 {
        return Err(TextureError::InvalidTextureSize { width, height });
    }
    width
        .checked_mul(height)
        .and_then(|value| value.checked_mul(4))
        .map(|value| value as usize)
        .ok_or(TextureError::InvalidTextureSize { width, height })
}

#[cfg(test)]
fn texture_resident_bytes(
    width: u32,
    height: u32,
    depth_or_array_layers: u32,
    format: wgpu::TextureFormat,
) -> Option<usize> {
    texture_resident_bytes_with_metadata(
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers,
        },
        wgpu::TextureDimension::D2,
        1,
        1,
        format,
    )
}

fn texture_resident_bytes_with_metadata(
    size: wgpu::Extent3d,
    dimension: wgpu::TextureDimension,
    mip_level_count: u32,
    sample_count: u32,
    format: wgpu::TextureFormat,
) -> Option<usize> {
    let bytes_per_texel = texture_format_bytes_per_texel(format)?;
    let mut total = 0usize;
    for mip in 0..mip_level_count.max(1) {
        let width = size.width.checked_shr(mip).unwrap_or(0).max(1) as usize;
        let height = match dimension {
            wgpu::TextureDimension::D1 => 1,
            wgpu::TextureDimension::D2 | wgpu::TextureDimension::D3 => {
                size.height.checked_shr(mip).unwrap_or(0).max(1) as usize
            }
        };
        let depth_or_layers = match dimension {
            wgpu::TextureDimension::D3 => size
                .depth_or_array_layers
                .checked_shr(mip)
                .unwrap_or(0)
                .max(1) as usize,
            wgpu::TextureDimension::D1 | wgpu::TextureDimension::D2 => {
                size.depth_or_array_layers.max(1) as usize
            }
        };
        let mip_bytes = width
            .checked_mul(height)?
            .checked_mul(depth_or_layers)?
            .checked_mul(bytes_per_texel)?
            .checked_mul(sample_count.max(1) as usize)?;
        total = total.checked_add(mip_bytes)?;
    }
    Some(total)
}

fn normalize_texture_size(
    size: wgpu::Extent3d,
    dimension: wgpu::TextureDimension,
) -> wgpu::Extent3d {
    match dimension {
        wgpu::TextureDimension::D1 => wgpu::Extent3d {
            width: size.width.max(1),
            height: 1,
            depth_or_array_layers: 1,
        },
        wgpu::TextureDimension::D2 => wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: size.depth_or_array_layers.max(1),
        },
        wgpu::TextureDimension::D3 => wgpu::Extent3d {
            width: size.width.max(1),
            height: size.height.max(1),
            depth_or_array_layers: size.depth_or_array_layers.max(1),
        },
    }
}

fn texture_format_bytes_per_texel(format: wgpu::TextureFormat) -> Option<usize> {
    Some(match format {
        wgpu::TextureFormat::R8Unorm
        | wgpu::TextureFormat::R8Snorm
        | wgpu::TextureFormat::R8Uint
        | wgpu::TextureFormat::R8Sint => 1,
        wgpu::TextureFormat::R16Uint
        | wgpu::TextureFormat::R16Sint
        | wgpu::TextureFormat::R16Float
        | wgpu::TextureFormat::Rg8Unorm
        | wgpu::TextureFormat::Rg8Snorm
        | wgpu::TextureFormat::Rg8Uint
        | wgpu::TextureFormat::Rg8Sint
        | wgpu::TextureFormat::Depth16Unorm => 2,
        wgpu::TextureFormat::R32Uint
        | wgpu::TextureFormat::R32Sint
        | wgpu::TextureFormat::R32Float
        | wgpu::TextureFormat::Rg16Uint
        | wgpu::TextureFormat::Rg16Sint
        | wgpu::TextureFormat::Rg16Float
        | wgpu::TextureFormat::Rgba8Unorm
        | wgpu::TextureFormat::Rgba8UnormSrgb
        | wgpu::TextureFormat::Rgba8Snorm
        | wgpu::TextureFormat::Rgba8Uint
        | wgpu::TextureFormat::Rgba8Sint
        | wgpu::TextureFormat::Bgra8Unorm
        | wgpu::TextureFormat::Bgra8UnormSrgb
        | wgpu::TextureFormat::Depth32Float => 4,
        wgpu::TextureFormat::Rg32Uint
        | wgpu::TextureFormat::Rg32Sint
        | wgpu::TextureFormat::Rg32Float
        | wgpu::TextureFormat::Rgba16Uint
        | wgpu::TextureFormat::Rgba16Sint
        | wgpu::TextureFormat::Rgba16Float => 8,
        wgpu::TextureFormat::Rgba32Uint
        | wgpu::TextureFormat::Rgba32Sint
        | wgpu::TextureFormat::Rgba32Float => 16,
        _ => return None,
    })
}
