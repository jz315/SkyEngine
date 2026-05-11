use super::bindings::binding_location;
use super::constants::{
    SSGI_COMPUTE_SHADER, SSGI_DEINTERLEAVE_COMPUTE_SHADER, SSGI_FINAL_PASS, SSGI_FINAL_SHADER,
    SSGI_MIP_COUNT, SSGI_TEXTURE_ATLAS_COLOR, SSGI_TEXTURE_ATLAS_DEPTH, SSGI_TEXTURE_DEPTH_MIPS,
    SSGI_TEXTURE_DIFFUSE_MIPS, SSGI_TEXTURE_FILTERED_DIFFUSE_MIPS, SSGI_TEXTURE_NORMAL_MIPS,
    SSGI_UPSAMPLE_COMPUTE_SHADER,
};
use super::contract::{
    ssgi_pass_descriptor, ssgi_pass_descriptors, SsgiPassKind, SsgiResourceRole,
};
use super::graph::{declare_ssgi_graph, validate_compiled_pass_resources, SsgiGraphInputs};
use super::layout::{ssgi_compute_texture_specs, SsgiMipLevel, SsgiResources};
use super::pipelines::ssgi_shader_source;
use super::settings::SsgiSettings;
use super::uniforms::SsgiUniform;
use crate::math::Mat4;
use crate::render::graph::{RenderGraph, ResourceRef, TargetSize, TextureSubresource};
use rustc_hash::FxHashSet;

fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for render tests");

    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("ssgi_shader_test_device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .expect("Failed to create test GPU device")
}

fn assert_wgsl_module_is_valid(device: &wgpu::Device, label: &'static str, source: &str) {
    let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let _module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let error = pollster::block_on(error_scope.pop());
    assert!(error.is_none(), "{label} should validate: {error:?}");
}

fn test_scene_view() -> crate::render::view::SceneView {
    let target_size = [640, 360];
    let transform = crate::render::Transform::default();
    let projection =
        crate::render::view::Projection::perspective(60.0_f32.to_radians(), 0.1, 100.0);
    let view_uniform = projection.view_uniform(transform, target_size);
    crate::render::view::SceneView::new(
        0,
        crate::render::view::ViewportRect::new(0, 0, target_size[0], target_size[1]),
        target_size,
        false,
        u32::MAX,
        transform,
        projection,
        view_uniform,
        false,
    )
}

fn decode_scene_normal_like_ssgi(encoded: [f32; 3]) -> [f32; 3] {
    let normal = [
        encoded[0] * 2.0 - 1.0,
        encoded[1] * 2.0 - 1.0,
        encoded[2] * 2.0 - 1.0,
    ];
    let len_sq = normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];
    if len_sq <= 0.000001 {
        return [0.0, 0.0, -1.0];
    }
    let inv_len = len_sq.sqrt().recip();
    [
        normal[0] * inv_len,
        normal[1] * inv_len,
        -normal[2] * inv_len,
    ]
}

fn reconstruct_positive_view_z_like_ssgi(projection: Mat4, depth: f32) -> f32 {
    let view = projection.inverse() * crate::math::Vec4::new(0.0, 0.0, depth, 1.0);
    let view = view.to_array();
    -(view[2] / view[3])
}

#[test]
fn ssgi_resources_clamp_zero_size_to_one() {
    let mut resources = SsgiResources::default();
    resources.resize(0, 0);
    assert_eq!(resources.target_size(), [1, 1]);
}

#[test]
fn ssgi_resources_track_requested_size() {
    let mut resources = SsgiResources::default();
    resources.resize(640, 360);
    assert_eq!(resources.target_size(), [640, 360]);
}

