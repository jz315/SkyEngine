//! Draw-list generation copied from EUI-NEO's `core/dsl_runtime.h` render traversal.
//!
//! The OpenGL primitives are deliberately not copied. This module keeps the
//! same ordered traversal, clip stack, state-color resolution, and dependent
//! visual transform semantics, then emits backend-neutral draw commands for
//! host renderers.

use super::Color;

use super::{
    Border, CenterMode, EdgeMode, Element, ElementKind, FontRef, Gradient, HorizontalAlign,
    ImageFit, ImageRef, LayoutRect, Runtime, Shadow, Slice, Transform, VerticalAlign,
};

/// Backend-neutral command stream emitted by the neo runtime.
#[derive(Debug, Clone, Default)]
pub struct UiDrawList {
    commands: Vec<UiDrawCommand>,
}

impl UiDrawList {
    pub fn new(commands: Vec<UiDrawCommand>) -> Self {
        Self { commands }
    }

    pub fn commands(&self) -> &[UiDrawCommand] {
        &self.commands
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// Primitive-level draw command stream. `PushClip` / `PopClip` mirror EUI-NEO's
/// scissor composition around each clipped element and its children.
#[derive(Debug, Clone)]
pub enum UiDrawCommand {
    Rect(UiRectDraw),
    Text(UiTextDraw),
    Image(UiImageDraw),
    NineSlice(UiNineSliceDraw),
    Polygon(UiPolygonDraw),
    PushClip(LayoutRect),
    PopClip,
}

#[derive(Debug, Clone)]
pub struct UiRectDraw {
    pub id: String,
    pub frame: LayoutRect,
    pub color: Color,
    pub gradient: Gradient,
    pub border: Border,
    pub shadow: Shadow,
    pub radius: f32,
    pub blur: f32,
    pub opacity: f32,
    pub transform: Transform,
}

#[derive(Debug, Clone)]
pub struct UiTextDraw {
    pub id: String,
    pub frame: LayoutRect,
    pub text: String,
    pub font: FontRef,
    pub font_size: f32,
    pub font_weight: i32,
    pub color: Color,
    pub max_width: f32,
    pub wrap: bool,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,
    pub line_height: f32,
    pub opacity: f32,
    pub transform: Transform,
}

#[derive(Debug, Clone)]
pub struct UiImageDraw {
    pub id: String,
    pub frame: LayoutRect,
    pub image: ImageRef,
    pub fit: ImageFit,
    pub tint: Color,
    pub radius: f32,
    pub opacity: f32,
    pub transform: Transform,
}

#[derive(Debug, Clone)]
pub struct UiNineSliceDraw {
    pub id: String,
    pub frame: LayoutRect,
    pub image: ImageRef,
    pub slice: Slice,
    pub center_mode: CenterMode,
    pub edge_mode: EdgeMode,
    pub tint: Color,
    pub opacity: f32,
    pub transform: Transform,
}

#[derive(Debug, Clone)]
pub struct UiPolygonDraw {
    pub id: String,
    pub frame: LayoutRect,
    pub points: Vec<[f32; 2]>,
    pub color: Color,
    pub opacity: f32,
    pub transform: Transform,
}

#[derive(Debug, Clone, Copy)]
struct RenderTransform {
    active: bool,
    origin: [f32; 2],
    scale: f32,
    translate: [f32; 2],
    opacity: f32,
}

impl Default for RenderTransform {
    fn default() -> Self {
        Self {
            active: false,
            origin: [0.0, 0.0],
            scale: 1.0,
            translate: [0.0, 0.0],
            opacity: 1.0,
        }
    }
}

pub(crate) fn build_draw_list(runtime: &Runtime) -> UiDrawList {
    let mut commands = Vec::new();
    let transform = RenderTransform::default();
    draw_elements(runtime.roots(), runtime, transform, None, &mut commands);
    UiDrawList::new(commands)
}

fn draw_element(
    element: &Element,
    runtime: &Runtime,
    inherited: RenderTransform,
    inherited_clip: Option<LayoutRect>,
    commands: &mut Vec<UiDrawCommand>,
) {
    let render_transform = resolve_render_transform(element, runtime, inherited);
    if render_transform.opacity <= 0.001 {
        return;
    }
    let frame = runtime.animated_frame(element);
    let transform = runtime.animated_transform(element);
    let opacity = runtime.animated_opacity(element);

    let mut active_clip = inherited_clip;
    let mut pushed_clip = false;
    if element.clip {
        let clip_frame = apply_render_transform(frame, render_transform);
        let Some(next_clip) = inherited_clip
            .and_then(|clip| intersect_rect(clip, clip_frame))
            .or_else(|| inherited_clip.is_none().then_some(clip_frame))
        else {
            return;
        };
        active_clip = Some(next_clip);
        commands.push(UiDrawCommand::PushClip(next_clip));
        pushed_clip = true;
    }

    match element.kind {
        ElementKind::Rect => commands.push(UiDrawCommand::Rect(UiRectDraw {
            id: element.id.clone(),
            frame,
            color: runtime.animated_color(element),
            gradient: element.gradient,
            border: runtime.animated_border(element),
            shadow: runtime.animated_shadow(element),
            radius: runtime.animated_radius(element),
            blur: runtime.animated_blur(element),
            opacity: opacity * render_transform.opacity,
            transform: compose_visual_transform(transform, frame, render_transform),
        })),
        ElementKind::Polygon => commands.push(UiDrawCommand::Polygon(UiPolygonDraw {
            id: element.id.clone(),
            frame,
            points: element.polygon_points.clone(),
            color: runtime.animated_color(element),
            opacity: opacity * render_transform.opacity,
            transform: compose_visual_transform(transform, frame, render_transform),
        })),
        ElementKind::Text => commands.push(UiDrawCommand::Text(UiTextDraw {
            id: element.id.clone(),
            frame,
            text: element.text.clone(),
            font: runtime.resolve_font_ref(&element.font),
            font_size: element.font_size,
            font_weight: element.font_weight,
            color: runtime.animated_text_color(element),
            max_width: element.text_max_width,
            wrap: element.wrap,
            horizontal_align: element.horizontal_align,
            vertical_align: element.vertical_align,
            line_height: element.line_height,
            opacity: opacity * render_transform.opacity,
            transform: compose_visual_transform(transform, frame, render_transform),
        })),
        ElementKind::Image => commands.push(UiDrawCommand::Image(UiImageDraw {
            id: element.id.clone(),
            frame,
            image: runtime.resolve_image_ref(&element.image),
            fit: element.image_fit,
            tint: runtime.animated_color(element),
            radius: runtime.animated_radius(element),
            opacity: opacity * render_transform.opacity,
            transform: compose_visual_transform(transform, frame, render_transform),
        })),
        ElementKind::NineSlice => commands.push(UiDrawCommand::NineSlice(UiNineSliceDraw {
            id: element.id.clone(),
            frame,
            image: runtime.resolve_image_ref(&element.image),
            slice: element.slice,
            center_mode: element.center_mode,
            edge_mode: element.edge_mode,
            tint: runtime.animated_color(element),
            opacity: opacity * render_transform.opacity,
            transform: compose_visual_transform(transform, frame, render_transform),
        })),
        ElementKind::Row | ElementKind::Column | ElementKind::Stack => {}
    }

    draw_elements(
        &element.children,
        runtime,
        render_transform,
        active_clip,
        commands,
    );

    if pushed_clip {
        commands.push(UiDrawCommand::PopClip);
    }
}

fn draw_elements(
    elements: &[Element],
    runtime: &Runtime,
    inherited: RenderTransform,
    inherited_clip: Option<LayoutRect>,
    commands: &mut Vec<UiDrawCommand>,
) {
    if elements.len() <= 1 {
        for element in elements {
            draw_element(element, runtime, inherited, inherited_clip, commands);
        }
        return;
    }

    if z_order_is_stable(elements) {
        for element in elements {
            draw_element(element, runtime, inherited, inherited_clip, commands);
        }
        return;
    }

    let mut order: Vec<usize> = (0..elements.len()).collect();
    order.sort_by_key(|&index| (elements[index].z_index, index));
    for index in order {
        draw_element(
            &elements[index],
            runtime,
            inherited,
            inherited_clip,
            commands,
        );
    }
}

fn z_order_is_stable(elements: &[Element]) -> bool {
    elements
        .windows(2)
        .all(|pair| pair[0].z_index <= pair[1].z_index)
}

fn resolve_render_transform(
    element: &Element,
    runtime: &Runtime,
    inherited: RenderTransform,
) -> RenderTransform {
    let mut result = inherited;

    if matches!(
        element.kind,
        ElementKind::Row | ElementKind::Column | ElementKind::Stack
    ) {
        let local = runtime.animated_transform(element);
        let opacity = runtime.animated_opacity(element);
        let has_transform = !is_identity_transform(local);
        let has_opacity = !close_enough(opacity, 1.0);
        if has_transform || has_opacity {
            let frame = apply_render_transform(runtime.animated_frame(element), result);
            result.active = true;
            result.origin = [
                frame.x + frame.width * local.origin[0],
                frame.y + frame.height * local.origin[1],
            ];
            result.scale *= (local.scale[0] + local.scale[1]) * 0.5;
            result.translate[0] += local.translate[0];
            result.translate[1] += local.translate[1];
            result.opacity *= opacity;
        }
    }

    if !element.hover_opacity_source_id.is_empty() {
        let hover = runtime
            .hover_blend_for_source(&element.hover_opacity_source_id)
            .unwrap_or(0.0);
        result.opacity *= lerp(
            element.hover_hidden_opacity,
            element.hover_visible_opacity,
            hover,
        );
    }

    if !element.visual_state_source_id.is_empty() {
        if let Some((press, source_frame)) =
            runtime.press_blend_for_source(&element.visual_state_source_id)
        {
            let scale = 1.0 - (1.0 - element.pressed_scale) * press;
            if !close_enough(scale, 1.0) {
                let frame = apply_render_transform(source_frame, result);
                result.active = true;
                result.origin = [frame.x + frame.width * 0.5, frame.y + frame.height * 0.5];
                result.scale *= scale;
            }
        }
    }

    result
}

fn compose_visual_transform(
    mut transform: Transform,
    frame: LayoutRect,
    inherited: RenderTransform,
) -> Transform {
    if inherited.active && frame.width > 0.0 && frame.height > 0.0 {
        transform.origin = [
            (inherited.origin[0] - frame.x) / frame.width,
            (inherited.origin[1] - frame.y) / frame.height,
        ];
        transform.scale[0] *= inherited.scale;
        transform.scale[1] *= inherited.scale;
        transform.translate[0] += inherited.translate[0];
        transform.translate[1] += inherited.translate[1];
    }
    transform
}

fn apply_render_transform(rect: LayoutRect, transform: RenderTransform) -> LayoutRect {
    if !transform.active {
        return rect;
    }
    let top_left = apply_render_transform_point([rect.x, rect.y], transform);
    LayoutRect::new(
        top_left[0],
        top_left[1],
        rect.width * transform.scale,
        rect.height * transform.scale,
    )
}

fn apply_render_transform_point(point: [f32; 2], transform: RenderTransform) -> [f32; 2] {
    if !transform.active {
        return point;
    }
    [
        transform.origin[0]
            + (point[0] - transform.origin[0]) * transform.scale
            + transform.translate[0],
        transform.origin[1]
            + (point[1] - transform.origin[1]) * transform.scale
            + transform.translate[1],
    ]
}

fn intersect_rect(left: LayoutRect, right: LayoutRect) -> Option<LayoutRect> {
    let x0 = left.x.max(right.x);
    let y0 = left.y.max(right.y);
    let x1 = left.right().min(right.right());
    let y1 = left.bottom().min(right.bottom());
    (x1 > x0 && y1 > y0).then(|| LayoutRect::new(x0, y0, x1 - x0, y1 - y0))
}

fn lerp(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount.clamp(0.0, 1.0)
}

fn is_identity_transform(transform: Transform) -> bool {
    close_enough(transform.translate[0], 0.0)
        && close_enough(transform.translate[1], 0.0)
        && close_enough(transform.scale[0], 1.0)
        && close_enough(transform.scale[1], 1.0)
        && close_enough(transform.rotation, 0.0)
}

fn close_enough(left: f32, right: f32) -> bool {
    (left - right).abs() <= 0.001
}

#[cfg(test)]
mod tests {
    use super::Color;
    use super::{UiDrawCommand, UiRectDraw};
    use crate::{PointerEvent, Runtime, Size};

    fn rect(command: &UiDrawCommand) -> Option<&UiRectDraw> {
        match command {
            UiDrawCommand::Rect(draw) => Some(draw),
            _ => None,
        }
    }

    #[test]
    fn draw_list_preserves_stable_z_order() {
        let mut runtime = Runtime::new("page");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("low").size(10.0, 10.0).z_index(0).build();
            ui.rect("high").size(10.0, 10.0).z_index(5).build();
        });

        let draw = runtime.draw_list();
        let ids: Vec<_> = draw
            .commands()
            .iter()
            .filter_map(rect)
            .map(|draw| draw.id.as_str())
            .collect();

        assert_eq!(ids, ["page.low", "page.high"]);
    }

