mod data;
mod pass;
mod scene_upload;
pub mod shadow;

pub use crate::render::resources::{GpuLight, GpuLightKind, LightTable, SceneLightingResources};
pub use data::{color_temperature, Light2D};
pub use pass::LightPass;
pub(crate) use scene_upload::collect_gpu_lights_into;
#[allow(unused_imports)]
pub use shadow::{
    DirectionalShadowPhase, SceneShadowResources, ShadowDebugResources, ShadowResourceKind,
};
