use std::borrow::Cow;
use std::sync::Arc;

use glyphon::cosmic_text::Align as TextAlign;
use glyphon::{
    Attrs, Buffer, Cache, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer, Viewport,
};
use rustc_hash::FxHashMap;

use crate::asset::{AssetId, AssetServer, Handle, TextureAsset};
use crate::ecs::{EntityId, World};
use crate::gpu::GpuContext;
use crate::render::{Color as SkyColor, SharedRenderAssetCache};

use super::{
    resolve_world_layout, UiAlign, UiButton, UiFontBook, UiFontSource, UiImage, UiInteraction,
    UiNode, UiPanel, UiProgressBar, UiScroll, UiSlider, UiState, UiText, UiToggle,
};

const UI_SHADER: &str = r#"
struct Screen {
    size: vec2<f32>,
    _pad: vec2<f32>,
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
    let y = 1.0 - input.position.y / max(screen.size.y, 1.0) * 2.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

const UI_IMAGE_SHADER: &str = r#"
struct Screen {
    size: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> screen: Screen;

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

struct VsIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    let x = input.position.x / max(screen.size.x, 1.0) * 2.0 - 1.0;
    let y = 1.0 - input.position.y / max(screen.size.y, 1.0) * 2.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = input.uv;
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, input.uv) * input.color;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct UiVertex {
    position: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct UiImageVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenUniform {
    size: [f32; 2],
    _pad: [f32; 2],
}

#[derive(Clone)]
struct TextItem {
    text: String,
    rect: super::UiRect,
    clip: super::UiRect,
    color: SkyColor,
    font_size: f32,
    align: UiAlign,
}

#[derive(Clone, Copy)]
struct ImageItem {
    texture: Handle<TextureAsset>,
    vertices: [UiImageVertex; 6],
}

#[derive(Clone, Copy)]
enum DrawOp {
    Color { start: u32, count: u32 },
    Image { index: usize },
}

#[derive(Default)]
struct WidgetSnapshot {
    panel: Option<UiPanel>,
    image: Option<UiImage>,
    button: Option<UiButton>,
    progress: Option<UiProgressBar>,
    slider: Option<UiSlider>,
    toggle: Option<UiToggle>,
    text: Option<UiText>,
    scroll: Option<UiScroll>,
}

pub(crate) struct UiRenderer {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    screen_buffer: wgpu::Buffer,
    screen_bind_group: wgpu::BindGroup,
    image_texture_layout: wgpu::BindGroupLayout,
    image_bind_groups: FxHashMap<AssetId, CachedUiImageBindGroup>,
    font_revision: u64,
    font_system: FontSystem,
    swash_cache: SwashCache,
    _glyph_cache: Cache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    text_buffers: Vec<Buffer>,
}

struct CachedUiImageBindGroup {
    texture_key: usize,
    bind_group: wgpu::BindGroup,
}

impl UiRenderer {
    fn new(gpu: &GpuContext, font_book: Option<&UiFontBook>) -> Self {
        let device = gpu.device();
        let queue = gpu.queue();
        let format = gpu.surface_format();

        let screen_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky_ui_screen_uniform"),
            size: std::mem::size_of::<ScreenUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let screen_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sky_ui_screen_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky_ui_screen_bg"),
            layout: &screen_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_ui_quad_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(UI_SHADER)),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky_ui_quad_pipeline_layout"),
            bind_group_layouts: &[&screen_bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky_ui_quad_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiVertex>() as u64,
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
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
            multiview: None,
            cache: None,
        });
        let image_texture_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("sky_ui_image_texture_bgl"),
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
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_ui_image_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(UI_IMAGE_SHADER)),
        });
        let image_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sky_ui_image_pipeline_layout"),
                bind_group_layouts: &[&screen_bind_group_layout, &image_texture_layout],
                push_constant_ranges: &[],
            });
        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky_ui_image_pipeline"),
            layout: Some(&image_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiImageVertex>() as u64,
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
                    ],
                }],
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
            multiview: None,
            cache: None,
        });

        let (font_system, font_revision) = create_font_system(font_book);
        let swash_cache = SwashCache::new();
        let glyph_cache = Cache::new(device);
        let viewport = Viewport::new(device, &glyph_cache);
        let mut atlas = TextAtlas::new(device, queue, &glyph_cache, format);
        let text_renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        Self {
            format,
            pipeline,
            image_pipeline,
            screen_buffer,
            screen_bind_group,
            image_texture_layout,
            image_bind_groups: FxHashMap::default(),
            font_revision,
            font_system,
            swash_cache,
            _glyph_cache: glyph_cache,
            viewport,
            atlas,
            text_renderer,
            text_buffers: Vec::new(),
        }
    }

    fn matches_surface(&self, format: wgpu::TextureFormat) -> bool {
        self.format == format
    }

    fn sync_fonts(&mut self, font_book: Option<&UiFontBook>) {
        let revision = font_book.map(UiFontBook::revision).unwrap_or(0);
        if self.font_revision == revision {
            return;
        }
        let (font_system, font_revision) = create_font_system(font_book);
        self.font_system = font_system;
        self.font_revision = font_revision;
        self.text_buffers.clear();
    }

    fn prepare_text(
        &mut self,
        gpu: &GpuContext,
        text_items: &[TextItem],
        logical_size: [f32; 2],
    ) -> bool {
        let physical_size = gpu.surface_size();
        self.viewport.update(
            gpu.queue(),
            Resolution {
                width: physical_size[0].max(1),
                height: physical_size[1].max(1),
            },
        );

        self.text_buffers.clear();
        if text_items.is_empty() {
            return false;
        }

        let scale_x = physical_size[0] as f32 / logical_size[0].max(1.0);
        let scale_y = physical_size[1] as f32 / logical_size[1].max(1.0);
        let text_scale = scale_x.min(scale_y).max(0.01);

        for item in text_items {
            let font_size = (item.font_size * text_scale).max(1.0);
            let mut buffer = Buffer::new(
                &mut self.font_system,
                Metrics::new(font_size, font_size * 1.25),
            );
            buffer.set_size(
                &mut self.font_system,
                Some((item.rect.width * scale_x).max(1.0)),
                Some((item.rect.height * scale_y).max(1.0)),
            );
            buffer.set_text(
                &mut self.font_system,
                &item.text,
                Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
            );
            let align = text_align(item.align);
            for line in &mut buffer.lines {
                line.set_align(Some(align));
            }
            buffer.shape_until_scroll(&mut self.font_system, false);
            self.text_buffers.push(buffer);
        }

        let areas: Vec<_> = self
            .text_buffers
            .iter()
            .zip(text_items.iter())
            .map(|(buffer, item)| {
                let text_height =
                    laid_out_text_height(buffer).unwrap_or(item.font_size * text_scale * 1.25);
                let rect_height = item.rect.height * scale_y;
                let y_offset = ((rect_height - text_height) * 0.5).max(0.0);
                let left = item.rect.x * scale_x;
                let top = item.rect.y * scale_y + y_offset;
                TextArea {
                    buffer,
                    left,
                    top,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: (item.clip.x * scale_x).round() as i32,
                        top: (item.clip.y * scale_y).round() as i32,
                        right: (item.clip.right() * scale_x).round() as i32,
                        bottom: (item.clip.bottom() * scale_y).round() as i32,
                    },
                    default_color: glyph_color(item.color),
                    custom_glyphs: &[],
                }
            })
            .collect();

        match self.text_renderer.prepare(
            gpu.device(),
            gpu.queue(),
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        ) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("[SkyEngine] UI text prepare failed: {error}");
                false
            }
        }
    }

    fn image_bind_group(
        &mut self,
        gpu: &GpuContext,
        assets: Option<&AssetServer>,
        render_assets: Option<&SharedRenderAssetCache>,
        handle: Handle<TextureAsset>,
    ) -> Option<wgpu::BindGroup> {
        let id = handle.id();
        let texture = match (assets, render_assets) {
            (Some(assets), Some(cache)) => cache.borrow_mut().texture(gpu, assets, handle),
            (_, Some(cache)) => {
                cache.borrow_mut().mark_texture_missing(handle);
                None
            }
            _ => None,
        }?;
        let texture_key = std::ptr::from_ref(texture.texture()) as usize;

        if let Some(cached) = self.image_bind_groups.get(&id) {
            if cached.texture_key == texture_key {
                return Some(cached.bind_group.clone());
            }
        }

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky_ui_image_texture_bg"),
            layout: &self.image_texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(gpu.sampler_linear()),
                },
            ],
        });
        self.image_bind_groups.insert(
            id,
            CachedUiImageBindGroup {
                texture_key,
                bind_group: bind_group.clone(),
            },
        );
        Some(bind_group)
    }
}

