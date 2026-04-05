//! Typed pipeline state and feature execution context.

use rustc_hash::FxHashMap;

use crate::render::core::color::Color;
use crate::render::ecs::{RenderSettings2D, ViewportRect};
use crate::render::gpu_scene2d::GpuScene2D;
use crate::render::graph::{PassHandle, TextureFormat, TextureHandle};
use crate::render::pipeline::prepared::{PreparedView2D, SpriteDrawSpan};
use crate::render::Texture;

#[derive(Debug, Clone, Copy)]
pub(crate) struct PassDispatchEntry2D {
    pub feature_index: usize,
    pub view_index: usize,
}

/// Typed shared state passed through render-feature graph setup for one view.
#[derive(Debug, Clone, Copy)]
pub struct PipelineState2D {
    view_size: [u32; 2],
    viewport: ViewportRect,
    surface_format: TextureFormat,
    has_surface: bool,
    clear_surface: bool,
    clear_color: Color,
    ambient_color: Color,
    scene_color: Option<TextureHandle>,
    lightmap: Option<TextureHandle>,
    current: Option<TextureHandle>,
}

impl PipelineState2D {
    pub(crate) fn new(
        view_size: [u32; 2],
        viewport: ViewportRect,
        surface_format: TextureFormat,
        has_surface: bool,
        clear_surface: bool,
        clear_color: Color,
        ambient_color: Color,
    ) -> Self {
        Self {
            view_size,
            viewport,
            surface_format,
            has_surface,
            clear_surface,
            clear_color,
            ambient_color,
            scene_color: None,
            lightmap: None,
            current: None,
        }
    }

    #[inline]
    pub fn view_size(&self) -> [u32; 2] {
        self.view_size
    }

    #[inline]
    pub fn viewport(&self) -> ViewportRect {
        self.viewport
    }

    #[inline]
    pub fn surface_format(&self) -> TextureFormat {
        self.surface_format
    }

    #[inline]
    pub fn has_surface(&self) -> bool {
        self.has_surface
    }

    #[inline]
    pub fn clear_surface(&self) -> bool {
        self.clear_surface
    }

    #[inline]
    pub fn clear_color(&self) -> Color {
        self.clear_color
    }

    #[inline]
    pub fn ambient_color(&self) -> Color {
        self.ambient_color
    }

    #[inline]
    pub fn scene_color(&self) -> Option<TextureHandle> {
        self.scene_color
    }

    #[inline]
    pub fn lightmap(&self) -> Option<TextureHandle> {
        self.lightmap
    }

    #[inline]
    pub fn current(&self) -> Option<TextureHandle> {
        self.current
    }

    #[inline]
    pub fn set_scene_color(&mut self, handle: TextureHandle) {
        self.scene_color = Some(handle);
        self.current = Some(handle);
    }

    #[inline]
    pub fn set_lightmap(&mut self, handle: TextureHandle) {
        self.lightmap = Some(handle);
    }