#[test]
fn ssgi_resources_match_wicked_aligned_atlas_dimensions() {
    let mut resources = SsgiResources::default();
    resources.resize(641, 359);

    assert_eq!(resources.target_size(), [641, 359]);
    assert_eq!(resources.aligned_size(), [704, 384]);
    assert_eq!(resources.atlas_size(), [88, 48]);
    assert_eq!(resources.atlas_layers(), 16);
    assert_eq!(
        resources.mip_level(0),
        Some(SsgiMipLevel {
            scale: 2,
            atlas_size: [88, 48],
            regular_size: [352, 192],
        })
    );
    assert_eq!(
        resources.mip_level(3),
        Some(SsgiMipLevel {
            scale: 16,
            atlas_size: [11, 6],
            regular_size: [44, 24],
        })
    );
}

#[test]
fn ssgi_compute_layout_matches_wicked_texture2d_array_contract() {
    let mut resources = SsgiResources::default();
    resources.resize(641, 359);

    let layout = resources.compute_texture_layout();

    assert_eq!(layout.atlas_size, [88, 48]);
    assert_eq!(layout.regular_mip_size, [352, 192]);
    assert_eq!(layout.mip_level_count, 4);
    assert_eq!(layout.atlas_layer_count, 16);
    assert_eq!(layout.atlas_color_format, wgpu::TextureFormat::Rgba16Float);
    assert_eq!(layout.atlas_depth_format, wgpu::TextureFormat::R32Float);
    assert_eq!(layout.depth_mip_format, wgpu::TextureFormat::R32Float);
    assert_eq!(layout.normal_mip_format, wgpu::TextureFormat::Rgba16Float);
    assert_eq!(layout.diffuse_mip_format, wgpu::TextureFormat::Rgba16Float);
    assert!(layout.usage.contains(wgpu::TextureUsages::STORAGE_BINDING));
    assert!(layout.usage.contains(wgpu::TextureUsages::TEXTURE_BINDING));
    assert!(!layout
        .usage
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT));
}

#[test]
fn ssgi_compute_texture_specs_allocate_array_atlas_and_mip_chains() {
    let mut resources = SsgiResources::default();
    resources.resize(1280, 720);

    let specs = ssgi_compute_texture_specs(resources);

    assert_eq!(specs.atlas_color.name(), SSGI_TEXTURE_ATLAS_COLOR);
    assert_eq!(specs.atlas_color.size(), TargetSize::Exact(160, 96));
    assert_eq!(specs.atlas_color.mip_level_count(), 4);
    assert_eq!(specs.atlas_color.array_layer_count(), 16);
    assert_eq!(specs.atlas_color.format(), wgpu::TextureFormat::Rgba16Float);
    assert!(specs
        .atlas_color
        .usage_flags()
        .contains(wgpu::TextureUsages::STORAGE_BINDING));

    assert_eq!(specs.atlas_depth.name(), SSGI_TEXTURE_ATLAS_DEPTH);
    assert_eq!(specs.atlas_depth.size(), TargetSize::Exact(160, 96));
    assert_eq!(specs.atlas_depth.mip_level_count(), 4);
    assert_eq!(specs.atlas_depth.array_layer_count(), 16);
    assert_eq!(specs.atlas_depth.format(), wgpu::TextureFormat::R32Float);

    assert_eq!(specs.depth_mips.name(), SSGI_TEXTURE_DEPTH_MIPS);
    assert_eq!(specs.depth_mips.size(), TargetSize::Exact(640, 384));
    assert_eq!(specs.depth_mips.mip_level_count(), 4);
    assert_eq!(specs.depth_mips.array_layer_count(), 1);
    assert_eq!(specs.depth_mips.format(), wgpu::TextureFormat::R32Float);

    assert_eq!(specs.normal_mips.name(), SSGI_TEXTURE_NORMAL_MIPS);
    assert_eq!(specs.normal_mips.size(), TargetSize::Exact(640, 384));
    assert_eq!(specs.normal_mips.format(), wgpu::TextureFormat::Rgba16Float);

    assert_eq!(specs.diffuse_mips.name(), SSGI_TEXTURE_DIFFUSE_MIPS);
    assert_eq!(specs.diffuse_mips.size(), TargetSize::Exact(640, 384));
    assert_eq!(
        specs.diffuse_mips.format(),
        wgpu::TextureFormat::Rgba16Float
    );

    assert_eq!(
        specs.filtered_diffuse_mips.name(),
        SSGI_TEXTURE_FILTERED_DIFFUSE_MIPS
    );
    assert_eq!(
        specs.filtered_diffuse_mips.size(),
        TargetSize::Exact(640, 384)
    );
    assert_eq!(
        specs.filtered_diffuse_mips.format(),
        wgpu::TextureFormat::Rgba16Float
    );
}

