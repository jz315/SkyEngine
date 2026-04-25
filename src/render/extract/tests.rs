use std::{fs, sync::mpsc};

use rustc_hash::FxHashMap;

use crate::asset::{AssetConfig, AssetServer, TextureAsset};
use crate::gpu::GpuContext;
use crate::render::gpu::{RenderTarget, Texture, TextureCreateDesc};
use crate::render::lighting::shadow::{
    create_shadow_compare_sampler, ShadowPassBindingLayout, ShadowSceneBindingLayout,
    ShadowViewBinding,
};
use crate::render::phase::{
    create_model_bind_group_layout, DrawContext, DrawFunctionRegistry, DrawMesh, DrawSprite,
    OpaquePhase, PhaseItem, SpriteDrawData, TransparentPhase,
};
use crate::render::view::{Projection, SceneView};
use crate::render::{
    expert::{Mesh, MeshRegistry},
    Color, GpuScene, LightTable, MeshRenderer, ModelMatrixTable, OrderInLayer, RenderComposer,
    RenderPipelineAsset, SortingLayer, SpriteMaterial, SpriteRenderer, StandardMaterial, Transform,
    UnlitMaterial, ViewportRect, DEFAULT_DEPTH_FORMAT,
};
use wgpu::util::DeviceExt;

use super::*;

fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for extract tests");

    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("extract_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .expect("Failed to create test GPU device")
}

fn make_view(size: [u32; 2]) -> SceneView {
    let projection = Projection::orthographic_fixed(size[0] as f32, size[1] as f32);
    SceneView::new(
        0,
        ViewportRect::from_surface_size(size),
        size,
        false,
        u32::MAX,
        Transform::default(),
        projection,
        projection.view_uniform(Transform::default(), size),
        true,
    )
}

fn read_pixel_rgba8(
    ctx: &GpuContext,
    target: &RenderTarget,
    buffer: &wgpu::Buffer,
    x: u32,
    y: u32,
) -> [u8; 4] {
    let slice = buffer.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender
            .send(result.map(|_| ()))
            .expect("map_async callback should send result");
    });
    ctx.device().poll(wgpu::Maintain::Wait);
    receiver
        .recv()
        .expect("map_async callback should run")
        .expect("readback buffer should map");

    let data = slice.get_mapped_range();
    let row_pitch = align_to(target.width() * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let offset = (y * row_pitch + x * 4) as usize;
    let pixel = [
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ];
    drop(data);
    buffer.unmap();
    assert!(row_pitch >= 4);
    pixel
}

