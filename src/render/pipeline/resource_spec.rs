use std::borrow::Cow;

use crate::render::execution::{TextureFormat, TextureSlot};
use crate::render::graph::{
    RenderGraph, TargetSize, TextureBuilder, TextureHandle, DEFAULT_TEXTURE_USAGE,
};

/// User-facing texture declaration for programmable render pipelines.
///
/// `TextureSpec` is a small convenience layer over the render graph's
/// lower-level [`TextureBuilder`]. It keeps custom graph/compute/post passes
/// declarative without exposing callers to private graph descriptor fields.
#[derive(Debug, Clone)]
pub struct TextureSpec {
    name: Cow<'static, str>,
    size: TargetSize,
    format: TextureFormat,
    usage: wgpu::TextureUsages,
    sample_count: u32,
    mip_level_count: u32,
    array_layer_count: u32,
    transient: bool,
}

impl TextureSpec {
    #[inline]
    pub fn new(name: impl Into<Cow<'static, str>>, format: TextureFormat) -> Self {
        Self {
            name: name.into(),
            size: TargetSize::Surface,
            format,
            usage: DEFAULT_TEXTURE_USAGE,
            sample_count: 1,
            mip_level_count: 1,
            array_layer_count: 1,
            transient: true,
        }
    }

    #[inline]
    pub fn rgba8(name: impl Into<Cow<'static, str>>) -> Self {
        Self::new(name, TextureFormat::Rgba8Unorm)
    }

    #[inline]
    pub fn rgba16f(name: impl Into<Cow<'static, str>>) -> Self {
        Self::new(name, TextureFormat::Rgba16Float)
    }

    #[inline]
    pub fn rgba32f(name: impl Into<Cow<'static, str>>) -> Self {
        Self::new(name, TextureFormat::Rgba32Float)
    }

    #[inline]
    pub fn rg16f(name: impl Into<Cow<'static, str>>) -> Self {
        Self::new(name, TextureFormat::Rg16Float)
    }

    #[inline]
    pub fn rg32f(name: impl Into<Cow<'static, str>>) -> Self {
        Self::new(name, TextureFormat::Rg32Float)
    }

    #[inline]
    pub fn r32f(name: impl Into<Cow<'static, str>>) -> Self {
        Self::new(name, TextureFormat::R32Float)
    }

    #[inline]
    pub fn depth32(name: impl Into<Cow<'static, str>>) -> Self {
        Self::new(name, TextureFormat::Depth32Float)
    }

    #[inline]
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    #[inline]
    pub fn size(&self) -> TargetSize {
        self.size
    }

    #[inline]
    pub fn format(&self) -> TextureFormat {
        self.format
    }

    #[inline]
    pub fn usage_flags(&self) -> wgpu::TextureUsages {
        self.usage
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
    pub fn is_transient(&self) -> bool {
        self.transient
    }

    #[inline]
    pub fn full_res(mut self) -> Self {
        self.size = TargetSize::Surface;
        self
    }

    #[inline]
    pub fn half_res(mut self) -> Self {
        self.size = TargetSize::Scale(0.5);
        self
    }

    #[inline]
    pub fn scale(mut self, scale: f32) -> Self {
        self.size = TargetSize::Scale(scale);
        self
    }

    #[inline]
    pub fn exact(mut self, width: u32, height: u32) -> Self {
        self.size = TargetSize::Exact(width, height);
        self
    }

    #[inline]
    pub fn with_format(mut self, format: TextureFormat) -> Self {
        self.format = format;
        self
    }

    #[inline]
    pub fn usage(mut self, usage: wgpu::TextureUsages) -> Self {
        self.usage = usage;
        self
    }

    #[inline]
    pub fn add_usage(mut self, usage: wgpu::TextureUsages) -> Self {
        self.usage |= usage;
        self
    }

    #[inline]
    pub fn storage(mut self) -> Self {
        self.usage |= wgpu::TextureUsages::STORAGE_BINDING;
        self
    }

    #[inline]
    pub fn sampled(mut self) -> Self {
        self.usage |= wgpu::TextureUsages::TEXTURE_BINDING;
        self
    }

    #[inline]
    pub fn render_attachment(mut self) -> Self {
        self.usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
        self
    }

    #[inline]
    pub fn copy_src(mut self) -> Self {
        self.usage |= wgpu::TextureUsages::COPY_SRC;
        self
    }

    #[inline]
    pub fn copy_dst(mut self) -> Self {
        self.usage |= wgpu::TextureUsages::COPY_DST;
        self
    }

    #[inline]
    pub fn samples(mut self, sample_count: u32) -> Self {
        self.sample_count = sample_count.max(1);
        self
    }

    #[inline]
    pub fn mips(mut self, mip_level_count: u32) -> Self {
        self.mip_level_count = mip_level_count.max(1);
        self
    }

    #[inline]
    pub fn array_layers(mut self, array_layer_count: u32) -> Self {
        self.array_layer_count = array_layer_count.max(1);
        self
    }

    #[inline]
    pub fn transient(mut self) -> Self {
        self.transient = true;
        self
    }

    #[inline]
    pub fn persistent(mut self) -> Self {
        self.transient = false;
        self
    }

    #[inline]
    pub fn apply_to(&self, builder: &mut TextureBuilder) {
        builder
            .name(self.name.clone())
            .size(self.size)
            .format(self.format)
            .usage(self.usage)
            .sample_count(self.sample_count)
            .mip_level_count(self.mip_level_count)
            .array_layer_count(self.array_layer_count);
        if !self.transient {
            builder.persistent();
        }
    }

    #[inline]
    pub fn create(&self, graph: &mut RenderGraph) -> TextureHandle {
        graph.create_texture(|builder| self.apply_to(builder))
    }

    #[inline]
    pub fn create_slot(&self, graph: &mut RenderGraph) -> TextureSlot {
        TextureSlot::new(self.create(graph), self.format)
    }
}
