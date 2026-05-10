#![allow(unused_imports)]

pub(super) use crate::asset::{AssetConfig, AssetId, AssetServer, Handle, TextureAsset};
pub(super) use crate::diagnostics::{
    DiagnosticSeverity, DiagnosticSubsystem, Diagnostics, EngineDiagnosticKind,
};
#[cfg(feature = "live2d")]
pub(super) use crate::ecs::EntityId;
pub(super) use crate::ecs::World;
pub(super) use crate::gpu::GpuContext;
pub(super) use crate::render::execution::{
    GraphPassExecuteContext, GraphPassSetupContext, PhaseExecuteContext, PhaseSetupContext,
    PreparedFrame, PreparedView,
};
pub(super) use crate::render::expert::{
    BoundingSphere, Mesh, MeshDescriptor, MeshIndexData, RenderGraphError, TargetSize,
};
pub(super) use crate::render::gpu::{read_render_target, RenderTarget, RenderTargetDescriptor};
pub(super) use crate::render::graph::ImportedTexture;
#[cfg(feature = "live2d")]
pub(super) use crate::render::live2d::{
    live2d_instance_visible_in_view, sort_live2d_scene_instances, Live2DSceneInstance,
};
pub(super) use crate::render::pipeline::{
    ComputePass, GraphPass, PostFxPass, RenderPass, RenderPhase,
};
pub(super) use crate::render::resources::texture_cache::SharedRenderAssetCache;
pub(super) use crate::render::view::Projection;
pub(super) use crate::render::{
    CameraMarker, CameraViewport, Color, ComputePassExecuteContext, ComputePassSetupContext,
    DirectionalLight, MainCamera, MaterialError, PostFxPassExecuteContext, PostFxPassSetupContext,
    RenderPassExecuteContext, RenderPassSetupContext, RenderPipelineAsset, RenderPipelineBuilder,
    RenderRuntime, RenderSettings, StandardMaterial, Transform, UnlitMaterial, ViewportRect,
    WgpuMeshRenderer,
};
#[cfg(feature = "live2d")]
pub(super) use crate::render::{RenderQueueSort, SceneView, SortingLayer};
pub(super) use std::sync::atomic::{AtomicUsize, Ordering};
pub(super) use std::sync::{Arc, Mutex};

pub(super) fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for runtime tests");

    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("runtime_test_device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        ..Default::default()
    }))
    .expect("Failed to create test GPU device")
}
