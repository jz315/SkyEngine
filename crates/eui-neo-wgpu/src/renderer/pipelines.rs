use crate::{default_neo_wgpu_shaders, NeoImageVertex, NeoPolygonVertex, NeoRectVertex};

const FULLSCREEN_SHADER: &str = r#"
struct FullscreenOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) idx: u32) -> FullscreenOutput {
    var out: FullscreenOutput;

    let x = f32(i32(idx & 1u) * 4 - 1);
    let y = f32(i32(idx >> 1u) * 4 - 1);

    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, 1.0 - (y + 1.0) * 0.5);
    return out;
}
"#;

/// Shared screen uniform used by Neo wgpu pipelines.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ScreenUniform {
    pub size: [f32; 4],
    pub backdrop_size: [f32; 4],
    pub backdrop_rect: [f32; 4],
}

/// WGSL sources used to build the standard Neo wgpu resources.
#[derive(Debug, Clone, Copy)]
pub struct NeoWgpuShaders<'a> {
    pub rect: &'a str,
    pub polygon: &'a str,
    pub image: &'a str,
    pub capture: &'a str,
}

/// Shared wgpu resources for rendering Neo draw-list primitives.
pub struct NeoWgpuResources {
    pub rect_pipeline: wgpu::RenderPipeline,
    pub capture_pipeline: wgpu::RenderPipeline,
    pub polygon_pipeline: wgpu::RenderPipeline,
    pub image_pipeline: wgpu::RenderPipeline,
    pub screen_buffer: wgpu::Buffer,
    pub screen_bind_group: wgpu::BindGroup,
    pub backdrop_texture_layout: wgpu::BindGroupLayout,
    pub image_texture_layout: wgpu::BindGroupLayout,
}

impl NeoWgpuResources {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_shaders(device, format, default_neo_wgpu_shaders())
    }

    pub fn new_with_shaders(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        shaders: NeoWgpuShaders<'_>,
    ) -> Self {
        let screen_buffer = create_screen_buffer(device);
        let screen_bind_group_layout = create_screen_bind_group_layout(device);
        let screen_bind_group =
            create_screen_bind_group(device, &screen_bind_group_layout, &screen_buffer);
        let backdrop_texture_layout = create_backdrop_texture_bind_group_layout(device);
        let image_texture_layout = create_image_texture_bind_group_layout(device);

        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("eui_neo_rect_shader"),
            source: wgpu::ShaderSource::Wgsl(shaders.rect.into()),
        });
        let polygon_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("eui_neo_polygon_shader"),
            source: wgpu::ShaderSource::Wgsl(shaders.polygon.into()),
        });
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("eui_neo_image_shader"),
            source: wgpu::ShaderSource::Wgsl(shaders.image.into()),
        });

        let rect_pipeline = create_rect_pipeline(
            device,
            format,
            &screen_bind_group_layout,
            &backdrop_texture_layout,
            &rect_shader,
        );
        let polygon_pipeline =
            create_polygon_pipeline(device, format, &screen_bind_group_layout, &polygon_shader);
        let capture_pipeline = create_fullscreen_pipeline(
            device,
            format,
            shaders.capture,
            "fs_main",
            &[&screen_bind_group_layout, &image_texture_layout],
            None,
            "eui_neo_backdrop_capture",
        );
        let image_pipeline = create_image_pipeline(
            device,
            format,
            &screen_bind_group_layout,
            &image_texture_layout,
            &image_shader,
        );

        Self {
            rect_pipeline,
            capture_pipeline,
            polygon_pipeline,
            image_pipeline,
            screen_buffer,
            screen_bind_group,
            backdrop_texture_layout,
            image_texture_layout,
        }
    }
}

pub fn create_screen_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("eui_neo_screen_uniform"),
        size: std::mem::size_of::<ScreenUniform>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub fn create_screen_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("eui_neo_screen_bgl"),
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
    })
}

pub fn create_screen_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("eui_neo_screen_bg"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    })
}

pub fn create_backdrop_texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    create_texture_bind_group_layout(device, "eui_neo_backdrop_texture_bgl")
}

