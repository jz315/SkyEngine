use std::sync::Arc;

use winit::window::Window;

use crate::asset::{AssetServer, Handle};
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::assets::{MeshAsset, StandardMaterialAsset};
use crate::render::pipeline::{RenderBackendKind, RenderPipelineAsset};
use crate::render::resources::assets::SharedRenderAssetCache;
use crate::render::resources::material::MaterialHandle;
use crate::render::resources::mesh::MeshHandle;
use crate::render::runtime::RenderComposer;
use crate::render::view::RenderStats;

use super::scene_renderer::{SceneRenderer, SceneRendererError, SceneRendererInitError};
use super::wgpu_assets::WgpuRenderAssetCache;

/// Native `wgpu` backend wrapper around the existing [`RenderComposer`].
pub struct WgpuSceneRenderer {
    gpu: GpuContext,
    composer: Option<RenderComposer>,
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
        let mut composer = pipeline.map(RenderComposer::from_asset);
        if let Some(composer) = composer.as_mut() {
            composer.initialize_for_gpu(&gpu);
        }
        Ok(Self {
            gpu,
            composer,
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
    pub fn composer(&self) -> Option<&RenderComposer> {
        self.composer.as_ref()
    }

    #[inline]
    pub fn composer_mut(&mut self) -> Option<&mut RenderComposer> {
        self.composer.as_mut()
    }

    pub fn sync_mesh_asset(
        &mut self,
        assets: &AssetServer,
        handle: Handle<MeshAsset>,
    ) -> Option<MeshHandle> {
        let composer = self.composer.as_mut()?;
        self.asset_cache
            .sync_mesh(&self.gpu, composer, assets, handle)
    }

    pub fn sync_standard_material_asset(
        &mut self,
        assets: &AssetServer,
        render_assets: &SharedRenderAssetCache,
        handle: Handle<StandardMaterialAsset>,
    ) -> Option<MaterialHandle> {
        let composer = self.composer.as_mut()?;
        self.asset_cache
            .sync_standard_material(&self.gpu, composer, assets, render_assets, handle)
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
        self.composer
            .as_mut()
            .expect("FrameContext::render requires App::with_render_pipeline(...)")
            .render_world(&mut self.gpu, world);
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize_surface(width, height);
        if let Some(composer) = self.composer.as_mut() {
            composer.resize(&self.gpu, width, height);
        }
    }

    fn surface_lost(&mut self) {
        if let Some(composer) = self.composer.as_mut() {
            composer.surface_lost();
        }
    }

    fn stats(&self) -> RenderStats {
        self.composer
            .as_ref()
            .map(RenderComposer::stats)
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

    fn wgpu_composer_mut(&mut self) -> Option<&mut RenderComposer> {
        self.composer.as_mut()
    }

    fn wgpu_parts_mut(&mut self) -> Option<(&mut RenderComposer, &mut GpuContext)> {
        let composer = self.composer.as_mut()?;
        Some((composer, &mut self.gpu))
    }
}
