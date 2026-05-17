//! WGPU overlay renderer for neo draw commands.
//!
//! This is the SkyEngine backend seam for the EUI-NEO-style draw list: source
//! traversal and primitive state live in `draw.rs`; this file translates those
//! commands to the active surface frame.

use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use glyphon::cosmic_text::Align as TextAlign;
use glyphon::{
    Attrs, Buffer, Cache, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap,
};
use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;
use crate::render::gpu::{FullscreenPass, FullscreenPipeline};
use crate::render::Color;

use super::fonts::{
    is_icon_family, load_default_eui_fonts, resolve_family, resolved_font_weight,
    DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE,
};
use super::{
    GradientDirection, HorizontalAlign, ImageFit, LayoutRect, Screen, Transform, UiDrawCommand,
    UiDrawList, UiImageDraw, UiPolygonDraw, UiRectDraw, UiTextDraw, VerticalAlign,
};

const NEO_RECT_SHADER: &str = r#"
struct Screen {
    size: vec4<f32>,
    backdrop_size: vec4<f32>,
    backdrop_rect: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> screen: Screen;

@group(1) @binding(0)
var t_backdrop: texture_2d<f32>;
@group(1) @binding(1)
var s_backdrop: sampler;

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) rect: vec4<f32>,
    @location(3) fill: vec4<f32>,
    @location(4) gradient_start: vec4<f32>,
    @location(5) gradient_end: vec4<f32>,
    @location(6) border: vec4<f32>,
    @location(7) params: vec4<f32>,
    @location(8) flags: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) gradient_start: vec4<f32>,
    @location(4) gradient_end: vec4<f32>,
    @location(5) border: vec4<f32>,
    @location(6) params: vec4<f32>,
    @location(7) flags: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    let x = input.position.x / max(screen.size.x, 1.0) * 2.0 - 1.0;
    let y = 1.0 - (input.position.y / max(screen.size.y, 1.0) * 2.0);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.local_pos = input.local_pos;
    out.rect = input.rect;
    out.fill = input.fill;
    out.gradient_start = input.gradient_start;
    out.gradient_end = input.gradient_end;
    out.border = input.border;
    out.params = input.params;
    out.flags = input.flags;
    return out;
}

fn rounded_box_distance(point: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let corner = abs(point) - half_size + vec2<f32>(radius, radius);
    return length(max(corner, vec2<f32>(0.0, 0.0))) + min(max(corner.x, corner.y), 0.0) - radius;
}

