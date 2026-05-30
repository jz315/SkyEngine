use super::super::*;
use super::common::*;

#[test]
fn read_before_write_requires_imported_or_surface_input() {
    let mut graph = RenderGraph::new();
    let a = graph.create_texture(|b| {
        b.name("a");
    });
    let b_tex = graph.create_texture(|b| {
        b.name("b");
    });

    graph.add_render_pass("pass_a", |setup| {
        setup.write(a);
        setup.read(b_tex);
    });

    graph.add_render_pass("pass_b", |setup| {
        setup.write(b_tex);
        setup.read(a);
    });

    assert!(matches!(
        graph.compile(),
        Err(RenderGraphError::ReadBeforeWrite { .. })
    ));
}

#[test]
fn stale_handle_is_rejected_after_reset() {
    let mut graph = RenderGraph::new();
    let stale = graph.create_texture(|b| {
        b.name("stale");
    });

    graph.reset();

    let current = graph.create_texture(|b| {
        b.name("current");
    });
    graph.add_render_pass("bad", |s| {
        s.read(stale);
        s.write(current);
    });

    assert!(matches!(
        graph.compile(),
        Err(RenderGraphError::InvalidResourceHandle {
            pass: Some(ref pass),
            resource: ResourceRef::Texture(handle),
        }) if pass == "bad" && handle == stale
    ));
}

#[test]
fn foreign_handle_is_rejected() {
    let mut foreign_graph = RenderGraph::new();
    let foreign = foreign_graph.create_buffer(|b| {
        b.name("foreign").size(64);
    });

    let mut graph = RenderGraph::new();
    let local = graph.create_buffer(|b| {
        b.name("local").size(64);
    });
    graph.add_compute_pass("bad", |s| {
        s.read_buffer(foreign);
        s.write_buffer(local);
    });

    assert!(matches!(
        graph.compile(),
        Err(RenderGraphError::InvalidResourceHandle {
            pass: Some(ref pass),
            resource: ResourceRef::Buffer(handle),
        }) if pass == "bad" && handle == foreign
    ));
}

#[test]
fn pass_setup_deduplicates_reads() {
    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("t");
    });

    graph.add_render_pass("reader", |s| {
        s.read(t);
        s.read(t); // duplicate
        s.write_surface();
    });

    // reads should contain only one entry for texture t (deduplicated)
    let tex_reads: Vec<_> = graph.passes[0]
        .reads
        .iter()
        .filter(|r| matches!(r, ResourceRef::Texture(_)))
        .collect();
    assert_eq!(tex_reads.len(), 1, "duplicate reads should be deduplicated");
}

#[test]
fn pass_setup_deduplicates_writes() {
    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("t");
    });

    graph.add_render_pass("writer", |s| {
        s.write(t);
        s.write(t); // duplicate
        s.write_surface();
    });

    let tex_writes: Vec<_> = graph.passes[0]
        .writes
        .iter()
        .filter(|r| matches!(r, ResourceRef::Texture(_)))
        .collect();
    assert_eq!(
        tex_writes.len(),
        1,
        "duplicate writes should be deduplicated"
    );
}

#[test]
fn set_depth_stencil_cleared_deduplicates_writes() {
    let mut graph = RenderGraph::new();
    let depth = graph.create_texture(|b| {
        b.name("depth").format(TextureFormat::Depth24PlusStencil8);
    });

    graph.add_render_pass("geometry", |s| {
        s.write_surface();
        // Call twice — should not produce duplicate write entries
        s.set_depth_stencil_cleared(depth, 1.0);
        s.set_depth_stencil_cleared(depth, 0.5);
    });

    let depth_writes: Vec<_> = graph.passes[0]
        .writes
        .iter()
        .filter(|r| **r == ResourceRef::Texture(depth))
        .collect();
    assert_eq!(
        depth_writes.len(),
        1,
        "set_depth_stencil_cleared should deduplicate writes"
    );
}

// ── Readwrite declares both ─────────────────────────────────────────

#[test]
fn readwrite_declares_both_read_and_write() {
    let mut graph = RenderGraph::new();
    let buf = graph.create_texture(|b| {
        b.name("particles");
    });

    graph.add_compute_pass("sim", |s| {
        s.readwrite(buf);
    });

    assert!(graph.passes[0].reads.contains(&ResourceRef::Texture(buf)));
    assert!(graph.passes[0].writes.contains(&ResourceRef::Texture(buf)));
}

#[test]
fn readwrite_buffer_declares_both() {
    let mut graph = RenderGraph::new();
    let buf = graph.create_buffer(|b| {
        b.name("data").size(256);
    });

    graph.add_compute_pass("process", |s| {
        s.readwrite_buffer(buf);
    });

    assert!(graph.passes[0].reads.contains(&ResourceRef::Buffer(buf)));
    assert!(graph.passes[0].writes.contains(&ResourceRef::Buffer(buf)));
}

#[test]
fn readwrite_subresource_declares_both() {
    let mut graph = RenderGraph::new();
    let texture = graph.create_texture(|b| {
        b.name("atlas").mip_level_count(4).array_layer_count(16);
    });
    let subresource = TextureSubresource::new(texture, 2, 1, 7, 1);

    graph.add_compute_pass("update_layer", |s| {
        s.readwrite_subresource(subresource);
    });

    let resource = ResourceRef::TextureSubresource(subresource);
    assert!(graph.passes[0].reads.contains(&resource));
    assert!(graph.passes[0].writes.contains(&resource));
}

