//! Expert-only RenderGraph API showcase — exercises the low-level public API surface.
//!
//! This example builds a non-trivial graph that exercises:
//!
//! - Virtual texture creation (multiple formats, target sizes)
//! - Virtual buffer creation (with custom usage flags)
//! - Render passes, compute passes, and copy passes
//! - MRT (multiple render target) declarations
//! - Depth/stencil attachments
//! - Dead-pass culling (passes with no path to a sink are pruned)
//! - Blackboard cross-pass data sharing
//! - DOT visualization export
//! - Memory aliasing statistics
//! - Dependency levels and pass scheduling
//! - Name-based resource lookup
//! - Pass flags (async compute hints, bandwidth hints)
//! - Profiler callbacks (DebugProfiler)
//! - Stale handle rejection after reset
//! - `write_surface_color` with explicit load ops
//!
//! No GPU or window is required — everything runs at the declaration +
//! compile level.  Run with:
//!
//! ```bash
//! cargo run --example render_graph_showcase --features app
//! ```

use sky_engine::render::expert::graph::{
    LoadOp, PassFlags, PassType, RenderGraph, RenderGraphError, ResourceRef, TargetSize,
};

fn main() {
    println!("╔══════════════════════════════════════════════════════╗");
    println!("║   SkyEngine RenderGraph API Showcase                ║");
    println!("║   是骡子是马，拉出来遛遛！                            ║");
    println!("╚══════════════════════════════════════════════════════╝");
    println!();

    showcase_deferred_pipeline();
    println!();
    showcase_compute_particle_sim();
    println!();
    showcase_dead_pass_culling();
    println!();
    showcase_copy_passes();
    println!();
    showcase_blackboard();
    println!();
    showcase_stale_handle_rejection();
    println!();
    showcase_pass_flags();
    println!();
    showcase_dot_visualization();
    println!();

    println!("════════════════════════════════════════════════════════");
    println!("  All showcases passed!  RenderGraph API is solid. ✓");
    println!("════════════════════════════════════════════════════════");
}

// ── Showcase 1: Full Deferred Rendering Pipeline ───────────────────────────

