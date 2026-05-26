//! wgpu renderer for `eui_neo` draw commands.

use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::time::Instant;

use crate::{
    draw_fullscreen_triangle, NeoImageVertex, NeoPolygonVertex, NeoRectVertex, NeoWgpuResources,
    ScreenUniform,
};
use eui_neo::expert::{
    UiDrawCommand, UiDrawList, UiImageDraw, UiNineSliceDraw, UiPolygonDraw, UiRectDraw, UiTextDraw,
};
use eui_neo::{
    Color, FontRef, Frame, GradientDirection, HorizontalAlign, ImageFit, ImageRef, LayoutRect,
    Screen, Transform, UiClip, VerticalAlign,
};
use glyphon::cosmic_text::Align as TextAlign;
use glyphon::{
    Attrs, Buffer, Cache, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap,
};
use rustc_hash::{FxHashMap, FxHasher};

use crate::fonts::{
    clear_registered_font, is_icon_font, register_font_bytes, resolve_family, resolved_font_weight,
    RegisteredFont, DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE,
};

const ATLAS_TRIM_INTERVAL_FRAMES: u32 = 120;

/// Current wgpu render target supplied by the host application.
pub struct Target<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub view: &'a wgpu::TextureView,
    pub target_texture: Option<TargetTexture<'a>>,
    pub format: wgpu::TextureFormat,
    pub physical_size: [u32; 2],
}

/// Optional copy source for backdrop blur.
#[derive(Clone, Copy)]
pub struct TargetTexture<'a> {
    pub texture: &'a wgpu::Texture,
}

struct WgpuVertexBuffer {
    buffer: wgpu::Buffer,
    used: u64,
    capacity: u64,
    content_hash: u64,
}

impl WgpuVertexBuffer {
    fn upload<T: bytemuck::Pod>(
        slot: &mut Option<Self>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        data: &[T],
    ) {
        if data.is_empty() {
            if let Some(buffer) = slot {
                buffer.used = 0;
            }
            return;
        }

        let used = std::mem::size_of_val(data) as u64;
        let bytes = bytemuck::cast_slice(data);
        let content_hash = hash_bytes(bytes);
        let needs_buffer = slot.as_ref().is_none_or(|buffer| buffer.capacity < used);
        if needs_buffer {
            let capacity = used.next_power_of_two().max(256);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            *slot = Some(Self {
                buffer,
                used,
                capacity,
                content_hash: 0,
            });
        } else if let Some(buffer) = slot {
            if buffer.used == used && buffer.content_hash == content_hash {
                return;
            }
            buffer.used = used;
        }

        let Some(buffer) = slot.as_ref() else {
            return;
        };
        queue.write_buffer(&buffer.buffer, 0, bytes);
        if let Some(buffer) = slot {
            buffer.content_hash = content_hash;
        }
    }

    #[inline]
    fn slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(0..self.used)
    }

    #[inline]
    fn ready(&self) -> Option<&Self> {
        (self.used > 0).then_some(self)
    }
}

fn create_linear_sampler(device: &wgpu::Device) -> wgpu::Sampler {
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("eui_neo_linear_sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    })
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = FxHasher::default();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone)]
struct TextItem {
    text: String,
    font: FontRef,
    frame: LayoutRect,
    clip: LayoutRect,
    color: Color,
    font_size: f32,
    font_weight: i32,
    max_width: f32,
    wrap: bool,
    horizontal_align: HorizontalAlign,
    vertical_align: VerticalAlign,
    line_height: f32,
}

#[derive(Clone)]
struct ImageItem {
    image: ImageRef,
}

struct CachedNeoImage {
    _texture: Option<wgpu::Texture>,
    bind_group: wgpu::BindGroup,
    size: [u32; 2],
    uv_rect: [f32; 4],
    revision: u64,
}

/// Borrowed GPU image supplied by a host image provider.
#[derive(Clone, Copy)]
pub struct GpuImage<'a> {
    pub revision: u64,
    pub size: [u32; 2],
    pub uv_rect: [f32; 4],
    pub view: &'a wgpu::TextureView,
    pub sampler: Option<&'a wgpu::Sampler>,
}

/// Borrowed decoded RGBA8 image supplied by a host image provider.
#[derive(Clone, Copy)]
pub struct ImagePixels<'a> {
    pub revision: u64,
    pub size: [u32; 2],
    pub uv_rect: [f32; 4],
    pub rgba: &'a [u8],
}

/// Current state for an image reference requested by the frame.
#[derive(Clone, Copy)]
pub enum ImageState<'a> {
    Gpu(GpuImage<'a>),
    Pixels(ImagePixels<'a>),
    Pending,
    Missing,
    Failed,
}

/// Host-provided resource lookup used by the renderer.
pub trait Resources {
    fn image<'a>(&'a mut self, image: &ImageRef) -> ImageState<'a>;
}

/// Empty resources that report every image as missing.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoResources;

impl Resources for NoResources {
    fn image<'a>(&'a mut self, _image: &ImageRef) -> ImageState<'a> {
        ImageState::Missing
    }
}

struct CachedBackdrop {
    _texture: wgpu::Texture,
    _snapshot: Option<wgpu::Texture>,
    bind_group: wgpu::BindGroup,
}

struct TextLayer {
    renderer: TextRenderer,
    buffers: Vec<Buffer>,
    buffer_keys: Vec<TextBufferKey>,
    area_keys: Vec<TextAreaKey>,
}