fn align_to(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

fn assign_model_slots(
    draw_functions: &mut DrawFunctionRegistry,
    phases: &mut [&mut [PhaseItem]],
    transforms: &crate::render::view::ResolvedSceneTransforms,
) -> Vec<[f32; 16]> {
    let mut entity_to_slot = FxHashMap::default();
    let mut model_matrices = vec![IDENTITY_MODEL_MATRIX];
    for items in phases {
        draw_functions.assign_model_matrices(
            items,
            transforms,
            &mut entity_to_slot,
            &mut model_matrices,
        );
    }
    model_matrices
}

const IDENTITY_MODEL_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

fn write_test_gltf(
    dir: &std::path::Path,
    positions_a: &[[f32; 3]],
    positions_b: &[[f32; 3]],
) -> std::path::PathBuf {
    fn push_aligned(buffer: &mut Vec<u8>, align: usize) {
        while buffer.len() % align != 0 {
            buffer.push(0);
        }
    }

    let gltf_path = dir.join("test_mesh.gltf");
    let bin_path = dir.join("test_mesh.bin");
    let normals_a = vec![[0.0f32, 0.0, 1.0]; positions_a.len()];
    let normals_b = vec![[0.0f32, 0.0, 1.0]; positions_b.len()];
    let uvs_a = vec![[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let uvs_b = vec![[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let indices_a = [0u16, 1, 2];
    let indices_b = [0u16, 1, 2];

    let mut bin = Vec::new();
    let pos_a_offset = bin.len();
    bin.extend_from_slice(bytemuck::cast_slice(positions_a));
    push_aligned(&mut bin, 4);
    let norm_a_offset = bin.len();
    bin.extend_from_slice(bytemuck::cast_slice(&normals_a));
    push_aligned(&mut bin, 4);
    let uv_a_offset = bin.len();
    bin.extend_from_slice(bytemuck::cast_slice(&uvs_a));
    push_aligned(&mut bin, 4);
    let idx_a_offset = bin.len();
    bin.extend_from_slice(bytemuck::cast_slice(&indices_a));
    push_aligned(&mut bin, 4);

    let pos_b_offset = bin.len();
    bin.extend_from_slice(bytemuck::cast_slice(positions_b));
    push_aligned(&mut bin, 4);
    let norm_b_offset = bin.len();
    bin.extend_from_slice(bytemuck::cast_slice(&normals_b));
    push_aligned(&mut bin, 4);
    let uv_b_offset = bin.len();
    bin.extend_from_slice(bytemuck::cast_slice(&uvs_b));
    push_aligned(&mut bin, 4);
    let idx_b_offset = bin.len();
    bin.extend_from_slice(bytemuck::cast_slice(&indices_b));
    push_aligned(&mut bin, 4);

    fs::write(&bin_path, &bin).expect("binary gltf buffer should be written");

    let min_a = positions_a.iter().fold(positions_a[0], |mut min, point| {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
        }
        min
    });
    let max_a = positions_a.iter().fold(positions_a[0], |mut max, point| {
        for axis in 0..3 {
            max[axis] = max[axis].max(point[axis]);
        }
        max
    });
    let min_b = positions_b.iter().fold(positions_b[0], |mut min, point| {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
        }
        min
    });
    let max_b = positions_b.iter().fold(positions_b[0], |mut max, point| {
        for axis in 0..3 {
            max[axis] = max[axis].max(point[axis]);
        }
        max
    });

    let json = format!(
        r#"{{
  "asset": {{ "version": "2.0" }},
  "buffers": [
    {{ "byteLength": {buffer_len}, "uri": "test_mesh.bin" }}
  ],
  "bufferViews": [
    {{ "buffer": 0, "byteOffset": {pos_a_offset}, "byteLength": 36 }},
    {{ "buffer": 0, "byteOffset": {norm_a_offset}, "byteLength": 36 }},
    {{ "buffer": 0, "byteOffset": {uv_a_offset}, "byteLength": 24 }},
    {{ "buffer": 0, "byteOffset": {idx_a_offset}, "byteLength": 6 }},
    {{ "buffer": 0, "byteOffset": {pos_b_offset}, "byteLength": 36 }},
    {{ "buffer": 0, "byteOffset": {norm_b_offset}, "byteLength": 36 }},
    {{ "buffer": 0, "byteOffset": {uv_b_offset}, "byteLength": 24 }},
    {{ "buffer": 0, "byteOffset": {idx_b_offset}, "byteLength": 6 }}
  ],
  "accessors": [
    {{ "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [{min_ax}, {min_ay}, {min_az}], "max": [{max_ax}, {max_ay}, {max_az}] }},
    {{ "bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3" }},
    {{ "bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC2" }},
    {{ "bufferView": 3, "componentType": 5123, "count": 3, "type": "SCALAR" }},
    {{ "bufferView": 4, "componentType": 5126, "count": 3, "type": "VEC3", "min": [{min_bx}, {min_by}, {min_bz}], "max": [{max_bx}, {max_by}, {max_bz}] }},
    {{ "bufferView": 5, "componentType": 5126, "count": 3, "type": "VEC3" }},
    {{ "bufferView": 6, "componentType": 5126, "count": 3, "type": "VEC2" }},
    {{ "bufferView": 7, "componentType": 5123, "count": 3, "type": "SCALAR" }}
  ],
  "materials": [{{}}, {{}}],
  "meshes": [
    {{
      "primitives": [
        {{
          "attributes": {{ "POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2 }},
          "indices": 3,
          "material": 0
        }},
        {{
          "attributes": {{ "POSITION": 4, "NORMAL": 5, "TEXCOORD_0": 6 }},
          "indices": 7,
          "material": 1
        }}
      ]
    }}
  ],
  "nodes": [{{ "mesh": 0 }}],
  "scenes": [{{ "nodes": [0] }}],
  "scene": 0
}}"#,
        buffer_len = bin.len(),
        pos_a_offset = pos_a_offset,
        norm_a_offset = norm_a_offset,
        uv_a_offset = uv_a_offset,
        idx_a_offset = idx_a_offset,
        pos_b_offset = pos_b_offset,
        norm_b_offset = norm_b_offset,
        uv_b_offset = uv_b_offset,
        idx_b_offset = idx_b_offset,
        min_ax = min_a[0],
        min_ay = min_a[1],
        min_az = min_a[2],
        max_ax = max_a[0],
        max_ay = max_a[1],
        max_az = max_a[2],
        min_bx = min_b[0],
        min_by = min_b[1],
        min_bz = min_b[2],
        max_bx = max_b[0],
        max_by = max_b[1],
        max_bz = max_b[2],
    );
    fs::write(&gltf_path, json).expect("gltf json should be written");
    gltf_path
}

#[test]
fn sprite_extract_schedule_renders_through_transparent_phase() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.register_material::<SpriteMaterial>(&ctx);

    let mut mesh_registry = MeshRegistry::default();
    let quad_mesh_handle = mesh_registry.ensure_builtin_quad(&ctx);
    let mut draw_functions = DrawFunctionRegistry::new();
    let draw_mesh = draw_functions.register(DrawSprite::new());
    let mut schedule = ExtractSchedule::new();
    schedule.add(ExtractSprites::new(draw_mesh));

    let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
    let white = asset_server.insert_runtime(TextureAsset::white_pixel());

    let mut world = crate::ecs::World::new();
    world.insert_resource(asset_server.clone());
    world.spawn((
        Transform::from_xy(0.0, 0.0),
        SpriteRenderer::new(32.0, 32.0)
            .color(Color::RED)
            .texture(white.clone()),
        SortingLayer(0),
        OrderInLayer(0),
    ));

    let transforms = renderer.resolve_scene_transforms(&world);
    let view = make_view([64, 64]);
    let mut opaque_phase = OpaquePhase::new();
    let mut phase = TransparentPhase::new();
    schedule
        .extract(
            &world,
            &transforms,
            &view,
            &mut ExtractContext {
                gpu: &ctx,
                asset_server: Some(&asset_server),
                render_assets: &mut renderer.runtime.render_assets,
                material_registry: &mut renderer.resources.material_registry,
                mesh_registry: &mesh_registry,
                opaque_phase: &mut opaque_phase,
                transparent_phase: &mut phase,
                quad_mesh_handle,
            },
        )
        .expect("sprite extraction should succeed");

    assert_eq!(phase.len(), 1);
    phase.sort();
    let model_matrices = {
        let items = phase.items_mut();
        assign_model_slots(&mut draw_functions, &mut [items], &transforms)
    };

    let target = RenderTarget::new(&ctx, 64, 64, wgpu::TextureFormat::Rgba8Unorm, "phase");
    let readback_pitch = align_to(64 * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let readback = ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("phase_readback"),
        size: (readback_pitch * 64) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let view_layout = ctx
        .device()
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("phase_view_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<
                            crate::render::view::ViewUniform,
                        >() as u64)
                        .expect("ViewUniform has non-zero size"),
                    ),
                },
                count: None,
            }],
        });
    let view_buffer = ctx
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("phase_view_uniform"),
            contents: bytemuck::bytes_of(&view.view_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
    let view_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("phase_view_bg"),
        layout: &view_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: view_buffer.as_entire_binding(),
        }],
    });

    let fallback = Texture::white_pixel(&ctx);
    let device = ctx.device().clone();
    let sampler_linear = ctx.sampler_linear().clone();
    let sampler_nearest = ctx.sampler_nearest().clone();
    let model_layout = create_model_bind_group_layout(ctx.device());
    ctx.begin_frame()
        .expect("headless frame should begin for transparent phase rendering");
    {
        let mut frame = ctx.frame();
        let mut pass = frame.begin_target_pass(
            "transparent_phase",
            &target,
            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        );
        let mut draw_ctx = DrawContext::new(
            &device,
            &sampler_linear,
            &sampler_nearest,
            &mut pass,
            &view_bind_group,
            &view_layout,
            &model_layout,
            Some(&model_matrices),
            None,
            None,
            None,
            &mut renderer.resources.material_registry,
            &mesh_registry,
            Some(&fallback),
            target.format(),
            None,
        );
        phase
            .render(&mut draw_functions, &mut draw_ctx)
            .expect("transparent phase should render extracted sprite");
    }
    ctx.encoder().copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: target.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(readback_pitch),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    ctx.end_frame();

    let pixel = read_pixel_rgba8(&ctx, &target, &readback, 32, 32);
    assert!(
        pixel[0] > 0,
        "rendered sprite should contribute red channel"
    );
    assert!(pixel[3] > 0, "rendered sprite should contribute alpha");
}

