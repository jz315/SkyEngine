use super::super::*;
use super::common::*;

#[test]
fn copy_pass_establishes_dependency() {
    let mut graph = RenderGraph::new();

    let src = graph.create_texture(|b| {
        b.name("src");
    });
    let dst = graph.create_texture(|b| {
        b.name("dst");
    });

    graph.add_render_pass("produce", |s| {
        s.write(src);
    });
    graph.add_copy_pass("copy", |s| {
        s.texture_to_texture(src, dst);
    });
    graph.add_render_pass("consume", |s| {
        s.read(dst);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 3);

    let pos = |name: &str| passes.iter().position(|p| p.name == name).unwrap();
    assert!(pos("produce") < pos("copy"));
    assert!(pos("copy") < pos("consume"));
    assert_eq!(passes[pos("copy")].copy_ops.len(), 1);
    assert!(matches!(
        passes[pos("copy")].copy_ops[0],
        CopyOp::TextureToTexture { .. }
    ));
}

#[test]
fn copy_pass_multi_ops() {
    let mut graph = RenderGraph::new();

    let t1 = graph.create_texture(|b| {
        b.name("t1");
    });
    let t2 = graph.create_texture(|b| {
        b.name("t2");
    });

    graph.add_render_pass("gen", |s| {
        s.write(t1);
    });
    graph.add_copy_pass("multi_copy", |s| {
        s.texture_to_texture(t1, t2);
    });
    graph.add_render_pass("use_it", |s| {
        s.read(t2);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 3);
    assert_eq!(passes[1].pass_type, PassType::Copy);
    assert_eq!(passes[1].copy_ops.len(), 1);
}

#[test]
fn copy_pass_deduplicates_resource_lists() {
    // A copy pass that copies from the same source twice should not have
    // duplicate entries in its reads list.
    let mut graph = RenderGraph::new();
    let src = graph.create_texture(|b| {
        b.name("src");
    });
    let dst1 = graph.create_texture(|b| {
        b.name("dst1");
    });
    let dst2 = graph.create_texture(|b| {
        b.name("dst2");
    });

    graph.add_render_pass("produce", |s| {
        s.write(src);
    });
    graph.add_copy_pass("multi_copy", |s| {
        s.texture_to_texture(src, dst1);
        s.texture_to_texture(src, dst2); // same src twice
    });
    graph.add_render_pass("consume", |s| {
        s.read(dst1);
        s.read(dst2);
        s.write_surface();
    });

    // The copy pass should only have src once in its reads.
    let copy_pass = &graph.passes[1];
    let src_read_count = copy_pass
        .reads
        .iter()
        .filter(|r| **r == ResourceRef::Texture(src))
        .count();
    assert_eq!(
        src_read_count, 1,
        "CopyPassSetup should deduplicate reads, got {} for src",
        src_read_count
    );

    // Should still compile and work fine.
    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 3);
}

#[test]
fn buffer_usage_for_infers_copy_src_from_copy_pass() {
    let mut graph = RenderGraph::new();
    let src_buf = graph.create_buffer(|b| {
        b.name("src").size(512);
    });
    let dst_buf = graph.create_buffer(|b| {
        b.name("dst").size(512);
    });

    graph.add_render_pass("init", |s| {
        s.write_buffer(src_buf);
    });
    graph.add_copy_pass("copy", |s| {
        s.buffer_to_buffer(src_buf, dst_buf);
    });
    graph.add_render_pass("consume", |s| {
        s.read_buffer(dst_buf);
        s.write_surface();
    });
    graph.compile().unwrap();

    let src_usage = graph.buffer_usage_for(src_buf);
    let dst_usage = graph.buffer_usage_for(dst_buf);
    assert!(src_usage.contains(wgpu::BufferUsages::COPY_SRC));
    assert!(dst_usage.contains(wgpu::BufferUsages::COPY_DST));
}

#[test]
fn texture_copy_infers_required_usage() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let src = graph.create_texture(|b| {
        b.name("src")
            .size(TargetSize::Exact(1, 1))
            .format(TextureFormat::Rgba8Unorm)
            .usage(wgpu::TextureUsages::TEXTURE_BINDING);
    });
    let dst = graph.create_texture(|b| {
        b.name("dst")
            .size(TargetSize::Exact(1, 1))
            .format(TextureFormat::Rgba8Unorm)
            .usage(wgpu::TextureUsages::TEXTURE_BINDING);
    });

    graph.add_copy_pass("upload_src", |s| {
        s.upload_to_texture(vec![255, 0, 0, 255], src, 1, 1, 4);
    });
    graph.add_copy_pass("copy", |s| {
        s.texture_to_texture(src, dst);
    });
    graph.add_render_pass("present", |s| {
        s.read(dst);
        s.write_surface();
    });

    graph
        .try_execute(&mut ctx, |_pass, _gpu, _resources| Ok(()))
        .expect("graph-owned textures should infer COPY_SRC/COPY_DST usage");
}