impl TextLayer {
    fn new(atlas: &mut TextAtlas, device: &wgpu::Device) -> Self {
        Self {
            renderer: TextRenderer::new(atlas, device, wgpu::MultisampleState::default(), None),
            buffers: Vec::new(),
            buffer_keys: Vec::new(),
            area_keys: Vec::new(),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct TextBufferKey {
    text: String,
    font: FontRef,
    font_size: u32,
    font_weight: i32,
    line_height: u32,
    width: u32,
    height: u32,
    wrap: bool,
    horizontal_align: HorizontalAlign,
}

struct TextBufferMetrics {
    font_size: f32,
    line_height: f32,
    width: f32,
    height: f32,
    align: TextAlign,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct TextAreaKey {
    left_bits: u32,
    top_bits: u32,
    bounds: [i32; 4],
    color_rgba: u32,
}

#[derive(Clone, Copy)]
enum PrimitiveKind {
    Rect,
    Polygon,
    Image { image_index: usize },
}

#[derive(Clone, Copy)]
enum RenderOp {
    Primitive(usize),
    Text { start: usize, count: usize },
}

#[derive(Clone, Copy)]
struct PrimitiveOp {
    kind: PrimitiveKind,
    start: u32,
    count: u32,
    clip: LayoutRect,
    backdrop_frame: LayoutRect,
    backdrop_blur: f32,
}

#[derive(Default)]
struct RenderScratch {
    key: Option<DrawCollectKey>,
    rect_vertices: Vec<NeoRectVertex>,
    polygon_vertices: Vec<NeoPolygonVertex>,
    image_vertices: Vec<NeoImageVertex>,
    primitive_ops: Vec<PrimitiveOp>,
    text_items: Vec<TextItem>,
    image_items: Vec<ImageItem>,
    render_ops: Vec<RenderOp>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct DrawCollectKey {
    draw_ptr: usize,
    draw_len: usize,
    logical_width_bits: u32,
    logical_height_bits: u32,
    surface_is_srgb: bool,
    image_cache_revision: u64,
}

impl RenderScratch {
    fn clear(&mut self) {
        self.key = None;
        self.rect_vertices.clear();
        self.polygon_vertices.clear();
        self.image_vertices.clear();
        self.primitive_ops.clear();
        self.text_items.clear();
        self.image_items.clear();
        self.render_ops.clear();
    }
}

/// Renderer for `eui_neo` draw lists.
pub struct WgpuRenderer {
    format: wgpu::TextureFormat,
    wgpu_resources: NeoWgpuResources,
    dummy_backdrop: CachedBackdrop,
    sampler_linear: wgpu::Sampler,
    image_cache: FxHashMap<ImageRef, CachedNeoImage>,
    image_cache_revision: u64,
    font_system: FontSystem,
    font_revisions: FxHashMap<FontRef, u64>,
    registered_fonts: FxHashMap<FontRef, RegisteredFont>,
    swash_cache: SwashCache,
    _glyph_cache: Cache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_layers: Vec<TextLayer>,
    rect_vertex_buffer: Option<WgpuVertexBuffer>,
    polygon_vertex_buffer: Option<WgpuVertexBuffer>,
    image_vertex_buffer: Option<WgpuVertexBuffer>,
    scratch: RenderScratch,
    frames_since_atlas_trim: u32,
    default_text_family: Option<String>,
    default_icon_family: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStatus {
    pub pending_images: bool,
    pub pending_fonts: bool,
}

impl std::fmt::Debug for WgpuRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WgpuRenderer")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl WgpuRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let wgpu_resources = NeoWgpuResources::new(device, format);
        let sampler_linear = create_linear_sampler(device);
        let dummy_backdrop = create_dummy_backdrop(
            device,
            queue,
            format,
            &sampler_linear,
            &wgpu_resources.backdrop_texture_layout,
        );

        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let glyph_cache = Cache::new(device);
        let viewport = Viewport::new(device, &glyph_cache);
        let atlas = TextAtlas::new(device, queue, &glyph_cache, format);

        Self {
            format,
            wgpu_resources,
            dummy_backdrop,
            sampler_linear,
            image_cache: FxHashMap::default(),
            image_cache_revision: 0,
            font_system,
            font_revisions: FxHashMap::default(),
            registered_fonts: FxHashMap::default(),
            swash_cache,
            _glyph_cache: glyph_cache,
            viewport,
            atlas,
            text_layers: Vec::new(),
            rect_vertex_buffer: None,
            polygon_vertex_buffer: None,
            image_vertex_buffer: None,
            scratch: RenderScratch::default(),
            frames_since_atlas_trim: 0,
            default_text_family: None,
            default_icon_family: None,
        }
    }

    pub fn register_font(&mut self, font: &FontRef, bytes: &[u8], revision: u64) {
        if self
            .font_revisions
            .get(font)
            .is_some_and(|current| *current == revision)
        {
            return;
        }
        if register_font_bytes(
            &mut self.font_system,
            &mut self.registered_fonts,
            &mut self.default_text_family,
            &mut self.default_icon_family,
            font,
            bytes,
        ) {
            self.font_revisions.insert(font.clone(), revision);
            self.invalidate_text_buffers();
        }
    }

    pub fn clear_font(&mut self, font: &FontRef) {
        self.font_revisions.remove(font);
        clear_registered_font(
            &mut self.registered_fonts,
            &mut self.default_text_family,
            &mut self.default_icon_family,
            font,
        );
        self.invalidate_text_buffers();
    }

    pub fn matches_format(&self, format: wgpu::TextureFormat) -> bool {
        self.format == format
    }

    fn invalidate_text_buffers(&mut self) {
        for layer in &mut self.text_layers {
            layer.buffers.clear();
            layer.buffer_keys.clear();
            layer.area_keys.clear();
        }
    }

    pub fn render(
        &mut self,
        target: &mut Target<'_>,
        frame: &Frame,
        resources: &mut dyn Resources,
    ) -> RenderStatus {
        self.render_draw_list(target, frame.draw_list(), frame.screen, resources)
    }

    fn render_draw_list(
        &mut self,
        ctx: &mut Target<'_>,
        draw_list: &UiDrawList,
        screen: Screen,
        resources: &mut dyn Resources,
    ) -> RenderStatus {
        debug_assert_eq!(
            ctx.format, self.format,
            "WgpuRenderer format must match Target format"
        );
        if draw_list.is_empty() || screen.width <= 0.0 || screen.height <= 0.0 {
            return RenderStatus::default();
        }

        let profile = neo_profile_enabled();
        let image_start = profile.then(Instant::now);
        let pending_images = self.prepare_images(ctx.device, ctx.queue, draw_list, resources);
        if profile {
            eprintln!(
                "[eui-neo-wgpu] image_prepare={:.3}ms commands={}",
                elapsed_ms(image_start),
                draw_list.commands().len()
            );
        }
        self.render_prepared(ctx, draw_list, screen, pending_images)
    }

    fn render_prepared(
        &mut self,
        ctx: &mut Target<'_>,
        draw_list: &UiDrawList,
        screen: Screen,
        pending_images: bool,
    ) -> RenderStatus {
        let profile = neo_profile_enabled();
        let total_start = profile.then(Instant::now);
        let logical_rect = LayoutRect::new(0.0, 0.0, screen.width, screen.height);
        let mut scratch = std::mem::take(&mut self.scratch);
        let surface_is_srgb = self.format.is_srgb();
        let collect_start = profile.then(Instant::now);
        let (draw_ptr, draw_len) = draw_list.cache_key();
        let collect_key = DrawCollectKey {
            draw_ptr,
            draw_len,
            logical_width_bits: screen.width.to_bits(),
            logical_height_bits: screen.height.to_bits(),
            surface_is_srgb,
            image_cache_revision: self.image_cache_revision,
        };
        let collect_hit = scratch.key == Some(collect_key);
        if !collect_hit {
            scratch.clear();
            collect_draw_items(
                draw_list,
                logical_rect,
                surface_is_srgb,
                &self.image_cache,
                &mut scratch.rect_vertices,
                &mut scratch.polygon_vertices,
                &mut scratch.image_vertices,
                &mut scratch.primitive_ops,
                &mut scratch.text_items,
                &mut scratch.image_items,
                &mut scratch.render_ops,
            );
            scratch.key = Some(collect_key);
        }
        let collect_ms = elapsed_ms(collect_start);

        if scratch.rect_vertices.is_empty()
            && scratch.polygon_vertices.is_empty()
            && scratch.image_vertices.is_empty()
            && scratch.text_items.is_empty()
        {
            self.scratch = scratch;
            return RenderStatus {
                pending_images,
                ..RenderStatus::default()
            };
        }

        let screen_uniform = ScreenUniform {
            size: [
                screen.width,
                screen.height,
                ctx.physical_size[0].max(1) as f32,
                ctx.physical_size[1].max(1) as f32,
            ],
            backdrop_size: [0.0, 0.0, 0.0, 0.0],
            backdrop_rect: [0.0, 0.0, 0.0, 0.0],
        };
        ctx.queue.write_buffer(
            &self.wgpu_resources.screen_buffer,
            0,
            bytemuck::bytes_of(&screen_uniform),
        );
        let upload_start = profile.then(Instant::now);
        WgpuVertexBuffer::upload(
            &mut self.rect_vertex_buffer,
            ctx.device,
            ctx.queue,
            "eui_neo_rect_vertices",
            &scratch.rect_vertices,
        );
        WgpuVertexBuffer::upload(
            &mut self.polygon_vertex_buffer,
            ctx.device,
            ctx.queue,
            "eui_neo_polygon_vertices",
            &scratch.polygon_vertices,
        );
        WgpuVertexBuffer::upload(
            &mut self.image_vertex_buffer,
            ctx.device,
            ctx.queue,
            "eui_neo_image_vertices",
            &scratch.image_vertices,
        );
        let upload_ms = elapsed_ms(upload_start);

        let text_start = profile.then(Instant::now);
        self.prepare_text_layers(
            ctx,
            &scratch.render_ops,
            &scratch.text_items,
            [screen.width, screen.height],
        );
        let text_ms = elapsed_ms(text_start);

        let rect_upload = self
            .rect_vertex_buffer
            .as_ref()
            .and_then(WgpuVertexBuffer::ready);
        let polygon_upload = self
            .polygon_vertex_buffer
            .as_ref()
            .and_then(WgpuVertexBuffer::ready);
        let image_upload = self
            .image_vertex_buffer
            .as_ref()
            .and_then(WgpuVertexBuffer::ready);
        let render_start = profile.then(Instant::now);
        render_ordered_ops(
            self,
            ctx,
            &scratch.primitive_ops,
            &scratch.render_ops,
            rect_upload,
            polygon_upload,
            image_upload,
            &scratch.image_items,
            [screen.width, screen.height],
        );
        let render_ms = elapsed_ms(render_start);

        self.frames_since_atlas_trim = self.frames_since_atlas_trim.saturating_add(1);
        if self.frames_since_atlas_trim >= ATLAS_TRIM_INTERVAL_FRAMES {
            self.atlas.trim();
            self.frames_since_atlas_trim = 0;
            for layer in &mut self.text_layers {
                layer.area_keys.clear();
            }
        }
        if profile {
            eprintln!(
                "[eui-neo-wgpu] render total={:.3}ms collect={:.3}ms upload={:.3}ms text={:.3}ms render_ops={:.3}ms collect_hit={} rect_v={} poly_v={} image_v={} text_items={} ops={}",
                elapsed_ms(total_start),
                collect_ms,
                upload_ms,
                text_ms,
                render_ms,
                collect_hit,
                scratch.rect_vertices.len(),
                scratch.polygon_vertices.len(),
                scratch.image_vertices.len(),
                scratch.text_items.len(),
                scratch.render_ops.len()
            );
        }
        self.scratch = scratch;
        RenderStatus {
            pending_images,
            ..RenderStatus::default()
        }
    }

    fn prepare_images(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        draw_list: &UiDrawList,
        resources: &mut dyn Resources,
    ) -> bool {
        let mut pending = false;
        for command in draw_list.commands() {
            let key = match command {
                UiDrawCommand::Image(draw) => &draw.image,
                UiDrawCommand::NineSlice(draw) => &draw.image,
                _ => continue,
            };
            if key.is_empty() {
                continue;
            }
            match resources.image(key) {
                ImageState::Gpu(image) => {
                    self.cache_gpu_image(device, key, image);
                }
                ImageState::Pixels(pixels) => {
                    self.cache_pixel_image(device, queue, key, pixels);
                }
                ImageState::Pending => {
                    pending = true;
                }
                ImageState::Missing => {}
                ImageState::Failed => {}
            }
        }
        pending
    }

    fn cache_gpu_image(&mut self, device: &wgpu::Device, key: &ImageRef, image: GpuImage<'_>) {
        if image.size[0] == 0 || image.size[1] == 0 {
            return;
        }
        if self.image_cache.get(key).is_some_and(|cached| {
            cached._texture.is_none()
                && cached.revision == image.revision
                && cached.size == image.size
                && cached.uv_rect == image.uv_rect
        }) {
            return;
        }

        let sampler = image.sampler.unwrap_or(&self.sampler_linear);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("eui_neo_provider_image_bg"),
            layout: &self.wgpu_resources.image_texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(image.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        self.image_cache.insert(
            key.clone(),
            CachedNeoImage {
                _texture: None,
                bind_group,
                size: image.size,
                uv_rect: image.uv_rect,
                revision: image.revision,
            },
        );
        self.image_cache_revision = self.image_cache_revision.wrapping_add(1);
    }

    fn cache_pixel_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &ImageRef,
        pixels: ImagePixels<'_>,
    ) {
        if pixels.size[0] == 0 || pixels.size[1] == 0 {
            return;
        }
        let expected_len = pixels.size[0] as usize * pixels.size[1] as usize * 4;
        if pixels.rgba.len() < expected_len {
            return;
        }
        if self.image_cache.get(key).is_some_and(|cached| {
            cached._texture.is_some()
                && cached.revision == pixels.revision
                && cached.size == pixels.size
                && cached.uv_rect == pixels.uv_rect
        }) {
            return;
        }

        let cached = upload_image_pixels(
            device,
            queue,
            &self.sampler_linear,
            &self.wgpu_resources.image_texture_layout,
            pixels,
        );
        self.image_cache.insert(key.clone(), cached);
        self.image_cache_revision = self.image_cache_revision.wrapping_add(1);
    }

    fn prepare_text_layers(
        &mut self,
        ctx: &Target<'_>,
        render_ops: &[RenderOp],
        text_items: &[TextItem],
        logical_size: [f32; 2],
    ) {
        let physical_size = ctx.physical_size;
        self.viewport.update(
            ctx.queue,
            Resolution {
                width: physical_size[0].max(1),
                height: physical_size[1].max(1),
            },
        );

        let mut layer_index = 0usize;
        for op in render_ops {
            let RenderOp::Text { start, count } = *op else {
                continue;
            };
            while self.text_layers.len() <= layer_index {
                self.text_layers
                    .push(TextLayer::new(&mut self.atlas, ctx.device));
            }
            let end = start.saturating_add(count).min(text_items.len());
            let layer_items = if start < end {
                &text_items[start..end]
            } else {
                &[]
            };
            self.prepare_text_layer(ctx, layer_index, layer_items, logical_size, physical_size);
            layer_index += 1;
        }
    }

    fn prepare_text_layer(
        &mut self,
        ctx: &Target<'_>,
        layer_index: usize,
        text_items: &[TextItem],
        logical_size: [f32; 2],
        physical_size: [u32; 2],
    ) {
        let scale_x = physical_size[0] as f32 / logical_size[0].max(1.0);
        let scale_y = physical_size[1] as f32 / logical_size[1].max(1.0);
        let text_scale = scale_x.min(scale_y).max(0.01);
        let default_text_family = self.default_text_family.clone();
        let default_icon_family = self.default_icon_family.clone();
        let layer = &mut self.text_layers[layer_index];
        let mut buffers_changed = layer.buffer_keys.len() != text_items.len();

        for (index, item) in text_items.iter().enumerate() {
            let metrics = text_buffer_metrics(item, logical_size, text_scale, scale_x, scale_y);
            let key = text_buffer_key(item, &metrics);
            if layer.buffer_keys.get(index) == Some(&key) {
                continue;
            }
            buffers_changed = true;

            let buffer = create_text_buffer(
                &mut self.font_system,
                &self.registered_fonts,
                default_text_family.as_deref(),
                default_icon_family.as_deref(),
                item,
                &metrics,
            );
            if index < layer.buffers.len() {
                layer.buffers[index] = buffer;
                layer.buffer_keys[index] = key;
            } else {
                layer.buffers.push(buffer);
                layer.buffer_keys.push(key);
            }
        }
        layer.buffers.truncate(text_items.len());
        layer.buffer_keys.truncate(text_items.len());

        let mut area_keys = Vec::with_capacity(text_items.len());
        let mut areas = Vec::with_capacity(text_items.len());
        for (buffer, item) in layer.buffers.iter().zip(text_items.iter()) {
            let text_height =
                laid_out_text_height(buffer).unwrap_or(item.font_size * text_scale * 1.25);
            let rect_height = item.frame.height * scale_y;
            let y_offset = match item.vertical_align {
                VerticalAlign::Top => 0.0,
                VerticalAlign::Center => ((rect_height - text_height) * 0.5).max(0.0),
                VerticalAlign::Bottom => (rect_height - text_height).max(0.0),
            };
            let left = item.frame.x * scale_x;
            let top = item.frame.y * scale_y + y_offset;
            let bounds = TextBounds {
                left: (item.clip.x * scale_x).round() as i32,
                top: (item.clip.y * scale_y).round() as i32,
                right: (item.clip.right() * scale_x).round() as i32,
                bottom: (item.clip.bottom() * scale_y).round() as i32,
            };
            let color = multiply_alpha(item.color, 1.0);
            area_keys.push(TextAreaKey {
                left_bits: left.to_bits(),
                top_bits: top.to_bits(),
                bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom],
                color_rgba: packed_color(color),
            });
            areas.push(TextArea {
                buffer,
                left,
                top,
                scale: 1.0,
                bounds,
                default_color: glyph_color(color),
                custom_glyphs: &[],
            });
        }

        if !buffers_changed && layer.area_keys == area_keys {
            return;
        }

        match layer.renderer.prepare(
            ctx.device,
            ctx.queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        ) {
            Ok(()) => {
                layer.area_keys = area_keys;
            }
            Err(error) => {
                layer.area_keys.clear();
                eprintln!("[eui-neo-wgpu] text prepare failed: {error}");
            }
        }
    }
}

fn text_buffer_metrics(
    item: &TextItem,
    logical_size: [f32; 2],
    text_scale: f32,
    scale_x: f32,
    scale_y: f32,
) -> TextBufferMetrics {
    let icon_font = is_icon_font(&item.font);
    let authored_font_size = (item.font_size * text_scale).max(1.0);
    let font_size = if icon_font {
        authored_font_size
    } else {
        authored_font_size * DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE
    }
    .max(1.0);
    let metrics_scale = if icon_font {
        1.0
    } else {
        DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE
    };
    let line_height = if item.line_height > 0.0 {
        item.line_height * text_scale * metrics_scale
    } else {
        authored_font_size * 1.2 * metrics_scale
    };
    let width = if item.frame.width > 0.0 {
        if item.wrap && item.max_width > 0.0 {
            item.max_width.min(item.frame.width)
        } else {
            item.frame.width
        }
    } else {
        logical_size[0]
    };

    TextBufferMetrics {
        font_size,
        line_height,
        width: (width * scale_x).max(1.0),
        height: (item.frame.height * scale_y).max(1.0),
        align: text_align(item.horizontal_align),
    }
}

fn text_buffer_key(item: &TextItem, metrics: &TextBufferMetrics) -> TextBufferKey {
    TextBufferKey {
        text: item.text.clone(),
        font: item.font.clone(),
        font_size: metrics.font_size.to_bits(),
        font_weight: item.font_weight,
        line_height: metrics.line_height.to_bits(),
        width: metrics.width.to_bits(),
        height: metrics.height.to_bits(),
        wrap: item.wrap,
        horizontal_align: item.horizontal_align,
    }
}

fn create_text_buffer(
    font_system: &mut FontSystem,
    registered_fonts: &FxHashMap<FontRef, RegisteredFont>,
    default_text_family: Option<&str>,
    default_icon_family: Option<&str>,
    item: &TextItem,
    metrics: &TextBufferMetrics,
) -> Buffer {
    let mut buffer = Buffer::new(
        font_system,
        Metrics::new(metrics.font_size, metrics.line_height),
    );
    buffer.set_size(font_system, Some(metrics.width), Some(metrics.height));
    buffer.set_wrap(
        font_system,
        if item.wrap {
            Wrap::WordOrGlyph
        } else {
            Wrap::None
        },
    );
    let attrs = Attrs::new()
        .family(resolve_family(
            &item.font,
            registered_fonts,
            default_text_family,
            default_icon_family,
        ))
        .weight(Weight(resolved_font_weight(&item.font, item.font_weight)));
    buffer.set_text(
        font_system,
        &item.text,
        &attrs,
        Shaping::Advanced,
        Some(metrics.align),
    );
    for line in &mut buffer.lines {
        line.set_align(Some(metrics.align));
    }
    buffer.shape_until_scroll(font_system, false);
    buffer
}

#[allow(clippy::too_many_arguments)]
fn render_ordered_ops(
    renderer: &WgpuRenderer,
    ctx: &mut Target<'_>,
    primitive_ops: &[PrimitiveOp],
    render_ops: &[RenderOp],
    rect_upload: Option<&WgpuVertexBuffer>,
    polygon_upload: Option<&WgpuVertexBuffer>,
    image_upload: Option<&WgpuVertexBuffer>,
    image_items: &[ImageItem],
    logical_size: [f32; 2],
) {
    let mut text_layer_index = 0usize;
    let mut segment_start = 0usize;

    for (index, render_op) in render_ops.iter().copied().enumerate() {
        let Some(primitive_index) = backdrop_blur_primitive_index(primitive_ops, render_op) else {
            continue;
        };

        if segment_start < index {
            render_mixed_ops(
                renderer,
                ctx,
                primitive_ops,
                &render_ops[segment_start..index],
                rect_upload,
                polygon_upload,
                image_upload,
                image_items,
                logical_size,
                &mut text_layer_index,
            );
        }

        let op = primitive_ops[primitive_index];
        let backdrop = capture_backdrop(renderer, ctx, op, logical_size);
        render_primitive_ops_batch(
            renderer,
            ctx,
            primitive_ops,
            &render_ops[index..index + 1],
            rect_upload,
            polygon_upload,
            image_upload,
            image_items,
            logical_size,
            backdrop.as_ref().map(|backdrop| &backdrop.bind_group),
        );
        segment_start = index + 1;
    }

    if segment_start < render_ops.len() {
        render_mixed_ops(
            renderer,
            ctx,
            primitive_ops,
            &render_ops[segment_start..],
            rect_upload,
            polygon_upload,
            image_upload,
            image_items,
            logical_size,
            &mut text_layer_index,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_mixed_ops(
    renderer: &WgpuRenderer,
    ctx: &mut Target<'_>,
    primitive_ops: &[PrimitiveOp],
    render_ops: &[RenderOp],
    rect_upload: Option<&WgpuVertexBuffer>,
    polygon_upload: Option<&WgpuVertexBuffer>,
    image_upload: Option<&WgpuVertexBuffer>,
    image_items: &[ImageItem],
    logical_size: [f32; 2],
    text_layer_index: &mut usize,
) {
    if render_ops.is_empty() {
        return;
    }

    let physical_size = ctx.physical_size;
    let color_attachment = Some(wgpu::RenderPassColorAttachment {
        view: ctx.view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Load,
            store: wgpu::StoreOp::Store,
        },
    });
    let color_attachments = [color_attachment];
    let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("eui_neo_overlay_mixed"),
        color_attachments: &color_attachments,
        depth_stencil_attachment: None,
        ..Default::default()
    });
    let mut active_kind: Option<PrimitiveKind> = None;
    let rect_backdrop_bind_group = &renderer.dummy_backdrop.bind_group;

    for render_op in render_ops {
        match *render_op {
            RenderOp::Primitive(index) => {
                let Some(op) = primitive_ops.get(index) else {
                    continue;
                };
                let Some((x, y, width, height)) =
                    scissor_rect(op.clip, logical_size, physical_size)
                else {
                    continue;
                };
                pass.set_scissor_rect(x, y, width, height);
                match op.kind {
                    PrimitiveKind::Rect => {
                        let Some(upload) = rect_upload else {
                            continue;
                        };
                        if !matches!(active_kind, Some(PrimitiveKind::Rect)) {
                            pass.set_pipeline(&renderer.wgpu_resources.rect_pipeline);
                            pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
                            pass.set_bind_group(1, rect_backdrop_bind_group, &[]);
                            pass.set_vertex_buffer(0, upload.slice());
                            active_kind = Some(op.kind);
                        }
                    }
                    PrimitiveKind::Polygon => {
                        let Some(upload) = polygon_upload else {
                            continue;
                        };
                        if !matches!(active_kind, Some(PrimitiveKind::Polygon)) {
                            pass.set_pipeline(&renderer.wgpu_resources.polygon_pipeline);
                            pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
                            pass.set_vertex_buffer(0, upload.slice());
                            active_kind = Some(PrimitiveKind::Polygon);
                        }
                    }
                    PrimitiveKind::Image { image_index } => {
                        let (Some(upload), Some(image_item)) =
                            (image_upload, image_items.get(image_index))
                        else {
                            continue;
                        };
                        let Some(cached) = renderer.image_cache.get(&image_item.image) else {
                            continue;
                        };
                        if !matches!(active_kind, Some(PrimitiveKind::Image { .. })) {
                            pass.set_pipeline(&renderer.wgpu_resources.image_pipeline);
                            pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
                            pass.set_vertex_buffer(0, upload.slice());
                            active_kind = Some(PrimitiveKind::Image { image_index });
                        }
                        pass.set_bind_group(1, &cached.bind_group, &[]);
                    }
                }
                pass.draw(op.start..op.start + op.count, 0..1);
            }
            RenderOp::Text { .. } => {
                active_kind = None;
                pass.set_scissor_rect(0, 0, physical_size[0].max(1), physical_size[1].max(1));
                if let Some(layer) = renderer.text_layers.get(*text_layer_index) {
                    if let Err(error) =
                        layer
                            .renderer
                            .render(&renderer.atlas, &renderer.viewport, &mut pass)
                    {
                        eprintln!("[eui-neo-wgpu] text render failed: {error}");
                    }
                }
                *text_layer_index += 1;
            }
        }
    }
}

fn backdrop_blur_primitive_index(
    primitive_ops: &[PrimitiveOp],
    render_op: RenderOp,
) -> Option<usize> {
    let RenderOp::Primitive(index) = render_op else {
        return None;
    };
    primitive_ops
        .get(index)
        .is_some_and(|op| matches!(op.kind, PrimitiveKind::Rect) && op.backdrop_blur > 0.0)
        .then_some(index)
}

#[allow(clippy::too_many_arguments)]
fn render_primitive_ops_batch(
    renderer: &WgpuRenderer,
    ctx: &mut Target<'_>,
    primitive_ops: &[PrimitiveOp],
    render_ops: &[RenderOp],
    rect_upload: Option<&WgpuVertexBuffer>,
    polygon_upload: Option<&WgpuVertexBuffer>,
    image_upload: Option<&WgpuVertexBuffer>,
    image_items: &[ImageItem],
    logical_size: [f32; 2],
    backdrop_bind_group: Option<&wgpu::BindGroup>,
) {
    if render_ops.is_empty() {
        return;
    }
    let physical_size = ctx.physical_size;
    let color_attachment = Some(wgpu::RenderPassColorAttachment {
        view: ctx.view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Load,
            store: wgpu::StoreOp::Store,
        },
    });
    let color_attachments = [color_attachment];
    let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("eui_neo_overlay"),
        color_attachments: &color_attachments,
        depth_stencil_attachment: None,
        ..Default::default()
    });
    let mut active_kind: Option<PrimitiveKind> = None;
    let rect_backdrop_bind_group =
        backdrop_bind_group.unwrap_or(&renderer.dummy_backdrop.bind_group);
    for op in render_ops {
        let RenderOp::Primitive(index) = *op else {
            continue;
        };
        let Some(op) = primitive_ops.get(index) else {
            continue;
        };
        let Some((x, y, width, height)) = scissor_rect(op.clip, logical_size, physical_size) else {
            continue;
        };
        pass.set_scissor_rect(x, y, width, height);
        match op.kind {
            PrimitiveKind::Rect => {
                let Some(upload) = rect_upload else {
                    continue;
                };
                if !matches!(active_kind, Some(PrimitiveKind::Rect)) {
                    pass.set_pipeline(&renderer.wgpu_resources.rect_pipeline);
                    pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
                    pass.set_bind_group(1, rect_backdrop_bind_group, &[]);
                    pass.set_vertex_buffer(0, upload.slice());
                    active_kind = Some(op.kind);
                }
            }
            PrimitiveKind::Polygon => {
                let Some(upload) = polygon_upload else {
                    continue;
                };
                if !matches!(active_kind, Some(PrimitiveKind::Polygon)) {
                    pass.set_pipeline(&renderer.wgpu_resources.polygon_pipeline);
                    pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
                    pass.set_vertex_buffer(0, upload.slice());
                    active_kind = Some(PrimitiveKind::Polygon);
                }
            }
            PrimitiveKind::Image { image_index } => {
                let (Some(upload), Some(image_item)) = (image_upload, image_items.get(image_index))
                else {
                    continue;
                };
                let Some(cached) = renderer.image_cache.get(&image_item.image) else {
                    continue;
                };
                if !matches!(active_kind, Some(PrimitiveKind::Image { .. })) {
                    pass.set_pipeline(&renderer.wgpu_resources.image_pipeline);
                    pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
                    pass.set_vertex_buffer(0, upload.slice());
                    active_kind = Some(PrimitiveKind::Image { image_index });
                }
                pass.set_bind_group(1, &cached.bind_group, &[]);
            }
        }
        pass.draw(op.start..op.start + op.count, 0..1);
    }
}

