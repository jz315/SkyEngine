pub use eui_neo::Color;

impl From<crate::render::Color> for Color {
    #[inline]
    fn from(value: crate::render::Color) -> Self {
        Self::new(value.r, value.g, value.b, value.a)
    }
}

impl From<Color> for crate::render::Color {
    #[inline]
    fn from(value: Color) -> Self {
        crate::render::Color::new(value.r, value.g, value.b, value.a)
    }
}