fn rand(co: vec2<f32>) -> f32 {
    return fract(sin(dot(co, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

fn backdrop_blur(uv: vec2<f32>, blur_amount: f32) -> vec3<f32> {
    let pixel_step = 1.0 / max(screen.backdrop_size.xy, vec2<f32>(1.0, 1.0));
    var blurred = textureSampleLevel(t_backdrop, s_backdrop, uv, 0.0).rgb;
    let repeats = mix(8.0, 24.0, clamp(blur_amount / 36.0, 0.0, 1.0));
    let tau = 6.28318530718;
    for (var index: i32 = 0; index < 24; index = index + 1) {
        let i = f32(index);
        if (i >= repeats) {
            break;
        }
        let angle = (i / repeats) * tau;
        let dir = vec2<f32>(cos(angle), sin(angle));
        let radius_a = blur_amount * (0.35 + 0.65 * rand(vec2<f32>(i, uv.x + uv.y)));
        let uv_a = clamp(
            uv + dir * radius_a * pixel_step,
            pixel_step * 0.5,
            vec2<f32>(1.0, 1.0) - pixel_step * 0.5
        );
        blurred += textureSampleLevel(t_backdrop, s_backdrop, uv_a, 0.0).rgb;

        let angle_b = angle + (0.5 * tau / repeats);
        let dir_b = vec2<f32>(cos(angle_b), sin(angle_b));
        let radius_b = blur_amount * (0.20 + 0.80 * rand(vec2<f32>(i + 2.0, uv.x + uv.y + 24.0)));
        let uv_b = clamp(
            uv + dir_b * radius_b * pixel_step,
            pixel_step * 0.5,
            vec2<f32>(1.0, 1.0) - pixel_step * 0.5
        );
        blurred += textureSampleLevel(t_backdrop, s_backdrop, uv_b, 0.0).rgb;
    }
    return blurred / (repeats * 2.0 + 1.0);
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    let radius = clamp(input.params.x, 0.0, min(input.rect.z, input.rect.w) * 0.5);
    let border_width = clamp(input.params.y, 0.0, min(input.rect.z, input.rect.w) * 0.5);
    let opacity = clamp(input.params.z, 0.0, 1.0);
    let use_gradient = input.params.w > 0.5;
    let is_shadow = input.flags.y > 0.5;
    let center = input.rect.xy + input.rect.zw * 0.5;
    let distance_to_edge = rounded_box_distance(input.local_pos - center, input.rect.zw * 0.5, radius);
    let edge_width = max(fwidth(distance_to_edge), 0.75);
    if (is_shadow) {
        let blur = max(input.params.y, edge_width);
        let shadow_alpha = 1.0 - smoothstep(-blur, blur, distance_to_edge);
        if (shadow_alpha <= 0.0 || opacity <= 0.0) {
            discard;
        }
        return vec4<f32>(input.fill.rgb, input.fill.a * shadow_alpha * opacity);
    }
    let shape_alpha = 1.0 - smoothstep(-edge_width, edge_width, distance_to_edge);
    if (shape_alpha <= 0.0 || opacity <= 0.0) {
        discard;
    }

    let gradient_amount = select(
        clamp((input.local_pos.y - input.rect.y) / max(input.rect.w, 1.0), 0.0, 1.0),
        clamp((input.local_pos.x - input.rect.x) / max(input.rect.z, 1.0), 0.0, 1.0),
        input.flags.x < 0.5
    );
    var fill = select(input.fill, mix(input.gradient_start, input.gradient_end, gradient_amount), use_gradient);
    let blur_scale = max(
        screen.size.z / max(screen.size.x, 1.0),
        screen.size.w / max(screen.size.y, 1.0)
    );
    let blur_amount = max(input.flags.z, 0.0) * max(blur_scale, 1.0);
    if (blur_amount > 0.0) {
        let backdrop_uv = clamp(
            (input.position.xy - screen.backdrop_rect.xy) / max(screen.backdrop_rect.zw, vec2<f32>(1.0, 1.0)),
            vec2<f32>(0.0, 0.0),
            vec2<f32>(1.0, 1.0)
        );
        let blurred = backdrop_blur(backdrop_uv, blur_amount);
        fill = vec4<f32>(mix(blurred, fill.rgb, fill.a), 1.0);
    }
    let border_alpha = select(
        0.0,
        smoothstep(-border_width - edge_width, -border_width + edge_width, distance_to_edge),
        border_width > 0.0
    );
    let color = mix(fill, input.border, border_alpha);
    return vec4<f32>(color.rgb, color.a * shape_alpha * opacity);
}
"#;

const NEO_CAPTURE_SHADER: &str = r#"
struct Screen {
    size: vec4<f32>,
    backdrop_size: vec4<f32>,
    backdrop_rect: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> screen: Screen;

@group(1) @binding(0)
var t_source: texture_2d<f32>;
@group(1) @binding(1)
var s_source: sampler;

@fragment
fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let source_uv = clamp(
        (screen.backdrop_rect.xy + in.uv * screen.backdrop_rect.zw) / max(screen.size.xy, vec2<f32>(1.0, 1.0)),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0)
    );
    return textureSample(t_source, s_source, source_uv);
}
"#;

const NEO_POLYGON_SHADER: &str = r#"
struct Screen {
    size: vec2<f32>,
    backdrop_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> screen: Screen;

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    let x = input.position.x / max(screen.size.x, 1.0) * 2.0 - 1.0;
    let y = 1.0 - (input.position.y / max(screen.size.y, 1.0) * 2.0);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

const NEO_IMAGE_SHADER: &str = r#"
struct Screen {
    size: vec2<f32>,
    backdrop_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> screen: Screen;

@group(1) @binding(0)
var t_image: texture_2d<f32>;
@group(1) @binding(1)
var s_image: sampler;

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) rect: vec4<f32>,
    @location(3) uv: vec2<f32>,
    @location(4) tint: vec4<f32>,
    @location(5) params: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) tint: vec4<f32>,
    @location(4) params: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    let x = input.position.x / max(screen.size.x, 1.0) * 2.0 - 1.0;
    let y = 1.0 - (input.position.y / max(screen.size.y, 1.0) * 2.0);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.local_pos = input.local_pos;
    out.rect = input.rect;
    out.uv = input.uv;
    out.tint = input.tint;
    out.params = input.params;
    return out;
}

fn rounded_box_distance(point: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let corner = abs(point) - half_size + vec2<f32>(radius, radius);
    return length(max(corner, vec2<f32>(0.0, 0.0))) + min(max(corner.x, corner.y), 0.0) - radius;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    let radius = clamp(input.params.x, 0.0, min(input.rect.z, input.rect.w) * 0.5);
    let opacity = clamp(input.params.y, 0.0, 1.0);
    let center = input.rect.xy + input.rect.zw * 0.5;
    let distance_to_edge = rounded_box_distance(input.local_pos - center, input.rect.zw * 0.5, radius);
    let edge_width = max(fwidth(distance_to_edge), 0.75);
    let shape_alpha = 1.0 - smoothstep(-edge_width, edge_width, distance_to_edge);
    if (shape_alpha <= 0.0 || opacity <= 0.0) {
        discard;
    }

    let sampled = textureSample(t_image, s_image, input.uv);
    let tint = vec4<f32>(input.tint.rgb, input.tint.a * opacity);
    return vec4<f32>(sampled.rgb * tint.rgb, sampled.a * tint.a * shape_alpha);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct NeoRectVertex {
    position: [f32; 2],
    local_pos: [f32; 2],
    rect: [f32; 4],
    fill: [f32; 4],
    gradient_start: [f32; 4],
    gradient_end: [f32; 4],
    border: [f32; 4],
    params: [f32; 4],
    flags: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct NeoPolygonVertex {
    position: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct NeoImageVertex {
    position: [f32; 2],
    local_pos: [f32; 2],
    rect: [f32; 4],
    uv: [f32; 2],
    tint: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenUniform {
    size: [f32; 4],
    backdrop_size: [f32; 4],
    backdrop_rect: [f32; 4],
}

#[derive(Clone)]
struct TextItem {
    text: String,
    font_family: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ImageCacheKey {
    source: String,
    flip_vertically: bool,
}

impl ImageCacheKey {
    fn new(source: impl Into<String>, flip_vertically: bool) -> Self {
        Self {
            source: source.into(),
            flip_vertically,
        }
    }

    fn from_source(source: &str, flip_vertically: bool) -> Option<Self> {
        normalize_image_source(source).map(|source| Self::new(source, flip_vertically))
    }

    fn remote(&self) -> bool {
        is_remote_image_source(&self.source) || self.source.starts_with("bing://daily")
    }
}

#[derive(Clone)]
struct ImageItem {
    key: ImageCacheKey,
}

struct CachedNeoImage {
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    size: [u32; 2],
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

/// Renderer for the experimental neo backend.
pub struct NeoRenderer {
    format: wgpu::TextureFormat,
    rect_pipeline: wgpu::RenderPipeline,
    capture_pipeline: FullscreenPipeline,
    polygon_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    screen_buffer: wgpu::Buffer,
    screen_bind_group: wgpu::BindGroup,
    backdrop_texture_layout: wgpu::BindGroupLayout,
    dummy_backdrop: CachedBackdrop,
    image_texture_layout: wgpu::BindGroupLayout,
    image_cache: FxHashMap<ImageCacheKey, CachedNeoImage>,
    pending_images: FxHashMap<ImageCacheKey, Receiver<Result<LoadedImagePixels, String>>>,
    failed_images: FxHashMap<ImageCacheKey, Instant>,
    font_system: FontSystem,
    swash_cache: SwashCache,
    _glyph_cache: Cache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_layers: Vec<TextLayer>,
    default_text_family: Option<String>,
    default_icon_family: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NeoRenderStatus {
    pub pending_images: bool,
}

impl std::fmt::Debug for NeoRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeoRenderer")
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl NeoRenderer {
    pub fn new(gpu: &GpuContext) -> Self {
        let device = gpu.device();
        let queue = gpu.queue();
        let format = gpu.surface_format();

        let screen_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky_neo_ui_screen_uniform"),
            size: std::mem::size_of::<ScreenUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sky_neo_ui_screen_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky_neo_ui_screen_bg"),
            layout: &screen_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_buffer.as_entire_binding(),
            }],
        });
        let backdrop_texture_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sky_neo_ui_backdrop_texture_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let dummy_backdrop = create_dummy_backdrop(gpu, &backdrop_texture_layout);

        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_neo_ui_rect_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(NEO_RECT_SHADER)),
        });
        let polygon_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_neo_ui_polygon_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(NEO_POLYGON_SHADER)),
        });
        let rect_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky_neo_ui_rect_pipeline_layout"),
            bind_group_layouts: &[
                Some(&screen_bind_group_layout),
                Some(&backdrop_texture_layout),
            ],
            immediate_size: 0,
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky_neo_ui_pipeline_layout"),
            bind_group_layouts: &[Some(&screen_bind_group_layout)],
            immediate_size: 0,
        });
        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky_neo_ui_rect_pipeline"),
            layout: Some(&rect_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rect_shader,
                entry_point: Some("vs_main"),
                buffers: &[rect_vertex_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &rect_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let polygon_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky_neo_ui_polygon_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &polygon_shader,
                entry_point: Some("vs_main"),
                buffers: &[polygon_vertex_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &polygon_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let image_texture_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sky_neo_ui_image_texture_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let capture_pipeline = FullscreenPipeline::new(
            gpu,
            NEO_CAPTURE_SHADER,
            "fs_main",
            &[&screen_bind_group_layout, &image_texture_layout],
            format,
            None,
            "sky_neo_ui_backdrop_capture",
        );
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_neo_ui_image_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(NEO_IMAGE_SHADER)),
        });
        let image_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sky_neo_ui_image_pipeline_layout"),
                bind_group_layouts: &[Some(&screen_bind_group_layout), Some(&image_texture_layout)],
                immediate_size: 0,
            });
        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky_neo_ui_image_pipeline"),
            layout: Some(&image_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: Some("vs_main"),
                buffers: &[image_vertex_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &image_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let mut font_system = FontSystem::new();
        let loaded_fonts = load_default_eui_fonts(&mut font_system);
        if loaded_fonts.text {
            font_system.db_mut().set_sans_serif_family("JinNanJunJunTi");
            font_system.db_mut().set_serif_family("JinNanJunJunTi");
        }
        let swash_cache = SwashCache::new();
        let glyph_cache = Cache::new(device);
        let viewport = Viewport::new(device, &glyph_cache);
        let atlas = TextAtlas::new(device, queue, &glyph_cache, format);

        Self {
            format,
            rect_pipeline,
            capture_pipeline,
            polygon_pipeline,
            image_pipeline,
            screen_buffer,
            screen_bind_group,
            backdrop_texture_layout,
            dummy_backdrop,
            image_texture_layout,
            image_cache: FxHashMap::default(),
            pending_images: FxHashMap::default(),
            failed_images: FxHashMap::default(),
            font_system,
            swash_cache,
            _glyph_cache: glyph_cache,
            viewport,
            atlas,
            text_layers: Vec::new(),
            default_text_family: loaded_fonts.text.then(|| "JinNanJunJunTi".to_string()),
            default_icon_family: loaded_fonts.icon.then(|| "Font Awesome 7 Free".to_string()),
        }
    }

    pub fn matches_surface(&self, format: wgpu::TextureFormat) -> bool {
        self.format == format
    }

    pub fn render(
        &mut self,
        gpu: &mut GpuContext,
        draw_list: &UiDrawList,
        screen: Screen,
    ) -> NeoRenderStatus {
        if draw_list.is_empty() || screen.width <= 0.0 || screen.height <= 0.0 {
            return NeoRenderStatus::default();
        }

        let logical_rect = LayoutRect::new(0.0, 0.0, screen.width, screen.height);
        self.prepare_image_cache(gpu, draw_list);
        let image_sizes: FxHashMap<_, _> = self
            .image_cache
            .iter()
            .map(|(key, image)| (key.clone(), image.size))
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
            return NeoRenderStatus {
                pending_images: !self.pending_images.is_empty(),
            };
        }

        let screen_uniform = ScreenUniform {
            size: [
                screen.width,
                screen.height,
                gpu.surface_size()[0].max(1) as f32,
                gpu.surface_size()[1].max(1) as f32,
            ],
            backdrop_size: [0.0, 0.0, 0.0, 0.0],
            backdrop_rect: [0.0, 0.0, 0.0, 0.0],
        };
        gpu.queue()
            .write_buffer(&self.screen_buffer, 0, bytemuck::bytes_of(&screen_uniform));
        let rect_upload = (!rect_vertices.is_empty()).then(|| gpu.upload_vertices(&rect_vertices));
        let polygon_upload =
            (!polygon_vertices.is_empty()).then(|| gpu.upload_vertices(&polygon_vertices));
        let image_upload =
            (!image_vertices.is_empty()).then(|| gpu.upload_vertices(&image_vertices));

        self.prepare_text_layers(gpu, &render_ops, &text_items, [screen.width, screen.height]);

        let mut pending_primitives = Vec::new();
        let mut text_layer_index = 0usize;
        for op in &render_ops {
            match *op {
                RenderOp::Primitive(_) => pending_primitives.push(*op),
                RenderOp::Text { .. } => {
                    if !pending_primitives.is_empty() {
                        render_primitive_ops(
                            self,
                            gpu,
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
                    render_text_layer(self, gpu, text_layer_index);
                    text_layer_index += 1;
                }
            }
        }
        if !pending_primitives.is_empty() {
            render_primitive_ops(
                self,
                gpu,
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
        NeoRenderStatus {
            pending_images: !self.pending_images.is_empty(),
        }
    }

    fn prepare_image_cache(&mut self, gpu: &GpuContext, draw_list: &UiDrawList) {
        let mut completed = Vec::new();
        for (key, receiver) in &self.pending_images {
            match receiver.try_recv() {
                Ok(Ok(loaded)) => {
                    let cached = upload_image(gpu, &self.image_texture_layout, loaded);
                    self.image_cache.insert(key.clone(), cached);
                    self.failed_images.remove(key);
                    completed.push(key.clone());
                }
                Ok(Err(error)) => {
                    eprintln!(
                        "[SkyEngine] neo UI image load failed for {}: {error}",
                        key.source
                    );
                    self.failed_images.insert(key.clone(), Instant::now());
                    completed.push(key.clone());
                }
                Err(TryRecvError::Disconnected) => {
                    eprintln!(
                        "[SkyEngine] neo UI image load worker disconnected for {}",
                        key.source
                    );
                    self.failed_images.insert(key.clone(), Instant::now());
                    completed.push(key.clone());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        for key in completed {
            self.pending_images.remove(&key);
        }

        for command in draw_list.commands() {
            let UiDrawCommand::Image(draw) = command else {
                continue;
            };
            if draw.source.is_empty() {
                continue;
            }
            let Some(key) = ImageCacheKey::from_source(&draw.source, draw.flip_vertically) else {
                continue;
            };
            if self.image_cache.contains_key(&key) || self.pending_images.contains_key(&key) {
                continue;
            }
            if let Some(failed_at) = self.failed_images.get(&key) {
                if failed_at.elapsed() < Duration::from_secs(5) {
                    continue;
                }
                self.failed_images.remove(&key);
            }
            if key.remote() {
                let (sender, receiver) = mpsc::channel();
                let source = key.source.clone();
                let flip_vertically = key.flip_vertically;
                std::thread::spawn(move || {
                    let _ = sender.send(load_image_pixels(&source, flip_vertically));
                });
                self.pending_images.insert(key, receiver);
                continue;
            }
            match load_image_pixels(&key.source, key.flip_vertically) {
                Ok(loaded) => {
                    let cached = upload_image(gpu, &self.image_texture_layout, loaded);
                    self.image_cache.insert(key, cached);
                }
                Err(error) => {
                    eprintln!(
                        "[SkyEngine] neo UI image load failed for {}: {error}",
                        key.source
                    );
                    self.failed_images.insert(key, Instant::now());
                }
            }
        }
    }

    fn prepare_text_layers(
        &mut self,
        gpu: &GpuContext,
        render_ops: &[RenderOp],
        text_items: &[TextItem],
        logical_size: [f32; 2],
    ) {
        let physical_size = gpu.surface_size();
        self.viewport.update(
            gpu.queue(),
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
                    .push(TextLayer::new(&mut self.atlas, gpu.device()));
            }
            let end = start.saturating_add(count).min(text_items.len());
            let layer_items = if start < end {
                &text_items[start..end]
            } else {
                &[]
            };
            self.prepare_text_layer(gpu, layer_index, layer_items, logical_size, physical_size);
            layer_index += 1;
        }
    }

    fn prepare_text_layer(
        &mut self,
        gpu: &GpuContext,
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
            let icon_font = is_icon_family(item.font_family.as_str());
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
                    item.font_family.as_str(),
                    default_text_family.as_deref(),
                    default_icon_family.as_deref(),
                ))
                .weight(Weight(resolved_font_weight(
                    item.font_family.as_str(),
                    item.font_weight,
                    icon_font,
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
            gpu.device(),
            gpu.queue(),
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        ) {
            Ok(()) => {}
            Err(error) => {
                eprintln!("[SkyEngine] neo UI text prepare failed: {error}");
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn render_primitive_ops(
    renderer: &mut NeoRenderer,
    gpu: &mut GpuContext,
    primitive_ops: &[PrimitiveOp],
    render_ops: &[RenderOp],
    rect_upload: Option<&crate::gpu::UploadSlice>,
    polygon_upload: Option<&crate::gpu::UploadSlice>,
    image_upload: Option<&crate::gpu::UploadSlice>,
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
                gpu,
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

        let Some(backdrop) = capture_backdrop(renderer, gpu, primitive_ops[index], logical_size)
        else {
            render_primitive_ops_batch(
                &*renderer,
                gpu,
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
            gpu,
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
            gpu,
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
    renderer: &NeoRenderer,
    gpu: &mut GpuContext,
    primitive_ops: &[PrimitiveOp],
    render_ops: &[RenderOp],
    rect_upload: Option<&crate::gpu::UploadSlice>,
    polygon_upload: Option<&crate::gpu::UploadSlice>,
    image_upload: Option<&crate::gpu::UploadSlice>,
    image_items: &[ImageItem],
    logical_size: [f32; 2],
    backdrop_bind_group: Option<&wgpu::BindGroup>,
) {
    if render_ops.is_empty() {
        return;
    }
    let physical_size = gpu.surface_size();
    let mut frame = gpu.frame();
    let mut pass = frame.begin_surface_pass_loaded("sky_neo_ui_overlay");
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
                    pass.set_pipeline(&renderer.rect_pipeline);
                    pass.set_bind_group(0, &renderer.screen_bind_group, &[]);
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
                    pass.set_pipeline(&renderer.polygon_pipeline);
                    pass.set_bind_group(0, &renderer.screen_bind_group, &[]);
                    pass.set_vertex_buffer(0, upload.slice());
                    active_kind = Some(PrimitiveKind::Polygon);
                }
            }
            PrimitiveKind::Image { image_index } => {
                let (Some(upload), Some(image_item)) = (image_upload, image_items.get(image_index))
                else {
                    continue;
                };
                let Some(cached) = renderer.image_cache.get(&image_item.key) else {
                    continue;
                };
                if !matches!(active_kind, Some(PrimitiveKind::Image { .. })) {
                    pass.set_pipeline(&renderer.image_pipeline);
                    pass.set_bind_group(0, &renderer.screen_bind_group, &[]);
                    pass.set_vertex_buffer(0, upload.slice());
                    active_kind = Some(PrimitiveKind::Image { image_index });
                }
                pass.set_bind_group(1, &cached.bind_group, &[]);
            }
        }
        pass.draw(op.start..op.start + op.count, 0..1);
    }
}

fn render_text_layer(renderer: &NeoRenderer, gpu: &mut GpuContext, layer_index: usize) {
    let Some(layer) = renderer.text_layers.get(layer_index) else {
        return;
    };
    let physical_size = gpu.surface_size();
    let mut frame = gpu.frame();
    let mut pass = frame.begin_surface_pass_loaded("sky_neo_ui_text_overlay");
    pass.set_scissor_rect(0, 0, physical_size[0].max(1), physical_size[1].max(1));
    if let Err(error) = layer
        .renderer
        .render(&renderer.atlas, &renderer.viewport, &mut pass)
    {
        eprintln!("[SkyEngine] neo UI text render failed: {error}");
    }
}

fn normalize_image_source(source: &str) -> Option<String> {
    if source.is_empty() {
        return None;
    }
    if is_remote_image_source(source) || source.starts_with("bing://daily") {
        return Some(source.to_string());
    }
    resolve_local_image_source(source).map(|path| path.to_string_lossy().into_owned())
}

fn is_remote_image_source(source: &str) -> bool {
    source.starts_with("http://") || source.starts_with("https://")
}

fn resolve_local_image_source(source: &str) -> Option<PathBuf> {
    let source_path = Path::new(source);
    let mut candidates = Vec::new();
    candidates.push(source_path.to_path_buf());
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(source_path));
        candidates.push(current_dir.join("assets").join(source_path));
        if let Some(file_name) = source_path.file_name() {
            candidates.push(current_dir.join("assets").join(file_name));
        }
    }

    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate.canonicalize().unwrap_or(candidate));
        }
    }
    None
}

struct LoadedImagePixels {
    pixels: Vec<u8>,
    width: u32,
    height: u32,
}

fn load_image_pixels(source: &str, flip_vertically: bool) -> Result<LoadedImagePixels, String> {
    let bytes = if let Some(params) = source.strip_prefix("bing://daily") {
        load_bing_daily_bytes(params)?
    } else if is_remote_image_source(source) {
        load_url_image_bytes_cached(source)?
    } else {
        let resolved = Path::new(source);
        std::fs::read(resolved).map_err(|error| format!("read {} failed: {error}", source))?
    };
    let image = ::image::load_from_memory(&bytes)
        .map_err(|error| format!("decode image failed: {error}"))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    let mut pixels = image.into_raw();
    if flip_vertically {
        flip_rgba_rows(&mut pixels, width, height);
    }
    Ok(LoadedImagePixels {
        pixels,
        width,
        height,
    })
}

fn load_bing_daily_bytes(query: &str) -> Result<Vec<u8>, String> {
    let idx = query_param(query, "idx").unwrap_or_else(|| "0".to_string());
    let mkt = query_param(query, "mkt").unwrap_or_else(|| "zh-CN".to_string());
    let daily_cache_key = format!("bing://daily?idx={idx}&mkt={mkt}&day={}", unix_day_now());
    if let Some(bytes) = load_cached_image_bytes(&daily_cache_key) {
        return Ok(bytes);
    }
    let fallback_cache_key = format!("bing://daily?idx={idx}&mkt={mkt}");
    let metadata_url =
        format!("https://www.bing.com/HPImageArchive.aspx?format=js&n=1&idx={idx}&mkt={mkt}");
    let fetched = (|| {
        let metadata = String::from_utf8(load_url_bytes(&metadata_url)?)
            .map_err(|error| format!("Bing metadata was not UTF-8: {error}"))?;
        let json: serde_json::Value = serde_json::from_str(&metadata)
            .map_err(|error| format!("Bing metadata JSON parse failed: {error}"))?;
        let image_url = json
            .get("images")
            .and_then(|images| images.get(0))
            .and_then(|image| image.get("url"))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Bing metadata did not contain images[0].url".to_string())?;
        let image_url = if is_remote_image_source(image_url) {
            image_url.to_string()
        } else {
            format!("https://www.bing.com{image_url}")
        };
        load_url_image_bytes_cached(&image_url)
    })();
    match fetched {
        Ok(bytes) => {
            store_cached_image_bytes(&daily_cache_key, &bytes);
            store_cached_image_bytes(&fallback_cache_key, &bytes);
            Ok(bytes)
        }
        Err(error) => load_cached_image_bytes(&fallback_cache_key).ok_or(error),
    }
}

fn query_param(query: &str, key: &str) -> Option<String> {
    let query = query.strip_prefix('?').unwrap_or(query);
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| value.to_string())
    })
}

