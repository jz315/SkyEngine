use super::super::*;
use super::common::*;

#[test]
fn alias_group_members_share_same_physical_texture() {
    // Two transient textures with non-overlapping lifetimes and same format
    // should alias to ONE physical RenderTarget.
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let t1 = graph.create_texture(|b| {
        b.name("temp_a")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });
    let t2 = graph.create_texture(|b| {
        b.name("temp_b")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });

    // Create a chain where t1's lifetime ends before t2's begins.
    // Use a third texture as a bridge to break the overlap.
    let bridge = graph.create_texture(|b| {
        b.name("bridge")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba16Float); // different format → won't alias
    });

    // Pass 0: write t1
    graph.add_render_pass("write_t1", |s| {
        s.write(t1);
    });
    // Pass 1: read t1, write bridge (t1 lifetime ends here)
    graph.add_render_pass("consume_t1", |s| {
        s.read(t1);
        s.write(bridge);
    });
    // Pass 2: read bridge, write t2 (t2 lifetime starts here, after t1 is dead)
    graph.add_render_pass("produce_t2", |s| {
        s.read(bridge);
        s.write(t2);
    });
    // Pass 3: present t2
    graph.add_render_pass("present", |s| {
        s.read(t2);
        s.write_surface();
    });

    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    // Both handles should resolve to the SAME underlying wgpu::Texture.
    let tex1_ptr = graph.try_resolve_texture(t1).unwrap() as *const wgpu::Texture;
    let tex2_ptr = graph.try_resolve_texture(t2).unwrap() as *const wgpu::Texture;

    // If aliasing works, they point to the same RenderTarget.
    // If aliasing produced two separate textures, this fails.
    assert_eq!(
        tex1_ptr, tex2_ptr,
        "alias group members should share the same physical texture, \
         but got two different textures"
    );

    // Verify stats reflect the aliasing.
    let stats = graph.alias_stats().expect("alias_stats should be present");
    assert!(
        stats.total_aliased_textures >= 2,
        "expected at least 2 aliased textures, got {}",
        stats.total_aliased_textures
    );
    assert!(
        stats.compression_ratio > 0.0,
        "expected positive compression ratio, got {}",
        stats.compression_ratio
    );

    graph.destroy_physical_resources();
}

#[test]
fn alias_release_does_not_double_return_to_pool() {
    // After aliasing, release should only return ONE target to the pool
    // per alias group, not N (one per member).
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let t1 = graph.create_texture(|b| {
        b.name("alias_a")
            .size(TargetSize::Exact(32, 32))
            .format(TextureFormat::Rgba8Unorm);
    });
    let t2 = graph.create_texture(|b| {
        b.name("alias_b")
            .size(TargetSize::Exact(32, 32))
            .format(TextureFormat::Rgba8Unorm);
    });

    graph.add_render_pass("write_a", |s| {
        s.write(t1);
    });
    graph.add_render_pass("read_a_write_b", |s| {
        s.read(t1);
        s.write(t2);
    });
    graph.add_render_pass("present_b", |s| {
        s.read(t2);
        s.write_surface();
    });

    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);
    graph.release_transient_resources(&ctx);

    // After release, primary's slot should be empty (released to pool).
    assert!(graph.physical_textures[t1.0].is_none());
    // Secondary's slot should also be empty (was always None due to redirect).
    assert!(graph.physical_textures[t2.0].is_none());

    // Allocate again — if pool received exactly 1 target back, the second
    // allocate will acquire from pool + create at most 0 new textures
    // for the aliased pair.
    graph.compiled = false;
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    // Should still work — both handles resolve.
    assert!(graph.try_resolve_texture(t1).is_ok());
    assert!(graph.try_resolve_texture(t2).is_ok());

    graph.destroy_physical_resources();
}

