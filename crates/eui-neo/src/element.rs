use super::fonts::FontRef;
use super::Color;

use super::Transition;

/// 2D point/vector shape copied from EUI-NEO's `core::Vec2`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl From<[f32; 2]> for Vec2 {
    fn from(value: [f32; 2]) -> Self {
        Self {
            x: value[0],
            y: value[1],
        }
    }
}

impl From<Vec2> for [f32; 2] {
    fn from(value: Vec2) -> Self {
        [value.x, value.y]
    }
}

/// Core element kinds ported from EUI-NEO's DSL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementKind {
    Row,
    Column,
    Stack,
    Rect,
    Polygon,
    Text,
    Image,
    NineSlice,
}

/// Main/cross-axis alignment for row, column, and stack layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    Start,
    Center,
    End,
}

/// Horizontal text alignment copied from EUI-NEO's text primitive surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HorizontalAlign {
    Left,
    Center,
    Right,
}

impl Default for HorizontalAlign {
    fn default() -> Self {
        Self::Left
    }
}

/// Vertical text alignment copied from EUI-NEO's text primitive surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerticalAlign {
    Top,
    Center,
    Bottom,
}

impl Default for VerticalAlign {
    fn default() -> Self {
        Self::Top
    }
}

/// Cursor hint for interactive neo elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CursorShape {
    Arrow,
    Hand,
}

impl Default for CursorShape {
    fn default() -> Self {
        Self::Arrow
    }
}

impl Default for Align {
    fn default() -> Self {
        Self::Start
    }
}

/// Logical size mode for the neo layout model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Size {
    Fixed(f32),
    WrapContent,
    Fill,
}

impl Size {
    pub const fn fixed(value: f32) -> Self {
        Self::Fixed(value)
    }

    pub const fn wrap_content() -> Self {
        Self::WrapContent
    }

    pub const fn fill() -> Self {
        Self::Fill
    }
}

impl Default for Size {
    fn default() -> Self {
        Self::WrapContent
    }
}

/// Insets used for margins and padding.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EdgeInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl EdgeInsets {
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    pub fn all(value: f32) -> Self {
        let value = value.max(0.0);
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }

    pub fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal.max(0.0),
            top: vertical.max(0.0),
            right: horizontal.max(0.0),
            bottom: vertical.max(0.0),
        }
    }

    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left: left.max(0.0),
            top: top.max(0.0),
            right: right.max(0.0),
            bottom: bottom.max(0.0),
        }
    }

    pub fn px4(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self::new(left, top, right, bottom)
    }

    pub fn xy(horizontal: f32, vertical: f32) -> Self {
        Self::symmetric(horizontal, vertical)
    }

    pub fn horizontal(self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(self) -> f32 {
        self.top + self.bottom
    }
}

/// Logical rectangle in screen-space UI coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Clip shape used by runtime hit-testing and backend draw-list commands.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UiClip {
    pub rect: LayoutRect,
    pub radius: f32,
}

impl UiClip {
    pub const fn new(rect: LayoutRect, radius: f32) -> Self {
        Self { rect, radius }
    }

    pub const fn rect(rect: LayoutRect) -> Self {
        Self { rect, radius: 0.0 }
    }

    pub fn contains(self, point: [f32; 2]) -> bool {
        if !self.rect.contains(point) {
            return false;
        }
        rounded_rect_contains(self.rect, self.radius, point)
    }
}

fn rounded_rect_contains(rect: LayoutRect, radius: f32, point: [f32; 2]) -> bool {
    let radius = radius.clamp(0.0, rect.width.min(rect.height) * 0.5);
    if radius <= 0.0 {
        return true;
    }

    let inner_left = rect.x + radius;
    let inner_right = rect.right() - radius;
    let inner_top = rect.y + radius;
    let inner_bottom = rect.bottom() - radius;
    if (point[0] >= inner_left && point[0] <= inner_right)
        || (point[1] >= inner_top && point[1] <= inner_bottom)
    {
        return true;
    }

    let cx = if point[0] < inner_left {
        inner_left
    } else {
        inner_right
    };
    let cy = if point[1] < inner_top {
        inner_top
    } else {
        inner_bottom
    };
    let dx = point[0] - cx;
    let dy = point[1] - cy;
    dx * dx + dy * dy <= radius * radius
}

impl LayoutRect {
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

    pub fn contains_xy(self, x: f32, y: f32) -> bool {
        self.contains([x, y])
    }

    pub fn contains_point(self, point: Vec2) -> bool {
        self.contains([point.x, point.y])
    }
}

/// Lightweight 2D transform metadata. Rendering support lands later.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub translate: [f32; 2],
    pub scale: [f32; 2],
    pub rotation: f32,
    pub origin: [f32; 2],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translate: [0.0, 0.0],
            scale: [1.0, 1.0],
            rotation: 0.0,
            origin: [0.5, 0.5],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Border {
    pub width: f32,
    pub color: Color,
}

