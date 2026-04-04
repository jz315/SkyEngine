//! Off-screen render target helpers.

use std::borrow::Cow;

use crate::gpu::GpuContext;

/// A persistent off-screen render target.
///
/// Holds a wgpu texture, its default view, and a sampler.
/// Resources are automatically released when the `RenderTarget` is dropped.
pub struct RenderTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    label: Cow<'static, str>,
}

impl std::fmt::Debug for RenderTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderTarget")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl RenderTarget {
    fn create_texture(
        ctx: &GpuContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = ctx.device().create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Create a new off-screen target.
    pub fn new(
        ctx: &GpuContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        let label = label.into();
        let (texture, view) = Self::create_texture(ctx, width, height, format, label.as_ref());

        Self {
            texture,
            view,
            width,
            height,
            format,
            label,
        }
    }

    /// Resize the target if the dimensions changed.
    ///
    /// The old texture is dropped automatically (wgpu's Drop handles GPU cleanup).
    pub fn resize(
        &mut self,
        ctx: &GpuContext,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) {
        // wgpu requires at least 1×1 textures; clamp to avoid panics on minimize.
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height && self.format == format {
            return;
        }

        let (texture, view) = Self::create_texture(ctx, width, height, format, self.label.as_ref());
        self.texture = texture;
        self.view = view;
        self.width = width;
        self.height = height;
        self.format = format;
    }

    /// The underlying wgpu texture.
    #[inline]
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// The default texture view.
    #[inline]
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
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
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }
}

impl crate::gpu::ColorTargetView for RenderTarget {
    #[inline]
    fn color_target_view(&self) -> &wgpu::TextureView {
        self.view()
    }
}
