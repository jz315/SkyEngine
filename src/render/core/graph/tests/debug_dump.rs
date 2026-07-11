use super::super::*;
use super::common::*;

#[test]
fn debug_dump_reports_pass_order_culling_and_lifetimes() {
    let mut graph = RenderGraph::new();
    let dead = graph.create_texture(|b| {
        b.name("dead_target");
    });
    let a = graph.create_texture(|b| {
        b.name("a");
    });
    let b = graph.create_texture(|b| {
        b.name("b");
    });

    graph.add_render_pass("dead", |s| {
        s.write(dead);
    });
    graph.add_render_pass("write_a", |s| {
        s.write(a);
    });
    graph.add_render_pass("a_to_b", |s| {
        s.read(a);
        s.write(b);
    });
    graph.add_render_pass("present", |s| {
        s.read(b);
        s.write_surface();
    });

    let compiled = graph.compile().unwrap();
    let dump = graph.debug_dump();
    let dot = graph.export_dot_with_options(RenderGraphDotOptions::detailed());

    assert!(dump.compiled);
    assert_eq!(dump.passes.len(), 4);
    assert_eq!(dump.culled_count, 1);
    assert_eq!(
        dump.compiled_execution_order,
        compiled.iter().map(|pass| pass.index).collect::<Vec<_>>()
    );

    let dead_pass = dump
        .passes
        .iter()
        .find(|pass| pass.name == "dead")
        .expect("dead pass should be present in declaration-order dump");
    assert_eq!(dead_pass.declaration_order, 0);
    assert!(!dead_pass.alive);
    assert_eq!(dead_pass.execution_order, None);
    assert!(dot.contains("culled"));
    assert!(dot.contains("style=\"filled,dashed\""));
    assert!(dot.contains("lifetime 0..1"));

    let write_a = dump
        .passes
        .iter()
        .find(|pass| pass.name == "write_a")
        .expect("write_a pass should be present");
    assert!(write_a.alive);
    assert_eq!(write_a.execution_order, Some(0));

    let a_lifetime = dump
        .lifetimes
        .iter()
        .find(|lifetime| lifetime.resource == ResourceRef::Texture(a))
        .expect("texture a lifetime should be reported");
    assert_eq!(a_lifetime.first_use, 0);
    assert_eq!(a_lifetime.last_use, 1);

    let b_lifetime = dump
        .lifetimes
        .iter()
        .find(|lifetime| lifetime.resource == ResourceRef::Texture(b))
        .expect("texture b lifetime should be reported");
    assert_eq!(b_lifetime.first_use, 1);
    assert_eq!(b_lifetime.last_use, 2);

    let surface_lifetime = dump
        .lifetimes
        .iter()
        .find(|lifetime| lifetime.resource == ResourceRef::Surface)
        .expect("surface lifetime should be reported");
    assert_eq!(surface_lifetime.first_use, 2);
    assert_eq!(surface_lifetime.last_use, 2);

    assert!(
        !dump
            .lifetimes
            .iter()
            .any(|lifetime| lifetime.resource == ResourceRef::Texture(dead)),
        "culled transient output should not have a live lifetime"
    );

    let surface = dump
        .resources
        .iter()
        .find(|resource| resource.resource == ResourceRef::Surface)
        .expect("surface debug resource should be present");
    assert_eq!(surface.kind, RenderGraphResourceKind::Surface);
    assert!(surface.external_source);
    assert!(surface.external_sink);
    assert!(surface.live);

    let a_resource = dump
        .resources
        .iter()
        .find(|resource| resource.resource == ResourceRef::Texture(a))
        .expect("texture a debug resource should be present");
    assert_eq!(a_resource.name, "a");
    assert_eq!(a_resource.kind, RenderGraphResourceKind::Texture);
    assert!(a_resource.transient);
    assert!(!a_resource.persistent);
    assert!(!a_resource.imported);
    assert!(a_resource.live);
    assert!(!a_resource.external_source);
    assert!(!a_resource.external_sink);
    assert_eq!(
        a_resource.texture.as_ref().map(|desc| desc.format),
        Some(TextureFormat::Rgba8Unorm)
    );
}