fn load_url_bytes(url: &str) -> Result<Vec<u8>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(12))
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|error| format!("GET {url} failed: {error}"))?;
    let mut reader = response.into_reader();
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read response from {url} failed: {error}"))?;
    Ok(bytes)
}

fn load_url_image_bytes_cached(url: &str) -> Result<Vec<u8>, String> {
    let Some(path) = remote_image_cache_path(url) else {
        return load_url_bytes(url);
    };
    if let Ok(bytes) = std::fs::read(&path) {
        if !bytes.is_empty() && ::image::load_from_memory(&bytes).is_ok() {
            return Ok(bytes);
        }
        let _ = std::fs::remove_file(&path);
    }
    let bytes = load_url_bytes(url)?;
    ::image::load_from_memory(&bytes)
        .map_err(|error| format!("downloaded image from {url} did not decode: {error}"))?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, &bytes);
    Ok(bytes)
}

fn remote_image_cache_path(url: &str) -> Option<PathBuf> {
    if !is_remote_image_source(url) {
        return None;
    }
    let extension = remote_image_extension(url);
    Some(image_cache_path_for_key(url, extension))
}

fn image_cache_path_for_key(key: &str, extension: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    std::env::temp_dir()
        .join("eui_test_image_cache")
        .join(format!("{:016x}{extension}", hasher.finish()))
}

