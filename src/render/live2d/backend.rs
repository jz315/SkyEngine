use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::live2d::render::clipping::ClippingManager;
use crate::render::live2d::{
    Live2DLoadError, Live2DModel, Live2DModelResource, Live2DRenderer, Live2DUserModel,
    PreparedLive2DFrame,
};
use crate::render::view::SceneView;

struct Live2DEntry {
    resource: Live2DModelResource,
    user_model: Live2DUserModel,
    clipping: Option<ClippingManager>,
    visible: bool,
}

pub struct Live2DBackend {
    active_only: Option<usize>,
    entries: Vec<Live2DEntry>,
    phase_renderer: Option<Live2DPhaseRenderer>,
}

#[derive(Clone)]
pub(crate) struct Live2DPhaseRenderer {
    pub(crate) renderer: Arc<Mutex<Live2DRenderer>>,
}

impl Live2DBackend {
    pub fn new() -> Self {
        Self {
            active_only: None,
            entries: Vec::new(),
            phase_renderer: None,
        }
    }

    pub fn load_model(
        &mut self,
        gpu: &GpuContext,
        path: impl AsRef<Path>,
    ) -> Result<usize, Live2DLoadError> {
        let resource = Live2DModelResource::load(gpu, path.as_ref())?;
        let user_model = resource.instantiate()?;
        let clip_mgr = ClippingManager::new(user_model.model());
        let clipping = clip_mgr.has_masks().then_some(clip_mgr);
        self.entries.push(Live2DEntry {
            resource,
            user_model,
            clipping,
            visible: true,
        });
        Ok(self.entries.len() - 1)
    }

    #[inline]
    pub fn model_count(&self) -> usize {
        self.entries.len()
    }

    pub fn user_model_mut(&mut self, index: usize) -> Option<&mut Live2DUserModel> {
        self.entries
            .get_mut(index)
            .map(|entry| &mut entry.user_model)
    }

    pub fn user_model(&self, index: usize) -> Option<&Live2DUserModel> {
        self.entries.get(index).map(|entry| &entry.user_model)
    }

    pub fn model_resource(&self, index: usize) -> Option<&Live2DModelResource> {
        self.entries.get(index).map(|entry| &entry.resource)
    }

    pub fn model(&self, index: usize) -> Option<&Live2DModel> {
        self.entries
            .get(index)
            .map(|entry| entry.user_model.model())
    }

    pub fn set_visible(&mut self, index: usize, visible: bool) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.visible = visible;
        }
    }

    #[inline]
    pub fn set_active_only(&mut self, index: usize) {
        self.active_only = Some(index);
    }

    #[inline]
    pub fn clear_active_only(&mut self) {
        self.active_only = None;
    }

    pub(crate) fn extract(&mut self, _world: &World, _surface_size: [u32; 2]) {}

    pub(crate) fn collect_views(&self, _views: &mut Vec<SceneView>) {}

    fn ensure_renderer(&mut self, ctx: &GpuContext) -> Arc<Mutex<Live2DRenderer>> {
        if let Some(renderer) = self.phase_renderer.as_ref() {
            return Arc::clone(&renderer.renderer);
        }
        let renderer = Arc::new(Mutex::new(Live2DRenderer::new(ctx)));
        self.phase_renderer = Some(Live2DPhaseRenderer {
            renderer: Arc::clone(&renderer),
        });
        renderer
    }

    pub(crate) fn prepare_entry_for_view(
        &mut self,
        gpu: &mut GpuContext,
        target_format: wgpu::TextureFormat,
        target_size: [u32; 2],
        entry_index: usize,
        projection: &[f32; 16],
    ) -> Option<PreparedLive2DFrame> {
        let renderer = self.ensure_renderer(gpu);
        if self.active_only.is_some_and(|active| active != entry_index) {
            return None;
        }
        let Some(entry) = self.entries.get_mut(entry_index) else {
            return None;
        };
        if !entry.visible {
            return None;
        }

        let mut renderer = renderer.lock().expect("Live2D renderer lock poisoned");
        Some(renderer.prepare_frame_with_projection(
            gpu,
            target_format,
            target_size,
            projection,
            entry.user_model.model(),
            entry.resource.textures(),
            &mut entry.clipping,
        ))
    }

    pub(crate) fn insert_frame_payloads<'a>(&'a self, frame: &mut PreparedFrame<'a>) {
        if let Some(renderer) = self.phase_renderer.as_ref() {
            let _ = frame.insert_payload(renderer);
        }
    }

    pub(crate) fn insert_view_payloads<'a>(
        &'a self,
        _view_index: usize,
        _view: &SceneView,
        _prepared_view: &mut PreparedView<'a>,
    ) {
    }
}

impl Default for Live2DBackend {
    fn default() -> Self {
        Self::new()
    }
}
