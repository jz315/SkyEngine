use super::*;

pub const DDGI_PROVIDER_ID: GiProviderId = "sky.ddgi";

pub(crate) const DDGI_IRRADIANCE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(crate) const DDGI_VISIBILITY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(crate) const DDGI_WORKGROUP_SIZE: u32 = 8;
pub(crate) const DDGI_ATLAS_BORDER_TEXELS: u32 = 1;
pub(crate) const DDGI_SHADER: &str = include_str!("../../../../shaders/gi/ddgi_update.wgsl");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DdgiDebugMode {
    #[default]
    Off,
    Probes,
    Irradiance,
    Visibility,
    RayBudget,
}

#[derive(Clone, Copy, Debug)]
pub struct DdgiVolumeSettings {
    pub origin: [f32; 3],
    pub spacing: f32,
    pub counts: [u32; 3],
    pub scroll_with_main_camera: bool,
}

impl Default for DdgiVolumeSettings {
    fn default() -> Self {
        Self {
            origin: [-14.0, -4.0, -14.0],
            spacing: 1.85,
            counts: [16, 8, 16],
            scroll_with_main_camera: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DdgiSettings {
    pub volume: DdgiVolumeSettings,
    pub rays_per_probe: u32,
    pub probes_per_frame: u32,
    pub hysteresis: f32,
    pub normal_bias: f32,
    pub view_bias: f32,
    pub max_ray_distance: f32,
    pub irradiance_resolution: u32,
    pub visibility_resolution: u32,
    pub bounces: u32,
    pub debug: DdgiDebugMode,
}

impl Default for DdgiSettings {
    fn default() -> Self {
        Self {
            volume: DdgiVolumeSettings::default(),
            rays_per_probe: 64,
            probes_per_frame: 128,
            hysteresis: 0.92,
            normal_bias: 0.08,
            view_bias: 0.20,
            max_ray_distance: 40.0,
            irradiance_resolution: 6,
            visibility_resolution: 6,
            bounces: 2,
            debug: DdgiDebugMode::Off,
        }
    }
}

#[inline]
pub fn global_illumination(settings: DdgiSettings) -> GlobalIllumination {
    GlobalIllumination::provider(crate::render::gi::GiProviderConfig::new(
        DDGI_PROVIDER_ID,
        settings,
    ))
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct DdgiUniform {
    pub(crate) origin_spacing: [f32; 4],
    pub(crate) counts_enabled: [u32; 4],
    pub(crate) irradiance_atlas_params: [u32; 4],
    pub(crate) visibility_atlas_params: [u32; 4],
    pub(crate) trace_params: [f32; 4],
    pub(crate) frame_params: [u32; 4],
    pub(crate) ambient: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct DdgiLightMeta {
    pub(crate) count: u32,
    pub(crate) _pad: [u32; 3],
}
