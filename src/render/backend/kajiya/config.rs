use std::path::PathBuf;

use winit::window::Window;

use crate::render::pipeline::{KajiyaDpiMode, KajiyaRendererSettings};

use super::cache;

#[derive(Clone, Debug)]
pub(crate) struct KajiyaRendererConfig {
    vendor_root: PathBuf,
    cache_dir: PathBuf,
    temporal_upsampling: f32,
    upscale_extent: Option<[u32; 2]>,
    dpi_mode: KajiyaDpiMode,
    sun_size_multiplier: f32,
    taa_jitter: bool,
    motion_blur: bool,
    device_index: Option<usize>,
    trace: bool,
}

impl KajiyaRendererConfig {
    pub(crate) fn from_settings(settings: KajiyaRendererSettings) -> Self {
        Self {
            vendor_root: cache::vendor_root(),
            cache_dir: cache::default_cache_dir(),
            temporal_upsampling: env_f32("SKY_KAJIYA_TEMPORAL_UPSAMPLE")
                .unwrap_or(settings.temporal_upsampling())
                .clamp(1.0, 8.0),
            upscale_extent: env_extent("SKY_KAJIYA_UPSCALE_EXTENT")
                .or_else(|| settings.upscale_extent()),
            dpi_mode: env_dpi_mode("SKY_KAJIYA_DPI_MODE").unwrap_or(settings.dpi_mode()),
            sun_size_multiplier: env_f32("SKY_KAJIYA_SUN_SIZE")
                .unwrap_or(settings.sun_size_multiplier())
                .clamp(0.0, 10.0),
            taa_jitter: env_bool_or("SKY_KAJIYA_TAA_JITTER", settings.taa_jitter_enabled()),
            motion_blur: env_bool_or("SKY_KAJIYA_MOTION_BLUR", settings.motion_blur_enabled()),
            device_index: env_usize("SKY_KAJIYA_DEVICE_INDEX").or_else(|| settings.device_index()),
            trace: env_bool_or("SKY_KAJIYA_TRACE", settings.trace_enabled()),
        }
    }

    #[inline]
    pub(crate) fn vendor_root(&self) -> &PathBuf {
        &self.vendor_root
    }

    #[inline]
    pub(crate) fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    #[inline]
    pub(crate) fn trace_enabled(&self) -> bool {
        self.trace
    }

    #[inline]
    pub(crate) fn should_trace_frame(&self, frame_index: u64) -> bool {
        self.trace && (frame_index < 8 || frame_index % 120 == 0)
    }

    #[inline]
    pub(crate) fn sun_size_multiplier(&self) -> f32 {
        self.sun_size_multiplier
    }

    #[inline]
    pub(crate) fn taa_jitter_enabled(&self) -> bool {
        self.taa_jitter
    }

    #[inline]
    pub(crate) fn motion_blur_enabled(&self) -> bool {
        self.motion_blur
    }

    #[inline]
    pub(crate) fn device_index(&self) -> Option<usize> {
        self.device_index
    }

    pub(crate) fn temporal_upscale_extent(
        &self,
        window: &Window,
        swapchain_extent: [u32; 2],
    ) -> [u32; 2] {
        temporal_upscale_extent_for(
            swapchain_extent,
            self.upscale_extent,
            self.dpi_mode,
            window.scale_factor() as f32,
        )
    }

    #[inline]
    pub(crate) fn render_extent(&self, temporal_upscale_extent: [u32; 2]) -> [u32; 2] {
        render_extent_for(temporal_upscale_extent, self.temporal_upsampling)
    }
}

