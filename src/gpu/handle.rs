//! Typed, generational GPU resource handles.
//!
//! Inspired by bgfx/sokol-gfx handle-based design and SakuraEngine CGPU's
//! opaque ID pattern — adapted to Rust with compile-time type safety and
//! generation counters for use-after-free detection.

use std::marker::PhantomData;

// ── Handle type tags (zero-sized, compile-time only) ────────────────────────

/// Marker trait for handle type tags.
pub trait HandleTag: 'static {}

macro_rules! define_tag {
    ($name:ident) => {
        #[doc = concat!("Type tag for [`", stringify!($name), "`] handles.")]
        #[derive(Debug)]
        pub enum $name {}
        impl HandleTag for $name {}
    };
}

define_tag!(BufferTag);
define_tag!(ImageTag);
define_tag!(ImageViewTag);
define_tag!(SamplerTag);
define_tag!(ShaderTag);
define_tag!(PipelineTag);
define_tag!(ComputePipelineTag);
define_tag!(BindGroupTag);
define_tag!(BindGroupLayoutTag);

/// GPU buffer handle.
pub type Buffer = Handle<BufferTag>;
/// GPU image (texture) handle.
pub type Image = Handle<ImageTag>;
/// GPU image view handle (sub-view of an image: mip level, array layer).
pub type ImageView = Handle<ImageViewTag>;
/// GPU sampler handle.
pub type Sampler = Handle<SamplerTag>;
/// Compiled shader module handle.
pub type Shader = Handle<ShaderTag>;
/// Render pipeline handle.
pub type Pipeline = Handle<PipelineTag>;
/// Compute pipeline handle.
pub type ComputePipeline = Handle<ComputePipelineTag>;
/// Bind group (descriptor set) handle.
pub type BindGroup = Handle<BindGroupTag>;
/// Bind group layout handle.
pub type BindGroupLayout = Handle<BindGroupLayoutTag>;

// ── Handle ──────────────────────────────────────────────────────────────────

/// A typed, generational GPU resource handle.
///
/// Handles are lightweight (`Copy`, 8 bytes), type-safe identifiers for GPU
/// resources managed by a [`HandlePool`]. The generation counter prevents
/// use-after-free bugs — a stale handle will fail validation when the slot
/// has been reused for a new resource.
///
/// Handles can be stored directly in ECS components.
#[derive(Debug)]
pub struct Handle<T: HandleTag> {
    index: u32,
    generation: u32,
    _marker: PhantomData<T>,
}

// Manual impls to avoid requiring T: Clone/Copy/etc.
impl<T: HandleTag> Clone for Handle<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: HandleTag> Copy for Handle<T> {}

impl<T: HandleTag> PartialEq for Handle<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}

impl<T: HandleTag> Eq for Handle<T> {}

impl<T: HandleTag> std::hash::Hash for Handle<T> {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(self.index);
        state.write_u32(self.generation);
    }
}

impl<T: HandleTag> Handle<T> {
    /// The raw index into the pool. For backend use only.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn index(self) -> u32 {
        self.index
    }

    /// Construct a handle from a raw index (generation 0).
    ///
    /// This is intended for tests and benchmarks that need to create
    /// handles without a `HandlePool`. **Not** for production use.
    #[inline]
    pub fn from_raw(index: u32) -> Self {
        Self {
            index,
            generation: 0,
            _marker: PhantomData,
        }
    }
}

// ── HandlePool ──────────────────────────────────────────────────────────────

struct PoolEntry<R> {
    resource: Option<R>,
    generation: u32,
}

/// A generational arena that maps [`Handle<T>`] → backend resource `R`.
///
/// Used internally by GPU backends to manage the lifecycle of GPU resources.
/// Allocating returns a `Handle`; freeing bumps the generation so stale
/// handles are automatically invalidated.
pub(crate) struct HandlePool<T: HandleTag, R> {
    entries: Vec<PoolEntry<R>>,
    free_list: Vec<u32>,
    _marker: PhantomData<T>,
}