#[test]
fn ssgi_pass_names_are_unique_and_lookup_is_total() {
    let mut names = FxHashSet::default();
    for descriptor in ssgi_pass_descriptors() {
        assert!(names.insert(descriptor.name));
        assert_eq!(
            ssgi_pass_descriptor(descriptor.name).map(|desc| desc.name),
            Some(descriptor.name)
        );
    }
    assert_eq!(ssgi_pass_descriptors().len(), SSGI_MIP_COUNT * 2 + 5);
}

#[test]
fn ssgi_descriptor_order_preserves_wicked_compute_chain() {
    let names: Vec<&str> = ssgi_pass_descriptors()
        .iter()
        .map(|descriptor| descriptor.name)
        .collect();
    assert_eq!(
        names,
        vec![
            "ssgi_compute_deinterleave_2x",
            "ssgi_compute_deinterleave_4x",
            "ssgi_compute_deinterleave_8x",
            "ssgi_compute_deinterleave_16x",
            "ssgi_compute_diffuse_16x",
            "ssgi_compute_diffuse_8x",
            "ssgi_compute_diffuse_4x",
            "ssgi_compute_diffuse_2x",
            "ssgi_compute_upsample_16x_to_8x",
            "ssgi_compute_upsample_8x_to_4x",
            "ssgi_compute_upsample_4x_to_2x",
            SSGI_FINAL_PASS,
            "ssgi_scene_composite",
        ]
    );
}

#[test]
fn ssgi_compute_descriptors_map_to_kinds_and_shader_sources() {
    for descriptor in ssgi_pass_descriptors() {
        match descriptor.kind {
            SsgiPassKind::Deinterleave { mip_index } | SsgiPassKind::Diffuse { mip_index } => {
                assert!(mip_index < SSGI_MIP_COUNT)
            }
            SsgiPassKind::Upsample {
                source_mip_index,
                target_mip_index,
                pass_index,
            } => {
                assert_eq!(source_mip_index, target_mip_index + 1);
                assert!(pass_index < SSGI_MIP_COUNT - 1);
            }
            SsgiPassKind::FinalComposite => assert_eq!(descriptor.name, SSGI_FINAL_PASS),
            SsgiPassKind::SceneComposite => assert_eq!(descriptor.name, "ssgi_scene_composite"),
        }
        assert!(!ssgi_shader_source(descriptor.shader).is_empty());
    }
}

#[test]
fn ssgi_upsample_reads_raw_target_diffuse_and_writes_filtered_target() {
    for descriptor in ssgi_pass_descriptors() {
        let SsgiPassKind::Upsample {
            target_mip_index, ..
        } = descriptor.kind
        else {
            continue;
        };
        let target = SsgiResourceRole::DiffuseMip {
            mip: target_mip_index as u32,
        };
        let filtered_target = SsgiResourceRole::FilteredDiffuseMip {
            mip: target_mip_index as u32,
        };
        assert!(
            descriptor.reads.contains(&target),
            "{} must preserve and blend the raw target mip diffuse result",
            descriptor.name
        );
        assert!(
            descriptor.writes.contains(&filtered_target),
            "{} must write the blended result into the filtered mip chain",
            descriptor.name
        );
        assert!(
            !descriptor.writes.contains(&target),
            "{} must not read and storage-write the same raw diffuse mip in one dispatch",
            descriptor.name
        );
    }
}

