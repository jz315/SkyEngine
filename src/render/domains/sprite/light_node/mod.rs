mod resources;

use crate::gpu::GpuContext;
use crate::render::core::target::RenderTarget;
use crate::render::ecs::RenderSettings;
use crate::render::frame_pipeline::{
    pass_first_write_texture, require_render_target, FrameViewNode, PhaseState, PreparedFrame,
    PreparedView, ViewExecutionContext,
};
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, TargetSize,
};
use crate::render::internal::LazyNodeResources;

use self::resources::SpriteLightNodeResources;
use super::{gpu_scene, prepared_view_2d};

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub struct SpriteLightNode {
    resources: LazyNodeResources<SpriteLightNodeResources>,
    #[cfg(test)]
    last_prepared_ambient: Option<[f32; 4]>,
}

impl SpriteLightNode {
    pub fn new(ctx: &GpuContext) -> Self {
        let mut node = Self {
            resources: LazyNodeResources::new(),
            #[cfg(test)]
            last_prepared_ambient: None,
        };
        node.initialize(ctx);
        node
    }

    fn initialize(&mut self, ctx: &GpuContext) {
        self.resources
            .initialize_with(|| SpriteLightNodeResources::new(ctx));
    }

    fn resources_mut(&mut self, ctx: &GpuContext) -> &mut SpriteLightNodeResources {
        self.resources
            .get_or_init(|| SpriteLightNodeResources::new(ctx))
    }

    fn validate_targets(normal_target: Option<&RenderTarget>, lightmap: &RenderTarget) {
        if let Some(normal_target) = normal_target {
            assert!(
                !std::ptr::eq(normal_target.texture(), lightmap.texture()),
                "SpriteLightNode requires distinct normal and lightmap targets",
            );
        }
    }
}

impl FrameViewNode for SpriteLightNode {
    fn name(&self) -> &'static str {
        "lights"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        _frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let light_tex = graph.create_texture(|b| {
            b.name("light")
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(HDR_FORMAT);
        });
        graph.add_render_pass("lights", |s| {
            s.write_color(0, light_tex);
        });
        state.set_texture_slot("lightmap", light_tex, HDR_FORMAT);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        let settings = execution
            .frame()
            .payload::<RenderSettings>()
            .copied()
            .unwrap_or_default();
        let prepared_view = prepared_view_2d(execution);
        let gpu_scene = gpu_scene(execution);
        #[cfg(test)]
        {
            self.last_prepared_ambient = Some(settings.ambient_color.to_array());
        }

        let target_handle = pass_first_write_texture(pass, self.name(), "lightmap");
        let target = require_render_target(resources, target_handle, self.name(), "lightmap");
        Self::validate_targets(None, target);

        let resources_mut = self.resources_mut(ctx);
        let pipeline = resources_mut.pipeline_for(ctx, target.format());
        ctx.queue().write_buffer(
            &resources_mut.camera_buffer,
            0,
            bytemuck::bytes_of(&prepared_view.view_uniform),
        );
        let scene_bg = resources_mut
            .scene_bind_group(
                ctx,
                gpu_scene.light_table_buffer(),
                gpu_scene.light_table_version(),
            )
            .clone();
        let normal_bg = resources_mut.normal_bind_group.clone();

        ctx.with_render_pass(
            &wgpu::RenderPassDescriptor {
                label: Some("light_scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: settings.ambient_color.r as f64,
                            g: settings.ambient_color.g as f64,
                            b: settings.ambient_color.b as f64,
                            a: settings.ambient_color.a as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            },
            |render_pass| {
                render_pass.set_pipeline(pipeline.as_ref());
                render_pass.set_bind_group(0, &resources_mut.camera_bind_group, &[]);
                render_pass.set_bind_group(1, &scene_bg, &[]);
                render_pass.set_bind_group(2, &normal_bg, &[]);
                render_pass.set_vertex_buffer(0, resources_mut.vertex_buffer.slice(..));
                render_pass.set_vertex_buffer(1, gpu_scene.visible_light_index_buffer().slice(..));
                render_pass.set_index_buffer(
                    resources_mut.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                let instance_range = prepared_view.light_range();
                if instance_range.start < instance_range.end {
                    render_pass.draw_indexed(0..6, 0, instance_range);
                }
            },
        );
        Ok(())
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        usize::from(!prepared_view_2d(execution).light_range().is_empty())
    }

    #[cfg(test)]
    fn debug_last_light_ambient(&self) -> Option<[f32; 4]> {
        self.last_prepared_ambient
    }
}
