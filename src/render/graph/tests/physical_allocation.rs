use super::super::*;
use super::common::*;

#[test]
fn persistent_texture_has_transient_false() {
    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("persist").persistent();
    });
    assert!(!graph.textures[t.0].transient);
}

#[test]
fn default_texture_is_transient() {
    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("tmp");
    });
    assert!(graph.textures[t.0].transient);
}

#[test]
fn storage_only_texture_allocates_with_storage_binding_usage() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let storage = graph.create_texture(|b| {
        b.name("storage")
            .size(TargetSize::Exact(32, 32))
            .format(TextureFormat::Rgba8Unorm)
            .usage(wgpu::TextureUsages::STORAGE_BINDING)
            .persistent();
    });
    graph.add_compute_pass("write_storage", |s| {
        s.write(storage);
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    let target = graph.physical_texture(storage);
    assert_eq!(target.usage(), wgpu::TextureUsages::STORAGE_BINDING);

    graph.destroy_physical_resources();
}

#[test]
fn transient_pool_separates_storage_and_render_attachment_usage() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );
    let mut pool = TransientPool::new();
    let storage_key = PoolKey {
        format: TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING,
        width: 32,
        height: 32,
        sample_count: 1,
        mip_level_count: 1,
        array_layer_count: 1,
    };
    let render_key = PoolKey {
        usage: DEFAULT_TEXTURE_USAGE,
        ..storage_key
    };

    let storage_target = pool.acquire(&ctx, storage_key, "storage_target".into());
    pool.release(storage_key, storage_target);
    let render_target = pool.acquire(&ctx, render_key, "render_target".into());

    assert_eq!(render_target.usage(), DEFAULT_TEXTURE_USAGE);
}

#[test]
fn physical_resources_view_follows_alias_redirect() {
    // PhysicalResources::view() must return a valid view for aliased secondaries.
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let t1 = graph.create_texture(|b| {
        b.name("pr_a")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });
    let t2 = graph.create_texture(|b| {
        b.name("pr_b")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });

    graph.add_render_pass("w1", |s| {
        s.write(t1);
    });
    graph.add_render_pass("r1w2", |s| {
        s.read(t1);
        s.write(t2);
    });
    graph.add_render_pass("present", |s| {
        s.read(t2);
        s.write_surface();
    });

    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    let resources = PhysicalResources {
        handle_token: graph.handle_token,
        textures: &graph.physical_textures,
        buffers: &graph.physical_buffers,
        texture_descs: &graph.textures,
        buffer_descs: &graph.buffers,
        alias_redirects: &graph.alias_redirects,
        blackboard: graph.blackboard_ref(),
        view_stats: None,
    };

    // Both handles must resolve to valid views without panicking.
    let _v1 = resources.view(t1);
    let _v2 = resources.view(t2);

    // And both render_target() calls should return Some.
    assert!(resources.render_target(t1).is_some());
    assert!(resources.render_target(t2).is_some());

    graph.destroy_physical_resources();
}

