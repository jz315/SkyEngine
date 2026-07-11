use crate::render::execution::PreparedFrame;
use crate::render::gi::{first_lit_view, GiMaterial, GiRenderable, GiRuntime, GiSceneInput};
use crate::render::phase::{MeshDrawData, OpaquePhase, PhaseItem, TransparentPhase};
use crate::render::resources::material::{AlphaMode, StandardMaterial};
use crate::render::resources::mesh::Mesh;
use crate::render::resources::FrameIndirectLighting;
use crate::render::runtime::{
    FrameExtension, FrameExtensionError, FrameExtensionInitContext, FrameExtensionPrepareContext,
};

const IDENTITY_MODEL_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

/// Feature-owned GI state inserted into the frame through the core adapter.
#[derive(Default)]
pub(crate) struct GiFrameExtension {
    runtime: Option<GiRuntime>,
    frame_resources: Option<FrameIndirectLighting>,
}

impl FrameExtension for GiFrameExtension {
    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn initialize(&mut self, ctx: FrameExtensionInitContext<'_>) {
        if self.runtime.is_none() {
            self.runtime = Some(GiRuntime::new(ctx.gpu));
        }
    }

    fn prepare(
        &mut self,
        ctx: FrameExtensionPrepareContext<'_>,
    ) -> Result<(), FrameExtensionError> {
        let gi_renderables = collect_gi_renderables(
            &ctx.extracted.opaque_phases,
            &ctx.extracted.transparent_phases,
            &ctx.resources.draw_functions,
            &ctx.uploads.model_matrices,
            &ctx.resources.material_registry,
            &ctx.resources.mesh_registry,
        );
        let (primary_view_index, primary_view) = first_lit_view(&ctx.extracted.views)
            .map_or((None, None), |(index, view)| (Some(index), Some(view)));
        let gi_scene = GiSceneInput {
            primary_view_index,
            primary_view,
            views: &ctx.extracted.views,
            lights: &ctx.uploads.lights,
            ambient_color: ctx.runtime.frame_settings.ambient_color,
            frame_index: 0,
            renderables: &gi_renderables,
        };
        let runtime = self
            .runtime
            .as_mut()
            .expect("GI extension must initialize before frame preparation");
        runtime.prepare(
            ctx.gpu,
            &ctx.runtime.frame_settings.global_illumination,
            gi_scene,
        );
        self.frame_resources = Some(FrameIndirectLighting::new(
            runtime.sampling_binding(),
            runtime.shader_descriptor(),
        ));
        Ok(())
    }

    fn insert_frame_payloads<'a>(&'a self, frame: &mut PreparedFrame<'a>) {
        let runtime = self
            .runtime
            .as_ref()
            .expect("GI extension must initialize before frame assembly");
        let _ = frame.insert_payload(runtime);
        if let Some(resources) = self.frame_resources.as_ref() {
            let _ = frame.insert_payload(resources);
        }
    }
}

pub(crate) fn collect_gi_renderables<'a>(
    opaque_phases: &[OpaquePhase],
    transparent_phases: &[TransparentPhase],
    draw_functions: &crate::render::phase::DrawFunctionRegistry,
    model_matrices: &[[f32; 16]],
    material_registry: &'a crate::render::resources::material::MaterialRegistry,
    mesh_registry: &'a crate::render::resources::mesh::MeshRegistry,
) -> Vec<GiRenderable<'a>> {
    let mut renderables = Vec::new();
    collect_standard_gi_renderables(
        opaque_phases,
        true,
        draw_functions,
        model_matrices,
        material_registry,
        mesh_registry,
        &mut renderables,
    );
    collect_standard_gi_renderables(
        transparent_phases,
        false,
        draw_functions,
        model_matrices,
        material_registry,
        mesh_registry,
        &mut renderables,
    );
    renderables
}

trait PhaseItems {
    fn phase_items(&self) -> &[PhaseItem];
}

impl PhaseItems for OpaquePhase {
    fn phase_items(&self) -> &[PhaseItem] {
        self.items()
    }
}

impl PhaseItems for TransparentPhase {
    fn phase_items(&self) -> &[PhaseItem] {
        self.items()
    }
}

fn collect_standard_gi_renderables<'a, P>(
    phases: &[P],
    opaque: bool,
    draw_functions: &crate::render::phase::DrawFunctionRegistry,
    model_matrices: &[[f32; 16]],
    material_registry: &'a crate::render::resources::material::MaterialRegistry,
    mesh_registry: &'a crate::render::resources::mesh::MeshRegistry,
    out: &mut Vec<GiRenderable<'a>>,
) where
    P: PhaseItems,
{
    if !material_registry.is_registered::<StandardMaterial>() {
        return;
    }
    let standard_type = std::any::TypeId::of::<StandardMaterial>();
    for phase in phases {
        for item in phase.phase_items() {
            if draw_functions.material_type_id(item.draw_function_id) != Some(standard_type) {
                continue;
            }
            if !item.has_payload::<MeshDrawData>() {
                continue;
            }
            let draw = *item.data::<MeshDrawData>();
            let Some(mesh) = mesh_registry.get(draw.mesh_handle()) else {
                continue;
            };
            let Some(ray_mesh) = mesh.ray_mesh() else {
                continue;
            };
            let Some(triangle_range) = ray_triangle_range_for_sub_mesh(mesh, draw.sub_mesh_index())
            else {
                continue;
            };
            let Ok(material) = material_registry
                .get_erased::<StandardMaterial>(draw.material_handle::<StandardMaterial>())
            else {
                continue;
            };
            let is_opaque =
                opaque && !matches!(material.alpha_mode, AlphaMode::Blend | AlphaMode::Additive);
            out.push(GiRenderable {
                model: crate::math::Mat4::from_cols_array(
                    *model_matrices
                        .get(draw.model_slot() as usize)
                        .unwrap_or(&IDENTITY_MODEL_MATRIX),
                ),
                layer_mask: u32::MAX,
                opaque: is_opaque,
                ray_triangles: ray_mesh.triangles(),
                triangle_range,
                material: GiMaterial {
                    albedo: material.albedo,
                    emissive: material.emissive,
                    metallic: material.metallic,
                },
            });
        }
    }
}

fn ray_triangle_range_for_sub_mesh(
    mesh: &Mesh,
    sub_mesh_index: u32,
) -> Option<std::ops::Range<usize>> {
    let ray_mesh = mesh.ray_mesh()?;
    let sub_mesh = mesh.sub_meshes().get(sub_mesh_index as usize)?;
    if !mesh.has_indices() {
        return Some(0..ray_mesh.triangles().len());
    }
    let index_offset = if sub_mesh.index_count == 0 {
        0
    } else {
        sub_mesh.index_offset
    };
    let index_count = if sub_mesh.index_count == 0 {
        mesh.index_count()
    } else {
        sub_mesh.index_count
    };
    let start = (index_offset / 3) as usize;
    let end = ((index_offset + index_count) / 3) as usize;
    Some(start.min(ray_mesh.triangles().len())..end.min(ray_mesh.triangles().len()))
}