    #[inline]
    pub fn set_current(&mut self, handle: TextureHandle) {
        self.current = Some(handle);
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ViewPipelineState2D {
    pub view: PreparedView2D,
    pub pipeline: PipelineState2D,
}

pub(crate) struct FramePipelineState2D {
    settings: RenderSettings2D,
    surface_format: TextureFormat,
    has_surface: bool,
    view_states: Vec<ViewPipelineState2D>,
    pass_dispatch: FxHashMap<PassHandle, PassDispatchEntry2D>,
}

impl FramePipelineState2D {
    pub(crate) fn new(
        settings: RenderSettings2D,
        surface_format: TextureFormat,
        has_surface: bool,
    ) -> Self {
        Self {
            settings,
            surface_format,
            has_surface,
            view_states: Vec::with_capacity(4),
            pass_dispatch: FxHashMap::default(),
        }
    }

    #[inline]
    pub(crate) fn settings(&self) -> &RenderSettings2D {
        &self.settings
    }

    #[inline]
    pub(crate) fn has_surface(&self) -> bool {
        self.has_surface
    }

    pub(crate) fn push_view(&mut self, view: PreparedView2D, clear_surface: bool) -> usize {
        let view_state = ViewPipelineState2D {
            view,
            pipeline: PipelineState2D::new(
                view.viewport.size(),
                view.viewport,
                self.surface_format,
                self.has_surface,
                clear_surface,
                self.settings.clear_color,
                self.settings.ambient_color,
            ),
        };
        self.view_states.push(view_state);
        self.view_states.len() - 1
    }

    #[inline]
    pub(crate) fn view_state_mut(&mut self, index: usize) -> &mut PipelineState2D {
        &mut self.view_states[index].pipeline
    }

    #[inline]
    pub(crate) fn execution_context<'a>(
        &'a self,
        view_index: usize,
        gpu_scene: &'a GpuScene2D,
    ) -> FeatureExecutionContext2D<'a> {
        FeatureExecutionContext2D {
            view_state: &self.view_states[view_index],
            gpu_scene,
        }
    }

    pub(crate) fn register_passes(
        &mut self,
        handles: &[PassHandle],
        feature_index: usize,
        view_index: usize,
    ) {
        for &handle in handles {
            self.pass_dispatch.insert(
                handle,
                PassDispatchEntry2D {
                    feature_index,
                    view_index,
                },
            );
        }
    }

    #[inline]
    pub(crate) fn dispatch_entry(&self, handle: PassHandle) -> Option<PassDispatchEntry2D> {
        self.pass_dispatch.get(&handle).copied()
    }
}

/// Execution-time access to the prepared view and uploaded GPU scene data.
pub struct FeatureExecutionContext2D<'a> {
    pub(crate) view_state: &'a ViewPipelineState2D,
    pub(crate) gpu_scene: &'a GpuScene2D,
}

impl<'a> FeatureExecutionContext2D<'a> {
    #[inline]
    pub fn state(&self) -> &PipelineState2D {
        &self.view_state.pipeline
    }

    #[inline]
    pub fn viewport(&self) -> ViewportRect {
        self.view_state.view.viewport
    }

    #[inline]
    pub fn camera(&self) -> &crate::render::Camera2D {
        &self.view_state.view.camera
    }

    #[inline]
    pub fn order(&self) -> i32 {
        self.view_state.view.order
    }

    #[inline]
    pub fn layer_mask(&self) -> u32 {
        self.view_state.view.layer_mask
    }

    #[inline]
    pub fn sprite_table_buffer(&self) -> &wgpu::Buffer {
        self.gpu_scene.sprite_table_buffer()
    }

    #[inline]
    pub fn light_table_buffer(&self) -> &wgpu::Buffer {
        self.gpu_scene.light_table_buffer()
    }

    #[inline]
    pub fn sprite_table_version(&self) -> u64 {
        self.gpu_scene.sprite_table_version()
    }

    #[inline]
    pub fn light_table_version(&self) -> u64 {
        self.gpu_scene.light_table_version()
    }

    #[inline]
    pub fn visible_sprite_index_buffer(&self) -> &wgpu::Buffer {
        self.gpu_scene.visible_sprite_index_buffer()
    }

    #[inline]
    pub fn visible_light_index_buffer(&self) -> &wgpu::Buffer {
        self.gpu_scene.visible_light_index_buffer()
    }

    #[inline]
    pub fn visible_sprite_range(&self) -> std::ops::Range<u32> {
        self.view_state.view.sprite_range()
    }

    #[inline]
    pub fn visible_light_range(&self) -> std::ops::Range<u32> {
        self.view_state.view.light_range()
    }

    #[inline]
    pub(crate) fn draw_spans(&self) -> &[SpriteDrawSpan] {
        self.gpu_scene.draw_spans_for_view(self.view_state.view)
    }

    #[inline]
    pub fn textures(&self) -> &[Texture] {
        self.gpu_scene.textures()
    }
}
