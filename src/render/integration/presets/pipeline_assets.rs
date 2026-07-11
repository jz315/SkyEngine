//! High-level pipeline presets assembled from core protocols and feature domains.

use crate::render::core::draw::{OpaquePhase, TransparentPhase};
use crate::render::core::pipeline::RenderPipelineAsset;
use crate::render::features::gi::{GiCompositePass, GiFeature, GiUpdateCompute};
use crate::render::features::lighting::{DirectionalShadowPhase, LightingFeature};
use crate::render::features::mesh::{SceneMaterialPrepass, SceneNormalPrepass};
use crate::render::features::postfx::{
    Bloom, ContactShadows, DebugView, Sharpen, TemporalAntiAliasing, ToneMap,
};
use crate::render::features::sprite::SpriteFeature;

impl RenderPipelineAsset {
    pub fn forward_2d() -> Self {
        Self::builder()
            .add_feature(SpriteFeature::lit_hdr())
            .add_phase(TransparentPhase::new())
            .add_postfx(Bloom::default())
            .add_postfx(ToneMap::default())
            .build()
    }

    #[cfg(feature = "live2d")]
    pub fn live2d_2d() -> Self {
        Self::builder()
            .add_feature(SpriteFeature::unlit())
            .add_feature(crate::render::features::live2d::Live2DFeature::new())
            .add_phase(TransparentPhase::new())
            .build()
    }

    pub fn forward_3d() -> Self {
        Self::builder()
            .add_feature(SpriteFeature::lit_hdr())
            .add_feature(LightingFeature)
            .add_feature(GiFeature)
            .add_phase(DirectionalShadowPhase::new())
            .add_compute(GiUpdateCompute)
            .add_phase(OpaquePhase::new())
            .add_phase(TransparentPhase::new())
            .add_postfx(Bloom::default())
            .add_postfx(ToneMap::default())
            .build()
    }

    pub fn modern_3d() -> Self {
        Self::builder()
            .add_feature(SpriteFeature::lit_hdr())
            .add_feature(LightingFeature)
            .add_feature(GiFeature)
            .add_phase(SceneNormalPrepass::default())
            .add_phase(SceneMaterialPrepass::default())
            .add_phase(DirectionalShadowPhase::new())
            .add_compute(GiUpdateCompute)
            .add_phase(OpaquePhase::new())
            .add_postfx(ContactShadows::default())
            .add_postfx(GiCompositePass)
            .add_phase(TransparentPhase::new())
            .add_postfx(TemporalAntiAliasing::default())
            .add_postfx(Sharpen::default())
            .add_postfx(Bloom::default())
            .add_postfx(ToneMap::default())
            .add_postfx(DebugView::default())
            .build()
    }
}
