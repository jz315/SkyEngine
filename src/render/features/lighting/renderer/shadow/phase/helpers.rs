use super::*;

pub(super) fn shadow_vertex_attributes(
    mesh_layout: &VertexLayout,
    kind: ShadowPipelineKind,
) -> Result<Vec<wgpu::VertexAttribute>, MaterialError> {
    let mut attributes = vec![shadow_vertex_attribute(
        mesh_layout,
        VertexSemantic::Position,
        wgpu::VertexFormat::Float32x3,
        0,
    )?];
    if matches!(
        kind,
        ShadowPipelineKind::AlphaTest | ShadowPipelineKind::Transparent
    ) {
        attributes.push(shadow_vertex_attribute(
            mesh_layout,
            VertexSemantic::UV0,
            wgpu::VertexFormat::Float32x2,
            1,
        )?);
    }
    Ok(attributes)
}

pub(super) fn shadow_vertex_attribute(
    mesh_layout: &VertexLayout,
    semantic: VertexSemantic,
    expected: wgpu::VertexFormat,
    shader_location: u32,
) -> Result<wgpu::VertexAttribute, MaterialError> {
    let actual = mesh_layout
        .attributes()
        .iter()
        .find(|attribute| attribute.semantic == semantic)
        .ok_or(MaterialError::MissingVertexAttribute { semantic })?;
    if actual.format != expected {
        return Err(MaterialError::VertexAttributeFormatMismatch {
            semantic,
            expected,
            actual: actual.format,
        });
    }
    Ok(wgpu::VertexAttribute {
        format: actual.format,
        offset: actual.offset as u64,
        shader_location,
    })
}

pub(super) fn shadow_caster_kind(
    item: &PhaseItem,
    draw_functions: &DrawFunctionRegistry,
    material_registry: &MaterialRegistry,
) -> ShadowCasterKind {
    if !item.has_payload::<MeshDrawData>() {
        return ShadowCasterKind::Opaque;
    }
    if draw_functions.material_type_id(item.draw_function_id)
        != Some(TypeId::of::<StandardMaterial>())
    {
        return ShadowCasterKind::Opaque;
    }

    let draw = *item.data::<MeshDrawData>();
    let raw_material_handle = draw.material_handle::<StandardMaterial>();
    if !material_registry.is_registered::<StandardMaterial>() {
        return ShadowCasterKind::Opaque;
    }
    let Ok(material) = material_registry.get_erased::<StandardMaterial>(raw_material_handle) else {
        return ShadowCasterKind::Opaque;
    };
    let Some(model_id) = material_registry.model_id::<StandardMaterial>() else {
        return ShadowCasterKind::Opaque;
    };
    let material_handle =
        crate::render::resources::material::TypedMaterialHandle::<StandardMaterial>::new(
            model_id,
            raw_material_handle.id(),
        );
    if material.casts_alpha_test_shadow() {
        ShadowCasterKind::AlphaTest(material_handle.into())
    } else {
        ShadowCasterKind::Opaque
    }
}

pub(super) fn count_phase_shadow_batches(items: &[PhaseItem]) -> usize {
    let mut draws = 0usize;
    let mut cursor = 0usize;
    while cursor < items.len() {
        if !items[cursor].has_payload::<MeshDrawData>() {
            cursor += 1;
            continue;
        }
        let base = *items[cursor].data::<MeshDrawData>();
        let base_draw_function = items[cursor].draw_function_id;
        let base_material = base.material_handle::<StandardMaterial>();
        let mut batch_end = cursor + 1;
        while batch_end < items.len() {
            if !items[batch_end].has_payload::<MeshDrawData>() {
                break;
            }
            let next = *items[batch_end].data::<MeshDrawData>();
            let next_draw_function = items[batch_end].draw_function_id;
            if next.mesh_handle() != base.mesh_handle()
                || next.sub_mesh_index() != base.sub_mesh_index()
                || next_draw_function != base_draw_function
                || next.material_handle::<StandardMaterial>() != base_material
            {
                break;
            }
            batch_end += 1;
        }
        draws += 1;
        cursor = batch_end;
    }
    draws
}

pub(super) fn shadow_pipeline_material_error(error: MaterialError) -> RenderGraphError {
    RenderGraphError::ExecutionFailed(format!("failed to build shadow pipeline: {error}"))
}

pub(super) fn import_shadow_target(shadow: &ShadowViewBinding) -> ImportedTexture {
    let target = shadow.target();
    import_render_target(target)
}

pub(super) fn import_transparent_shadow_target(shadow: &ShadowViewBinding) -> ImportedTexture {
    import_render_target(shadow.transparent_target())
}

pub(super) fn import_render_target(target: &crate::render::gpu::RenderTarget) -> ImportedTexture {
    ImportedTexture {
        texture: std::sync::Arc::new(target.texture().clone()),
        view: std::sync::Arc::new(target.view().clone()),
        size: [target.width(), target.height()],
        format: target.format(),
        usage: target.usage(),
        sample_count: target.sample_count(),
        mip_level_count: target.mip_level_count(),
        array_layer_count: target.array_layer_count(),
    }
}
