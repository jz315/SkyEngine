use super::common::*;

#[test]
fn forward_3d_enables_shadow_view_for_perspective_directional_light() {
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
            position: [-1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 1.0, 0.0],
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
            "shadow_quad",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material = renderer.insert_material::<StandardMaterial>(StandardMaterial::default());

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, -3.0),
        WgpuMeshRenderer::new(mesh_handle, material),
    ));
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2]).shadow_filter_radius(0.05),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.shadows.views.len(), 1);
    assert!(renderer.shadows.views[0].enabled());
    assert!(renderer.shadows.views[0].caster_count() > 0);
    assert!((renderer.shadows.views[0].radius() - 0.05).abs() < 0.0001);
}

#[test]
fn forward_3d_directional_shadow_atlas_writes_depth_for_casters() {
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
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [-0.5, -0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [-1.0, -1.0, 0.0],
            normal: [-1.0, 0.0, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [-1.0, 0.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [-1.0, 0.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [-1.0, 0.0, 0.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [1.0, 0.0, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [1.0, 0.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [1.0, 0.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [1.0, 0.0, 0.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [-0.5, -0.5, -0.5],
            normal: [0.0, -1.0, 0.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [0.0, -1.0, 0.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, -1.0, 0.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, -1.0, 0.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices: [u16; 36] = [
        0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16, 17,
        18, 16, 18, 19, 20, 21, 22, 20, 22, 23,
    ];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "shadow_depth_readback_cube",
        )
        .with_indices(MeshIndexData::U16(&indices))
        .with_bounding_sphere(BoundingSphere::new(
            [0.0, 0.0, 0.0],
            (0.5f32 * 0.5 + 0.5 * 0.5 + 0.5 * 0.5).sqrt(),
        )),
    ));
    let material = renderer.insert_material::<StandardMaterial>(StandardMaterial::default());

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, -3.0),
        WgpuMeshRenderer::new(mesh_handle, material),
    ));
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2]).shadow_map_size(64),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.shadow_cascade_count, 1);
    assert_eq!(stats.shadow_caster_count, 1);
    assert_eq!(stats.shadow_draw_calls, 1);

    let shadow_view = renderer
        .shadows
        .views
        .first()
        .expect("directional shadow binding should exist");
    assert!(shadow_view.enabled());
    let readback =
        read_render_target(&ctx, shadow_view.target()).expect("shadow atlas readback should work");
    let mut min_depth = f32::INFINITY;
    let mut max_depth = f32::NEG_INFINITY;
    let mut below_clear_depth_count = 0usize;
    let mut written_depth_count = 0usize;
    for bytes in readback.data().chunks_exact(4) {
        let depth = f32::from_le_bytes(bytes.try_into().unwrap());
        if !depth.is_finite() {
            continue;
        }
        min_depth = min_depth.min(depth);
        max_depth = max_depth.max(depth);
        if depth < 1.0 {
            below_clear_depth_count += 1;
        }
        if depth < 0.999 {
            written_depth_count += 1;
        }
    }

    assert!(
        written_depth_count > 0,
        "shadow atlas should contain caster depth values below the clear depth; min_depth={min_depth}, max_depth={max_depth}, below_clear_depth_count={below_clear_depth_count}"
    );
}

#[test]
fn directional_shadow_atlas_draws_caster_between_light_and_near_cascade() {
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
    let mut renderer = RenderRuntime::from_asset(
        RenderPipelineAsset::builder()
            .register_material::<StandardMaterial>()
            .add_phase(crate::render::lighting::shadow::DirectionalShadowPhase::new())
            .add_phase(crate::render::OpaquePhase::new())
            .build(),
    );
    renderer.register_material::<StandardMaterial>(&ctx);

    let vertices = [
        Vertex {
            position: [-0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [0.5, -0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [0.5, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-0.5, 0.5, 0.5],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 0.0],
        },
        Vertex {
            position: [0.5, -0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [-0.5, -0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [-0.5, 0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [0.5, 0.5, -0.5],
            normal: [0.0, 0.0, -1.0],
            uv: [0.0, 0.0],
        },
    ];
    let indices: [u16; 12] = [0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
    let mesh_handle = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&vertices),
            vertices.len() as u32,
            Mesh::vertex_layout_position_normal_uv(),
            "near_cascade_light_ray_caster",
        )
        .with_indices(MeshIndexData::U16(&indices))
        .with_bounding_sphere(BoundingSphere::new([0.0, 0.0, 0.0], 0.87)),
    ));
    let material = renderer.insert_material::<StandardMaterial>(StandardMaterial::default());

    let mut world = World::new();
    world.insert_resource(RenderSettings {
        clear_color: Color::BLACK,
        ambient_color: Color::BLACK,
        global_illumination: crate::render::GlobalIllumination::Off,
        ..RenderSettings::default()
    });
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 120.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(-2.8, 8.0, -3.5),
        WgpuMeshRenderer::new(mesh_handle, material).shadow_lod_cascades(1),
    ));
    world.spawn((DirectionalLight::new([0.35, -1.0, -0.25])
        .cascade_count(4)
        .cascade_distances([8.0, 24.0, 60.0, 120.0])
        .shadow_map_size(128)
        .shadow_bias(0.0)
        .shadow_depth_bias(0)
        .shadow_slope_bias(0.0)
        .shadow_normal_bias(0.0)
        .shadow_filter_radius(0.0),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.shadow_cascade_count, 4);
    assert_eq!(stats.shadow_caster_count_by_cascade[0], 1);
    assert_eq!(stats.shadow_draw_calls_by_cascade[0], 1);
    let shadow_view = renderer
        .shadows
        .views
        .first()
        .expect("directional shadow binding should exist");
    let readback =
        read_render_target(&ctx, shadow_view.target()).expect("shadow atlas readback should work");
    let cascade_width = readback.width() / stats.shadow_cascade_count.max(1) as u32;
    let mut written_first_cascade = 0usize;
    for y in 0..readback.height() {
        for x in 0..cascade_width {
            let index = ((y * readback.width() + x) * readback.bytes_per_pixel()) as usize;
            let depth = f32::from_le_bytes(readback.data()[index..index + 4].try_into().unwrap());
            if depth < 0.999 {
                written_first_cascade += 1;
            }
        }
    }
    assert!(
        written_first_cascade > 0,
        "first cascade should contain depth from the light-ray caster"
    );
}

#[test]
fn directional_shadow_cascade_boundary_keeps_near_and_far_receivers_consistent() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [128, 96]);
    let capture = Arc::new(RenderTarget::from_descriptor(
        &ctx,
        RenderTargetDescriptor::new(128, 96, wgpu::TextureFormat::Rgba8Unorm)
            .label("shadow_boundary_capture"),
    ));
    let mut renderer = RenderRuntime::from_asset(
        RenderPipelineAsset::builder()
            .register_material::<StandardMaterial>()
            .add_phase(crate::render::lighting::shadow::DirectionalShadowPhase::new())
            .add_compute(crate::render::GiUpdateCompute)
            .add_phase(crate::render::OpaquePhase::new())
            .add_postfx(CaptureCurrentColorPass {
                target: capture.clone(),
            })
            .build(),
    );
    renderer.register_material::<StandardMaterial>(&ctx);

    let plane = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&[
                Vertex {
                    position: [-1.0, 0.0, -1.0],
                    normal: [0.0, 0.0, 1.0],
                    uv: [0.0, 1.0],
                },
                Vertex {
                    position: [1.0, 0.0, -1.0],
                    normal: [0.0, 0.0, 1.0],
                    uv: [1.0, 1.0],
                },
                Vertex {
                    position: [1.0, 0.0, 1.0],
                    normal: [0.0, 0.0, 1.0],
                    uv: [1.0, 0.0],
                },
                Vertex {
                    position: [-1.0, 0.0, 1.0],
                    normal: [0.0, 0.0, 1.0],
                    uv: [0.0, 0.0],
                },
            ]),
            4,
            Mesh::vertex_layout_position_normal_uv(),
            "shadow_boundary_receiver",
        )
        .with_indices(MeshIndexData::U16(&[0, 1, 2, 0, 2, 3]))
        .with_bounding_sphere(BoundingSphere::new([0.0, 0.0, 0.0], 1.5)),
    ));
    let blocker = renderer.insert_mesh(Mesh::from_raw(
        &ctx,
        MeshDescriptor::new(
            bytemuck::cast_slice(&[
                Vertex {
                    position: [-0.25, -0.25, -0.25],
                    normal: [0.0, 0.0, 1.0],
                    uv: [0.0, 1.0],
                },
                Vertex {
                    position: [0.25, -0.25, -0.25],
                    normal: [0.0, 0.0, 1.0],
                    uv: [1.0, 1.0],
                },
                Vertex {
                    position: [0.25, 0.25, -0.25],
                    normal: [0.0, 0.0, 1.0],
                    uv: [1.0, 0.0],
                },
                Vertex {
                    position: [-0.25, 0.25, -0.25],
                    normal: [0.0, 0.0, 1.0],
                    uv: [0.0, 0.0],
                },
            ]),
            4,
            Mesh::vertex_layout_position_normal_uv(),
            "shadow_boundary_blocker",
        )
        .with_indices(MeshIndexData::U16(&[0, 1, 2, 0, 2, 3]))
        .with_bounding_sphere(BoundingSphere::new([0.0, 0.0, -0.25], 0.45)),
    ));
    let receiver_material = renderer.insert_material::<StandardMaterial>(StandardMaterial {
        albedo: Color::WHITE,
        roughness: 0.85,
        receive_shadows: true,
        ..Default::default()
    });
    let blocker_material = renderer.insert_material::<StandardMaterial>(StandardMaterial {
        albedo: Color::BLACK,
        roughness: 1.0,
        receive_shadows: false,
        ..Default::default()
    });

    let mut world = World::new();
    world.insert_resource(RenderSettings {
        clear_color: Color::BLACK,
        ambient_color: Color::BLACK,
        global_illumination: crate::render::GlobalIllumination::Off,
        bloom: crate::render::BloomSettings {
            enabled: false,
            ..Default::default()
        },
        tonemap: crate::render::ToneMapSettings {
            enabled: false,
            ..Default::default()
        },
        temporal_aa: crate::render::TemporalAntiAliasingSettings {
            enabled: false,
            ..Default::default()
        },
        contact_shadows: crate::render::ContactShadowsSettings {
            enabled: false,
            ..Default::default()
        },
        ..RenderSettings::default()
    });
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 120.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, -7.0).with_scale3(7.0, 1.0, 1.0),
        WgpuMeshRenderer::new(plane, receiver_material).casts_shadows(false),
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, -2.4),
        WgpuMeshRenderer::new(blocker, blocker_material),
    ));
    world.spawn((DirectionalLight::new([0.55, -1.0, -0.35])
        .intensity(9.0)
        .cascade_count(4)
        .cascade_distances([4.0, 9.5, 24.0, 60.0])
        .shadow_map_size(256)
        .shadow_bias(0.0)
        .shadow_depth_bias(0)
        .shadow_slope_bias(0.0)
        .shadow_normal_bias(0.0)
        .shadow_filter_radius(0.0),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.shadow_cascade_count, 4);
    assert!(
        stats.shadow_caster_count >= 1,
        "the blocker should be submitted to at least one shadow cascade"
    );
    assert!(
        stats
            .shadow_caster_count_by_cascade
            .iter()
            .any(|count| *count > 0),
        "the blocker should survive cascade culling in the boundary scene"
    );

    let readback =
        read_render_target(&ctx, &capture).expect("shadow boundary color readback should work");
    assert_eq!(readback.format(), wgpu::TextureFormat::Rgba8Unorm);

    let pixels = readback
        .data()
        .chunks_exact(readback.bytes_per_pixel() as usize)
        .map(|pixel| {
            [
                pixel[0] as f32 / 255.0,
                pixel[1] as f32 / 255.0,
                pixel[2] as f32 / 255.0,
            ]
        })
        .collect::<Vec<_>>();
    let width = readback.width();
    let height = readback.height();
    let left_index = (height / 2 * width + width / 3) as usize;
    let right_index = (height / 2 * width + width * 2 / 3) as usize;
    let center_index = (height / 2 * width + width / 2) as usize;
    let left_luma = pixels[left_index][0] + pixels[left_index][1] + pixels[left_index][2];
    let right_luma = pixels[right_index][0] + pixels[right_index][1] + pixels[right_index][2];
    let center_luma = pixels[center_index][0] + pixels[center_index][1] + pixels[center_index][2];

    assert!(
        (left_luma - right_luma).abs() < 0.20,
        "cascade boundary should not create a large brightness jump; left={left_luma}, right={right_luma}, center={center_luma}"
    );
    assert!(
        center_luma < 2.6,
        "receiver center should remain shaded enough to prove the blocker contributes to the frame; center={center_luma}"
    );
}

