use super::super::*;

#[test]
fn queue_diagnostic_reports_async_compute_candidate_without_changing_order() {
    let mut graph = RenderGraph::new();
    let lighting = graph.create_texture(|b| {
        b.name("lighting").storage_binding().sampled();
    });

    graph.add_compute_pass("async_lighting", |s| {
        s.with_flags(PassFlags::PREFER_ASYNC_COMPUTE);
        s.write(lighting);
    });
    graph.add_render_pass("present", |s| {
        s.read(lighting);
        s.write_surface();
    });

    graph.compile().unwrap();
    let before_order = graph.debug_dump().compiled_execution_order;
    let diagnostic = graph.queue_schedule_diagnostic();
    let after_order = graph.debug_dump().compiled_execution_order;

    assert_eq!(before_order, after_order);
    assert_eq!(diagnostic.async_compute_candidates, 1);

    let assignment = diagnostic
        .pass_assignments
        .iter()
        .find(|assignment| assignment.name == "async_lighting")
        .expect("async compute pass should be reported");
    assert_eq!(assignment.class, QueueDiagnosticClass::ComputeCandidate);
    assert_eq!(assignment.execution_order, Some(0));
    assert!(assignment
        .reasons
        .contains(&QueueScheduleReason::PreferAsyncCompute));
    assert_eq!(
        assignment.shared_with_graphics,
        vec![ResourceRef::Texture(lighting)]
    );
}

#[test]
fn queue_diagnostic_reports_surface_as_graphics_required() {
    let mut graph = RenderGraph::new();

    graph.add_render_pass("present", |s| {
        s.write_surface();
    });

    graph.compile().unwrap();
    let diagnostic = graph.queue_schedule_diagnostic();
    let assignment = diagnostic
        .pass_assignments
        .iter()
        .find(|assignment| assignment.name == "present")
        .expect("surface pass should be reported");

    assert_eq!(assignment.class, QueueDiagnosticClass::GraphicsRequired);
    assert!(assignment
        .reasons
        .contains(&QueueScheduleReason::SurfaceInteraction));
    assert!(diagnostic
        .blockers
        .iter()
        .any(|blocker| matches!(blocker, QueueScheduleBlocker::SurfaceInteraction { .. })));
}

#[test]
fn queue_diagnostic_reports_copy_candidate() {
    let mut graph = RenderGraph::new();
    let uploaded = graph.create_texture(|b| {
        b.name("uploaded")
            .size(TargetSize::Exact(2, 2))
            .format(TextureFormat::Rgba8Unorm)
            .copy_dst()
            .sampled();
    });

    graph.add_copy_pass("upload", |s| {
        s.upload_to_texture(vec![255; 16], uploaded, 2, 2, 4);
    });
    graph.add_render_pass("present", |s| {
        s.read(uploaded);
        s.write_surface();
    });

    graph.compile().unwrap();
    let diagnostic = graph.queue_schedule_diagnostic();
    let assignment = diagnostic
        .pass_assignments
        .iter()
        .find(|assignment| assignment.name == "upload")
        .expect("copy pass should be reported");

    assert_eq!(diagnostic.copy_queue_candidates, 1);
    assert_eq!(assignment.class, QueueDiagnosticClass::CopyCandidate);
    assert!(assignment.reasons.contains(&QueueScheduleReason::CopyPass));
    assert_eq!(
        assignment.shared_with_graphics,
        vec![ResourceRef::Texture(uploaded)]
    );
}
