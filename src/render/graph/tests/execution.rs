use super::super::*;
use super::common::*;

#[test]
fn try_execute_propagates_render_pass_error() {
    let (device, queue) = create_test_device();
    let mut ctx = crate::gpu::GpuContext::new_headless(
        device,
        queue,
        wgpu::TextureFormat::Bgra8Unorm,
        [16, 16],
    );

    let mut graph = RenderGraph::new();
    graph.add_render_pass("present", |s| {
        s.write_surface();
    });

    let err = graph
        .try_execute(&mut ctx, |_pass, _gpu, _resources| {
            Err(RenderGraphError::ExecutionFailed("boom".into()))
        })
        .unwrap_err();
    assert!(matches!(err, RenderGraphError::ExecutionFailed(ref msg) if
        msg.contains("pass \"present\"")
            && msg.contains("#0")
            && msg.contains("Render")
            && msg.contains("exec #0")
            && msg.contains("compiled order [0]")
            && msg.contains("boom")
    ));
}
