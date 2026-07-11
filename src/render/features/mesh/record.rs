use crate::gpu::GpuContext;
use crate::render::view::RenderView;

use super::draw::{MeshDraw, PreparedMeshDraw};
use super::{MeshPass, VIEW_BIND_GROUP_SLOT};

impl MeshPass {
    pub(super) fn upload_view(&self, ctx: &GpuContext, view: &impl RenderView) {
        ctx.queue().write_buffer(
            &self.view_buffer,
            0,
            bytemuck::bytes_of(&view.view_uniform()),
        );
    }

    pub(super) fn record_draws(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        draws: &mut [MeshDraw<'_>],
        prepared: &[PreparedMeshDraw],
    ) {
        for (draw, prepared) in draws.iter_mut().zip(prepared.iter()) {
            if prepared.instances.is_empty() {
                continue;
            }

            pass.set_pipeline(prepared.pipeline.as_ref());
            pass.set_bind_group(VIEW_BIND_GROUP_SLOT, &self.view_bind_group, &[]);

            if let Some(material) = draw.material {
                if let Some(slot) = draw.pipeline.material_slot() {
                    pass.set_bind_group(slot, material.bind_group(), &[]);
                }
            }

            pass.set_vertex_buffer(0, draw.mesh.vertex_buffer().slice(..));
            if let Some(index_range) = &prepared.index_range {
                pass.set_index_buffer(
                    draw.mesh
                        .index_buffer()
                        .expect("indexed draw validated before recording")
                        .slice(..),
                    draw.mesh
                        .index_format()
                        .expect("indexed draw validated before recording"),
                );
                pass.draw_indexed(
                    index_range.clone(),
                    prepared.base_vertex,
                    prepared.instances.clone(),
                );
            } else {
                pass.draw(prepared.vertex_range.clone(), prepared.instances.clone());
            }
        }
    }
}
