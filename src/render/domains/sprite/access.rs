use crate::render::frame_pipeline::{PhaseState, TextureSlot, ViewExecutionContext};

use super::gpu_scene::GpuScene2D;
use super::prepared::{PreparedView2D, SpriteDrawSpan};

#[inline]
pub(crate) fn gpu_scene<'a>(execution: &'a ViewExecutionContext<'a>) -> &'a GpuScene2D {
    execution
        .frame_payload::<GpuScene2D>()
        .expect("2D render feature requires a GpuScene2D frame payload")
}

#[inline]
pub(crate) fn prepared_view_2d<'a>(execution: &'a ViewExecutionContext<'a>) -> &'a PreparedView2D {
    execution
        .view_payload::<PreparedView2D>()
        .expect("2D render feature requires a PreparedView2D view payload")
}

#[inline]
pub(crate) fn draw_spans<'a>(execution: &'a ViewExecutionContext<'a>) -> &'a [SpriteDrawSpan] {
    let view = prepared_view_2d(execution);
    gpu_scene(execution).draw_spans_for_view(*view)
}

#[inline]
pub(crate) fn require_texture_slot(state: &PhaseState, name: &str, node_name: &str) -> TextureSlot {
    state
        .texture_slot(name)
        .unwrap_or_else(|| panic!("{node_name} requires `{name}` texture slot"))
}
