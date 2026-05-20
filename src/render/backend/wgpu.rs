use std::sync::Arc;

use winit::window::Window;

use crate::asset::{Assets, Handle};
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::asset::{MeshAsset, StandardMaterialAsset};
use crate::render::pipeline::{RenderBackendKind, RenderPipelineAsset};
use crate::render::resources::material::MaterialHandle;
use crate::render::resources::mesh::MeshHandle;
use crate::render::resources::texture_cache::SharedRenderAssetCache;
use crate::render::runtime::RenderRuntime;
use crate::render::view::RenderStats;

use super::scene_renderer::{SceneRenderer, SceneRendererError, SceneRendererInitError};
use super::wgpu_asset_bridge::WgpuRenderAssetCache;

/// Native `wgpu` backend wrapper around the high-level [`RenderRuntime`].
pub struct WgpuSceneRenderer {
    gpu: GpuContext,
    render_runtime: Option<RenderRuntime>,
    asset_cache: WgpuRenderAssetCache,
}

impl WgpuSceneRenderer {
    pub fn try_new(
        window: Arc<Window>,
        vsync: bool,
        pipeline: Option<RenderPipelineAsset>,
    ) -> Result<Self, SceneRendererInitError> {
        if let Some(asset) = pipeline.as_ref() {
            debug_assert_eq!(asset.backend_kind(), RenderBackendKind::Wgpu);
        }
        let gpu = GpuContext::try_new(window, vsync)?;
        let mut render_runtime = pipeline.map(RenderRuntime::from_asset);
        if let Some(render_runtime) = render_runtime.as_mut() {
            render_runtime.prepare_gpu_resources(&gpu);
        }
        Ok(Self {
            gpu,
            render_runtime,
            asset_cache: WgpuRenderAssetCache::default(),
        })
    }

    #[inline]
    pub fn gpu(&self) -> &GpuContext {
        &self.gpu
    }

    #[inline]
    pub fn gpu_mut(&mut self) -> &mut GpuContext {
        &mut self.gpu
    }

    #[inline]
    pub fn render_runtime(&self) -> Option<&RenderRuntime> {
        self.render_runtime.as_ref()
    }

    #[inline]
    pub fn render_runtime_mut(&mut self) -> Option<&mut RenderRuntime> {
        self.render_runtime.as_mut()
    }

    pub fn sync_mesh_asset(
        &mut self,
        assets: &Assets,
        handle: Handle<MeshAsset>,
    ) -> Option<MeshHandle> {
        let render_runtime = self.render_runtime.as_mut()?;
        self.asset_cache
            .sync_mesh(&self.gpu, render_runtime, assets, handle)
    }

    pub fn sync_standard_material_asset(
        &mut self,
        assets: &Assets,
        render_assets: &SharedRenderAssetCache,
        handle: Handle<StandardMaterialAsset>,
    ) -> Option<MaterialHandle> {
        let render_runtime = self.render_runtime.as_mut()?;
        self.asset_cache.sync_standard_material(
            &self.gpu,
            render_runtime,
            assets,
            render_assets,
            handle,
        )
    }
}

impl SceneRenderer for WgpuSceneRenderer {
    fn backend_kind(&self) -> RenderBackendKind {
        RenderBackendKind::Wgpu
    }

    fn begin_frame(&mut self) -> Result<(), SceneRendererError> {
        self.gpu.begin_frame().map_err(SceneRendererError::from)
    }

    fn end_frame(&mut self) {
        self.gpu.end_frame();
    }

    fn render_world(&mut self, world: &World) {
        self.render_runtime
            .as_mut()
            .expect("FrameContext::render requires RenderPlugin::pipeline(...)")
            .render_world(&mut self.gpu, world);
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize_surface(width, height);
        if let Some(render_runtime) = self.render_runtime.as_mut() {
            render_runtime.resize(&self.gpu, width, height);
        }
    }

    fn surface_lost(&mut self) {
        if let Some(render_runtime) = self.render_runtime.as_mut() {
            render_runtime.surface_lost();
        }
    }

    fn stats(&self) -> RenderStats {
        self.render_runtime
            .as_ref()
            .map(RenderRuntime::stats)
            .unwrap_or_default()
    }

    fn surface_size(&self) -> [u32; 2] {
        self.gpu.surface_size()
    }

    fn adapter_name(&self) -> &str {
        self.gpu.adapter_name()
    }

    fn backend_name(&self) -> &str {
        self.gpu.backend_name()
    }

    fn wgpu(&self) -> Option<&GpuContext> {
        Some(&self.gpu)
    }

    fn wgpu_mut(&mut self) -> Option<&mut GpuContext> {
        Some(&mut self.gpu)
    }

    fn wgpu_render_runtime_mut(&mut self) -> Option<&mut RenderRuntime> {
        self.render_runtime.as_mut()
    }

    fn wgpu_render_runtime_parts_mut(&mut self) -> Option<(&mut RenderRuntime, &mut GpuContext)> {
        let render_runtime = self.render_runtime.as_mut()?;
        Some((render_runtime, &mut self.gpu))
    }

    fn wgpu_overlay_parts_mut(
        &mut self,
    ) -> Option<(&mut GpuContext, Option<&SharedRenderAssetCache>)> {
        let render_assets = self
            .render_runtime
            .as_ref()
            .map(RenderRuntime::render_asset_cache);
        Some((&mut self.gpu, render_assets))
    }
}
