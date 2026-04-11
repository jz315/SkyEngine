use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::render::core::viewport::ViewportRect;
use crate::render::graph::{BufferHandle, TextureHandle};

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
}

impl PhaseState {
    #[inline]
    pub fn new(surface_format: TextureFormat, has_surface: bool) -> Self {
        Self {
            surface_format,
            has_surface,
            slots: ResourceSlotMap::default(),
        }
    }

    #[inline]
    pub fn with_slots(
        surface_format: TextureFormat,
        has_surface: bool,
        slots: ResourceSlotMap,
    ) -> Self {
        Self {
            surface_format,
            has_surface,
            slots,
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
    pub fn current_color(&self) -> Option<TextureSlot> {
        self.slots.current_color()
    }

    #[inline]
    pub fn set_current_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        self.slots.set_current_color(handle, format);
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
}

#[derive(Debug, Clone)]
pub struct CompletedViewState {
    view_index: usize,
    order: i32,
    viewport: ViewportRect,
    target_size: [u32; 2],
    clear_surface: bool,
    slots: ResourceSlotMap,
}

impl CompletedViewState {
    pub(crate) fn new(view_index: usize, view: &PreparedView<'_>, slots: ResourceSlotMap) -> Self {
        Self {
            view_index,
            order: view.order(),
            viewport: view.viewport(),
            target_size: view.target_size(),
            clear_surface: view.clear_surface(),
            slots,
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
}

pub struct FinalizePhaseState<'a> {
    surface_format: TextureFormat,
    has_surface: bool,
    slots: &'a mut ResourceSlotMap,
    completed_views: &'a [CompletedViewState],
}

impl<'a> FinalizePhaseState<'a> {
    pub(crate) fn new(
        surface_format: TextureFormat,
        has_surface: bool,
        slots: &'a mut ResourceSlotMap,
        completed_views: &'a [CompletedViewState],
    ) -> Self {
        Self {
            surface_format,
            has_surface,
            slots,
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
    pub fn completed_views(&self) -> &'a [CompletedViewState] {
        self.completed_views
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
    pub fn set_texture_slot(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> Option<TextureSlot> {
        self.slots.insert_texture(name, handle, format)
    }
}