/// Render UI as an overlay on the active surface frame.
pub fn render_ui(world: &mut World, gpu: &mut GpuContext) {
    if !gpu.has_surface() || !gpu.has_active_frame() {
        return;
    }
    super::ensure_legacy_ui_resources(world);

    let physical_size = gpu.surface_size();
    let state = world.get_resource::<UiState>().cloned().unwrap_or_default();
    let logical_size = {
        let size = state.surface_size();
        if size[0] > 0.0 && size[1] > 0.0 {
            size
        } else {
            [physical_size[0] as f32, physical_size[1] as f32]
        }
    };

    let resolved = resolve_world_layout(world, logical_size);
    let widgets = collect_widgets(world);
    let mut vertices = Vec::new();
    let mut images = Vec::new();
    let mut draw_ops = Vec::new();
    let mut text_items = Vec::new();
    let mut scrollbars = Vec::new();

    for node in &resolved {
        if !node.visible || node.clip_rect.is_empty() {
            continue;
        }
        let Some(widget) = widgets.get(&node.entity) else {
            continue;
        };
        if let Some(panel) = widget.panel {
            push_clipped_quad(
                &mut vertices,
                &mut draw_ops,
                node.rect,
                node.clip_rect,
                panel.color,
            );
        }
        if let Some(image) = widget.image {
            push_image_item(&mut images, &mut draw_ops, node.rect, node.clip_rect, image);
        }
        if let Some(button) = widget.button.as_ref() {
            let color = match state.interaction(node.entity) {
                UiInteraction::Pressed => button.pressed_color,
                UiInteraction::Hovered => button.hover_color,
                UiInteraction::Disabled => button.disabled_color,
                UiInteraction::None => button.normal_color,
            };
            push_clipped_quad(
                &mut vertices,
                &mut draw_ops,
                node.rect,
                node.clip_rect,
                color,
            );
            if !button.label.is_empty() {
                push_text_item(
                    &mut text_items,
                    button.label.clone(),
                    node.rect,
                    node.clip_rect,
                    button.text_color,
                    18.0,
                    UiAlign::Center,
                );
            }
        }
        if let Some(progress) = widget.progress {
            push_clipped_quad(
                &mut vertices,
                &mut draw_ops,
                node.rect,
                node.clip_rect,
                progress.background_color,
            );
            let fill = super::UiRect::new(
                node.rect.x,
                node.rect.y,
                node.rect.width * progress.fraction(),
                node.rect.height,
            );
            push_clipped_quad(
                &mut vertices,
                &mut draw_ops,
                fill,
                node.clip_rect,
                progress.fill_color,
            );
        }
        if let Some(slider) = widget.slider {
            push_slider(
                &mut vertices,
                &mut draw_ops,
                node.rect,
                node.clip_rect,
                slider,
                state.interaction(node.entity),
            );
        }
        if let Some(toggle) = widget.toggle.as_ref() {
            if let Some(text_item) = push_toggle(
                &mut vertices,
                &mut draw_ops,
                node.rect,
                node.clip_rect,
                toggle,
                state.interaction(node.entity),
            ) {
                text_items.push(text_item);
            }
        }
        if let Some(text) = widget.text.as_ref() {
            push_text_item(
                &mut text_items,
                text.text.clone(),
                node.rect,
                node.clip_rect,
                text.color,
                text.font_size,
                text.align,
            );
        }
        if let Some(scroll) = widget.scroll {
            if scroll.show_bars {
                scrollbars.push((node.rect, node.clip_rect, scroll));
            }
        }
    }
    for (rect, clip, scroll) in scrollbars {
        push_scrollbars(&mut vertices, &mut draw_ops, rect, clip, scroll);
    }

    if vertices.is_empty() && images.is_empty() && text_items.is_empty() {
        return;
    }

    let font_book = world.get_resource::<UiFontBook>().cloned();
    let asset_server = world.get_resource::<AssetServer>().cloned();
    if !world.contains_resource::<SharedRenderAssetCache>() {
        world.insert_resource(SharedRenderAssetCache::default());
    }
    let mut renderer = world
        .remove_resource::<UiRenderer>()
        .filter(|renderer| renderer.matches_surface(gpu.surface_format()))
        .unwrap_or_else(|| UiRenderer::new(gpu, font_book.as_ref()));
    renderer.sync_fonts(font_book.as_ref());

    let screen = ScreenUniform {
        size: logical_size,
        _pad: [0.0, 0.0],
    };
    gpu.queue()
        .write_buffer(&renderer.screen_buffer, 0, bytemuck::bytes_of(&screen));
    let vertex_upload = if vertices.is_empty() {
        None
    } else {
        Some(gpu.upload_vertices(&vertices))
    };
    let image_vertices: Vec<_> = images
        .iter()
        .flat_map(|item| item.vertices.into_iter())
        .collect();
    let image_upload = if image_vertices.is_empty() {
        None
    } else {
        Some(gpu.upload_vertices(&image_vertices))
    };
    let text_ready = renderer.prepare_text(gpu, &text_items, logical_size);
    let image_bind_groups: Vec<_> = {
        let render_assets = world.get_resource::<SharedRenderAssetCache>();
        let bind_groups: Vec<_> = images
            .iter()
            .map(|image| {
                renderer.image_bind_group(gpu, asset_server.as_ref(), render_assets, image.texture)
            })
            .collect();
        if let Some(cache) = render_assets {
            cache.borrow_mut().prepare_queued_textures(gpu);
        }
        bind_groups
    };

    {
        let mut frame = gpu.frame();
        let mut pass = frame.begin_surface_pass_loaded("sky_ui_overlay");
        let mut color_pipeline_bound = false;
        let mut image_pipeline_bound = false;
        for op in &draw_ops {
            match *op {
                DrawOp::Color { start, count } => {
                    let Some(upload) = vertex_upload.as_ref() else {
                        continue;
                    };
                    if !color_pipeline_bound {
                        pass.set_pipeline(&renderer.pipeline);
                        pass.set_bind_group(0, &renderer.screen_bind_group, &[]);
                        pass.set_vertex_buffer(0, upload.slice());
                        color_pipeline_bound = true;
                        image_pipeline_bound = false;
                    }
                    pass.draw(start..start + count, 0..1);
                }
                DrawOp::Image { index } => {
                    let (Some(upload), Some(Some(bind_group))) =
                        (image_upload.as_ref(), image_bind_groups.get(index))
                    else {
                        continue;
                    };
                    if !image_pipeline_bound {
                        pass.set_pipeline(&renderer.image_pipeline);
                        pass.set_bind_group(0, &renderer.screen_bind_group, &[]);
                        pass.set_vertex_buffer(0, upload.slice());
                        image_pipeline_bound = true;
                        color_pipeline_bound = false;
                    }
                    pass.set_bind_group(1, bind_group, &[]);
                    let start = index as u32 * 6;
                    pass.draw(start..start + 6, 0..1);
                }
            }
        }
        if text_ready {
            if let Err(error) =
                renderer
                    .text_renderer
                    .render(&renderer.atlas, &renderer.viewport, &mut pass)
            {
                eprintln!("[SkyEngine] UI text render failed: {error}");
            }
        }
    }

    renderer.atlas.trim();
    world.insert_resource(renderer);
}