#[derive(Clone)]
struct CaptureCurrentColorPass {
    target: Arc<RenderTarget>,
}

impl CaptureCurrentColorPass {
    fn import_target(&self) -> ImportedTexture {
        ImportedTexture {
            texture: Arc::new(self.target.texture().clone()),
            view: Arc::new(self.target.view().clone()),
            size: [self.target.width(), self.target.height()],
            format: self.target.format(),
            usage: self.target.usage(),
            sample_count: self.target.sample_count(),
            mip_level_count: self.target.mip_level_count(),
            array_layer_count: self.target.array_layer_count(),
        }
    }
}

impl PostFxPass for CaptureCurrentColorPass {
    fn name(&self) -> &'static str {
        "capture_current_color"
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        let Some(current) = ctx.state().current_color() else {
            return;
        };
        let capture = ctx.graph().create_texture(|builder| {
            builder
                .name("captured_current_color")
                .import_external(self.import_target());
        });
        ctx.graph().add_copy_pass(self.name(), |setup| {
            setup.texture_to_texture(current.handle(), capture);
        });
        ctx.state().set_current_color(capture, current.format());
    }
}

#[test]
fn standard_material_directional_shadow_darkens_final_color() {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    fn plane_mesh(ctx: &GpuContext, label: &'static str) -> Mesh {
        let vertices = [
            Vertex {
                position: [-1.0, -1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [1.0, -1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [1.0, 1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-1.0, 1.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 0.0],
            },
        ];
        Mesh::from_raw(
            ctx,
            MeshDescriptor::new(
                bytemuck::cast_slice(&vertices),
                vertices.len() as u32,
                Mesh::vertex_layout_position_normal_uv(),
                label,
            )
            .with_indices(MeshIndexData::U16(&[0, 1, 2, 0, 2, 3]))
            .with_bounding_sphere(BoundingSphere::new([0.0, 0.0, 0.0], (2.0f32).sqrt())),
        )
    }

    fn box_mesh(ctx: &GpuContext, label: &'static str) -> Mesh {
        let vertices = [
            Vertex {
                position: [-0.5, -0.5, 0.5],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [0.5, -0.5, 0.5],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, 0.5],
                normal: [0.0, 0.0, 1.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-0.5, 0.5, 0.5],
                normal: [0.0, 0.0, 1.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [0.5, -0.5, -0.5],
                normal: [0.0, 0.0, -1.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [-0.5, -0.5, -0.5],
                normal: [0.0, 0.0, -1.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [-0.5, 0.5, -0.5],
                normal: [0.0, 0.0, -1.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [0.5, 0.5, -0.5],
                normal: [0.0, 0.0, -1.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [-0.5, -0.5, -0.5],
                normal: [-1.0, 0.0, 0.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [-0.5, -0.5, 0.5],
                normal: [-1.0, 0.0, 0.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [-0.5, 0.5, 0.5],
                normal: [-1.0, 0.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-0.5, 0.5, -0.5],
                normal: [-1.0, 0.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [0.5, -0.5, 0.5],
                normal: [1.0, 0.0, 0.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [0.5, -0.5, -0.5],
                normal: [1.0, 0.0, 0.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, -0.5],
                normal: [1.0, 0.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [0.5, 0.5, 0.5],
                normal: [1.0, 0.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [-0.5, 0.5, 0.5],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, 0.5],
                normal: [0.0, 1.0, 0.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, -0.5],
                normal: [0.0, 1.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-0.5, 0.5, -0.5],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [-0.5, -0.5, -0.5],
                normal: [0.0, -1.0, 0.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [0.5, -0.5, -0.5],
                normal: [0.0, -1.0, 0.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [0.5, -0.5, 0.5],
                normal: [0.0, -1.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-0.5, -0.5, 0.5],
                normal: [0.0, -1.0, 0.0],
                uv: [0.0, 0.0],
            },
        ];
        let indices: [u16; 36] = [
            0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16,
            17, 18, 16, 18, 19, 20, 21, 22, 20, 22, 23,
        ];
        Mesh::from_raw(
            ctx,
            MeshDescriptor::new(
                bytemuck::cast_slice(&vertices),
                vertices.len() as u32,
                Mesh::vertex_layout_position_normal_uv(),
                label,
            )
            .with_indices(MeshIndexData::U16(&indices))
            .with_bounding_sphere(BoundingSphere::new([0.0, 0.0, 0.0], (0.75f32).sqrt())),
        )
    }

    fn render_scene(
        receive_shadows: bool,
    ) -> (Vec<[f32; 3]>, u32, u32, crate::render::view::RenderStats) {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Rgba8Unorm, [96, 96]);
        let capture = Arc::new(RenderTarget::from_descriptor(
            &ctx,
            RenderTargetDescriptor::new(96, 96, wgpu::TextureFormat::Rgba8Unorm)
                .label("standard_material_shadow_capture"),
        ));
        let mut renderer = RenderRuntime::from_asset(
            RenderPipelineAsset::builder()
                .register_material::<StandardMaterial>()
                .add_phase(crate::render::lighting::shadow::DirectionalShadowPhase::new())
                .add_compute(crate::render::GiUpdateCompute)
                .add_phase(crate::render::OpaquePhase::new())
                .add_postfx(CaptureCurrentColorPass {
                    target: capture.clone(),
                })
                .build(),
        );
        renderer.register_material::<StandardMaterial>(&ctx);

        let plane = renderer.insert_mesh(plane_mesh(&ctx, "shadow_receiver_plane"));
        let cube = renderer.insert_mesh(box_mesh(&ctx, "shadow_caster_cube"));
        let receiver_material = renderer.insert_material::<StandardMaterial>(StandardMaterial {
            albedo: Color::WHITE,
            roughness: 0.8,
            receive_shadows,
            ..StandardMaterial::default()
        });
        let caster_material = renderer.insert_material::<StandardMaterial>(StandardMaterial {
            albedo: Color::BLACK,
            roughness: 1.0,
            receive_shadows: false,
            ..StandardMaterial::default()
        });

        let mut world = World::new();
        world.insert_resource(RenderSettings {
            clear_color: Color::BLACK,
            ambient_color: Color::BLACK,
            global_illumination: crate::render::GlobalIllumination::Off,
            bloom: crate::render::BloomSettings {
                enabled: false,
                ..Default::default()
            },
            tonemap: crate::render::ToneMapSettings {
                enabled: false,
                ..Default::default()
            },
            temporal_aa: crate::render::TemporalAntiAliasingSettings {
                enabled: false,
                ..Default::default()
            },
            vignette: crate::render::VignetteSettings {
                enabled: false,
                ..Default::default()
            },
            contact_shadows: crate::render::ContactShadowsSettings {
                enabled: false,
                ..Default::default()
            },
            ..RenderSettings::default()
        });
        world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::perspective(60.0f32.to_radians(), 0.1, 32.0),
            MainCamera,
        ));
        world.spawn((
            Transform::from_xyz(0.0, 0.0, -5.0).with_scale3(2.4, 2.4, 1.0),
            WgpuMeshRenderer::new(plane, receiver_material).casts_shadows(false),
        ));
        world.spawn((
            Transform::from_xyz(-0.65, 0.0, -4.0).with_scale3(0.7, 0.7, 0.7),
            WgpuMeshRenderer::new(cube, caster_material),
        ));
        world.spawn((DirectionalLight::new([0.65, 0.0, -1.0])
            .intensity(8.0)
            .color(Color::WHITE)
            .shadow_map_size(256)
            .shadow_bias(0.0)
            .shadow_depth_bias(0)
            .shadow_slope_bias(0.0)
            .shadow_normal_bias(0.0)
            .shadow_filter_radius(0.0),));

        ctx.begin_frame()
            .expect("headless begin_frame should succeed");
        renderer.render_world(&mut ctx, &world);
        ctx.end_frame();

        let readback =
            read_render_target(&ctx, &capture).expect("captured color readback should work");
        assert_eq!(readback.format(), wgpu::TextureFormat::Rgba8Unorm);
        let pixels = readback
            .data()
            .chunks_exact(readback.bytes_per_pixel() as usize)
            .map(|pixel| {
                [
                    pixel[0] as f32 / 255.0,
                    pixel[1] as f32 / 255.0,
                    pixel[2] as f32 / 255.0,
                ]
            })
            .collect();
        (
            pixels,
            readback.width(),
            readback.height(),
            renderer.stats(),
        )
    }

    let (shadowed, width, height, shadowed_stats) = render_scene(true);
    let (unshadowed, unshadowed_width, unshadowed_height, unshadowed_stats) = render_scene(false);
    assert_eq!([width, height], [unshadowed_width, unshadowed_height]);
    let center_index = (48 * width + 48) as usize;
    let shadowed_center = shadowed[center_index];
    let unshadowed_center = unshadowed[center_index];
    let center_delta = (unshadowed_center[0] + unshadowed_center[1] + unshadowed_center[2])
        - (shadowed_center[0] + shadowed_center[1] + shadowed_center[2]);
    let mut max_delta = f32::NEG_INFINITY;
    let mut max_delta_pixel = [0u32; 2];
    let mut max_shadowed = [0.0; 3];
    let mut max_unshadowed = [0.0; 3];
    let mut min_far_delta = f32::INFINITY;
    let mut min_far_pixel = [0u32; 2];
    let mut far_shadowed = [0.0; 3];
    let mut far_unshadowed = [0.0; 3];
    for y in 16..(height - 16) {
        for x in 16..(width - 16) {
            let index = (y * width + x) as usize;
            let shadowed_luma = shadowed[index][0] + shadowed[index][1] + shadowed[index][2];
            let unshadowed_luma =
                unshadowed[index][0] + unshadowed[index][1] + unshadowed[index][2];
            let delta = unshadowed_luma - shadowed_luma;
            if delta > max_delta {
                max_delta = delta;
                max_delta_pixel = [x, y];
                max_shadowed = shadowed[index];
                max_unshadowed = unshadowed[index];
            }
        }
    }
    for y in 16..(height - 16) {
        for x in 16..(width - 16) {
            let dx = x as i32 - max_delta_pixel[0] as i32;
            let dy = y as i32 - max_delta_pixel[1] as i32;
            if dx * dx + dy * dy < 28 * 28 {
                continue;
            }
            let index = (y * width + x) as usize;
            let shadowed_luma = shadowed[index][0] + shadowed[index][1] + shadowed[index][2];
            let unshadowed_luma =
                unshadowed[index][0] + unshadowed[index][1] + unshadowed[index][2];
            let delta = (unshadowed_luma - shadowed_luma).abs();
            if delta < min_far_delta {
                min_far_delta = delta;
                min_far_pixel = [x, y];
                far_shadowed = shadowed[index];
                far_unshadowed = unshadowed[index];
            }
        }
    }

    assert_eq!(shadowed_stats.shadow_caster_count, 1);
    assert_eq!(shadowed_stats.shadow_draw_calls, 1);
    assert_eq!(unshadowed_stats.shadow_caster_count, 1);
    assert_eq!(unshadowed_stats.shadow_draw_calls, 1);
    assert!(
        max_delta > 0.25,
        "expected receive_shadows=true to darken at least one receiver pixel; center_delta={center_delta}, max_delta={max_delta} at {max_delta_pixel:?}, shadowed={max_shadowed:?}, unshadowed={max_unshadowed:?}"
    );
    assert!(
        min_far_delta < 0.04,
        "expected a far receiver pixel to stay lit while the caster shadows another area; min_far_delta={min_far_delta} at {min_far_pixel:?}, shadowed={far_shadowed:?}, unshadowed={far_unshadowed:?}, shadow_pixel={max_delta_pixel:?}"
    );
}

#[test]
fn forward_3d_enables_shadow_view_for_orthographic_directional_light() {
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
            position: [-1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 1.0, 0.0],
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
            "shadow_quad_ortho",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material = renderer.insert_material::<StandardMaterial>(StandardMaterial::default());

    let mut world = World::new();
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 8.0),
        CameraMarker::new(),
        Projection::orthographic_fixed(12.0, 12.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, 0.0).with_scale(4.0, 4.0),
        WgpuMeshRenderer::new(mesh_handle, material),
    ));
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2]),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(renderer.shadows.views.len(), 1);
    assert!(renderer.shadows.views[0].enabled());
    assert!(renderer.shadows.views[0].caster_count() > 0);
}

#[test]
fn forward_3d_shadow_stats_track_lod_mask_per_cascade() {
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
            position: [-1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.0, 1.0],
        },
        Vertex {
            position: [1.0, -1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 1.0],
        },
        Vertex {
            position: [1.0, 1.0, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [1.0, 0.0],
        },
        Vertex {
            position: [-1.0, 1.0, 0.0],
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
            "shadow_lod_stats_quad",
        )
        .with_indices(MeshIndexData::U16(&indices)),
    ));
    let material = renderer.insert_material::<StandardMaterial>(StandardMaterial::default());

    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::perspective(60.0f32.to_radians(), 0.1, 32.0),
        MainCamera,
    ));
    world.spawn((
        Transform::from_xyz(0.0, 0.0, -3.0),
        WgpuMeshRenderer::new(mesh_handle, material).shadow_lod_cascades(1),
    ));
    world.spawn((DirectionalLight::new([0.3, -1.0, 0.2])
        .cascade_count(2)
        .cascade_distances([8.0, 32.0, 0.0, 0.0])
        .shadow_map_size(64),));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    let stats = renderer.stats();
    assert_eq!(stats.shadow_cascade_count, 2);
    assert_eq!(stats.shadow_caster_count_by_cascade[0], 1);
    assert_eq!(stats.shadow_caster_count_by_cascade[1], 0);
    assert_eq!(stats.shadow_caster_count, 1);
    assert_eq!(stats.shadow_draw_calls_by_cascade[0], 1);
    assert_eq!(stats.shadow_draw_calls_by_cascade[1], 0);
}
