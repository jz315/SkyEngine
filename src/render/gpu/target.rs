//! Off-screen render target helpers.

use std::borrow::Cow;

use crate::gpu::GpuContext;
use crate::render::gpu::TextureCreateDesc;

pub const DEFAULT_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[inline]
pub const fn is_depth_format(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Depth16Unorm
            | wgpu::TextureFormat::Depth24Plus
            | wgpu::TextureFormat::Depth24PlusStencil8
            | wgpu::TextureFormat::Depth32Float
            | wgpu::TextureFormat::Depth32FloatStencil8
    )
}

/// Explicit descriptor for creating a render target.
#[derive(Debug, Clone)]
pub struct RenderTargetDescriptor {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub usage: wgpu::TextureUsages,
    pub sample_count: u32,
    pub mip_level_count: u32,
    pub array_layer_count: u32,
    pub label: Cow<'static, str>,
}

impl RenderTargetDescriptor {
    pub fn new(width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        Self {
            width,
            height,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            sample_count: 1,
            mip_level_count: 1,
            array_layer_count: 1,
            label: Cow::Borrowed("render_target"),
        }
    }

    pub fn new_depth(width: u32, height: u32) -> Self {
        Self::new(width, height, DEFAULT_DEPTH_FORMAT).label("depth_target")
    }

    #[inline]
    pub fn sample_count(mut self, sample_count: u32) -> Self {
        self.sample_count = sample_count.max(1);
        self
    }

    #[inline]
    pub fn usage(mut self, usage: wgpu::TextureUsages) -> Self {
        self.usage = usage;
        self
    }

    #[inline]
    pub fn mip_level_count(mut self, mip_level_count: u32) -> Self {
        self.mip_level_count = mip_level_count.max(1);
        self
    }

    #[inline]
    pub fn array_layer_count(mut self, array_layer_count: u32) -> Self {
        self.array_layer_count = array_layer_count.max(1);
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
    usage: wgpu::TextureUsages,
    sample_count: u32,
    mip_level_count: u32,
    array_layer_count: u32,
    label: Cow<'static, str>,
}

impl std::fmt::Debug for RenderTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderTarget")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .field("usage", &self.usage)
            .field("sample_count", &self.sample_count)
            .field("mip_level_count", &self.mip_level_count)
            .field("array_layer_count", &self.array_layer_count)
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
            .usage(desc.usage)
            .sample_count(desc.sample_count)
            .mip_level_count(desc.mip_level_count)
            .depth_or_array_layers(desc.array_layer_count)
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

    pub fn new_depth(ctx: &GpuContext, width: u32, height: u32) -> Self {
        Self::from_descriptor(ctx, RenderTargetDescriptor::new_depth(width, height))
    }

    /// Create a new off-screen target from an explicit descriptor.
    pub fn from_descriptor(ctx: &GpuContext, desc: RenderTargetDescriptor) -> Self {
        let width = desc.width.max(1);
        let height = desc.height.max(1);
        let desc = RenderTargetDescriptor {
            width,
            height,
            array_layer_count: desc.array_layer_count.max(1),
            ..desc
        };
        let (texture, view) = Self::create_texture(ctx, &desc);

        Self {
            texture,
            view,
            width: desc.width,
            height: desc.height,
            format: desc.format,
            usage: desc.usage,
            sample_count: desc.sample_count,
            mip_level_count: desc.mip_level_count,
            array_layer_count: desc.array_layer_count,
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
                .usage(self.usage)
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
        let array_layer_count = desc.array_layer_count.max(1);
        if self.width == width
            && self.height == height
            && self.format == desc.format
            && self.usage == desc.usage
            && self.sample_count == sample_count
            && self.mip_level_count == mip_level_count
            && self.array_layer_count == array_layer_count
        {
            return;
        }

        let desc = RenderTargetDescriptor {
            width,
            height,
            format: desc.format,
            usage: desc.usage,
            sample_count,
            mip_level_count,
            array_layer_count,
            label: desc.label,
        };
        let (texture, view) = Self::create_texture(ctx, &desc);
        self.texture = texture;
        self.view = view;
        self.width = desc.width;
        self.height = desc.height;
        self.format = desc.format;
        self.usage = desc.usage;
        self.sample_count = desc.sample_count;
        self.mip_level_count = desc.mip_level_count;
        self.array_layer_count = desc.array_layer_count;
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
    fn validate_view_subresource_range(
        &self,
        base_mip_level: u32,
        mip_level_count: Option<u32>,
        base_array_layer: u32,
        array_layer_count: Option<u32>,
    ) {
        assert!(
            base_mip_level < self.mip_level_count,
            "RenderTarget mip level {base_mip_level} out of range for {} mip levels",
            self.mip_level_count
        );

        let mip_count = mip_level_count.unwrap_or(self.mip_level_count - base_mip_level);
        assert!(
            mip_count >= 1 && base_mip_level + mip_count <= self.mip_level_count,
            "RenderTarget mip range [{base_mip_level}, {}) exceeds {} mip levels",
            base_mip_level + mip_count,
            self.mip_level_count
        );

        let layer_count = array_layer_count.unwrap_or(1);
        assert!(
            base_array_layer < self.array_layer_count,
            "RenderTarget array layer {base_array_layer} out of range for {} layers",
            self.array_layer_count
        );
        assert!(
            layer_count >= 1 && base_array_layer + layer_count <= self.array_layer_count,
            "RenderTarget array layer range [{base_array_layer}, {}) exceeds {} layers",
            base_array_layer + layer_count,
            self.array_layer_count
        );
    }

    /// Create an explicit subresource view into this render target.
    ///
    /// This is the main hook for passes that need per-mip sampling or storage views.
    pub fn create_view_with(&self, desc: &wgpu::TextureViewDescriptor<'_>) -> wgpu::TextureView {
        self.validate_view_subresource_range(
            desc.base_mip_level,
            desc.mip_level_count,
            desc.base_array_layer,
            desc.array_layer_count,
        );
        self.texture.create_view(desc)
    }

    /// Create a view for a single mip level.
    #[inline]
    pub fn create_mip_view(&self, mip_level: u32) -> wgpu::TextureView {
        self.create_view_with(&wgpu::TextureViewDescriptor {
            base_mip_level: mip_level,
            mip_level_count: Some(1),
            ..Default::default()
        })
    }

    /// Create a view for one array layer.
    #[inline]
    pub fn create_array_layer_view(&self, array_layer: u32) -> wgpu::TextureView {
        self.create_view_with(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_array_layer: array_layer,
            array_layer_count: Some(1),
            ..Default::default()
        })
    }

    /// Create a view for one mip level and one array layer.
    #[inline]
    pub fn create_mip_array_layer_view(
        &self,
        mip_level: u32,
        array_layer: u32,
    ) -> wgpu::TextureView {
        self.create_view_with(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            base_mip_level: mip_level,
            mip_level_count: Some(1),
            base_array_layer: array_layer,
            array_layer_count: Some(1),
            ..Default::default()
        })
    }