fn collect_widgets(world: &World) -> FxHashMap<EntityId, WidgetSnapshot> {
    let mut query = world.query::<(
        &UiNode,
        Option<&UiPanel>,
        Option<&UiImage>,
        Option<&UiButton>,
        Option<&UiProgressBar>,
        Option<&UiSlider>,
        Option<&UiToggle>,
    )>();
    let mut widgets = FxHashMap::default();
    query.for_each_with_entity(
        world,
        |entity, (_node, panel, image, button, progress, slider, toggle)| {
            widgets.insert(
                entity,
                WidgetSnapshot {
                    panel: panel.copied(),
                    image: image.copied(),
                    button: button.cloned(),
                    progress: progress.copied(),
                    slider: slider.copied(),
                    toggle: toggle.cloned(),
                    text: None,
                    scroll: None,
                },
            );
        },
    );
    let mut text_query = world.query::<(&UiNode, Option<&UiText>, Option<&UiScroll>)>();
    text_query.for_each_with_entity(world, |entity, (_node, text, scroll)| {
        let widget = widgets.entry(entity).or_default();
        widget.text = text.cloned();
        widget.scroll = scroll.copied();
    });
    widgets
}

fn push_image_item(
    images: &mut Vec<ImageItem>,
    draw_ops: &mut Vec<DrawOp>,
    rect: super::UiRect,
    clip: super::UiRect,
    image: UiImage,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 || image.color.a <= 0.0 {
        return;
    }
    let Some((rect, uv_rect)) = clipped_image_rect(rect, clip, image.uv_rect) else {
        return;
    };
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }

    let c = image.color.to_array();
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.right();
    let y1 = rect.bottom();
    let [u0, v0, u1, v1] = uv_rect;
    let index = images.len();
    images.push(ImageItem {
        texture: image.texture,
        vertices: [
            UiImageVertex {
                position: [x0, y0],
                uv: [u0, v0],
                color: c,
            },
            UiImageVertex {
                position: [x1, y0],
                uv: [u1, v0],
                color: c,
            },
            UiImageVertex {
                position: [x1, y1],
                uv: [u1, v1],
                color: c,
            },
            UiImageVertex {
                position: [x0, y0],
                uv: [u0, v0],
                color: c,
            },
            UiImageVertex {
                position: [x1, y1],
                uv: [u1, v1],
                color: c,
            },
            UiImageVertex {
                position: [x0, y1],
                uv: [u0, v1],
                color: c,
            },
        ],
    });
    draw_ops.push(DrawOp::Image { index });
}