fn upload_image_pixels(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    sampler: &wgpu::Sampler,
    image_texture_layout: &wgpu::BindGroupLayout,
    pixels: ImagePixels<'_>,
) -> CachedNeoImage {
    let [width, height] = pixels.size;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("eui_neo_image_texture"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixels.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width.max(1)),
            rows_per_image: Some(height.max(1)),
        },
        wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("eui_neo_image_texture_bg"),
        layout: image_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    CachedNeoImage {
        _texture: Some(texture),
        bind_group,
        size: [width.max(1), height.max(1)],
        uv_rect: pixels.uv_rect,
        revision: pixels.revision,
    }
}

fn create_dummy_backdrop(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    sampler: &wgpu::Sampler,
    backdrop_texture_layout: &wgpu::BindGroupLayout,
) -> CachedBackdrop {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("eui_neo_dummy_backdrop_texture"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[0, 0, 0, 0],
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("eui_neo_dummy_backdrop_bg"),
        layout: backdrop_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    CachedBackdrop {
        _texture: texture,
        _snapshot: None,
        bind_group,
    }
}

fn capture_backdrop(
    renderer: &WgpuRenderer,
    ctx: &mut Target<'_>,
    op: PrimitiveOp,
    logical_size: [f32; 2],
) -> Option<CachedBackdrop> {
    let source = ctx.target_texture?;
    let [surface_width, surface_height] = ctx.physical_size;
    let capture_rect = backdrop_capture_rect(op, logical_size, [surface_width, surface_height])?;
    let texture_size = [
        ((capture_rect[2] as f32) * 0.5).ceil().max(1.0) as u32,
        ((capture_rect[3] as f32) * 0.5).ceil().max(1.0) as u32,
    ];

    let snapshot = ctx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("eui_neo_backdrop_snapshot_texture"),
        size: wgpu::Extent3d {
            width: surface_width.max(1),
            height: surface_height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ctx.format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    ctx.encoder.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo {
            texture: source.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyTextureInfo {
            texture: &snapshot,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::Extent3d {
            width: surface_width.max(1),
            height: surface_height.max(1),
            depth_or_array_layers: 1,
        },
    );

    let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("eui_neo_backdrop_texture"),
        size: wgpu::Extent3d {
            width: texture_size[0],
            height: texture_size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: ctx.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let snapshot_view = snapshot.create_view(&wgpu::TextureViewDescriptor::default());
    let capture_bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("eui_neo_backdrop_capture_bg"),
        layout: &renderer.wgpu_resources.image_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&snapshot_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&renderer.sampler_linear),
            },
        ],
    });
    write_backdrop_uniform(
        ctx.queue,
        &renderer.wgpu_resources.screen_buffer,
        logical_size,
        [surface_width, surface_height],
        capture_rect,
        texture_size,
    );
    {
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let color_attachment = Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        });
        let color_attachments = [color_attachment];
        let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("eui_neo_backdrop_capture"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(&renderer.wgpu_resources.capture_pipeline);
        pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
        pass.set_bind_group(1, &capture_bind_group, &[]);
        draw_fullscreen_triangle(&mut pass);
    }

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("eui_neo_backdrop_bg"),
        layout: &renderer.wgpu_resources.backdrop_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&renderer.sampler_linear),
            },
        ],
    });
    Some(CachedBackdrop {
        _texture: texture,
        _snapshot: Some(snapshot),
        bind_group,
    })
}

