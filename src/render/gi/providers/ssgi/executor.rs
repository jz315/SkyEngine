use super::bindings::{
    color_attachment, create_composite_bind_group, create_compute_deinterleave_output_bind_group,
    create_compute_diffuse_input_bind_group, create_compute_diffuse_output_bind_group,
    create_compute_scene_bind_group, create_compute_upsample_input_bind_group,
    create_compute_upsample_output_bind_group, create_final_bind_group, create_uniform_bind_group,
    SsgiBindGroupLayouts,
};
use super::constants::SSGI_MIP_COUNT;
use super::contract::{
    ssgi_pass_descriptor, SsgiBindingRole, SsgiDispatchRule, SsgiPassDescriptor, SsgiPassKind,
    SsgiResourceRole,
};
use super::debug::should_log_scene_view;
use super::graph::{
    declare_ssgi_graph, graph_resources_blackboard_key, validate_compiled_pass_resources,
    SsgiGraphInputs, SsgiGraphResources, SsgiResolvedPassResources,
};
use super::layout::SsgiResources;
use super::pipelines::SsgiPipelineCache;
use super::settings::SsgiSettings;
use super::uniforms::SsgiUniform;
use crate::gpu::GpuContext;
use crate::render::execution::{PostFxPassExecuteContext, PostFxPassSetupContext};
use crate::render::gpu::FullscreenPass;
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraphError, TextureHandle, TextureSubresource,
};
use crate::render::view::SceneView;

#[derive(Default)]
pub struct SsgiExecutor {
    resources: SsgiResources,
    gpu_state: SsgiGpuState,
}

#[derive(Default)]
pub(crate) struct SsgiGpuState {
    layouts: SsgiBindGroupLayouts,
    pipelines: SsgiPipelineCache,
    uniform_buffers: [Option<wgpu::Buffer>; SSGI_PASS_COUNT],
    uniform_bind_groups: [Option<wgpu::BindGroup>; SSGI_PASS_COUNT],
}

impl SsgiExecutor {
    #[inline]
    pub const fn resources(&self) -> SsgiResources {
        self.resources
    }

