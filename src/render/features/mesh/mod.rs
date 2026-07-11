//! Custom mesh rendering pass built on top of [`MaterialPipelineCache`].

mod draw;
mod errors;
mod prepare;
mod prepass;
mod record;
#[cfg(test)]
mod tests;

use crate::gpu::GpuContext;
use crate::render::gpu::RenderTarget;
use crate::render::resources::material::{
    MaterialError, MaterialPipelineCache, MaterialPipelineDesc,
};
use crate::render::view::RenderView;
use crate::render::Color;

pub use self::draw::MeshDraw;
pub use self::errors::MeshPassError;
pub use self::prepass::{SceneMaterialPrepass, SceneNormalPrepass};

const VIEW_BIND_GROUP_SLOT: u32 = 0;

/// Generic mesh renderer using the shared material system.
pub struct MeshPass {
    view_buffer: wgpu::Buffer,
    view_bind_group: wgpu::BindGroup,
    view_bind_group_layout: wgpu::BindGroupLayout,
}

impl MeshPass {
    pub fn new(ctx: &GpuContext) -> Self {
        let view_bind_group_layout =
            ctx.device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("mesh_view_bgl"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: Some(
                                std::num::NonZeroU64::new(std::mem::size_of::<
                                    crate::render::view::ViewUniform,
                                >()
                                    as u64)
                                .expect("ViewUniform has non-zero size"),
                            ),
                        },
                        count: None,
                    }],
                });

        let view_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh_view_buffer"),
            size: std::mem::size_of::<crate::render::view::ViewUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let view_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh_view_bg"),
            layout: &view_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_buffer.as_entire_binding(),
            }],
        });

        Self {
            view_buffer,
            view_bind_group,
            view_bind_group_layout,
        }
    }

    #[inline]
    pub fn view_layout(&self) -> &wgpu::BindGroupLayout {
        &self.view_bind_group_layout
    }

    pub fn create_pipeline_cache(
        &self,
        ctx: &GpuContext,
        desc: MaterialPipelineDesc,
        material_layout: Option<&wgpu::BindGroupLayout>,
    ) -> Result<MaterialPipelineCache, MaterialError> {
        MaterialPipelineCache::try_new_with_fixed_layouts(
            ctx,
            desc,
            &[(VIEW_BIND_GROUP_SLOT, &self.view_bind_group_layout)],
            material_layout,
        )
    }

    pub fn render_to_target(
        &mut self,
        ctx: &mut GpuContext,
        target: &RenderTarget,
        view: &impl RenderView,
        clear: Option<Color>,
        draws: &mut [MeshDraw<'_>],
    ) -> Result<(), MeshPassError> {
        self.upload_view(ctx, view);
        let prepared = self.prepare_draws(
            ctx,
            target.format(),
            target.sample_count(),
            None,
            false,
            draws,
        )?;

        let color_load = clear
            .map(Color::to_wgpu)
            .map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: target.view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: color_load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = ctx.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mesh_pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        self.record_draws(&mut pass, draws, &prepared);
        Ok(())
    }

    pub fn render_to_target_with_depth(
        &mut self,
        ctx: &mut GpuContext,
        target: &RenderTarget,
        depth_target: &RenderTarget,
        view: &impl RenderView,
        clear: Option<Color>,
        clear_depth: Option<f32>,
        draws: &mut [MeshDraw<'_>],
    ) -> Result<(), MeshPassError> {
        if std::ptr::eq(target.texture(), depth_target.texture()) {
            return Err(MeshPassError::AliasingTargets);
        }
        if target.sample_count() != depth_target.sample_count() {
            return Err(MeshPassError::DepthSampleCountMismatch {
                color_samples: target.sample_count(),
                depth_samples: depth_target.sample_count(),
            });
        }

        self.upload_view(ctx, view);
        let prepared = self.prepare_draws(
            ctx,
            target.format(),
            target.sample_count(),
            Some(depth_target),
            false,
            draws,
        )?;

        let color_load = clear
            .map(Color::to_wgpu)
            .map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
        let depth_load = clear_depth.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: target.view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: color_load,
                store: wgpu::StoreOp::Store,
            },
        })];
        let depth_stencil_attachment = Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth_target.view(),
            depth_ops: Some(wgpu::Operations {
                load: depth_load,
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        });
        let mut frame = ctx.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mesh_pass_depth"),
            color_attachments: &color_attachments,
            depth_stencil_attachment,
            ..Default::default()
        });
        self.record_draws(&mut pass, draws, &prepared);
        Ok(())
    }

    pub fn render_to_surface(
        &mut self,
        ctx: &mut GpuContext,
        view: &impl RenderView,
        clear: Option<Color>,
        draws: &mut [MeshDraw<'_>],
    ) -> Result<(), MeshPassError> {
        self.upload_view(ctx, view);
        let prepared = self.prepare_draws(ctx, ctx.surface_format(), 1, None, true, draws)?;

        let mut frame = ctx.frame();
        let mut pass = frame.begin_surface_pass("mesh_pass", clear.map(Color::to_wgpu));
        self.record_draws(&mut pass, draws, &prepared);
        Ok(())
    }
}
mod component;

pub use component::{MeshRenderer, WgpuMeshRenderer, ALL_SHADOW_CASCADE_MASK};
