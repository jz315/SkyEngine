use crate::gpu::GpuContext;
use crate::render::gpu::RenderTarget;

use super::draw::{
    validate_index_range, validate_instances, validate_vertex_range, MeshDraw, PreparedMeshDraw,
};
use super::{MeshPass, MeshPassError};

impl MeshPass {
    pub(super) fn prepare_draws(
        &self,
        ctx: &GpuContext,
        target_format: wgpu::TextureFormat,
        target_samples: u32,
        depth_target: Option<&RenderTarget>,
        is_surface: bool,
        draws: &mut [MeshDraw<'_>],
    ) -> Result<Vec<PreparedMeshDraw>, MeshPassError> {
        let mut prepared = Vec::with_capacity(draws.len());

        for draw in draws.iter_mut() {
            let (pipeline_label, pipeline_samples, pipeline_depth) = {
                let pipeline_desc = draw.pipeline.desc();
                (
                    pipeline_desc.label.to_string(),
                    pipeline_desc.multisample.count.max(1),
                    pipeline_desc.depth_stencil.clone(),
                )
            };

            if is_surface {
                if pipeline_samples != 1 {
                    return Err(MeshPassError::SurfaceSampleCountMismatch {
                        pipeline: pipeline_label,
                        pipeline_samples,
                    });
                }
            } else if pipeline_samples != target_samples {
                return Err(MeshPassError::TargetSampleCountMismatch {
                    pipeline: pipeline_label,
                    pipeline_samples,
                    target_samples,
                });
            }

            match (pipeline_depth.as_ref(), depth_target) {
                (Some(depth_state), Some(depth_target)) => {
                    if depth_state.format != depth_target.format() {
                        return Err(MeshPassError::DepthFormatMismatch {
                            pipeline: pipeline_label,
                            pipeline_format: depth_state.format,
                            target_format: depth_target.format(),
                        });
                    }
                    if pipeline_samples != depth_target.sample_count() {
                        return Err(MeshPassError::DepthSampleCountMismatch {
                            color_samples: target_samples,
                            depth_samples: depth_target.sample_count(),
                        });
                    }
                }
                (Some(_), None) => {
                    return Err(MeshPassError::MissingDepthTarget {
                        pipeline: pipeline_label,
                    });
                }
                (None, Some(_)) => {
                    return Err(MeshPassError::UnexpectedDepthTarget {
                        pipeline: pipeline_label,
                    });
                }
                (None, None) => {}
            }

            if draw.pipeline.material_slot().is_some() && draw.material.is_none() {
                return Err(MeshPassError::MissingMaterial {
                    pipeline: draw.pipeline.desc().label.to_string(),
                });
            }

            let instances = validate_instances(draw.instances.clone())?;
            let pipeline = draw.pipeline.try_pipeline_arc(ctx, target_format)?;

            let index_range = if draw.mesh.has_indices() || draw.index_range.is_some() {
                if !draw.mesh.has_indices() {
                    return Err(MeshPassError::MissingIndexBuffer {
                        mesh: draw.mesh.label().to_string(),
                    });
                }
                Some(validate_index_range(
                    draw.mesh,
                    draw.index_range
                        .clone()
                        .unwrap_or(0..draw.mesh.index_count()),
                )?)
            } else {
                None
            };

            let vertex_range = validate_vertex_range(
                draw.mesh,
                draw.vertex_range
                    .clone()
                    .unwrap_or(0..draw.mesh.vertex_count()),
            )?;

            prepared.push(PreparedMeshDraw {
                pipeline,
                vertex_range,
                index_range,
                base_vertex: draw.base_vertex,
                instances,
            });
        }

        Ok(prepared)
    }
}
