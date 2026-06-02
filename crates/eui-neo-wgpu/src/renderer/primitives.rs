#![allow(unused_imports)]
use super::backdrop::*;
use super::buffers::*;
use super::collect::*;
use super::images::*;
use super::text::*;
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn render_ordered_ops(
    renderer: &WgpuRenderer,
    ctx: &mut Target<'_>,
    primitive_ops: &[PrimitiveOp],
    render_ops: &[RenderOp],
    rect_upload: Option<&WgpuVertexBuffer>,
    polygon_upload: Option<&WgpuVertexBuffer>,
    image_upload: Option<&WgpuVertexBuffer>,
    image_items: &[ImageItem],
    logical_size: [f32; 2],
) {
    let mut text_layer_index = 0usize;
    let mut segment_start = 0usize;

    for (index, render_op) in render_ops.iter().copied().enumerate() {
        let Some(primitive_index) = backdrop_blur_primitive_index(primitive_ops, render_op) else {
            continue;
        };

        if segment_start < index {
            render_mixed_ops(
                renderer,
                ctx,
                primitive_ops,
                &render_ops[segment_start..index],
                rect_upload,
                polygon_upload,
                image_upload,
                image_items,
                logical_size,
                &mut text_layer_index,
            );
        }

        let op = primitive_ops[primitive_index];
        let backdrop = capture_backdrop(renderer, ctx, op, logical_size);
        render_primitive_ops_batch(
            renderer,
            ctx,
            primitive_ops,
            &render_ops[index..index + 1],
            rect_upload,
            polygon_upload,
            image_upload,
            image_items,
            logical_size,
            backdrop.as_ref().map(|backdrop| &backdrop.bind_group),
        );
        segment_start = index + 1;
    }

    if segment_start < render_ops.len() {
        render_mixed_ops(
            renderer,
            ctx,
            primitive_ops,
            &render_ops[segment_start..],
            rect_upload,
            polygon_upload,
            image_upload,
            image_items,
            logical_size,
            &mut text_layer_index,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_mixed_ops(
    renderer: &WgpuRenderer,
    ctx: &mut Target<'_>,
    primitive_ops: &[PrimitiveOp],
    render_ops: &[RenderOp],
    rect_upload: Option<&WgpuVertexBuffer>,
    polygon_upload: Option<&WgpuVertexBuffer>,
    image_upload: Option<&WgpuVertexBuffer>,
    image_items: &[ImageItem],
    logical_size: [f32; 2],
    text_layer_index: &mut usize,
) {
    if render_ops.is_empty() {
        return;
    }

    let physical_size = ctx.physical_size;
    let color_attachment = Some(wgpu::RenderPassColorAttachment {
        view: ctx.view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Load,
            store: wgpu::StoreOp::Store,
        },
    });
    let color_attachments = [color_attachment];
    let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("eui_neo_overlay_mixed"),
        color_attachments: &color_attachments,
        depth_stencil_attachment: None,
        ..Default::default()
    });
    let mut active_kind: Option<PrimitiveKind> = None;
    let rect_backdrop_bind_group = &renderer.dummy_backdrop.bind_group;

    for render_op in render_ops {
        match *render_op {
            RenderOp::Primitive(index) => {
                let Some(op) = primitive_ops.get(index) else {
                    continue;
                };
                let Some((x, y, width, height)) =
                    scissor_rect(op.clip, logical_size, physical_size)
                else {
                    continue;
                };
                pass.set_scissor_rect(x, y, width, height);
                match op.kind {
                    PrimitiveKind::Rect => {
                        let Some(upload) = rect_upload else {
                            continue;
                        };
                        if !matches!(active_kind, Some(PrimitiveKind::Rect)) {
                            pass.set_pipeline(&renderer.wgpu_resources.rect_pipeline);
                            pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
                            pass.set_bind_group(1, rect_backdrop_bind_group, &[]);
                            pass.set_vertex_buffer(0, upload.slice());
                            active_kind = Some(op.kind);
                        }
                    }
                    PrimitiveKind::Polygon => {
                        let Some(upload) = polygon_upload else {
                            continue;
                        };
                        if !matches!(active_kind, Some(PrimitiveKind::Polygon)) {
                            pass.set_pipeline(&renderer.wgpu_resources.polygon_pipeline);
                            pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
                            pass.set_vertex_buffer(0, upload.slice());
                            active_kind = Some(PrimitiveKind::Polygon);
                        }
                    }
                    PrimitiveKind::Image { image_index } => {
                        let (Some(upload), Some(image_item)) =
                            (image_upload, image_items.get(image_index))
                        else {
                            continue;
                        };
                        let Some(cached) = renderer.image_cache.get(&image_item.image) else {
                            continue;
                        };
                        if !matches!(active_kind, Some(PrimitiveKind::Image { .. })) {
                            pass.set_pipeline(&renderer.wgpu_resources.image_pipeline);
                            pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
                            pass.set_vertex_buffer(0, upload.slice());
                            active_kind = Some(PrimitiveKind::Image { image_index });
                        }
                        pass.set_bind_group(1, &cached.bind_group, &[]);
                    }
                }
                pass.draw(op.start..op.start + op.count, 0..1);
            }
            RenderOp::Text { .. } => {
                active_kind = None;
                pass.set_scissor_rect(0, 0, physical_size[0].max(1), physical_size[1].max(1));
                if let Some(layer) = renderer.text_layers.get(*text_layer_index) {
                    if let Err(error) =
                        layer
                            .renderer
                            .render(&renderer.atlas, &renderer.viewport, &mut pass)
                    {
                        eprintln!("[eui-neo-wgpu] text render failed: {error}");
                    }
                }
                *text_layer_index += 1;
            }
        }
    }
}

