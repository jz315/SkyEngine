use super::Color;
use std::time::Instant;

use super::DragEvent;
use super::{
    Align, AnimProperty, Border, CenterMode, CursorShape, EdgeInsets, EdgeMode, Element, FontRef,
    Gradient, GradientDirection, HorizontalAlign, ImageFit, ImageRef, Insets, IntoPolygonPoints,
    KeyboardEvent, LayoutRect, MotionPreset, PointerEvent, ScrollEvent, Shadow, Size, Slice,
    Transform, Transition, Ui, VerticalAlign,
};

/// Immediate response returned by neo component/element builders.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Response {
    pub hovered: bool,
    pub pressed: bool,
    pub clicked: bool,
    pub focused: bool,
    pub changed: bool,
}

impl Response {
    pub fn hovered(self) -> bool {
        self.hovered
    }

    pub fn pressed(self) -> bool {
        self.pressed
    }

    pub fn clicked(self) -> bool {
        self.clicked
    }

    pub fn focused(self) -> bool {
        self.focused
    }

    pub fn changed(self) -> bool {
        self.changed
    }
}

/// EUI-style element builder.
pub struct ElementBuilder<'ui> {
    ui: &'ui mut Ui,
    element: Element,
}

impl<'ui> ElementBuilder<'ui> {
    pub(crate) fn new(ui: &'ui mut Ui, element: Element) -> Self {
        Self { ui, element }
    }

    pub fn x(mut self, value: f32) -> Self {
        self.element.has_x = true;
        self.element.x = value;
        self
    }

    pub fn y(mut self, value: f32) -> Self {
        self.element.has_y = true;
        self.element.y = value;
        self
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.element.has_x = true;
        self.element.has_y = true;
        self.element.x = x;
        self.element.y = y;
        self
    }

    pub fn width(mut self, value: impl Into<Size>) -> Self {
        self.element.width = value.into();
        self
    }

    pub fn height(mut self, value: impl Into<Size>) -> Self {
        self.element.height = value.into();
        self
    }

    pub fn size(mut self, width: impl Into<Size>, height: impl Into<Size>) -> Self {
        self.element.width = width.into();
        self.element.height = height.into();
        self
    }

    pub fn fill(mut self) -> Self {
        self.element.width = Size::Fill;
        self.element.height = Size::Fill;
        self
    }

    pub fn wrap_content(mut self) -> Self {
        self.element.width = Size::WrapContent;
        self.element.height = Size::WrapContent;
        self
    }

    pub fn margin(mut self, value: f32) -> Self {
        self.element.margin = EdgeInsets::all(value);
        self
    }

    pub fn margin_xy(mut self, horizontal: f32, vertical: f32) -> Self {
        self.element.margin = EdgeInsets::symmetric(horizontal, vertical);
        self
    }

