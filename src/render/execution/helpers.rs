#![allow(dead_code)]

use crate::render::gpu::RenderTarget;
use crate::render::graph::{
    CompiledPass, PhysicalResources, RenderGraph, ResourceRef, TargetSize, TextureHandle,
};

use super::slots::{PhaseState, TextureSlot};
use super::TextureFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SceneTextureKind {
    Depth,
    Normal,
    Velocity,
    Albedo,
    Material,
    Emissive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RequiredSceneGBuffer {
    pub color: TextureSlot,
    pub depth: TextureSlot,
    pub normal: TextureSlot,
    pub velocity: TextureSlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RequiredSceneMaterialInputs {
    pub albedo: TextureSlot,
    pub material: TextureSlot,
    pub emissive: TextureSlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RequiredSceneInputs {
    pub color: TextureSlot,
    pub depth: TextureSlot,
    pub normal: TextureSlot,
    pub velocity: TextureSlot,
    pub albedo: TextureSlot,
    pub material: TextureSlot,
    pub emissive: TextureSlot,
}

#[inline]
fn require_texture_slot(slot: Option<TextureSlot>, node_name: &str, label: &str) -> TextureSlot {
    slot.unwrap_or_else(|| panic!("{node_name} requires {label}"))
}

#[inline]
pub(crate) fn require_current_color(state: &PhaseState, node_name: &str) -> TextureSlot {
    require_texture_slot(state.current_color(), node_name, "current color input")
}

#[inline]
pub(crate) fn require_scene_color(state: &PhaseState, node_name: &str) -> TextureSlot {
    require_texture_slot(state.scene_color(), node_name, "scene color input")
}

#[inline]
pub(crate) fn require_scene_depth(state: &PhaseState, node_name: &str) -> TextureSlot {
    require_texture_slot(state.scene_depth(), node_name, "scene depth input")
}

#[inline]
pub(crate) fn require_scene_normal(state: &PhaseState, node_name: &str) -> TextureSlot {
    require_texture_slot(state.scene_normal(), node_name, "scene normal input")
}

#[inline]
pub(crate) fn require_scene_velocity(state: &PhaseState, node_name: &str) -> TextureSlot {
    require_texture_slot(state.scene_velocity(), node_name, "scene velocity input")
}

#[inline]
pub(crate) fn require_scene_albedo(state: &PhaseState, node_name: &str) -> TextureSlot {
    require_texture_slot(state.scene_albedo(), node_name, "scene albedo input")
}

#[inline]
pub(crate) fn require_scene_material(state: &PhaseState, node_name: &str) -> TextureSlot {
    require_texture_slot(state.scene_material(), node_name, "scene material input")
}

#[inline]
pub(crate) fn require_scene_emissive(state: &PhaseState, node_name: &str) -> TextureSlot {
    require_texture_slot(state.scene_emissive(), node_name, "scene emissive input")
}

#[inline]
pub(crate) fn require_scene_gbuffer(state: &PhaseState, node_name: &str) -> RequiredSceneGBuffer {
    RequiredSceneGBuffer {
        color: require_scene_color(state, node_name),
        depth: require_scene_depth(state, node_name),
        normal: require_scene_normal(state, node_name),
        velocity: require_scene_velocity(state, node_name),
    }
}

#[inline]
pub(crate) fn require_scene_material_inputs(
    state: &PhaseState,
    node_name: &str,
) -> RequiredSceneMaterialInputs {
    RequiredSceneMaterialInputs {
        albedo: require_scene_albedo(state, node_name),
        material: require_scene_material(state, node_name),
        emissive: require_scene_emissive(state, node_name),
    }
}

#[inline]
pub(crate) fn require_scene_inputs(state: &PhaseState, node_name: &str) -> RequiredSceneInputs {
    let gbuffer = require_scene_gbuffer(state, node_name);
    let material = require_scene_material_inputs(state, node_name);
    RequiredSceneInputs {
        color: gbuffer.color,
        depth: gbuffer.depth,
        normal: gbuffer.normal,
        velocity: gbuffer.velocity,
        albedo: material.albedo,
        material: material.material,
        emissive: material.emissive,
    }
}

#[inline]
pub(crate) fn bind_current_as_scene_color(state: &mut PhaseState, node_name: &str) -> TextureSlot {
    let current = require_current_color(state, node_name);
    state.set_scene_color(current.handle(), current.format());
    current
}

#[inline]
fn scene_texture(state: &PhaseState, kind: SceneTextureKind) -> Option<TextureSlot> {
    match kind {
        SceneTextureKind::Depth => state.scene_depth(),
        SceneTextureKind::Normal => state.scene_normal(),
        SceneTextureKind::Velocity => state.scene_velocity(),
        SceneTextureKind::Albedo => state.scene_albedo(),
        SceneTextureKind::Material => state.scene_material(),
        SceneTextureKind::Emissive => state.scene_emissive(),
    }
}

#[inline]
fn set_scene_texture(
    state: &mut PhaseState,
    kind: SceneTextureKind,
    handle: TextureHandle,
    format: TextureFormat,
) {
    match kind {
        SceneTextureKind::Depth => state.set_scene_depth(handle, format),
        SceneTextureKind::Normal => state.set_scene_normal(handle, format),
        SceneTextureKind::Velocity => state.set_scene_velocity(handle, format),
        SceneTextureKind::Albedo => state.set_scene_albedo(handle, format),
        SceneTextureKind::Material => state.set_scene_material(handle, format),
        SceneTextureKind::Emissive => state.set_scene_emissive(handle, format),
    }
}

#[inline]
pub(crate) fn create_scene_texture(
    graph: &mut RenderGraph,
    state: &mut PhaseState,
    target_size: [u32; 2],
    kind: SceneTextureKind,
    format: TextureFormat,
    debug_name: &'static str,
) -> TextureSlot {
    let handle = graph.create_texture(|builder| {
        builder
            .name(debug_name)
            .size(TargetSize::Exact(target_size[0], target_size[1]))
            .format(format);
    });
    set_scene_texture(state, kind, handle, format);
    TextureSlot::new(handle, format)
}

#[inline]
pub(crate) fn ensure_scene_texture(
    graph: &mut RenderGraph,
    state: &mut PhaseState,
    target_size: [u32; 2],
    kind: SceneTextureKind,
    format: TextureFormat,
    debug_name: &'static str,
) -> TextureSlot {
    scene_texture(state, kind).unwrap_or_else(|| {
        create_scene_texture(graph, state, target_size, kind, format, debug_name)
    })
}

#[inline]
pub(crate) fn pass_first_read_texture(
    pass: &CompiledPass,
    node_name: &str,
    label: &str,
) -> TextureHandle {
    pass_nth_read_texture(pass, 0, node_name, label)
}

#[inline]
pub(crate) fn pass_nth_read_texture(
    pass: &CompiledPass,
    index: usize,
    node_name: &str,
    label: &str,
) -> TextureHandle {
    pass.reads
        .iter()
        .filter_map(|resource| match resource {
            ResourceRef::Texture(handle) => Some(*handle),
            _ => None,
        })
        .nth(index)
        .unwrap_or_else(|| panic!("{node_name} should read {label} texture"))
}

#[inline]
pub(crate) fn pass_first_write_texture(
    pass: &CompiledPass,
    node_name: &str,
    label: &str,
) -> TextureHandle {
    pass_nth_write_texture(pass, 0, node_name, label)
}

#[inline]
pub(crate) fn pass_nth_write_texture(
    pass: &CompiledPass,
    index: usize,
    node_name: &str,
    label: &str,
) -> TextureHandle {
    pass.writes
        .iter()
        .filter_map(|resource| match resource {
            ResourceRef::Texture(handle) => Some(*handle),
            _ => None,
        })
        .nth(index)
        .unwrap_or_else(|| panic!("{node_name} should write {label} texture"))
}

#[inline]
pub(crate) fn require_render_target<'a>(
    resources: &'a PhysicalResources<'a>,
    handle: TextureHandle,
    node_name: &str,
    label: &str,
) -> &'a RenderTarget {
    resources
        .render_target(handle)
        .unwrap_or_else(|| panic!("{node_name} {label} target should be allocated"))
}
