mod atlas;
mod bindings;
#[cfg(test)]
mod formula;
mod frame_extension;
mod phase;
mod resources;
mod sync;

pub(crate) const TRANSPARENT_SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub(crate) use frame_extension::ShadowFrameExtension;
pub use phase::DirectionalShadowPhase;
pub(crate) use resources::SceneShadowGraphResources;
pub use resources::{SceneShadowResources, ShadowDebugResources, ShadowResourceKind};

pub(crate) use atlas::{
    ShadowAtlasLayout, ShadowAtlasStats, DEFAULT_SHADOW_ATLAS_GUARD_BAND_TEXELS,
};
#[allow(unused_imports)]
pub(crate) use bindings::{
    create_shadow_compare_sampler, create_shadow_pass_bind_group, create_shadow_scene_bind_group,
    create_shadow_scene_bind_group_layout, ShadowPassBindingLayout, ShadowPassUniform,
    ShadowSceneBindingLayout, ShadowUniform,
};
#[cfg(test)]
pub(crate) use sync::append_directional_shadow_views;
pub(crate) use sync::{
    append_directional_shadow_views_into, sync_shadow_views, DirectionalShadowSetup,
    DirectionalShadowViewScratch, ShadowViewBinding,
};
