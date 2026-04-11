use crate::render::core::target::RenderTarget;
use crate::render::graph::{CompiledPass, PhysicalResources, ResourceRef, TextureHandle};

use super::slots::{PhaseState, TextureSlot};

#[inline]
pub(crate) fn require_current_color(state: &PhaseState, node_name: &str) -> TextureSlot {
    state
        .current_color()
        .unwrap_or_else(|| panic!("{node_name} requires current color input"))
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
    pass.writes
        .iter()
        .filter_map(|resource| match resource {
            ResourceRef::Texture(handle) => Some(*handle),
            _ => None,
        })
        .next()
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
