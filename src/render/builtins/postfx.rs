use crate::render::builtins::logging::should_log_scene_view;
use crate::render::component::RenderSettings;
use crate::render::execution::{
    pass_first_read_texture, pass_first_write_texture, pass_nth_read_texture,
    require_render_target, PostFxPassExecuteContext, PostFxPassSetupContext, PreparedFrame,
    PreparedView, ViewExecutionContext,
};
use crate::render::graph::{CompiledPass, PassFlags, RenderGraphError, ResourceRef, TextureHandle};
use crate::render::pipeline::PostFxPass;
use crate::render::postfx::bloom::{Bloom as LowLevelBloom, DRAW_CALLS_PER_APPLY};
use crate::render::postfx::sharpen::Sharpen as LowLevelSharpen;
use crate::render::postfx::taa::{
    TemporalAntiAliasing as LowLevelTemporalAntiAliasing, TemporalAntiAliasingParams,
};
use crate::render::postfx::tonemap::ToneMap as LowLevelToneMap;
use crate::render::postfx::vignette::Vignette as LowLevelVignette;
use crate::render::view::{SceneView, SCENE_HDR_FORMAT};

#[derive(Default)]
pub struct Bloom {
    runtime: Option<LowLevelBloom>,
}

impl PostFxPass for Bloom {
    fn name(&self) -> &'static str {
        "bloom"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>, _view: &PreparedView<'_>) -> bool {
        frame
            .payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .bloom
            .enabled
    }

    fn requires_hdr_input(&self) -> bool {
        true
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        let settings = ctx
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default();
        if !settings.bloom.enabled {
            return;
        }

        let input = ctx
            .state()
            .current_color()
            .unwrap_or_else(|| panic!("{} requires current color input", self.name()));
        let target_size = ctx.view().target_size();
        let bloom_out = ctx.graph().create_texture(|builder| {
            builder
                .name("bloom_out")
                .size(crate::render::graph::TargetSize::Exact(
                    target_size[0],
                    target_size[1],
                ))
                .format(SCENE_HDR_FORMAT);
        });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(input.handle());
            setup.write_color(0, bloom_out);
        });
        ctx.state().set_current_color(bloom_out, SCENE_HDR_FORMAT);
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        let (gpu, pass, resources, execution) = ctx.split();
        let input_handle = pass_first_read_texture(pass, self.name(), "input");
        let output_handle = pass_first_write_texture(pass, self.name(), "output");
        let input_rt = require_render_target(resources, input_handle, self.name(), "input");
        let output_rt = require_render_target(resources, output_handle, self.name(), "output");
        let settings = execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .bloom;
        let runtime = self.runtime.get_or_insert_with(|| {
            LowLevelBloom::new(gpu, input_rt.width(), input_rt.height(), output_rt.format())
        });
        runtime.intensity = settings.intensity;
        runtime.spread = settings.spread;
        runtime.resize(gpu, input_rt.width(), input_rt.height(), output_rt.format());
        runtime.apply(gpu, input_rt, output_rt);
        Ok(())
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        if execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .bloom
            .enabled
        {
            DRAW_CALLS_PER_APPLY
        } else {
            0
        }
    }
}

#[derive(Default)]
pub struct ToneMap {
    runtime: Option<LowLevelToneMap>,
}

impl PostFxPass for ToneMap {
    fn name(&self) -> &'static str {
        "tonemap"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>, _view: &PreparedView<'_>) -> bool {
        frame
            .payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .tonemap
            .enabled
    }

    fn requires_hdr_input(&self) -> bool {
        true
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        let settings = ctx
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default();
        if !settings.tonemap.enabled {
            return;
        }

        let input = ctx
            .state()
            .current_color()
            .unwrap_or_else(|| panic!("{} requires current color input", self.name()));
        let target_size = ctx.view().target_size();
        let format = ctx.state().surface_format();
        let output = ctx.graph().create_texture(|builder| {
            builder
                .name("tonemap_out")
                .size(crate::render::graph::TargetSize::Exact(
                    target_size[0],
                    target_size[1],
                ))
                .format(format);
        });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(input.handle());
            setup.write_color(0, output);
        });
        ctx.state().set_current_color(output, format);
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        let (gpu, pass, resources, execution) = ctx.split();
        let input_handle = pass_first_read_texture(pass, self.name(), "input");
        let input_rt = require_render_target(resources, input_handle, self.name(), "input");
        let output_handle = pass_first_write_texture(pass, self.name(), "output");
        let output_rt = require_render_target(resources, output_handle, self.name(), "output");
        let settings = execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .tonemap;
        let runtime = self
            .runtime
            .get_or_insert_with(|| LowLevelToneMap::new(gpu, output_rt.format()));
        runtime.exposure = settings.exposure;
        runtime.gamma = settings.gamma.max(0.001);
        runtime.apply_to_target(gpu, input_rt, output_rt);
        Ok(())
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        if execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .tonemap
            .enabled
        {
            1
        } else {
            0
        }
    }
}