pub(super) fn backdrop_blur_primitive_index(
    primitive_ops: &[PrimitiveOp],
    render_op: RenderOp,
) -> Option<usize> {
    let RenderOp::Primitive(index) = render_op else {
        return None;
    };
    primitive_ops
        .get(index)
        .is_some_and(|op| matches!(op.kind, PrimitiveKind::Rect) && op.backdrop_blur > 0.0)
        .then_some(index)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_primitive_ops_batch(
    renderer: &WgpuRenderer,
    ctx: &mut Target<'_>,
    primitive_ops: &[PrimitiveOp],
    render_ops: &[RenderOp],
    rect_upload: Option<&WgpuVertexBuffer>,
    polygon_upload: Option<&WgpuVertexBuffer>,
    image_upload: Option<&WgpuVertexBuffer>,
    image_items: &[ImageItem],
    logical_size: [f32; 2],
    backdrop_bind_group: Option<&wgpu::BindGroup>,
) {
    if render_ops.is_empty() {
        return;
    }
    let physical_size = ctx.physical_size;
    let color_attachment = Some(wgpu::RenderPassColorAttachment {
        view: ctx.view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Load,
            store: wgpu::StoreOp::Store,
        },
    });
    let color_attachments = [color_attachment];
    let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("eui_neo_overlay"),
        color_attachments: &color_attachments,
        depth_stencil_attachment: None,
        ..Default::default()
    });
    let mut active_kind: Option<PrimitiveKind> = None;
    let rect_backdrop_bind_group =
        backdrop_bind_group.unwrap_or(&renderer.dummy_backdrop.bind_group);
    for op in render_ops {
        let RenderOp::Primitive(index) = *op else {
            continue;
        };
        let Some(op) = primitive_ops.get(index) else {
            continue;
        };
        let Some((x, y, width, height)) = scissor_rect(op.clip, logical_size, physical_size) else {
            continue;
        };
        pass.set_scissor_rect(x, y, width, height);
        match op.kind {
            PrimitiveKind::Rect => {
                let Some(upload) = rect_upload else {
                    continue;
                };
                if !matches!(active_kind, Some(PrimitiveKind::Rect)) {
                    pass.set_pipeline(&renderer.wgpu_resources.rect_pipeline);
                    pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
                    pass.set_bind_group(1, rect_backdrop_bind_group, &[]);
                    pass.set_vertex_buffer(0, upload.slice());
                    active_kind = Some(op.kind);
                }
            }
            PrimitiveKind::Polygon => {
                let Some(upload) = polygon_upload else {
                    continue;
                };
                if !matches!(active_kind, Some(PrimitiveKind::Polygon)) {
                    pass.set_pipeline(&renderer.wgpu_resources.polygon_pipeline);
                    pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
                    pass.set_vertex_buffer(0, upload.slice());
                    active_kind = Some(PrimitiveKind::Polygon);
                }
            }
            PrimitiveKind::Image { image_index } => {
                let (Some(upload), Some(image_item)) = (image_upload, image_items.get(image_index))
                else {
                    continue;
                };
                let Some(cached) = renderer.image_cache.get(&image_item.image) else {
                    continue;
                };
                if !matches!(active_kind, Some(PrimitiveKind::Image { .. })) {
                    pass.set_pipeline(&renderer.wgpu_resources.image_pipeline);
                    pass.set_bind_group(0, &renderer.wgpu_resources.screen_bind_group, &[]);
                    pass.set_vertex_buffer(0, upload.slice());
                    active_kind = Some(PrimitiveKind::Image { image_index });
                }
                pass.set_bind_group(1, &cached.bind_group, &[]);
            }
        }
        pass.draw(op.start..op.start + op.count, 0..1);
    }
}