#[test]
fn subresource_read_after_write_creates_dependency() {
    let mut graph = RenderGraph::new();
    let texture = graph.create_texture(|b| {
        b.name("atlas").mip_level_count(4).array_layer_count(16);
    });
    let subresource = TextureSubresource::new(texture, 1, 1, 3, 1);

    let writer = graph.add_compute_pass("write_subresource", |s| {
        s.write_subresource(subresource);
    });
    let reader = graph.add_compute_pass("read_subresource", |s| {
        s.read_subresource(subresource);
        s.write_surface();
    });

    let compiled = graph.compile().unwrap();
    assert_eq!(compiled[0].handle, writer);
    assert_eq!(compiled[1].handle, reader);
}

#[test]
fn non_overlapping_subresource_writes_can_coexist() {
    let mut graph = RenderGraph::new();
    let texture = graph.create_texture(|b| {
        b.name("atlas")
            .mip_level_count(4)
            .array_layer_count(16)
            .persistent();
    });
    let mip0 = TextureSubresource::new(texture, 0, 1, 0, 1);
    let mip1 = TextureSubresource::new(texture, 1, 1, 0, 1);

    graph.add_compute_pass("write_mip0", |s| {
        s.write_subresource(mip0);
    });
    graph.add_compute_pass("write_mip1", |s| {
        s.write_subresource(mip1);
    });

    let compiled = graph.compile().unwrap();
    assert_eq!(compiled.len(), 2);
    assert_eq!(compiled[0].dep_level, 0);
    assert_eq!(compiled[1].dep_level, 0);
}

#[test]
fn whole_texture_read_depends_on_subresource_write() {
    let mut graph = RenderGraph::new();
    let texture = graph.create_texture(|b| {
        b.name("atlas").mip_level_count(4).array_layer_count(16);
    });
    let subresource = TextureSubresource::new(texture, 2, 1, 7, 1);

    let writer = graph.add_compute_pass("write_subresource", |s| {
        s.write_subresource(subresource);
    });
    let whole_reader = graph.add_compute_pass("read_whole", |s| {
        s.read_texture(texture);
        s.write_surface();
    });

    let compiled = graph.compile().unwrap();
    assert_eq!(compiled[0].handle, writer);
    assert_eq!(compiled[1].handle, whole_reader);
}

#[test]
fn subresource_handles_reject_foreign_texture_handles() {
    let mut source_graph = RenderGraph::new();
    let foreign_texture = source_graph.create_texture(|b| {
        b.name("foreign").mip_level_count(2).array_layer_count(2);
    });

    let mut graph = RenderGraph::new();
    let foreign_subresource = TextureSubresource::new(foreign_texture, 0, 1, 0, 1);
    graph.add_compute_pass("bad", |s| {
        s.write_subresource(foreign_subresource);
        s.write_surface();
    });

    assert!(matches!(
        graph.compile(),
        Err(RenderGraphError::InvalidResourceHandle {
            resource: ResourceRef::TextureSubresource(_),
            ..
        })
    ));
}

// ── Cycle detection ─────────────────────────────────────────────────

#[test]
#[should_panic(expected = "MRT slot 0 already declared")]
fn duplicate_mrt_slot_panics() {
    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("rt");
    });

    graph.add_render_pass("bad", |s| {
        s.write_color(0, t);
        s.write_color(0, t); // duplicate slot
    });
}

#[test]
fn buffer_to_texture_rejects_too_small_source_buffer() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [16, 16],
    );

    let mut graph = RenderGraph::new();
    let src = graph.create_buffer(|b| {
        b.name("src").size(256);
    });
    let dst = graph.create_texture(|b| {
        b.name("dst")
            .size(TargetSize::Exact(16, 16))
            .format(TextureFormat::Rgba8Unorm);
    });

    graph.add_render_pass("fill", |s| {
        s.write_buffer(src);
    });
    graph.add_copy_pass("upload", |s| {
        s.buffer_to_texture_with_layout(src, dst, Some(256), Some(16));
    });
    graph.add_render_pass("present", |s| {
        s.read(dst);
        s.write_surface();
    });

    let err = graph
        .try_execute(&mut ctx, |_pass, _gpu, _resources| Ok(()))
        .unwrap_err();
    assert!(matches!(
        err,
        RenderGraphError::InvalidBufferTextureCopyLayout {
            buffer,
            texture,
            ref details,
        } if buffer == src
            && texture == dst
            && details.contains("needs 3904 bytes")
            && details.contains("has 256")
    ));
}

#[test]
fn upload_to_texture_rejects_mismatched_data_length() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [16, 16],
    );

    let mut graph = RenderGraph::new();
    let dst = graph.create_texture(|b| {
        b.name("dst")
            .size(TargetSize::Exact(4, 4))
            .format(TextureFormat::Rgba8Unorm);
    });

    graph.add_copy_pass("upload", |s| {
        s.upload_to_texture(vec![0u8; 63], dst, 4, 4, 4);
    });
    graph.add_render_pass("present", |s| {
        s.read(dst);
        s.write_surface();
    });

    let err = graph
        .try_execute(&mut ctx, |_pass, _gpu, _resources| Ok(()))
        .unwrap_err();
    assert!(matches!(
        err,
        RenderGraphError::InvalidTextureUpload { texture, .. } if texture == dst
    ));
}