impl Default for Border {
    fn default() -> Self {
        Self {
            width: 0.0,
            color: Color::WHITE,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Shadow {
    pub enabled: bool,
    pub offset: [f32; 2],
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            enabled: false,
            offset: [0.0, 4.0],
            blur: 8.0,
            spread: 0.0,
            color: Color::new(0.0, 0.0, 0.0, 0.28),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GradientDirection {
    Horizontal,
    Vertical,
}

impl Default for GradientDirection {
    fn default() -> Self {
        Self::Vertical
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Gradient {
    pub enabled: bool,
    pub start: Color,
    pub end: Color,
    pub direction: GradientDirection,
}

impl Default for Gradient {
    fn default() -> Self {
        Self {
            enabled: false,
            start: Color::WHITE,
            end: Color::WHITE,
            direction: GradientDirection::Vertical,
        }
    }
}

/// Accepts both Rust-native `[f32; 2]` point lists and source-shaped `Vec2`
/// point lists for polygon DSL parity.
pub trait IntoPolygonPoints {
    fn into_polygon_points(self) -> Vec<[f32; 2]>;
}

impl IntoPolygonPoints for Vec<[f32; 2]> {
    fn into_polygon_points(self) -> Vec<[f32; 2]> {
        self
    }
}

impl IntoPolygonPoints for &[[f32; 2]] {
    fn into_polygon_points(self) -> Vec<[f32; 2]> {
        self.to_vec()
    }
}

impl<const N: usize> IntoPolygonPoints for [[f32; 2]; N] {
    fn into_polygon_points(self) -> Vec<[f32; 2]> {
        self.into()
    }
}

impl IntoPolygonPoints for Vec<Vec2> {
    fn into_polygon_points(self) -> Vec<[f32; 2]> {
        self.into_iter().map(Into::into).collect()
    }
}

impl IntoPolygonPoints for &[Vec2] {
    fn into_polygon_points(self) -> Vec<[f32; 2]> {
        self.iter().copied().map(Into::into).collect()
    }
}

impl<const N: usize> IntoPolygonPoints for [Vec2; N] {
    fn into_polygon_points(self) -> Vec<[f32; 2]> {
        self.into_iter().map(Into::into).collect()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ImageFit {
    #[default]
    Cover,
    Contain,
    Stretch,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum ImageRefKind {
    #[default]
    Empty,
    Key,
    Path,
    Url,
    Asset,
    BingDaily,
}

/// Stable image reference emitted by UI widgets and resolved by host renderers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ImageRef {
    kind: ImageRefKind,
    source: String,
    flip_vertically: bool,
}

impl ImageRef {
    pub fn new(kind: ImageRefKind, source: impl Into<String>) -> Self {
        let source = normalize_image_source(source.into());
        let kind = if source.is_empty() {
            ImageRefKind::Empty
        } else {
            kind
        };
        Self {
            kind,
            source,
            flip_vertically: false,
        }
    }

    pub fn key(key: impl Into<String>) -> Self {
        Self::new(ImageRefKind::Key, key)
    }

    pub fn path(path: impl Into<String>) -> Self {
        Self::new(ImageRefKind::Path, path)
    }

    pub fn uri(uri: impl Into<String>) -> Self {
        Self::url(uri)
    }

    pub fn url(url: impl Into<String>) -> Self {
        Self::new(ImageRefKind::Url, url)
    }

    pub fn remote(url: impl Into<String>) -> Self {
        Self::url(url)
    }

    pub fn asset(asset: impl Into<String>) -> Self {
        Self::new(ImageRefKind::Asset, asset)
    }

    pub fn bing_daily(idx: i32, mkt: impl AsRef<str>) -> Self {
        Self::new(
            ImageRefKind::BingDaily,
            format!("bing://daily?idx={}&mkt={}", idx.max(0), mkt.as_ref()),
        )
    }

    pub fn with_flip_vertically(mut self, value: bool) -> Self {
        self.flip_vertically = value;
        self
    }

    pub fn flipped(self) -> Self {
        self.with_flip_vertically(true)
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn kind(&self) -> ImageRefKind {
        self.kind
    }

    pub fn is_key(&self) -> bool {
        self.kind == ImageRefKind::Key
    }

    pub fn is_path(&self) -> bool {
        self.kind == ImageRefKind::Path
    }

    pub fn is_url(&self) -> bool {
        self.kind == ImageRefKind::Url
    }

    pub fn is_asset(&self) -> bool {
        self.kind == ImageRefKind::Asset
    }

    pub fn flip_vertically(&self) -> bool {
        self.flip_vertically
    }

    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }
}

impl From<&str> for ImageRef {
    fn from(value: &str) -> Self {
        Self::path(value)
    }
}

impl From<String> for ImageRef {
    fn from(value: String) -> Self {
        Self::path(value)
    }
}

fn normalize_image_source(source: String) -> String {
    source.trim().to_string()
}

pub type Insets = EdgeInsets;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Slice {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Slice {
    pub const ZERO: Self = Self {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };

    pub fn all(value: f32) -> Self {
        let value = value.max(0.0);
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }

    pub fn xy(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal.max(0.0),
            top: vertical.max(0.0),
            right: horizontal.max(0.0),
            bottom: vertical.max(0.0),
        }
    }

    pub fn px4(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        Self {
            left: left.max(0.0),
            top: top.max(0.0),
            right: right.max(0.0),
            bottom: bottom.max(0.0),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum CenterMode {
    #[default]
    Stretch,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum EdgeMode {
    #[default]
    Stretch,
}

/// One node in the declarative neo UI tree.
#[derive(Debug, Clone)]
pub struct Element {
    pub kind: ElementKind,
    pub id: String,

    pub has_x: bool,
    pub has_y: bool,
    pub x: f32,
    pub y: f32,
    pub width: Size,
    pub height: Size,
    pub margin: EdgeInsets,
    pub padding: EdgeInsets,
    pub min_width: f32,
    pub max_layout_width: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub grow: f32,
    pub spacing: f32,
    pub main_align: Align,
    pub cross_align: Align,
    pub frame: LayoutRect,
    pub z_index: i32,
    pub clip: bool,
    pub clip_radius: f32,

    pub color: Color,
    pub gradient: Gradient,
    pub border: Border,
    pub shadow: Shadow,
    pub transform: Transform,
    pub radius: f32,
    pub blur: f32,
    pub opacity: f32,
    pub polygon_points: Vec<[f32; 2]>,

    pub text: String,
    pub font: FontRef,
    pub font_size: f32,
    pub font_weight: i32,
    pub text_color: Color,
    pub text_max_width: f32,
    pub wrap: bool,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,
    pub line_height: f32,

    pub image: ImageRef,
    pub image_fit: ImageFit,
    pub tint: Color,
    pub slice: Slice,
    pub content_inset: Insets,
    pub center_mode: CenterMode,
    pub edge_mode: EdgeMode,

    pub interactive: bool,
    pub focusable: bool,
    pub disabled: bool,
    pub cursor: CursorShape,
    pub has_ime_rect: bool,
    pub ime_rect: LayoutRect,
    pub hover_color: Color,
    pub pressed_color: Color,
    pub has_state_colors: bool,
    pub smooth_state_colors: bool,
    pub visual_state_source_id: String,
    pub hover_opacity_source_id: String,
    pub pressed_scale: f32,
    pub hover_hidden_opacity: f32,
    pub hover_visible_opacity: f32,
    pub transition: Transition,
    pub explicit_frame_animation: bool,
    pub timer_seconds: f32,

    pub children: Vec<Element>,
}

impl Element {
    pub fn new(kind: ElementKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
            has_x: false,
            has_y: false,
            x: 0.0,
            y: 0.0,
            width: Size::WrapContent,
            height: Size::WrapContent,
            margin: EdgeInsets::ZERO,
            padding: EdgeInsets::ZERO,
            min_width: 0.0,
            max_layout_width: 0.0,
            min_height: 0.0,
            max_height: 0.0,
            grow: 0.0,
            spacing: 0.0,
            main_align: Align::Start,
            cross_align: Align::Start,
            frame: LayoutRect::ZERO,
            z_index: 0,
            clip: false,
            clip_radius: 0.0,
            color: Color::WHITE,
            gradient: Gradient::default(),
            border: Border::default(),
            shadow: Shadow::default(),
            transform: Transform::default(),
            radius: 0.0,
            blur: 0.0,
            opacity: 1.0,
            polygon_points: Vec::new(),
            text: String::new(),
            font: FontRef::DefaultText,
            font_size: 16.0,
            font_weight: 400,
            text_color: Color::WHITE,
            text_max_width: 0.0,
            wrap: false,
            horizontal_align: HorizontalAlign::Left,
            vertical_align: VerticalAlign::Top,
            line_height: 0.0,
            image: ImageRef::default(),
            image_fit: ImageFit::Cover,
            tint: Color::WHITE,
            slice: Slice::ZERO,
            content_inset: Insets::ZERO,
            center_mode: CenterMode::Stretch,
            edge_mode: EdgeMode::Stretch,
            interactive: false,
            focusable: false,
            disabled: false,
            cursor: CursorShape::Arrow,
            has_ime_rect: false,
            ime_rect: LayoutRect::ZERO,
            hover_color: Color::WHITE,
            pressed_color: Color::WHITE,
            has_state_colors: false,
            smooth_state_colors: true,
            visual_state_source_id: String::new(),
            hover_opacity_source_id: String::new(),
            pressed_scale: 1.0,
            hover_hidden_opacity: 0.0,
            hover_visible_opacity: 1.0,
            transition: Transition::default(),
            explicit_frame_animation: false,
            timer_seconds: 0.0,
            children: Vec::new(),
        }
    }
}
