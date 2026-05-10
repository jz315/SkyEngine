//! RGBA color type with common presets and conversions.

/// 32-bit RGBA colour (linear, 0.0-1.0 per channel).
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
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const RED: Self = Self {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const GREEN: Self = Self {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    pub const BLUE: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    pub const YELLOW: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    pub const CYAN: Self = Self {
        r: 0.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const MAGENTA: Self = Self {
        r: 1.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    /// Create a colour from linear 0.0-1.0 RGBA values.
    #[inline]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Create a colour from linear 0.0-1.0 RGBA values.
    #[inline]
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Create a colour from linear 0.0-1.0 RGB values (alpha = 1).
    #[inline]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Create a colour from linear 0-255 RGBA channel values.
    #[inline]
    pub fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Create a colour from a linear hex value `0xRRGGBB`.
    #[inline]
    pub fn hex(v: u32) -> Self {
        Self::rgba8(
            ((v >> 16) & 0xFF) as u8,
            ((v >> 8) & 0xFF) as u8,
            (v & 0xFF) as u8,
            255,
        )
    }

    /// Create a colour from HSL (hue in degrees, saturation/lightness 0-1).
    pub fn hsl(h: f32, s: f32, l: f32) -> Self {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let h2 = h.rem_euclid(360.0) / 60.0;
        let x = c * (1.0 - (h2 % 2.0 - 1.0).abs());
        let (r1, g1, b1) = match h2 as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = l - c * 0.5;
        Self::new(r1 + m, g1 + m, b1 + m, 1.0)
    }

    /// Return as `[f32; 4]` array.
    #[inline]
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Multiply alpha (for pre-multiplied alpha blending).
    #[inline]
    pub fn premultiply(self) -> Self {
        Self {
            r: self.r * self.a,
            g: self.g * self.a,
            b: self.b * self.a,
            a: self.a,
        }
    }

    /// Convert to `wgpu::Color`.
    #[inline]
    pub fn to_wgpu(self) -> wgpu::Color {
        wgpu::Color {
            r: self.r as f64,
            g: self.g as f64,
            b: self.b as f64,
            a: self.a as f64,
        }
    }
}

impl From<[f32; 4]> for Color {
    #[inline]
    fn from(a: [f32; 4]) -> Self {
        Self {
            r: a[0],
            g: a[1],
            b: a[2],
            a: a[3],
        }
    }
}

impl From<Color> for [f32; 4] {
    #[inline]
    fn from(c: Color) -> Self {
        c.to_array()
    }
}

#[cfg(test)]
mod tests {
    use super::Color;

    fn assert_close(lhs: Color, rhs: Color) {
        let lhs = lhs.to_array();
        let rhs = rhs.to_array();
        for (lhs, rhs) in lhs.into_iter().zip(rhs) {
            assert!((lhs - rhs).abs() < 1.0e-6, "{lhs} != {rhs}");
        }
    }

    #[test]
    fn hsl_wraps_hue_degrees() {
        assert_close(Color::hsl(420.0, 1.0, 0.5), Color::hsl(60.0, 1.0, 0.5));
        assert_close(Color::hsl(-60.0, 1.0, 0.5), Color::hsl(300.0, 1.0, 0.5));
    }
}
