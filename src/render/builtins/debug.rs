use crate::render::component::{RenderDebugView, RenderSettings};
use crate::render::execution::{
    pass_first_write_texture, pass_nth_read_texture, PostFxPassExecuteContext,
    PostFxPassSetupContext, PreparedFrame, PreparedView, SceneTexture, ViewExecutionContext,
};
use crate::render::graph::{CompiledPass, RenderGraphError, ResourceRef, TextureHandle};
use crate::render::lighting::shadow::ShadowDebugResources;
use crate::render::pipeline::PostFxPass;
use crate::render::postfx::debug_view::{
    DebugView as LowLevelDebugView, DebugViewMode as LowLevelDebugViewMode,
    DebugViewParams as LowLevelDebugViewParams,
};
use crate::render::view::SceneView;

#[derive(Clone, Copy)]
enum DebugViewSource {
    Texture(TextureHandle),
}

impl DebugViewSource {
    #[inline]
    fn resource(self) -> ResourceRef {
        match self {
            Self::Texture(handle) => ResourceRef::Texture(handle),
        }
    }
}

#[derive(Default)]
pub struct DebugView {
    runtime: Option<LowLevelDebugView>,
}

impl PostFxPass for DebugView {
    fn name(&self) -> &'static str {
        "debug_view"
    }

    fn is_enabled(&self, frame: &PreparedFrame<'_>, view: &PreparedView<'_>) -> bool {
        if view
            .payload::<SceneView>()
            .is_some_and(SceneView::is_shadow)
        {
            return false;
        }
        let debug_view = frame
            .payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .debug_view;
        debug_view.is_enabled() && !debug_view_is_material_shadow(debug_view)
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        let debug_view = ctx
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .debug_view;
        if !debug_view.is_enabled() || debug_view_is_material_shadow(debug_view) {
            return;
        }

        let Some(current) = ctx.state().current_color() else {
            return;
        };
        let depth = match debug_view {
            RenderDebugView::DirectionalShadowMap
            | RenderDebugView::DirectionalShadowCascade(_) => {
                let Some(shadow_debug) = ctx.frame_payload::<ShadowDebugResources>() else {
                    return;
                };
                let imported = shadow_debug.directional_shadow_atlas();
                ctx.graph().create_texture(|builder| {
                    builder
                        .name("debug_directional_shadow_map")
                        .import_external(imported);
                })
            }
            _ => {
                let Some(depth) = ctx.state().scene_depth() else {
                    return;
                };
                depth.handle()
            }
        };
        let Some(source) = debug_view_source(ctx.state(), debug_view, current) else {
            return;
        };

        let target_size = ctx.view().target_size();
        let output = ctx.graph().create_texture(|builder| {
            builder
                .name("debug_view_out")
                .size(crate::render::graph::TargetSize::Exact(
                    target_size[0],
                    target_size[1],
                ))
                .format(current.format());
        });

        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(depth);
            match source.resource() {
                ResourceRef::Texture(handle) => setup.read(handle),
                ResourceRef::TextureSubresource(subresource) => setup.read_subresource(subresource),
                ResourceRef::Surface | ResourceRef::Buffer(_) => unreachable!(),
            }
            setup.write_color(0, output);
        });
        ctx.state().set_current_color(output, current.format());
        ctx.state().set_scene_color(output, current.format());
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        let (gpu, pass, resources, execution) = ctx.split();
        let debug_view = execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .debug_view;
        let mode = debug_view_mode(debug_view).ok_or_else(|| {
            RenderGraphError::ExecutionFailed("debug_view missing active mode".into())
        })?;
        let params = debug_view_params(execution, debug_view);
        let depth_handle = pass_nth_read_texture(pass, 0, self.name(), "scene depth");
        let source_ref = pass_nth_read_texture_resource(pass, 1, self.name(), "source");
        let output_handle = pass_first_write_texture(pass, self.name(), "output");
        let output = crate::render::execution::require_render_target(
            resources,
            output_handle,
            self.name(),
            "output",
        );
        let depth = resources.texture_view(depth_handle);
        let subresource_view;
        let source = match source_ref {
            ResourceRef::Texture(handle) => resources.texture_view(handle),
            ResourceRef::TextureSubresource(subresource) => {
                subresource_view =
                    resources.texture_subresource_view(subresource, wgpu::TextureViewDimension::D2);
                &subresource_view
            }
            ResourceRef::Surface | ResourceRef::Buffer(_) => {
                return Err(RenderGraphError::ExecutionFailed(
                    "debug_view source must be a texture".into(),
                ));
            }
        };
        let runtime = self
            .runtime
            .get_or_insert_with(|| LowLevelDebugView::new(gpu, output.format()));
        runtime.apply_to_target(gpu, depth, source, output, mode, params);
        Ok(())
    }

    fn draw_calls(&self, execution: &ViewExecutionContext<'_>) -> usize {
        let debug_view = execution
            .frame_payload::<RenderSettings>()
            .cloned()
            .unwrap_or_default()
            .debug_view;
        if debug_view.is_enabled() && !debug_view_is_material_shadow(debug_view) {
            1
        } else {
            0
        }
    }
}

