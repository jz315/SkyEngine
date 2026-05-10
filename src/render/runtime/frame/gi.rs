use crate::gpu::GpuContext;
use crate::render::gi::{first_lit_view, GiMaterial, GiRenderable, GiSceneInput};
use crate::render::phase::{MeshDrawData, OpaquePhase, PhaseItem, TransparentPhase};
use crate::render::resources::material::{AlphaMode, StandardMaterial};
use crate::render::resources::mesh::Mesh;

use super::{ExtractedFrame, FrameRuntimeParts, SceneUploadFrame, IDENTITY_MODEL_MATRIX};

pub(crate) fn prepare_global_illumination(
    parts: &mut FrameRuntimeParts<'_>,
    gpu: &mut GpuContext,
    extracted: &ExtractedFrame,
    uploads: &SceneUploadFrame,
) {
    let gi_renderables = collect_gi_renderables(
        &extracted.opaque_phases,
        &extracted.transparent_phases,
        &parts.resources.draw_functions,
        &uploads.model_matrices,
        &parts.resources.material_registry,
        &parts.resources.mesh_registry,
    );
    let (primary_view_index, primary_view) = first_lit_view(&extracted.views)
        .map_or((None, None), |(index, view)| (Some(index), Some(view)));
    let gi_scene = GiSceneInput {
        primary_view_index,
        primary_view,
        views: &extracted.views,
        lights: &uploads.lights,
        ambient_color: parts.runtime.frame_settings.ambient_color,
        frame_index: 0,
        renderables: &gi_renderables,
    };
    parts
        .runtime
        .gi
        .as_mut()
        .expect("GI runtime should initialize before frame build")
        .prepare(
            gpu,
            &parts.runtime.frame_settings.global_illumination,
            gi_scene,
        );
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