pub(super) fn push_rect(
    vertices: &mut Vec<NeoRectVertex>,
    ops: &mut Vec<PrimitiveOp>,
    draw: &UiRectDraw,
    clip: UiClip,
    surface_is_srgb: bool,
) -> bool {
    if draw.frame.width <= 0.0
        || draw.frame.height <= 0.0
        || draw.opacity <= 0.0
        || clip.rect.width <= 0.0
        || clip.rect.height <= 0.0
    {
        return false;
    }

    let start = vertices.len() as u32;
    if shadow_visible(draw) {
        push_rect_shadow_vertices(vertices, draw, clip, surface_is_srgb);
    }
    if draw.color.a > 0.0 {
        push_rect_fill_vertices(vertices, draw, clip, surface_is_srgb);
    }
    let count = vertices.len() as u32 - start;
    if count == 0 {
        return false;
    }
    dump_rect_collect(draw, clip, start, count, &vertices[start as usize..]);
    ops.push(PrimitiveOp {
        kind: PrimitiveKind::Rect,
        start,
        count,
        clip: clip.rect,
        backdrop_frame: draw.frame,
        backdrop_blur: draw.blur.max(0.0),
    });
    true
}

pub(super) fn dump_rect_collect(
    draw: &UiRectDraw,
    clip: UiClip,
    start: u32,
    count: u32,
    vertices: &[NeoRectVertex],
) {
    static FILTER: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let filter = FILTER.get_or_init(|| std::env::var("SKY_NEO_WGPU_DUMP_FILTER").ok());
    let Some(filter) = filter.as_deref() else {
        return;
    };
    if !draw.id.contains(filter) {
        return;
    }

    eprintln!(
        "[eui-neo-wgpu rect] id={} frame=({:.1},{:.1},{:.1},{:.1}) radius={:.1} border={:.1} opacity={:.3} clip=({:.1},{:.1},{:.1},{:.1}) start={} count={}",
        draw.id,
        draw.frame.x,
        draw.frame.y,
        draw.frame.width,
        draw.frame.height,
        draw.radius,
        draw.border.width,
        draw.opacity,
        clip.rect.x,
        clip.rect.y,
        clip.rect.width,
        clip.rect.height,
        start,
        count
    );
    for (i, vertex) in vertices.iter().enumerate() {
        eprintln!(
            "[eui-neo-wgpu rect vertex] id={} i={} pos=({:.1},{:.1}) local=({:.1},{:.1}) rect=({:.1},{:.1},{:.1},{:.1}) clip=({:.1},{:.1},{:.1},{:.1}) params=({:.1},{:.1},{:.3},{:.1})",
            draw.id,
            i,
            vertex.position[0],
            vertex.position[1],
            vertex.local_pos[0],
            vertex.local_pos[1],
            vertex.rect[0],
            vertex.rect[1],
            vertex.rect[2],
            vertex.rect[3],
            vertex.clip_rect[0],
            vertex.clip_rect[1],
            vertex.clip_rect[2],
            vertex.clip_rect[3],
            vertex.params[0],
            vertex.params[1],
            vertex.params[2],
            vertex.params[3]
        );
    }
}

pub(super) fn push_rect_fill_vertices(
    vertices: &mut Vec<NeoRectVertex>,
    draw: &UiRectDraw,
    clip: UiClip,
    surface_is_srgb: bool,
) {
    let gradient_direction = match draw.gradient.direction {
        GradientDirection::Horizontal => 0.0,
        GradientDirection::Vertical => 1.0,
    };
    push_rect_vertices(
        vertices,
        draw,
        draw.frame,
        draw.frame,
        output_color(draw.color, surface_is_srgb),
        output_color(draw.gradient.start, surface_is_srgb),
        output_color(draw.gradient.end, surface_is_srgb),
        output_color(draw.border.color, surface_is_srgb),
        clip,
        [
            draw.radius.max(0.0),
            draw.border.width.max(0.0),
            draw.opacity.clamp(0.0, 1.0),
            draw.gradient.enabled as u8 as f32,
        ],
        [gradient_direction, 0.0, draw.blur.max(0.0), 0.0],
    );
}

