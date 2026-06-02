//! wgpu renderer for `eui_neo` draw commands.

use std::hash::{Hash, Hasher};

use crate::{NeoImageVertex, NeoPolygonVertex, NeoRectVertex};
use eui_neo::expert::{
    CacheCell, UiDrawCommand, UiDrawList, UiImageDraw, UiNineSliceDraw, UiPolygonDraw, UiRectDraw,
    UiTextDraw,
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
const TEXT_BUFFER_CACHE_BUDGET_BYTES: usize = 8 * 1024 * 1024;
const TEXT_BUFFER_CACHE_MAX_ENTRIES: usize = 512;
const TEXT_BUFFER_CACHE_ADMIT_AFTER_SEEN: u8 = 2;
const TEXT_BUFFER_CACHE_VOLATILE_STREAK: u8 = 2;
const TEXT_BUFFER_CACHE_HISTORY_MAX_AGE_FRAMES: u64 = 600;

mod backdrop;
mod buffers;
mod collect;
mod images;
mod pipelines;
mod primitives;
mod text;

pub use images::{GpuImage, ImagePixels, ImageState, NoResources, Resources};
pub use pipelines::{
    compose_fullscreen_shader, create_backdrop_texture_bind_group_layout,
    create_fullscreen_pipeline, create_image_pipeline, create_image_texture_bind_group_layout,
    create_polygon_pipeline, create_rect_pipeline, create_screen_bind_group,
    create_screen_bind_group_layout, create_screen_buffer, draw_fullscreen_triangle,
    image_vertex_layout, polygon_vertex_layout, rect_vertex_layout, NeoWgpuResources,
    NeoWgpuShaders, ScreenUniform,
};

use backdrop::{create_dummy_backdrop, CachedBackdrop};
use buffers::{create_linear_sampler, WgpuVertexBuffer};
use collect::{collect_draw_items, DrawCollectKey, RenderScratch};
use images::CachedNeoImage;
use primitives::render_ordered_ops;
use text::{
    CachedTextBuffer, TextBufferIdentityKey, TextBufferKey, TextIdentityHistory, TextKeyHistory,
    TextLayer,
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
    text_buffer_cache: FxHashMap<TextBufferKey, CachedTextBuffer>,
    text_buffer_cache_bytes: usize,
    text_buffer_cache_frame: u64,
    text_buffer_key_history: FxHashMap<TextBufferKey, TextKeyHistory>,
    text_buffer_identity_history: FxHashMap<TextBufferIdentityKey, TextIdentityHistory>,
    rect_vertex_buffer: Option<WgpuVertexBuffer>,
    polygon_vertex_buffer: Option<WgpuVertexBuffer>,
    image_vertex_buffer: Option<WgpuVertexBuffer>,
    draw_collect_cache: CacheCell<DrawCollectKey, RenderScratch>,
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
            text_buffer_cache: FxHashMap::default(),
            text_buffer_cache_bytes: 0,
            text_buffer_cache_frame: 0,
            text_buffer_key_history: FxHashMap::default(),
            text_buffer_identity_history: FxHashMap::default(),
            rect_vertex_buffer: None,
            polygon_vertex_buffer: None,
            image_vertex_buffer: None,
            draw_collect_cache: CacheCell::default(),
            frames_since_atlas_trim: 0,
            default_text_family: None,
            default_icon_family: None,
        }
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

        let frame_index = self.text_buffer_cache_frame.wrapping_add(1);
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo_wgpu.render_draw_list");
        let pending_images = {
            #[cfg(feature = "profile")]
            ::profiling::scope!("eui_neo_wgpu.prepare_images");
            self.prepare_images(ctx.device, ctx.queue, draw_list, resources)
        };
        self.render_prepared(ctx, draw_list, screen, pending_images, frame_index)
    }

    fn render_prepared(
        &mut self,
        ctx: &mut Target<'_>,
        draw_list: &UiDrawList,
        screen: Screen,
        pending_images: bool,
        frame_index: u64,
    ) -> RenderStatus {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo_wgpu.render_prepared");
        let logical_rect = LayoutRect::new(0.0, 0.0, screen.width, screen.height);
        let mut draw_collect_cache = std::mem::take(&mut self.draw_collect_cache);
        let surface_is_srgb = self.format.is_srgb();
        let (draw_ptr, draw_len, _) = draw_list.cache_key();
        let collect_key = DrawCollectKey {
            draw_ptr,
            draw_len,
            draw_revision: draw_list.revision(),
            logical_width_bits: screen.width.to_bits(),
            logical_height_bits: screen.height.to_bits(),
            surface_is_srgb,
            image_cache_revision: self.image_cache_revision,
        };
        let collect_access = {
            #[cfg(feature = "profile")]
            ::profiling::scope!("eui_neo_wgpu.collect");
            draw_collect_cache.get_or_rebuild(collect_key, |scratch| {
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
            })
        };
        let scratch = draw_collect_cache.value();

        if scratch.rect_vertices.is_empty()
            && scratch.polygon_vertices.is_empty()
            && scratch.image_vertices.is_empty()
            && scratch.text_items.is_empty()
        {
            self.draw_collect_cache = draw_collect_cache;
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
        {
            #[cfg(feature = "profile")]
            ::profiling::scope!("eui_neo_wgpu.upload_vertices");
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
        }

        {
            #[cfg(feature = "profile")]
            ::profiling::scope!("eui_neo_wgpu.prepare_text");
            self.text_buffer_cache_frame = frame_index;
            self.prepare_text_layers(
                ctx,
                &scratch.render_ops,
                &scratch.text_items,
                [screen.width, screen.height],
            );
        }

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
        {
            #[cfg(feature = "profile")]
            ::profiling::scope!("eui_neo_wgpu.render_ops");
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
        }

        self.frames_since_atlas_trim = self.frames_since_atlas_trim.saturating_add(1);
        if self.frames_since_atlas_trim >= ATLAS_TRIM_INTERVAL_FRAMES {
            self.atlas.trim();
            self.frames_since_atlas_trim = 0;
            for layer in &mut self.text_layers {
                layer.area_keys.clear();
            }
        }
        self.evict_text_buffer_cache();
        let _ = collect_access;
        self.draw_collect_cache = draw_collect_cache;
        RenderStatus {
            pending_images,
            ..RenderStatus::default()
        }
    }
}
#[cfg(test)]
mod tests;
