use std::any::TypeId;
use std::hash::{Hash, Hasher};

use rustc_hash::FxHashMap;
use wgpu::util::DeviceExt;

use crate::render::execution::{PhaseExecuteContext, PhaseSetupContext};
use crate::render::gpu::DEFAULT_DEPTH_FORMAT;
use crate::render::graph::{ImportedTexture, LoadOp, RenderGraphError, ResourceRef};
use crate::render::phase::{
    DrawFunctionRegistry, MeshDrawData, OpaquePhase, PhaseItem, TransparentPhase,
};
use crate::render::pipeline::RenderPhase;
use crate::render::resources::material::{MaterialError, MaterialHandle, MaterialRegistry};
use crate::render::resources::mesh::{VertexLayout, VertexSemantic};
use crate::render::resources::ShadowResourceKind;
use crate::render::view::SceneView;
use crate::render::StandardMaterial;

use super::sync::{ShadowRasterBias, ShadowViewBinding, IDENTITY_MATRIX};
use super::{
    SceneShadowGraphResources, SceneShadowResources, ShadowPassBindingLayout,
    ShadowSceneBindingLayout, TRANSPARENT_SHADOW_FORMAT,
};

pub struct DirectionalShadowPhase {
    pipelines: FxHashMap<u64, wgpu::RenderPipeline>,
    clear_pipeline: Option<wgpu::RenderPipeline>,
    transparent_clear_pipeline: Option<wgpu::RenderPipeline>,
}

const SHADOW_ATLAS_CLEAR_SHADER: &str = r#"
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let xy = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(xy[vertex_index], 1.0, 1.0);
}
"#;

const TRANSPARENT_SHADOW_ATLAS_CLEAR_SHADER: &str = r#"
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let xy = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(xy[vertex_index], 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 1.0, 0.0);
}
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ShadowPipelineKind {
    Opaque,
    AlphaTest,
    Transparent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShadowCasterKind {
    Opaque,
    AlphaTest(MaterialHandle),
}

const TRANSPARENT_SHADOW_BLEND_STATE: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::Src,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Max,
    },
};

mod helpers;
mod opaque;
mod pipelines;
#[cfg(test)]
mod tests;
mod transparent;

use helpers::*;
use transparent::*;

impl ShadowCasterKind {
    #[inline]
    const fn pipeline_kind(self) -> ShadowPipelineKind {
        match self {
            Self::Opaque => ShadowPipelineKind::Opaque,
            Self::AlphaTest(_) => ShadowPipelineKind::AlphaTest,
        }
    }
}