fn clipped_image_rect(
    rect: super::UiRect,
    clip: super::UiRect,
    uv: [f32; 4],
) -> Option<(super::UiRect, [f32; 4])> {
    let clipped = rect.intersection(clip)?;
    let left = ((clipped.x - rect.x) / rect.width).clamp(0.0, 1.0);
    let top = ((clipped.y - rect.y) / rect.height).clamp(0.0, 1.0);
    let right = ((clipped.right() - rect.x) / rect.width).clamp(0.0, 1.0);
    let bottom = ((clipped.bottom() - rect.y) / rect.height).clamp(0.0, 1.0);
    let [u0, v0, u1, v1] = uv;
    let du = u1 - u0;
    let dv = v1 - v0;
    Some((
        clipped,
        [
            u0 + du * left,
            v0 + dv * top,
            u0 + du * right,
            v0 + dv * bottom,
        ],
    ))
}

fn push_slider(
    vertices: &mut Vec<UiVertex>,
    draw_ops: &mut Vec<DrawOp>,
    rect: super::UiRect,
    clip: super::UiRect,
    slider: UiSlider,
    interaction: UiInteraction,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return;
    }
    let track_height = slider.track_height.min(rect.height).max(1.0);
    let track = super::UiRect::new(
        rect.x,
        rect.y + (rect.height - track_height) * 0.5,
        rect.width,
        track_height,
    );
    let disabled = interaction == UiInteraction::Disabled;
    push_clipped_quad(vertices, draw_ops, track, clip, slider.background_color);

    let fraction = slider.fraction();
    let fill = super::UiRect::new(track.x, track.y, track.width * fraction, track.height);
    push_clipped_quad(
        vertices,
        draw_ops,
        fill,
        clip,
        if disabled {
            slider.disabled_color
        } else {
            slider.fill_color
        },
    );

    let thumb_width = slider.thumb_width.min(rect.width).max(1.0);
    let thumb_height = slider.thumb_height.min(rect.height).max(track_height);
    let thumb_x = (track.x + track.width * fraction - thumb_width * 0.5)
        .clamp(rect.x, rect.right() - thumb_width);
    let thumb = super::UiRect::new(
        thumb_x,
        rect.y + (rect.height - thumb_height) * 0.5,
        thumb_width,
        thumb_height,
    );
    let thumb_color = match interaction {
        UiInteraction::Pressed => slider.pressed_thumb_color,
        UiInteraction::Hovered => slider.hover_thumb_color,
        UiInteraction::Disabled => slider.disabled_color,
        UiInteraction::None => slider.thumb_color,
    };
    push_clipped_quad(vertices, draw_ops, thumb, clip, thumb_color);
}