fn backdrop_capture_rect(
    op: PrimitiveOp,
    logical_size: [f32; 2],
    physical_size: [u32; 2],
) -> Option<[f32; 4]> {
    let scale_x = physical_size[0].max(1) as f32 / logical_size[0].max(1.0);
    let scale_y = physical_size[1].max(1) as f32 / logical_size[1].max(1.0);
    let blur = op.backdrop_blur.max(0.0) * scale_x.max(scale_y).max(1.0);
    let frame = op.backdrop_frame;
    let left = ((frame.x * scale_x) - blur)
        .floor()
        .clamp(0.0, physical_size[0].saturating_sub(1).max(1) as f32);
    let top = ((frame.y * scale_y) - blur)
        .floor()
        .clamp(0.0, physical_size[1].saturating_sub(1).max(1) as f32);
    let right = ((frame.right() * scale_x) + blur)
        .ceil()
        .clamp(left + 1.0, physical_size[0].max(1) as f32);
    let bottom = ((frame.bottom() * scale_y) + blur)
        .ceil()
        .clamp(top + 1.0, physical_size[1].max(1) as f32);
    let width = (right - left).max(1.0);
    let height = (bottom - top).max(1.0);
    (width > 0.0 && height > 0.0).then_some([left, top, width, height])
}

