use super::common::*;

struct OrderedGraphPass {
    name: &'static str,
    executions: Arc<Mutex<Vec<&'static str>>>,
}

impl GraphPass for OrderedGraphPass {
    fn name(&self) -> &'static str {
        self.name
    }

    fn setup(&mut self, ctx: &mut GraphPassSetupContext<'_, '_>) {
        let target_size = ctx.view().target_size();
        let output = ctx.graph().create_texture(|builder| {
            builder
                .name(self.name)
                .size(TargetSize::Exact(target_size[0], target_size[1]))
                .format(wgpu::TextureFormat::Bgra8Unorm)
                .persistent();
        });
        ctx.graph().add_render_pass(self.name, |setup| {
            setup.write_color(0, output);
        });
    }

    fn execute(
        &mut self,
        ctx: &mut GraphPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        if ctx.pass().name == self.name {
            self.executions
                .lock()
                .expect("order log lock")
                .push(self.name);
        }
        Ok(())
    }
}

#[test]
fn custom_graph_pass_execution_order_matches_builder_order() {
    let (device, queue) = create_test_device();
    let mut ctx =
        GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
    let executions = Arc::new(Mutex::new(Vec::new()));
    let pipeline = RenderPipelineAsset::builder()
        .add_graph_pass(OrderedGraphPass {
            name: "graph_order_a",
            executions: executions.clone(),
        })
        .add_graph_pass(OrderedGraphPass {
            name: "graph_order_b",
            executions: executions.clone(),
        })
        .build();
    assert_eq!(
        pipeline.descriptor().step_names,
        vec![
            crate::render::PipelineStepDescriptor::Graph("graph_order_a"),
            crate::render::PipelineStepDescriptor::Graph("graph_order_b"),
        ]
    );
    let mut renderer = RenderRuntime::from_asset(pipeline);
    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic_fixed(32.0, 32.0),
        MainCamera,
    ));

    ctx.begin_frame()
        .expect("headless begin_frame should succeed");
    renderer.render_world(&mut ctx, &world);
    ctx.end_frame();

    assert_eq!(
        executions.lock().expect("order log lock").as_slice(),
        ["graph_order_a", "graph_order_b"]
    );
}

#[test]
fn forward_3d_descriptor_includes_directional_shadow_phase() {
    let descriptor = RenderPipelineAsset::forward_3d().descriptor();
    assert!(descriptor.step_names.iter().any(|step| matches!(
        step,
        crate::render::PipelineStepDescriptor::Phase("directional_shadow")
    )));
    assert!(descriptor.step_names.iter().any(|step| matches!(
        step,
        crate::render::PipelineStepDescriptor::Compute("gi_update")
    )));
    assert!(!descriptor.step_names.iter().any(|step| matches!(
        step,
        crate::render::PipelineStepDescriptor::Phase("scene_material_prepass")
    )));
}

#[test]
fn modern_3d_descriptor_uses_wicked_style_pass_order() {
    use crate::render::PipelineStepDescriptor::{Compute, Phase, PostFx};

    let descriptor = RenderPipelineAsset::modern_3d().descriptor();
    let steps = descriptor.step_names;
    let expected = [
        Phase("scene_normal_prepass"),
        Phase("scene_material_prepass"),
        Phase("directional_shadow"),
        Compute("gi_update"),
        Phase("opaque"),
        PostFx("contact_shadows"),
        PostFx("gi_composite"),
        Phase("transparent"),
        PostFx("taa"),
        PostFx("sharpen"),
        PostFx("bloom"),
        PostFx("tonemap"),
        PostFx("debug_view"),
    ];
    assert_eq!(steps.as_slice(), &expected);
}