#[test]
fn debug_dump_reports_alias_groups_after_allocation() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let temp_a = graph.create_texture(|b| {
        b.name("temp_a")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });
    let temp_b = graph.create_texture(|b| {
        b.name("temp_b")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });
    let bridge = graph.create_texture(|b| {
        b.name("bridge")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba16Float);
    });

    graph.add_render_pass("write_temp_a", |s| {
        s.write(temp_a);
    });
    graph.add_render_pass("consume_temp_a", |s| {
        s.read(temp_a);
        s.write(bridge);
    });
    graph.add_render_pass("produce_temp_b", |s| {
        s.read(bridge);
        s.write(temp_b);
    });
    graph.add_render_pass("present", |s| {
        s.read(temp_b);
        s.write_surface();
    });

    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);
    let dump = graph.debug_dump();
    let dot = graph.export_dot_with_options(RenderGraphDotOptions::detailed());

    let aliasing = dump
        .aliasing
        .as_ref()
        .expect("aliasing stats should be present after allocation");
    assert!(aliasing.total_aliased_textures >= 2);
    assert!(aliasing.compression_ratio > 0.0);

    let shared_group = dump
        .alias_groups
        .iter()
        .find(|group| {
            let names = group
                .members
                .iter()
                .map(|member| member.name.as_ref())
                .collect::<Vec<_>>();
            names.contains(&"temp_a") && names.contains(&"temp_b")
        })
        .expect("temp_a and temp_b should share an alias group");
    assert_eq!(shared_group.format, TextureFormat::Rgba8Unorm);
    assert_eq!(shared_group.width, 64);
    assert_eq!(shared_group.height, 64);
    assert_eq!(shared_group.members.iter().filter(|m| m.primary).count(), 1);

    assert!(
        dump.alias_redirects
            .iter()
            .any(|redirect| redirect.from == temp_a || redirect.from == temp_b),
        "one alias group member should redirect to the primary"
    );
    assert!(dot.contains("alias_group_"));
    assert!(dot.contains("Alias group"));
    assert!(dot.contains("alias group"));

    graph.destroy_physical_resources();
}

#[test]
fn debug_dump_reports_copy_ops_without_cloning_upload_payload() {
    let mut graph = RenderGraph::new();
    let texture = graph.create_texture(|b| {
        b.name("uploaded")
            .size(TargetSize::Exact(2, 2))
            .format(TextureFormat::Rgba8Unorm)
            .copy_dst();
    });
    let data = vec![255u8; 16];

    graph.add_copy_pass("upload", |s| {
        s.upload_to_texture(data, texture, 2, 2, 4);
    });
    graph.add_render_pass("present", |s| {
        s.read(texture);
        s.write_surface();
    });

    graph.compile().unwrap();
    let dump = graph.debug_dump();
    let dot = graph.export_dot_with_options(RenderGraphDotOptions::detailed());
    let upload = dump
        .passes
        .iter()
        .find(|pass| pass.name == "upload")
        .expect("upload pass should be present");
    assert_eq!(upload.copy_ops.len(), 1);
    assert_eq!(
        upload.copy_ops[0],
        CopyOpDebug::UploadToTexture {
            dst: texture,
            width: 2,
            height: 2,
            bytes_per_pixel: 4,
            data_len: 16,
        }
    );
    assert_eq!(upload.writes, vec![ResourceRef::Texture(texture)]);
    assert!(dot.contains("UploadToTexture(16 bytes)"));
}

#[test]
fn detailed_dot_reports_subresource_edge_labels() {
    let mut graph = RenderGraph::new();
    let texture = graph.create_texture(|b| {
        b.name("array_texture")
            .mip_level_count(4)
            .array_layer_count(8)
            .storage_binding();
    });
    let subresource = TextureSubresource::new(texture, 2, 1, 3, 1);

    graph.add_compute_pass("write_subresource", |s| {
        s.write_subresource(subresource);
    });
    graph.add_compute_pass("read_subresource", |s| {
        s.read_subresource(subresource);
        s.write_surface();
    });

    graph.compile().unwrap();
    let dot = graph.export_dot_with_options(RenderGraphDotOptions::detailed());
    assert!(dot.contains("m2+1 l3+1"));
}
