//! Explicit linear and sRGB color types.

use core::ops::{Mul, MulAssign};

/// Linear floating-point RGBA color. HDR RGB values are allowed.
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

    /// Compatibility constructor that normalizes bytes without sRGB decoding.
    /// Prefer [`Color::from_linear_rgba8`] or [`Color::from_srgba8`] in new code.
    #[inline]
    pub fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::from_linear_rgba8(r, g, b, a)
    }

    #[inline]
    pub fn from_linear_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Compatibility `0xRRGGBB` constructor without sRGB decoding.
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

    /// Decode sRGB bytes to linear RGB. Alpha is only normalized.
    #[inline]
    pub fn from_srgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Srgba::rgba8(r, g, b, a).to_linear()
    }

    #[inline]
    pub fn to_srgba(self) -> Srgba {
        Srgba::new(
            linear_to_srgb(self.r),
            linear_to_srgb(self.g),
            linear_to_srgb(self.b),
            self.a,
        )
    }

    #[inline]
    pub const fn with_alpha(mut self, alpha: f32) -> Self {
        self.a = alpha;
        self
    }

    #[inline]
    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        Self::new(
            self.r + (rhs.r - self.r) * t,
            self.g + (rhs.g - self.g) * t,
            self.b + (rhs.b - self.b) * t,
            self.a + (rhs.a - self.a) * t,
        )
    }

    #[inline]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self::new(
            self.r.clamp(min.r, max.r),
            self.g.clamp(min.g, max.g),
            self.b.clamp(min.b, max.b),
            self.a.clamp(min.a, max.a),
        )
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

    #[inline]
    pub fn unpremultiply(self) -> Self {
        if self.a.abs() <= f32::EPSILON {
            Self::TRANSPARENT
        } else {
            Self::new(self.r / self.a, self.g / self.a, self.b / self.a, self.a)
        }
    }

    /// Convert to `wgpu::Color`.
    #[cfg(feature = "wgpu")]
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

impl Mul for Color {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::new(
            self.r * rhs.r,
            self.g * rhs.g,
            self.b * rhs.b,
            self.a * rhs.a,
        )
    }
}

impl MulAssign for Color {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}

impl PartialEq for Color {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.r.to_bits() == other.r.to_bits()
            && self.g.to_bits() == other.g.to_bits()
            && self.b.to_bits() == other.b.to_bits()
            && self.a.to_bits() == other.a.to_bits()
    }
}

impl Eq for Color {}

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

/// Non-linear sRGB color with a linear alpha channel.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Srgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Srgba {
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const RED: Self = Self::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Self = Self::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Self = Self::rgb(0.0, 0.0, 1.0);
    pub const YELLOW: Self = Self::rgb(1.0, 1.0, 0.0);
    pub const CYAN: Self = Self::rgb(0.0, 1.0, 1.0);
    pub const MAGENTA: Self = Self::rgb(1.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);

    #[inline]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
    #[inline]
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self::new(r, g, b, a)
    }
    #[inline]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::new(r, g, b, 1.0)
    }

    #[inline]
    pub fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    #[inline]
    pub fn hex(value: u32) -> Self {
        Self::rgba8((value >> 16) as u8, (value >> 8) as u8, value as u8, 255)
    }

    pub fn hsl(h: f32, s: f32, l: f32) -> Self {
        let legacy = Color::hsl(h, s, l);
        Self::new(legacy.r, legacy.g, legacy.b, legacy.a)
    }

    #[inline]
    pub fn to_linear(self) -> Color {
        Color::new(
            srgb_to_linear(self.r),
            srgb_to_linear(self.g),
            srgb_to_linear(self.b),
            self.a,
        )
    }

    #[inline]
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl PartialEq for Srgba {
    fn eq(&self, rhs: &Self) -> bool {
        self.r.to_bits() == rhs.r.to_bits()
            && self.g.to_bits() == rhs.g.to_bits()
            && self.b.to_bits() == rhs.b.to_bits()
            && self.a.to_bits() == rhs.a.to_bits()
    }
}
impl Eq for Srgba {}
impl From<[f32; 4]> for Srgba {
    fn from(v: [f32; 4]) -> Self {
        Self::new(v[0], v[1], v[2], v[3])
    }
}
impl From<Srgba> for [f32; 4] {
    fn from(v: Srgba) -> Self {
        v.to_array()
    }
}

#[inline]
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[inline]
fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::{Color, Srgba};

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

    #[test]
    fn srgb_reference_value_and_round_trip_are_correct() {
        let linear = Srgba::rgb(0.5, 0.5, 0.5).to_linear();
        assert!((linear.r - 0.214_041_14).abs() < 1e-6);
        let round_trip = linear.to_srgba();
        assert!((round_trip.r - 0.5).abs() < 1e-6);
        assert_eq!(round_trip.a, 1.0);
    }

    #[test]
    fn compatibility_bytes_remain_linear_and_premultiply_round_trips() {
        assert_eq!(Color::rgba8(128, 0, 0, 255).r, 128.0 / 255.0);
        let color = Color::new(0.4, 0.2, 0.1, 0.5);
        assert_close(color.premultiply().unpremultiply(), color);
        assert_close(
            Color::BLACK.lerp(Color::WHITE, 0.5),
            Color::new(0.5, 0.5, 0.5, 1.0),
        );
    }
}