fn debug_view_is_material_shadow(debug_view: RenderDebugView) -> bool {
    matches!(
        debug_view,
        RenderDebugView::DirectionalShadowCoverage
            | RenderDebugView::DirectionalShadowSplitCoverage
            | RenderDebugView::DirectionalShadowFade
            | RenderDebugView::DirectionalShadowCompareDelta
            | RenderDebugView::DirectionalShadowBias
            | RenderDebugView::DirectionalShadowPcss
            | RenderDebugView::DirectLighting
            | RenderDebugView::IndirectLighting
    )
}

fn debug_view_source(
    state: &crate::render::execution::PhaseState,
    debug_view: RenderDebugView,
    current_color: crate::render::execution::TextureSlot,
) -> Option<DebugViewSource> {
    let slot = |texture: SceneTexture| {
        state
            .scene_texture(texture)
            .map(|slot| DebugViewSource::Texture(slot.handle()))
    };

    match debug_view {
        RenderDebugView::None => None,
        RenderDebugView::SceneColor => Some(DebugViewSource::Texture(current_color.handle())),
        RenderDebugView::SceneDepth => Some(DebugViewSource::Texture(current_color.handle())),
        RenderDebugView::SceneNormal => slot(SceneTexture::Normal),
        RenderDebugView::Albedo => slot(SceneTexture::Albedo),
        RenderDebugView::Roughness => slot(SceneTexture::Material),
        RenderDebugView::Metallic => slot(SceneTexture::Material),
        RenderDebugView::Emissive => slot(SceneTexture::Emissive),
        RenderDebugView::Velocity => slot(SceneTexture::Velocity),
        RenderDebugView::Light => slot(SceneTexture::Light),
        RenderDebugView::IndirectDiffuse => slot(SceneTexture::IndirectDiffuse),
        RenderDebugView::DirectionalShadowMap | RenderDebugView::DirectionalShadowCascade(_) => {
            Some(DebugViewSource::Texture(current_color.handle()))
        }
        RenderDebugView::DirectionalShadowCoverage
        | RenderDebugView::DirectionalShadowSplitCoverage
        | RenderDebugView::DirectionalShadowFade
        | RenderDebugView::DirectionalShadowCompareDelta
        | RenderDebugView::DirectionalShadowBias
        | RenderDebugView::DirectionalShadowPcss
        | RenderDebugView::DirectLighting
        | RenderDebugView::IndirectLighting => {
            Some(DebugViewSource::Texture(current_color.handle()))
        }
    }
}

fn debug_view_mode(debug_view: RenderDebugView) -> Option<LowLevelDebugViewMode> {
    match debug_view {
        RenderDebugView::None => None,
        RenderDebugView::SceneDepth => Some(LowLevelDebugViewMode::SceneDepth),
        RenderDebugView::DirectionalShadowMap | RenderDebugView::DirectionalShadowCascade(_) => {
            Some(LowLevelDebugViewMode::ShadowDepth)
        }
        RenderDebugView::DirectionalShadowCoverage
        | RenderDebugView::DirectionalShadowSplitCoverage
        | RenderDebugView::DirectionalShadowFade
        | RenderDebugView::DirectionalShadowCompareDelta
        | RenderDebugView::DirectionalShadowBias
        | RenderDebugView::DirectionalShadowPcss
        | RenderDebugView::DirectLighting
        | RenderDebugView::IndirectLighting => None,
        RenderDebugView::SceneNormal => Some(LowLevelDebugViewMode::SceneNormal),
        RenderDebugView::Roughness => Some(LowLevelDebugViewMode::Roughness),
        RenderDebugView::Metallic => Some(LowLevelDebugViewMode::Metallic),
        RenderDebugView::Velocity => Some(LowLevelDebugViewMode::Velocity),
        RenderDebugView::SceneColor
        | RenderDebugView::Albedo
        | RenderDebugView::Emissive
        | RenderDebugView::Light
        | RenderDebugView::IndirectDiffuse => Some(LowLevelDebugViewMode::SourceRgb),
    }
}

