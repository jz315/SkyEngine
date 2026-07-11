use super::*;

pub(super) fn nth_texture(resources: &[ResourceRef], index: usize, access: &str) -> TextureHandle {
    resources
        .iter()
        .filter_map(|resource| match resource {
            ResourceRef::Texture(handle) => Some(*handle),
            _ => None,
        })
        .nth(index)
        .unwrap_or_else(|| panic!("compute pass should have {access} texture at index {index}"))
}

pub(super) fn nth_subresource(
    resources: &[ResourceRef],
    index: usize,
    access: &str,
) -> TextureSubresource {
    resources
        .iter()
        .filter_map(|resource| match resource {
            ResourceRef::TextureSubresource(subresource) => Some(*subresource),
            _ => None,
        })
        .nth(index)
        .unwrap_or_else(|| panic!("compute pass should have {access} subresource at index {index}"))
}

#[inline]
pub(super) fn require_scene_texture_slot(
    slot: Option<TextureSlot>,
    texture: SceneTexture,
) -> TextureSlot {
    slot.unwrap_or_else(|| panic!("{} texture is required", texture.label()))
}

#[inline]
pub(super) fn create_scene_texture(
    graph: &mut RenderGraph,
    target_size: [u32; 2],
    texture: SceneTexture,
    format: TextureFormat,
) -> TextureHandle {
    graph.create_texture(|builder| {
        builder
            .name(texture.debug_name())
            .size(TargetSize::Exact(target_size[0], target_size[1]))
            .format(format);
    })
}

#[inline]
pub(super) fn set_phase_scene_texture(
    state: &mut PhaseState,
    texture: SceneTexture,
    handle: TextureHandle,
    format: TextureFormat,
) -> TextureSlot {
    let _ = state.set_scene_texture(texture, handle, format);
    TextureSlot::new(handle, format)
}

#[inline]
pub(super) fn set_finalize_scene_texture(
    state: &mut FinalizePhaseState<'_>,
    texture: SceneTexture,
    handle: TextureHandle,
    format: TextureFormat,
) -> TextureSlot {
    let _ = state.set_scene_texture(texture, handle, format);
    TextureSlot::new(handle, format)
}

#[inline]
pub(super) fn ensure_phase_scene_texture(
    graph: &mut RenderGraph,
    state: &mut PhaseState,
    target_size: [u32; 2],
    texture: SceneTexture,
    format: TextureFormat,
) -> TextureSlot {
    state.scene_texture(texture).unwrap_or_else(|| {
        let handle = create_scene_texture(graph, target_size, texture, format);
        set_phase_scene_texture(state, texture, handle, format)
    })
}

#[inline]
pub(super) fn ensure_finalize_scene_texture(
    graph: &mut RenderGraph,
    state: &mut FinalizePhaseState<'_>,
    target_size: [u32; 2],
    texture: SceneTexture,
    format: TextureFormat,
) -> TextureSlot {
    state.scene_texture(texture).unwrap_or_else(|| {
        let handle = create_scene_texture(graph, target_size, texture, format);
        set_finalize_scene_texture(state, texture, handle, format)
    })
}

#[inline]
pub(super) fn create_texture_from_spec(graph: &mut RenderGraph, spec: TextureSpec) -> TextureSlot {
    spec.create_slot(graph)
}

pub(super) fn scene_lighting_for_view<'frame>(
    frame: &'frame PreparedFrame<'frame>,
    view: &'frame PreparedView<'frame>,
) -> Option<SceneLightingResources<'frame>> {
    let gpu_scene = frame.payload::<GpuScene>()?;
    let settings = frame
        .payload::<RenderSettings>()
        .cloned()
        .unwrap_or_default();
    let mut lighting = SceneLightingResources::new(
        gpu_scene.table::<LightTable>(),
        settings.ambient_color.to_array(),
    );
    if let Some(scene_shadows) = view.payload::<SceneShadowResources>() {
        lighting = lighting.with_scene_bind_group(
            scene_shadows
                .bind_group()
                .expect("enabled scene shadow resources must have a bind group"),
            scene_shadows
                .bind_group_layout()
                .expect("enabled scene shadow resources must have a bind group layout"),
            scene_shadows.enabled(),
        );
    }
    Some(lighting)
}

#[inline]
pub(super) fn scene_shadows_for_view<'frame>(
    frame: &'frame PreparedFrame<'frame>,
    view: &'frame PreparedView<'frame>,
) -> Option<SceneShadowResources> {
    let _ = frame;
    view.payload::<SceneShadowResources>().cloned()
}

#[inline]
pub(super) fn require_scene_shadows(shadows: Option<SceneShadowResources>) -> SceneShadowResources {
    shadows.expect("scene shadow resources are required")
}
