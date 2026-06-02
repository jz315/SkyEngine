#![allow(unused_imports)]
use super::backdrop::*;
use super::buffers::*;
use super::collect::*;
use super::images::*;
use super::primitives::*;
use super::text::*;
use super::*;
use eui_neo::expert::{UiDrawCommand, UiDrawList, UiRectDraw, UiTextDraw};
use eui_neo::{
    Border, Color, FontRef, Gradient, HorizontalAlign, ImageFit, LayoutRect, Shadow, Transform,
    VerticalAlign,
};
use rustc_hash::FxHashMap;

#[test]
fn srgb_output_conversion_keeps_alpha_and_linearizes_rgb() {
    let converted = output_color(Color::new(0.16, 0.18, 0.20, 0.5), true);

    assert!((converted.r - srgb_to_linear(0.16)).abs() < 0.0001);
    assert!((converted.g - srgb_to_linear(0.18)).abs() < 0.0001);
    assert!((converted.b - srgb_to_linear(0.20)).abs() < 0.0001);
    assert_eq!(converted.a, 0.5);
}

#[test]
fn non_srgb_output_leaves_authored_color_unchanged() {
    let color = Color::new(0.16, 0.18, 0.20, 0.5);

    assert_eq!(output_color(color, false).to_array(), color.to_array());
}

