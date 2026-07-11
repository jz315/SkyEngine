use super::super::*;

#[test]
fn pass_setup_with_flags_are_preserved() {
    let mut graph = RenderGraph::new();
    graph.add_render_pass("flagged", |s| {
        s.write_surface();
        s.with_flags(PassFlags::PREFER_ASYNC_COMPUTE);
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes.len(), 1);
    assert!(passes[0].flags.contains(PassFlags::PREFER_ASYNC_COMPUTE));
}

#[test]
fn write_color_defaults_to_dont_care() {
    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("color");
    });

    graph.add_render_pass("draw", |s| {
        s.write_color(0, t);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    let pass = &passes[0];
    assert_eq!(pass.color_outputs.len(), 1);
    assert_eq!(pass.color_outputs[0].slot, 0);
    assert!(matches!(pass.color_outputs[0].load, LoadOp::DontCare));
}

#[test]
fn write_color_loaded_preserves_existing_contents() {
    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("color").persistent();
    });

    graph.add_render_pass("blend", |s| {
        s.write_color_loaded(0, t);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    let pass = &passes[0];
    assert_eq!(pass.color_outputs.len(), 1);
    assert!(matches!(pass.color_outputs[0].load, LoadOp::Load));
}

#[test]
fn write_color_cleared_sets_clear_color() {
    let mut graph = RenderGraph::new();
    let t = graph.create_texture(|b| {
        b.name("color");
    });

    graph.add_render_pass("clear_draw", |s| {
        s.write_color_cleared(0, t, [1.0, 0.0, 0.5, 1.0]);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    let pass = &passes[0];
    assert_eq!(pass.color_outputs.len(), 1);
    match pass.color_outputs[0].load {
        LoadOp::Clear(color) => {
            assert_eq!(color, [1.0, 0.0, 0.5, 1.0]);
        }
        _ => panic!("expected LoadOp::Clear"),
    }
}

#[test]
fn write_surface_color_targets_surface() {
    let mut graph = RenderGraph::new();

    graph.add_render_pass("surface_mrt", |s| {
        s.write_surface_color(0, LoadOp::Clear([0.0, 0.0, 0.0, 1.0]));
    });

    let passes = graph.compile().unwrap();
    let pass = &passes[0];
    assert_eq!(pass.color_outputs.len(), 1);
    assert_eq!(pass.color_outputs[0].target, ResourceRef::Surface);
}

#[test]
fn mrt_multi_slot_color_outputs() {
    let mut graph = RenderGraph::new();
    let t0 = graph.create_texture(|b| {
        b.name("rt0");
    });
    let t1 = graph.create_texture(|b| {
        b.name("rt1");
    });
    let t2 = graph.create_texture(|b| {
        b.name("rt2");
    });

    graph.add_render_pass("gbuffer", |s| {
        s.write_color_cleared(0, t0, [0.0; 4]);
        s.write_color_cleared(1, t1, [0.0; 4]);
        s.write_color(2, t2);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    assert_eq!(passes[0].color_outputs.len(), 3);
    assert_eq!(passes[0].color_outputs[0].slot, 0);
    assert_eq!(passes[0].color_outputs[1].slot, 1);
    assert_eq!(passes[0].color_outputs[2].slot, 2);
}

#[test]
fn set_depth_stencil_defaults_to_cleared_depth() {
    let mut graph = RenderGraph::new();
    let depth = graph.create_texture(|b| {
        b.name("depth").format(TextureFormat::Depth32Float);
    });

    graph.add_render_pass("draw", |s| {
        s.set_depth_stencil(depth);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    let ds = passes[0].depth_stencil.as_ref().unwrap();
    assert_eq!(ds.handle, depth);
    assert_eq!(ds.clear_depth, Some(1.0));
    assert!(ds.clear_stencil.is_none());
    assert!(ds.depth_store);
}

#[test]
fn set_depth_stencil_loaded_preserves_existing_depth() {
    let mut graph = RenderGraph::new();
    let depth = graph.create_texture(|b| {
        b.name("depth")
            .format(TextureFormat::Depth32Float)
            .persistent();
    });

    graph.add_render_pass("draw", |s| {
        s.set_depth_stencil_loaded(depth);
        s.write_surface();
    });

    let passes = graph.compile().unwrap();
    let ds = passes[0].depth_stencil.as_ref().unwrap();
    assert_eq!(ds.handle, depth);
    assert!(ds.clear_depth.is_none());
    assert!(ds.clear_stencil.is_none());
}
