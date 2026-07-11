use crate::render::execution::{
    create_scene_texture, ensure_scene_texture, pass_first_write_texture, pass_nth_write_texture,
    require_render_target, PreparedFrame, PreparedView, SceneTexture,
};
use crate::render::execution::{PhaseExecuteContext, PhaseSetupContext};
use crate::render::gpu::GpuScene;
use crate::render::graph::RenderGraphError;
use crate::render::phase::{
    OpaquePhase, SceneMaterialPrepassContext, SceneMaterialPrepassPipelineCache,
};
use crate::render::pipeline::RenderPhase;
use crate::render::{SceneView, DEFAULT_DEPTH_FORMAT};

use super::normal::{SCENE_NORMAL_FORMAT, SCENE_VELOCITY_CLEAR, SCENE_VELOCITY_FORMAT};

pub(super) const SCENE_ALBEDO_FORMAT: wgpu::TextureFormat = SceneTexture::Albedo.modern_3d_format();
pub(super) const SCENE_MATERIAL_FORMAT: wgpu::TextureFormat =
    SceneTexture::Material.modern_3d_format();
pub(super) const SCENE_EMISSIVE_FORMAT: wgpu::TextureFormat =
    SceneTexture::Emissive.modern_3d_format();
pub(super) const SCENE_MATERIAL_VELOCITY_CLEAR_PASS: &str = "scene_material_prepass_velocity_clear";

#[derive(Default)]
pub struct SceneMaterialPrepass {
    pipeline_cache: SceneMaterialPrepassPipelineCache,
}

impl SceneMaterialPrepass {
    fn is_view_enabled(view: &PreparedView<'_>) -> bool {
        !view
            .payload::<SceneView>()
            .is_some_and(SceneView::is_shadow)
            && view
                .payload::<OpaquePhase>()
                .is_some_and(|phase| !phase.is_empty())
    }
}