impl<T: HandleTag, R> HandlePool<T, R> {
    /// Create an empty pool.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            free_list: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// Insert a resource and return its handle.
    pub fn insert(&mut self, resource: R) -> Handle<T> {
        if let Some(index) = self.free_list.pop() {
            let entry = &mut self.entries[index as usize];
            debug_assert!(entry.resource.is_none());
            entry.resource = Some(resource);
            Handle {
                index,
                generation: entry.generation,
                _marker: PhantomData,
            }
        } else {
            let index = self.entries.len() as u32;
            self.entries.push(PoolEntry {
                resource: Some(resource),
                generation: 0,
            });
            Handle {
                index,
                generation: 0,
                _marker: PhantomData,
            }
        }
    }

    /// Get a reference to the resource behind `handle`.
    ///
    /// Returns `None` if the handle is stale (generation mismatch) or the
    /// slot is vacant.
    #[inline]
    pub fn get(&self, handle: Handle<T>) -> Option<&R> {
        let entry = self.entries.get(handle.index as usize)?;
        if entry.generation != handle.generation {
            return None;
        }
        entry.resource.as_ref()
    }

    /// Get a mutable reference to the resource behind `handle`.
    #[inline]
    #[allow(dead_code)]
    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut R> {
        let entry = self.entries.get_mut(handle.index as usize)?;
        if entry.generation != handle.generation {
            return None;
        }
        entry.resource.as_mut()
    }

    /// Remove and return the resource behind `handle`.
    ///
    /// The slot's generation is bumped so any remaining copies of this handle
    /// become stale.
    pub fn remove(&mut self, handle: Handle<T>) -> Option<R> {
        let entry = self.entries.get_mut(handle.index as usize)?;
        if entry.generation != handle.generation {
            return None;
        }
        let resource = entry.resource.take()?;
        entry.generation = entry.generation.wrapping_add(1);
        self.free_list.push(handle.index);
        Some(resource)
    }

    /// Remove and return all live resources, clearing the pool.
    ///
    /// Used by backend `Drop` implementations to ensure deterministic
    /// cleanup of GPU resources.
    pub fn drain(&mut self) -> impl Iterator<Item = R> + '_ {
        self.free_list.clear();
        self.entries.iter_mut().filter_map(|entry| entry.resource.take())
    }

    /// Number of live (occupied) slots.
    #[allow(dead_code)]
    pub fn live_count(&self) -> usize {
        self.entries.iter().filter(|e| e.resource.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove() {
        let mut pool: HandlePool<BufferTag, String> = HandlePool::new();
        let h = pool.insert("hello".to_string());
        assert_eq!(pool.get(h).unwrap(), "hello");

        let removed = pool.remove(h).unwrap();
        assert_eq!(removed, "hello");
        assert!(pool.get(h).is_none(), "stale handle should return None");
    }

    #[test]
    fn generation_prevents_reuse() {
        let mut pool: HandlePool<BufferTag, i32> = HandlePool::new();
        let h1 = pool.insert(42);
        pool.remove(h1);

        let h2 = pool.insert(99);
        // h2 reuses the same slot but with bumped generation
        assert_eq!(h2.index, h1.index);
        assert_ne!(h2.generation, h1.generation);

        // old handle is stale
        assert!(pool.get(h1).is_none());
        // new handle works
        assert_eq!(pool.get(h2), Some(&99));
    }

    #[test]
    fn handle_is_copy_and_small() {
        assert_eq!(std::mem::size_of::<Buffer>(), 8);
        let mut pool: HandlePool<BufferTag, u64> = HandlePool::new();
        let h = pool.insert(123);
        let h2 = h; // Copy
        assert_eq!(h, h2);
        assert_eq!(pool.get(h), pool.get(h2));
    }

    #[test]
    fn drain_returns_all_live_resources() {
        let mut pool: HandlePool<BufferTag, String> = HandlePool::new();
        let h1 = pool.insert("a".into());
        let _h2 = pool.insert("b".into());
        let _h3 = pool.insert("c".into());
        pool.remove(h1); // remove one

        assert_eq!(pool.live_count(), 2);
        let drained: Vec<String> = pool.drain().collect();
        assert_eq!(drained.len(), 2);
        assert!(drained.contains(&"b".to_string()));
        assert!(drained.contains(&"c".to_string()));
        assert_eq!(pool.live_count(), 0);
    }
}