pub(super) fn push_rect_shadow_vertices(
    vertices: &mut Vec<NeoRectVertex>,
    draw: &UiRectDraw,
    clip: UiClip,
    surface_is_srgb: bool,
) {
    let blur = draw.shadow.blur.max(1.0);
    let spread = draw.shadow.spread.max(0.0);
    let base_shape = LayoutRect::new(
        draw.frame.x + draw.shadow.offset[0] - spread,
        draw.frame.y + draw.shadow.offset[1] - spread,
        (draw.frame.width + spread * 2.0).max(0.0),
        (draw.frame.height + spread * 2.0).max(0.0),
    );

    let ambient_shape = offset_rect(
        base_shape,
        draw.shadow.offset[0] * 0.15,
        draw.shadow.offset[1] * 0.15,
    );
    push_rect_shadow_layer_vertices(
        vertices,
        draw,
        ambient_shape,
        blur * 1.4,
        0.22,
        clip,
        surface_is_srgb,
    );

    let mid_shape = offset_rect(
        base_shape,
        draw.shadow.offset[0] * 0.65,
        draw.shadow.offset[1] * 0.65,
    );
    push_rect_shadow_layer_vertices(
        vertices,
        draw,
        mid_shape,
        blur * 0.85,
        0.34,
        clip,
        surface_is_srgb,
    );

    push_rect_shadow_layer_vertices(
        vertices,
        draw,
        base_shape,
        blur * 0.38,
        0.26,
        clip,
        surface_is_srgb,
    );
}

pub(super) fn push_rect_shadow_layer_vertices(
    vertices: &mut Vec<NeoRectVertex>,
    draw: &UiRectDraw,
    shape: LayoutRect,
    blur: f32,
    alpha_scale: f32,
    clip: UiClip,
    surface_is_srgb: bool,
) {
    let geometry = expand_rect(shape, blur);
    let color = output_color(
        multiply_alpha(draw.shadow.color, alpha_scale),
        surface_is_srgb,
    );
    push_rect_vertices(
        vertices,
        draw,
        geometry,
        shape,
        color,
        color,
        color,
        output_color(Color::new(0.0, 0.0, 0.0, 0.0), surface_is_srgb),
        clip,
        [
            draw.radius.max(0.0),
            blur,
            draw.opacity.clamp(0.0, 1.0),
            0.0,
        ],
        [0.0, 1.0, 0.0, 0.0],
    );
}

pub(super) fn offset_rect(rect: LayoutRect, x: f32, y: f32) -> LayoutRect {
    LayoutRect::new(rect.x + x, rect.y + y, rect.width, rect.height)
}

