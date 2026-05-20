/// 32-bit RGBA color used by the tile scene model.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    #[inline]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    #[inline]
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    #[inline]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    #[inline]
    pub fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl PartialEq for Color {
    fn eq(&self, other: &Self) -> bool {
        self.r.to_bits() == other.r.to_bits()
            && self.g.to_bits() == other.g.to_bits()
            && self.b.to_bits() == other.b.to_bits()
            && self.a.to_bits() == other.a.to_bits()
    }
}

impl Eq for Color {}

impl From<crate::render::Color> for Color {
    fn from(value: crate::render::Color) -> Self {
        Self::new(value.r, value.g, value.b, value.a)
    }
}

impl From<Color> for crate::render::Color {
    fn from(value: Color) -> Self {
        Self::new(value.r, value.g, value.b, value.a)
    }
}

impl From<[f32; 4]> for Color {
    #[inline]
    fn from(value: [f32; 4]) -> Self {
        Self::new(value[0], value[1], value[2], value[3])
    }
}

impl From<Color> for [f32; 4] {
    #[inline]
    fn from(value: Color) -> Self {
        value.to_array()
    }
}