#[test]
fn ssgi_binding_roles_are_unique_within_descriptor_groups() {
    for descriptor in ssgi_pass_descriptors() {
        let mut locations = FxHashSet::default();
        for role in descriptor.bindings {
            assert!(
                locations.insert(binding_location(*role)),
                "duplicate binding location in {} for {:?}",
                descriptor.name,
                role
            );
        }
    }
}

#[test]
fn ssgi_graph_resource_roles_resolve_to_expected_subresources() {
    let mut resources = SsgiResources::default();
    resources.resize(320, 180);
    let mut graph = RenderGraph::new();
    let scene_color = graph.create_texture(|builder| {
        builder
            .name("ssgi_test_scene_color")
            .format(wgpu::TextureFormat::Rgba16Float)
            .persistent();
    });
    let scene_depth = graph.create_texture(|builder| {
        builder
            .name("ssgi_test_scene_depth")
            .format(wgpu::TextureFormat::Depth32Float)
            .persistent();
    });
    let scene_normal = graph.create_texture(|builder| {
        builder
            .name("ssgi_test_scene_normal")
            .format(wgpu::TextureFormat::Rgba16Float)
            .persistent();
    });
    let scene_velocity = graph.create_texture(|builder| {
        builder
            .name("ssgi_test_scene_velocity")
            .format(wgpu::TextureFormat::Rgba16Float)
            .persistent();
    });

    let handles = declare_ssgi_graph(
        &mut graph,
        resources,
        SsgiGraphInputs {
            scene_color,
            scene_depth,
            scene_normal,
            scene_velocity,
            target_size: [320, 180],
            output_format: wgpu::TextureFormat::Rgba16Float,
        },
    );

    assert_eq!(
        graph.get_texture(SSGI_TEXTURE_ATLAS_COLOR),
        Some(handles.atlas_color)
    );
    assert_eq!(
        graph.get_texture(SSGI_TEXTURE_ATLAS_DEPTH),
        Some(handles.atlas_depth)
    );
    assert_eq!(
        graph.get_texture(SSGI_TEXTURE_DEPTH_MIPS),
        Some(handles.depth_mips)
    );
    assert_eq!(
        graph.get_texture(SSGI_TEXTURE_NORMAL_MIPS),
        Some(handles.normal_mips)
    );
    assert_eq!(
        graph.get_texture(SSGI_TEXTURE_DIFFUSE_MIPS),
        Some(handles.diffuse_mips)
    );
    assert_eq!(
        graph.get_texture(SSGI_TEXTURE_FILTERED_DIFFUSE_MIPS),
        Some(handles.filtered_diffuse_mips)
    );
    assert_eq!(
        handles.resolve(SsgiResourceRole::AtlasColor { mip: 2 }),
        ResourceRef::TextureSubresource(TextureSubresource::new(handles.atlas_color, 2, 1, 0, 16))
    );
    assert_eq!(
        handles.atlas_color_layer(2, 7),
        TextureSubresource::new(handles.atlas_color, 2, 1, 7, 1)
    );
    assert_eq!(
        handles.atlas_depth_layer(1, 3),
        TextureSubresource::new(handles.atlas_depth, 1, 1, 3, 1)
    );
    assert_eq!(
        handles.resolve(SsgiResourceRole::DiffuseMip { mip: 3 }),
        ResourceRef::TextureSubresource(TextureSubresource::new(handles.diffuse_mips, 3, 1, 0, 1))
    );
    assert_eq!(
        handles.resolve(SsgiResourceRole::FilteredDiffuseMip { mip: 2 }),
        ResourceRef::TextureSubresource(TextureSubresource::new(
            handles.filtered_diffuse_mips,
            2,
            1,
            0,
            1
        ))
    );
}