    #[inline]
    pub(crate) fn name(&self) -> &'static str {
        "ssgi"
    }

    #[inline]
    pub(crate) fn requires_hdr_input(&self) -> bool {
        true
    }

    pub(crate) fn setup_with_settings(
        &mut self,
        ctx: &mut PostFxPassSetupContext<'_, '_>,
        settings: SsgiSettings,
    ) {
        let Some(current) = ctx.state().current_color() else {
            return;
        };
        let Some(depth) = ctx.state().scene_depth() else {
            return;
        };
        let Some(normal) = ctx.state().scene_normal() else {
            return;
        };
        let Some(velocity) = ctx.state().scene_velocity() else {
            return;
        };

        let target_size = ctx.view().target_size();
        self.resources.resize(target_size[0], target_size[1]);
        if let Some(scene_view) = ctx.view_payload::<SceneView>() {
            if should_log_scene_view(scene_view) {
                eprintln!(
                    "[ssgi][setup][frame={}] target={:?} current={:?} depth={:?} normal={:?} resources={:?} settings={:?} jitter={:?} prev_jitter={:?}",
                    scene_view.temporal.frame_index,
                    target_size,
                    current.format(),
                    depth.format(),
                    normal.format(),
                    self.resources,
                    settings,
                    scene_view.temporal.jitter,
                    scene_view.temporal.previous_jitter
                );
            }
        }

        let graph_resources = declare_ssgi_graph(
            ctx.graph(),
            self.resources,
            SsgiGraphInputs {
                scene_color: current.handle(),
                scene_depth: depth.handle(),
                scene_normal: normal.handle(),
                scene_velocity: velocity.handle(),
                target_size,
                output_format: current.format(),
            },
        );
        ctx.blackboard_set(graph_resources_blackboard_key(), graph_resources);
        ctx.state().set_scene_indirect_diffuse(
            graph_resources.output_indirect_diffuse(),
            current.format(),
        );
        ctx.state()
            .set_current_color(graph_resources.output_scene_color(), current.format());
        ctx.state()
            .set_scene_color(graph_resources.output_scene_color(), current.format());
    }

    pub(crate) fn execute_with_settings(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
        settings: SsgiSettings,
    ) -> Result<(), RenderGraphError> {
        let graph_resources = *ctx
            .blackboard_get::<SsgiGraphResources>(graph_resources_blackboard_key())
            .ok_or_else(|| {
                RenderGraphError::ExecutionFailed(
                    "SSGI graph resources were not published during setup".into(),
                )
            })?;
        let descriptor = ssgi_pass_descriptor(ctx.pass().name.as_ref()).ok_or_else(|| {
            RenderGraphError::ExecutionFailed(format!("unknown SSGI pass `{}`", ctx.pass().name))
        })?;
        validate_required_binding(descriptor, SsgiBindingRole::Uniform)?;
        let (gpu, pass, physical, execution) = ctx.split();
        let resolved = validate_compiled_pass_resources(pass, descriptor, graph_resources)?;
        let scene_view = execution.view_payload::<SceneView>().ok_or_else(|| {
            RenderGraphError::ExecutionFailed("ssgi missing SceneView payload".into())
        })?;

        match descriptor.kind {
            SsgiPassKind::FinalComposite => self.execute_final(
                gpu, pass, physical, settings, scene_view, descriptor, resolved,
            ),
            SsgiPassKind::SceneComposite => self.execute_scene_composite(
                gpu, pass, physical, settings, scene_view, descriptor, resolved,
            ),
            _ => self.execute_compute(
                gpu, pass, physical, settings, scene_view, descriptor, resolved,
            ),
        }
    }

    fn execute_compute(
        &mut self,
        gpu: &mut GpuContext,
        pass: &CompiledPass,
        resources: &PhysicalResources<'_>,
        settings: SsgiSettings,
        scene_view: &SceneView,
        descriptor: &'static SsgiPassDescriptor,
        resolved: SsgiResolvedPassResources,
    ) -> Result<(), RenderGraphError> {
        self.gpu_state.ensure_compute(gpu);
        let uniform = SsgiUniform::for_pass(settings, scene_view, descriptor.kind);
        self.gpu_state
            .write_uniform(gpu, pass_index(descriptor.kind), &uniform);

        let (input_bg, output_bg, dispatch_size) = match descriptor.kind {
            SsgiPassKind::Deinterleave { .. } => {
                let current = require_render_target(
                    resources,
                    resolved.read_texture(SsgiResourceRole::SceneColorInput)?,
                    self.name(),
                    "current color",
                );
                let depth = require_render_target(
                    resources,
                    resolved.read_texture(SsgiResourceRole::SceneDepth)?,
                    self.name(),
                    "scene depth",
                );
                let normal = require_render_target(
                    resources,
                    resolved.read_texture(SsgiResourceRole::SceneNormal)?,
                    self.name(),
                    "scene normal",
                );
                let velocity = require_render_target(
                    resources,
                    resolved.read_texture(SsgiResourceRole::SceneVelocity)?,
                    self.name(),
                    "scene velocity",
                );
                let mip = mip_index(descriptor.kind) as u32;
                let atlas_depth =
                    resolved.write_subresource(SsgiResourceRole::AtlasDepth { mip })?;
                let atlas_color =
                    resolved.write_subresource(SsgiResourceRole::AtlasColor { mip })?;
                let regular_depth =
                    resolved.write_subresource(SsgiResourceRole::DepthMip { mip })?;
                let regular_normal =
                    resolved.write_subresource(SsgiResourceRole::NormalMip { mip })?;
                let atlas_depth_view = resources
                    .storage_texture_view(atlas_depth, wgpu::TextureViewDimension::D2Array);
                let atlas_color_view = resources
                    .storage_texture_view(atlas_color, wgpu::TextureViewDimension::D2Array);
                let regular_depth_view =
                    resources.storage_texture_view(regular_depth, wgpu::TextureViewDimension::D2);
                let regular_normal_view =
                    resources.storage_texture_view(regular_normal, wgpu::TextureViewDimension::D2);
                (
                    create_compute_scene_bind_group(
                        gpu,
                        &self.gpu_state.layouts,
                        current,
                        depth,
                        normal,
                        velocity,
                    ),
                    create_compute_deinterleave_output_bind_group(
                        gpu,
                        &self.gpu_state.layouts,
                        &atlas_depth_view,
                        &atlas_color_view,
                        &regular_depth_view,
                        &regular_normal_view,
                    ),
                    dispatch_size(resources, descriptor, &resolved)?,
                )
            }
            SsgiPassKind::Diffuse { mip_index } => {
                let mip = mip_index as u32;
                let atlas_depth =
                    resolved.read_subresource(SsgiResourceRole::AtlasDepth { mip })?;
                let atlas_color =
                    resolved.read_subresource(SsgiResourceRole::AtlasColor { mip })?;
                let normal = resolved.read_subresource(SsgiResourceRole::NormalMip { mip })?;
                let output = resolved.write_subresource(SsgiResourceRole::DiffuseMip { mip })?;
                let atlas_depth_view = resources
                    .texture_subresource_view(atlas_depth, wgpu::TextureViewDimension::D2Array);
                let atlas_color_view = resources
                    .texture_subresource_view(atlas_color, wgpu::TextureViewDimension::D2Array);
                let normal_view =
                    resources.texture_subresource_view(normal, wgpu::TextureViewDimension::D2);
                let output_view =
                    resources.storage_texture_view(output, wgpu::TextureViewDimension::D2);
                (
                    create_compute_diffuse_input_bind_group(
                        gpu,
                        &self.gpu_state.layouts,
                        &atlas_depth_view,
                        &atlas_color_view,
                        &normal_view,
                    ),
                    create_compute_diffuse_output_bind_group(
                        gpu,
                        &self.gpu_state.layouts,
                        &output_view,
                    ),
                    dispatch_size(resources, descriptor, &resolved)?,
                )
            }
            SsgiPassKind::Upsample {
                source_mip_index,
                target_mip_index,
                ..
            } => {
                let source_mip = source_mip_index as u32;
                let target_mip = target_mip_index as u32;
                let depth_low =
                    resolved.read_subresource(SsgiResourceRole::DepthMip { mip: source_mip })?;
                let normal_low =
                    resolved.read_subresource(SsgiResourceRole::NormalMip { mip: source_mip })?;
                let diffuse_low = resolved.read_subresource(source_diffuse_role(source_mip))?;
                let depth_high =
                    resolved.read_subresource(SsgiResourceRole::DepthMip { mip: target_mip })?;
                let normal_high =
                    resolved.read_subresource(SsgiResourceRole::NormalMip { mip: target_mip })?;
                let diffuse_high =
                    resolved.read_subresource(SsgiResourceRole::DiffuseMip { mip: target_mip })?;
                let output = resolved
                    .write_subresource(SsgiResourceRole::FilteredDiffuseMip { mip: target_mip })?;
                let depth_low_view =
                    resources.texture_subresource_view(depth_low, wgpu::TextureViewDimension::D2);
                let normal_low_view =
                    resources.texture_subresource_view(normal_low, wgpu::TextureViewDimension::D2);
                let diffuse_low_view =
                    resources.texture_subresource_view(diffuse_low, wgpu::TextureViewDimension::D2);
                let depth_high_view =
                    resources.texture_subresource_view(depth_high, wgpu::TextureViewDimension::D2);
                let normal_high_view =
                    resources.texture_subresource_view(normal_high, wgpu::TextureViewDimension::D2);
                let output_view =
                    resources.storage_texture_view(output, wgpu::TextureViewDimension::D2);
                (
                    create_compute_upsample_input_bind_group(
                        gpu,
                        &self.gpu_state.layouts,
                        &depth_low_view,
                        &normal_low_view,
                        &diffuse_low_view,
                        &depth_high_view,
                        &normal_high_view,
                        &resources
                            .texture_subresource_view(diffuse_high, wgpu::TextureViewDimension::D2),
                    ),
                    create_compute_upsample_output_bind_group(
                        gpu,
                        &self.gpu_state.layouts,
                        &output_view,
                    ),
                    dispatch_size(resources, descriptor, &resolved)?,
                )
            }
            SsgiPassKind::FinalComposite | SsgiPassKind::SceneComposite => {
                unreachable!("fullscreen pass is not compute")
            }
        };

        if dispatch_size[0] == 0 || dispatch_size[1] == 0 {
            return Ok(());
        }
        if should_log_scene_view(scene_view) {
            eprintln!(
                "[ssgi][compute][frame={}] pass={} kind={:?} dispatch={:?} settings={:?} uniform.params0={:?} params1={:?} params2={:?}",
                scene_view.temporal.frame_index,
                pass.name,
                descriptor.kind,
                dispatch_size,
                settings,
                uniform.params0,
                uniform.params1,
                uniform.params2
            );
        }

        let pipeline = self
            .gpu_state
            .pipelines
            .compute_pipeline(gpu, descriptor.shader);
        let uniform_bg = self
            .gpu_state
            .uniform_bind_group(pass_index(descriptor.kind));
        let mut frame = gpu.frame();
        let mut compute = frame.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(pass.name.as_ref()),
            ..Default::default()
        });
        compute.set_pipeline(&pipeline);
        compute.set_bind_group(0, &input_bg, &[]);
        compute.set_bind_group(1, uniform_bg, &[]);
        compute.set_bind_group(2, &output_bg, &[]);
        compute.dispatch_workgroups(
            dispatch_size[0].div_ceil(8),
            dispatch_size[1].div_ceil(8),
            1,
        );
        Ok(())
    }

    fn execute_final(
        &mut self,
        gpu: &mut GpuContext,
        _pass: &CompiledPass,
        resources: &PhysicalResources<'_>,
        settings: SsgiSettings,
        scene_view: &SceneView,
        descriptor: &'static SsgiPassDescriptor,
        resolved: SsgiResolvedPassResources,
    ) -> Result<(), RenderGraphError> {
        let output_handle = resolved.write_texture(SsgiResourceRole::FinalIndirectDiffuse)?;
        let output = require_render_target(resources, output_handle, self.name(), "output");
        self.gpu_state.ensure_final(gpu, output.format());
        let uniform = SsgiUniform::for_pass(settings, scene_view, descriptor.kind);
        self.gpu_state
            .write_uniform(gpu, pass_index(descriptor.kind), &uniform);

        let low_depth = resolved.read_subresource(SsgiResourceRole::DepthMip { mip: 0 })?;
        let low_normal = resolved.read_subresource(SsgiResourceRole::NormalMip { mip: 0 })?;
        let low_diffuse =
            resolved.read_subresource(SsgiResourceRole::FilteredDiffuseMip { mip: 0 })?;
        let scene_depth_handle = resolved.read_texture(SsgiResourceRole::SceneDepth)?;
        let scene_normal_handle = resolved.read_texture(SsgiResourceRole::SceneNormal)?;
        let scene_color_handle = resolved.read_texture(SsgiResourceRole::SceneColorInput)?;
        let low_depth_view =
            resources.texture_subresource_view(low_depth, wgpu::TextureViewDimension::D2);
        let low_normal_view =
            resources.texture_subresource_view(low_normal, wgpu::TextureViewDimension::D2);
        let low_diffuse_view =
            resources.texture_subresource_view(low_diffuse, wgpu::TextureViewDimension::D2);
        let scene_depth =
            require_render_target(resources, scene_depth_handle, self.name(), "scene depth");
        let scene_normal =
            require_render_target(resources, scene_normal_handle, self.name(), "scene normal");
        let scene_color =
            require_render_target(resources, scene_color_handle, self.name(), "scene color");
        let texture_bg = create_final_bind_group(
            gpu,
            &self.gpu_state.layouts,
            &low_depth_view,
            &low_normal_view,
            &low_diffuse_view,
            scene_depth.view(),
            scene_normal.view(),
            scene_color.view(),
        );

        if should_log_scene_view(scene_view) {
            eprintln!(
                "[ssgi][final][frame={}] output={}x{} format={:?} scene_depth={:?} scene_normal={:?} settings={:?} uniform.params0={:?} params1={:?} params2={:?}",
                scene_view.temporal.frame_index,
                output.width(),
                output.height(),
                output.format(),
                scene_depth.format(),
                scene_normal.format(),
                settings,
                uniform.params0,
                uniform.params1,
                uniform.params2
            );
        }

        let pipeline =
            self.gpu_state
                .pipelines
                .fullscreen_pipeline(gpu, descriptor.shader, output.format());
        let uniform_bg = self
            .gpu_state
            .uniform_bind_group(pass_index(descriptor.kind));
        let color_attachments = vec![Some(color_attachment(output))];
        let mut frame = gpu.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline.as_ref());
        pass.set_bind_group(0, &texture_bg, &[]);
        pass.set_bind_group(1, uniform_bg, &[]);
        FullscreenPass::draw(&mut pass);
        Ok(())
    }

    fn execute_scene_composite(
        &mut self,
        gpu: &mut GpuContext,
        _pass: &CompiledPass,
        resources: &PhysicalResources<'_>,
        settings: SsgiSettings,
        scene_view: &SceneView,
        descriptor: &'static SsgiPassDescriptor,
        resolved: SsgiResolvedPassResources,
    ) -> Result<(), RenderGraphError> {
        let output_handle = resolved.write_texture(SsgiResourceRole::OutputSceneColor)?;
        let output = require_render_target(resources, output_handle, self.name(), "output");
        self.gpu_state.ensure_final(gpu, output.format());
        let uniform = SsgiUniform::for_pass(settings, scene_view, descriptor.kind);
        self.gpu_state
            .write_uniform(gpu, pass_index(descriptor.kind), &uniform);

        let indirect_handle = resolved.read_texture(SsgiResourceRole::FinalIndirectDiffuse)?;
        let scene_color_handle = resolved.read_texture(SsgiResourceRole::SceneColorInput)?;
        let indirect =
            require_render_target(resources, indirect_handle, self.name(), "indirect diffuse");
        let scene_color =
            require_render_target(resources, scene_color_handle, self.name(), "scene color");
        let texture_bg = create_composite_bind_group(
            gpu,
            &self.gpu_state.layouts,
            indirect.view(),
            scene_color.view(),
        );

        let pipeline =
            self.gpu_state
                .pipelines
                .fullscreen_pipeline(gpu, descriptor.shader, output.format());
        let uniform_bg = self
            .gpu_state
            .uniform_bind_group(pass_index(descriptor.kind));
        let color_attachments = vec![Some(color_attachment(output))];
        let mut frame = gpu.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline.as_ref());
        pass.set_bind_group(0, &texture_bg, &[]);
        pass.set_bind_group(1, uniform_bg, &[]);
        FullscreenPass::draw(&mut pass);
        Ok(())
    }
}

