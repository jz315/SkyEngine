use crate::asset::{Handle, TextureAsset};
use crate::ecs::EntityId;
use crate::render::Color;

/// Stable UI identifier used by events and game code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiId(String);

impl UiId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for UiId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for UiId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for UiId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Rectangle in logical screen pixels, origin at the top-left corner.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UiRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl UiRect {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn contains(self, point: [f32; 2]) -> bool {
        point[0] >= self.x
            && point[0] <= self.right()
            && point[1] >= self.y
            && point[1] <= self.bottom()
    }

    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = self.right().min(other.right());
        let y1 = self.bottom().min(other.bottom());
        (x1 > x0 && y1 > y0).then(|| Self::new(x0, y0, x1 - x0, y1 - y0))
    }

    pub fn inset(self, padding: UiRect) -> Self {
        let width = (self.width - padding.x - padding.width).max(0.0);
        let height = (self.height - padding.y - padding.height).max(0.0);
        Self {
            x: self.x + padding.x,
            y: self.y + padding.y,
            width,
            height,
        }
    }
}

/// Screen anchor used by [`UiNode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
    Stretch,
}

/// Length value in logical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UiLength {
    Px(f32),
    Percent(f32),
    Fill(f32),
    Auto,
}

impl UiLength {
    pub const fn px(value: f32) -> Self {
        Self::Px(value)
    }

    pub const fn percent(value: f32) -> Self {
        Self::Percent(value)
    }

    pub const fn fill(weight: f32) -> Self {
        Self::Fill(weight)
    }
}

/// Alignment on the cross-axis of a row/column layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAlign {
    Start,
    Center,
    End,
    Stretch,
}

/// Tiny retained layout model for v1 HUD/menu surfaces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UiLayout {
    None,
    Row {
        padding: UiRect,
        gap: f32,
        align: UiAlign,
    },
    Column {
        padding: UiRect,
        gap: f32,
        align: UiAlign,
    },
}

impl UiLayout {
    pub const NONE: Self = Self::None;

    pub const fn row(padding: UiRect, gap: f32, align: UiAlign) -> Self {
        Self::Row {
            padding,
            gap,
            align,
        }
    }

    pub const fn column(padding: UiRect, gap: f32, align: UiAlign) -> Self {
        Self::Column {
            padding,
            gap,
            align,
        }
    }
}

/// Base retained UI component.
#[derive(Debug, Clone)]
pub struct UiNode {
    pub id: Option<UiId>,
    pub parent: Option<EntityId>,
    pub anchor: UiAnchor,
    pub position: [f32; 2],
    pub size: [UiLength; 2],
    pub min_size: [f32; 2],
    /// Local stack offset. Child nodes inherit their parent's effective z.
    pub z: i32,
    pub visible: bool,
    pub enabled: bool,
    pub blocks_input: bool,
    pub layout: UiLayout,
}

impl UiNode {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn panel(width: f32, height: f32) -> Self {
        Self::new().size(width, height)
    }

    pub fn id(mut self, id: impl Into<UiId>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn child_of(mut self, parent: EntityId) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn anchor(mut self, anchor: UiAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    pub fn at(mut self, x: f32, y: f32) -> Self {
        self.position = [x, y];
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = [UiLength::Px(width), UiLength::Px(height)];
        self
    }

    pub fn auto_width(mut self) -> Self {
        self.size[0] = UiLength::Auto;
        self
    }

    pub fn auto_height(mut self) -> Self {
        self.size[1] = UiLength::Auto;
        self
    }

    pub fn width(mut self, width: UiLength) -> Self {
        self.size[0] = width;
        self
    }

    pub fn height(mut self, height: UiLength) -> Self {
        self.size[1] = height;
        self
    }

    pub fn min_size(mut self, width: f32, height: f32) -> Self {
        self.min_size = [width, height];
        self
    }

    pub fn z(mut self, z: i32) -> Self {
        self.z = z;
        self
    }

    pub fn layout(mut self, layout: UiLayout) -> Self {
        self.layout = layout;
        self
    }

    pub fn hidden(mut self) -> Self {
        self.visible = false;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn input_transparent(mut self) -> Self {
        self.blocks_input = false;
        self
    }
}

impl Default for UiNode {
    fn default() -> Self {
        Self {
            id: None,
            parent: None,
            anchor: UiAnchor::TopLeft,
            position: [0.0, 0.0],
            size: [UiLength::Auto, UiLength::Auto],
            min_size: [0.0, 0.0],
            z: 0,
            visible: true,
            enabled: true,
            blocks_input: true,
            layout: UiLayout::None,
        }
    }
}

/// Colored rectangular UI surface.
#[derive(Debug, Clone, Copy)]
pub struct UiPanel {
    pub color: Color,
}

impl UiPanel {
    pub const fn new(color: Color) -> Self {
        Self { color }
    }
}

/// Textured rectangular UI surface.
#[derive(Debug, Clone, Copy)]
pub struct UiImage {
    pub texture: Handle<TextureAsset>,
    pub color: Color,
    pub uv_rect: [f32; 4],
}

impl UiImage {
    pub const fn new(texture: Handle<TextureAsset>) -> Self {
        Self {
            texture,
            color: Color::WHITE,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
        }
    }

    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub const fn uv(mut self, u_min: f32, v_min: f32, u_max: f32, v_max: f32) -> Self {
        self.uv_rect = [u_min, v_min, u_max, v_max];
        self
    }
}

/// Text label component.
#[derive(Debug, Clone)]
pub struct UiText {
    pub text: String,
    pub font_size: f32,
    pub color: Color,
    pub align: UiAlign,
    pub font: Option<String>,
}

impl UiText {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }

    pub fn size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn align(mut self, align: UiAlign) -> Self {
        self.align = align;
        self
    }

    pub fn font(mut self, name: impl Into<String>) -> Self {
        self.font = Some(name.into());
        self
    }
}

impl Default for UiText {
    fn default() -> Self {
        Self {
            text: String::new(),
            font_size: 18.0,
            color: Color::WHITE,
            align: UiAlign::Start,
            font: None,
        }
    }
}

/// Clickable button. A button may also carry a [`UiText`] child/label.
#[derive(Debug, Clone)]
pub struct UiButton {
    pub label: String,
    pub normal_color: Color,
    pub hover_color: Color,
    pub pressed_color: Color,
    pub disabled_color: Color,
    pub text_color: Color,
}

impl UiButton {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Default::default()
        }
    }
}