#[test]
fn ssgi_graph_declarations_match_contract_and_keep_compute_chain_alive() {
    let mut resources = SsgiResources::default();
    resources.resize(320, 180);
    let mut graph = RenderGraph::new();
    let scene_color = graph.create_texture(|builder| {
        builder
            .name("ssgi_test_scene_color")
            .format(wgpu::TextureFormat::Rgba16Float)
            .persistent();
    });
    let scene_depth = graph.create_texture(|builder| {
        builder
            .name("ssgi_test_scene_depth")
            .format(wgpu::TextureFormat::Depth32Float)
            .persistent();
    });
    let scene_normal = graph.create_texture(|builder| {
        builder
            .name("ssgi_test_scene_normal")
            .format(wgpu::TextureFormat::Rgba16Float)
            .persistent();
    });
    let scene_velocity = graph.create_texture(|builder| {
        builder
            .name("ssgi_test_scene_velocity")
            .format(wgpu::TextureFormat::Rgba16Float)
            .persistent();
    });
    let handles = declare_ssgi_graph(
        &mut graph,
        resources,
        SsgiGraphInputs {
            scene_color,
            scene_depth,
            scene_normal,
            scene_velocity,
            target_size: [320, 180],
            output_format: wgpu::TextureFormat::Rgba16Float,
        },
    );
    graph.add_render_pass("ssgi_test_present", |setup| {
        setup.read(handles.output_scene_color());
        setup.write_surface();
    });

    assert_eq!(graph.pass_count(), ssgi_pass_descriptors().len() + 1);
    let compiled = graph
        .compile()
        .expect("SSGI graph should compile with final output");
    assert_eq!(compiled.len(), ssgi_pass_descriptors().len() + 1);

    for pass in &compiled {
        if let Some(descriptor) = ssgi_pass_descriptor(pass.name.as_ref()) {
            validate_compiled_pass_resources(pass, descriptor, handles)
                .expect("compiled pass resources should match SSGI contract");
        }
    }
}

#[test]
fn ssgi_compute_wgsl_sources_validate() {
    let (device, _queue) = create_test_device();

    assert_wgsl_module_is_valid(
        &device,
        "ssgi_deinterleave_compute_test",
        SSGI_DEINTERLEAVE_COMPUTE_SHADER,
    );
    assert_wgsl_module_is_valid(&device, "ssgi_compute_test", SSGI_COMPUTE_SHADER);
    assert_wgsl_module_is_valid(
        &device,
        "ssgi_upsample_compute_test",
        SSGI_UPSAMPLE_COMPUTE_SHADER,
    );
}

#[test]
fn ssgi_upsample_shader_blends_high_frequency_diffuse_without_additive_blowout() {
    assert!(SSGI_UPSAMPLE_COMPUTE_SHADER.contains("var input_diffuse_high: texture_2d<f32>;"));
    assert!(SSGI_UPSAMPLE_COMPUTE_SHADER.contains("textureLoad(input_diffuse_high"));
    assert!(SSGI_UPSAMPLE_COMPUTE_SHADER.contains("mix(high_diffuse, result"));
    assert!(
        !SSGI_UPSAMPLE_COMPUTE_SHADER.contains("result + high_diffuse"),
        "upsample must not add every mip contribution together; that turns leaked low mip energy into bright bands"
    );
}

#[test]
fn ssgi_deinterleave_samples_current_color_without_velocity_smear() {
    assert!(
        SSGI_DEINTERLEAVE_COMPUTE_SHADER.contains("textureLoad(t_scene_color, scene_pixel, 0).rgb")
    );
    assert!(
        !SSGI_DEINTERLEAVE_COMPUTE_SHADER.contains("prev_pixel"),
        "current scene color must not be reprojected through velocity; that smears bright pixels across depth edges"
    );
    assert!(SSGI_DEINTERLEAVE_COMPUTE_SHADER.contains("luminance(color) <= ssgi.params1.w"));
    assert!(SSGI_DEINTERLEAVE_COMPUTE_SHADER.contains("SSGI_MAX_SOURCE_LUMINANCE"));
}