pub(super) fn expand_rect(rect: LayoutRect, amount: f32) -> LayoutRect {
    LayoutRect::new(
        rect.x - amount,
        rect.y - amount,
        rect.width + amount * 2.0,
        rect.height + amount * 2.0,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_rect_vertices(
    vertices: &mut Vec<NeoRectVertex>,
    draw: &UiRectDraw,
    geometry: LayoutRect,
    shape: LayoutRect,
    fill: Color,
    gradient_start: Color,
    gradient_end: Color,
    border: Color,
    clip: UiClip,
    params: [f32; 4],
    flags: [f32; 4],
) {
    let x0 = geometry.x;
    let y0 = geometry.y;
    let x1 = geometry.right();
    let y1 = geometry.bottom();
    let rect = [shape.x, shape.y, shape.width, shape.height];
    let fill = fill.to_array();
    let gradient_start = gradient_start.to_array();
    let gradient_end = gradient_end.to_array();
    let border = border.to_array();
    let clip_rect = clip_rect_array(clip);
    let clip_params = clip_params_array(clip);
    let points = [[x0, y0], [x1, y0], [x1, y1], [x0, y0], [x1, y1], [x0, y1]];
    for local in points {
        vertices.push(NeoRectVertex {
            position: transform_point(local, draw.frame, draw.transform),
            local_pos: local,
            rect,
            fill,
            gradient_start,
            gradient_end,
            border,
            params,
            flags,
            clip_rect,
            clip_params,
        });
    }
}

pub(super) fn shadow_visible(draw: &UiRectDraw) -> bool {
    draw.shadow.enabled
        && draw.shadow.color.a > 0.0
        && draw.opacity > 0.0
        && draw.frame.width > 0.0
        && draw.frame.height > 0.0
        && (draw.shadow.blur > 0.0 || draw.shadow.spread > 0.0 || draw.shadow.offset != [0.0, 0.0])
}

pub(super) fn push_polygon(
    vertices: &mut Vec<NeoPolygonVertex>,
    ops: &mut Vec<PrimitiveOp>,
    draw: &UiPolygonDraw,
    clip: UiClip,
    surface_is_srgb: bool,
) -> bool {
    if draw.points.len() < 3 || draw.opacity <= 0.0 || draw.color.a <= 0.0 {
        return false;
    }
    if clip.rect.width <= 0.0 || clip.rect.height <= 0.0 {
        return false;
    }

    let start = vertices.len() as u32;
    let mut color = draw.color;
    color.a *= draw.opacity.clamp(0.0, 1.0);
    let color = output_color(color, surface_is_srgb).to_array();
    let clip_rect = clip_rect_array(clip);
    let clip_params = clip_params_array(clip);
    let origin = draw.points[0];
    for index in 1..draw.points.len() - 1 {
        for point in [origin, draw.points[index], draw.points[index + 1]] {
            let absolute = [draw.frame.x + point[0], draw.frame.y + point[1]];
            vertices.push(NeoPolygonVertex {
                position: transform_point(absolute, draw.frame, draw.transform),
                color,
                clip_rect,
                clip_params,
            });
        }
    }
    ops.push(PrimitiveOp {
        kind: PrimitiveKind::Polygon,
        start,
        count: (vertices.len() as u32) - start,
        clip: clip.rect,
        backdrop_frame: LayoutRect::ZERO,
        backdrop_blur: 0.0,
    });
    true
}

pub(super) fn push_image(
    vertices: &mut Vec<NeoImageVertex>,
    ops: &mut Vec<PrimitiveOp>,
    images: &mut Vec<ImageItem>,
    draw: &UiImageDraw,
    clip: UiClip,
    image_cache: &FxHashMap<ImageRef, CachedNeoImage>,
    surface_is_srgb: bool,
) -> bool {
    if draw.image.is_empty()
        || draw.frame.width <= 0.0
        || draw.frame.height <= 0.0
        || draw.opacity <= 0.0
        || draw.tint.a <= 0.0
        || clip.rect.width <= 0.0
        || clip.rect.height <= 0.0
    {
        return false;
    }

    let Some(image_info) = image_cache.get(&draw.image) else {
        return false;
    };
    let Some((draw_rect, uv_rect)) = image_rect_and_uv(draw.frame, draw.fit, image_info.size)
    else {
        return false;
    };
    let uv_rect = remap_uv_rect(uv_rect, image_info.uv_rect);

    let start = vertices.len() as u32;
    let image_index = images.len();
    let rect = [
        draw.frame.x,
        draw.frame.y,
        draw.frame.width,
        draw.frame.height,
    ];
    let mut tint = draw.tint;
    tint.a *= draw.opacity.clamp(0.0, 1.0);
    let tint = output_color(tint, surface_is_srgb).to_array();
    let params = [draw.radius, draw.opacity.clamp(0.0, 1.0), 0.0, 0.0];
    let clip_rect = clip_rect_array(clip);
    let clip_params = clip_params_array(clip);
    let x0 = draw_rect.x;
    let y0 = draw_rect.y;
    let x1 = draw_rect.right();
    let y1 = draw_rect.bottom();
    let [u0, v0, u1, v1] = uv_rect;
    let points = [
        ([x0, y0], [u0, v0]),
        ([x1, y0], [u1, v0]),
        ([x1, y1], [u1, v1]),
        ([x0, y0], [u0, v0]),
        ([x1, y1], [u1, v1]),
        ([x0, y1], [u0, v1]),
    ];
    for (local, uv) in points {
        vertices.push(NeoImageVertex {
            position: transform_point(local, draw.frame, draw.transform),
            local_pos: local,
            rect,
            uv,
            tint,
            params,
            clip_rect,
            clip_params,
        });
    }
    images.push(ImageItem {
        image: draw.image.clone(),
    });
    ops.push(PrimitiveOp {
        kind: PrimitiveKind::Image { image_index },
        start,
        count: 6,
        clip: clip.rect,
        backdrop_frame: LayoutRect::ZERO,
        backdrop_blur: 0.0,
    });
    true
}

pub(super) fn push_nine_slice(
    vertices: &mut Vec<NeoImageVertex>,
    ops: &mut Vec<PrimitiveOp>,
    images: &mut Vec<ImageItem>,
    draw: &UiNineSliceDraw,
    clip: UiClip,
    image_cache: &FxHashMap<ImageRef, CachedNeoImage>,
    surface_is_srgb: bool,
) -> bool {
    if draw.image.is_empty()
        || draw.frame.width <= 0.0
        || draw.frame.height <= 0.0
        || draw.opacity <= 0.0
        || draw.tint.a <= 0.0
        || clip.rect.width <= 0.0
        || clip.rect.height <= 0.0
    {
        return false;
    }

    let Some(image_info) = image_cache.get(&draw.image) else {
        return false;
    };
    if image_info.size[0] == 0 || image_info.size[1] == 0 {
        return false;
    }

    let source_w = image_info.size[0] as f32;
    let source_h = image_info.size[1] as f32;
    let left_src = draw.slice.left.min(source_w).max(0.0);
    let right_src = draw
        .slice
        .right
        .min((source_w - left_src).max(0.0))
        .max(0.0);
    let top_src = draw.slice.top.min(source_h).max(0.0);
    let bottom_src = draw
        .slice
        .bottom
        .min((source_h - top_src).max(0.0))
        .max(0.0);

    let left_dst = left_src.min(draw.frame.width * 0.5);
    let right_dst = right_src.min((draw.frame.width - left_dst).max(0.0));
    let top_dst = top_src.min(draw.frame.height * 0.5);
    let bottom_dst = bottom_src.min((draw.frame.height - top_dst).max(0.0));

    let x = [
        draw.frame.x,
        draw.frame.x + left_dst,
        draw.frame.right() - right_dst,
        draw.frame.right(),
    ];
    let y = [
        draw.frame.y,
        draw.frame.y + top_dst,
        draw.frame.bottom() - bottom_dst,
        draw.frame.bottom(),
    ];
    let u = [
        0.0,
        left_src / source_w,
        (source_w - right_src) / source_w,
        1.0,
    ];
    let v = [
        0.0,
        top_src / source_h,
        (source_h - bottom_src) / source_h,
        1.0,
    ];

    let start = vertices.len() as u32;
    let image_index = images.len();
    let rect = [
        draw.frame.x,
        draw.frame.y,
        draw.frame.width,
        draw.frame.height,
    ];
    let mut tint = draw.tint;
    tint.a *= draw.opacity.clamp(0.0, 1.0);
    let tint = output_color(tint, surface_is_srgb).to_array();
    let params = [0.0, draw.opacity.clamp(0.0, 1.0), 0.0, 0.0];
    let clip_rect = clip_rect_array(clip);
    let clip_params = clip_params_array(clip);

    for row in 0..3 {
        for column in 0..3 {
            let quad = LayoutRect::new(
                x[column],
                y[row],
                x[column + 1] - x[column],
                y[row + 1] - y[row],
            );
            if quad.width <= 0.0 || quad.height <= 0.0 {
                continue;
            }
            let uv_rect = remap_uv_rect(
                [u[column], v[row], u[column + 1], v[row + 1]],
                image_info.uv_rect,
            );
            push_image_quad_vertices(
                vertices,
                draw.frame,
                draw.transform,
                quad,
                rect,
                uv_rect,
                tint,
                params,
                clip_rect,
                clip_params,
            );
        }
    }

    let count = vertices.len() as u32 - start;
    if count == 0 {
        return false;
    }
    images.push(ImageItem {
        image: draw.image.clone(),
    });
    ops.push(PrimitiveOp {
        kind: PrimitiveKind::Image { image_index },
        start,
        count,
        clip: clip.rect,
        backdrop_frame: LayoutRect::ZERO,
        backdrop_blur: 0.0,
    });
    true
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_image_quad_vertices(
    vertices: &mut Vec<NeoImageVertex>,
    frame: LayoutRect,
    transform: Transform,
    quad: LayoutRect,
    rect: [f32; 4],
    uv_rect: [f32; 4],
    tint: [f32; 4],
    params: [f32; 4],
    clip_rect: [f32; 4],
    clip_params: [f32; 4],
) {
    let x0 = quad.x;
    let y0 = quad.y;
    let x1 = quad.right();
    let y1 = quad.bottom();
    let [u0, v0, u1, v1] = uv_rect;
    let points = [
        ([x0, y0], [u0, v0]),
        ([x1, y0], [u1, v0]),
        ([x1, y1], [u1, v1]),
        ([x0, y0], [u0, v0]),
        ([x1, y1], [u1, v1]),
        ([x0, y1], [u0, v1]),
    ];
    for (local, uv) in points {
        vertices.push(NeoImageVertex {
            position: transform_point(local, frame, transform),
            local_pos: local,
            rect,
            uv,
            tint,
            params,
            clip_rect,
            clip_params,
        });
    }
}

pub(super) fn clip_rect_array(clip: UiClip) -> [f32; 4] {
    [clip.rect.x, clip.rect.y, clip.rect.width, clip.rect.height]
}

pub(super) fn clip_params_array(clip: UiClip) -> [f32; 4] {
    [clip.radius.max(0.0), 0.0, 0.0, 0.0]
}

pub(super) fn remap_uv_rect(local: [f32; 4], visible: [f32; 4]) -> [f32; 4] {
    let [u0, v0, u1, v1] = visible;
    let width = u1 - u0;
    let height = v1 - v0;
    [
        u0 + local[0] * width,
        v0 + local[1] * height,
        u0 + local[2] * width,
        v0 + local[3] * height,
    ]
}

pub(super) fn image_rect_and_uv(
    frame: LayoutRect,
    fit: ImageFit,
    texture_size: [u32; 2],
) -> Option<(LayoutRect, [f32; 4])> {
    if frame.width <= 0.0 || frame.height <= 0.0 || texture_size[0] == 0 || texture_size[1] == 0 {
        return None;
    }

    let image_aspect = texture_size[0] as f32 / texture_size[1] as f32;
    let rect_aspect = frame.width / frame.height;
    let mut rect = frame;
    let mut uv = [0.0, 0.0, 1.0, 1.0];

    match fit {
        ImageFit::Stretch => {}
        ImageFit::Contain => {
            if image_aspect > rect_aspect {
                rect.height = frame.width / image_aspect;
                rect.y = frame.y + (frame.height - rect.height) * 0.5;
            } else if image_aspect < rect_aspect {
                rect.width = frame.height * image_aspect;
                rect.x = frame.x + (frame.width - rect.width) * 0.5;
            }
        }
        ImageFit::Cover => {
            if image_aspect > rect_aspect {
                let visible = (rect_aspect / image_aspect).clamp(0.0, 1.0);
                uv[0] = (1.0 - visible) * 0.5;
                uv[2] = 1.0 - uv[0];
            } else if image_aspect < rect_aspect {
                let visible = (image_aspect / rect_aspect).clamp(0.0, 1.0);
                uv[1] = (1.0 - visible) * 0.5;
                uv[3] = 1.0 - uv[1];
            }
        }
    }

    Some((rect, uv))
}

pub(super) fn push_text(
    text_items: &mut Vec<TextItem>,
    draw: &UiTextDraw,
    clip: LayoutRect,
    surface_is_srgb: bool,
) -> bool {
    if draw.text.is_empty()
        || draw.opacity <= 0.0
        || draw.color.a <= 0.0
        || draw.frame.width <= 0.0
        || draw.frame.height <= 0.0
        || clip.width <= 0.0
        || clip.height <= 0.0
    {
        return false;
    }
    let frame = transformed_rect(draw.frame, draw.transform);
    let max_width = if draw.max_width > 0.0 {
        draw.max_width.min(frame.width)
    } else {
        frame.width
    };
    let frame = LayoutRect::new(frame.x, frame.y, max_width, frame.height);
    text_items.push(TextItem {
        id: TextBufferIdentityKey::from_node(draw.node_id()),
        text: draw.text.clone(),
        font: draw.font.clone(),
        frame,
        clip,
        color: output_color(multiply_alpha(draw.color, draw.opacity), surface_is_srgb),
        font_size: draw.font_size,
        font_weight: draw.font_weight,
        max_width: draw.max_width,
        wrap: draw.wrap,
        horizontal_align: draw.horizontal_align,
        vertical_align: draw.vertical_align,
        line_height: draw.line_height,
    });
    true
}

pub(super) fn transformed_rect(rect: LayoutRect, transform: Transform) -> LayoutRect {
    let points = [
        transform_point([rect.x, rect.y], rect, transform),
        transform_point([rect.right(), rect.y], rect, transform),
        transform_point([rect.right(), rect.bottom()], rect, transform),
        transform_point([rect.x, rect.bottom()], rect, transform),
    ];
    let mut min_x = points[0][0];
    let mut min_y = points[0][1];
    let mut max_x = points[0][0];
    let mut max_y = points[0][1];
    for point in &points[1..] {
        min_x = min_x.min(point[0]);
        min_y = min_y.min(point[1]);
        max_x = max_x.max(point[0]);
        max_y = max_y.max(point[1]);
    }
    LayoutRect::new(min_x, min_y, max_x - min_x, max_y - min_y)
}

pub(super) fn transform_point(
    point: [f32; 2],
    frame: LayoutRect,
    transform: Transform,
) -> [f32; 2] {
    let origin = [
        frame.x + frame.width * transform.origin[0],
        frame.y + frame.height * transform.origin[1],
    ];
    let scaled_x = (point[0] - origin[0]) * transform.scale[0];
    let scaled_y = (point[1] - origin[1]) * transform.scale[1];
    let cosine = transform.rotation.cos();
    let sine = transform.rotation.sin();
    [
        origin[0] + scaled_x * cosine - scaled_y * sine + transform.translate[0],
        origin[1] + scaled_x * sine + scaled_y * cosine + transform.translate[1],
    ]
}

pub(super) fn scissor_rect(
    rect: LayoutRect,
    logical_size: [f32; 2],
    physical_size: [u32; 2],
) -> Option<(u32, u32, u32, u32)> {
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return None;
    }
    let scale_x = physical_size[0] as f32 / logical_size[0].max(1.0);
    let scale_y = physical_size[1] as f32 / logical_size[1].max(1.0);
    let left = (rect.x * scale_x)
        .floor()
        .clamp(0.0, physical_size[0] as f32) as u32;
    let top = (rect.y * scale_y)
        .floor()
        .clamp(0.0, physical_size[1] as f32) as u32;
    let right = (rect.right() * scale_x)
        .ceil()
        .clamp(0.0, physical_size[0] as f32) as u32;
    let bottom = (rect.bottom() * scale_y)
        .ceil()
        .clamp(0.0, physical_size[1] as f32) as u32;
    (right > left && bottom > top).then_some((left, top, right - left, bottom - top))
}

pub(super) fn intersect_rect(left: LayoutRect, right: LayoutRect) -> Option<LayoutRect> {
    let x0 = left.x.max(right.x);
    let y0 = left.y.max(right.y);
    let x1 = left.right().min(right.right());
    let y1 = left.bottom().min(right.bottom());
    (x1 > x0 && y1 > y0).then(|| LayoutRect::new(x0, y0, x1 - x0, y1 - y0))
}

pub(super) fn intersect_clip(left: UiClip, right: UiClip) -> Option<UiClip> {
    let rect = intersect_rect(left.rect, right.rect)?;
    let radius = if same_rect(rect, right.rect) {
        right.radius
    } else if same_rect(rect, left.rect) {
        left.radius
    } else {
        left.radius.min(right.radius)
    };
    Some(UiClip::new(rect, radius))
}

pub(super) fn same_rect(left: LayoutRect, right: LayoutRect) -> bool {
    (left.x - right.x).abs() <= 0.001
        && (left.y - right.y).abs() <= 0.001
        && (left.width - right.width).abs() <= 0.001
        && (left.height - right.height).abs() <= 0.001
}

pub(super) fn multiply_alpha(mut color: Color, opacity: f32) -> Color {
    color.a *= opacity.clamp(0.0, 1.0);
    color
}

pub(super) fn output_color(color: Color, surface_is_srgb: bool) -> Color {
    if surface_is_srgb {
        Color::new(
            srgb_to_linear(color.r),
            srgb_to_linear(color.g),
            srgb_to_linear(color.b),
            color.a,
        )
    } else {
        color
    }
}

pub(super) fn srgb_to_linear(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub(super) fn packed_color(color: Color) -> u32 {
    u32::from(channel(color.r)) << 24
        | u32::from(channel(color.g)) << 16
        | u32::from(channel(color.b)) << 8
        | u32::from(channel(color.a))
}

pub(super) fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