impl SsgiGpuState {
    fn ensure_compute(&mut self, gpu: &GpuContext) {
        self.layouts.ensure_compute(gpu);
        self.ensure_uniform_buffer(gpu);
        self.pipelines.ensure_compute(gpu, &self.layouts);
    }

    fn ensure_final(&mut self, gpu: &GpuContext, target_format: wgpu::TextureFormat) {
        self.layouts.ensure_final(gpu);
        self.ensure_uniform_buffer(gpu);
        self.pipelines
            .ensure_final(gpu, &self.layouts, target_format);
    }

    fn ensure_uniform_buffer(&mut self, gpu: &GpuContext) {
        for index in 0..SSGI_PASS_COUNT {
            if self.uniform_buffers[index].is_none() {
                let buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
                    label: Some("ssgi_uniform"),
                    size: std::mem::size_of::<SsgiUniform>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bind_group = create_uniform_bind_group(gpu, &self.layouts, &buffer);
                self.uniform_buffers[index] = Some(buffer);
                self.uniform_bind_groups[index] = Some(bind_group);
            }
        }
    }

    fn uniform_buffer(&self, index: usize) -> &wgpu::Buffer {
        self.uniform_buffers[index]
            .as_ref()
            .expect("SSGI uniform buffer should exist")
    }