fn debug_view_params(
    execution: &ViewExecutionContext<'_>,
    debug_view: RenderDebugView,
) -> LowLevelDebugViewParams {
    match debug_view {
        RenderDebugView::DirectionalShadowCascade(cascade) => execution
            .frame_payload::<ShadowDebugResources>()
            .map(|resources| {
                LowLevelDebugViewParams::atlas_slice_with_mul_add(
                    cascade,
                    resources.directional_cascade_count(),
                    resources.directional_shadow_mul_add(),
                )
            })
            .unwrap_or_else(LowLevelDebugViewParams::full),
        _ => LowLevelDebugViewParams::full(),
    }
}

fn pass_nth_read_texture_resource(
    pass: &CompiledPass,
    index: usize,
    node_name: &str,
    label: &str,
) -> ResourceRef {
    pass.reads
        .iter()
        .filter_map(|resource| match resource {
            ResourceRef::Texture(_) | ResourceRef::TextureSubresource(_) => Some(*resource),
            ResourceRef::Surface | ResourceRef::Buffer(_) => None,
        })
        .nth(index)
        .unwrap_or_else(|| panic!("{node_name} should read {label} texture"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::execution::{PhaseState, TextureFormat};
    use crate::render::gpu::RenderTarget;
    use crate::render::graph::{RenderGraph, TargetSize};

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for built-in debug tests");

        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("builtin_debug_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn directional_shadow_debug_view_imports_shadow_depth_without_scene_depth() {
        let (device, queue) = create_test_device();
        let gpu = crate::gpu::GpuContext::new_headless(
            device,
            queue,
            TextureFormat::Bgra8Unorm,
            [16, 16],
        );
        let shadow_target = RenderTarget::new_depth(&gpu, 16, 16);
        let shadow_debug = ShadowDebugResources::from_directional_map(&shadow_target);
        let settings = RenderSettings {
            debug_view: RenderDebugView::DirectionalShadowMap,
            ..RenderSettings::default()
        };
        let mut frame = PreparedFrame::new(TextureFormat::Bgra8Unorm, false);
        let _ = frame.insert_payload(&settings);
        let _ = frame.insert_payload(&shadow_debug);
        let view = PreparedView::new(
            0,
            crate::render::ViewportRect::from_surface_size([16, 16]),
            [16, 16],
            false,
        );
        let mut graph = RenderGraph::new();
        let mut state = PhaseState::new(TextureFormat::Bgra8Unorm, false);
        let current = graph.create_texture(|builder| {
            builder
                .name("debug_shadow_current")
                .size(TargetSize::Exact(16, 16))
                .format(crate::render::view::SCENE_HDR_FORMAT);
        });
        state.set_current_color(current, crate::render::view::SCENE_HDR_FORMAT);
        let mut debug_view = DebugView::default();

        {
            let mut ctx = PostFxPassSetupContext::new(&mut graph, &mut state, &frame, &view);
            debug_view.setup(&mut ctx);
        }

        assert!(graph.get_texture("debug_directional_shadow_map").is_some());
        assert_eq!(graph.pass_count(), 1);
        assert_ne!(
            state
                .current_color()
                .expect("debug view should replace current color")
                .handle(),
            current
        );
        assert_eq!(
            debug_view_mode(RenderDebugView::DirectionalShadowMap),
            Some(LowLevelDebugViewMode::ShadowDepth)
        );
        assert_eq!(
            debug_view_mode(RenderDebugView::DirectionalShadowCascade(2)),
            Some(LowLevelDebugViewMode::ShadowDepth)
        );
        assert_eq!(
            debug_view_mode(RenderDebugView::DirectionalShadowCoverage),
            None
        );
        assert_eq!(
            debug_view_mode(RenderDebugView::DirectionalShadowCompareDelta),
            None
        );
        assert!(debug_view_is_material_shadow(
            RenderDebugView::DirectionalShadowBias
        ));
        assert!(debug_view_is_material_shadow(
            RenderDebugView::DirectionalShadowPcss
        ));
        assert!(debug_view_is_material_shadow(
            RenderDebugView::DirectLighting
        ));
        assert!(debug_view_is_material_shadow(
            RenderDebugView::IndirectLighting
        ));
    }
}