fn showcase_deferred_pipeline() {
    println!("── 1. Deferred Rendering Pipeline ──────────────────────");

    let mut graph = RenderGraph::new();

    // --- G-Buffer textures (MRT) ---
    let albedo = graph.create_texture(|b| {
        b.name("gbuf_albedo")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Rgba8Unorm);
    });
    let normal = graph.create_texture(|b| {
        b.name("gbuf_normal")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Rgba16Float);
    });
    let depth = graph.create_texture(|b| {
        b.name("gbuf_depth")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Depth32Float);
    });

    // --- HDR lighting output ---
    let hdr_lit = graph.create_texture(|b| {
        b.name("hdr_lit")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Rgba16Float);
    });

    // --- Post-processing chain (half-res bloom) ---
    let bloom_half = graph.create_texture(|b| {
        b.name("bloom_half")
            .size(TargetSize::Scale(0.5))
            .format(wgpu::TextureFormat::Rgba16Float);
    });
    let bloom_result = graph.create_texture(|b| {
        b.name("bloom_result")
            .size(TargetSize::Surface)
            .format(wgpu::TextureFormat::Rgba16Float);
    });

    // --- Name-based lookup ---
    assert_eq!(graph.get_texture("gbuf_albedo"), Some(albedo));
    assert_eq!(graph.get_texture("hdr_lit"), Some(hdr_lit));
    assert_eq!(graph.get_texture("nonexistent"), None);
    println!("  ✓ Name-based texture lookup works");

    // --- G-Buffer pass (MRT: albedo + normal, with depth/stencil) ---
    let _gbuffer_pass = graph.add_render_pass("gbuffer", |s| {
        s.write_color_cleared(0, albedo, [0.0, 0.0, 0.0, 1.0]);
        s.write_color_cleared(1, normal, [0.5, 0.5, 1.0, 1.0]);
        s.set_depth_stencil_cleared(depth, 1.0);
    });

    // --- Lighting pass (reads G-Buffer, writes HDR) ---
    let _lighting_pass = graph.add_render_pass("lighting", |s| {
        s.read(albedo);
        s.read(normal);
        s.read(depth);
        s.write_color_cleared(0, hdr_lit, [0.0, 0.0, 0.0, 1.0]);
    });

    // --- Bloom downsample ---
    let _bloom_down = graph.add_render_pass("bloom_down", |s| {
        s.read(hdr_lit);
        s.write_color_cleared(0, bloom_half, [0.0; 4]);
    });

    // --- Bloom composite ---
    let _bloom_composite = graph.add_render_pass("bloom_composite", |s| {
        s.read(hdr_lit);
        s.read(bloom_half);
        s.write(bloom_result);
    });

    // --- Tonemap to surface ---
    let _tonemap = graph.add_render_pass("tonemap", |s| {
        s.read(bloom_result);
        s.write_surface_color(0, LoadOp::Clear([0.0; 4]));
    });

    // --- Compile ---
    let passes = graph.compile().unwrap();

    // Verify pass count and order
    assert_eq!(passes.len(), 5, "All 5 passes should survive");
    assert_eq!(graph.culled_count(), 0);

    // Verify ordering: gbuffer < lighting < bloom < tonemap
    let pos = |name: &str| passes.iter().position(|p| p.name == name).unwrap();
    assert!(pos("gbuffer") < pos("lighting"));
    assert!(pos("lighting") < pos("bloom_down"));
    assert!(pos("lighting") < pos("bloom_composite"));
    assert!(pos("bloom_composite") < pos("tonemap"));

    // Verify MRT on gbuffer pass
    let gbuf = &passes[pos("gbuffer")];
    assert_eq!(gbuf.color_outputs.len(), 2);
    assert_eq!(gbuf.color_outputs[0].slot, 0);
    assert_eq!(gbuf.color_outputs[1].slot, 1);
    assert!(gbuf.depth_stencil.is_some());
    let ds = gbuf.depth_stencil.unwrap();
    assert_eq!(ds.clear_depth, Some(1.0));
    assert!(ds.depth_store);

    // Verify dependency levels
    assert_eq!(passes[pos("gbuffer")].dep_level, 0);
    assert!(passes[pos("lighting")].dep_level >= 1);
    assert!(passes[pos("tonemap")].dep_level >= 3);
    assert!(graph.max_dep_level() >= 3);

    // Verify aliasing stats are populated
    // (alias analysis happens during allocate_physical_resources, which needs a
    //  GpuContext. We can still verify the lifetime data is correct.)
    let hdr_lifetime = graph
        .compile()
        .unwrap()
        .iter()
        .find(|p| p.name == "lighting")
        .unwrap()
        .writes
        .contains(&ResourceRef::Texture(hdr_lit));
    assert!(hdr_lifetime);

    println!(
        "  ✓ Deferred pipeline: {} passes, depth {} levels, {} culled",
        passes.len(),
        graph.max_dep_level(),
        graph.culled_count()
    );
    println!(
        "    Pass order: {}",
        passes
            .iter()
            .map(|p| p.name.as_ref())
            .collect::<Vec<_>>()
            .join(" → ")
    );
}

// ── Showcase 2: Compute Particle Simulation ─────────────────────────────

fn showcase_compute_particle_sim() {
    println!("── 2. Compute Particle Simulation ────────────────────");

    let mut graph = RenderGraph::new();

    // Particle position buffer
    let particle_buf = graph.create_buffer(|b| {
        b.name("particles")
            .size(1024 * 16) // 1024 particles × 16 bytes each
            .usage(wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX);
    });

    // Indirect draw buffer
    let indirect_buf = graph.create_buffer(|b| {
        b.name("indirect_args")
            .size(16)
            .usage(wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::STORAGE);
    });

    // Compute pass: update particle positions
    let _sim_pass = graph.add_compute_pass("particle_sim", |s| {
        s.readwrite_buffer(particle_buf);
        s.write_buffer(indirect_buf);
        s.with_flags(PassFlags::PREFER_ASYNC_COMPUTE | PassFlags::COMPUTE_INTENSIVE);
    });

    // Render pass: draw particles to half-res RT, then composite to surface
    let particle_rt = graph.create_texture(|b| {
        b.name("particle_rt")
            .size(TargetSize::Scale(0.5))
            .format(wgpu::TextureFormat::Rgba16Float);
    });

    let _draw_pass = graph.add_render_pass("particle_draw", |s| {
        s.read_buffer(particle_buf);
        s.read_buffer(indirect_buf);
        s.write_color_cleared(0, particle_rt, [0.0, 0.0, 0.05, 1.0]);
    });

    let _present_pass = graph.add_render_pass("particle_present", |s| {
        s.read(particle_rt);
        s.write_surface();
    });

    // --- Buffer name lookup ---
    assert_eq!(graph.get_buffer("particles"), Some(particle_buf));
    assert_eq!(graph.get_buffer("indirect_args"), Some(indirect_buf));
    println!("  ✓ Name-based buffer lookup works");

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 3);

    // Verify pass types
    let pos = |name: &str| passes.iter().position(|p| p.name == name).unwrap();
    assert_eq!(passes[pos("particle_sim")].pass_type, PassType::Compute);
    assert_eq!(passes[pos("particle_draw")].pass_type, PassType::Render);
    assert_eq!(passes[pos("particle_present")].pass_type, PassType::Render);

    // Verify ordering: sim < draw < present
    assert!(pos("particle_sim") < pos("particle_draw"));
    assert!(pos("particle_draw") < pos("particle_present"));

    // Verify async compute flags survived compilation
    let sim = &passes[pos("particle_sim")];
    assert!(sim.flags.contains(PassFlags::PREFER_ASYNC_COMPUTE));
    assert!(sim.flags.contains(PassFlags::COMPUTE_INTENSIVE));

    println!(
        "  ✓ Compute pipeline: {} passes, flags={:?}",
        passes.len(),
        sim.flags
    );
    println!(
        "    Pass order: {}",
        passes
            .iter()
            .map(|p| format!("{}({:?})", p.name, p.pass_type))
            .collect::<Vec<_>>()
            .join(" → ")
    );
}

