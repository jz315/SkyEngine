mod atlas;
mod bindings;
mod phase;
mod resources;
mod view;

pub(crate) const TRANSPARENT_SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub use phase::DirectionalShadowPhase;
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
pub(crate) use view::{append_directional_shadow_views, sync_shadow_views, ShadowViewBinding};
