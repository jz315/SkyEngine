use super::super::{EdgeInsets, ElementBuilder, Size};

#[derive(Debug, Clone, Copy)]
pub(crate) struct WidgetLayout {
    pub width: Size,
    pub height: Size,
    pub margin: EdgeInsets,
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub grow: f32,
}

impl WidgetLayout {
    pub(crate) fn new(width: f32, height: f32) -> Self {
        Self {
            width: Size::Fixed(width.max(0.0)),
            height: Size::Fixed(height.max(0.0)),
            margin: EdgeInsets::ZERO,
            min_width: 0.0,
            max_width: 0.0,
            min_height: 0.0,
            max_height: 0.0,
            grow: 0.0,
        }
    }

    pub(crate) fn width(mut self, value: impl Into<Size>) -> Self {
        self.width = value.into();
        self
    }

    pub(crate) fn height(mut self, value: impl Into<Size>) -> Self {
        self.height = value.into();
        self
    }

    pub(crate) fn size(mut self, width: impl Into<Size>, height: impl Into<Size>) -> Self {
        self.width = width.into();
        self.height = height.into();
        self
    }

    pub(crate) fn margin(mut self, value: f32) -> Self {
        self.margin = EdgeInsets::all(value);
        self
    }

    pub(crate) fn margin_xy(mut self, horizontal: f32, vertical: f32) -> Self {
        self.margin = EdgeInsets::symmetric(horizontal, vertical);
        self
    }

    pub(crate) fn margin_each(mut self, left: f32, top: f32, right: f32, bottom: f32) -> Self {
        self.margin = EdgeInsets::new(left, top, right, bottom);
        self
    }

    pub(crate) fn min_width(mut self, value: f32) -> Self {
        self.min_width = value.max(0.0);
        self
    }

    pub(crate) fn max_width(mut self, value: f32) -> Self {
        self.max_width = value.max(0.0);
        self
    }

    pub(crate) fn min_height(mut self, value: f32) -> Self {
        self.min_height = value.max(0.0);
        self
    }

    pub(crate) fn max_height(mut self, value: f32) -> Self {
        self.max_height = value.max(0.0);
        self
    }

    pub(crate) fn grow(mut self, value: f32) -> Self {
        self.grow = value.max(0.0);
        self
    }

    pub(crate) fn fixed_width_or(self, fallback: f32) -> f32 {
        match self.width {
            Size::Fixed(value) => value,
            Size::WrapContent | Size::Fill => fallback,
        }
    }

    pub(crate) fn fixed_height_or(self, fallback: f32) -> f32 {
        match self.height {
            Size::Fixed(value) => value,
            Size::WrapContent | Size::Fill => fallback,
        }
    }

    pub(crate) fn apply_to_size<'ui>(
        self,
        builder: ElementBuilder<'ui>,
        width: impl Into<Size>,
        height: impl Into<Size>,
    ) -> ElementBuilder<'ui> {
        builder
            .size(width, height)
            .margin_each(
                self.margin.left,
                self.margin.top,
                self.margin.right,
                self.margin.bottom,
            )
            .min_width(self.min_width)
            .max_width(self.max_width)
            .min_height(self.min_height)
            .max_height(self.max_height)
            .grow(self.grow)
    }
}

pub(crate) fn scale_size(size: Size, scale: f32, wrap_fallback: f32) -> Size {
    match size {
        Size::Fixed(value) => Size::Fixed(value * scale),
        Size::WrapContent => Size::Fixed(wrap_fallback * scale),
        Size::Fill => Size::Fill,
    }
}