impl Default for UiButton {
    fn default() -> Self {
        Self {
            label: String::new(),
            normal_color: Color::rgba8(44, 52, 66, 230),
            hover_color: Color::rgba8(74, 92, 120, 240),
            pressed_color: Color::rgba8(35, 43, 56, 245),
            disabled_color: Color::rgba8(40, 40, 44, 150),
            text_color: Color::WHITE,
        }
    }
}

/// Horizontal progress bar.
#[derive(Debug, Clone, Copy)]
pub struct UiProgressBar {
    pub value: f32,
    pub max: f32,
    pub fill_color: Color,
    pub background_color: Color,
}

impl UiProgressBar {
    pub fn new(value: f32, max: f32) -> Self {
        Self {
            value,
            max,
            ..Default::default()
        }
    }

    pub fn fraction(self) -> f32 {
        if self.max <= f32::EPSILON {
            0.0
        } else {
            (self.value / self.max).clamp(0.0, 1.0)
        }
    }
}

impl Default for UiProgressBar {
    fn default() -> Self {
        Self {
            value: 1.0,
            max: 1.0,
            fill_color: Color::rgba8(80, 220, 132, 255),
            background_color: Color::rgba8(18, 24, 30, 220),
        }
    }
}

/// Horizontal value slider.
///
/// `update_ui` updates `value` while the left mouse button is dragging this
/// node and emits [`crate::ui::UiEventKind::ValueChanged`].
#[derive(Debug, Clone, Copy)]
pub struct UiSlider {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub track_height: f32,
    pub thumb_width: f32,
    pub thumb_height: f32,
    pub background_color: Color,
    pub fill_color: Color,
    pub thumb_color: Color,
    pub hover_thumb_color: Color,
    pub pressed_thumb_color: Color,
    pub disabled_color: Color,
}

impl UiSlider {
    pub fn new(value: f32, min: f32, max: f32) -> Self {
        let mut slider = Self {
            min,
            max,
            ..Default::default()
        };
        slider.set_value(value);
        slider
    }

    pub fn with_step(mut self, step: f32) -> Self {
        self.step = step.max(0.0);
        self.set_value(self.value);
        self
    }

    pub fn fraction(self) -> f32 {
        let span = self.max - self.min;
        if span.abs() <= f32::EPSILON {
            0.0
        } else {
            ((self.value - self.min) / span).clamp(0.0, 1.0)
        }
    }

    pub fn set_fraction(&mut self, fraction: f32) -> bool {
        self.set_value(self.min + (self.max - self.min) * fraction.clamp(0.0, 1.0))
    }

    pub fn set_from_x(&mut self, x: f32, rect: UiRect) -> bool {
        if rect.width <= f32::EPSILON {
            return false;
        }
        self.set_fraction((x - rect.x) / rect.width)
    }

    pub fn set_value(&mut self, value: f32) -> bool {
        let lo = self.min.min(self.max);
        let hi = self.min.max(self.max);
        let mut next = value.clamp(lo, hi);
        if self.step > f32::EPSILON {
            let steps = ((next - self.min) / self.step).round();
            next = (self.min + steps * self.step).clamp(lo, hi);
        }
        let changed = (self.value - next).abs() > f32::EPSILON;
        self.value = next;
        changed
    }
}

