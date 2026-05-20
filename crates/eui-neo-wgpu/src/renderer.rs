//! wgpu renderer for `eui_neo` draw commands.

use crate::{
    draw_fullscreen_triangle, NeoImageVertex, NeoPolygonVertex, NeoRectVertex, NeoWgpuResources,
    ScreenUniform,
};
use eui_neo::expert::{
    UiDrawCommand, UiDrawList, UiImageDraw, UiPolygonDraw, UiRectDraw, UiTextDraw,
};
use eui_neo::{
    Color, FontRef, Frame, GradientDirection, HorizontalAlign, ImageFit, ImageRef, LayoutRect,
    Screen, Transform, VerticalAlign,
};
use glyphon::cosmic_text::Align as TextAlign;
use glyphon::{
    Attrs, Buffer, Cache, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap,
};
use rustc_hash::FxHashMap;

use crate::fonts::{
    clear_registered_font, is_icon_font, register_font_bytes, resolve_family,
    resolved_font_weight, RegisteredFont, DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE,
};

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
    size: u64,
}

impl WgpuVertexBuffer {
    fn new<T: bytemuck::Pod>(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        data: &[T],
    ) -> Option<Self> {
        if data.is_empty() {
            return None;
        }
        let size = std::mem::size_of_val(data) as u64;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, bytemuck::cast_slice(data));
        Some(Self { buffer, size })
    }

    #[inline]
    fn slice(&self) -> wgpu::BufferSlice<'_> {
        self.buffer.slice(0..self.size)
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

#[derive(Clone, Copy)]
struct PreparedImageInfo {
    size: [u32; 2],
    uv_rect: [f32; 4],
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
}

impl TextLayer {
    fn new(atlas: &mut TextAtlas, device: &wgpu::Device) -> Self {
        Self {
            renderer: TextRenderer::new(atlas, device, wgpu::MultisampleState::default(), None),
            buffers: Vec::new(),
        }
    }
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

/// Renderer for `eui_neo` draw lists.
pub struct WgpuRenderer {
    format: wgpu::TextureFormat,
    wgpu_resources: NeoWgpuResources,
    dummy_backdrop: CachedBackdrop,
    sampler_linear: wgpu::Sampler,
    image_cache: FxHashMap<ImageRef, CachedNeoImage>,
    font_system: FontSystem,
    font_revisions: FxHashMap<FontRef, u64>,
    registered_fonts: FxHashMap<FontRef, RegisteredFont>,
    swash_cache: SwashCache,
    _glyph_cache: Cache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_layers: Vec<TextLayer>,
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
            font_system,
            font_revisions: FxHashMap::default(),
            registered_fonts: FxHashMap::default(),
            swash_cache,
            _glyph_cache: glyph_cache,
            viewport,
            atlas,
            text_layers: Vec::new(),
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
    }

    pub fn matches_format(&self, format: wgpu::TextureFormat) -> bool {
        self.format == format
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

        let pending_images = self.prepare_images(ctx.device, ctx.queue, draw_list, resources);
        self.render_prepared(ctx, draw_list, screen, pending_images)
    }