    fn write_uniform(&self, gpu: &GpuContext, index: usize, uniform: &SsgiUniform) {
        gpu.queue()
            .write_buffer(self.uniform_buffer(index), 0, bytemuck::bytes_of(uniform));
    }

    fn uniform_bind_group(&self, index: usize) -> &wgpu::BindGroup {
        self.uniform_bind_groups[index]
            .as_ref()
            .expect("SSGI uniform bind group should exist")
    }
}

const SSGI_PASS_COUNT: usize = 13;

fn pass_index(kind: SsgiPassKind) -> usize {
    match kind {
        SsgiPassKind::Deinterleave { mip_index } => mip_index,
        SsgiPassKind::Diffuse { mip_index } => 4 + mip_index,
        SsgiPassKind::Upsample { pass_index, .. } => 8 + pass_index,
        SsgiPassKind::FinalComposite => 11,
        SsgiPassKind::SceneComposite => 12,
    }
}

fn mip_index(kind: SsgiPassKind) -> usize {
    match kind {
        SsgiPassKind::Deinterleave { mip_index } | SsgiPassKind::Diffuse { mip_index } => mip_index,
        SsgiPassKind::Upsample {
            target_mip_index, ..
        } => target_mip_index,
        SsgiPassKind::FinalComposite | SsgiPassKind::SceneComposite => 0,
    }
}

