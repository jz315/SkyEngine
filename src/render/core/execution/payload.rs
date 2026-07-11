use std::any::{Any, TypeId};

use rustc_hash::FxHashMap;

use crate::render::view::ViewportRect;

use super::TextureFormat;

type PayloadEntry<'a> = (TypeId, &'a dyn Any);

struct InlinePayloadStore<'a, const INLINE: usize> {
    inline: [Option<PayloadEntry<'a>>; INLINE],
    inline_len: usize,
    overflow: FxHashMap<TypeId, &'a dyn Any>,
}

impl<'a, const INLINE: usize> Default for InlinePayloadStore<'a, INLINE> {
    fn default() -> Self {
        Self {
            inline: [None; INLINE],
            inline_len: 0,
            overflow: FxHashMap::default(),
        }
    }
}

impl<'a, const INLINE: usize> InlinePayloadStore<'a, INLINE> {
    fn insert<T: Any>(&mut self, value: &'a T) -> Option<&'a T> {
        let type_id = TypeId::of::<T>();
        for entry in self.inline[..self.inline_len].iter_mut().flatten() {
            if entry.0 == type_id {
                let previous = std::mem::replace(&mut entry.1, value as &'a dyn Any);
                return previous.downcast_ref::<T>();
            }
        }

        if self.inline_len < INLINE {
            self.inline[self.inline_len] = Some((type_id, value as &'a dyn Any));
            self.inline_len += 1;
            return None;
        }

        self.overflow
            .insert(type_id, value as &'a dyn Any)
            .and_then(|previous| previous.downcast_ref::<T>())
    }

    fn contains<T: Any>(&self) -> bool {
        let type_id = TypeId::of::<T>();
        self.inline[..self.inline_len]
            .iter()
            .flatten()
            .any(|entry| entry.0 == type_id)
            || self.overflow.contains_key(&type_id)
    }

    fn get<T: Any>(&self) -> Option<&'a T> {
        let type_id = TypeId::of::<T>();
        if let Some((_, value)) = self.inline[..self.inline_len]
            .iter()
            .flatten()
            .find(|entry| entry.0 == type_id)
            .copied()
        {
            return value.downcast_ref::<T>();
        }
        self.overflow
            .get(&type_id)
            .and_then(|value| value.downcast_ref::<T>())
    }
}

#[derive(Default)]
pub struct FramePayloadStore<'a> {
    typed: InlinePayloadStore<'a, 12>,
}

impl<'a> FramePayloadStore<'a> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn insert<T: Any>(&mut self, value: &'a T) -> Option<&'a T> {
        self.typed.insert(value)
    }

    #[inline]
    pub fn contains<T: Any>(&self) -> bool {
        self.typed.contains::<T>()
    }

    #[inline]
    pub fn get<T: Any>(&self) -> Option<&'a T> {
        self.typed.get::<T>()
    }
}

#[derive(Default)]
pub struct ViewPayloadStore<'a> {
    typed: InlinePayloadStore<'a, 6>,
}

impl<'a> ViewPayloadStore<'a> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn insert<T: Any>(&mut self, value: &'a T) -> Option<&'a T> {
        self.typed.insert(value)
    }

    #[inline]
    pub fn contains<T: Any>(&self) -> bool {
        self.typed.contains::<T>()
    }

    #[inline]
    pub fn get<T: Any>(&self) -> Option<&'a T> {
        self.typed.get::<T>()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_payload_store_replaces_values_and_uses_overflow() {
        let first = 7u32;
        let replacement = 9u32;
        let overflow = 11u64;
        let mut store = InlinePayloadStore::<1>::default();

        assert!(store.insert(&first).is_none());
        assert!(store.insert(&overflow).is_none());
        assert_eq!(store.insert(&replacement), Some(&first));
        assert_eq!(store.get::<u32>(), Some(&replacement));
        assert_eq!(store.get::<u64>(), Some(&overflow));
        assert!(store.contains::<u32>());
        assert!(store.contains::<u64>());
    }
}