#[derive(Default)]
pub struct TemporalAntiAliasing {
    runtime: Option<LowLevelTemporalAntiAliasing>,
}

impl PostFxPass for TemporalAntiAliasing {
    fn name(&self) -> &'static str {
        "taa"
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
            .temporal_aa
            .enabled
    }

    fn requires_hdr_input(&self) -> bool {
        true
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        let settings = ctx
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default();
        if !settings.temporal_aa.enabled {
            return;
        }

        let Some(input) = ctx.state().current_color() else {
            return;
        };
        let Some(depth) = ctx.state().scene_depth() else {
            return;
        };
        let Some(velocity) = ctx.state().scene_velocity() else {
            return;
        };
        if input.format() != SCENE_HDR_FORMAT {
            return;
        }

        let target_size = ctx.view().target_size();
        let history_color = ctx
            .history_texture("taa_color")
            .format(input.format())
            .ping_pong()
            .get();
        let history_depth = ctx
            .history_texture("taa_depth")
            .format(depth.format())
            .ping_pong()
            .get();
        let output = ctx.graph().create_texture(|builder| {
            builder
                .name("taa_out")
                .size(crate::render::graph::TargetSize::Exact(
                    target_size[0],
                    target_size[1],
                ))
                .format(input.format())
                .storage_binding();
        });

        ctx.graph().add_compute_pass(self.name(), |setup| {
            setup.read(input.handle());
            setup.read(depth.handle());
            setup.read(velocity.handle());
            if let Some(history) = history_color.read() {
                setup.read(history);
            }
            if let Some(history) = history_depth.read() {
                setup.read(history);
            }
            setup.write(output);
            setup.with_flags(PassFlags::PREFER_ASYNC_COMPUTE | PassFlags::BANDWIDTH_INTENSIVE);
        });
        ctx.graph()
            .add_copy_pass("taa_history_color_copy", |setup| {
                setup.texture_to_texture(output, history_color.write());
            });
        ctx.graph()
            .add_copy_pass("taa_history_depth_copy", |setup| {
                setup.texture_to_texture(depth.handle(), history_depth.write());
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
        let velocity_handle = pass_nth_read_texture(pass, 2, self.name(), "velocity");
        let history_handle = pass_nth_read_texture_optional(pass, 3).unwrap_or(input_handle);
        let depth_history_handle = pass_nth_read_texture_optional(pass, 4).unwrap_or(depth_handle);
        let output_handle = pass_first_write_texture(pass, self.name(), "output");
        let input = require_render_target(resources, input_handle, self.name(), "input");
        let depth = require_render_target(resources, depth_handle, self.name(), "depth");
        let velocity = require_render_target(resources, velocity_handle, self.name(), "velocity");
        let output = require_render_target(resources, output_handle, self.name(), "output");
        let history = resources.texture_view(history_handle);
        let depth_history = resources.texture_view(depth_history_handle);
        let scene_view = execution.view_payload::<SceneView>().ok_or_else(|| {
            RenderGraphError::ExecutionFailed("taa missing SceneView payload".into())
        })?;
        let settings = execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .temporal_aa;
        let params = TemporalAntiAliasingParams {
            reset: scene_view.temporal.history_reset
                || pass_nth_read_texture_optional(pass, 3).is_none(),
            feedback: settings.feedback,
            history_clamp: settings.history_clamp,
            jitter: scene_view.temporal.jitter,
            previous_jitter: scene_view.temporal.previous_jitter,
            near: scene_view.near,
            far: scene_view.far,
        };
        if should_log_scene_view(scene_view) {
            eprintln!(
                "[taa][frame={}] input={}x{} output={}x{} settings={:?} reset={} jitter={:?} prev_jitter={:?} near_far=({:.3},{:.3}) history_present={} depth_history_present={}",
                scene_view.temporal.frame_index,
                input.width(),
                input.height(),
                output.width(),
                output.height(),
                settings,
                params.reset,
                params.jitter,
                params.previous_jitter,
                params.near,
                params.far,
                pass_nth_read_texture_optional(pass, 3).is_some(),
                pass_nth_read_texture_optional(pass, 4).is_some()
            );
        }
        let runtime = self
            .runtime
            .get_or_insert_with(|| LowLevelTemporalAntiAliasing::new(gpu));
        runtime.apply_to_target(
            gpu,
            input,
            history,
            depth,
            depth_history,
            velocity,
            output,
            params,
        );
        Ok(())
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        if execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .temporal_aa
            .enabled
        {
            1
        } else {
            0
        }
    }
}

#[derive(Default)]
pub struct Sharpen {
    runtime: Option<LowLevelSharpen>,
}

impl PostFxPass for Sharpen {
    fn name(&self) -> &'static str {
        "sharpen"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>, _view: &PreparedView<'_>) -> bool {
        frame
            .payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .sharpen
            .enabled
    }

    fn requires_hdr_input(&self) -> bool {
        true
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        let settings = ctx
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default();
        if !settings.sharpen.enabled {
            return;
        }

        let input = ctx
            .state()
            .current_color()
            .unwrap_or_else(|| panic!("{} requires current color input", self.name()));
        let target_size = ctx.view().target_size();
        let output = ctx.graph().create_texture(|builder| {
            builder
                .name("sharpen_out")
                .size(crate::render::graph::TargetSize::Exact(
                    target_size[0],
                    target_size[1],
                ))
                .format(input.format());
        });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(input.handle());
            setup.write_color(0, output);
        });
        ctx.state().set_current_color(output, input.format());
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        let (gpu, pass, resources, execution) = ctx.split();
        let input_handle = pass_first_read_texture(pass, self.name(), "input");
        let output_handle = pass_first_write_texture(pass, self.name(), "output");
        let input_rt = require_render_target(resources, input_handle, self.name(), "input");
        let output_rt = require_render_target(resources, output_handle, self.name(), "output");
        let render_settings = execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default();
        let settings = render_settings.sharpen;
        let runtime = self
            .runtime
            .get_or_insert_with(|| LowLevelSharpen::new(gpu, output_rt.format()));
        let taa_sharpen = if render_settings.temporal_aa.enabled {
            render_settings.temporal_aa.sharpen_amount.max(0.0)
        } else {
            0.0
        };
        runtime.strength = (settings.strength + taa_sharpen).max(0.0);
        runtime.clamp = settings.clamp.max(0.0);
        runtime.apply_to_target(gpu, input_rt, output_rt);
        Ok(())
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        if execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .sharpen
            .enabled
        {
            1
        } else {
            0
        }
    }
}