    fn render_prepared(
        &mut self,
        ctx: &mut Target<'_>,
        draw_list: &UiDrawList,
        screen: Screen,
        pending_images: bool,
    ) -> RenderStatus {
        let logical_rect = LayoutRect::new(0.0, 0.0, screen.width, screen.height);
        let image_sizes: FxHashMap<_, _> = self
            .image_cache
            .iter()
            .map(|(key, image)| {
                (
                    key.clone(),
                    PreparedImageInfo {
                        size: image.size,
                        uv_rect: image.uv_rect,
                    },
                )
            })
            .collect();
        let mut rect_vertices = Vec::new();
        let mut polygon_vertices = Vec::new();
        let mut image_vertices = Vec::new();
        let mut primitive_ops = Vec::new();
        let mut text_items = Vec::new();
        let mut image_items = Vec::new();
        let mut render_ops = Vec::new();
        let surface_is_srgb = self.format.is_srgb();
        collect_draw_items(
            draw_list,
            logical_rect,
            surface_is_srgb,
            &image_sizes,
            &mut rect_vertices,
            &mut polygon_vertices,
            &mut image_vertices,
            &mut primitive_ops,
            &mut text_items,
            &mut image_items,
            &mut render_ops,
        );

        if rect_vertices.is_empty()
            && polygon_vertices.is_empty()
            && image_vertices.is_empty()
            && text_items.is_empty()
        {
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
        let rect_upload = WgpuVertexBuffer::new(
            ctx.device,
            ctx.queue,
            "eui_neo_rect_vertices",
            &rect_vertices,
        );
        let polygon_upload = WgpuVertexBuffer::new(
            ctx.device,
            ctx.queue,
            "eui_neo_polygon_vertices",
            &polygon_vertices,
        );
        let image_upload = WgpuVertexBuffer::new(
            ctx.device,
            ctx.queue,
            "eui_neo_image_vertices",
            &image_vertices,
        );

        self.prepare_text_layers(ctx, &render_ops, &text_items, [screen.width, screen.height]);

        let mut pending_primitives = Vec::new();
        let mut text_layer_index = 0usize;
        for op in &render_ops {
            match *op {
                RenderOp::Primitive(_) => pending_primitives.push(*op),
                RenderOp::Text { .. } => {
                    if !pending_primitives.is_empty() {
                        render_primitive_ops(
                            self,
                            ctx,
                            &primitive_ops,
                            &pending_primitives,
                            rect_upload.as_ref(),
                            polygon_upload.as_ref(),
                            image_upload.as_ref(),
                            &image_items,
                            [screen.width, screen.height],
                        );
                        pending_primitives.clear();
                    }
                    render_text_layer(self, ctx, text_layer_index);
                    text_layer_index += 1;
                }
            }
        }
        if !pending_primitives.is_empty() {
            render_primitive_ops(
                self,
                ctx,
                &primitive_ops,
                &pending_primitives,
                rect_upload.as_ref(),
                polygon_upload.as_ref(),
                image_upload.as_ref(),
                &image_items,
                [screen.width, screen.height],
            );
        }

        self.atlas.trim();
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
            let UiDrawCommand::Image(draw) = command else {
                continue;
            };
            if draw.image.is_empty() {
                continue;
            }
            match resources.image(&draw.image) {
                ImageState::Gpu(image) => {
                    self.cache_gpu_image(device, &draw.image, image);
                }
                ImageState::Pixels(pixels) => {
                    self.cache_pixel_image(device, queue, &draw.image, pixels);
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

        layer.buffers.clear();
        for item in text_items {
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
            let mut buffer =
                Buffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
            let width = if item.frame.width > 0.0 {
                if item.wrap && item.max_width > 0.0 {
                    item.max_width.min(item.frame.width)
                } else {
                    item.frame.width
                }
            } else {
                logical_size[0]
            };
            buffer.set_size(
                &mut self.font_system,
                Some((width * scale_x).max(1.0)),
                Some((item.frame.height * scale_y).max(1.0)),
            );
            buffer.set_wrap(
                &mut self.font_system,
                if item.wrap {
                    Wrap::WordOrGlyph
                } else {
                    Wrap::None
                },
            );
            let align = text_align(item.horizontal_align);
            let attrs = Attrs::new()
                .family(resolve_family(
                    &item.font,
                    &self.registered_fonts,
                    default_text_family.as_deref(),
                    default_icon_family.as_deref(),
                ))
                .weight(Weight(resolved_font_weight(
                    &item.font,
                    item.font_weight,
                )));
            buffer.set_text(
                &mut self.font_system,
                &item.text,
                &attrs,
                Shaping::Advanced,
                Some(align),
            );
            for line in &mut buffer.lines {
                line.set_align(Some(align));
            }
            buffer.shape_until_scroll(&mut self.font_system, false);
            layer.buffers.push(buffer);
        }

        let areas: Vec<_> = layer
            .buffers
            .iter()
            .zip(text_items.iter())
            .map(|(buffer, item)| {
                let text_height =
                    laid_out_text_height(buffer).unwrap_or(item.font_size * text_scale * 1.25);
                let rect_height = item.frame.height * scale_y;
                let y_offset = match item.vertical_align {
                    VerticalAlign::Top => 0.0,
                    VerticalAlign::Center => ((rect_height - text_height) * 0.5).max(0.0),
                    VerticalAlign::Bottom => (rect_height - text_height).max(0.0),
                };
                let color = multiply_alpha(item.color, 1.0);
                TextArea {
                    buffer,
                    left: item.frame.x * scale_x,
                    top: item.frame.y * scale_y + y_offset,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: (item.clip.x * scale_x).round() as i32,
                        top: (item.clip.y * scale_y).round() as i32,
                        right: (item.clip.right() * scale_x).round() as i32,
                        bottom: (item.clip.bottom() * scale_y).round() as i32,
                    },
                    default_color: glyph_color(color),
                    custom_glyphs: &[],
                }
            })
            .collect();

        match layer.renderer.prepare(
            ctx.device,
            ctx.queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        ) {
            Ok(()) => {}
            Err(error) => {
                eprintln!("[eui-neo-wgpu] text prepare failed: {error}");
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_primitive_ops(
    renderer: &mut WgpuRenderer,
    ctx: &mut Target<'_>,
    primitive_ops: &[PrimitiveOp],
    render_ops: &[RenderOp],
    rect_upload: Option<&WgpuVertexBuffer>,
    polygon_upload: Option<&WgpuVertexBuffer>,
    image_upload: Option<&WgpuVertexBuffer>,
    image_items: &[ImageItem],
    logical_size: [f32; 2],
) {
    let mut batch_start = 0usize;
    for (index, render_op) in render_ops.iter().enumerate() {
        if !render_op_uses_backdrop_blur(primitive_ops, *render_op) {
            continue;
        }

        if batch_start < index {
            render_primitive_ops_batch(
                &*renderer,
                ctx,
                primitive_ops,
                &render_ops[batch_start..index],
                rect_upload,
                polygon_upload,
                image_upload,
                image_items,
                logical_size,
                None,
            );
        }

        let Some(backdrop) = capture_backdrop(renderer, ctx, primitive_ops[index], logical_size)
        else {
            render_primitive_ops_batch(
                &*renderer,
                ctx,
                primitive_ops,
                &render_ops[index..index + 1],
                rect_upload,
                polygon_upload,
                image_upload,
                image_items,
                logical_size,
                None,
            );
            batch_start = index + 1;
            continue;
        };
        render_primitive_ops_batch(
            &*renderer,
            ctx,
            primitive_ops,
            &render_ops[index..index + 1],
            rect_upload,
            polygon_upload,
            image_upload,
            image_items,
            logical_size,
            Some(&backdrop.bind_group),
        );
        batch_start = index + 1;
    }

    if batch_start < render_ops.len() {
        render_primitive_ops_batch(
            &*renderer,
            ctx,
            primitive_ops,
            &render_ops[batch_start..],
            rect_upload,
            polygon_upload,
            image_upload,
            image_items,
            logical_size,
            None,
        );
    }
}

fn render_op_uses_backdrop_blur(primitive_ops: &[PrimitiveOp], render_op: RenderOp) -> bool {
    let RenderOp::Primitive(index) = render_op else {
        return false;
    };
    primitive_ops
        .get(index)
        .is_some_and(|op| matches!(op.kind, PrimitiveKind::Rect) && op.backdrop_blur > 0.0)
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

fn render_text_layer(renderer: &WgpuRenderer, ctx: &mut Target<'_>, layer_index: usize) {
    let Some(layer) = renderer.text_layers.get(layer_index) else {
        return;
    };
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
        label: Some("eui_neo_text_overlay"),
        color_attachments: &color_attachments,
        depth_stencil_attachment: None,
        ..Default::default()
    });
    pass.set_scissor_rect(0, 0, physical_size[0].max(1), physical_size[1].max(1));
    if let Err(error) = layer
        .renderer
        .render(&renderer.atlas, &renderer.viewport, &mut pass)
    {
        eprintln!("[eui-neo-wgpu] text render failed: {error}");
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
    renderer: &mut WgpuRenderer,
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
    image_sizes: &FxHashMap<ImageRef, PreparedImageInfo>,
    rect_vertices: &mut Vec<NeoRectVertex>,
    polygon_vertices: &mut Vec<NeoPolygonVertex>,
    image_vertices: &mut Vec<NeoImageVertex>,
    primitive_ops: &mut Vec<PrimitiveOp>,
    text_items: &mut Vec<TextItem>,
    image_items: &mut Vec<ImageItem>,
    render_ops: &mut Vec<RenderOp>,
) {
    let mut clip = full_clip;
    let mut stack = Vec::new();

    for command in draw_list.commands() {
        match command {
            UiDrawCommand::PushClip(next) => {
                stack.push(clip);
                clip = intersect_rect(clip, *next).unwrap_or(LayoutRect::ZERO);
            }
            UiDrawCommand::PopClip => {
                clip = stack.pop().unwrap_or(full_clip);
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
                if push_text(text_items, draw, clip, surface_is_srgb) {
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
                    image_sizes,
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
    clip: LayoutRect,
    surface_is_srgb: bool,
) -> bool {
    if draw.frame.width <= 0.0
        || draw.frame.height <= 0.0
        || draw.opacity <= 0.0
        || clip.width <= 0.0
        || clip.height <= 0.0
    {
        return false;
    }

    let start = vertices.len() as u32;
    if shadow_visible(draw) {
        push_rect_shadow_vertices(vertices, draw, surface_is_srgb);
    }
    if draw.color.a > 0.0 {
        push_rect_fill_vertices(vertices, draw, surface_is_srgb);
    }
    let count = vertices.len() as u32 - start;
    if count == 0 {
        return false;
    }
    ops.push(PrimitiveOp {
        kind: PrimitiveKind::Rect,
        start,
        count,
        clip,
        backdrop_frame: draw.frame,
        backdrop_blur: draw.blur.max(0.0),
    });
    true
}

fn push_rect_fill_vertices(
    vertices: &mut Vec<NeoRectVertex>,
    draw: &UiRectDraw,
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
        surface_is_srgb,
    );

    push_rect_shadow_layer_vertices(
        vertices,
        draw,
        base_shape,
        blur * 0.38,
        0.26,
        surface_is_srgb,
    );
}

fn push_rect_shadow_layer_vertices(
    vertices: &mut Vec<NeoRectVertex>,
    draw: &UiRectDraw,
    shape: LayoutRect,
    blur: f32,
    alpha_scale: f32,
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
    clip: LayoutRect,
    surface_is_srgb: bool,
) -> bool {
    if draw.points.len() < 3 || draw.opacity <= 0.0 || draw.color.a <= 0.0 {
        return false;
    }
    if clip.width <= 0.0 || clip.height <= 0.0 {
        return false;
    }

    let start = vertices.len() as u32;
    let mut color = draw.color;
    color.a *= draw.opacity.clamp(0.0, 1.0);
    let color = output_color(color, surface_is_srgb).to_array();
    let origin = draw.points[0];
    for index in 1..draw.points.len() - 1 {
        for point in [origin, draw.points[index], draw.points[index + 1]] {
            let absolute = [draw.frame.x + point[0], draw.frame.y + point[1]];
            vertices.push(NeoPolygonVertex {
                position: transform_point(absolute, draw.frame, draw.transform),
                color,
            });
        }
    }
    ops.push(PrimitiveOp {
        kind: PrimitiveKind::Polygon,
        start,
        count: (vertices.len() as u32) - start,
        clip,
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
    clip: LayoutRect,
    image_sizes: &FxHashMap<ImageRef, PreparedImageInfo>,
    surface_is_srgb: bool,
) -> bool {
    if draw.image.is_empty()
        || draw.frame.width <= 0.0
        || draw.frame.height <= 0.0
        || draw.opacity <= 0.0
        || draw.tint.a <= 0.0
        || clip.width <= 0.0
        || clip.height <= 0.0
    {
        return false;
    }

    let Some(image_info) = image_sizes.get(&draw.image).copied() else {
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
        });
    }
    images.push(ImageItem {
        image: draw.image.clone(),
    });
    ops.push(PrimitiveOp {
        kind: PrimitiveKind::Image { image_index },
        start,
        count: 6,
        clip,
        backdrop_frame: LayoutRect::ZERO,
        backdrop_blur: 0.0,
    });
    true
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
        backdrop_capture_rect, collect_draw_items, image_rect_and_uv, output_color, srgb_to_linear,
        PrimitiveKind, PrimitiveOp, RenderOp,
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
            id: id.to_string(),
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
            id: id.to_string(),
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
