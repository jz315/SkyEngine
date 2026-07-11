use crate::render::GlobalIllumination;

pub use super::constants::SSGI_PROVIDER_ID;

/// Screen-space GI controls for the Wicked-inspired provider.
#[derive(Clone, Copy, Debug)]
pub struct SsgiSettings {
    /// Final composite strength. Wicked applies SSGI as a separate indirect term;
    /// `1.0` preserves that energy in SkyEngine's current post-composite path.
    pub intensity: f32,
    /// Public radius hint. The current WGSL pass maps `8.0` to Wicked's narrow
    /// `range = 2, spread = 2` SSGI sampling pass.
    pub radius_pixels: f32,
    /// Wicked's SSGI depth rejection distance; the shader uses its reciprocal.
    pub depth_rejection: f32,
    /// Wicked's bilateral normal threshold for SSGI upsample-style rejection.
    pub normal_power: f32,
}

impl Default for SsgiSettings {
    fn default() -> Self {
        Self {
            intensity: 1.0,
            radius_pixels: 8.0,
            depth_rejection: 8.0,
            normal_power: 64.0,
        }
    }
}

#[inline]
pub fn global_illumination(settings: SsgiSettings) -> GlobalIllumination {
    GlobalIllumination::provider(crate::render::gi::GiProviderConfig::new(
        SSGI_PROVIDER_ID,
        settings,
    ))
}