fn push_toggle(
    vertices: &mut Vec<UiVertex>,
    draw_ops: &mut Vec<DrawOp>,
    rect: super::UiRect,
    clip: super::UiRect,
    toggle: &UiToggle,
    interaction: UiInteraction,
) -> Option<TextItem> {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return None;
    }

    let track_width = toggle.track_width.min(rect.width).max(1.0);
    let track_height = toggle.track_height.min(rect.height).max(1.0);
    let label_gap = if toggle.label.is_empty() { 0.0 } else { 10.0 };
    let track_x = if toggle.label.is_empty() {
        rect.x + (rect.width - track_width) * 0.5
    } else {
        rect.x
    };
    let track = super::UiRect::new(
        track_x,
        rect.y + (rect.height - track_height) * 0.5,
        track_width,
        track_height,
    );

    let track_color = match interaction {
        UiInteraction::Disabled => toggle.disabled_color,
        UiInteraction::Pressed => toggle.pressed_color,
        UiInteraction::Hovered => {
            if toggle.checked {
                toggle.checked_color
            } else {
                toggle.hover_color
            }
        }
        UiInteraction::None => {
            if toggle.checked {
                toggle.checked_color
            } else {
                toggle.unchecked_color
            }
        }
    };
    push_clipped_quad(vertices, draw_ops, track, clip, track_color);

    let padding = toggle.knob_padding.max(0.0).min(track_height * 0.4);
    let knob_size = (track_height - padding * 2.0).max(1.0);
    let knob_x = if toggle.checked {
        track.right() - padding - knob_size
    } else {
        track.x + padding
    };
    let knob = super::UiRect::new(knob_x, track.y + padding, knob_size, knob_size);
    push_clipped_quad(vertices, draw_ops, knob, clip, toggle.knob_color);

    if toggle.label.is_empty() {
        return None;
    }
    let label_x = track.right() + label_gap;
    let rect = super::UiRect::new(
        label_x,
        rect.y,
        (rect.right() - label_x).max(0.0),
        rect.height,
    );
    rect.intersection(clip).map(|_| TextItem {
        text: toggle.label.clone(),
        rect,
        clip,
        color: toggle.text_color,
        font_size: 18.0,
        align: UiAlign::Start,
    })
}