fn write_backdrop_uniform(
    queue: &wgpu::Queue,
    screen_buffer: &wgpu::Buffer,
    logical_size: [f32; 2],
    physical_size: [u32; 2],
    capture_rect: [f32; 4],
    texture_size: [u32; 2],
) {
    let screen_uniform = ScreenUniform {
        size: [
            logical_size[0],
            logical_size[1],
            physical_size[0].max(1) as f32,
            physical_size[1].max(1) as f32,
        ],
        backdrop_size: [
            texture_size[0].max(1) as f32,
            texture_size[1].max(1) as f32,
            0.0,
            0.0,
        ],
        backdrop_rect: capture_rect,
    };
    queue.write_buffer(screen_buffer, 0, bytemuck::bytes_of(&screen_uniform));
}

fn collect_draw_items(
    draw_list: &UiDrawList,
    full_clip: LayoutRect,
    surface_is_srgb: bool,
    image_cache: &FxHashMap<ImageRef, CachedNeoImage>,
    rect_vertices: &mut Vec<NeoRectVertex>,
    polygon_vertices: &mut Vec<NeoPolygonVertex>,
    image_vertices: &mut Vec<NeoImageVertex>,
    primitive_ops: &mut Vec<PrimitiveOp>,
    text_items: &mut Vec<TextItem>,
    image_items: &mut Vec<ImageItem>,
    render_ops: &mut Vec<RenderOp>,
) {
    let mut clip = UiClip::rect(full_clip);
    let mut stack = Vec::new();

    for command in draw_list.commands() {
        match command {
            UiDrawCommand::PushClip(next) => {
                stack.push(clip);
                clip =
                    intersect_clip(clip, *next).unwrap_or_else(|| UiClip::rect(LayoutRect::ZERO));
            }
            UiDrawCommand::PopClip => {
                clip = stack.pop().unwrap_or_else(|| UiClip::rect(full_clip));
            }
            UiDrawCommand::Rect(draw) => {
                if push_rect(rect_vertices, primitive_ops, draw, clip, surface_is_srgb) {
                    render_ops.push(RenderOp::Primitive(primitive_ops.len() - 1));
                }
            }
            UiDrawCommand::Polygon(draw) => {
                if push_polygon(polygon_vertices, primitive_ops, draw, clip, surface_is_srgb) {
                    render_ops.push(RenderOp::Primitive(primitive_ops.len() - 1));
                }
            }
            UiDrawCommand::Text(draw) => {
                let start = text_items.len();
                if push_text(text_items, draw, clip.rect, surface_is_srgb) {
                    match render_ops.last_mut() {
                        Some(RenderOp::Text { start: _, count })
                            if start == text_items.len() - 1 =>
                        {
                            *count += 1;
                        }
                        _ => render_ops.push(RenderOp::Text { start, count: 1 }),
                    }
                }
            }
            UiDrawCommand::Image(draw) => {
                if push_image(
                    image_vertices,
                    primitive_ops,
                    image_items,
                    draw,
                    clip,
                    image_cache,
                    surface_is_srgb,
                ) {
                    render_ops.push(RenderOp::Primitive(primitive_ops.len() - 1));
                }
            }
            UiDrawCommand::NineSlice(draw) => {
                if push_nine_slice(
                    image_vertices,
                    primitive_ops,
                    image_items,
                    draw,
                    clip,
                    image_cache,
                    surface_is_srgb,
                ) {
                    render_ops.push(RenderOp::Primitive(primitive_ops.len() - 1));
                }
            }
        }
    }
}

fn push_rect(
    vertices: &mut Vec<NeoRectVertex>,
    ops: &mut Vec<PrimitiveOp>,
    draw: &UiRectDraw,
    clip: UiClip,
    surface_is_srgb: bool,
) -> bool {
    if draw.frame.width <= 0.0
        || draw.frame.height <= 0.0
        || draw.opacity <= 0.0
        || clip.rect.width <= 0.0
        || clip.rect.height <= 0.0
    {
        return false;
    }

    let start = vertices.len() as u32;
    if shadow_visible(draw) {
        push_rect_shadow_vertices(vertices, draw, clip, surface_is_srgb);
    }
    if draw.color.a > 0.0 {
        push_rect_fill_vertices(vertices, draw, clip, surface_is_srgb);
    }
    let count = vertices.len() as u32 - start;
    if count == 0 {
        return false;
    }
    ops.push(PrimitiveOp {
        kind: PrimitiveKind::Rect,
        start,
        count,
        clip: clip.rect,
        backdrop_frame: draw.frame,
        backdrop_blur: draw.blur.max(0.0),
    });
    true
}

fn push_rect_fill_vertices(
    vertices: &mut Vec<NeoRectVertex>,
    draw: &UiRectDraw,
    clip: UiClip,
    surface_is_srgb: bool,
) {
    let gradient_direction = match draw.gradient.direction {
        GradientDirection::Horizontal => 0.0,
        GradientDirection::Vertical => 1.0,
    };
    push_rect_vertices(
        vertices,
        draw,
        draw.frame,
        draw.frame,
        output_color(draw.color, surface_is_srgb),
        output_color(draw.gradient.start, surface_is_srgb),
        output_color(draw.gradient.end, surface_is_srgb),
        output_color(draw.border.color, surface_is_srgb),
        clip,
        [
            draw.radius.max(0.0),
            draw.border.width.max(0.0),
            draw.opacity.clamp(0.0, 1.0),
            draw.gradient.enabled as u8 as f32,
        ],
        [gradient_direction, 0.0, draw.blur.max(0.0), 0.0],
    );
}

fn push_rect_shadow_vertices(
    vertices: &mut Vec<NeoRectVertex>,
    draw: &UiRectDraw,
    clip: UiClip,
    surface_is_srgb: bool,
) {
    let blur = draw.shadow.blur.max(1.0);
    let spread = draw.shadow.spread.max(0.0);
    let base_shape = LayoutRect::new(
        draw.frame.x + draw.shadow.offset[0] - spread,
        draw.frame.y + draw.shadow.offset[1] - spread,
        (draw.frame.width + spread * 2.0).max(0.0),
        (draw.frame.height + spread * 2.0).max(0.0),
    );

    let ambient_shape = offset_rect(
        base_shape,
        draw.shadow.offset[0] * 0.15,
        draw.shadow.offset[1] * 0.15,
    );
    push_rect_shadow_layer_vertices(
        vertices,
        draw,
        ambient_shape,
        blur * 1.4,
        0.22,
        clip,
        surface_is_srgb,
    );

    let mid_shape = offset_rect(
        base_shape,
        draw.shadow.offset[0] * 0.65,
        draw.shadow.offset[1] * 0.65,
    );
    push_rect_shadow_layer_vertices(
        vertices,
        draw,
        mid_shape,
        blur * 0.85,
        0.34,
        clip,
        surface_is_srgb,
    );

    push_rect_shadow_layer_vertices(
        vertices,
        draw,
        base_shape,
        blur * 0.38,
        0.26,
        clip,
        surface_is_srgb,
    );
}

fn push_rect_shadow_layer_vertices(
    vertices: &mut Vec<NeoRectVertex>,
    draw: &UiRectDraw,
    shape: LayoutRect,
    blur: f32,
    alpha_scale: f32,
    clip: UiClip,
    surface_is_srgb: bool,
) {
    let geometry = expand_rect(shape, blur);
    let color = output_color(
        multiply_alpha(draw.shadow.color, alpha_scale),
        surface_is_srgb,
    );
    push_rect_vertices(
        vertices,
        draw,
        geometry,
        shape,
        color,
        color,
        color,
        output_color(Color::new(0.0, 0.0, 0.0, 0.0), surface_is_srgb),
        clip,
        [
            draw.radius.max(0.0),
            blur,
            draw.opacity.clamp(0.0, 1.0),
            0.0,
        ],
        [0.0, 1.0, 0.0, 0.0],
    );
}

