//! 2D lighting data types.

use crate::render::Color;

/// 2D point light definition.
#[derive(Debug, Clone, Copy)]
pub struct Light2D {
    pub position: [f32; 2],
    pub color: Color,
    pub radius: f32,
    pub intensity: f32,
    pub temperature: f32,
    pub falloff: f32,
}

impl Light2D {
    pub fn new(x: f32, y: f32, radius: f32) -> Self {
        Self {
            position: [x, y],
            color: Color::WHITE,
            radius,
            intensity: 1.0,
            temperature: 6500.0,
            falloff: 2.0,
        }
    }

    #[inline]
    pub fn temperature(mut self, kelvin: f32) -> Self {
        self.temperature = kelvin;
        self
    }

    #[inline]
    pub fn intensity(mut self, intensity: f32) -> Self {
        self.intensity = intensity;
        self
    }

    #[inline]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    #[inline]
    pub fn falloff(mut self, falloff: f32) -> Self {
        self.falloff = falloff;
        self
    }

    pub fn effective_color(&self) -> [f32; 4] {
        let temp = color_temperature(self.temperature);
        [
            self.color.r * temp[0] * self.intensity,
            self.color.g * temp[1] * self.intensity,
            self.color.b * temp[2] * self.intensity,
            self.color.a,
        ]
    }
}

/// Approximate Planckian-locus RGB from Kelvin temperature.
pub fn color_temperature(kelvin: f32) -> [f32; 3] {
    let t = kelvin.clamp(1000.0, 15000.0) / 100.0;

    let (r, g, b) = if t <= 66.0 {
        let g = 0.390_081_58 * t.ln() - 0.631_841_4;
        let b = if t <= 19.0 {
            0.0
        } else {
            0.543_206_8 * (t - 10.0).ln() - 1.196_254_1
        };
        (1.0, g, b)
    } else {
        (
            1.292_936_2 * (t - 60.0).powf(-0.133_204_76),
            1.129_890_9 * (t - 60.0).powf(-0.075_514_846),
            1.0,
        )
    };

    [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_is_clamped_and_normalized() {
        let warm = color_temperature(1000.0);
        let neutral = color_temperature(6500.0);
        let cold = color_temperature(15000.0);

        for color in [warm, neutral, cold] {
            assert!(color
                .into_iter()
                .all(|channel| (0.0..=1.0).contains(&channel)));
        }

        assert!(warm[0] >= neutral[0]);
        assert!(cold[2] >= neutral[2]);
    }
}
