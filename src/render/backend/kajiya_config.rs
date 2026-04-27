use std::path::PathBuf;

use winit::window::Window;

use super::kajiya_cache;

#[derive(Clone, Debug)]
pub(crate) struct KajiyaRendererConfig {
    vendor_root: PathBuf,
    cache_dir: PathBuf,
    temporal_upsampling: f32,
    upscale_extent: Option<[u32; 2]>,
    trace: bool,
}

impl KajiyaRendererConfig {
    pub(crate) fn from_env() -> Self {
        Self {
            vendor_root: kajiya_cache::vendor_root(),
            cache_dir: kajiya_cache::default_cache_dir(),
            temporal_upsampling: env_f32("SKY_KAJIYA_TEMPORAL_UPSAMPLE")
                .unwrap_or(1.0)
                .clamp(1.0, 8.0),
            upscale_extent: env_extent("SKY_KAJIYA_UPSCALE_EXTENT"),
            trace: env_bool("SKY_KAJIYA_TRACE"),
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

    pub(crate) fn temporal_upscale_extent(
        &self,
        window: &Window,
        swapchain_extent: [u32; 2],
    ) -> [u32; 2] {
        if let Some(extent) = self.upscale_extent {
            return extent;
        }

        let scale = (window.scale_factor() as f32).max(1.0);
        [
            ((swapchain_extent[0] as f32 / scale).round() as u32).max(1),
            ((swapchain_extent[1] as f32 / scale).round() as u32).max(1),
        ]
    }

    #[inline]
    pub(crate) fn render_extent(&self, temporal_upscale_extent: [u32; 2]) -> [u32; 2] {
        render_extent_for(temporal_upscale_extent, self.temporal_upsampling)
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

fn env_bool(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| {
        let value = value.to_string_lossy();
        !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
    })
}

fn env_f32(name: &str) -> Option<f32> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
}

fn env_extent(name: &str) -> Option<[u32; 2]> {
    let value = std::env::var(name).ok()?;
    let (width, height) = value.split_once(['x', 'X', ','])?;
    let width = width.trim().parse::<u32>().ok()?.max(1);
    let height = height.trim().parse::<u32>().ok()?.max(1);
    Some([width, height])
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
}
