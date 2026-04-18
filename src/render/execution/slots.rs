use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::render::graph::{BufferHandle, TextureHandle};
use crate::render::view::ViewportRect;

use super::payload::PreparedView;
use super::TextureFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureSlot {
    handle: TextureHandle,
    format: TextureFormat,
}

impl TextureSlot {
    #[inline]
    pub const fn new(handle: TextureHandle, format: TextureFormat) -> Self {
        Self { handle, format }
    }

    #[inline]
    pub const fn handle(self) -> TextureHandle {
        self.handle
    }

    #[inline]
    pub const fn format(self) -> TextureFormat {
        self.format
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneGBufferSlots {
    color: Option<TextureSlot>,
    depth: Option<TextureSlot>,
    normal: Option<TextureSlot>,
    velocity: Option<TextureSlot>,
    albedo: Option<TextureSlot>,
    material: Option<TextureSlot>,
    emissive: Option<TextureSlot>,
}

impl SceneGBufferSlots {
    #[inline]
    pub fn color(&self) -> Option<TextureSlot> {
        self.color
    }

    #[inline]
    pub fn depth(&self) -> Option<TextureSlot> {
        self.depth
    }

    #[inline]
    pub fn normal(&self) -> Option<TextureSlot> {
        self.normal
    }

    #[inline]
    pub fn velocity(&self) -> Option<TextureSlot> {
        self.velocity
    }

    #[inline]
    pub fn albedo(&self) -> Option<TextureSlot> {
        self.albedo
    }

    #[inline]
    pub fn material(&self) -> Option<TextureSlot> {
        self.material
    }

    #[inline]
    pub fn emissive(&self) -> Option<TextureSlot> {
        self.emissive
    }

    #[inline]
    pub fn set_color(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.color;
        self.color = Some(slot);
        previous
    }

    #[inline]
    pub fn set_depth(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.depth;
        self.depth = Some(slot);
        previous
    }

    #[inline]
    pub fn set_normal(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.normal;
        self.normal = Some(slot);
        previous
    }

    #[inline]
    pub fn set_velocity(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.velocity;
        self.velocity = Some(slot);
        previous
    }

    #[inline]
    pub fn set_albedo(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.albedo;
        self.albedo = Some(slot);
        previous
    }

    #[inline]
    pub fn set_material(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.material;
        self.material = Some(slot);
        previous
    }

    #[inline]
    pub fn set_emissive(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.emissive;
        self.emissive = Some(slot);
        previous
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotResource {
    Texture(TextureSlot),
    Buffer(BufferHandle),
}

#[derive(Debug, Clone, Default)]
pub struct ResourceSlotMap {
    slots: FxHashMap<Cow<'static, str>, SlotResource>,
}

impl ResourceSlotMap {
    pub const CURRENT_COLOR: &'static str = "current_color";

    #[inline]
    pub fn insert_texture(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> Option<TextureSlot> {
        self.slots
            .insert(
                name.into(),
                SlotResource::Texture(TextureSlot::new(handle, format)),
            )
            .and_then(|previous| match previous {
                SlotResource::Texture(slot) => Some(slot),
                SlotResource::Buffer(_) => None,
            })
    }

    #[inline]
    pub fn insert_buffer(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        handle: BufferHandle,
    ) -> Option<BufferHandle> {
        self.slots
            .insert(name.into(), SlotResource::Buffer(handle))
            .and_then(|previous| match previous {
                SlotResource::Buffer(handle) => Some(handle),
                SlotResource::Texture(_) => None,
            })
    }

    #[inline]
    pub fn texture(&self, name: &str) -> Option<TextureSlot> {
        self.slots.get(name).and_then(|resource| match resource {
            SlotResource::Texture(slot) => Some(*slot),
            SlotResource::Buffer(_) => None,
        })
    }

    #[inline]
    pub fn buffer(&self, name: &str) -> Option<BufferHandle> {
        self.slots.get(name).and_then(|resource| match resource {
            SlotResource::Buffer(handle) => Some(*handle),
            SlotResource::Texture(_) => None,
        })
    }

    #[inline]
    pub fn resource(&self, name: &str) -> Option<SlotResource> {
        self.slots.get(name).copied()
    }

    #[inline]
    pub fn contains(&self, name: &str) -> bool {
        self.slots.contains_key(name)
    }

    #[inline]
    pub fn set_current_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        let _ = self.insert_texture(Self::CURRENT_COLOR, handle, format);
    }

    #[inline]
    pub fn current_color(&self) -> Option<TextureSlot> {
        self.texture(Self::CURRENT_COLOR)
    }
}

#[derive(Debug, Clone)]
pub struct PhaseState {
    surface_format: TextureFormat,
    has_surface: bool,
    slots: ResourceSlotMap,
    scene_gbuffer: SceneGBufferSlots,
}

impl PhaseState {
    #[inline]
    pub fn new(surface_format: TextureFormat, has_surface: bool) -> Self {
        Self {
            surface_format,
            has_surface,
            slots: ResourceSlotMap::default(),
            scene_gbuffer: SceneGBufferSlots::default(),
        }
    }

    #[inline]
    pub fn with_slots(
        surface_format: TextureFormat,
        has_surface: bool,
        slots: ResourceSlotMap,
        scene_gbuffer: SceneGBufferSlots,
    ) -> Self {
        Self {
            surface_format,
            has_surface,
            slots,
            scene_gbuffer,
        }
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
    pub fn slots(&self) -> &ResourceSlotMap {
        &self.slots
    }

    #[inline]
    pub fn slots_mut(&mut self) -> &mut ResourceSlotMap {
        &mut self.slots
    }

    #[inline]
    pub fn scene_gbuffer(&self) -> &SceneGBufferSlots {
        &self.scene_gbuffer
    }

    #[inline]
    pub fn scene_gbuffer_mut(&mut self) -> &mut SceneGBufferSlots {
        &mut self.scene_gbuffer
    }

    #[inline]
    pub fn current_color(&self) -> Option<TextureSlot> {
        self.slots.current_color()
    }

    #[inline]
    pub fn set_current_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        self.slots.set_current_color(handle, format);
    }

    #[inline]
    pub fn scene_color(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.color()
    }

    #[inline]
    pub fn scene_depth(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.depth()
    }

    #[inline]
    pub fn scene_normal(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.normal()
    }

    #[inline]
    pub fn scene_velocity(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.velocity()
    }

    #[inline]
    pub fn scene_albedo(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.albedo()
    }

    #[inline]
    pub fn scene_material(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.material()
    }

    #[inline]
    pub fn scene_emissive(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.emissive()
    }

    #[inline]
    pub fn set_scene_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_color(slot);
    }

    #[inline]
    pub fn set_scene_depth(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_depth(slot);
    }

    #[inline]
    pub fn set_scene_normal(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_normal(slot);
    }

    #[inline]
    pub fn set_scene_velocity(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_velocity(slot);
    }

    #[inline]
    pub fn set_scene_albedo(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_albedo(slot);
    }

    #[inline]
    pub fn set_scene_material(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_material(slot);
    }

    #[inline]
    pub fn set_scene_emissive(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_emissive(slot);
    }

    #[inline]
    pub fn texture_slot(&self, name: &str) -> Option<TextureSlot> {
        self.slots.texture(name)
    }

    #[inline]
    pub fn set_texture_slot(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> Option<TextureSlot> {
        self.slots.insert_texture(name, handle, format)
    }

    #[inline]
    pub fn into_slots(self) -> ResourceSlotMap {
        self.slots
    }

    #[inline]
    pub fn into_parts(self) -> (ResourceSlotMap, SceneGBufferSlots) {
        (self.slots, self.scene_gbuffer)
    }
}

#[derive(Debug, Clone)]
pub struct CompletedViewState {
    view_index: usize,
    order: i32,
    viewport: ViewportRect,
    target_size: [u32; 2],
    clear_surface: bool,
    slots: ResourceSlotMap,
    scene_gbuffer: SceneGBufferSlots,
}

impl CompletedViewState {
    pub(crate) fn new(
        view_index: usize,
        view: &PreparedView<'_>,
        slots: ResourceSlotMap,
        scene_gbuffer: SceneGBufferSlots,
    ) -> Self {
        Self {
            view_index,
            order: view.order(),
            viewport: view.viewport(),
            target_size: view.target_size(),
            clear_surface: view.clear_surface(),
            slots,
            scene_gbuffer,
        }
    }

    #[inline]
    pub fn view_index(&self) -> usize {
        self.view_index
    }

    #[inline]
    pub fn order(&self) -> i32 {
        self.order
    }

    #[inline]
    pub fn viewport(&self) -> ViewportRect {
        self.viewport
    }

    #[inline]
    pub fn target_size(&self) -> [u32; 2] {
        self.target_size
    }

    #[inline]
    pub fn clear_surface(&self) -> bool {
        self.clear_surface
    }

    #[inline]
    pub fn slots(&self) -> &ResourceSlotMap {
        &self.slots
    }

    #[inline]
    pub fn scene_gbuffer(&self) -> &SceneGBufferSlots {
        &self.scene_gbuffer
    }

    #[inline]
    pub fn scene_color(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.color()
    }

    #[inline]
    pub fn scene_depth(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.depth()
    }

    #[inline]
    pub fn scene_normal(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.normal()
    }

    #[inline]
    pub fn scene_velocity(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.velocity()
    }

    #[inline]
    pub fn scene_albedo(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.albedo()
    }

    #[inline]
    pub fn scene_material(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.material()
    }

    #[inline]
    pub fn scene_emissive(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.emissive()
    }
}

pub struct FinalizePhaseState<'a> {
    surface_format: TextureFormat,
    has_surface: bool,
    slots: &'a mut ResourceSlotMap,
    scene_gbuffer: &'a mut SceneGBufferSlots,
    completed_views: &'a [CompletedViewState],
}

impl<'a> FinalizePhaseState<'a> {
    pub(crate) fn new(
        surface_format: TextureFormat,
        has_surface: bool,
        slots: &'a mut ResourceSlotMap,
        scene_gbuffer: &'a mut SceneGBufferSlots,
        completed_views: &'a [CompletedViewState],
    ) -> Self {
        Self {
            surface_format,
            has_surface,
            slots,
            scene_gbuffer,
            completed_views,
        }
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
    pub fn slots(&self) -> &ResourceSlotMap {
        self.slots
    }

    #[inline]
    pub fn slots_mut(&mut self) -> &mut ResourceSlotMap {
        self.slots
    }

    #[inline]
    pub fn scene_gbuffer(&self) -> &SceneGBufferSlots {
        self.scene_gbuffer
    }

    #[inline]
    pub fn scene_gbuffer_mut(&mut self) -> &mut SceneGBufferSlots {
        self.scene_gbuffer
    }

    #[inline]
    pub fn completed_views(&self) -> &'a [CompletedViewState] {
        self.completed_views
    }

    #[inline]
    pub fn current_color(&self) -> Option<TextureSlot> {
        self.slots.current_color()
    }

    #[inline]
    pub fn scene_color(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.color()
    }

    #[inline]
    pub fn scene_depth(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.depth()
    }

    #[inline]
    pub fn scene_normal(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.normal()
    }

    #[inline]
    pub fn scene_velocity(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.velocity()
    }

    #[inline]
    pub fn scene_albedo(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.albedo()
    }

    #[inline]
    pub fn scene_material(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.material()
    }

    #[inline]
    pub fn scene_emissive(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.emissive()
    }

    #[inline]
    pub fn set_current_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        self.slots.set_current_color(handle, format);
    }

    #[inline]
    pub fn set_scene_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_color(slot);
    }

    #[inline]
    pub fn set_scene_depth(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_depth(slot);
    }

    #[inline]
    pub fn set_scene_normal(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_normal(slot);
    }

    #[inline]
    pub fn set_scene_velocity(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_velocity(slot);
    }

    #[inline]
    pub fn set_scene_albedo(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_albedo(slot);
    }

    #[inline]
    pub fn set_scene_material(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_material(slot);
    }

    #[inline]
    pub fn set_scene_emissive(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_emissive(slot);
    }

    #[inline]
    pub fn set_texture_slot(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> Option<TextureSlot> {
        self.slots.insert_texture(name, handle, format)
    }
}