    #[test]
    fn clipped_element_emits_push_and_pop_clip() {
        let mut runtime = Runtime::new("page");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.stack("root").size(50.0, 50.0).clip().content(|ui| {
                ui.rect("child").size(10.0, 10.0).build();
            });
        });

        let draw = runtime.draw_list();
        assert!(matches!(draw.commands()[0], UiDrawCommand::PushClip(_)));
        assert!(matches!(
            draw.commands().last(),
            Some(UiDrawCommand::PopClip)
        ));
    }

    #[test]
    fn rect_state_color_follows_current_interaction() {
        let mut runtime = Runtime::new("page");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("button")
                .size(20.0, 20.0)
                .states(Color::BLACK, Color::RED, Color::GREEN)
                .build();
        });

        runtime.update_pointer(PointerEvent::at(5.0, 5.0));
        let draw = runtime.draw_list();
        let button = draw.commands().iter().find_map(rect).unwrap();

        assert_eq!(button.color.to_array(), Color::RED.to_array());
    }

    #[test]
    fn visual_state_from_scales_dependents_around_source() {
        let mut runtime = Runtime::new("page");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.stack("root")
                .size(Size::fill(), Size::fill())
                .content(|ui| {
                    ui.rect("button")
                        .position(10.0, 10.0)
                        .size(20.0, 20.0)
                        .states(Color::BLACK, Color::RED, Color::GREEN)
                        .build();
                    ui.rect("chrome")
                        .position(10.0, 10.0)
                        .size(20.0, 20.0)
                        .visual_state_from("button", 0.9)
                        .build();
                });
        });

        runtime.update_pointer(PointerEvent::pressed_at(12.0, 12.0));
        runtime.tick_animations(0.0);
        let draw = runtime.draw_list();
        let chrome = draw
            .commands()
            .iter()
            .filter_map(rect)
            .find(|draw| draw.id == "page.chrome")
            .unwrap();

        assert!((chrome.transform.scale[0] - 0.9).abs() < 0.001);
        assert!((chrome.transform.scale[1] - 0.9).abs() < 0.001);
    }

    #[test]
    fn visual_state_from_waits_for_source_animation_state() {
        let mut runtime = Runtime::new("page");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.stack("root")
                .size(Size::fill(), Size::fill())
                .content(|ui| {
                    ui.rect("button")
                        .position(10.0, 10.0)
                        .size(20.0, 20.0)
                        .states(Color::BLACK, Color::RED, Color::GREEN)
                        .build();
                    ui.rect("chrome")
                        .position(10.0, 10.0)
                        .size(20.0, 20.0)
                        .visual_state_from("button", 0.9)
                        .build();
                });
        });

        runtime.update_pointer(PointerEvent::pressed_at(12.0, 12.0));
        let draw = runtime.draw_list();
        let chrome = draw
            .commands()
            .iter()
            .filter_map(rect)
            .find(|draw| draw.id == "page.chrome")
            .unwrap();

        assert!((chrome.transform.scale[0] - 1.0).abs() < 0.001);
        assert!((chrome.transform.scale[1] - 1.0).abs() < 0.001);
    }

    #[test]
    fn hover_opacity_from_uses_hidden_opacity_without_source_blend() {
        let mut runtime = Runtime::new("page");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.stack("root")
                .size(Size::fill(), Size::fill())
                .content(|ui| {
                    ui.rect("button")
                        .position(10.0, 10.0)
                        .size(20.0, 20.0)
                        .states(Color::BLACK, Color::RED, Color::GREEN)
                        .build();
                    ui.rect("tooltip")
                        .position(10.0, 40.0)
                        .size(20.0, 20.0)
                        .hover_opacity_from("button", 0.25, 1.0)
                        .build();
                });
        });

        runtime.update_pointer(PointerEvent::at(12.0, 12.0));
        let draw = runtime.draw_list();
        let tooltip = draw
            .commands()
            .iter()
            .filter_map(rect)
            .find(|draw| draw.id == "page.tooltip")
            .unwrap();

        assert!((tooltip.opacity - 0.25).abs() < 0.001);
    }

    #[test]
    fn dependent_visual_state_ignores_layout_sources_like_eui() {
        let mut runtime = Runtime::new("page");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.stack("button")
                .position(10.0, 10.0)
                .size(20.0, 20.0)
                .interactive(true)
                .content(|ui| {
                    ui.rect("fill").size(20.0, 20.0).build();
                });
            ui.rect("chrome")
                .position(10.0, 40.0)
                .size(20.0, 20.0)
                .visual_state_from("button", 0.9)
                .hover_opacity_from("button", 0.25, 1.0)
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(12.0, 12.0));
        runtime.tick_animations(0.0);
        let draw = runtime.draw_list();
        let chrome = draw
            .commands()
            .iter()
            .filter_map(rect)
            .find(|draw| draw.id == "page.chrome")
            .unwrap();

        assert!((chrome.transform.scale[0] - 1.0).abs() < 0.001);
        assert!((chrome.opacity - 0.25).abs() < 0.001);
    }
}
