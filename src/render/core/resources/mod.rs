pub mod atlas;
pub mod blackboard;
pub mod indirect_lighting;
pub mod lights;
pub mod material;
pub mod mesh;
pub mod scene_shadows;
pub mod texture_cache;

pub(crate) use indirect_lighting::FrameIndirectLighting;
pub use indirect_lighting::{
    IndirectLightingSampling, IndirectLightingShader, NULL_INDIRECT_LIGHTING_SHADER,
};
pub use lights::{GpuLight, GpuLightKind, LightTable, SceneLightingResources};
pub(crate) use scene_shadows::SceneShadowGraphResources;
pub use scene_shadows::{
    SceneShadowResources, ShadowDebugResources, ShadowResourceKind, MAX_DIRECTIONAL_SHADOW_CASCADES,
};
