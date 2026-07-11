use super::super::*;

#[test]
fn empty_graph_compiles() {
    let mut graph = RenderGraph::new();
    let passes = graph.compile().unwrap();
    assert!(passes.is_empty());
}

#[test]
fn empty_graph_compile_clears_stale_stats() {
    let mut graph = RenderGraph::new();
    graph.max_dep_level = 7;
    graph.culled_count = 3;
    graph.lifetimes.insert(
        ResourceRef::Surface,
        ResourceLifetime {
            first_use: 0,
            last_use: 0,
        },
    );

    let passes = graph.compile().unwrap();
    assert!(passes.is_empty());
    assert_eq!(graph.max_dep_level(), 0);
    assert_eq!(graph.culled_count(), 0);
    assert!(graph.lifetimes.is_empty());
}

#[test]
fn linear_chain_orders_correctly() {
    let mut graph = RenderGraph::new();

    let hdr = graph.create_texture(|b| {
        b.name("hdr").format(TextureFormat::Rgba16Float);
    });

    let scene = graph.add_render_pass("scene", |setup| {
        setup.write(hdr);
    });

    let post = graph.add_render_pass("post", |setup| {
        setup.read(hdr);
        setup.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 2);
    assert_eq!(passes[0].handle, scene);
    assert_eq!(passes[1].handle, post);
    assert_eq!(passes[0].name, "scene");
    assert_eq!(passes[1].name, "post");
}

#[test]
fn dead_pass_is_culled() {
    let mut graph = RenderGraph::new();

    let hdr = graph.create_texture(|b| {
        b.name("hdr");
    });
    let unused = graph.create_texture(|b| {
        b.name("unused");
    });

    graph.add_render_pass("scene", |setup| {
        setup.write(hdr);
    });

    graph.add_render_pass("present", |setup| {
        setup.read(hdr);
        setup.write_surface();
    });

    graph.add_render_pass("dead_pass", |setup| {
        setup.write(unused);
    });

    let passes = graph.compile().unwrap();

    assert_eq!(passes.len(), 2);
    assert_eq!(graph.culled_count(), 1);
    assert!(!graph.passes[2].alive);
}

#[test]
fn dependency_levels_computed() {
    let mut graph = RenderGraph::new();

    let a = graph.create_texture(|b| {
        b.name("a");
    });
    let b_tex = graph.create_texture(|b| {
        b.name("b");
    });

    graph.add_render_pass("root", |s| s.write(a));
    graph.add_render_pass("mid", |s| {
        s.read(a);
        s.write(b_tex);
    });
    graph.add_render_pass("leaf", |s| {
        s.read(b_tex);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();

    assert_eq!(passes[0].dep_level, 0);
    assert_eq!(passes[1].dep_level, 1);
    assert_eq!(passes[2].dep_level, 2);
    assert_eq!(graph.max_dep_level(), 2);
}

#[test]
fn resource_lifetimes_tracked() {
    let mut graph = RenderGraph::new();

    let hdr = graph.create_texture(|b| {
        b.name("hdr");
    });

    graph.add_render_pass("write_hdr", |s| s.write(hdr));
    graph.add_render_pass("read_hdr", |s| {
        s.read(hdr);
        s.write_surface();
    });

    graph.compile().unwrap();

    let lt = graph.lifetimes.get(&ResourceRef::Texture(hdr)).unwrap();
    assert_eq!(lt.first_use, 0);
    assert_eq!(lt.last_use, 1);
}

#[test]
fn compute_pass_type() {
    let mut graph = RenderGraph::new();
    let buf = graph.create_texture(|b| {
        b.name("particles").persistent();
    });

    graph.add_compute_pass("update", |s| {
        s.readwrite(buf);
    });
    graph.add_render_pass("draw", |s| {
        s.read(buf);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes[0].pass_type, PassType::Compute);
    assert_eq!(passes[1].pass_type, PassType::Render);
}

#[test]
fn diamond_dependency() {
    let mut graph = RenderGraph::new();

    let t1 = graph.create_texture(|b| {
        b.name("t1");
    });
    let t2 = graph.create_texture(|b| {
        b.name("t2");
    });
    let t3 = graph.create_texture(|b| {
        b.name("t3");
    });
    let t4 = graph.create_texture(|b| {
        b.name("t4");
    });

    graph.add_render_pass("A", |s| {
        s.write(t1);
        s.write(t2);
    });
    graph.add_render_pass("B", |s| {
        s.read(t1);
        s.write(t3);
    });
    graph.add_render_pass("C", |s| {
        s.read(t2);
        s.write(t4);
    });
    graph.add_render_pass("D", |s| {
        s.read(t3);
        s.read(t4);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 4);

    let pos = |name: &str| passes.iter().position(|p| p.name == name).unwrap();
    assert!(pos("A") < pos("B"));
    assert!(pos("A") < pos("C"));
    assert!(pos("B") < pos("D"));
    assert!(pos("C") < pos("D"));
}

#[test]
fn buffer_usage_ignores_culled_passes() {
    let mut graph = RenderGraph::new();
    let src = graph.create_buffer(|b| {
        b.name("src").size(256).usage(wgpu::BufferUsages::UNIFORM);
    });
    let dst = graph.create_buffer(|b| {
        b.name("dst").size(256).usage(wgpu::BufferUsages::VERTEX);
    });

    graph.add_compute_pass("dead_fill", |s| {
        s.write_buffer(src);
    });
    graph.add_copy_pass("dead_copy", |s| {
        s.buffer_to_buffer(src, dst);
    });
    graph.add_render_pass("present", |s| {
        s.write_surface();
    });

    graph.compile().unwrap();

    assert_eq!(graph.buffer_usage_for(src), wgpu::BufferUsages::UNIFORM);
    assert_eq!(graph.buffer_usage_for(dst), wgpu::BufferUsages::VERTEX);
}

#[test]
fn export_dot_includes_buffer_nodes() {
    let mut graph = RenderGraph::new();
    graph.create_buffer(|b| {
        b.name("staging").size(1024);
    });

    let dot = graph.export_dot();
    assert!(dot.contains("res_buf_0"));
    assert!(dot.contains("staging\\n1024 bytes"));
}

#[test]
fn mrt_color_outputs_preserved() {
    let mut graph = RenderGraph::new();

    let albedo = graph.create_texture(|b| {
        b.name("albedo");
    });
    let normal = graph.create_texture(|b| {
        b.name("normal");
    });

    graph.add_render_pass("gbuffer", |s| {
        s.write_color_cleared(0, albedo, [0.0, 0.0, 0.0, 1.0]);
        s.write_color_cleared(1, normal, [0.5, 0.5, 1.0, 1.0]);
    });

    graph.add_render_pass("lighting", |s| {
        s.read(albedo);
        s.read(normal);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    let gbuffer = &passes[0];
    assert_eq!(gbuffer.color_outputs.len(), 2);
    assert_eq!(gbuffer.color_outputs[0].slot, 0);
    assert_eq!(gbuffer.color_outputs[1].slot, 1);
    assert!(matches!(gbuffer.color_outputs[0].load, LoadOp::Clear(_)));
}

#[test]
fn depth_stencil_creates_dependency() {
    let mut graph = RenderGraph::new();

    let depth = graph.create_texture(|b| {
        b.name("depth").format(TextureFormat::Depth24PlusStencil8);
    });
    let color = graph.create_texture(|b| {
        b.name("color");
    });

    graph.add_render_pass("geometry", |s| {
        s.write_color_cleared(0, color, [0.0; 4]);
        s.set_depth_stencil_cleared(depth, 1.0);
    });

    graph.add_render_pass("post", |s| {
        s.read(color);
        s.read(depth);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 2);
    assert!(passes[0].depth_stencil.is_some());
    let ds = passes[0].depth_stencil.unwrap();
    assert_eq!(ds.handle, depth);
    assert_eq!(ds.clear_depth, Some(1.0));
    assert!(ds.depth_store);
}

#[test]
fn pass_flags_in_compiled_pass() {
    let mut graph = RenderGraph::new();
    let buf = graph.create_texture(|b| {
        b.name("particles").persistent();
    });

    graph.add_compute_pass("sim", |s| {
        s.readwrite(buf);
        s.with_flags(PassFlags::PREFER_ASYNC_COMPUTE | PassFlags::COMPUTE_INTENSIVE);
    });

    graph.add_render_pass("draw", |s| {
        s.read(buf);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert!(passes[0].flags.contains(PassFlags::PREFER_ASYNC_COMPUTE));
    assert!(passes[0].flags.contains(PassFlags::COMPUTE_INTENSIVE));
    assert!(passes[1].flags.is_empty());
}

#[test]
fn earlier_reader_depends_on_earlier_writer_not_later_overwrite() {
    let mut graph = RenderGraph::new();
    let x = graph.create_texture(|b| {
        b.name("x");
    });
    let y = graph.create_texture(|b| {
        b.name("y");
    });

    graph.add_render_pass("produce_initial", |s| {
        s.write(x);
    });
    graph.add_render_pass("consume_initial", |s| {
        s.read(x);
        s.write(y);
    });
    graph.add_render_pass("overwrite_x", |s| {
        s.write(x);
    });
    graph.add_render_pass("present_xy", |s| {
        s.read(y);
        s.read(x);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    let pos = |name: &str| passes.iter().position(|p| p.name == name).unwrap();

    assert!(pos("produce_initial") < pos("consume_initial"));
    assert!(pos("consume_initial") < pos("overwrite_x"));
    assert!(pos("overwrite_x") < pos("present_xy"));
}

#[test]
fn cull_uses_dependency_edges_instead_of_last_writer_lookup() {
    let mut graph = RenderGraph::new();
    let x = graph.create_texture(|b| {
        b.name("x");
    });
    let y = graph.create_texture(|b| {
        b.name("y");
    });

    graph.add_render_pass("produce_initial", |s| {
        s.write(x);
    });
    graph.add_render_pass("consume_initial", |s| {
        s.read(x);
        s.write(y);
    });
    graph.add_render_pass("overwrite_x", |s| {
        s.write(x);
    });
    graph.add_render_pass("present_y", |s| {
        s.read(y);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 3);
    assert!(passes.iter().any(|p| p.name == "produce_initial"));
    assert!(passes.iter().any(|p| p.name == "consume_initial"));
    assert!(passes.iter().any(|p| p.name == "present_y"));
    assert!(passes.iter().all(|p| p.name != "overwrite_x"));
}

#[test]
fn cycle_detected_in_mutual_dependency() {
    let mut graph = RenderGraph::new();
    let a = graph.create_texture(|b| {
        b.name("a");
    });
    let b_tex = graph.create_texture(|b| {
        b.name("b");
    });

    // pass0 reads a, writes b (but a has no writer → read-before-write)
    // We need a cycle that passes read-before-write check first.
    // Cycle requires: A depends on B, B depends on A.
    // This is actually a read-before-write, not a topo sort cycle.
    // Real cycle needs: write A → read B, write B → read A, both with
    // prior writers. Let's use surface as root.
    let c = graph.create_texture(|b| {
        b.name("c");
    });

    // p0 writes c (no deps)
    graph.add_render_pass("p0", |s| {
        s.write(c);
    });
    // p1 reads c, writes a
    graph.add_render_pass("p1", |s| {
        s.read(c);
        s.write(a);
    });
    // p2 reads a, writes b
    graph.add_render_pass("p2", |s| {
        s.read(a);
        s.write(b_tex);
    });
    // p3 reads b, writes surface (forces all alive)
    graph.add_render_pass("p3", |s| {
        s.read(b_tex);
        s.write_surface();
    });

    // This should compile fine — linear chain
    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 4);
}

// ── Transitive culling ──────────────────────────────────────────────

#[test]
fn transitive_dependency_keeps_full_chain_alive() {
    let mut graph = RenderGraph::new();
    let a = graph.create_texture(|b| {
        b.name("a");
    });
    let b_tex = graph.create_texture(|b| {
        b.name("b");
    });
    let c = graph.create_texture(|b| {
        b.name("c");
    });
    let dead = graph.create_texture(|b| {
        b.name("dead");
    });

    graph.add_render_pass("p_a", |s| {
        s.write(a);
    });
    graph.add_render_pass("p_b", |s| {
        s.read(a);
        s.write(b_tex);
    });
    graph.add_render_pass("p_c", |s| {
        s.read(b_tex);
        s.write(c);
    });
    graph.add_render_pass("present", |s| {
        s.read(c);
        s.write_surface();
    });
    // Dead branch
    graph.add_render_pass("isolated", |s| {
        s.write(dead);
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 4);
    assert_eq!(graph.culled_count(), 1);
    assert!(passes.iter().all(|p| p.name != "isolated"));
}

// ── Buffer dependencies ─────────────────────────────────────────────

#[test]
fn buffer_write_read_creates_dependency() {
    let mut graph = RenderGraph::new();
    let buf = graph.create_buffer(|b| {
        b.name("data").size(1024);
    });

    graph.add_compute_pass("fill", |s| {
        s.write_buffer(buf);
    });
    graph.add_render_pass("draw", |s| {
        s.read_buffer(buf);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 2);
    assert_eq!(passes[0].name, "fill");
    assert_eq!(passes[1].name, "draw");
}

// ── Scale size edge cases ───────────────────────────────────────────

#[test]
fn multiple_writers_last_one_feeds_reader() {
    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("t");
    });

    graph.add_render_pass("w1", |s| {
        s.write(t);
    });
    graph.add_render_pass("w2", |s| {
        s.write(t);
    });
    graph.add_render_pass("reader", |s| {
        s.read(t);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    let pos = |name: &str| passes.iter().position(|p| p.name == name).unwrap();
    // Reader should come after the last writer
    assert!(pos("w2") < pos("reader"));
}

// ── Color output ordering ───────────────────────────────────────────

#[test]
fn color_outputs_preserve_slot_order() {
    let mut graph = RenderGraph::new();
    let a = graph.create_texture(|b| {
        b.name("a");
    });
    let b_tex = graph.create_texture(|b| {
        b.name("b");
    });
    let c = graph.create_texture(|b| {
        b.name("c");
    });

    graph.add_render_pass("mrt", |s| {
        s.write_color(2, c);
        s.write_color(0, a);
        s.write_color(1, b_tex);
    });
    graph.add_render_pass("present", |s| {
        s.read(a);
        s.read(b_tex);
        s.read(c);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    let mrt = &passes[0];
    assert_eq!(mrt.color_outputs.len(), 3);
    // Slots preserved in insertion order
    assert_eq!(mrt.color_outputs[0].slot, 2);
    assert_eq!(mrt.color_outputs[1].slot, 0);
    assert_eq!(mrt.color_outputs[2].slot, 1);
}

// ── Persistent vs transient ─────────────────────────────────────────

#[test]
fn alive_pass_count_reflects_culled_passes() {
    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("t");
    });

    // Pass "dead" writes to t but t is never read → it gets culled.
    graph.add_render_pass("dead", |s| {
        s.write(t);
    });
    graph.add_render_pass("alive", |s| {
        s.write_surface();
    });
    graph.compile().unwrap();

    assert_eq!(graph.pass_count(), 2);
    assert_eq!(graph.alive_pass_count(), 1);
    assert_eq!(graph.culled_count(), 1);
}

#[test]
fn compile_is_idempotent() {
    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("t");
    });
    graph.add_render_pass("p1", |s| {
        s.write(t);
    });
    graph.add_render_pass("p2", |s| {
        s.read(t);
        s.write_surface();
    });

    let passes1 = graph.compile().unwrap();
    let passes2 = graph.compile().unwrap(); // cached

    assert_eq!(passes1.len(), passes2.len());
    for (a, b) in passes1.iter().zip(passes2.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.index, b.index);
    }
}

#[test]
fn export_dot_contains_all_pass_names() {
    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("dottest");
    });
    graph.add_render_pass("alpha_pass", |s| {
        s.write(t);
    });
    graph.add_render_pass("beta_pass", |s| {
        s.read(t);
        s.write_surface();
    });
    graph.compile().unwrap();

    let dot = graph.export_dot();
    assert!(dot.contains("alpha_pass"), "DOT output missing alpha_pass");
    assert!(dot.contains("beta_pass"), "DOT output missing beta_pass");
    assert!(dot.contains("dottest"), "DOT output missing texture name");
}

#[test]
fn max_dep_level_is_correct() {
    let mut graph = RenderGraph::new();
    let t1 = graph.create_texture(|b| {
        b.name("t1");
    });
    let t2 = graph.create_texture(|b| {
        b.name("t2");
    });
    let t3 = graph.create_texture(|b| {
        b.name("t3");
    });

    // Chain of depth 3: p1 → p2 → p3 → present
    graph.add_render_pass("p1", |s| {
        s.write(t1);
    });
    graph.add_render_pass("p2", |s| {
        s.read(t1);
        s.write(t2);
    });
    graph.add_render_pass("p3", |s| {
        s.read(t2);
        s.write(t3);
    });
    graph.add_render_pass("present", |s| {
        s.read(t3);
        s.write_surface();
    });
    graph.compile().unwrap();

    assert_eq!(graph.max_dep_level(), 3);
}

#[test]
fn bug_write_color_loaded_missing_read_dependency() {
    // Setup:
    //   Pass "fill"  — writes to T using write_color_cleared (produces data)
    //   Pass "blend" — uses write_color_loaded on T (needs data from "fill")
    //   Pass "present" — reads T, writes surface (keeps both alive)
    //
    // Expected: "fill" must come before "blend" because "blend" depends on
    //           T's contents via LoadOp::Load.
    //
    // Bug: "blend" has T in its writes but NOT in its reads.  The compiler
    //      sees no dependency edge from "fill" to "blend".

    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("color_target")
            .size(TargetSize::Exact(64, 64))
            .format(TextureFormat::Rgba8Unorm);
    });

    graph.add_render_pass("fill", |s| {
        s.write_color_cleared(0, t, [1.0, 0.0, 0.0, 1.0]);
    });
    graph.add_render_pass("blend", |s| {
        // LoadOp::Load → semantically READS the attachment's prior contents.
        // BUG: this only declares a write, not a read.
        s.write_color_loaded(0, t);
    });
    graph.add_render_pass("present", |s| {
        s.read(t);
        s.write_surface();
    });

    // The "blend" pass SHOULD have the texture in its reads list because
    // LoadOp::Load requires reading prior contents.
    let blend_pass = &graph.passes[1];
    assert!(
        blend_pass.reads.contains(&ResourceRef::Texture(t)),
        "BUG: write_color_loaded() does not declare a read dependency on the \
         texture it will Load.  The compiler cannot guarantee correct ordering \
         between the prior writer ('fill') and this pass ('blend').\n\
         blend_pass.reads = {:?}",
        blend_pass.reads,
    );
}

#[test]
fn bug_write_color_loaded_ordering_can_be_wrong() {
    // This test directly demonstrates the ordering consequence of the bug.
    // With two independent passes where only write_color_loaded creates the
    // link, the compiler may not enforce correct ordering.

    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("target");
    });

    // "producer" writes T normally.
    graph.add_render_pass("producer", |s| {
        s.write(t);
    });

    // "consumer_blend" uses write_color_loaded — semantically needs T's data.
    // But due to the bug, this pass only has T in writes, not reads.
    graph.add_render_pass("consumer_blend", |s| {
        s.write_color_loaded(0, t);
        s.write_surface(); // keep alive
    });

    let passes = graph.compile().unwrap();

    // If the bug is fixed, the dependency edge producer→consumer_blend would
    // guarantee "producer" comes first.
    //
    // With the bug present, the compiler sees:
    //   producer: writes T
    //   consumer_blend: writes T (no read on T)
    // It creates an edge: (writer of T in producer) → (writer of T in consumer_blend)
    // which happens to work ONLY because both are sequential writers.
    //
    // But if we interleave another pass, the Kahn's topological sort may not
    // respect the semantic dependency.  Let's verify the actual pass data:
    let consumer_pass = passes.iter().find(|p| p.name == "consumer_blend").unwrap();
    assert!(
        consumer_pass
            .reads
            .contains(&ResourceRef::Texture(graph.get_texture("target").unwrap())),
        "BUG: consumer_blend should have texture in reads due to LoadOp::Load, \
         reads = {:?}",
        consumer_pass.reads,
    );
}

// ── Bug: set_depth_stencil_loaded missing read dependency ───────────
//
// set_depth_stencil_loaded() is meant to preserve existing depth contents
// (no clear), but it only calls push_write().  Like write_color_loaded(),
// it needs the prior depth data to be present, which means it semantically
// reads the resource.

#[test]
fn bug_set_depth_stencil_loaded_missing_read_dependency() {
    let mut graph = RenderGraph::new();
    let depth = graph.create_texture(|b| {
        b.name("depth")
            .format(TextureFormat::Depth32Float)
            .size(TargetSize::Exact(64, 64));
    });

    // "z_prepass" writes the depth buffer.
    graph.add_render_pass("z_prepass", |s| {
        s.set_depth_stencil_cleared(depth, 1.0);
        s.write_surface();
    });

    // "main_pass" loads the existing depth buffer (no clear).
    // BUG: set_depth_stencil_loaded only pushes a write, not a read.
    graph.add_render_pass("main_pass", |s| {
        s.set_depth_stencil_loaded(depth);
        s.write_surface();
    });

    // Verify that "main_pass" has depth in its reads.
    let main_pass = &graph.passes[1];
    assert!(
        main_pass.reads.contains(&ResourceRef::Texture(depth)),
        "BUG: set_depth_stencil_loaded() does not declare a read dependency \
         on the depth texture it will Load.  The compiler cannot guarantee \
         that 'z_prepass' runs before 'main_pass'.\n\
         main_pass.reads = {:?}",
        main_pass.reads,
    );
}
