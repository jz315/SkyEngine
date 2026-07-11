use super::common::*;

#[test]
fn forward_3d_ddgi_executes_with_standard_material_geometry() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let mut renderer = RenderRuntime::from_asset(RenderPipelineAsset::forward_3d());
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-0.9, -0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.9, -0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.9, 0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.9, 0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "forward_3d_global_illumination_mesh",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material_handle = renderer.insert_material::<StandardMaterial>(StandardMaterial {
        albedo: Color::new(0.82, 0.48, 0.26, 1.0),
        emissive: Color::new(0.08, 0.03, 0.01, 1.0),
        ..StandardMaterial::default()
    });

    let mut world = World::new();
    world.insert_resource(RenderSettings {
        global_illumination: crate::render::gi::providers::ddgi::global_illumination(
            crate::render::gi::providers::ddgi::DdgiSettings {
                volume: crate::render::gi::providers::ddgi::DdgiVolumeSettings {
                    origin: [-6.0, -4.0, -6.0],
                    spacing: 3.0,
                    counts: [6, 4, 6],
                    scroll_with_main_camera: false,
                },
                rays_per_probe: 8,
                probes_per_frame: 8,
                irradiance_resolution: 4,
                visibility_resolution: 4,
                max_ray_distance: 16.0,
                ..Default::default()
            },
        ),
        bloom: crate::render::BloomSettings {
            enabled: false,
            ..Default::default()
        },
        tonemap: crate::render::ToneMapSettings {
            enabled: false,
            ..Default::default()
        },
        temporal_aa: crate::render::TemporalAntiAliasingSettings {
            enabled: true,
            ..Default::default()
        },
        vignette: crate::render::VignetteSettings {
            enabled: false,
            ..Default::default()
        },
        ..RenderSettings::default()
    });
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 4.0),
        CameraMarker::new(),
        Projection::perspective(55.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        WgpuMeshRenderer::new(mesh_handle, material_handle),
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 2.0),
        DirectionalLight::new([0.0, 0.0, -1.0])
            .intensity(0.9)
            .color(Color::new(1.0, 0.96, 0.9, 1.0)),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.view_count, 2);
    assert!(stats.passes >= 5);
    assert!(stats.draw_calls >= 1);
}

#[test]
fn modern_3d_ssgi_executes_with_standard_material_geometry() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [96, 96]);
    let mut renderer = RenderRuntime::from_asset(RenderPipelineAsset::modern_3d());
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-0.9, -0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.9, -0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.9, 0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.9, 0.9, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices = [0u16, 1, 2, 0, 2, 3];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "modern_3d_ssgi_mesh",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material_handle = renderer.insert_material::<StandardMaterial>(StandardMaterial {
        albedo: Color::new(0.72, 0.56, 0.42, 1.0),
        roughness: 0.92,
        emissive: Color::new(0.12, 0.08, 0.04, 1.0),
        ..StandardMaterial::default()
    });

    let mut world = World::new();
    world.insert_resource(RenderSettings {
        global_illumination: crate::render::gi::providers::ssgi::global_illumination(
            crate::render::gi::providers::ssgi::SsgiSettings {
                intensity: 1.0,
                radius_pixels: 8.0,
                depth_rejection: 8.0,
                normal_power: 64.0,
            },
        ),
        bloom: crate::render::BloomSettings {
            enabled: false,
            ..Default::default()
        },
        tonemap: crate::render::ToneMapSettings {
            enabled: false,
            ..Default::default()
        },
        vignette: crate::render::VignetteSettings {
            enabled: false,
            ..Default::default()
        },
        ..RenderSettings::default()
    });
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 4.0),
        CameraMarker::new(),
        Projection::perspective(55.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::default(),
        WgpuMeshRenderer::new(mesh_handle, material_handle),
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.view_count, 1);
    assert!(stats.passes >= 5);
    assert!(stats.draw_calls >= 2);
}