    /// Return the logical dimensions of a specific mip level.
    #[inline]
    pub fn mip_extent(&self, mip_level: u32) -> (u32, u32) {
        assert!(
            mip_level < self.mip_level_count,
            "RenderTarget mip level {mip_level} out of range for {} mip levels",
            self.mip_level_count
        );
        let width = self.width.checked_shr(mip_level).unwrap_or(0).max(1);
        let height = self.height.checked_shr(mip_level).unwrap_or(0).max(1);
        (width, height)
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
    pub fn usage(&self) -> wgpu::TextureUsages {
        self.usage
    }

    #[inline]
    pub fn is_depth(&self) -> bool {
        is_depth_format(self.format)
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
    pub fn array_layer_count(&self) -> u32 {
        self.array_layer_count
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
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for render target tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("render_target_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
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
        assert_eq!(target.array_layer_count(), 1);
        assert_eq!(target.label(), "target_reconfigured");
    }

    #[test]
    fn depth_target_helper_uses_depth_format() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [32, 32]);
        let target = RenderTarget::new_depth(&ctx, 16, 16);

        assert_eq!(target.format(), DEFAULT_DEPTH_FORMAT);
        assert!(target.is_depth());
        assert!(is_depth_format(DEFAULT_DEPTH_FORMAT));
    }

    #[test]
    fn mip_view_helpers_create_bindable_subresource_views() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [32, 32]);
        let target = RenderTarget::from_descriptor(
            &ctx,
            RenderTargetDescriptor::new(16, 8, wgpu::TextureFormat::Rgba8Unorm)
                .mip_level_count(4)
                .label("mipped_target"),
        );

        assert_eq!(target.mip_extent(0), (16, 8));
        assert_eq!(target.mip_extent(1), (8, 4));
        assert_eq!(target.mip_extent(2), (4, 2));
        assert_eq!(target.mip_extent(3), (2, 1));

        let mip_view = target.create_mip_view(2);
        let range_view = target.create_view_with(&wgpu::TextureViewDescriptor {
            base_mip_level: 1,
            mip_level_count: Some(2),
            ..Default::default()
        });

        let bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("render_target_mip_view_test_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });

        let _single_mip_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render_target_single_mip_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&mip_view),
            }],
        });

        let _range_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render_target_range_mip_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&range_view),
            }],
        });
    }

    #[test]
    fn array_layer_view_helpers_create_bindable_subresource_views() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [32, 32]);
        let target = RenderTarget::from_descriptor(
            &ctx,
            RenderTargetDescriptor::new(16, 8, wgpu::TextureFormat::Rgba8Unorm)
                .mip_level_count(3)
                .array_layer_count(4)
                .label("array_target"),
        );

        assert_eq!(target.array_layer_count(), 4);

        let layer_view = target.create_array_layer_view(2);
        let mip_layer_view = target.create_mip_array_layer_view(1, 3);

        let bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("render_target_array_view_test_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
            });

        let _layer_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render_target_array_layer_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&layer_view),
            }],
        });

        let _mip_layer_bg = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render_target_mip_array_layer_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&mip_layer_view),
            }],
        });
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn create_mip_view_panics_for_invalid_level() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [32, 32]);
        let target = RenderTarget::from_descriptor(
            &ctx,
            RenderTargetDescriptor::new(8, 8, wgpu::TextureFormat::Rgba8Unorm)
                .mip_level_count(2)
                .label("invalid_mip_target"),
        );

        let _ = target.create_mip_view(2);
    }
}