fn offset_rect(rect: LayoutRect, x: f32, y: f32) -> LayoutRect {
    LayoutRect::new(rect.x + x, rect.y + y, rect.width, rect.height)
}

fn expand_rect(rect: LayoutRect, amount: f32) -> LayoutRect {
    LayoutRect::new(
        rect.x - amount,
        rect.y - amount,
        rect.width + amount * 2.0,
        rect.height + amount * 2.0,
    )
}

#[allow(clippy::too_many_arguments)]
fn push_rect_vertices(
    vertices: &mut Vec<NeoRectVertex>,
    draw: &UiRectDraw,
    geometry: LayoutRect,
    shape: LayoutRect,
    fill: Color,
    gradient_start: Color,
    gradient_end: Color,
    border: Color,
    clip: UiClip,
    params: [f32; 4],
    flags: [f32; 4],
) {
    let x0 = geometry.x;
    let y0 = geometry.y;
    let x1 = geometry.right();
    let y1 = geometry.bottom();
    let rect = [shape.x, shape.y, shape.width, shape.height];
    let fill = fill.to_array();
    let gradient_start = gradient_start.to_array();
    let gradient_end = gradient_end.to_array();
    let border = border.to_array();
    let clip_rect = clip_rect_array(clip);
    let clip_params = clip_params_array(clip);
    let points = [[x0, y0], [x1, y0], [x1, y1], [x0, y0], [x1, y1], [x0, y1]];
    for local in points {
        vertices.push(NeoRectVertex {
            position: transform_point(local, draw.frame, draw.transform),
            local_pos: local,
            rect,
            fill,
            gradient_start,
            gradient_end,
            border,
            params,
            flags,
            clip_rect,
            clip_params,
        });
    }
}

fn shadow_visible(draw: &UiRectDraw) -> bool {
    draw.shadow.enabled
        && draw.shadow.color.a > 0.0
        && draw.opacity > 0.0
        && draw.frame.width > 0.0
        && draw.frame.height > 0.0
        && (draw.shadow.blur > 0.0 || draw.shadow.spread > 0.0 || draw.shadow.offset != [0.0, 0.0])
}

fn push_polygon(
    vertices: &mut Vec<NeoPolygonVertex>,
    ops: &mut Vec<PrimitiveOp>,
    draw: &UiPolygonDraw,
    clip: UiClip,
    surface_is_srgb: bool,
) -> bool {
    if draw.points.len() < 3 || draw.opacity <= 0.0 || draw.color.a <= 0.0 {
        return false;
    }
    if clip.rect.width <= 0.0 || clip.rect.height <= 0.0 {
        return false;
    }

    let start = vertices.len() as u32;
    let mut color = draw.color;
    color.a *= draw.opacity.clamp(0.0, 1.0);
    let color = output_color(color, surface_is_srgb).to_array();
    let clip_rect = clip_rect_array(clip);
    let clip_params = clip_params_array(clip);
    let origin = draw.points[0];
    for index in 1..draw.points.len() - 1 {
        for point in [origin, draw.points[index], draw.points[index + 1]] {
            let absolute = [draw.frame.x + point[0], draw.frame.y + point[1]];
            vertices.push(NeoPolygonVertex {
                position: transform_point(absolute, draw.frame, draw.transform),
                color,
                clip_rect,
                clip_params,
            });
        }
    }
    ops.push(PrimitiveOp {
        kind: PrimitiveKind::Polygon,
        start,
        count: (vertices.len() as u32) - start,
        clip: clip.rect,
        backdrop_frame: LayoutRect::ZERO,
        backdrop_blur: 0.0,
    });
    true
}

fn push_image(
    vertices: &mut Vec<NeoImageVertex>,
    ops: &mut Vec<PrimitiveOp>,
    images: &mut Vec<ImageItem>,
    draw: &UiImageDraw,
    clip: UiClip,
    image_cache: &FxHashMap<ImageRef, CachedNeoImage>,
    surface_is_srgb: bool,
) -> bool {
    if draw.image.is_empty()
        || draw.frame.width <= 0.0
        || draw.frame.height <= 0.0
        || draw.opacity <= 0.0
        || draw.tint.a <= 0.0
        || clip.rect.width <= 0.0
        || clip.rect.height <= 0.0
    {
        return false;
    }

    let Some(image_info) = image_cache.get(&draw.image) else {
        return false;
    };
    let Some((draw_rect, uv_rect)) = image_rect_and_uv(draw.frame, draw.fit, image_info.size)
    else {
        return false;
    };
    let uv_rect = remap_uv_rect(uv_rect, image_info.uv_rect);

    let start = vertices.len() as u32;
    let image_index = images.len();
    let rect = [
        draw.frame.x,
        draw.frame.y,
        draw.frame.width,
        draw.frame.height,
    ];
    let mut tint = draw.tint;
    tint.a *= draw.opacity.clamp(0.0, 1.0);
    let tint = output_color(tint, surface_is_srgb).to_array();
    let params = [draw.radius, draw.opacity.clamp(0.0, 1.0), 0.0, 0.0];
    let clip_rect = clip_rect_array(clip);
    let clip_params = clip_params_array(clip);
    let x0 = draw_rect.x;
    let y0 = draw_rect.y;
    let x1 = draw_rect.right();
    let y1 = draw_rect.bottom();
    let [u0, v0, u1, v1] = uv_rect;
    let points = [
        ([x0, y0], [u0, v0]),
        ([x1, y0], [u1, v0]),
        ([x1, y1], [u1, v1]),
        ([x0, y0], [u0, v0]),
        ([x1, y1], [u1, v1]),
        ([x0, y1], [u0, v1]),
    ];
    for (local, uv) in points {
        vertices.push(NeoImageVertex {
            position: transform_point(local, draw.frame, draw.transform),
            local_pos: local,
            rect,
            uv,
            tint,
            params,
            clip_rect,
            clip_params,
        });
    }
    images.push(ImageItem {
        image: draw.image.clone(),
    });
    ops.push(PrimitiveOp {
        kind: PrimitiveKind::Image { image_index },
        start,
        count: 6,
        clip: clip.rect,
        backdrop_frame: LayoutRect::ZERO,
        backdrop_blur: 0.0,
    });
    true
}

fn push_nine_slice(
    vertices: &mut Vec<NeoImageVertex>,
    ops: &mut Vec<PrimitiveOp>,
    images: &mut Vec<ImageItem>,
    draw: &UiNineSliceDraw,
    clip: UiClip,
    image_cache: &FxHashMap<ImageRef, CachedNeoImage>,
    surface_is_srgb: bool,
) -> bool {
    if draw.image.is_empty()
        || draw.frame.width <= 0.0
        || draw.frame.height <= 0.0
        || draw.opacity <= 0.0
        || draw.tint.a <= 0.0
        || clip.rect.width <= 0.0
        || clip.rect.height <= 0.0
    {
        return false;
    }

    let Some(image_info) = image_cache.get(&draw.image) else {
        return false;
    };
    if image_info.size[0] == 0 || image_info.size[1] == 0 {
        return false;
    }

    let source_w = image_info.size[0] as f32;
    let source_h = image_info.size[1] as f32;
    let left_src = draw.slice.left.min(source_w).max(0.0);
    let right_src = draw
        .slice
        .right
        .min((source_w - left_src).max(0.0))
        .max(0.0);
    let top_src = draw.slice.top.min(source_h).max(0.0);
    let bottom_src = draw
        .slice
        .bottom
        .min((source_h - top_src).max(0.0))
        .max(0.0);

    let left_dst = left_src.min(draw.frame.width * 0.5);
    let right_dst = right_src.min((draw.frame.width - left_dst).max(0.0));
    let top_dst = top_src.min(draw.frame.height * 0.5);
    let bottom_dst = bottom_src.min((draw.frame.height - top_dst).max(0.0));

    let x = [
        draw.frame.x,
        draw.frame.x + left_dst,
        draw.frame.right() - right_dst,
        draw.frame.right(),
    ];
    let y = [
        draw.frame.y,
        draw.frame.y + top_dst,
        draw.frame.bottom() - bottom_dst,
        draw.frame.bottom(),
    ];
    let u = [
        0.0,
        left_src / source_w,
        (source_w - right_src) / source_w,
        1.0,
    ];
    let v = [
        0.0,
        top_src / source_h,
        (source_h - bottom_src) / source_h,
        1.0,
    ];

    let start = vertices.len() as u32;
    let image_index = images.len();
    let rect = [
        draw.frame.x,
        draw.frame.y,
        draw.frame.width,
        draw.frame.height,
    ];
    let mut tint = draw.tint;
    tint.a *= draw.opacity.clamp(0.0, 1.0);
    let tint = output_color(tint, surface_is_srgb).to_array();
    let params = [0.0, draw.opacity.clamp(0.0, 1.0), 0.0, 0.0];
    let clip_rect = clip_rect_array(clip);
    let clip_params = clip_params_array(clip);

    for row in 0..3 {
        for column in 0..3 {
            let quad = LayoutRect::new(
                x[column],
                y[row],
                x[column + 1] - x[column],
                y[row + 1] - y[row],
            );
            if quad.width <= 0.0 || quad.height <= 0.0 {
                continue;
            }
            let uv_rect = remap_uv_rect(
                [u[column], v[row], u[column + 1], v[row + 1]],
                image_info.uv_rect,
            );
            push_image_quad_vertices(
                vertices,
                draw.frame,
                draw.transform,
                quad,
                rect,
                uv_rect,
                tint,
                params,
                clip_rect,
                clip_params,
            );
        }
    }

    let count = vertices.len() as u32 - start;
    if count == 0 {
        return false;
    }
    images.push(ImageItem {
        image: draw.image.clone(),
    });
    ops.push(PrimitiveOp {
        kind: PrimitiveKind::Image { image_index },
        start,
        count,
        clip: clip.rect,
        backdrop_frame: LayoutRect::ZERO,
        backdrop_blur: 0.0,
    });
    true
}