fn load_cached_image_bytes(key: &str) -> Option<Vec<u8>> {
    let path = image_cache_path_for_key(key, ".cache");
    let Ok(bytes) = std::fs::read(&path) else {
        return None;
    };
    if !bytes.is_empty() && ::image::load_from_memory(&bytes).is_ok() {
        return Some(bytes);
    }
    let _ = std::fs::remove_file(path);
    None
}

fn store_cached_image_bytes(key: &str, bytes: &[u8]) {
    if bytes.is_empty() || ::image::load_from_memory(bytes).is_err() {
        return;
    }
    let path = image_cache_path_for_key(key, ".cache");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, bytes);
}

fn unix_day_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86_400
}

fn remote_image_extension(url: &str) -> &'static str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    match extension.as_deref() {
        Some("png") => ".png",
        Some("jpg") | Some("jpeg") => ".jpg",
        Some("webp") => ".webp",
        Some("bmp") => ".bmp",
        _ => ".cache",
    }
}

fn upload_image(
    gpu: &GpuContext,
    image_texture_layout: &wgpu::BindGroupLayout,
    loaded: LoadedImagePixels,
) -> CachedNeoImage {
    let device = gpu.device();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sky_neo_ui_image_texture"),
        size: wgpu::Extent3d {
            width: loaded.width.max(1),
            height: loaded.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &loaded.pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * loaded.width.max(1)),
            rows_per_image: Some(loaded.height.max(1)),
        },
        wgpu::Extent3d {
            width: loaded.width.max(1),
            height: loaded.height.max(1),
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sky_neo_ui_image_texture_bg"),
        layout: image_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(gpu.sampler_linear()),
            },
        ],
    });
    CachedNeoImage {
        _texture: texture,
        bind_group,
        size: [loaded.width.max(1), loaded.height.max(1)],
    }
}