#[test]
fn ssgi_diffuse_and_upsample_reject_cross_edge_leaks() {
    assert!(
        SSGI_COMPUTE_SHADER.contains("dot(sample_normal, -origin_to_sample_dir)"),
        "diffuse gathering must reject samples whose surface normal faces away from the receiver"
    );
    assert!(SSGI_COMPUTE_SHADER.contains("bilateral_depth_weight(abs(origin_to_sample.z))"));
    assert!(
        SSGI_COMPUTE_SHADER.contains("sample_z < z - thickness")
            && SSGI_COMPUTE_SHADER.contains("occlusion = 0.0"),
        "diffuse gathering must drop samples hidden behind a nearer depth blocker"
    );
    assert!(
        !SSGI_COMPUTE_SHADER.contains("bilateral_depth_weight(sample_distance)"),
        "diffuse gathering should reject by view-depth separation, not lateral distance on one surface"
    );
    for source in [SSGI_UPSAMPLE_COMPUTE_SHADER, SSGI_FINAL_SHADER] {
        assert!(source
            .contains("bilateral_depth_weight(abs(sample_linear_depth - center_linear_depth))"));
        assert!(
            !source.contains("+ 0.001"),
            "bilateral upsample should not keep an always-on normal-weight leak across hard edges"
        );
    }
}

#[test]
fn ssgi_uniform_clamps_and_sets_scale_parameters() {
    let scene_view = test_scene_view();
    let uniform = SsgiUniform::for_pass(
        SsgiSettings {
            intensity: -1.0,
            radius_pixels: 100.0,
            depth_rejection: 0.0,
            normal_power: 0.0,
        },
        &scene_view,
        SsgiPassKind::Diffuse { mip_index: 0 },
    );

    assert_eq!(uniform.params0[0], 0.0);
    assert_eq!(uniform.params0[1], 3.0);
    assert!((uniform.params0[3] - 1000.0).abs() < 0.001);
    assert_eq!(uniform.params1[1], 0.001);
    assert_eq!(uniform.params2, [2.0, 2.0, 0.0, 0.0]);
}

#[test]
fn ssgi_final_uniform_uses_final_composite_scales() {
    let scene_view = test_scene_view();
    let uniform = SsgiUniform::for_pass(
        SsgiSettings::default(),
        &scene_view,
        SsgiPassKind::FinalComposite,
    );

    assert_eq!(uniform.params2, [1.0, 2.0, 1.0, 0.0]);
}

#[test]
fn ssgi_decode_flips_normal_z_to_match_positive_view_z() {
    let facing_camera = decode_scene_normal_like_ssgi([0.5, 0.5, 1.0]);
    assert_eq!(facing_camera, [0.0, 0.0, -1.0]);

    let facing_away = decode_scene_normal_like_ssgi([0.5, 0.5, 0.0]);
    assert_eq!(facing_away, [0.0, 0.0, 1.0]);

    for source in [
        SSGI_COMPUTE_SHADER,
        SSGI_UPSAMPLE_COMPUTE_SHADER,
        SSGI_FINAL_SHADER,
    ] {
        assert!(
            source.contains("unit.x, unit.y, -unit.z"),
            "SSGI normal decode must mirror z because reconstructed positions use Wicked-style positive view z"
        );
    }
}

#[test]
fn ssgi_depth_reconstruct_matches_wicked_positive_view_z() {
    let projection = Mat4::perspective_rh(60.0_f32.to_radians(), 16.0 / 9.0, 0.1, 100.0);
    let view_position = crate::math::Vec4::new(0.0, 0.0, -8.0, 1.0);
    let clip = projection * view_position;
    let depth = clip.z() / clip.w();

    let reconstructed_z = reconstruct_positive_view_z_like_ssgi(projection, depth);

    assert!((reconstructed_z - 8.0).abs() < 0.001);
}
