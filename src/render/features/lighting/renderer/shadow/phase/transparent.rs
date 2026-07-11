use super::*;

pub(super) fn transparent_shadow_material_handle(
    item: &PhaseItem,
    draw_functions: &DrawFunctionRegistry,
    material_registry: &MaterialRegistry,
) -> Option<MaterialHandle> {
    if !item.has_payload::<MeshDrawData>() {
        return None;
    }
    if draw_functions.material_type_id(item.draw_function_id)
        != Some(TypeId::of::<StandardMaterial>())
    {
        return None;
    }

    let draw = *item.data::<MeshDrawData>();
    let raw_material_handle = draw.material_handle::<StandardMaterial>();
    let material = material_registry
        .get_erased::<StandardMaterial>(raw_material_handle)
        .ok()?;
    let model_id = material_registry.model_id::<StandardMaterial>()?;
    let material_handle =
        crate::render::resources::material::TypedMaterialHandle::<StandardMaterial>::new(
            model_id,
            raw_material_handle.id(),
        );
    material
        .alpha_mode
        .is_transparent()
        .then_some(material_handle.into())
}

pub(super) fn has_transparent_mesh_shadow_candidates(
    items: &[PhaseItem],
    draw_functions: &DrawFunctionRegistry,
) -> bool {
    items.iter().any(|item| {
        item.has_payload::<MeshDrawData>()
            && draw_functions.material_type_id(item.draw_function_id)
                == Some(TypeId::of::<StandardMaterial>())
    })
}