#[test]
fn image_contain_preserves_aspect_by_shrinking_draw_rect() {
    let frame = LayoutRect::new(10.0, 20.0, 100.0, 100.0);
    let (rect, uv) = image_rect_and_uv(frame, ImageFit::Contain, [200, 100]).unwrap();

    assert_eq!(rect, LayoutRect::new(10.0, 45.0, 100.0, 50.0));
    assert_eq!(uv, [0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn image_cover_preserves_bounds_and_crops_uv() {
    let frame = LayoutRect::new(0.0, 0.0, 100.0, 100.0);
    let (rect, uv) = image_rect_and_uv(frame, ImageFit::Cover, [200, 100]).unwrap();

    assert_eq!(rect, frame);
    assert_eq!(uv, [0.25, 0.0, 0.75, 1.0]);
}

#[test]
fn render_ops_keep_text_before_later_panel_rects() {
    let draw_list = UiDrawList::new(vec![
        UiDrawCommand::Rect(rect_draw("page.bg")),
        UiDrawCommand::Text(text_draw("page.text")),
        UiDrawCommand::Rect(rect_draw("dialog.panel")),
    ]);
    let mut rect_vertices = Vec::new();
    let mut polygon_vertices = Vec::new();
    let mut image_vertices = Vec::new();
    let mut primitive_ops = Vec::new();
    let mut text_items = Vec::new();
    let mut image_items = Vec::new();
    let mut render_ops = Vec::new();

    collect_draw_items(
        &draw_list,
        LayoutRect::new(0.0, 0.0, 320.0, 200.0),
        false,
        &FxHashMap::default(),
        &mut rect_vertices,
        &mut polygon_vertices,
        &mut image_vertices,
        &mut primitive_ops,
        &mut text_items,
        &mut image_items,
        &mut render_ops,
    );

    assert!(matches!(render_ops[0], RenderOp::Primitive(0)));
    assert!(matches!(
        render_ops[1],
        RenderOp::Text { start: 0, count: 1 }
    ));
    assert!(matches!(render_ops[2], RenderOp::Primitive(1)));
}

#[test]
fn consecutive_text_draws_stay_batched_for_render_order_efficiency() {
    let draw_list = UiDrawList::new(vec![
        UiDrawCommand::Text(text_draw("page.static")),
        UiDrawCommand::Text(text_draw("page.live")),
    ]);
    let mut rect_vertices = Vec::new();
    let mut polygon_vertices = Vec::new();
    let mut image_vertices = Vec::new();
    let mut primitive_ops = Vec::new();
    let mut text_items = Vec::new();
    let mut image_items = Vec::new();
    let mut render_ops = Vec::new();

    collect_draw_items(
        &draw_list,
        LayoutRect::new(0.0, 0.0, 320.0, 200.0),
        false,
        &FxHashMap::default(),
        &mut rect_vertices,
        &mut polygon_vertices,
        &mut image_vertices,
        &mut primitive_ops,
        &mut text_items,
        &mut image_items,
        &mut render_ops,
    );

    assert_eq!(text_items.len(), 2);
    assert_eq!(text_items[0].id.node_id().as_str(), "page.static");
    assert_eq!(text_items[1].id.node_id().as_str(), "page.live");
    assert!(matches!(
        render_ops.as_slice(),
        [RenderOp::Text { start: 0, count: 2 }]
    ));
}

#[test]
fn text_buffer_cache_admission_requires_reuse_and_rejects_volatile_ids() {
    let mut key_history: FxHashMap<TextBufferKey, TextKeyHistory> = FxHashMap::default();
    let mut identity_history: FxHashMap<TextBufferIdentityKey, TextIdentityHistory> =
        FxHashMap::default();
    let status_id = TextBufferIdentityKey::new("status");
    let counter_id = TextBufferIdentityKey::new("counter");
    let stable = text_key("Ready");

    assert!(!text_buffer_cache_admitted(
        &mut key_history,
        &mut identity_history,
        1,
        &status_id,
        &stable,
    ));
    assert!(text_buffer_cache_admitted(
        &mut key_history,
        &mut identity_history,
        2,
        &status_id,
        &stable,
    ));
    assert!(identity_history.contains_key(&status_id));

    let one = text_key("1");
    let two = text_key("2");
    let three = text_key("3");
    assert!(!text_buffer_cache_admitted(
        &mut key_history,
        &mut identity_history,
        3,
        &counter_id,
        &one,
    ));
    assert!(!text_buffer_cache_admitted(
        &mut key_history,
        &mut identity_history,
        4,
        &counter_id,
        &two,
    ));
    assert!(!text_buffer_cache_admitted(
        &mut key_history,
        &mut identity_history,
        5,
        &counter_id,
        &three,
    ));
    assert!(text_buffer_cache_admitted(
        &mut key_history,
        &mut identity_history,
        6,
        &counter_id,
        &two,
    ));
    assert!(identity_history.contains_key(&counter_id));
}

#[test]
fn shadow_rects_emit_soft_three_layer_glow_before_fill() {
    let mut draw = rect_draw("page.glow");
    draw.radius = 7.0;
    draw.shadow = Shadow {
        enabled: true,
        offset: [0.0, 4.0],
        blur: 10.0,
        spread: 3.0,
        color: Color::new(0.2, 0.8, 0.7, 0.5),
    };
    let draw_list = UiDrawList::new(vec![UiDrawCommand::Rect(draw)]);
    let mut rect_vertices = Vec::new();
    let mut polygon_vertices = Vec::new();
    let mut image_vertices = Vec::new();
    let mut primitive_ops = Vec::new();
    let mut text_items = Vec::new();
    let mut image_items = Vec::new();
    let mut render_ops = Vec::new();

    collect_draw_items(
        &draw_list,
        LayoutRect::new(0.0, 0.0, 320.0, 200.0),
        false,
        &FxHashMap::default(),
        &mut rect_vertices,
        &mut polygon_vertices,
        &mut image_vertices,
        &mut primitive_ops,
        &mut text_items,
        &mut image_items,
        &mut render_ops,
    );

    assert_eq!(primitive_ops.len(), 1);
    assert_eq!(primitive_ops[0].count, 24);
    assert_eq!(rect_vertices.len(), 24);
    assert!((rect_vertices[0].fill[3] - 0.11).abs() < 0.001);
    assert!((rect_vertices[6].fill[3] - 0.17).abs() < 0.001);
    assert!((rect_vertices[12].fill[3] - 0.13).abs() < 0.001);
    assert_eq!(rect_vertices[18].fill[3], 1.0);
    assert_eq!(rect_vertices[0].params[0], 7.0);
    assert_eq!(rect_vertices[6].params[0], 7.0);
    assert_eq!(rect_vertices[12].params[0], 7.0);
}

#[test]
fn blurred_rects_mark_backdrop_capture_and_preserve_blur_amount() {
    let mut draw = rect_draw("page.glass");
    draw.color = Color::new(0.8, 0.9, 1.0, 0.36);
    draw.blur = 18.0;
    let frame = draw.frame;
    let draw_list = UiDrawList::new(vec![UiDrawCommand::Rect(draw)]);
    let mut rect_vertices = Vec::new();
    let mut polygon_vertices = Vec::new();
    let mut image_vertices = Vec::new();
    let mut primitive_ops = Vec::new();
    let mut text_items = Vec::new();
    let mut image_items = Vec::new();
    let mut render_ops = Vec::new();

    collect_draw_items(
        &draw_list,
        LayoutRect::new(0.0, 0.0, 320.0, 200.0),
        false,
        &FxHashMap::default(),
        &mut rect_vertices,
        &mut polygon_vertices,
        &mut image_vertices,
        &mut primitive_ops,
        &mut text_items,
        &mut image_items,
        &mut render_ops,
    );

    assert_eq!(primitive_ops.len(), 1);
    assert!(matches!(primitive_ops[0].kind, PrimitiveKind::Rect));
    assert_eq!(primitive_ops[0].backdrop_frame, frame);
    assert_eq!(primitive_ops[0].backdrop_blur, 18.0);
    assert!(rect_vertices.iter().all(|vertex| vertex.flags[2] == 18.0));
}

#[test]
fn backdrop_blur_lookup_uses_primitive_index_after_text_ops() {
    let normal = PrimitiveOp {
        kind: PrimitiveKind::Rect,
        start: 0,
        count: 6,
        clip: LayoutRect::new(0.0, 0.0, 320.0, 200.0),
        backdrop_frame: LayoutRect::new(0.0, 0.0, 100.0, 40.0),
        backdrop_blur: 0.0,
    };
    let blurred = PrimitiveOp {
        backdrop_blur: 18.0,
        ..normal
    };
    let primitive_ops = [normal, blurred];
    let render_ops = [
        RenderOp::Primitive(0),
        RenderOp::Text { start: 0, count: 1 },
        RenderOp::Primitive(1),
    ];

    assert_eq!(
        backdrop_blur_primitive_index(&primitive_ops, render_ops[2]),
        Some(1)
    );
    assert_eq!(
        backdrop_blur_primitive_index(&primitive_ops, render_ops[1]),
        None
    );
}

#[test]
fn backdrop_capture_rect_expands_by_physical_blur_and_half_scales_texture() {
    let op = PrimitiveOp {
        kind: PrimitiveKind::Rect,
        start: 0,
        count: 6,
        clip: LayoutRect::new(0.0, 0.0, 320.0, 200.0),
        backdrop_frame: LayoutRect::new(10.0, 20.0, 100.0, 40.0),
        backdrop_blur: 18.0,
    };

    let capture = backdrop_capture_rect(op, [320.0, 200.0], [640, 400]).unwrap();
    assert_eq!(capture, [0.0, 4.0, 256.0, 152.0]);

    let texture_size = [
        ((capture[2] as f32) * 0.5).ceil().max(1.0) as u32,
        ((capture[3] as f32) * 0.5).ceil().max(1.0) as u32,
    ];
    assert_eq!(texture_size, [128, 76]);
}

fn rect_draw(id: &str) -> UiRectDraw {
    UiRectDraw {
        id: id.into(),
        frame: LayoutRect::new(0.0, 0.0, 100.0, 40.0),
        color: Color::WHITE,
        gradient: Gradient::default(),
        border: Border::default(),
        shadow: Shadow::default(),
        radius: 0.0,
        blur: 0.0,
        opacity: 1.0,
        transform: Transform::default(),
    }
}

fn text_draw(id: &str) -> UiTextDraw {
    UiTextDraw {
        id: id.into(),
        frame: LayoutRect::new(0.0, 0.0, 100.0, 24.0),
        text: "Text".to_string(),
        font: FontRef::DefaultText,
        font_size: 16.0,
        font_weight: 400,
        color: Color::WHITE,
        max_width: 100.0,
        wrap: false,
        horizontal_align: HorizontalAlign::Left,
        vertical_align: VerticalAlign::Top,
        line_height: 20.0,
        opacity: 1.0,
        transform: Transform::default(),
    }
}

fn text_key(text: &str) -> TextBufferKey {
    TextBufferKey {
        text: text.to_string(),
        font: FontRef::DefaultText,
        font_size: 16.0f32.to_bits(),
        font_weight: 400,
        line_height: 20.0f32.to_bits(),
        width: 100.0f32.to_bits(),
        height: 24.0f32.to_bits(),
        wrap: false,
        horizontal_align: HorizontalAlign::Left,
    }
}