#[test]
fn sprite_extract_keeps_material_handles_valid_across_multiple_views() {
    let (device, queue) = create_test_device();
    let ctx = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.register_material::<SpriteMaterial>(&ctx);

    let mut mesh_registry = MeshRegistry::default();
    let quad_mesh_handle = mesh_registry.ensure_builtin_quad(&ctx);
    let mut draw_functions = DrawFunctionRegistry::new();
    let draw_sprite = draw_functions.register(DrawSprite::new());
    let mut schedule = ExtractSchedule::new();
    schedule.add(ExtractSprites::new(draw_sprite));

    let mut world = crate::ecs::World::new();
    world.spawn((
        Transform::from_xy(0.0, 0.0),
        SpriteRenderer::new(16.0, 16.0).color(Color::RED),
    ));

    let transforms = renderer.resolve_scene_transforms(&world);
    let first_view = make_view([64, 64]);
    let second_view = make_view([64, 64]);
    let mut first_opaque = OpaquePhase::new();
    let mut first_transparent = TransparentPhase::new();
    schedule
        .extract(
            &world,
            &transforms,
            &first_view,
            &mut ExtractContext {
                gpu: &ctx,
                asset_server: None,
                render_assets: &mut renderer.runtime.render_assets,
                material_registry: &mut renderer.resources.material_registry,
                mesh_registry: &mesh_registry,
                opaque_phase: &mut first_opaque,
                transparent_phase: &mut first_transparent,
                quad_mesh_handle,
            },
        )
        .expect("first view sprite extraction should succeed");
    let first_material = first_transparent.items()[0]
        .data::<SpriteDrawData>()
        .material_handle();

    let mut second_opaque = OpaquePhase::new();
    let mut second_transparent = TransparentPhase::new();
    schedule
        .extract(
            &world,
            &transforms,
            &second_view,
            &mut ExtractContext {
                gpu: &ctx,
                asset_server: None,
                render_assets: &mut renderer.runtime.render_assets,
                material_registry: &mut renderer.resources.material_registry,
                mesh_registry: &mesh_registry,
                opaque_phase: &mut second_opaque,
                transparent_phase: &mut second_transparent,
                quad_mesh_handle,
            },
        )
        .expect("second view sprite extraction should succeed");

    assert!(renderer
        .materials::<SpriteMaterial>()
        .get(first_material)
        .is_some());
}