impl Default for UiSlider {
    fn default() -> Self {
        Self {
            value: 0.0,
            min: 0.0,
            max: 1.0,
            step: 0.0,
            track_height: 8.0,
            thumb_width: 16.0,
            thumb_height: 24.0,
            background_color: Color::rgba8(20, 28, 38, 230),
            fill_color: Color::rgba8(84, 164, 248, 255),
            thumb_color: Color::rgba8(232, 240, 248, 255),
            hover_thumb_color: Color::rgba8(255, 255, 255, 255),
            pressed_thumb_color: Color::rgba8(176, 220, 255, 255),
            disabled_color: Color::rgba8(58, 64, 72, 180),
        }
    }
}

/// Binary switch/checkbox-style setting.
///
/// `update_ui` toggles `checked` when this node is clicked and emits
/// [`crate::ui::UiEventKind::ValueChanged`].
#[derive(Debug, Clone)]
pub struct UiToggle {
    pub checked: bool,
    pub label: String,
    pub track_width: f32,
    pub track_height: f32,
    pub knob_padding: f32,
    pub unchecked_color: Color,
    pub checked_color: Color,
    pub hover_color: Color,
    pub pressed_color: Color,
    pub disabled_color: Color,
    pub knob_color: Color,
    pub text_color: Color,
}

impl UiToggle {
    pub fn new(checked: bool) -> Self {
        Self {
            checked,
            ..Default::default()
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn set_checked(&mut self, checked: bool) -> bool {
        let changed = self.checked != checked;
        self.checked = checked;
        changed
    }

    pub fn toggle(&mut self) -> bool {
        self.checked = !self.checked;
        true
    }
}

impl Default for UiToggle {
    fn default() -> Self {
        Self {
            checked: false,
            label: String::new(),
            track_width: 46.0,
            track_height: 24.0,
            knob_padding: 3.0,
            unchecked_color: Color::rgba8(48, 57, 70, 235),
            checked_color: Color::rgba8(70, 205, 145, 255),
            hover_color: Color::rgba8(78, 92, 112, 240),
            pressed_color: Color::rgba8(55, 125, 100, 245),
            disabled_color: Color::rgba8(50, 52, 58, 165),
            knob_color: Color::rgba8(244, 248, 252, 255),
            text_color: Color::rgba8(232, 240, 248, 255),
        }
    }
}

/// Scroll behavior for a UI node whose children may be larger than its rect.
#[derive(Debug, Clone, Copy)]
pub struct UiScroll {
    pub offset: [f32; 2],
    pub content_size: [f32; 2],
    pub min_content_size: [f32; 2],
    pub wheel_speed: f32,
    pub horizontal: bool,
    pub vertical: bool,
    pub clip: bool,
    pub show_bars: bool,
}

impl UiScroll {
    pub fn vertical() -> Self {
        Self::default()
    }

    pub fn horizontal() -> Self {
        Self {
            horizontal: true,
            vertical: false,
            ..Self::default()
        }
    }

    pub fn both() -> Self {
        Self {
            horizontal: true,
            vertical: true,
            ..Self::default()
        }
    }

    pub fn content_size(mut self, width: f32, height: f32) -> Self {
        self.min_content_size = [width.max(0.0), height.max(0.0)];
        self.content_size = self.min_content_size;
        self
    }

    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset = [x.max(0.0), y.max(0.0)];
        self
    }

    pub fn wheel_speed(mut self, speed: f32) -> Self {
        self.wheel_speed = speed.max(1.0);
        self
    }

    pub fn max_offset(self, viewport: UiRect) -> [f32; 2] {
        [
            if self.horizontal {
                (self.content_size[0] - viewport.width).max(0.0)
            } else {
                0.0
            },
            if self.vertical {
                (self.content_size[1] - viewport.height).max(0.0)
            } else {
                0.0
            },
        ]
    }

    pub fn clamp_offset(&mut self, viewport: UiRect) -> bool {
        let before = self.offset;
        let max_offset = self.max_offset(viewport);
        self.offset[0] = self.offset[0].clamp(0.0, max_offset[0]);
        self.offset[1] = self.offset[1].clamp(0.0, max_offset[1]);
        before != self.offset
    }

    pub fn scroll_by(&mut self, delta: [f32; 2], viewport: UiRect) -> bool {
        let before = self.offset;
        if self.horizontal {
            self.offset[0] += delta[0];
        }
        if self.vertical {
            self.offset[1] += delta[1];
        }
        self.clamp_offset(viewport);
        before != self.offset
    }
}

impl Default for UiScroll {
    fn default() -> Self {
        Self {
            offset: [0.0, 0.0],
            content_size: [0.0, 0.0],
            min_content_size: [0.0, 0.0],
            wheel_speed: 40.0,
            horizontal: false,
            vertical: true,
            clip: true,
            show_bars: true,
        }
    }
}

/// Per-entity interaction state derived from pointer input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiInteraction {
    None,
    Hovered,
    Pressed,
    Disabled,
}
