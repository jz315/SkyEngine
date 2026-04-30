use std::any::{Any, TypeId};

use rustc_hash::FxHashMap;

use crate::render::view::ViewportRect;

use super::TextureFormat;

#[derive(Default)]
pub struct FramePayloadStore<'a> {
    typed: FxHashMap<TypeId, &'a dyn Any>,
}

impl<'a> FramePayloadStore<'a> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
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

#[derive(Default)]
pub struct ViewPayloadStore<'a> {
    typed: FxHashMap<TypeId, &'a dyn Any>,
}

impl<'a> ViewPayloadStore<'a> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
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

pub struct PreparedView<'a> {
    order: i32,
    history_key: u64,
    viewport: ViewportRect,
    target_size: [u32; 2],
    clear_surface: bool,
    payloads: ViewPayloadStore<'a>,
}

impl<'a> PreparedView<'a> {
    #[inline]
    pub fn new(
        order: i32,
        viewport: ViewportRect,
        target_size: [u32; 2],
        clear_surface: bool,
    ) -> Self {
        Self {
            order,
            history_key: 0,
            viewport,
            target_size: [target_size[0].max(1), target_size[1].max(1)],
            clear_surface,
            payloads: ViewPayloadStore::new(),
        }
    }

    #[inline]
    pub fn order(&self) -> i32 {
        self.order
    }

    #[inline]
    pub fn history_key(&self) -> u64 {
        self.history_key
    }

    #[inline]
    pub fn set_history_key(&mut self, history_key: u64) {
        self.history_key = history_key;
    }

    #[inline]
    pub fn with_history_key(mut self, history_key: u64) -> Self {
        self.history_key = history_key;
        self
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
    pub fn insert_payload<T: Any>(&mut self, value: &'a T) -> Option<&'a T> {
        self.payloads.insert(value)
    }

    #[inline]
    pub fn with_payload<T: Any>(mut self, value: &'a T) -> Self {
        let _ = self.insert_payload(value);
        self
    }

    #[inline]
    pub fn payload<T: Any>(&self) -> Option<&'a T> {
        self.payloads.get::<T>()
    }

    #[inline]
    pub fn has_payload<T: Any>(&self) -> bool {
        self.payloads.contains::<T>()
    }
}

pub struct PreparedFrame<'a> {
    surface_format: TextureFormat,
    has_surface: bool,
    payloads: FramePayloadStore<'a>,
    views: Vec<PreparedView<'a>>,
}

impl<'a> PreparedFrame<'a> {
    #[inline]
    pub fn new(surface_format: TextureFormat, has_surface: bool) -> Self {
        Self {
            surface_format,
            has_surface,
            payloads: FramePayloadStore::new(),
            views: Vec::with_capacity(4),
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
    pub fn insert_payload<T: Any>(&mut self, value: &'a T) -> Option<&'a T> {
        self.payloads.insert(value)
    }

    #[inline]
    pub fn with_payload<T: Any>(mut self, value: &'a T) -> Self {
        let _ = self.insert_payload(value);
        self
    }

    #[inline]
    pub fn payload<T: Any>(&self) -> Option<&'a T> {
        self.payloads.get::<T>()
    }

    #[inline]
    pub fn has_payload<T: Any>(&self) -> bool {
        self.payloads.contains::<T>()
    }

    #[inline]
    pub fn add_view(&mut self, view: PreparedView<'a>) -> &mut Self {
        self.views.push(view);
        self
    }

    #[inline]
    pub fn views(&self) -> &[PreparedView<'a>] {
        &self.views
    }

    #[inline]
    pub fn view(&self, index: usize) -> &PreparedView<'a> {
        &self.views[index]
    }

    #[inline]
    pub fn view_count(&self) -> usize {
        self.views.len()
    }
}