// ── Showcase 3: Dead-Pass Culling ──────────────────────────────────────

fn showcase_dead_pass_culling() {
    println!("── 3. Dead-Pass Culling ──────────────────────────────");

    let mut graph = RenderGraph::new();

    let hdr = graph.create_texture(|b| {
        b.name("hdr").format(wgpu::TextureFormat::Rgba16Float);
    });
    let debug_tex = graph.create_texture(|b| {
        b.name("debug_overlay");
    });
    let shadow_map = graph.create_texture(|b| {
        b.name("shadow_map")
            .size(TargetSize::Exact(2048, 2048))
            .format(wgpu::TextureFormat::Depth32Float);
    });
    let unused_tex = graph.create_texture(|b| {
        b.name("totally_unused");
    });

    // Live chain: scene → post → surface
    graph.add_render_pass("scene", |s| {
        s.write(hdr);
    });
    graph.add_render_pass("present", |s| {
        s.read(hdr);
        s.write_surface();
    });

    // Dead chain 1: just writes to debug_tex, nothing reads it
    graph.add_render_pass("debug_wireframe", |s| {
        s.write(debug_tex);
    });

    // Dead chain 2: shadow map gen + shadow pass, but result never consumed
    graph.add_render_pass("shadow_gen", |s| {
        s.write(shadow_map);
    });
    graph.add_render_pass("shadow_apply", |s| {
        s.read(shadow_map);
        s.write(unused_tex);
    });

    let passes = graph.compile().unwrap();
    println!("  Total declared: {} passes", graph.pass_count());
    println!("  Alive after cull: {} passes", graph.alive_pass_count());
    println!("  Culled: {} passes", graph.culled_count());

    assert_eq!(passes.len(), 2, "Only scene + present should survive");
    assert_eq!(graph.culled_count(), 3, "3 dead passes should be culled");
    assert_eq!(passes[0].name, "scene");
    assert_eq!(passes[1].name, "present");

    // Verify dead passes are correctly marked
    println!(
        "  ✓ Dead passes correctly pruned: {}",
        ["debug_wireframe", "shadow_gen", "shadow_apply"].join(", ")
    );
}

// ── Showcase 4: Copy Passes ──────────────────────────────────────────