#[test]
fn non_overlapping_different_format_textures_dont_alias() {
    // Same lifetime pattern but different formats → must NOT share.
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let t_rgba8 = graph.create_texture(|b| {
        b.name("rgba8")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });
    let t_rgba16 = graph.create_texture(|b| {
        b.name("rgba16f")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba16Float);
    });

    graph.add_render_pass("use_rgba8", |s| {
        s.write(t_rgba8);
    });
    graph.add_render_pass("use_rgba16", |s| {
        s.read(t_rgba8);
        s.write(t_rgba16);
    });
    graph.add_render_pass("present", |s| {
        s.read(t_rgba16);
        s.write_surface();
    });

    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    // Different formats → different physical textures.
    let tex1_ptr = graph.try_resolve_texture(t_rgba8).unwrap() as *const wgpu::Texture;
    let tex2_ptr = graph.try_resolve_texture(t_rgba16).unwrap() as *const wgpu::Texture;
    assert_ne!(
        tex1_ptr, tex2_ptr,
        "textures with different formats should NOT alias"
    );

    graph.destroy_physical_resources();
}

#[test]
fn overlapping_lifetime_textures_dont_alias() {
    // Textures alive at the same time must NOT share.
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let t1 = graph.create_texture(|b| {
        b.name("overlap_a")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });
    let t2 = graph.create_texture(|b| {
        b.name("overlap_b")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });

    // Both textures are alive simultaneously in "use_both".
    graph.add_render_pass("write_both", |s| {
        s.write(t1);
        s.write(t2);
    });
    graph.add_render_pass("use_both", |s| {
        s.read(t1);
        s.read(t2);
        s.write_surface();
    });

    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    // Overlapping → must be different physical textures.
    let tex1_ptr = graph.try_resolve_texture(t1).unwrap() as *const wgpu::Texture;
    let tex2_ptr = graph.try_resolve_texture(t2).unwrap() as *const wgpu::Texture;
    assert_ne!(
        tex1_ptr, tex2_ptr,
        "textures with overlapping lifetimes should NOT alias"
    );

    graph.destroy_physical_resources();
}

#[test]
fn imported_texture_not_aliased() {
    // Imported (external) textures must never be placed in alias groups.
    let (device, queue) = create_test_device();

    let texture = Arc::new(device.create_texture(&wgpu::TextureDescriptor {
        label: Some("imported"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    }));
    let view = Arc::new(texture.create_view(&wgpu::TextureViewDescriptor::default()));

    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let ext = graph.create_texture(|b| {
        b.name("imported")
            .import(Arc::clone(&texture), Arc::clone(&view));
    });
    let transient = graph.create_texture(|b| {
        b.name("transient")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });

    graph.add_render_pass("use_ext", |s| {
        s.read(ext);
        s.write(transient);
    });
    graph.add_render_pass("present", |s| {
        s.read(transient);
        s.write_surface();
    });

    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    // The imported texture should NOT be in any alias redirect.
    assert!(
        !graph.alias_redirects.contains_key(&ext.0),
        "imported texture should never be aliased"
    );
    // And the transient should resolve to a pool-allocated texture, not the imported one.
    let ext_ptr = graph.try_resolve_texture(ext).unwrap() as *const wgpu::Texture;
    let trans_ptr = graph.try_resolve_texture(transient).unwrap() as *const wgpu::Texture;
    assert_ne!(
        ext_ptr, trans_ptr,
        "imported and transient should be different textures"
    );

    graph.destroy_physical_resources();
}

#[test]
fn persistent_texture_not_aliased() {
    // Persistent (non-transient) textures must not be aliased.
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let p1 = graph.create_texture(|b| {
        b.name("persist_a")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm)
            .persistent();
    });
    let p2 = graph.create_texture(|b| {
        b.name("persist_b")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm)
            .persistent();
    });

    graph.add_render_pass("use_p1", |s| {
        s.write(p1);
    });
    graph.add_render_pass("use_p2", |s| {
        s.read(p1);
        s.write(p2);
    });
    graph.add_render_pass("present", |s| {
        s.read(p2);
        s.write_surface();
    });

    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    // Persistent textures should have separate physical textures.
    let p1_ptr = graph.try_resolve_texture(p1).unwrap() as *const wgpu::Texture;
    let p2_ptr = graph.try_resolve_texture(p2).unwrap() as *const wgpu::Texture;
    assert_ne!(p1_ptr, p2_ptr, "persistent textures should NOT alias");

    graph.destroy_physical_resources();
}

// ── API coverage gap tests ─────────────────────────────────────────────

