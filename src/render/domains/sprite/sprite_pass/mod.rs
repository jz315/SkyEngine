mod resources;

use std::sync::atomic::{AtomicU64, Ordering};

use crate::gpu::GpuContext;
use crate::render::ecs::RenderSettings;
use crate::render::frame_pipeline::{
    pass_first_write_texture, require_render_target, FrameViewNode, PhaseState, PreparedFrame,
    PreparedView, ViewExecutionContext,
};
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, RenderGraphError, TargetSize,
};
use crate::render::internal::LazyNodeResources;

use self::resources::SpriteSceneNodeResources;
use super::{draw_spans, gpu_scene, prepared_view_2d};

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
static NEXT_SPRITE_SCENE_NAME: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy)]
enum SpriteTargetFormat {
    Fixed(wgpu::TextureFormat),
    Surface,
}

pub struct SpriteSceneNode {
    resources: LazyNodeResources<SpriteSceneNodeResources>,
    target_format: SpriteTargetFormat,
}

impl SpriteSceneNode {
    pub fn new() -> Self {
        Self {
            resources: LazyNodeResources::new(),
            target_format: SpriteTargetFormat::Fixed(HDR_FORMAT),
        }
    }

    pub(crate) fn hdr(ctx: &GpuContext) -> Self {
        let mut pass = Self::new();
        pass.initialize(ctx);
        pass
    }

    pub(crate) fn surface(ctx: &GpuContext) -> Self {
        let mut pass = Self {
            resources: LazyNodeResources::new(),
            target_format: SpriteTargetFormat::Surface,
        };
        pass.initialize(ctx);
        pass
    }

    fn initialize(&mut self, ctx: &GpuContext) {
        self.resources
            .initialize_with(|| SpriteSceneNodeResources::new(ctx));
    }

    fn resources_mut(&mut self, ctx: &GpuContext) -> &mut SpriteSceneNodeResources {
        self.resources
            .get_or_init(|| SpriteSceneNodeResources::new(ctx))
    }

    fn target_format(&self, state: &PhaseState) -> wgpu::TextureFormat {
        match self.target_format {
            SpriteTargetFormat::Fixed(format) => format,
            SpriteTargetFormat::Surface => state.surface_format(),
        }
    }
}

impl Default for SpriteSceneNode {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameViewNode for SpriteSceneNode {
    fn name(&self) -> &'static str {
        "sprites"
    }

    fn setup(
        &mut self,
        graph: &mut RenderGraph,
        state: &mut PhaseState,
        _frame: &PreparedFrame<'_>,
        view: &PreparedView<'_>,
    ) {
        let target_format = self.target_format(state);
        let scene_name = format!(
            "scene_{}",
            NEXT_SPRITE_SCENE_NAME.fetch_add(1, Ordering::Relaxed)
        );
        let scene_tex = graph.create_texture(|b| {
            b.name(scene_name.clone())
                .size(TargetSize::Exact(
                    view.target_size()[0],
                    view.target_size()[1],
                ))
                .format(target_format);
        });
        graph.add_render_pass("sprites", |s| {
            s.write_color(0, scene_tex);
        });
        state.set_texture_slot("scene_color", scene_tex, target_format);
        state.set_current_color(scene_tex, target_format);
    }

    fn execute(
        &mut self,
        pass: &CompiledPass,
        ctx: &mut GpuContext,
        resources: &PhysicalResources<'_>,
        execution: &ViewExecutionContext<'_>,
    ) -> Result<(), RenderGraphError> {
        let target_handle = pass_first_write_texture(pass, self.name(), "scene");
        let target = require_render_target(resources, target_handle, self.name(), "scene");

        let settings = execution
            .frame()
            .payload::<RenderSettings>()
            .copied()
            .unwrap_or_default();
        let prepared_view = prepared_view_2d(execution);
        let gpu_scene = gpu_scene(execution);
        let resources_mut = self.resources_mut(ctx);
        ctx.queue().write_buffer(
            &resources_mut.camera_buffer,
            0,
            bytemuck::bytes_of(&prepared_view.view_uniform),
        );
        let scene_bind_group = resources_mut
            .scene_bind_group(
                ctx,
                gpu_scene.sprite_table_buffer(),
                gpu_scene.sprite_table_version(),
            )
            .clone();

        let format = target.format();
        let pip_color = resources_mut.pipeline_for(ctx, format, false);
        let pip_tex = resources_mut.pipeline_for(ctx, format, true);
        let camera_bind_group = resources_mut.camera_bind_group.clone();
        let vertex_buffer = resources_mut.vertex_buffer.clone();
        let index_buffer = resources_mut.index_buffer.clone();
        let bind_groups = resources_mut.texture_bind_groups(ctx, gpu_scene.textures());
        let load = wgpu::LoadOp::Clear(settings.clear_color.to_wgpu());

        ctx.with_render_pass(
            &wgpu::RenderPassDescriptor {
                label: Some("sprite_scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            },
            |render_pass| {
                render_pass.set_bind_group(0, &camera_bind_group, &[]);
                render_pass.set_bind_group(1, &scene_bind_group, &[]);
                render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                render_pass.set_vertex_buffer(1, gpu_scene.visible_sprite_index_buffer().slice(..));
                render_pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint16);

                for draw in draw_spans(execution) {
                    if let Some(texture_index) = draw.texture_index {
                        let Some(bg) = bind_groups.get(texture_index) else {
                            continue;
                        };
                        render_pass.set_pipeline(pip_tex.as_ref());
                        render_pass.set_bind_group(2, bg, &[]);
                    } else {
                        render_pass.set_pipeline(pip_color.as_ref());
                    }
                    render_pass.draw_indexed(
                        0..6,
                        0,
                        draw.first_instance..(draw.first_instance + draw.instance_count),
                    );
                }
            },
        );
        Ok(())
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        draw_spans(execution).len()
    }
}
