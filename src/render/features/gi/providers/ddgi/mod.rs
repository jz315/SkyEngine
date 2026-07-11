//! DDGI provider facade. Runtime state, BVH construction, resource creation,
//! preparation, execution, and tests are kept in focused sibling modules.

use std::borrow::Cow;

use crate::gpu::GpuContext;
use crate::math::{Mat4, Vec3};
use crate::render::execution::ComputePassExecuteContext;
use crate::render::gi::{
    downcast_settings, GiMaterial, GiProviderFactory, GiProviderId, GiProviderRuntime,
    GiSamplingBinding, GiSceneInput, GiSettings, GiShaderDescriptor, GiUpdateDescriptor,
};
use crate::render::gpu::{ComputePipelineCache, RenderTarget, RenderTargetDescriptor};
use crate::render::graph::{PassFlags, RenderGraphError};
use crate::render::resources::mesh::RayTriangle;
use crate::render::view::SceneView;
use crate::render::{Color, GlobalIllumination, GpuLight};

mod bvh;
mod execute;
mod prepare;
mod provider;
mod resources;
mod settings;
mod shader;
#[cfg(test)]
mod tests;

pub use provider::DdgiProviderFactory;
pub use settings::{
    global_illumination, DdgiDebugMode, DdgiSettings, DdgiVolumeSettings, DDGI_PROVIDER_ID,
};

pub(crate) use bvh::*;
pub(crate) use prepare::*;
pub(crate) use provider::DdgiRuntime;
pub(crate) use resources::*;
pub(crate) use settings::*;
pub(crate) use shader::*;