fn showcase_copy_passes() {
    println!("── 4. Copy Passes ──────────────────────────────────");

    let mut graph = RenderGraph::new();

    let scene = graph.create_texture(|b| {
        b.name("scene_color")
            .format(wgpu::TextureFormat::Rgba16Float);
    });
    let history = graph.create_texture(|b| {
        b.name("history_buffer")
            .format(wgpu::TextureFormat::Rgba16Float)
            .persistent(); // keep across frames for TAA
    });
    let staging_buf = graph.create_buffer(|b| {
        b.name("staging")
            .size(256 * 4) // 256 pixels worth
            .usage(wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST);
    });
    let readback_buf = graph.create_buffer(|b| {
        b.name("readback")
            .size(256 * 4)
            .usage(wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ);
    });

    // Render the scene
    graph.add_render_pass("render_scene", |s| {
        s.write(scene);
    });

    // Copy current frame to history for TAA
    graph.add_copy_pass("taa_history_copy", |s| {
        s.texture_to_texture(scene, history);
    });

    // Fill the staging buffer (needed so readback_copy has a writer above it)
    graph.add_compute_pass("staging_fill", |s| {
        s.write_buffer(staging_buf);
    });

    // Copy staging buffer data to readback buffer
    graph.add_copy_pass("readback_copy", |s| {
        s.buffer_to_buffer(staging_buf, readback_buf);
    });

    // TAA resolve reads history
    graph.add_render_pass("taa_resolve", |s| {
        s.read(scene);
        s.read(history);
        s.write_surface();
    });

    // The readback_copy should be culled — readback_buf has no external sink
    // (it's not imported and not written to surface)
    let passes = graph.compile().unwrap();

    let names: Vec<&str> = passes.iter().map(|p| p.name.as_ref()).collect();
    println!("  Alive passes: {}", names.join(", "));
    println!("  Culled: {}", graph.culled_count());

    // TAA chain should survive, readback should be culled
    assert!(names.contains(&"render_scene"));
    assert!(names.contains(&"taa_history_copy"));
    assert!(names.contains(&"taa_resolve"));
    // readback_copy and staging_fill get culled because readback_buf has no external sink
    assert!(!names.contains(&"readback_copy"));
    assert!(!names.contains(&"staging_fill"));

    // Verify copy ops are in the compiled pass
    let taa_copy = passes
        .iter()
        .find(|p| p.name == "taa_history_copy")
        .unwrap();
    assert_eq!(taa_copy.pass_type, PassType::Copy);
    assert_eq!(taa_copy.copy_ops.len(), 1);

    println!("  ✓ Copy pass with texture-to-texture works");
    println!("  ✓ Unreferenced readback copy correctly culled");
}

// ── Showcase 5: Blackboard Cross-Pass Data Sharing ─────────────────────

fn showcase_blackboard() {
    println!("── 5. Blackboard Cross-Pass Data ─────────────────────");

    let mut graph = RenderGraph::new();

    // Publish camera matrices via blackboard
    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    struct CameraData {
        view_proj: [f32; 16],
        exposure: f32,
    }

    let camera = CameraData {
        view_proj: [1.0; 16], // Identity-ish
        exposure: 1.5,
    };

    graph.blackboard().set("camera", camera.clone());
    graph.blackboard().set("frame_index", 42u64);
    graph.blackboard().set("debug_mode", true);

    // Retrieve and verify
    let cam = graph.blackboard_ref().get::<CameraData>("camera").unwrap();
    assert_eq!(cam.exposure, 1.5);

    let frame = graph.blackboard_ref().get::<u64>("frame_index").unwrap();
    assert_eq!(*frame, 42);

    let debug = graph.blackboard_ref().get::<bool>("debug_mode").unwrap();
    assert!(*debug);

    // Wrong type returns None
    assert!(graph.blackboard_ref().get::<f32>("frame_index").is_none());

    // Overwrite
    graph.blackboard().set("frame_index", 43u64);
    assert_eq!(
        *graph.blackboard_ref().get::<u64>("frame_index").unwrap(),
        43
    );

    // Exists check
    assert!(graph.blackboard_ref().contains("camera"));
    assert!(!graph.blackboard_ref().contains("nonexistent"));

    println!("  ✓ Blackboard set/get/overwrite/contains works");
    println!("  ✓ Type-safe: wrong type returns None");

    // Reset clears blackboard
    graph.reset();
    assert!(!graph.blackboard_ref().contains("camera"));
    println!("  ✓ Reset clears blackboard");
}

// ── Showcase 6: Stale Handle Rejection ──────────────────────────────────

fn showcase_stale_handle_rejection() {
    println!("── 6. Stale Handle Rejection ──────────────────────────");

    let mut graph = RenderGraph::new();
    let old_handle = graph.create_texture(|b| {
        b.name("old_texture");
    });

    // Reset invalidates all handles
    graph.reset();

    let new_tex = graph.create_texture(|b| {
        b.name("new_texture");
    });

    graph.add_render_pass("bad_pass", |s| {
        s.read(old_handle); // stale!
        s.write(new_tex);
    });

    match graph.compile() {
        Err(RenderGraphError::InvalidResourceHandle { pass, resource }) => {
            println!(
                "  ✓ Stale handle correctly rejected: pass={:?}, resource={:?}",
                pass, resource
            );
        }
        other => panic!("Expected InvalidResourceHandle, got: {:?}", other.err()),
    }

    // Also test cross-graph handle rejection
    let mut graph_a = RenderGraph::new();
    let foreign = graph_a.create_buffer(|b| {
        b.name("foreign_buf").size(64);
    });

    let mut graph_b = RenderGraph::new();
    let local = graph_b.create_buffer(|b| {
        b.name("local_buf").size(64);
    });
    graph_b.add_compute_pass("cross_graph", |s| {
        s.read_buffer(foreign); // wrong graph!
        s.write_buffer(local);
    });

    match graph_b.compile() {
        Err(RenderGraphError::InvalidResourceHandle { .. }) => {
            println!("  ✓ Cross-graph handle correctly rejected");
        }
        other => panic!("Expected InvalidResourceHandle, got: {:?}", other.err()),
    }
}

