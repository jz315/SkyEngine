/// Rendering backend requested by a [`RenderPipelineAsset`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderBackendKind {
    /// SkyEngine's native `wgpu` renderer.
    #[default]
    Wgpu,
    /// Kajiya-backed high-quality 3D renderer.
    Kajiya,
    /// Renderling-backed experimental 3D renderer.
    Renderling,
}

/// DPI handling used by the Kajiya backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KajiyaDpiMode {
    /// Match Kajiya's official viewer: app size is logical, swapchain may be
    /// larger on HiDPI displays, and the final blit scales to physical pixels.
    Logical,
    /// Render to the physical swapchain size.
    Physical,
}

/// Runtime settings for [`RenderPipelineAsset::kajiya_3d`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KajiyaRendererSettings {
    temporal_upsampling: f32,
    upscale_extent: Option<[u32; 2]>,
    dpi_mode: KajiyaDpiMode,
    sun_size_multiplier: f32,
    taa_jitter: bool,
    motion_blur: bool,
    device_index: Option<usize>,
    trace: bool,
}

impl KajiyaRendererSettings {
    /// Match Kajiya's simple viewer defaults for a 1280x720 target:
    /// render at the full temporal upscale extent and let the final blit scale
    /// to the swapchain when the window is HiDPI or resized.
    pub fn viewer_720p() -> Self {
        Self::default()
            .with_upscale_extent(1280, 720)
            .with_temporal_upsampling(1.0)
    }

    pub fn performance() -> Self {
        Self::default().with_temporal_upsampling(1.5)
    }

    #[inline]
    pub fn temporal_upsampling(&self) -> f32 {
        self.temporal_upsampling
    }

    #[inline]
    pub fn upscale_extent(&self) -> Option<[u32; 2]> {
        self.upscale_extent
    }

    #[inline]
    pub fn dpi_mode(&self) -> KajiyaDpiMode {
        self.dpi_mode
    }

    #[inline]
    pub fn sun_size_multiplier(&self) -> f32 {
        self.sun_size_multiplier
    }

    #[inline]
    pub fn taa_jitter_enabled(&self) -> bool {
        self.taa_jitter
    }

    #[inline]
    pub fn motion_blur_enabled(&self) -> bool {
        self.motion_blur
    }

    #[inline]
    pub fn device_index(&self) -> Option<usize> {
        self.device_index
    }

    #[inline]
    pub fn trace_enabled(&self) -> bool {
        self.trace
    }

    pub fn with_temporal_upsampling(mut self, temporal_upsampling: f32) -> Self {
        self.temporal_upsampling = temporal_upsampling.clamp(1.0, 8.0);
        self
    }

    pub fn with_upscale_extent(mut self, width: u32, height: u32) -> Self {
        self.upscale_extent = Some([width.max(1), height.max(1)]);
        self
    }

    pub fn with_dpi_mode(mut self, dpi_mode: KajiyaDpiMode) -> Self {
        self.dpi_mode = dpi_mode;
        self
    }

    pub fn with_sun_size_multiplier(mut self, sun_size_multiplier: f32) -> Self {
        self.sun_size_multiplier = sun_size_multiplier.clamp(0.0, 10.0);
        self
    }

    pub fn with_taa_jitter(mut self, enabled: bool) -> Self {
        self.taa_jitter = enabled;
        self
    }

    pub fn with_motion_blur(mut self, enabled: bool) -> Self {
        self.motion_blur = enabled;
        self
    }

    pub fn with_device_index(mut self, device_index: Option<usize>) -> Self {
        self.device_index = device_index;
        self
    }

    pub fn with_trace(mut self, enabled: bool) -> Self {
        self.trace = enabled;
        self
    }
}

impl Default for KajiyaRendererSettings {
    fn default() -> Self {
        Self {
            temporal_upsampling: 1.0,
            upscale_extent: None,
            dpi_mode: KajiyaDpiMode::Logical,
            sun_size_multiplier: 1.0,
            taa_jitter: true,
            motion_blur: false,
            device_index: None,
            trace: false,
        }
    }
}