#[test]
fn alias_group_count_matches_groups() {
    let mut graph = RenderGraph::new();
    let t1 = graph.create_texture(|b| {
        b.name("a1")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });
    let t2 = graph.create_texture(|b| {
        b.name("a2")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });

    let bridge = graph.create_texture(|b| {
        b.name("bridge")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba16Float);
    });
    graph.add_render_pass("w1", |s| {
        s.write(t1);
    });
    graph.add_render_pass("bridge_pass", |s| {
        s.read(t1);
        s.write(bridge);
    });
    graph.add_render_pass("w2", |s| {
        s.read(bridge);
        s.write(t2);
    });
    graph.add_render_pass("present", |s| {
        s.read(t2);
        s.write_surface();
    });

    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    assert!(graph.alias_group_count() > 0);

    graph.destroy_physical_resources();
}

#[test]
fn pass_local_read_write_textures_do_not_alias() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let scene = graph.create_texture(|b| {
        b.name("scene_rt")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba16Float);
    });
    let light = graph.create_texture(|b| {
        b.name("light_rt")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba16Float);
    });
    let hdr = graph.create_texture(|b| {
        b.name("hdr_rt")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba16Float);
    });
    let bloom = graph.create_texture(|b| {
        b.name("bloom_rt")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba16Float);
    });

    graph.add_render_pass("scene", |s| {
        s.write_color_cleared(0, scene, [0.0, 0.0, 0.0, 1.0]);
    });
    graph.add_render_pass("lighting", |s| {
        s.write_color_cleared(0, light, [0.0, 0.0, 0.0, 1.0]);
    });
    graph.add_render_pass("composite", |s| {
        s.read(scene);
        s.read(light);
        s.write(hdr);
    });
    graph.add_render_pass("bloom", |s| {
        s.read(hdr);
        s.write(bloom);
    });
    graph.add_render_pass("tonemap", |s| {
        s.read(bloom);
        s.write_surface();
    });

    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    let hdr_ptr = graph.try_resolve_texture(hdr).unwrap() as *const wgpu::Texture;
    let bloom_ptr = graph.try_resolve_texture(bloom).unwrap() as *const wgpu::Texture;
    assert_ne!(
        hdr_ptr, bloom_ptr,
        "textures read and written by the same graph pass must not alias"
    );

    graph.destroy_physical_resources();
}

#[test]
fn aliased_owner_changes_execute_across_encoder_boundary() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [32, 32],
    );

    let mut graph = RenderGraph::new();
    let first = graph.create_texture(|b| {
        b.name("first")
            .size(TargetSize::Exact(32, 32))
            .format(TextureFormat::Rgba8Unorm);
    });
    let second = graph.create_texture(|b| {
        b.name("second")
            .size(TargetSize::Exact(32, 32))
            .format(TextureFormat::Rgba8Unorm);
    });
    let bridge = graph.create_texture(|b| {
        b.name("bridge")
            .size(TargetSize::Exact(32, 32))
            .format(TextureFormat::Rgba16Float);
    });

    graph.add_render_pass("write_first", |s| {
        s.write_color_cleared(0, first, [0.0, 0.0, 0.0, 1.0]);
    });
    graph.add_render_pass("consume_first", |s| {
        s.read(first);
        s.write_color_cleared(0, bridge, [0.0, 0.0, 0.0, 1.0]);
    });
    graph.add_render_pass("write_second", |s| {
        s.read(bridge);
        s.write_color_cleared(0, second, [0.0, 0.0, 0.0, 1.0]);
    });
    graph.add_render_pass("present", |s| {
        s.read(second);
        s.write_surface();
    });

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    graph
        .try_execute(&mut ctx, |pass, gpu, resources| {
            for output in &pass.color_outputs {
                let ResourceRef::Texture(handle) = output.target else {
                    continue;
                };
                let target = resources.render_target(handle).expect("render target");
                let load = match output.load {
                    LoadOp::Clear(color) => wgpu::LoadOp::Clear(wgpu::Color {
                        r: color[0] as f64,
                        g: color[1] as f64,
                        b: color[2] as f64,
                        a: color[3] as f64,
                    }),
                    LoadOp::Load => wgpu::LoadOp::Load,
                    LoadOp::DontCare => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                };
                let mut frame = gpu.frame();
                let _pass = frame.begin_target_pass("alias_owner_change_test", target, load);
            }
            Ok(())
        })
        .expect("aliased owner changes should execute without wgpu validation errors");
    ctx.end_frame();

    assert!(graph.alias_group_count() > 0);
    graph.destroy_physical_resources();
}