#[test]
fn mesh_extract_schedule_renders_opaque_phase_with_depth() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.register_material::<UnlitMaterial>(&ctx);

    let white = Texture::create(
        &ctx,
        TextureCreateDesc::new_2d(1, 1, wgpu::TextureFormat::Rgba8Unorm)
            .usage(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST)
            .label("mesh_white"),
    );
    ctx.queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: white.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255, 255, 255, 255],
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

    let red = renderer.materials_mut::<UnlitMaterial>().insert(
        UnlitMaterial::default()
            .color(Color::RED)
            .texture(white.clone()),
    );
    let blue = renderer.materials_mut::<UnlitMaterial>().insert(
        UnlitMaterial::default()
            .color(Color::BLUE)
            .texture(white.clone()),
    );
    let mesh_handle = renderer.insert_mesh(Mesh::builtin_quad(&ctx));

    let mut world = crate::ecs::World::new();
    world.spawn((
        Transform::from_xyz(-16.0, -16.0, 0.2).with_scale(32.0, 32.0),
        MeshRenderer::new(mesh_handle, red),
    ));
    world.spawn((
        Transform::from_xyz(-16.0, -16.0, 0.8).with_scale(32.0, 32.0),
        MeshRenderer::new(mesh_handle, blue),
    ));

    let transforms = renderer.resolve_scene_transforms(&world);
    let view = make_view([64, 64]);
    let mut opaque_phase = OpaquePhase::new();
    let mut transparent_phase = TransparentPhase::new();
    let quad_mesh_handle = renderer.resources.mesh_registry.ensure_builtin_quad(&ctx);
    let mut schedule = ExtractSchedule::new();
    let mut draw_functions = DrawFunctionRegistry::new();
    let draw_mesh = draw_functions.register(DrawMesh::<UnlitMaterial>::new());
    schedule.add(ExtractMeshes::<UnlitMaterial>::new(draw_mesh));
    schedule
        .extract(
            &world,
            &transforms,
            &view,
            &mut ExtractContext {
                gpu: &ctx,
                asset_server: None,
                render_assets: &mut renderer.runtime.render_assets,
                material_registry: &mut renderer.resources.material_registry,
                mesh_registry: &renderer.resources.mesh_registry,
                opaque_phase: &mut opaque_phase,
                transparent_phase: &mut transparent_phase,
                quad_mesh_handle,
            },
        )
        .expect("mesh extraction should succeed");

    assert_eq!(opaque_phase.len(), 2);
    assert!(transparent_phase.is_empty());
    opaque_phase.sort();
    let model_matrices = {
        let items = opaque_phase.items_mut();
        assign_model_slots(&mut draw_functions, &mut [items], &transforms)
    };

    let target = RenderTarget::new(&ctx, 64, 64, wgpu::TextureFormat::Rgba8Unorm, "mesh_phase");
    let depth_target = RenderTarget::new_depth(&ctx, 64, 64);
    let readback_pitch = align_to(64 * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let readback = ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("mesh_phase_readback"),
        size: (readback_pitch * 64) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let view_layout = ctx
        .device()
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh_phase_view_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<
                            crate::render::view::ViewUniform,
                        >() as u64)
                        .expect("ViewUniform has non-zero size"),
                    ),
                },
                count: None,
            }],
        });
    let view_buffer = ctx
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh_phase_view_uniform"),
            contents: bytemuck::bytes_of(&view.view_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
    let view_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mesh_phase_view_bg"),
        layout: &view_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: view_buffer.as_entire_binding(),
        }],
    });

    let fallback = Texture::white_pixel(&ctx);
    let device = ctx.device().clone();
    let sampler_linear = ctx.sampler_linear().clone();
    let sampler_nearest = ctx.sampler_nearest().clone();
    let model_layout = create_model_bind_group_layout(ctx.device());
    ctx.begin_frame()
        .expect("headless frame should begin for opaque mesh rendering");
    {
        let mut frame = ctx.frame();
        let color_attachment = [Some(wgpu::RenderPassColorAttachment {
            view: target.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })];
        let depth_attachment = wgpu::RenderPassDepthStencilAttachment {
            view: depth_target.view(),
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        };
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mesh_opaque_phase"),
            color_attachments: &color_attachment,
            depth_stencil_attachment: Some(depth_attachment),
            ..Default::default()
        });
        let mut draw_ctx = DrawContext::new(
            &device,
            &sampler_linear,
            &sampler_nearest,
            &mut pass,
            &view_bind_group,
            &view_layout,
            &model_layout,
            Some(&model_matrices),
            None,
            None,
            None,
            &mut renderer.resources.material_registry,
            &renderer.resources.mesh_registry,
            Some(&fallback),
            target.format(),
            Some(DEFAULT_DEPTH_FORMAT),
        );
        opaque_phase
            .render(&mut draw_functions, &mut draw_ctx)
            .expect("opaque phase should render extracted meshes");
    }
    ctx.encoder().copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: target.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(readback_pitch),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    ctx.end_frame();

    let pixel = read_pixel_rgba8(&ctx, &target, &readback, 32, 32);
    assert!(
        pixel[0] > pixel[2],
        "near red mesh should occlude far blue mesh, got pixel {pixel:?}"
    );
    assert!(pixel[3] > 0, "opaque mesh should contribute alpha");
}