impl RenderPhase for SceneMaterialPrepass {
    fn name(&self) -> &'static str {
        "scene_material_prepass"
    }

    fn is_enabled(&self, _frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        Self::is_view_enabled(view)
    }

    fn setup(&mut self, ctx: &mut PhaseSetupContext<'_, '_>) {
        if !Self::is_view_enabled(ctx.view()) {
            return;
        }

        let target_size = ctx.view().target_size();
        let (
            albedo,
            material,
            emissive,
            normal,
            velocity,
            depth,
            existing_depth,
            existing_normal,
            existing_velocity,
        ) = {
            let (graph, state) = ctx.graph_and_state();
            let albedo = create_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Albedo,
                SCENE_ALBEDO_FORMAT,
                "scene_albedo",
            );
            let material = create_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Material,
                SCENE_MATERIAL_FORMAT,
                "scene_material",
            );
            let emissive = create_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Emissive,
                SCENE_EMISSIVE_FORMAT,
                "scene_emissive",
            );
            let existing_normal = state.scene_normal().is_some();
            let normal = ensure_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Normal,
                SCENE_NORMAL_FORMAT,
                "scene_normal",
            );
            let existing_velocity = state.scene_velocity().is_some();
            let velocity = ensure_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Velocity,
                SCENE_VELOCITY_FORMAT,
                "scene_velocity",
            );
            let existing_depth = state.scene_depth().is_some();
            let depth = ensure_scene_texture(
                graph,
                state,
                target_size,
                SceneTexture::Depth,
                DEFAULT_DEPTH_FORMAT,
                "scene_depth",
            );
            (
                albedo,
                material,
                emissive,
                normal,
                velocity,
                depth,
                existing_depth,
                existing_normal,
                existing_velocity,
            )
        };

        if !existing_velocity {
            ctx.graph()
                .add_render_pass(SCENE_MATERIAL_VELOCITY_CLEAR_PASS, |setup| {
                    setup.write_color_cleared(0, velocity.handle(), SCENE_VELOCITY_CLEAR);
                });
        }

        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.write_color_cleared(0, albedo.handle(), [0.0, 0.0, 0.0, 0.0]);
            setup.write_color_cleared(1, material.handle(), [1.0, 0.0, 1.0, 0.0]);
            setup.write_color_cleared(2, emissive.handle(), [0.0, 0.0, 0.0, 0.0]);
            if existing_normal {
                setup.write_color_loaded(3, normal.handle());
            } else {
                setup.write_color_cleared(3, normal.handle(), [0.5, 0.5, 1.0, 1.0]);
            }
            if existing_depth {
                setup.set_depth_stencil_loaded(depth.handle());
            } else {
                setup.set_depth_stencil(depth.handle());
            }
        });
    }

    fn execute(
        &mut self,
        ctx: &mut PhaseExecuteContext<'_, '_, '_>,
    ) -> Result<(), RenderGraphError> {
        let (gpu, pass, resources, execution, draw_services) = ctx.split();
        let (draw_functions, materials, mesh_registry, _fallback_texture) = draw_services.split();
        if pass.name == SCENE_MATERIAL_VELOCITY_CLEAR_PASS {
            let velocity_handle = pass_first_write_texture(pass, self.name(), "scene_velocity");
            let velocity_rt =
                require_render_target(resources, velocity_handle, self.name(), "scene_velocity");
            let mut frame = gpu.frame();
            let color_attachments = [Some(wgpu::RenderPassColorAttachment {
                view: velocity_rt.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: SCENE_VELOCITY_CLEAR[0] as f64,
                        g: SCENE_VELOCITY_CLEAR[1] as f64,
                        b: SCENE_VELOCITY_CLEAR[2] as f64,
                        a: SCENE_VELOCITY_CLEAR[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })];
            let _clear_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(SCENE_MATERIAL_VELOCITY_CLEAR_PASS),
                color_attachments: &color_attachments,
                ..Default::default()
            });
            return Ok(());
        }

        let Some(scene_view) = execution.view_payload::<SceneView>() else {
            return Ok(());
        };
        if scene_view.is_shadow() {
            return Ok(());
        }
        let Some(opaque_phase) = execution.view_payload::<OpaquePhase>() else {
            return Ok(());
        };
        let phase_items = opaque_phase.items();
        if phase_items.is_empty() {
            return Ok(());
        }

        let Some(gpu_scene) = execution.frame_payload::<GpuScene>() else {
            return Ok(());
        };
        gpu_scene.write_view_uniform(gpu.queue(), &scene_view.view_uniform);

        let albedo_handle = pass_first_write_texture(pass, self.name(), "scene_albedo");
        let material_handle = pass_nth_write_texture(pass, 1, self.name(), "scene_material");
        let emissive_handle = pass_nth_write_texture(pass, 2, self.name(), "scene_emissive");
        let normal_handle = pass_nth_write_texture(pass, 3, self.name(), "scene_normal");
        let albedo_rt =
            require_render_target(resources, albedo_handle, self.name(), "scene_albedo");
        let material_rt =
            require_render_target(resources, material_handle, self.name(), "scene_material");
        let emissive_rt =
            require_render_target(resources, emissive_handle, self.name(), "scene_emissive");
        let normal_rt =
            require_render_target(resources, normal_handle, self.name(), "scene_normal");
        let Some(depth_output) = pass.depth_stencil.as_ref() else {
            return Ok(());
        };
        let depth_rt = require_render_target(resources, depth_output.handle, self.name(), "depth");

        let color_load = |index: usize| match pass.color_outputs[index].load {
            crate::render::graph::LoadOp::Clear([r, g, b, a]) => wgpu::LoadOp::Clear(wgpu::Color {
                r: r as f64,
                g: g as f64,
                b: b as f64,
                a: a as f64,
            }),
            crate::render::graph::LoadOp::Load => wgpu::LoadOp::Load,
            crate::render::graph::LoadOp::DontCare => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
        };
        let color_attachments = [
            Some(wgpu::RenderPassColorAttachment {
                view: albedo_rt.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load(0),
                    store: wgpu::StoreOp::Store,
                },
            }),
            Some(wgpu::RenderPassColorAttachment {
                view: material_rt.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load(1),
                    store: wgpu::StoreOp::Store,
                },
            }),
            Some(wgpu::RenderPassColorAttachment {
                view: emissive_rt.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load(2),
                    store: wgpu::StoreOp::Store,
                },
            }),
            Some(wgpu::RenderPassColorAttachment {
                view: normal_rt.view(),
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: color_load(3),
                    store: wgpu::StoreOp::Store,
                },
            }),
        ];
        let depth_attachment = Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth_rt.view(),
            depth_ops: Some(wgpu::Operations {
                load: match depth_output.clear_depth {
                    Some(value) => wgpu::LoadOp::Clear(value),
                    None => wgpu::LoadOp::Load,
                },
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        });

        let model_matrices = execution
            .frame_payload::<Vec<[f32; 16]>>()
            .map(std::vec::Vec::as_slice);
        let device = gpu.device().clone();
        let mut frame = gpu.frame();
        let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: depth_attachment,
            ..Default::default()
        });
        let mut prepass_ctx = SceneMaterialPrepassContext::new(
            &device,
            &mut render_pass,
            gpu_scene.view_bind_group(),
            gpu_scene.view_bind_group_layout(),
            model_matrices,
            materials,
            mesh_registry,
            &mut self.pipeline_cache,
            albedo_rt.format(),
            material_rt.format(),
            emissive_rt.format(),
            normal_rt.format(),
            depth_rt.format(),
        );
        let mut cursor = 0usize;
        while cursor < phase_items.len() {
            let draw_function_id = phase_items[cursor].draw_function_id;
            let mut batch_end = cursor + 1;
            while batch_end < phase_items.len()
                && phase_items[batch_end].draw_function_id == draw_function_id
            {
                batch_end += 1;
            }
            if draw_functions.supports_scene_material_prepass(draw_function_id) {
                draw_functions
                    .draw_scene_material_prepass_batch(
                        draw_function_id,
                        &mut prepass_ctx,
                        &phase_items[cursor..batch_end],
                    )
                    .map_err(|error| RenderGraphError::ExecutionFailed(error.to_string()))?;
            }
            cursor = batch_end;
        }

        Ok(())
    }
}