#[derive(Default)]
pub struct Vignette {
    runtime: Option<LowLevelVignette>,
}

impl PostFxPass for Vignette {
    fn name(&self) -> &'static str {
        "vignette"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>, _view: &PreparedView<'_>) -> bool {
        frame
            .payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .vignette
            .enabled
    }

    fn requires_hdr_input(&self) -> bool {
        true
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        let settings = ctx
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default();
        if !settings.vignette.enabled {
            return;
        }

        let input = ctx
            .state()
            .current_color()
            .unwrap_or_else(|| panic!("{} requires current color input", self.name()));
        let target_size = ctx.view().target_size();
        let vignette_out = ctx.graph().create_texture(|builder| {
            builder
                .name("vignette_out")
                .size(crate::render::graph::TargetSize::Exact(
                    target_size[0],
                    target_size[1],
                ))
                .format(SCENE_HDR_FORMAT);
        });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(input.handle());
            setup.write_color(0, vignette_out);
        });
        ctx.state()
            .set_current_color(vignette_out, SCENE_HDR_FORMAT);
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        let (gpu, pass, resources, execution) = ctx.split();
        let input_handle = pass_first_read_texture(pass, self.name(), "input");
        let output_handle = pass_first_write_texture(pass, self.name(), "output");
        let input_rt = require_render_target(resources, input_handle, self.name(), "input");
        let output_rt = require_render_target(resources, output_handle, self.name(), "output");
        let settings = execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .vignette;
        let runtime = self
            .runtime
            .get_or_insert_with(|| LowLevelVignette::new(gpu, output_rt.format()));
        runtime.intensity = settings.intensity;
        runtime.smoothness = settings.smoothness;
        runtime.apply_to_target(gpu, input_rt, output_rt);
        Ok(())
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        if execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .vignette
            .enabled
        {
            1
        } else {
            0
        }
    }
}

fn pass_nth_read_texture_optional(pass: &CompiledPass, index: usize) -> Option<TextureHandle> {
    pass.reads
        .iter()
        .filter_map(|resource| match resource {
            ResourceRef::Texture(handle) => Some(*handle),
            _ => None,
        })
        .nth(index)
}