fn source_diffuse_role(source_mip: u32) -> SsgiResourceRole {
    if source_mip as usize == SSGI_MIP_COUNT - 1 {
        SsgiResourceRole::DiffuseMip { mip: source_mip }
    } else {
        SsgiResourceRole::FilteredDiffuseMip { mip: source_mip }
    }
}

fn dispatch_size(
    physical: &PhysicalResources<'_>,
    descriptor: &SsgiPassDescriptor,
    resolved: &SsgiResolvedPassResources,
) -> Result<[u32; 2], RenderGraphError> {
    match descriptor.dispatch {
        SsgiDispatchRule::WorkgroupsForWrite(role) => {
            let subresource = resolved.write_subresource(role)?;
            Ok(subresource_extent(physical, subresource))
        }
        SsgiDispatchRule::Fullscreen => Err(RenderGraphError::ExecutionFailed(format!(
            "SSGI pass `{}` uses fullscreen dispatch in compute path",
            descriptor.name
        ))),
    }
}

fn subresource_extent(
    resources: &PhysicalResources<'_>,
    subresource: TextureSubresource,
) -> [u32; 2] {
    let texture = resources.texture_ref(subresource.texture);
    let divisor = 1u32
        .checked_shl(subresource.base_mip_level)
        .unwrap_or(u32::MAX);
    [
        texture.size[0].div_ceil(divisor).max(1),
        texture.size[1].div_ceil(divisor).max(1),
    ]
}

fn require_render_target<'a>(
    resources: &'a PhysicalResources<'a>,
    handle: TextureHandle,
    node_name: &str,
    label: &str,
) -> &'a crate::render::gpu::RenderTarget {
    resources
        .render_target(handle)
        .unwrap_or_else(|| panic!("{node_name} {label} target should be allocated"))
}

fn validate_required_binding(
    descriptor: &SsgiPassDescriptor,
    role: SsgiBindingRole,
) -> Result<(), RenderGraphError> {
    if descriptor.bindings.contains(&role) {
        Ok(())
    } else {
        Err(RenderGraphError::ExecutionFailed(format!(
            "SSGI pass `{}` is missing required binding role {role:?}",
            descriptor.name
        )))
    }
}
