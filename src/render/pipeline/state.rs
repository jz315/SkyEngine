//! Typed pipeline state and feature execution context.

use std::any::{Any, TypeId};

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

/// Frame-scoped typed payload registry for render features.
///
/// This lets features access prepared data without hard-coding every future
/// renderer payload into [`FeatureExecutionContext2D`]. Payload types must be
/// `'static` so they can participate in typed downcasting.
pub struct FramePayloads2D<'a> {
    gpu_scene: Option<&'a GpuScene2D>,
    typed: FxHashMap<TypeId, &'a dyn Any>,
}

impl<'a> FramePayloads2D<'a> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn with_gpu_scene(mut self, gpu_scene: &'a GpuScene2D) -> Self {
        self.gpu_scene = Some(gpu_scene);
        self
    }

    #[inline]
    pub fn set_gpu_scene(&mut self, gpu_scene: &'a GpuScene2D) -> &mut Self {
        self.gpu_scene = Some(gpu_scene);
        self
    }

    #[inline]
    pub fn gpu_scene(&self) -> Option<&'a GpuScene2D> {
        self.gpu_scene
    }

    #[inline]
    pub fn insert<T: Any>(&mut self, value: &'a T) -> Option<&'a T> {
        self.typed
            .insert(TypeId::of::<T>(), value as &'a dyn Any)
            .and_then(|previous| previous.downcast_ref::<T>())
    }

    #[inline]
    pub fn contains<T: Any>(&self) -> bool {
        self.typed.contains_key(&TypeId::of::<T>())
    }

    #[inline]
    pub fn get<T: Any>(&self) -> Option<&'a T> {
        self.typed
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref::<T>())
    }
}

impl<'a> Default for FramePayloads2D<'a> {
    fn default() -> Self {
        Self {
            gpu_scene: None,
            typed: FxHashMap::default(),
        }
    }
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
    scene_color_format: Option<TextureFormat>,
    lightmap: Option<TextureHandle>,
    lightmap_format: Option<TextureFormat>,
    current: Option<TextureHandle>,
    current_format: Option<TextureFormat>,
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
            scene_color_format: None,
            lightmap: None,
            lightmap_format: None,
            current: None,
            current_format: None,
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
    pub fn scene_color_format(&self) -> Option<TextureFormat> {
        self.scene_color_format
    }

    #[inline]
    pub fn lightmap(&self) -> Option<TextureHandle> {
        self.lightmap
    }

    #[inline]
    pub fn lightmap_format(&self) -> Option<TextureFormat> {
        self.lightmap_format
    }

    #[inline]
    pub fn current(&self) -> Option<TextureHandle> {
        self.current
    }

    #[inline]
    pub fn current_format(&self) -> Option<TextureFormat> {
        self.current_format
    }

    #[inline]
    pub fn set_scene_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        self.scene_color = Some(handle);
        self.scene_color_format = Some(format);
        self.current = Some(handle);
        self.current_format = Some(format);
    }

    #[inline]
    pub fn set_lightmap(&mut self, handle: TextureHandle, format: TextureFormat) {
        self.lightmap = Some(handle);
        self.lightmap_format = Some(format);
    }

    #[inline]
    pub fn set_current(&mut self, handle: TextureHandle, format: TextureFormat) {
        self.current = Some(handle);
        self.current_format = Some(format);
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
        frame_payloads: &'a FramePayloads2D<'a>,
    ) -> FeatureExecutionContext2D<'a> {
        FeatureExecutionContext2D {
            view_index,
            view_state: &self.view_states[view_index],
            frame_payloads,
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

/// Execution-time access to the prepared view and frame-scoped render payloads.
pub struct FeatureExecutionContext2D<'a> {
    pub(crate) view_index: usize,
    pub(crate) view_state: &'a ViewPipelineState2D,
    pub(crate) frame_payloads: &'a FramePayloads2D<'a>,
}

impl<'a> FeatureExecutionContext2D<'a> {
    #[inline]
    pub fn state(&self) -> &PipelineState2D {
        &self.view_state.pipeline
    }

    #[inline]
    pub fn view_index(&self) -> usize {
        self.view_index
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
    pub fn gpu_scene(&self) -> Option<&'a GpuScene2D> {
        self.frame_payloads.gpu_scene()
    }

    #[inline]
    pub fn payload<T: Any>(&self) -> Option<&'a T> {
        self.frame_payloads.get::<T>()
    }

    #[inline]
    pub fn has_payload<T: Any>(&self) -> bool {
        self.frame_payloads.contains::<T>()
    }

    #[inline]
    pub fn sprite_table_buffer(&self) -> &wgpu::Buffer {
        self.require_gpu_scene().sprite_table_buffer()
    }

    #[inline]
    pub fn light_table_buffer(&self) -> &wgpu::Buffer {
        self.require_gpu_scene().light_table_buffer()
    }

    #[inline]
    pub fn sprite_table_version(&self) -> u64 {
        self.require_gpu_scene().sprite_table_version()
    }

    #[inline]
    pub fn light_table_version(&self) -> u64 {
        self.require_gpu_scene().light_table_version()
    }

    #[inline]
    pub fn visible_sprite_index_buffer(&self) -> &wgpu::Buffer {
        self.require_gpu_scene().visible_sprite_index_buffer()
    }

    #[inline]
    pub fn visible_light_index_buffer(&self) -> &wgpu::Buffer {
        self.require_gpu_scene().visible_light_index_buffer()
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
        self.require_gpu_scene()
            .draw_spans_for_view(self.view_state.view)
    }

    #[inline]
    pub fn textures(&self) -> &[Texture] {
        self.require_gpu_scene().textures()
    }

    #[inline]
    fn require_gpu_scene(&self) -> &'a GpuScene2D {
        self.gpu_scene().expect(
            "FeatureExecutionContext2D requires a GpuScene2D payload for sprite/light access",
        )
    }
}