fn push_text_item(
    text_items: &mut Vec<TextItem>,
    text: String,
    rect: super::UiRect,
    clip: super::UiRect,
    color: SkyColor,
    font_size: f32,
    align: UiAlign,
) {
    if rect.intersection(clip).is_some() {
        text_items.push(TextItem {
            text,
            rect,
            clip,
            color,
            font_size,
            align,
        });
    }
}

fn push_scrollbars(
    vertices: &mut Vec<UiVertex>,
    draw_ops: &mut Vec<DrawOp>,
    rect: super::UiRect,
    clip: super::UiRect,
    scroll: UiScroll,
) {
    let max_offset = scroll.max_offset(rect);
    if scroll.vertical && max_offset[1] > 0.0 {
        let track = super::UiRect::new(rect.right() - 5.0, rect.y + 4.0, 3.0, rect.height - 8.0);
        if track.height > 0.0 {
            let min_thumb = 18.0_f32.min(track.height);
            let thumb_h = (track.height * (rect.height / scroll.content_size[1].max(1.0)))
                .clamp(min_thumb, track.height);
            let travel = (track.height - thumb_h).max(0.0);
            let y = track.y + travel * (scroll.offset[1] / max_offset[1]);
            push_clipped_quad(
                vertices,
                draw_ops,
                track,
                clip,
                SkyColor::rgba8(12, 18, 24, 170),
            );
            push_clipped_quad(
                vertices,
                draw_ops,
                super::UiRect::new(track.x, y, track.width, thumb_h),
                clip,
                SkyColor::rgba8(210, 230, 245, 190),
            );
        }
    }
    if scroll.horizontal && max_offset[0] > 0.0 {
        let track = super::UiRect::new(rect.x + 4.0, rect.bottom() - 5.0, rect.width - 8.0, 3.0);
        if track.width > 0.0 {
            let min_thumb = 18.0_f32.min(track.width);
            let thumb_w = (track.width * (rect.width / scroll.content_size[0].max(1.0)))
                .clamp(min_thumb, track.width);
            let travel = (track.width - thumb_w).max(0.0);
            let x = track.x + travel * (scroll.offset[0] / max_offset[0]);
            push_clipped_quad(
                vertices,
                draw_ops,
                track,
                clip,
                SkyColor::rgba8(12, 18, 24, 170),
            );
            push_clipped_quad(
                vertices,
                draw_ops,
                super::UiRect::new(x, track.y, thumb_w, track.height),
                clip,
                SkyColor::rgba8(210, 230, 245, 190),
            );
        }
    }
}