#[test]
fn upload_to_texture_infers_copy_dst_usage() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let tex = graph.create_texture(|b| {
        b.name("upload_dst")
            .size(TargetSize::Exact(1, 1))
            .format(TextureFormat::Rgba8Unorm)
            .usage(wgpu::TextureUsages::TEXTURE_BINDING);
    });

    graph.add_copy_pass("upload", |s| {
        s.upload_to_texture(vec![0, 255, 0, 255], tex, 1, 1, 4);
    });
    graph.add_render_pass("present", |s| {
        s.read(tex);
        s.write_surface();
    });

    graph
        .try_execute(&mut ctx, |_pass, _gpu, _resources| Ok(()))
        .expect("graph-owned upload targets should infer COPY_DST usage");
}

#[test]
fn copy_pass_buffer_to_texture_establishes_dependency() {
    let mut graph = RenderGraph::new();
    let buf = graph.create_buffer(|b| {
        b.name("upload_buf").size(256);
    });
    let tex = graph.create_texture(|b| {
        b.name("target");
    });

    graph.add_render_pass("fill_buf", |s| {
        s.write_buffer(buf);
    });
    graph.add_copy_pass("upload", |s| {
        s.buffer_to_texture(buf, tex);
    });
    graph.add_render_pass("present", |s| {
        s.read(tex);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 3);
    // Verify ordering: fill_buf < upload < present.
    let names: Vec<_> = passes.iter().map(|p| p.name.as_ref()).collect();
    assert_eq!(names, vec!["fill_buf", "upload", "present"]);
}

#[test]
fn single_row_buffer_to_texture_copy_accepts_tight_layout() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let buf = graph.create_buffer(|b| {
        b.name("single_row").size(4);
    });
    let tex = graph.create_texture(|b| {
        b.name("target")
            .size(TargetSize::Exact(1, 1))
            .format(TextureFormat::Rgba8Unorm)
            .usage(wgpu::TextureUsages::TEXTURE_BINDING);
    });

    graph.add_compute_pass("fill", |s| {
        s.write_buffer(buf);
    });
    graph.add_copy_pass("upload", |s| {
        s.buffer_to_texture(buf, tex);
    });
    graph.add_render_pass("present", |s| {
        s.read(tex);
        s.write_surface();
    });

    graph
        .try_execute(&mut ctx, |_pass, _gpu, _resources| Ok(()))
        .expect("single-row buffer-to-texture copy should allow tight layout");
}

#[test]
fn buffer_to_texture_rows_per_image_padding_does_not_inflate_single_layer_size() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let buf = graph.create_buffer(|b| {
        b.name("sparse_rows").size(264);
    });
    let tex = graph.create_texture(|b| {
        b.name("target")
            .size(TargetSize::Exact(2, 2))
            .format(TextureFormat::Rgba8Unorm)
            .usage(wgpu::TextureUsages::TEXTURE_BINDING);
    });

    graph.add_compute_pass("fill", |s| {
        s.write_buffer(buf);
    });
    graph.add_copy_pass("upload", |s| {
        s.buffer_to_texture_with_layout(buf, tex, Some(256), Some(8));
    });
    graph.add_render_pass("present", |s| {
        s.read(tex);
        s.write_surface();
    });

    graph
        .try_execute(&mut ctx, |_pass, _gpu, _resources| Ok(()))
        .expect("single-layer copy size should not include unused rows_per_image padding");
}