#[test]
fn gltf_mesh_extract_schedule_renders_submeshes_with_material_slots_and_depth() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [64, 64]);
    let mut renderer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
    renderer.register_material::<StandardMaterial>(&ctx);

    let white = Texture::create(
        &ctx,
        TextureCreateDesc::new_2d(1, 1, wgpu::TextureFormat::Rgba8Unorm)
            .usage(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST)
            .label("standard_white"),
    );
    ctx.queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: white.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255, 255, 255, 255],
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

    let temp_dir = tempfile::tempdir().expect("tempdir should be created");
    let gltf_path = write_test_gltf(
        temp_dir.path(),
        &[[0.0, 0.0, 0.2], [0.0, 1.0, 0.2], [1.0, 0.0, 0.2]],
        &[[0.0, 0.0, 0.8], [0.0, 1.0, 0.8], [1.0, 0.0, 0.8]],
    );
    let mesh_handle = renderer
        .insert_mesh(Mesh::from_gltf(&ctx, &gltf_path).expect("temporary gltf mesh should load"));

    let red = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial {
            albedo: Color::RED,
            albedo_texture: Some(white.clone()),
            ..Default::default()
        });
    let blue = renderer
        .materials_mut::<StandardMaterial>()
        .insert(StandardMaterial {
            albedo: Color::BLUE,
            albedo_texture: Some(white.clone()),
            ..Default::default()
        });

    let mut world = crate::ecs::World::new();
    world.spawn((
        Transform::from_xyz(16.0, -16.0, 0.0).with_scale(-32.0, 32.0),
        MeshRenderer::new(mesh_handle, red).materials(vec![red, blue]),
    ));

    let transforms = renderer.resolve_scene_transforms(&world);
    let view = make_view([64, 64]);
    let mut opaque_phase = OpaquePhase::new();
    let mut transparent_phase = TransparentPhase::new();
    let quad_mesh_handle = renderer.resources.mesh_registry.ensure_builtin_quad(&ctx);
    let mut schedule = ExtractSchedule::new();
    let mut draw_functions = DrawFunctionRegistry::new();
    let draw_mesh = draw_functions.register(DrawMesh::<StandardMaterial>::new());
    schedule.add(ExtractMeshes::<StandardMaterial>::new(draw_mesh));
    schedule
        .extract(
            &world,
            &transforms,
            &view,
            &mut ExtractContext {
                gpu: &ctx,
                asset_server: None,
                render_assets: &mut renderer.runtime.render_assets,
                material_registry: &mut renderer.resources.material_registry,
                mesh_registry: &renderer.resources.mesh_registry,
                opaque_phase: &mut opaque_phase,
                transparent_phase: &mut transparent_phase,
                quad_mesh_handle,
            },
        )
        .expect("gltf mesh extraction should succeed");

    assert_eq!(opaque_phase.len(), 2);
    assert!(transparent_phase.is_empty());
    opaque_phase.sort();

    let target = RenderTarget::new(&ctx, 64, 64, wgpu::TextureFormat::Rgba8Unorm, "gltf_phase");
    let depth_target = RenderTarget::new_depth(&ctx, 64, 64);
    let readback_pitch = align_to(64 * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let readback = ctx.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("gltf_phase_readback"),
        size: (readback_pitch * 64) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let view_layout = ctx
        .device()
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gltf_phase_view_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<
                            crate::render::view::ViewUniform,
                        >() as u64)
                        .expect("ViewUniform has non-zero size"),
                    ),
                },
                count: None,
            }],
        });
    let view_buffer = ctx
        .device()
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gltf_phase_view_uniform"),
            contents: bytemuck::bytes_of(&view.view_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
    let view_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gltf_phase_view_bg"),
        layout: &view_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: view_buffer.as_entire_binding(),
        }],
    });

    let fallback = Texture::white_pixel(&ctx);
    let device = ctx.device().clone();
    let sampler_linear = ctx.sampler_linear().clone();
    let sampler_nearest = ctx.sampler_nearest().clone();
    let model_layout = create_model_bind_group_layout(ctx.device());
    let mut gpu_scene = GpuScene::new(&ctx);
    let shadow_layout = ShadowSceneBindingLayout::new(ctx.device());
    let shadow_pass_layout = ShadowPassBindingLayout::new(ctx.device());
    let shadow_sampler = create_shadow_compare_sampler(ctx.device());
    let model_matrices = {
        let items = opaque_phase.items_mut();
        assign_model_slots(&mut draw_functions, &mut [items], &transforms)
    };
    gpu_scene
        .table_mut::<ModelMatrixTable>()
        .set_all(&ctx, &model_matrices);
    gpu_scene.table_mut::<LightTable>().set_all(&ctx, &[]);
    gpu_scene.upload_all(ctx.queue());
    let shadow_view = ShadowViewBinding::new(
        &ctx,
        &shadow_layout,
        &shadow_pass_layout,
        &shadow_sampler,
        gpu_scene.table::<LightTable>(),
    );
    ctx.begin_frame()
        .expect("headless frame should begin for gltf mesh rendering");
    {
        let mut frame = ctx.frame();
        let color_attachment = [Some(wgpu::RenderPassColorAttachment {
            view: target.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })];
        let depth_attachment = wgpu::RenderPassDepthStencilAttachment {
            view: depth_target.view(),
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        };
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("gltf_opaque_phase"),
            color_attachments: &color_attachment,
            depth_stencil_attachment: Some(depth_attachment),
            ..Default::default()
        });
        let mut draw_ctx = DrawContext::new(
            &device,
            &sampler_linear,
            &sampler_nearest,
            &mut pass,
            &view_bind_group,
            &view_layout,
            &model_layout,
            Some(&model_matrices),
            Some(&gpu_scene),
            Some(&shadow_layout),
            Some(&shadow_view),
            &mut renderer.resources.material_registry,
            &renderer.resources.mesh_registry,
            Some(&fallback),
            target.format(),
            Some(DEFAULT_DEPTH_FORMAT),
        );
        opaque_phase
            .render(&mut draw_functions, &mut draw_ctx)
            .expect("opaque gltf phase should render");
    }
    ctx.encoder().copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: target.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(readback_pitch),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    ctx.end_frame();

    let pixel = read_pixel_rgba8(&ctx, &target, &readback, 37, 37);
    assert!(
        pixel[0] > pixel[2],
        "near red submesh should occlude far blue submesh, got pixel {pixel:?}"
    );
}