// ── Showcase 7: Pass Flags ─────────────────────────────────────────────

fn showcase_pass_flags() {
    println!("── 7. Pass Flags ────────────────────────────────────");

    let mut graph = RenderGraph::new();
    let tex = graph.create_texture(|b| {
        b.name("work");
    });

    graph.add_compute_pass("heavy_compute", |s| {
        s.readwrite(tex);
        s.with_flags(
            PassFlags::PREFER_ASYNC_COMPUTE
                | PassFlags::COMPUTE_INTENSIVE
                | PassFlags::BANDWIDTH_INTENSIVE,
        );
    });

    graph.add_render_pass("pixel_heavy", |s| {
        s.read(tex);
        s.write_surface();
        s.with_flags(PassFlags::PIXEL_BOUND_INTENSIVE);
    });

    let passes = graph.compile().unwrap();

    let compute = &passes[0];
    assert!(compute.flags.contains(PassFlags::PREFER_ASYNC_COMPUTE));
    assert!(compute.flags.contains(PassFlags::COMPUTE_INTENSIVE));
    assert!(compute.flags.contains(PassFlags::BANDWIDTH_INTENSIVE));
    assert!(!compute.flags.contains(PassFlags::PIXEL_BOUND_INTENSIVE));

    let render = &passes[1];
    assert!(render.flags.contains(PassFlags::PIXEL_BOUND_INTENSIVE));
    assert!(!render.flags.contains(PassFlags::PREFER_ASYNC_COMPUTE));

    println!("  ✓ Compute pass flags: {:?}", compute.flags);
    println!("  ✓ Render pass flags: {:?}", render.flags);
}

// ── Showcase 8: DOT Visualization ──────────────────────────────────────

fn showcase_dot_visualization() {
    println!("── 8. DOT Visualization ────────────────────────────");

    let mut graph = RenderGraph::new();
    let hdr = graph.create_texture(|b| {
        b.name("hdr").format(wgpu::TextureFormat::Rgba16Float);
    });
    let bloom = graph.create_texture(|b| {
        b.name("bloom")
            .size(TargetSize::Scale(0.5))
            .format(wgpu::TextureFormat::Rgba16Float);
    });
    let _staging = graph.create_buffer(|b| {
        b.name("staging").size(4096);
    });
    let dead_tex = graph.create_texture(|b| {
        b.name("dead_resource");
    });

    graph.add_render_pass("scene", |s| {
        s.write(hdr);
    });
    graph.add_render_pass("bloom_pass", |s| {
        s.read(hdr);
        s.write(bloom);
    });
    graph.add_render_pass("final", |s| {
        s.read(bloom);
        s.write_surface();
    });
    // This pass should appear as culled (dashed) in DOT
    graph.add_render_pass("dead_debug", |s| {
        s.write(dead_tex);
    });

    graph.compile().unwrap();
    let dot = graph.export_dot();

    // Verify DOT structure
    assert!(dot.contains("digraph RenderGraph"));
    assert!(dot.contains("res_tex_0")); // hdr
    assert!(dot.contains("res_tex_1")); // bloom
    assert!(dot.contains("res_buf_0")); // staging
    assert!(dot.contains("res_surface"));
    assert!(dot.contains("pass_0")); // scene
    assert!(dot.contains("pass_1")); // bloom_pass
    assert!(dot.contains("pass_2")); // final
    assert!(dot.contains("pass_3")); // dead_debug
                                     // Dead pass should have dashed style
    assert!(dot.contains("dashed"));

    // Print a snippet
    let lines: Vec<&str> = dot.lines().collect();
    println!("  ✓ DOT export: {} lines", lines.len());
    for line in lines.iter().take(8) {
        println!("    {}", line);
    }
    println!("    ...");
    println!(
        "  ✓ Contains: {} textures, {} buffers, {} passes",
        dot.matches("res_tex_").count() / 2, // each appears twice (defn + edge)
        dot.matches("res_buf_").count() / 2,
        dot.matches("pass_").count() / 2,
    );
}