pub(crate) fn temporal_upscale_extent_for(
    swapchain_extent: [u32; 2],
    override_extent: Option<[u32; 2]>,
    dpi_mode: KajiyaDpiMode,
    scale_factor: f32,
) -> [u32; 2] {
    if let Some(extent) = override_extent {
        return extent;
    }

    match dpi_mode {
        KajiyaDpiMode::Physical => [swapchain_extent[0].max(1), swapchain_extent[1].max(1)],
        KajiyaDpiMode::Logical => {
            let scale = scale_factor.max(1.0);
            [
                ((swapchain_extent[0] as f32 / scale).round() as u32).max(1),
                ((swapchain_extent[1] as f32 / scale).round() as u32).max(1),
            ]
        }
    }
}

pub(crate) fn render_extent_for(
    temporal_upscale_extent: [u32; 2],
    temporal_upsampling: f32,
) -> [u32; 2] {
    [
        ((temporal_upscale_extent[0] as f32 / temporal_upsampling).round() as u32).max(1),
        ((temporal_upscale_extent[1] as f32 / temporal_upsampling).round() as u32).max(1),
    ]
}

fn env_bool_or(name: &str, default: bool) -> bool {
    std::env::var_os(name).map_or(default, |value| {
        let value = value.to_string_lossy();
        !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
    })
}

fn env_f32(name: &str) -> Option<f32> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
}

fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
}

fn env_extent(name: &str) -> Option<[u32; 2]> {
    let value = std::env::var(name).ok()?;
    let (width, height) = value.split_once(['x', 'X', ','])?;
    let width = width.trim().parse::<u32>().ok()?.max(1);
    let height = height.trim().parse::<u32>().ok()?.max(1);
    Some([width, height])
}

fn env_dpi_mode(name: &str) -> Option<KajiyaDpiMode> {
    let value = std::env::var(name).ok()?;
    parse_dpi_mode(&value)
}

fn parse_dpi_mode(value: &str) -> Option<KajiyaDpiMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "logical" | "scaled" | "dpi" => Some(KajiyaDpiMode::Logical),
        "physical" | "native" | "1:1" => Some(KajiyaDpiMode::Physical),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_extent_follows_temporal_upsampling() {
        assert_eq!(render_extent_for([1280, 720], 1.0), [1280, 720]);
        assert_eq!(render_extent_for([1280, 720], 2.0), [640, 360]);
        assert_eq!(render_extent_for([1, 1], 8.0), [1, 1]);
    }

    #[test]
    fn temporal_upscale_extent_defaults_to_logical_window_extent() {
        assert_eq!(
            temporal_upscale_extent_for([2560, 1440], None, KajiyaDpiMode::Logical, 2.0),
            [1280, 720]
        );
        assert_eq!(
            temporal_upscale_extent_for([1920, 1080], None, KajiyaDpiMode::Logical, 1.5),
            [1280, 720]
        );
        assert_eq!(
            temporal_upscale_extent_for([0, 0], None, KajiyaDpiMode::Logical, 2.0),
            [1, 1]
        );
    }

    #[test]
    fn temporal_upscale_extent_can_use_physical_swapchain_extent() {
        assert_eq!(
            temporal_upscale_extent_for([2560, 1440], None, KajiyaDpiMode::Physical, 2.0),
            [2560, 1440]
        );
    }

    #[test]
    fn temporal_upscale_extent_override_wins_over_dpi_mode() {
        assert_eq!(
            temporal_upscale_extent_for(
                [2560, 1440],
                Some([1600, 900]),
                KajiyaDpiMode::Physical,
                2.0
            ),
            [1600, 900]
        );
    }

    #[test]
    fn parses_dpi_mode_env_values() {
        assert_eq!(parse_dpi_mode("logical"), Some(KajiyaDpiMode::Logical));
        assert_eq!(parse_dpi_mode("native"), Some(KajiyaDpiMode::Physical));
        assert_eq!(parse_dpi_mode("wat"), None);
    }

    #[test]
    fn bool_env_default_is_used_when_absent() {
        let name = "SKY_KAJIYA_TEST_ABSENT_BOOL";
        std::env::remove_var(name);
        assert!(env_bool_or(name, true));
        assert!(!env_bool_or(name, false));
    }
}