fn push_clipped_quad(
    vertices: &mut Vec<UiVertex>,
    draw_ops: &mut Vec<DrawOp>,
    rect: super::UiRect,
    clip: super::UiRect,
    color: SkyColor,
) {
    if let Some(rect) = rect.intersection(clip) {
        push_quad(vertices, draw_ops, rect, color);
    }
}

fn push_quad(
    vertices: &mut Vec<UiVertex>,
    draw_ops: &mut Vec<DrawOp>,
    rect: super::UiRect,
    color: SkyColor,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 || color.a <= 0.0 {
        return;
    }
    let start = vertices.len() as u32;
    let c = color.to_array();
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.right();
    let y1 = rect.bottom();
    vertices.extend_from_slice(&[
        UiVertex {
            position: [x0, y0],
            color: c,
        },
        UiVertex {
            position: [x1, y0],
            color: c,
        },
        UiVertex {
            position: [x1, y1],
            color: c,
        },
        UiVertex {
            position: [x0, y0],
            color: c,
        },
        UiVertex {
            position: [x1, y1],
            color: c,
        },
        UiVertex {
            position: [x0, y1],
            color: c,
        },
    ]);
    draw_ops.push(DrawOp::Color { start, count: 6 });
}

fn glyph_color(color: SkyColor) -> glyphon::Color {
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

fn text_align(align: UiAlign) -> TextAlign {
    match align {
        UiAlign::Start | UiAlign::Stretch => TextAlign::Left,
        UiAlign::Center => TextAlign::Center,
        UiAlign::End => TextAlign::End,
    }
}

fn laid_out_text_height(buffer: &Buffer) -> Option<f32> {
    buffer
        .layout_runs()
        .map(|run| run.line_top + run.line_height)
        .reduce(f32::max)
}

fn create_font_system(font_book: Option<&UiFontBook>) -> (FontSystem, u64) {
    let use_system_fonts = font_book
        .map(|book| {
            book.sources()
                .iter()
                .any(|source| matches!(source, UiFontSource::SystemFonts))
        })
        .unwrap_or(true);
    let mut font_system = if use_system_fonts {
        FontSystem::new()
    } else {
        FontSystem::new_with_locale_and_db("en-US".to_string(), glyphon::fontdb::Database::new())
    };
    let revision = apply_font_book(&mut font_system, font_book);
    (font_system, revision)
}

fn apply_font_book(font_system: &mut FontSystem, font_book: Option<&UiFontBook>) -> u64 {
    let Some(font_book) = font_book else {
        return 0;
    };
    for source in font_book.sources() {
        match source {
            UiFontSource::SystemFonts => {}
            UiFontSource::Bytes { bytes, .. } => {
                font_system
                    .db_mut()
                    .load_font_source(glyphon::fontdb::Source::Binary(Arc::new(bytes.clone())));
            }
        }
    }
    font_book.revision()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipped_image_rect_preserves_uv_scale_when_clipped() {
        let rect = super::super::UiRect::new(100.0, 50.0, 200.0, 100.0);
        let clip = super::super::UiRect::new(150.0, 75.0, 100.0, 50.0);

        let (clipped, uv) =
            clipped_image_rect(rect, clip, [0.2, 0.1, 0.8, 0.9]).expect("rect should overlap");

        assert_eq!(clipped, clip);
        assert_slice_near(uv, [0.35, 0.3, 0.65, 0.7]);
    }

    #[test]
    fn clipped_image_rect_returns_none_without_overlap() {
        let rect = super::super::UiRect::new(0.0, 0.0, 10.0, 10.0);
        let clip = super::super::UiRect::new(20.0, 20.0, 5.0, 5.0);

        assert!(clipped_image_rect(rect, clip, [0.0, 0.0, 1.0, 1.0]).is_none());
    }

    fn assert_slice_near(actual: [f32; 4], expected: [f32; 4]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.001,
                "expected {expected}, got {actual}"
            );
        }
    }
}