#[test]
fn copy_pass_buffer_to_texture_with_layout_has_extra_params() {
    let mut graph = RenderGraph::new();
    let buf = graph.create_buffer(|b| {
        b.name("data").size(512);
    });
    let tex = graph.create_texture(|b| {
        b.name("img");
    });

    graph.add_render_pass("fill", |s| {
        s.write_buffer(buf);
    });
    graph.add_copy_pass("upload_layout", |s| {
        s.buffer_to_texture_with_layout(buf, tex, Some(256), Some(4));
    });
    graph.add_render_pass("present", |s| {
        s.read(tex);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 3);

    // Verify the copy op has the layout params.
    let copy_pass = &passes[1];
    assert_eq!(copy_pass.copy_ops.len(), 1);
    match &copy_pass.copy_ops[0] {
        CopyOp::BufferToTexture {
            bytes_per_row,
            rows_per_image,
            ..
        } => {
            assert_eq!(*bytes_per_row, Some(256));
            assert_eq!(*rows_per_image, Some(4));
        }
        other => panic!("expected BufferToTexture, got {:?}", other),
    }
}

#[test]
fn texture_to_texture_rejects_msaa_copy_before_allocation() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let src = graph.create_texture(|b| {
        b.name("src_msaa")
            .size(TargetSize::Exact(4, 4))
            .format(TextureFormat::Rgba8Unorm)
            .sample_count(4);
    });
    let dst = graph.create_texture(|b| {
        b.name("dst")
            .size(TargetSize::Exact(4, 4))
            .format(TextureFormat::Rgba8Unorm);
    });

    graph.add_render_pass("produce", |s| {
        s.write(src);
    });
    graph.add_copy_pass("copy", |s| {
        s.texture_to_texture(src, dst);
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
        RenderGraphError::InvalidTextureCopy { src: s, dst: d, .. } if s == src && d == dst
    ));
}

#[test]
fn copy_pass_texture_to_texture_rejects_mismatched_extent() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let src = graph.create_texture(|b| {
        b.name("src")
            .size(TargetSize::Exact(8, 8))
            .format(TextureFormat::Rgba8Unorm);
    });
    let dst = graph.create_texture(|b| {
        b.name("dst")
            .size(TargetSize::Exact(4, 4))
            .format(TextureFormat::Rgba8Unorm);
    });

    graph.add_render_pass("produce", |s| {
        s.write(src);
    });
    graph.add_copy_pass("copy", |s| {
        s.texture_to_texture(src, dst);
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
        RenderGraphError::InvalidTextureCopy { src: s, dst: d, .. } if s == src && d == dst
    ));
}

#[test]
fn copy_pass_buffer_to_buffer_rejects_mismatched_sizes() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [64, 64],
    );

    let mut graph = RenderGraph::new();
    let src = graph.create_buffer(|b| {
        b.name("src").size(64);
    });
    let dst = graph.create_buffer(|b| {
        b.name("dst").size(128);
    });

    graph.add_render_pass("fill", |s| {
        s.write_buffer(src);
    });
    graph.add_copy_pass("copy", |s| {
        s.buffer_to_buffer(src, dst);
    });
    graph.add_render_pass("present", |s| {
        s.read_buffer(dst);
        s.write_surface();
    });

    let err = graph
        .try_execute(&mut ctx, |_pass, _gpu, _resources| Ok(()))
        .unwrap_err();
    assert!(matches!(
        err,
        RenderGraphError::InvalidBufferCopy { src: s, dst: d, .. } if s == src && d == dst
    ));
}

#[test]
fn copy_pass_upload_to_texture_creates_write_only() {
    let mut graph = RenderGraph::new();
    let tex = graph.create_texture(|b| {
        b.name("dst");
    });

    // UploadToTexture only writes, no read dependency.
    graph.add_copy_pass("upload", |s| {
        s.upload_to_texture(vec![0u8; 64], tex, 4, 4, 4);
    });
    graph.add_render_pass("present", |s| {
        s.read(tex);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 2);

    let upload_pass = &passes[0];
    assert_eq!(upload_pass.copy_ops.len(), 1);
    match &upload_pass.copy_ops[0] {
        CopyOp::UploadToTexture {
            data,
            width,
            height,
            bytes_per_pixel,
            ..
        } => {
            assert_eq!(data.len(), 64);
            assert_eq!(*width, 4);
            assert_eq!(*height, 4);
            assert_eq!(*bytes_per_pixel, 4);
        }
        other => panic!("expected UploadToTexture, got {:?}", other),
    }
}

#[test]
fn copy_pass_with_flags_are_preserved() {
    let mut graph = RenderGraph::new();
    let t1 = graph.create_texture(|b| {
        b.name("s");
    });
    let t2 = graph.create_texture(|b| {
        b.name("d");
    });

    graph.add_render_pass("prod", |s| {
        s.write(t1);
    });
    graph.add_copy_pass("flagged_copy", |s| {
        s.texture_to_texture(t1, t2);
        s.with_flags(PassFlags::PREFER_ASYNC_COMPUTE);
    });
    graph.add_render_pass("present", |s| {
        s.read(t2);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    let copy = passes
        .iter()
        .find(|p| p.name.as_ref() == "flagged_copy")
        .unwrap();
    assert!(copy.flags.contains(PassFlags::PREFER_ASYNC_COMPUTE));
}