fn create_dummy_backdrop(
    gpu: &GpuContext,
    backdrop_texture_layout: &wgpu::BindGroupLayout,
) -> CachedBackdrop {
    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("sky_neo_ui_dummy_backdrop_texture"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu.surface_format(),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue().write_texture(
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
    let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sky_neo_ui_dummy_backdrop_bg"),
        layout: backdrop_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(gpu.sampler_linear()),
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
    renderer: &mut NeoRenderer,
    gpu: &mut GpuContext,
    op: PrimitiveOp,
    logical_size: [f32; 2],
) -> Option<CachedBackdrop> {
    let [surface_width, surface_height] = gpu.surface_size();
    let capture_rect = backdrop_capture_rect(op, logical_size, [surface_width, surface_height])?;
    let texture_size = [
        ((capture_rect[2] as f32) * 0.5).ceil().max(1.0) as u32,
        ((capture_rect[3] as f32) * 0.5).ceil().max(1.0) as u32,
    ];

    let snapshot = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("sky_neo_ui_backdrop_snapshot_texture"),
        size: wgpu::Extent3d {
            width: surface_width.max(1),
            height: surface_height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu.surface_format(),
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    if let Err(error) = gpu.copy_current_surface_to_texture_extent(
        &snapshot,
        wgpu::Extent3d {
            width: surface_width.max(1),
            height: surface_height.max(1),
            depth_or_array_layers: 1,
        },
    ) {
        eprintln!("[SkyEngine] neo UI backdrop blur capture skipped: {error}");
        return None;
    }

    let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("sky_neo_ui_backdrop_texture"),
        size: wgpu::Extent3d {
            width: texture_size[0],
            height: texture_size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu.surface_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let snapshot_view = snapshot.create_view(&wgpu::TextureViewDescriptor::default());
    let capture_bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sky_neo_ui_backdrop_capture_bg"),
        layout: &renderer.image_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&snapshot_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(gpu.sampler_linear()),
            },
        ],
    });
    write_backdrop_uniform(
        gpu,
        &renderer.screen_buffer,
        logical_size,
        capture_rect,
        texture_size,
    );
    {
        let pipeline = renderer
            .capture_pipeline
            .pipeline(gpu, gpu.surface_format());
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut frame = gpu.frame();
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
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sky_neo_ui_backdrop_capture"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline.as_ref());
        pass.set_bind_group(0, &renderer.screen_bind_group, &[]);
        pass.set_bind_group(1, &capture_bind_group, &[]);
        FullscreenPass::draw(&mut pass);
    }

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sky_neo_ui_backdrop_bg"),
        layout: &renderer.backdrop_texture_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(gpu.sampler_linear()),
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
    gpu: &GpuContext,
    screen_buffer: &wgpu::Buffer,
    logical_size: [f32; 2],
    capture_rect: [f32; 4],
    texture_size: [u32; 2],
) {
    let [surface_width, surface_height] = gpu.surface_size();
    let screen_uniform = ScreenUniform {
        size: [
            logical_size[0],
            logical_size[1],
            surface_width.max(1) as f32,
            surface_height.max(1) as f32,
        ],
        backdrop_size: [
            texture_size[0].max(1) as f32,
            texture_size[1].max(1) as f32,
            0.0,
            0.0,
        ],
        backdrop_rect: capture_rect,
    };
    gpu.queue()
        .write_buffer(screen_buffer, 0, bytemuck::bytes_of(&screen_uniform));
}

