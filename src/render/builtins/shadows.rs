use crate::math::{Mat4, Vec3};
use crate::render::builtins::logging::should_log_scene_view;
use crate::render::component::RenderSettings;
use crate::render::execution::{
    pass_first_write_texture, pass_nth_read_texture, require_render_target,
    PostFxPassExecuteContext, PostFxPassSetupContext, PreparedFrame, PreparedView,
    ViewExecutionContext,
};
use crate::render::gpu::GpuScene;
use crate::render::graph::RenderGraphError;
use crate::render::pipeline::PostFxPass;
use crate::render::postfx::contact_shadows::{
    ContactShadows as LowLevelContactShadows, ContactShadowsParams,
};
use crate::render::view::SceneView;

#[derive(Default)]
pub struct ContactShadows {
    runtime: Option<LowLevelContactShadows>,
}

impl ContactShadows {
    fn primary_directional_light(execution: &ViewExecutionContext<'_>) -> Option<[f32; 3]> {
        let gpu_scene = execution.frame_payload::<GpuScene>()?;
        gpu_scene
            .table::<crate::render::LightTable>()
            .lights()
            .iter()
            .find(|light| light.is_directional())
            .map(|light| {
                let dir = Vec3::from_array([
                    -light.pos_radius[0],
                    -light.pos_radius[1],
                    -light.pos_radius[2],
                ]);
                dir.normalized().to_array()
            })
    }
}

impl PostFxPass for ContactShadows {
    fn name(&self) -> &'static str {
        "contact_shadows"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        if view
            .payload::<SceneView>()
            .is_some_and(SceneView::is_shadow)
        {
            return false;
        }
        frame
            .payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .contact_shadows
            .enabled
    }

    fn requires_hdr_input(&self) -> bool {
        true
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        let settings = ctx
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .contact_shadows;
        if !settings.enabled {
            return;
        }
        if ctx
            .scene_lighting()
            .is_none_or(|lighting| lighting.directional_light_count() == 0)
        {
            return;
        }

        let Some(input) = ctx.state().current_color() else {
            return;
        };
        let Some(depth) = ctx.state().scene_depth() else {
            return;
        };
        let Some(normal) = ctx.state().scene_normal() else {
            return;
        };

        let target_size = ctx.view().target_size();
        let output = ctx.graph().create_texture(|builder| {
            builder
                .name("contact_shadows_out")
                .size(crate::render::graph::TargetSize::Exact(
                    target_size[0],
                    target_size[1],
                ))
                .format(input.format());
        });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(input.handle());
            setup.read(depth.handle());
            setup.read(normal.handle());
            setup.write_color(0, output);
        });
        ctx.state().set_current_color(output, input.format());
        ctx.state().set_scene_color(output, input.format());
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        let (gpu, pass, resources, execution) = ctx.split();
        let input_handle = pass_nth_read_texture(pass, 0, self.name(), "input");
        let depth_handle = pass_nth_read_texture(pass, 1, self.name(), "depth");
        let normal_handle = pass_nth_read_texture(pass, 2, self.name(), "normal");
        let output_handle = pass_first_write_texture(pass, self.name(), "output");
        let input = require_render_target(resources, input_handle, self.name(), "input");
        let depth = require_render_target(resources, depth_handle, self.name(), "depth");
        let normal = require_render_target(resources, normal_handle, self.name(), "normal");
        let output = require_render_target(resources, output_handle, self.name(), "output");
        let scene_view = execution.view_payload::<SceneView>().ok_or_else(|| {
            RenderGraphError::ExecutionFailed("contact shadows missing SceneView payload".into())
        })?;
        let settings = execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .contact_shadows;
        let Some(light_direction_world) = Self::primary_directional_light(execution) else {
            return Ok(());
        };
        let view = Mat4::from_cols_array(scene_view.view_matrix);
        let light_direction_view = view
            .transform_vector3(Vec3::from_array(light_direction_world))
            .normalized()
            .to_array();
        let projection = scene_view.unjittered_projection_matrix;
        let params = ContactShadowsParams {
            projection,
            inverse_projection: Mat4::from_cols_array(projection).inverse().to_cols_array(),
            light_direction_view,
            temporal_seed: scene_view.temporal.frame_index as f32,
            intensity: settings.intensity,
            max_distance: settings.max_distance,
            thickness: settings.thickness,
            ray_steps: settings.ray_steps,
            ao_intensity: settings.ao_intensity,
            ao_radius_pixels: settings.ao_radius_pixels,
            ao_steps: settings.ao_steps,
        };

        if should_log_scene_view(scene_view) {
            eprintln!(
                "[contact_shadows][frame={}] input={}x{} output={}x{} settings={:?} light_world={:?} light_view={:?} jitter={:?}",
                scene_view.temporal.frame_index,
                input.width(),
                input.height(),
                output.width(),
                output.height(),
                settings,
                light_direction_world,
                light_direction_view,
                scene_view.temporal.jitter
            );
        }

        let runtime = self
            .runtime
            .get_or_insert_with(|| LowLevelContactShadows::new(gpu, output.format()));
        runtime.apply_to_target(gpu, input, depth, normal, output, params);
        Ok(())
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        if execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .contact_shadows
            .enabled
            && Self::primary_directional_light(execution).is_some()
        {
            1
        } else {
            0
        }
    }
}
