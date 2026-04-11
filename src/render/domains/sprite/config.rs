use crate::render::ecs::RenderSettings;
use crate::render::stats::RenderTimingStats;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ecs2DRenderPath {
    Unlit,
    LitHdr,
}

impl Ecs2DRenderPath {
    #[inline]
    pub(super) const fn unlit() -> Self {
        Self::Unlit
    }

    #[inline]
    pub(super) const fn lit_hdr() -> Self {
        Self::LitHdr
    }

    #[inline]
    pub(super) const fn uses_hdr(self) -> bool {
        matches!(self, Self::LitHdr)
    }

    pub(super) fn default_settings(self) -> RenderSettings {
        use crate::render::ecs::{BloomSettings, ToneMapSettings, VignetteSettings};
        use crate::render::Color;

        match self {
            Self::Unlit => RenderSettings {
                clear_color: Color::rgb(0.02, 0.02, 0.06),
                ambient_color: Color::BLACK,
                bloom: BloomSettings {
                    enabled: false,
                    ..Default::default()
                },
                tonemap: ToneMapSettings {
                    enabled: false,
                    ..Default::default()
                },
                vignette: VignetteSettings {
                    enabled: false,
                    ..Default::default()
                },
            },
            Self::LitHdr => RenderSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SpriteBackendStats {
    pub view_count: usize,
    pub sprite_count: usize,
    pub light_count: usize,
    pub dirty_sprite_slots: usize,
    pub dirty_light_slots: usize,
    pub visible_sprite_upload_count: usize,
    pub visible_light_upload_count: usize,
    pub timings: RenderTimingStats,
}