    pub fn margin_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.element.margin = EdgeInsets::new(left, top, right, bottom);
        self
    }

    pub fn padding(mut self, value: f32) -> Self {
        self.element.padding = EdgeInsets::all(value);
        self
    }

    pub fn padding_xy(mut self, horizontal: f32, vertical: f32) -> Self {
        self.element.padding = EdgeInsets::symmetric(horizontal, vertical);
        self
    }

    pub fn padding_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.element.padding = EdgeInsets::new(left, top, right, bottom);
        self
    }

    pub fn min_width(mut self, value: f32) -> Self {
        self.element.min_width = value.max(0.0);
        self
    }

    pub fn max_width(mut self, value: f32) -> Self {
        let value = value.max(0.0);
        self.element.max_layout_width = value;
        if matches!(self.element.kind, super::ElementKind::Text) {
            self.element.text_max_width = value;
        }
        self
    }

    pub fn min_height(mut self, value: f32) -> Self {
        self.element.min_height = value.max(0.0);
        self
    }

    pub fn max_height(mut self, value: f32) -> Self {
        self.element.max_height = value.max(0.0);
        self
    }

    pub fn grow(mut self, value: f32) -> Self {
        self.element.grow = value.max(0.0);
        self
    }

    pub fn gap(mut self, value: f32) -> Self {
        self.element.spacing = value.max(0.0);
        self
    }

    pub fn spacing(self, value: f32) -> Self {
        self.gap(value)
    }

    pub fn justify_content(mut self, value: Align) -> Self {
        self.element.main_align = value;
        self
    }

    pub fn align_items(mut self, value: Align) -> Self {
        self.element.cross_align = value;
        self
    }

    pub fn align(mut self, main: Align, cross: Align) -> Self {
        self.element.main_align = main;
        self.element.cross_align = cross;
        self
    }

    pub fn z_index(mut self, value: i32) -> Self {
        self.element.z_index = value;
        self
    }

    pub fn z(self, value: i32) -> Self {
        self.z_index(value)
    }

    pub fn clip(mut self) -> Self {
        self.element.clip = true;
        self
    }

    pub fn rounded_clip(mut self, radius: f32) -> Self {
        self.element.clip = true;
        self.element.clip_radius = radius.max(0.0);
        self
    }

    pub fn clip_radius(self, radius: f32) -> Self {
        self.rounded_clip(radius)
    }

    pub fn clip_to_radius(mut self) -> Self {
        self.element.clip = true;
        self.element.clip_radius = self.element.radius;
        self
    }

    pub fn clip_value(mut self, value: bool) -> Self {
        self.element.clip = value;
        self
    }

    pub fn overflow_hidden(self, value: bool) -> Self {
        self.clip_value(value)
    }

    pub fn color(mut self, value: impl Into<Color>) -> Self {
        let value = value.into();
        match self.element.kind {
            super::ElementKind::Text => {
                self.element.text_color = value;
            }
            super::ElementKind::Image | super::ElementKind::NineSlice => {
                self.element.color = value;
                self.element.tint = value;
            }
            _ => {
                self.element.color = value;
            }
        }
        self
    }

    pub fn background(self, value: impl Into<Color>) -> Self {
        self.color(value)
    }

    pub fn gradient(mut self, start: impl Into<Color>, end: impl Into<Color>) -> Self {
        self.element.gradient = Gradient {
            enabled: true,
            start: start.into(),
            end: end.into(),
            direction: GradientDirection::Vertical,
        };
        self
    }

    pub fn gradient_style(mut self, value: Gradient) -> Self {
        self.element.gradient = value;
        self
    }

    pub fn gradient_direction(mut self, direction: GradientDirection) -> Self {
        self.element.gradient.direction = direction;
        self
    }

    pub fn radius(mut self, value: f32) -> Self {
        self.element.radius = value.max(0.0);
        self
    }

    pub fn rounding(self, value: f32) -> Self {
        self.radius(value)
    }

    pub fn border(mut self, width: f32, color: impl Into<Color>) -> Self {
        self.element.border = Border {
            width: width.max(0.0),
            color: color.into(),
        };
        self
    }

    pub fn border_style(mut self, value: Border) -> Self {
        self.element.border = value;
        self
    }

    pub fn blur(mut self, value: f32) -> Self {
        self.element.blur = value.max(0.0);
        self
    }

    pub fn shadow(
        mut self,
        blur: f32,
        offset_x: f32,
        offset_y: f32,
        color: impl Into<Color>,
    ) -> Self {
        self.element.shadow = Shadow {
            enabled: true,
            offset: [offset_x, offset_y],
            blur: blur.max(0.0),
            spread: 0.0,
            color: color.into(),
        };
        self
    }

    pub fn shadow_style(mut self, value: Shadow) -> Self {
        self.element.shadow = value;
        self
    }

    pub fn opacity(mut self, value: f32) -> Self {
        self.element.opacity = value.clamp(0.0, 1.0);
        self
    }

    pub fn translate(mut self, x: f32, y: f32) -> Self {
        self.element.transform.translate = [x, y];
        self
    }

    pub fn translate_x(mut self, value: f32) -> Self {
        self.element.transform.translate[0] = value;
        self
    }

    pub fn translate_y(mut self, value: f32) -> Self {
        self.element.transform.translate[1] = value;
        self
    }

    pub fn scale(mut self, value: f32) -> Self {
        let value = value.max(0.0);
        self.element.transform.scale = [value, value];
        self
    }

    pub fn scale_xy(mut self, x: f32, y: f32) -> Self {
        self.element.transform.scale = [x, y];
        self
    }

    pub fn rotate(mut self, radians: f32) -> Self {
        self.element.transform.rotation = radians;
        self
    }

    pub fn rotation(self, radians: f32) -> Self {
        self.rotate(radians)
    }

    pub fn transform_origin(mut self, x: f32, y: f32) -> Self {
        self.element.transform.origin = [x, y];
        self
    }

    pub fn transform(mut self, transform: Transform) -> Self {
        self.element.transform = transform;
        self
    }

    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.element.text = value.into();
        self
    }

    pub fn icon(mut self, value: impl Into<String>) -> Self {
        self.element.text = value.into();
        self.element.font = FontRef::DefaultIcon;
        self
    }

    pub fn icon_codepoint(self, codepoint: u32) -> Self {
        self.icon(
            char::from_u32(codepoint)
                .map(|value| value.to_string())
                .unwrap_or_default(),
        )
    }

    pub fn font_family(mut self, value: impl Into<String>) -> Self {
        self.element.font = FontRef::family(value);
        self
    }

    pub fn font(mut self, value: impl Into<FontRef>) -> Self {
        self.element.font = value.into();
        self
    }

    pub fn custom_font(self, value: impl Into<String>) -> Self {
        self.font_source(value)
    }

    pub fn font_source(mut self, value: impl Into<String>) -> Self {
        self.element.font = FontRef::source(value);
        self
    }

    pub fn icon_font(mut self) -> Self {
        self.element.font = FontRef::DefaultIcon;
        self
    }

    pub fn font_size(mut self, value: f32) -> Self {
        self.element.font_size = value.max(1.0);
        self
    }

    pub fn font_weight(mut self, value: i32) -> Self {
        self.element.font_weight = value;
        self
    }

    pub fn text_color(mut self, value: impl Into<Color>) -> Self {
        self.element.text_color = value.into();
        self
    }

    pub fn text_colour(self, value: impl Into<Color>) -> Self {
        self.text_color(value)
    }

    pub fn text_max_width(mut self, value: f32) -> Self {
        self.element.text_max_width = value.max(0.0);
        self
    }

    pub fn wrap(mut self, value: bool) -> Self {
        self.element.wrap = value;
        self
    }

    pub fn horizontal_align(mut self, value: HorizontalAlign) -> Self {
        self.element.horizontal_align = value;
        self
    }

    pub fn vertical_align(mut self, value: VerticalAlign) -> Self {
        self.element.vertical_align = value;
        self
    }

    pub fn line_height(mut self, value: f32) -> Self {
        self.element.line_height = value.max(0.0);
        self
    }

    pub fn image(mut self, value: impl Into<ImageRef>) -> Self {
        self.element.image = value.into();
        self
    }

    pub fn image_source(self, value: impl Into<ImageRef>) -> Self {
        self.image(value)
    }

    pub fn source(self, value: impl Into<ImageRef>) -> Self {
        self.image(value)
    }

    pub fn path(self, value: impl Into<ImageRef>) -> Self {
        self.image(value)
    }

    pub fn url(self, value: impl Into<String>) -> Self {
        self.image(ImageRef::remote(value))
    }

    pub fn bing_daily(self, idx: i32, mkt: impl AsRef<str>) -> Self {
        self.image(ImageRef::bing_daily(idx, mkt))
    }

    pub fn image_fit(mut self, value: ImageFit) -> Self {
        self.element.image_fit = value;
        self
    }

    pub fn cover(self) -> Self {
        self.image_fit(ImageFit::Cover)
    }

    pub fn contain(self) -> Self {
        self.image_fit(ImageFit::Contain)
    }

    pub fn stretch(self) -> Self {
        self.image_fit(ImageFit::Stretch)
    }

    pub fn tint(self, value: impl Into<Color>) -> Self {
        self.color(value)
    }

    pub fn slice(mut self, value: Slice) -> Self {
        self.element.slice = value;
        self
    }

    pub fn slice_px4(self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.slice(Slice::px4(left, top, right, bottom))
    }

    pub fn content_inset(mut self, value: Insets) -> Self {
        self.element.content_inset = value;
        self.element.padding = value;
        self
    }

    pub fn content_inset_px4(self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.content_inset(Insets::px4(left, top, right, bottom))
    }

    pub fn center(mut self, value: CenterMode) -> Self {
        self.element.center_mode = value;
        self
    }

    pub fn edges(mut self, value: EdgeMode) -> Self {
        self.element.edge_mode = value;
        self
    }

    pub fn flip_vertically(mut self, value: bool) -> Self {
        self.element.image = self.element.image.with_flip_vertically(value);
        self
    }

    pub fn polygon_points(mut self, points: impl IntoPolygonPoints) -> Self {
        self.element.polygon_points = points.into_polygon_points();
        self
    }

    pub fn points(self, points: impl IntoPolygonPoints) -> Self {
        self.polygon_points(points)
    }

    pub fn point(mut self, x: f32, y: f32) -> Self {
        self.element.polygon_points.push([x, y]);
        self
    }

    pub fn clear_points(mut self) -> Self {
        self.element.polygon_points.clear();
        self
    }

    pub fn interactive(mut self, value: bool) -> Self {
        self.element.interactive = value;
        if value {
            self.element.cursor = CursorShape::Hand;
        }
        self
    }

    pub fn disabled(mut self, value: bool) -> Self {
        self.element.disabled = value;
        self
    }

    pub fn enabled(self, value: bool) -> Self {
        self.disabled(!value)
    }

    pub fn focusable(mut self, value: bool) -> Self {
        self.element.focusable = value;
        if value {
            self.element.interactive = true;
        }
        self
    }

    pub fn ime_rect(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.element.has_ime_rect = true;
        self.element.ime_rect = LayoutRect::new(x, y, width.max(0.0), height.max(0.0));
        self
    }

    pub fn cursor(mut self, value: CursorShape) -> Self {
        self.element.cursor = value;
        self
    }

    pub fn hover_color(mut self, value: impl Into<Color>) -> Self {
        self.element.hover_color = value.into();
        self.element.has_state_colors = true;
        self
    }

    pub fn pressed_color(mut self, value: impl Into<Color>) -> Self {
        self.element.pressed_color = value.into();
        self.element.has_state_colors = true;
        self
    }

    pub fn states(
        mut self,
        normal: impl Into<Color>,
        hover: impl Into<Color>,
        pressed: impl Into<Color>,
    ) -> Self {
        self.element.color = normal.into();
        self.element.hover_color = hover.into();
        self.element.pressed_color = pressed.into();
        self.element.has_state_colors = true;
        self.element.interactive = true;
        self.element.cursor = CursorShape::Hand;
        self
    }

    pub fn smooth_states(mut self, value: bool) -> Self {
        self.element.smooth_state_colors = value;
        self
    }

    pub fn instant_states(self) -> Self {
        self.smooth_states(false)
    }

    pub fn visual_state_from(mut self, id: impl AsRef<str>, pressed_scale: f32) -> Self {
        self.element.visual_state_source_id = self.ui.resolve_id(id.as_ref());
        self.element.pressed_scale = pressed_scale.clamp(0.80, 1.0);
        self
    }

    pub fn hover_opacity_from(
        mut self,
        id: impl AsRef<str>,
        hidden_opacity: f32,
        visible_opacity: f32,
    ) -> Self {
        self.element.hover_opacity_source_id = self.ui.resolve_id(id.as_ref());
        self.element.hover_hidden_opacity = hidden_opacity.clamp(0.0, 1.0);
        self.element.hover_visible_opacity = visible_opacity.clamp(0.0, 1.0);
        self
    }

    pub fn pressed_scale(mut self, value: f32) -> Self {
        self.element.pressed_scale = value.clamp(0.80, 1.0);
        self
    }

    pub fn transition(mut self, transition: Transition) -> Self {
        self.element.transition = transition;
        self
    }

    pub fn motion(mut self, preset: MotionPreset) -> Self {
        self.element.transition = Transition::spring_preset(preset);
        self
    }

    pub fn motion_preset(self, preset: MotionPreset) -> Self {
        self.motion(preset)
    }

    pub fn spring(mut self, response_seconds: f32, damping_ratio: f32) -> Self {
        self.element.transition = Transition::spring(response_seconds, damping_ratio);
        self
    }

    pub fn animate(mut self, properties: AnimProperty) -> Self {
        self.element.transition.enabled = true;
        self.element.transition.properties = properties;
        if properties.contains(AnimProperty::FRAME) {
            self.element.explicit_frame_animation = true;
        }
        self
    }

    pub fn on_click<F>(mut self, callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.element.interactive = true;
        self.element.cursor = CursorShape::Hand;
        self.ui
            .register_on_click(self.element.id.clone(), Box::new(callback));
        self
    }

    pub fn on_press<F>(mut self, callback: F) -> Self
    where
        F: FnMut(PointerEvent, LayoutRect) + 'static,
    {
        self.element.interactive = true;
        self.element.cursor = CursorShape::Hand;
        self.ui
            .register_on_press(self.element.id.clone(), Box::new(callback));
        self
    }

    pub fn on_context_menu<F>(mut self, callback: F) -> Self
    where
        F: FnMut(PointerEvent, LayoutRect) + 'static,
    {
        self.element.interactive = true;
        self.element.cursor = CursorShape::Hand;
        self.ui
            .register_on_context_menu(self.element.id.clone(), Box::new(callback));
        self
    }

    pub fn on_focus_changed<F>(mut self, callback: F) -> Self
    where
        F: FnMut(bool) + 'static,
    {
        self.element.focusable = true;
        self.element.interactive = true;
        self.ui
            .register_on_focus_changed(self.element.id.clone(), Box::new(callback));
        self
    }

    pub fn on_text_input<F>(mut self, callback: F) -> Self
    where
        F: FnMut(KeyboardEvent) + 'static,
    {
        self.element.focusable = true;
        self.element.interactive = true;
        self.ui
            .register_on_text_input(self.element.id.clone(), Box::new(callback));
        self
    }

    pub fn on_scroll<F>(mut self, callback: F) -> Self
    where
        F: FnMut(ScrollEvent) + 'static,
    {
        self.element.interactive = true;
        self.ui
            .register_on_scroll(self.element.id.clone(), Box::new(callback));
        self
    }

    pub fn on_drag<F>(mut self, callback: F) -> Self
    where
        F: FnMut(DragEvent) + 'static,
    {
        self.element.interactive = true;
        self.ui
            .register_on_drag(self.element.id.clone(), Box::new(callback));
        self
    }

    pub fn on_timer<F>(mut self, seconds: f32, callback: F) -> Self
    where
        F: FnMut() + 'static,
    {
        self.element.timer_seconds = seconds.max(0.0);
        self.ui
            .register_on_timer(self.element.id.clone(), Box::new(callback));
        self
    }

    pub fn fit(self, value: ImageFit) -> Self {
        self.image_fit(value)
    }

    pub fn build(self) -> Response {
        let response = self.ui.response(&self.element.id);
        self.ui.push_element(self.element);
        response
    }

    pub fn content(self, content: impl FnOnce(&mut Ui)) -> Response {
        let response = self.ui.response(&self.element.id);
        if self.ui.reuse_retained_element(&self.element.id) {
            return response;
        }
        let build_start = crate::retained::scope_profile_enabled().then(Instant::now);
        let id = self.element.id.clone();
        let index = self.ui.push_element(self.element);
        self.ui.push_path(index);
        let pushed_dirty_owner = self.ui.push_dirty_owner_if_exact_dirty(&id);
        content(self.ui);
        if pushed_dirty_owner {
            self.ui.pop_dirty_owner();
        }
        self.ui.pop_path();
        let build_ms = build_start
            .map(|start| start.elapsed().as_secs_f32() * 1000.0)
            .unwrap_or(0.0);
        self.ui.record_retained_element(id, index, build_ms);
        response
    }
}

impl From<f32> for Size {
    fn from(value: f32) -> Self {
        Self::Fixed(value)
    }
}
