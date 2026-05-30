#![allow(unused_imports)]
use super::backdrop::*;
use super::buffers::*;
use super::images::*;
use super::primitives::*;
use super::text::*;
use super::*;

#[derive(Clone, Copy)]
pub(super) enum PrimitiveKind {
    Rect,
    Polygon,
    Image { image_index: usize },
}

#[derive(Clone, Copy)]
pub(super) enum RenderOp {
    Primitive(usize),
    Text { start: usize, count: usize },
}

#[derive(Clone, Copy)]
pub(super) struct PrimitiveOp {
    pub(super) kind: PrimitiveKind,
    pub(super) start: u32,
    pub(super) count: u32,
    pub(super) clip: LayoutRect,
    pub(super) backdrop_frame: LayoutRect,
    pub(super) backdrop_blur: f32,
}

#[derive(Default)]
pub(super) struct RenderScratch {
    pub(super) rect_vertices: Vec<NeoRectVertex>,
    pub(super) polygon_vertices: Vec<NeoPolygonVertex>,
    pub(super) image_vertices: Vec<NeoImageVertex>,
    pub(super) primitive_ops: Vec<PrimitiveOp>,
    pub(super) text_items: Vec<TextItem>,
    pub(super) image_items: Vec<ImageItem>,
    pub(super) render_ops: Vec<RenderOp>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct DrawCollectKey {
    pub(super) draw_ptr: usize,
    pub(super) draw_len: usize,
    pub(super) draw_revision: u64,
    pub(super) logical_width_bits: u32,
    pub(super) logical_height_bits: u32,
    pub(super) surface_is_srgb: bool,
    pub(super) image_cache_revision: u64,
}

impl RenderScratch {
    pub(super) fn clear(&mut self) {
        self.rect_vertices.clear();
        self.polygon_vertices.clear();
        self.image_vertices.clear();
        self.primitive_ops.clear();
        self.text_items.clear();
        self.image_items.clear();
        self.render_ops.clear();
    }
}

pub(super) fn collect_draw_items(
    draw_list: &UiDrawList,
    full_clip: LayoutRect,
    surface_is_srgb: bool,
    image_cache: &FxHashMap<ImageRef, CachedNeoImage>,
    rect_vertices: &mut Vec<NeoRectVertex>,
    polygon_vertices: &mut Vec<NeoPolygonVertex>,
    image_vertices: &mut Vec<NeoImageVertex>,
    primitive_ops: &mut Vec<PrimitiveOp>,
    text_items: &mut Vec<TextItem>,
    image_items: &mut Vec<ImageItem>,
    render_ops: &mut Vec<RenderOp>,
) {
    let mut clip = UiClip::rect(full_clip);
    let mut stack = Vec::new();

    for command in draw_list.commands() {
        match command {
            UiDrawCommand::PushClip(next) => {
                stack.push(clip);
                clip =
                    intersect_clip(clip, *next).unwrap_or_else(|| UiClip::rect(LayoutRect::ZERO));
            }
            UiDrawCommand::PopClip => {
                clip = stack.pop().unwrap_or_else(|| UiClip::rect(full_clip));
            }
            UiDrawCommand::Rect(draw) => {
                if push_rect(rect_vertices, primitive_ops, draw, clip, surface_is_srgb) {
                    render_ops.push(RenderOp::Primitive(primitive_ops.len() - 1));
                }
            }
            UiDrawCommand::Polygon(draw) => {
                if push_polygon(polygon_vertices, primitive_ops, draw, clip, surface_is_srgb) {
                    render_ops.push(RenderOp::Primitive(primitive_ops.len() - 1));
                }
            }
            UiDrawCommand::Text(draw) => {
                let start = text_items.len();
                if push_text(text_items, draw, clip.rect, surface_is_srgb) {
                    match render_ops.last_mut() {
                        Some(RenderOp::Text { start: _, count })
                            if start == text_items.len() - 1 =>
                        {
                            *count += 1;
                        }
                        _ => render_ops.push(RenderOp::Text { start, count: 1 }),
                    }
                }
            }
            UiDrawCommand::Image(draw) => {
                if push_image(
                    image_vertices,
                    primitive_ops,
                    image_items,
                    draw,
                    clip,
                    image_cache,
                    surface_is_srgb,
                ) {
                    render_ops.push(RenderOp::Primitive(primitive_ops.len() - 1));
                }
            }
            UiDrawCommand::NineSlice(draw) => {
                if push_nine_slice(
                    image_vertices,
                    primitive_ops,
                    image_items,
                    draw,
                    clip,
                    image_cache,
                    surface_is_srgb,
                ) {
                    render_ops.push(RenderOp::Primitive(primitive_ops.len() - 1));
                }
            }
        }
    }
}
