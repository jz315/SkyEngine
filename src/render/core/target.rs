//! Off-screen render target helpers.

use std::borrow::Cow;

use crate::gpu::GpuContext;
use crate::render::core::texture::TextureCreateDesc;

/// Explicit descriptor for creating a render target.
#[derive(Debug, Clone)]
pub struct RenderTargetDescriptor {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub sample_count: u32,
    pub mip_level_count: u32,
    pub label: Cow<'static, str>,
}

impl RenderTargetDescriptor {
    pub fn new(width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        Self {
            width,
            height,
            format,
            sample_count: 1,
            mip_level_count: 1,
            label: Cow::Borrowed("render_target"),
        }
    }

    #[inline]
    pub fn sample_count(mut self, sample_count: u32) -> Self {
        self.sample_count = sample_count.max(1);
        self
    }

    #[inline]
    pub fn mip_level_count(mut self, mip_level_count: u32) -> Self {
        self.mip_level_count = mip_level_count.max(1);
        self
    }

    #[inline]
    pub fn label(mut self, label: impl Into<Cow<'static, str>>) -> Self {
        self.label = label.into();
        self
    }
}

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
    sample_count: u32,
    mip_level_count: u32,
    label: Cow<'static, str>,
}

impl std::fmt::Debug for RenderTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderTarget")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .field("sample_count", &self.sample_count)
            .field("mip_level_count", &self.mip_level_count)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl RenderTarget {
    fn create_texture(
        ctx: &GpuContext,
        desc: &RenderTargetDescriptor,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture_desc = TextureCreateDesc::new_2d(desc.width, desc.height, desc.format)
            .usage(
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
            )
            .sample_count(desc.sample_count)
            .mip_level_count(desc.mip_level_count)
            .label(desc.label.clone());
        let texture = ctx.device().create_texture(&wgpu::TextureDescriptor {
            label: Some(texture_desc.label.as_ref()),
            size: texture_desc.size,
            mip_level_count: texture_desc.mip_level_count,
            sample_count: texture_desc.sample_count,
            dimension: texture_desc.dimension,
            format: texture_desc.format,
            usage: texture_desc.usage,
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
        let desc = RenderTargetDescriptor::new(width, height, format).label(label);
        Self::from_descriptor(ctx, desc)
    }

    /// Create a new off-screen target from an explicit descriptor.
    pub fn from_descriptor(ctx: &GpuContext, desc: RenderTargetDescriptor) -> Self {
        let width = desc.width.max(1);
        let height = desc.height.max(1);
        let desc = RenderTargetDescriptor {
            width,
            height,
            ..desc
        };
        let (texture, view) = Self::create_texture(ctx, &desc);

        Self {
            texture,
            view,
            width: desc.width,
            height: desc.height,
            format: desc.format,
            sample_count: desc.sample_count,
            mip_level_count: desc.mip_level_count,
            label: desc.label,
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
        self.resize_with(
            ctx,
            RenderTargetDescriptor::new(width, height, format)
                .sample_count(self.sample_count)
                .mip_level_count(self.mip_level_count)
                .label(self.label.clone()),
        );
    }

    /// Resize or reconfigure the target from an explicit descriptor.
    pub fn resize_with(&mut self, ctx: &GpuContext, desc: RenderTargetDescriptor) {
        // wgpu requires at least 1×1 textures; clamp to avoid panics on minimize.
        let width = desc.width.max(1);
        let height = desc.height.max(1);
        let sample_count = desc.sample_count.max(1);
        let mip_level_count = desc.mip_level_count.max(1);
        if self.width == width
            && self.height == height
            && self.format == desc.format
            && self.sample_count == sample_count
            && self.mip_level_count == mip_level_count
        {
            return;
        }

        let desc = RenderTargetDescriptor {
            width,
            height,
            format: desc.format,
            sample_count,
            mip_level_count,
            label: desc.label,
        };
        let (texture, view) = Self::create_texture(ctx, &desc);
        self.texture = texture;
        self.view = view;
        self.width = desc.width;
        self.height = desc.height;
        self.format = desc.format;
        self.sample_count = desc.sample_count;
        self.mip_level_count = desc.mip_level_count;
        self.label = desc.label;
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
    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    #[inline]
    pub fn mip_level_count(&self) -> u32 {
        self.mip_level_count
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for render target tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("render_target_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn resize_with_updates_descriptor_fields() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [32, 32]);
        let mut target = RenderTarget::from_descriptor(
            &ctx,
            RenderTargetDescriptor::new(16, 16, wgpu::TextureFormat::Rgba8Unorm)
                .mip_level_count(2)
                .label("target"),
        );

        target.resize_with(
            &ctx,
            RenderTargetDescriptor::new(32, 24, wgpu::TextureFormat::Rgba16Float)
                .mip_level_count(3)
                .label("target_reconfigured"),
        );

        assert_eq!(target.width(), 32);
        assert_eq!(target.height(), 24);
        assert_eq!(target.format(), wgpu::TextureFormat::Rgba16Float);
        assert_eq!(target.sample_count(), 1);
        assert_eq!(target.mip_level_count(), 3);
        assert_eq!(target.label(), "target_reconfigured");
    }
}