fn flip_rgba_rows(pixels: &mut [u8], width: u32, height: u32) {
    if width == 0 || height <= 1 {
        return;
    }
    let row_bytes = width as usize * 4;
    let mut temp = vec![0u8; row_bytes];
    for y in 0..(height / 2) as usize {
        let top = y * row_bytes;
        let bottom = (height as usize - 1 - y) * row_bytes;
        temp.copy_from_slice(&pixels[top..top + row_bytes]);
        pixels.copy_within(bottom..bottom + row_bytes, top);
        pixels[bottom..bottom + row_bytes].copy_from_slice(&temp);
    }
}

fn collect_draw_items(
    draw_list: &UiDrawList,
    full_clip: LayoutRect,
    surface_is_srgb: bool,
    image_sizes: &FxHashMap<ImageCacheKey, [u32; 2]>,
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
    image_sizes: &FxHashMap<ImageCacheKey, [u32; 2]>,
    surface_is_srgb: bool,
) -> bool {
    if draw.source.is_empty()
        || draw.frame.width <= 0.0
        || draw.frame.height <= 0.0
        || draw.opacity <= 0.0
        || draw.tint.a <= 0.0
        || clip.width <= 0.0
        || clip.height <= 0.0
    {
        return false;
    }

    let Some(key) = ImageCacheKey::from_source(&draw.source, draw.flip_vertically) else {
        return false;
    };
    let Some(texture_size) = image_sizes.get(&key).copied() else {
        return false;
    };
    let Some((draw_rect, uv_rect)) = image_rect_and_uv(draw.frame, draw.fit, texture_size) else {
        return false;
    };

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
    images.push(ImageItem { key });
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
        font_family: draw.font_family.clone(),
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

fn rect_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<NeoRectVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: std::mem::size_of::<[f32; 2]>() as u64,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (std::mem::size_of::<[f32; 2]>() * 2) as u64,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (std::mem::size_of::<[f32; 2]>() * 2 + std::mem::size_of::<[f32; 4]>())
                    as u64,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (std::mem::size_of::<[f32; 2]>() * 2 + std::mem::size_of::<[f32; 4]>() * 2)
                    as u64,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (std::mem::size_of::<[f32; 2]>() * 2 + std::mem::size_of::<[f32; 4]>() * 3)
                    as u64,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (std::mem::size_of::<[f32; 2]>() * 2 + std::mem::size_of::<[f32; 4]>() * 4)
                    as u64,
                shader_location: 6,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (std::mem::size_of::<[f32; 2]>() * 2 + std::mem::size_of::<[f32; 4]>() * 5)
                    as u64,
                shader_location: 7,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (std::mem::size_of::<[f32; 2]>() * 2 + std::mem::size_of::<[f32; 4]>() * 6)
                    as u64,
                shader_location: 8,
            },
        ],
    }
}

fn polygon_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<NeoPolygonVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: std::mem::size_of::<[f32; 2]>() as u64,
                shader_location: 1,
            },
        ],
    }
}

fn image_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<NeoImageVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: std::mem::size_of::<[f32; 2]>() as u64,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (std::mem::size_of::<[f32; 2]>() * 2) as u64,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: (std::mem::size_of::<[f32; 2]>() * 2 + std::mem::size_of::<[f32; 4]>())
                    as u64,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (std::mem::size_of::<[f32; 2]>() * 3 + std::mem::size_of::<[f32; 4]>())
                    as u64,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: (std::mem::size_of::<[f32; 2]>() * 3 + std::mem::size_of::<[f32; 4]>() * 2)
                    as u64,
                shader_location: 5,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        backdrop_capture_rect, collect_draw_items, image_rect_and_uv, output_color, srgb_to_linear,
        PrimitiveKind, PrimitiveOp, RenderOp, UiDrawCommand, UiDrawList, UiRectDraw, UiTextDraw,
    };
    use crate::render::Color;
    use crate::ui::neo::{
        Border, Gradient, HorizontalAlign, ImageFit, LayoutRect, Shadow, Transform, VerticalAlign,
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
            font_family: String::new(),
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