#[allow(clippy::too_many_arguments)]
fn push_image_quad_vertices(
    vertices: &mut Vec<NeoImageVertex>,
    frame: LayoutRect,
    transform: Transform,
    quad: LayoutRect,
    rect: [f32; 4],
    uv_rect: [f32; 4],
    tint: [f32; 4],
    params: [f32; 4],
    clip_rect: [f32; 4],
    clip_params: [f32; 4],
) {
    let x0 = quad.x;
    let y0 = quad.y;
    let x1 = quad.right();
    let y1 = quad.bottom();
    let [u0, v0, u1, v1] = uv_rect;
    let points = [
        ([x0, y0], [u0, v0]),
        ([x1, y0], [u1, v0]),
        ([x1, y1], [u1, v1]),
        ([x0, y0], [u0, v0]),
        ([x1, y1], [u1, v1]),
        ([x0, y1], [u0, v1]),
    ];
    for (local, uv) in points {
        vertices.push(NeoImageVertex {
            position: transform_point(local, frame, transform),
            local_pos: local,
            rect,
            uv,
            tint,
            params,
            clip_rect,
            clip_params,
        });
    }
}

fn clip_rect_array(clip: UiClip) -> [f32; 4] {
    [clip.rect.x, clip.rect.y, clip.rect.width, clip.rect.height]
}

fn clip_params_array(clip: UiClip) -> [f32; 4] {
    [clip.radius.max(0.0), 0.0, 0.0, 0.0]
}

fn remap_uv_rect(local: [f32; 4], visible: [f32; 4]) -> [f32; 4] {
    let [u0, v0, u1, v1] = visible;
    let width = u1 - u0;
    let height = v1 - v0;
    [
        u0 + local[0] * width,
        v0 + local[1] * height,
        u0 + local[2] * width,
        v0 + local[3] * height,
    ]
}

fn image_rect_and_uv(
    frame: LayoutRect,
    fit: ImageFit,
    texture_size: [u32; 2],
) -> Option<(LayoutRect, [f32; 4])> {
    if frame.width <= 0.0 || frame.height <= 0.0 || texture_size[0] == 0 || texture_size[1] == 0 {
        return None;
    }

    let image_aspect = texture_size[0] as f32 / texture_size[1] as f32;
    let rect_aspect = frame.width / frame.height;
    let mut rect = frame;
    let mut uv = [0.0, 0.0, 1.0, 1.0];

    match fit {
        ImageFit::Stretch => {}
        ImageFit::Contain => {
            if image_aspect > rect_aspect {
                rect.height = frame.width / image_aspect;
                rect.y = frame.y + (frame.height - rect.height) * 0.5;
            } else if image_aspect < rect_aspect {
                rect.width = frame.height * image_aspect;
                rect.x = frame.x + (frame.width - rect.width) * 0.5;
            }
        }
        ImageFit::Cover => {
            if image_aspect > rect_aspect {
                let visible = (rect_aspect / image_aspect).clamp(0.0, 1.0);
                uv[0] = (1.0 - visible) * 0.5;
                uv[2] = 1.0 - uv[0];
            } else if image_aspect < rect_aspect {
                let visible = (image_aspect / rect_aspect).clamp(0.0, 1.0);
                uv[1] = (1.0 - visible) * 0.5;
                uv[3] = 1.0 - uv[1];
            }
        }
    }

    Some((rect, uv))
}

fn push_text(
    text_items: &mut Vec<TextItem>,
    draw: &UiTextDraw,
    clip: LayoutRect,
    surface_is_srgb: bool,
) -> bool {
    if draw.text.is_empty()
        || draw.opacity <= 0.0
        || draw.color.a <= 0.0
        || draw.frame.width <= 0.0
        || draw.frame.height <= 0.0
        || clip.width <= 0.0
        || clip.height <= 0.0
    {
        return false;
    }
    let frame = transformed_rect(draw.frame, draw.transform);
    let max_width = if draw.max_width > 0.0 {
        draw.max_width.min(frame.width)
    } else {
        frame.width
    };
    let frame = LayoutRect::new(frame.x, frame.y, max_width, frame.height);
    text_items.push(TextItem {
        text: draw.text.clone(),
        font: draw.font.clone(),
        frame,
        clip,
        color: output_color(multiply_alpha(draw.color, draw.opacity), surface_is_srgb),
        font_size: draw.font_size,
        font_weight: draw.font_weight,
        max_width: draw.max_width,
        wrap: draw.wrap,
        horizontal_align: draw.horizontal_align,
        vertical_align: draw.vertical_align,
        line_height: draw.line_height,
    });
    true
}

fn transformed_rect(rect: LayoutRect, transform: Transform) -> LayoutRect {
    let points = [
        transform_point([rect.x, rect.y], rect, transform),
        transform_point([rect.right(), rect.y], rect, transform),
        transform_point([rect.right(), rect.bottom()], rect, transform),
        transform_point([rect.x, rect.bottom()], rect, transform),
    ];
    let mut min_x = points[0][0];
    let mut min_y = points[0][1];
    let mut max_x = points[0][0];
    let mut max_y = points[0][1];
    for point in &points[1..] {
        min_x = min_x.min(point[0]);
        min_y = min_y.min(point[1]);
        max_x = max_x.max(point[0]);
        max_y = max_y.max(point[1]);
    }
    LayoutRect::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

fn transform_point(point: [f32; 2], frame: LayoutRect, transform: Transform) -> [f32; 2] {
    let origin = [
        frame.x + frame.width * transform.origin[0],
        frame.y + frame.height * transform.origin[1],
    ];
    let scaled_x = (point[0] - origin[0]) * transform.scale[0];
    let scaled_y = (point[1] - origin[1]) * transform.scale[1];
    let cosine = transform.rotation.cos();
    let sine = transform.rotation.sin();
    [
        origin[0] + scaled_x * cosine - scaled_y * sine + transform.translate[0],
        origin[1] + scaled_x * sine + scaled_y * cosine + transform.translate[1],
    ]
}

fn neo_profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SKY_NEO_PROFILE").is_some())
}

fn elapsed_ms(start: Option<Instant>) -> f32 {
    start
        .map(|start| start.elapsed().as_secs_f32() * 1000.0)
        .unwrap_or(0.0)
}

fn scissor_rect(
    rect: LayoutRect,
    logical_size: [f32; 2],
    physical_size: [u32; 2],
) -> Option<(u32, u32, u32, u32)> {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return None;
    }
    let scale_x = physical_size[0] as f32 / logical_size[0].max(1.0);
    let scale_y = physical_size[1] as f32 / logical_size[1].max(1.0);
    let left = (rect.x * scale_x)
        .floor()
        .clamp(0.0, physical_size[0] as f32) as u32;
    let top = (rect.y * scale_y)
        .floor()
        .clamp(0.0, physical_size[1] as f32) as u32;
    let right = (rect.right() * scale_x)
        .ceil()
        .clamp(0.0, physical_size[0] as f32) as u32;
    let bottom = (rect.bottom() * scale_y)
        .ceil()
        .clamp(0.0, physical_size[1] as f32) as u32;
    (right > left && bottom > top).then_some((left, top, right - left, bottom - top))
}

fn intersect_rect(left: LayoutRect, right: LayoutRect) -> Option<LayoutRect> {
    let x0 = left.x.max(right.x);
    let y0 = left.y.max(right.y);
    let x1 = left.right().min(right.right());
    let y1 = left.bottom().min(right.bottom());
    (x1 > x0 && y1 > y0).then(|| LayoutRect::new(x0, y0, x1 - x0, y1 - y0))
}

fn intersect_clip(left: UiClip, right: UiClip) -> Option<UiClip> {
    let rect = intersect_rect(left.rect, right.rect)?;
    let radius = if same_rect(rect, right.rect) {
        right.radius
    } else if same_rect(rect, left.rect) {
        left.radius
    } else {
        left.radius.min(right.radius)
    };
    Some(UiClip::new(rect, radius))
}

fn same_rect(left: LayoutRect, right: LayoutRect) -> bool {
    (left.x - right.x).abs() <= 0.001
        && (left.y - right.y).abs() <= 0.001
        && (left.width - right.width).abs() <= 0.001
        && (left.height - right.height).abs() <= 0.001
}

fn multiply_alpha(mut color: Color, opacity: f32) -> Color {
    color.a *= opacity.clamp(0.0, 1.0);
    color
}

fn output_color(color: Color, surface_is_srgb: bool) -> Color {
    if surface_is_srgb {
        Color::new(
            srgb_to_linear(color.r),
            srgb_to_linear(color.g),
            srgb_to_linear(color.b),
            color.a,
        )
    } else {
        color
    }
}