pub fn create_image_texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    create_texture_bind_group_layout(device, "eui_neo_image_texture_bgl")
}

fn create_texture_bind_group_layout(
    device: &wgpu::Device,
    label: &'static str,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
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
    })
}

pub fn create_rect_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    screen_bind_group_layout: &wgpu::BindGroupLayout,
    backdrop_texture_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("eui_neo_rect_pipeline_layout"),
        bind_group_layouts: &[
            Some(screen_bind_group_layout),
            Some(backdrop_texture_layout),
        ],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("eui_neo_rect_pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[rect_vertex_layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
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
    })
}

pub fn create_polygon_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    screen_bind_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    let layout = create_single_screen_pipeline_layout(
        device,
        screen_bind_group_layout,
        "eui_neo_polygon_pipeline_layout",
    );
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("eui_neo_polygon_pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[polygon_vertex_layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
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
    })
}

pub fn create_image_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    screen_bind_group_layout: &wgpu::BindGroupLayout,
    image_texture_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("eui_neo_image_pipeline_layout"),
        bind_group_layouts: &[Some(screen_bind_group_layout), Some(image_texture_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("eui_neo_image_pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            buffers: &[image_vertex_layout()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
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
    })
}

pub fn create_fullscreen_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    fragment_source: &str,
    fragment_entry: &'static str,
    bind_group_layouts: &[&wgpu::BindGroupLayout],
    blend: Option<wgpu::BlendState>,
    label: &str,
) -> wgpu::RenderPipeline {
    let shader_label = format!("{label}_shader");
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&shader_label),
        source: wgpu::ShaderSource::Wgsl(compose_fullscreen_shader(fragment_source).into()),
    });

    let layout_label = format!("{label}_layout");
    let bind_group_layout_refs: Vec<_> = bind_group_layouts.iter().copied().map(Some).collect();
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&layout_label),
        bind_group_layouts: &bind_group_layout_refs,
        immediate_size: 0,
    });

    let pipeline_label = format!("{label}_pipeline");
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(&pipeline_label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_fullscreen"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(fragment_entry),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
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
    })
}

pub fn compose_fullscreen_shader(fragment_source: &str) -> String {
    format!("{FULLSCREEN_SHADER}\n{fragment_source}")
}

#[inline]
pub fn draw_fullscreen_triangle(pass: &mut wgpu::RenderPass<'_>) {
    pass.draw(0..3, 0..1);
}

fn create_single_screen_pipeline_layout(
    device: &wgpu::Device,
    screen_bind_group_layout: &wgpu::BindGroupLayout,
    label: &'static str,
) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(screen_bind_group_layout)],
        immediate_size: 0,
    })
}

pub fn rect_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const V2: u64 = std::mem::size_of::<[f32; 2]>() as u64;
    const V4: u64 = std::mem::size_of::<[f32; 4]>() as u64;

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
                offset: V2,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 2,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 2 + V4,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 2 + V4 * 2,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 2 + V4 * 3,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 2 + V4 * 4,
                shader_location: 6,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 2 + V4 * 5,
                shader_location: 7,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 2 + V4 * 6,
                shader_location: 8,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 2 + V4 * 7,
                shader_location: 9,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 2 + V4 * 8,
                shader_location: 10,
            },
        ],
    }
}

pub fn polygon_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const V2: u64 = std::mem::size_of::<[f32; 2]>() as u64;
    const V4: u64 = std::mem::size_of::<[f32; 4]>() as u64;

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
                offset: V2,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 + V4,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 + V4 * 2,
                shader_location: 3,
            },
        ],
    }
}

pub fn image_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const V2: u64 = std::mem::size_of::<[f32; 2]>() as u64;
    const V4: u64 = std::mem::size_of::<[f32; 4]>() as u64;

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
                offset: V2,
                shader_location: 1,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 2,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: V2 * 2 + V4,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 3 + V4,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 3 + V4 * 2,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 3 + V4 * 3,
                shader_location: 6,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: V2 * 3 + V4 * 4,
                shader_location: 7,
            },
        ],
    }
}
