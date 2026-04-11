use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::frame_pipeline::{FrameViewNode, PreparedFrame, PreparedView};
use crate::render::live2d::render::clipping::ClippingManager;
use crate::render::live2d::{
    Live2DLoadError, Live2DModel, Live2DModelResource, Live2DOverlayNode, Live2DRenderer,
    Live2DUserModel, PreparedLive2DFrameSet,
};
use crate::render::scene::SceneView;

struct Live2DEntry {
    resource: Live2DModelResource,
    user_model: Live2DUserModel,
    clipping: Option<ClippingManager>,
    visible: bool,
}

pub struct Live2DBackend {
    active_only: Option<usize>,
    entries: Vec<Live2DEntry>,
    prepared_frames: PreparedLive2DFrameSet,
    renderer: Option<Arc<Mutex<Live2DRenderer>>>,
}

impl Live2DBackend {
    pub fn new() -> Self {
        Self {
            active_only: None,
            entries: Vec::new(),
            prepared_frames: PreparedLive2DFrameSet::new(),
            renderer: None,
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

    pub(crate) fn clear_prepared_frames(&mut self) {
        self.prepared_frames = PreparedLive2DFrameSet::new();
    }

    pub(crate) fn prepare_entry_for_view(
        &mut self,
        gpu: &mut GpuContext,
        target_format: wgpu::TextureFormat,
        view_index: usize,
        target_size: [u32; 2],
        entry_index: usize,
        projection: &[f32; 16],
    ) {
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        if self.active_only.is_some_and(|active| active != entry_index) {
            return;
        }
        let Some(entry) = self.entries.get_mut(entry_index) else {
            return;
        };
        if !entry.visible {
            return;
        }

        let mut renderer = renderer.lock().expect("Live2D renderer lock poisoned");
        let prepared = renderer.prepare_frame_with_projection(
            gpu,
            target_format,
            target_size,
            projection,
            entry.user_model.model(),
            entry.resource.textures(),
            &mut entry.clipping,
        );
        self.prepared_frames.push(view_index, prepared);
    }

    pub(crate) fn insert_frame_payloads<'a>(&'a self, frame: &mut PreparedFrame<'a>) {
        if !self.prepared_frames.is_empty() {
            let _ = frame.insert_payload(&self.prepared_frames);
        }
    }

    pub(crate) fn insert_view_payloads<'a>(
        &'a self,
        _view_index: usize,
        _view: &SceneView,
        _prepared_view: &mut PreparedView<'a>,
    ) {
    }

    pub(crate) fn create_view_nodes(&mut self, ctx: &GpuContext) -> Vec<Box<dyn FrameViewNode>> {
        let renderer = Arc::new(Mutex::new(Live2DRenderer::new(ctx)));
        self.renderer = Some(Arc::clone(&renderer));
        vec![Box::new(Live2DOverlayNode::with_shared_renderer(
            ctx, renderer,
        ))]
    }

    pub(crate) fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {}

    pub(crate) fn surface_lost(&mut self) {}
}

impl Default for Live2DBackend {
    fn default() -> Self {
        Self::new()
    }
}