#[test]
fn destroy_physical_resources_clears_all_slots() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("x")
            .size(TargetSize::Exact(32, 32))
            .format(TextureFormat::Rgba8Unorm);
    });
    graph.add_render_pass("p", |s| {
        s.write(t);
        s.write_surface();
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    assert!(graph.try_physical_texture(t).is_ok());
    graph.destroy_physical_resources();
    assert!(graph.try_physical_texture(t).is_err());
}

#[test]
fn clear_frame_reuses_persistent_texture_allocation() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let first = graph.create_texture(|b| {
        b.name("history")
            .persistent()
            .size(TargetSize::Exact(32, 32))
            .format(TextureFormat::Rgba8Unorm);
    });
    graph.add_render_pass("write_history", |s| {
        s.write(first);
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    let first_ptr = graph.physical_texture(first).texture() as *const wgpu::Texture;

    graph.clear_frame();

    let second = graph.create_texture(|b| {
        b.name("history")
            .persistent()
            .size(TargetSize::Exact(32, 32))
            .format(TextureFormat::Rgba8Unorm);
    });
    graph.add_render_pass("write_history_again", |s| {
        s.write(second);
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    let second_ptr = graph.physical_texture(second).texture() as *const wgpu::Texture;
    assert_eq!(
        first_ptr, second_ptr,
        "clear_frame should preserve persistent texture allocations across frames"
    );

    graph.destroy_physical_resources();
}

#[test]
fn persistent_buffer_recreated_when_size_grows() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let first = graph.create_buffer(|b| {
        b.name("persistent_buf").persistent().size(64);
    });
    graph.add_compute_pass("write_buf", |s| {
        s.write_buffer(first);
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);
    assert_eq!(graph.physical_buffer(first).size(), 64);

    graph.clear_frame();

    let second = graph.create_buffer(|b| {
        b.name("persistent_buf").persistent().size(256);
    });
    graph.add_compute_pass("write_buf_again", |s| {
        s.write_buffer(second);
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    assert_eq!(
        graph.physical_buffer(second).size(),
        256,
        "persistent buffers should be recreated when the requested size grows"
    );

    graph.destroy_physical_resources();
}

#[test]
fn try_physical_texture_invalid_handle() {
    let graph = RenderGraph::new();
    let fake_handle = TextureHandle(999, 0);
    let err = graph.try_physical_texture(fake_handle);
    assert!(err.is_err());
}

#[test]
#[should_panic]
fn physical_texture_panics_on_invalid_handle() {
    let graph = RenderGraph::new();
    let fake_handle = TextureHandle(999, 0);
    let _ = graph.physical_texture(fake_handle);
}

#[test]
fn try_physical_buffer_returns_error_before_allocation() {
    let mut graph = RenderGraph::new();
    let b = graph.create_buffer(|bld| {
        bld.name("buf").size(256);
    });
    // No allocation done yet → should be Err.
    graph.add_render_pass("p", |s| {
        s.write_buffer(b);
        s.write_surface();
    });
    graph.compile().unwrap();
    // Physical buffers vec is empty before allocate.
    let err = graph.try_physical_buffer(b);
    assert!(err.is_err());
}

#[test]
#[should_panic]
fn physical_buffer_panics_on_missing() {
    let mut graph = RenderGraph::new();
    let b = graph.create_buffer(|bld| {
        bld.name("buf").size(256);
    });
    graph.add_render_pass("p", |s| {
        s.write_buffer(b);
        s.write_surface();
    });
    graph.compile().unwrap();
    let _ = graph.physical_buffer(b);
}

#[test]
fn buffer_physical_allocation_roundtrip() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let b = graph.create_buffer(|bld| {
        bld.name("storage")
            .size(1024)
            .usage(wgpu::BufferUsages::STORAGE);
    });
    graph.add_render_pass("compute", |s| {
        s.write_buffer(b);
        s.write_surface();
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    let buf = graph.try_physical_buffer(b).unwrap();
    assert!(buf.size() >= 1024);
    // Also test the panicking variant succeeds.
    let _ = graph.physical_buffer(b);

    graph.destroy_physical_resources();
}

#[test]
fn physical_resources_texture_ref_returns_correct_data() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("ref_test")
            .size(TargetSize::Exact(128, 64))
            .format(TextureFormat::Rgba8Unorm);
    });
    graph.add_render_pass("w", |s| {
        s.write(t);
        s.write_surface();
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    let resources = PhysicalResources {
        handle_token: graph.handle_token,
        textures: &graph.physical_textures,
        buffers: &graph.physical_buffers,
        texture_descs: &graph.textures,
        buffer_descs: &graph.buffers,
        alias_redirects: &graph.alias_redirects,
        blackboard: graph.blackboard_ref(),
        view_stats: None,
    };

    let tex_ref = resources.texture_ref(t);
    assert_eq!(tex_ref.size, [128, 64]);
    assert_eq!(tex_ref.format, wgpu::TextureFormat::Rgba8Unorm);
    assert!(tex_ref.render_target.is_some());

    graph.destroy_physical_resources();
}

#[test]
fn physical_resources_texture_subresource_view_creates_mip_layer_view() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let texture = graph.create_texture(|b| {
        b.name("atlas")
            .size(TargetSize::Exact(128, 128))
            .format(TextureFormat::Rgba8Unorm)
            .mip_level_count(4)
            .array_layer_count(8)
            .persistent();
    });
    let subresource = TextureSubresource::new(texture, 2, 1, 3, 1);
    graph.add_compute_pass("write_subresource", |s| {
        s.write_subresource(subresource);
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    let resources = PhysicalResources {
        handle_token: graph.handle_token,
        textures: &graph.physical_textures,
        buffers: &graph.physical_buffers,
        texture_descs: &graph.textures,
        buffer_descs: &graph.buffers,
        alias_redirects: &graph.alias_redirects,
        blackboard: graph.blackboard_ref(),
        view_stats: None,
    };

    let _view = resources.texture_subresource_view(subresource, wgpu::TextureViewDimension::D2);

    graph.destroy_physical_resources();
}

#[test]
fn storage_texture_view_accepts_storage_usage() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let texture = graph.create_texture(|b| {
        b.name("storage_atlas")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm)
            .usage(wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING)
            .mip_level_count(2)
            .array_layer_count(4)
            .persistent();
    });
    let subresource = TextureSubresource::new(texture, 1, 1, 0, 4);
    graph.add_compute_pass("write_storage", |s| {
        s.write_subresource(subresource);
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    let resources = PhysicalResources {
        handle_token: graph.handle_token,
        textures: &graph.physical_textures,
        buffers: &graph.physical_buffers,
        texture_descs: &graph.textures,
        buffer_descs: &graph.buffers,
        alias_redirects: &graph.alias_redirects,
        blackboard: graph.blackboard_ref(),
        view_stats: None,
    };

    let _view = resources.storage_texture_view(subresource, wgpu::TextureViewDimension::D2Array);

    graph.destroy_physical_resources();
}

#[test]
#[should_panic(expected = "storage_texture_view requires STORAGE_BINDING usage")]
fn storage_texture_view_panics_without_storage_usage() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let texture = graph.create_texture(|b| {
        b.name("sampled")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm)
            .array_layer_count(2)
            .persistent();
    });
    let subresource = TextureSubresource::new(texture, 0, 1, 0, 1);
    graph.add_compute_pass("write_sampled", |s| {
        s.write_subresource(subresource);
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    let resources = PhysicalResources {
        handle_token: graph.handle_token,
        textures: &graph.physical_textures,
        buffers: &graph.physical_buffers,
        texture_descs: &graph.textures,
        buffer_descs: &graph.buffers,
        alias_redirects: &graph.alias_redirects,
        blackboard: graph.blackboard_ref(),
        view_stats: None,
    };

    let _view = resources.storage_texture_view(subresource, wgpu::TextureViewDimension::D2);
}

#[test]
#[should_panic(expected = "render_attachment_view requires exactly one array layer")]
fn render_attachment_view_panics_for_multi_layer_range() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let texture = graph.create_texture(|b| {
        b.name("attachment_array")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm)
            .array_layer_count(2)
            .persistent();
    });
    let subresource = TextureSubresource::new(texture, 0, 1, 0, 2);
    graph.add_render_pass("write_attachment", |s| {
        s.write_subresource(subresource);
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    let resources = PhysicalResources {
        handle_token: graph.handle_token,
        textures: &graph.physical_textures,
        buffers: &graph.physical_buffers,
        texture_descs: &graph.textures,
        buffer_descs: &graph.buffers,
        alias_redirects: &graph.alias_redirects,
        blackboard: graph.blackboard_ref(),
        view_stats: None,
    };

    let _view = resources.render_attachment_view(subresource);
}

#[test]
fn physical_resources_buffer_resolves() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let b = graph.create_buffer(|bld| {
        bld.name("pr_buf")
            .size(512)
            .usage(wgpu::BufferUsages::STORAGE);
    });
    graph.add_render_pass("use", |s| {
        s.write_buffer(b);
        s.write_surface();
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);

    let resources = PhysicalResources {
        handle_token: graph.handle_token,
        textures: &graph.physical_textures,
        buffers: &graph.physical_buffers,
        texture_descs: &graph.textures,
        buffer_descs: &graph.buffers,
        alias_redirects: &graph.alias_redirects,
        blackboard: graph.blackboard_ref(),
        view_stats: None,
    };

    let buf = resources.buffer(b);
    assert!(buf.size() >= 512);

    graph.destroy_physical_resources();
}

#[test]
fn physical_resource_view_stats_count_latest_execution() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [16, 16],
    );

    let mut graph = RenderGraph::new();
    let texture = graph.create_texture(|b| {
        b.name("tracked_views")
            .size(TargetSize::Exact(8, 8))
            .format(TextureFormat::Rgba8Unorm)
            .mip_level_count(2)
            .storage_binding()
            .render_attachment()
            .sampled();
    });
    let subresource = TextureSubresource::new(texture, 0, 1, 0, 1);

    graph.add_compute_pass("produce", |s| {
        s.write(texture);
    });
    graph.add_render_pass("inspect", |s| {
        s.read(texture);
        s.write_surface();
    });

    let mut run = |pass: &CompiledPass,
                   _gpu: &mut crate::gpu::GpuContext,
                   resources: &PhysicalResources<'_>| {
        if pass.name == "inspect" {
            let _texture_ref = resources.texture_ref(texture);
            let _default_view = resources.view(texture);
            let _subresource_view =
                resources.texture_subresource_view(subresource, wgpu::TextureViewDimension::D2);
            let _storage_view =
                resources.storage_texture_view(subresource, wgpu::TextureViewDimension::D2);
            let _attachment_view = resources.render_attachment_view(subresource);
        }
        Ok(())
    };

    graph.try_execute(&mut ctx, &mut run).unwrap();
    let stats = graph.physical_resource_view_stats();
    assert_eq!(stats.default_texture_view_resolves, 3);
    assert_eq!(stats.texture_subresource_view_creations, 3);
    assert_eq!(stats.storage_texture_view_creations, 1);
    assert_eq!(stats.render_attachment_view_creations, 1);

    graph.try_execute(&mut ctx, run).unwrap();
    let stats = graph.physical_resource_view_stats();
    assert_eq!(
        stats,
        PhysicalResourceViewStats {
            default_texture_view_resolves: 3,
            texture_subresource_view_creations: 3,
            storage_texture_view_creations: 1,
            render_attachment_view_creations: 1,
        }
    );
}

#[test]
fn buffer_release_returns_to_pool() {
    let (device, queue) = create_test_device();
    let ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let b = graph.create_buffer(|bld| {
        bld.name("transient_buf").size(256);
    });
    graph.add_render_pass("use", |s| {
        s.write_buffer(b);
        s.write_surface();
    });
    graph.compile().unwrap();
    graph.allocate_physical_resources(&ctx);
    assert!(graph.try_physical_buffer(b).is_ok());

    graph.release_transient_resources(&ctx);
    // After release, the slot should be empty.
    assert!(graph.physical_buffers[b.0].is_none());

    graph.destroy_physical_resources();
}