fn srgb_to_linear(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn glyph_color(color: Color) -> glyphon::Color {
    glyphon::Color::rgba(
        channel(color.r),
        channel(color.g),
        channel(color.b),
        channel(color.a),
    )
}

fn packed_color(color: Color) -> u32 {
    u32::from(channel(color.r)) << 24
        | u32::from(channel(color.g)) << 16
        | u32::from(channel(color.b)) << 8
        | u32::from(channel(color.a))
}

fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn text_align(align: HorizontalAlign) -> TextAlign {
    match align {
        HorizontalAlign::Left => TextAlign::Left,
        HorizontalAlign::Center => TextAlign::Center,
        HorizontalAlign::Right => TextAlign::End,
    }
}

fn laid_out_text_height(buffer: &Buffer) -> Option<f32> {
    buffer
        .layout_runs()
        .map(|run| run.line_top + run.line_height)
        .reduce(f32::max)
}

#[cfg(test)]
mod tests {
    use super::{
        backdrop_blur_primitive_index, backdrop_capture_rect, collect_draw_items,
        image_rect_and_uv, output_color, srgb_to_linear, PrimitiveKind, PrimitiveOp, RenderOp,
    };
    use eui_neo::expert::{UiDrawCommand, UiDrawList, UiRectDraw, UiTextDraw};
    use eui_neo::{
        Border, Color, FontRef, Gradient, HorizontalAlign, ImageFit, LayoutRect, Shadow, Transform,
        VerticalAlign,
    };
    use rustc_hash::FxHashMap;

    #[test]
    fn srgb_output_conversion_keeps_alpha_and_linearizes_rgb() {
        let converted = output_color(Color::new(0.16, 0.18, 0.20, 0.5), true);

        assert!((converted.r - srgb_to_linear(0.16)).abs() < 0.0001);
        assert!((converted.g - srgb_to_linear(0.18)).abs() < 0.0001);
        assert!((converted.b - srgb_to_linear(0.20)).abs() < 0.0001);
        assert_eq!(converted.a, 0.5);
    }

    #[test]
    fn non_srgb_output_leaves_authored_color_unchanged() {
        let color = Color::new(0.16, 0.18, 0.20, 0.5);

        assert_eq!(output_color(color, false).to_array(), color.to_array());
    }

    #[test]
    fn image_contain_preserves_aspect_by_shrinking_draw_rect() {
        let frame = LayoutRect::new(10.0, 20.0, 100.0, 100.0);
        let (rect, uv) = image_rect_and_uv(frame, ImageFit::Contain, [200, 100]).unwrap();

        assert_eq!(rect, LayoutRect::new(10.0, 45.0, 100.0, 50.0));
        assert_eq!(uv, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn image_cover_preserves_bounds_and_crops_uv() {
        let frame = LayoutRect::new(0.0, 0.0, 100.0, 100.0);
        let (rect, uv) = image_rect_and_uv(frame, ImageFit::Cover, [200, 100]).unwrap();

        assert_eq!(rect, frame);
        assert_eq!(uv, [0.25, 0.0, 0.75, 1.0]);
    }

    #[test]
    fn render_ops_keep_text_before_later_panel_rects() {
        let draw_list = UiDrawList::new(vec![
            UiDrawCommand::Rect(rect_draw("page.bg")),
            UiDrawCommand::Text(text_draw("page.text")),
            UiDrawCommand::Rect(rect_draw("dialog.panel")),
        ]);
        let mut rect_vertices = Vec::new();
        let mut polygon_vertices = Vec::new();
        let mut image_vertices = Vec::new();
        let mut primitive_ops = Vec::new();
        let mut text_items = Vec::new();
        let mut image_items = Vec::new();
        let mut render_ops = Vec::new();

        collect_draw_items(
            &draw_list,
            LayoutRect::new(0.0, 0.0, 320.0, 200.0),
            false,
            &FxHashMap::default(),
            &mut rect_vertices,
            &mut polygon_vertices,
            &mut image_vertices,
            &mut primitive_ops,
            &mut text_items,
            &mut image_items,
            &mut render_ops,
        );

        assert!(matches!(render_ops[0], RenderOp::Primitive(0)));
        assert!(matches!(
            render_ops[1],
            RenderOp::Text { start: 0, count: 1 }
        ));
        assert!(matches!(render_ops[2], RenderOp::Primitive(1)));
    }

    #[test]
    fn shadow_rects_emit_soft_three_layer_glow_before_fill() {
        let mut draw = rect_draw("page.glow");
        draw.radius = 7.0;
        draw.shadow = Shadow {
            enabled: true,
            offset: [0.0, 4.0],
            blur: 10.0,
            spread: 3.0,
            color: Color::new(0.2, 0.8, 0.7, 0.5),
        };
        let draw_list = UiDrawList::new(vec![UiDrawCommand::Rect(draw)]);
        let mut rect_vertices = Vec::new();
        let mut polygon_vertices = Vec::new();
        let mut image_vertices = Vec::new();
        let mut primitive_ops = Vec::new();
        let mut text_items = Vec::new();
        let mut image_items = Vec::new();
        let mut render_ops = Vec::new();

        collect_draw_items(
            &draw_list,
            LayoutRect::new(0.0, 0.0, 320.0, 200.0),
            false,
            &FxHashMap::default(),
            &mut rect_vertices,
            &mut polygon_vertices,
            &mut image_vertices,
            &mut primitive_ops,
            &mut text_items,
            &mut image_items,
            &mut render_ops,
        );

        assert_eq!(primitive_ops.len(), 1);
        assert_eq!(primitive_ops[0].count, 24);
        assert_eq!(rect_vertices.len(), 24);
        assert!((rect_vertices[0].fill[3] - 0.11).abs() < 0.001);
        assert!((rect_vertices[6].fill[3] - 0.17).abs() < 0.001);
        assert!((rect_vertices[12].fill[3] - 0.13).abs() < 0.001);
        assert_eq!(rect_vertices[18].fill[3], 1.0);
        assert_eq!(rect_vertices[0].params[0], 7.0);
        assert_eq!(rect_vertices[6].params[0], 7.0);
        assert_eq!(rect_vertices[12].params[0], 7.0);
    }

    #[test]
    fn blurred_rects_mark_backdrop_capture_and_preserve_blur_amount() {
        let mut draw = rect_draw("page.glass");
        draw.color = Color::new(0.8, 0.9, 1.0, 0.36);
        draw.blur = 18.0;
        let frame = draw.frame;
        let draw_list = UiDrawList::new(vec![UiDrawCommand::Rect(draw)]);
        let mut rect_vertices = Vec::new();
        let mut polygon_vertices = Vec::new();
        let mut image_vertices = Vec::new();
        let mut primitive_ops = Vec::new();
        let mut text_items = Vec::new();
        let mut image_items = Vec::new();
        let mut render_ops = Vec::new();

        collect_draw_items(
            &draw_list,
            LayoutRect::new(0.0, 0.0, 320.0, 200.0),
            false,
            &FxHashMap::default(),
            &mut rect_vertices,
            &mut polygon_vertices,
            &mut image_vertices,
            &mut primitive_ops,
            &mut text_items,
            &mut image_items,
            &mut render_ops,
        );

        assert_eq!(primitive_ops.len(), 1);
        assert!(matches!(primitive_ops[0].kind, PrimitiveKind::Rect));
        assert_eq!(primitive_ops[0].backdrop_frame, frame);
        assert_eq!(primitive_ops[0].backdrop_blur, 18.0);
        assert!(rect_vertices.iter().all(|vertex| vertex.flags[2] == 18.0));
    }

    #[test]
    fn backdrop_blur_lookup_uses_primitive_index_after_text_ops() {
        let normal = PrimitiveOp {
            kind: PrimitiveKind::Rect,
            start: 0,
            count: 6,
            clip: LayoutRect::new(0.0, 0.0, 320.0, 200.0),
            backdrop_frame: LayoutRect::new(0.0, 0.0, 100.0, 40.0),
            backdrop_blur: 0.0,
        };
        let blurred = PrimitiveOp {
            backdrop_blur: 18.0,
            ..normal
        };
        let primitive_ops = [normal, blurred];
        let render_ops = [
            RenderOp::Primitive(0),
            RenderOp::Text { start: 0, count: 1 },
            RenderOp::Primitive(1),
        ];

        assert_eq!(
            backdrop_blur_primitive_index(&primitive_ops, render_ops[2]),
            Some(1)
        );
        assert_eq!(
            backdrop_blur_primitive_index(&primitive_ops, render_ops[1]),
            None
        );
    }

    #[test]
    fn backdrop_capture_rect_expands_by_physical_blur_and_half_scales_texture() {
        let op = PrimitiveOp {
            kind: PrimitiveKind::Rect,
            start: 0,
            count: 6,
            clip: LayoutRect::new(0.0, 0.0, 320.0, 200.0),
            backdrop_frame: LayoutRect::new(10.0, 20.0, 100.0, 40.0),
            backdrop_blur: 18.0,
        };

        let capture = backdrop_capture_rect(op, [320.0, 200.0], [640, 400]).unwrap();
        assert_eq!(capture, [0.0, 4.0, 256.0, 152.0]);

        let texture_size = [
            ((capture[2] as f32) * 0.5).ceil().max(1.0) as u32,
            ((capture[3] as f32) * 0.5).ceil().max(1.0) as u32,
        ];
        assert_eq!(texture_size, [128, 76]);
    }

    fn rect_draw(id: &str) -> UiRectDraw {
        UiRectDraw {
            id: id.into(),
            frame: LayoutRect::new(0.0, 0.0, 100.0, 40.0),
            color: Color::WHITE,
            gradient: Gradient::default(),
            border: Border::default(),
            shadow: Shadow::default(),
            radius: 0.0,
            blur: 0.0,
            opacity: 1.0,
            transform: Transform::default(),
        }
    }

    fn text_draw(id: &str) -> UiTextDraw {
        UiTextDraw {
            id: id.into(),
            frame: LayoutRect::new(0.0, 0.0, 100.0, 24.0),
            text: "Text".to_string(),
            font: FontRef::DefaultText,
            font_size: 16.0,
            font_weight: 400,
            color: Color::WHITE,
            max_width: 100.0,
            wrap: false,
            horizontal_align: HorizontalAlign::Left,
            vertical_align: VerticalAlign::Top,
            line_height: 20.0,
            opacity: 1.0,
            transform: Transform::default(),
        }
    }
}
