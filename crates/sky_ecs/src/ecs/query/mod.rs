mod filter;
mod parallel;
mod param;
mod prepared;

use super::{Chunk, EntityId, World};
use crate::ecs::ComponentType;
use core::ptr;
use smallvec::SmallVec;

pub use filter::{QueryFilter, With, Without};
pub use param::{QuerySpec, ReadOnlyQuerySpec};
pub use prepared::PreparedQuery;

const INLINE_QUERY_COMPONENTS: usize = 8;
const OPTIONAL_SENTINEL: u8 = u8::MAX;

#[derive(Clone, Copy)]
pub struct QueryComponent {
    pub(crate) ty: ComponentType,
    pub(crate) mutable: bool,
    pub(crate) optional: bool,
}

impl QueryComponent {
    pub(crate) fn new(ty: ComponentType, mutable: bool) -> Self {
        Self {
            ty,
            mutable,
            optional: false,
        }
    }

    pub(crate) fn optional(ty: ComponentType, mutable: bool) -> Self {
        Self {
            ty,
            mutable,
            optional: true,
        }
    }
}

pub struct QueryDescriptor {
    pub(crate) components: SmallVec<[QueryComponent; INLINE_QUERY_COMPONENTS]>,
}

impl QueryDescriptor {
    pub(crate) fn new(components: SmallVec<[QueryComponent; INLINE_QUERY_COMPONENTS]>) -> Self {
        for (index, component) in components.iter().enumerate() {
            for other in &components[(index + 1)..] {
                if component.ty.id() == other.ty.id() {
                    let access_mode = if component.mutable || other.mutable {
                        "mutable"
                    } else {
                        "shared"
                    };
                    panic!(
                        "duplicate component type `{}` is not supported in {} queries",
                        component.ty.name, access_mode
                    );
                }
            }
        }

        Self { components }
    }

    pub(crate) fn len(&self) -> usize {
        self.components.len()
    }
}

#[derive(Clone)]
pub(crate) struct CachedArchetype {
    pub data_index: usize,
    pub component_indices: SmallVec<[u8; INLINE_QUERY_COMPONENTS]>,
}

#[derive(Default)]
pub(crate) struct PreparedCache {
    cached_epoch: Option<usize>,
    pub archetypes: Vec<CachedArchetype>,
}

#[doc(hidden)]
pub trait QueryWorld<Q: QuerySpec>: sealed::QueryWorldSealed {
    fn as_world(&self) -> &World;
}

mod sealed {
    use super::World;

    pub trait QueryWorldSealed {}

    impl QueryWorldSealed for &World {}
    impl QueryWorldSealed for &mut World {}
}

impl<Q: QuerySpec> QueryWorld<Q> for &mut World {
    #[inline(always)]
    fn as_world(&self) -> &World {
        self
    }
}

impl<Q: ReadOnlyQuerySpec> QueryWorld<Q> for &World {
    #[inline(always)]
    fn as_world(&self) -> &World {
        self
    }
}

impl PreparedCache {
    #[inline(always)]
    pub fn prepare<Flt: QueryFilter>(&mut self, world: &World, descriptor: &QueryDescriptor) {
        let current_epoch = world.archetype_epoch();
        if self.cached_epoch == Some(current_epoch) {
            return;
        }

        self.archetypes.clear();

        for (data_index, data) in world.data.iter().enumerate() {
            let archetype = data.archetype;

            if !Flt::matches_archetype(&archetype) {
                continue;
            }

            let mut component_indices =
                SmallVec::<[u8; INLINE_QUERY_COMPONENTS]>::with_capacity(descriptor.len());

            let mut matches = true;
            for component in &descriptor.components {
                if let Some(index) = archetype.query_component_index(&component.ty) {
                    debug_assert!(index < OPTIONAL_SENTINEL as usize);
                    component_indices.push(index as u8);
                } else if component.optional {
                    component_indices.push(OPTIONAL_SENTINEL);
                } else {
                    matches = false;
                    break;
                }
            }

            if matches {
                self.archetypes.push(CachedArchetype {
                    data_index,
                    component_indices,
                });
            }
        }

        self.cached_epoch = Some(current_epoch);
    }

    #[inline(always)]
    pub fn visit_chunks<'w, F>(&self, world: &'w World, mut f: F)
    where
        F: FnMut(&CachedArchetype, &'w Chunk),
    {
        for cached in &self.archetypes {
            let data = &world.data[cached.data_index];

            for chunk in &data.chunks {
                debug_assert!(chunk.entity_count != 0);
                f(cached, chunk);
            }
        }
    }

    pub fn cached_archetype_count(&self) -> usize {
        self.archetypes.len()
    }
}

#[inline(always)]
pub(crate) fn resolve_column_ptr(chunk: &Chunk, index: u8) -> *mut u8 {
    if index == OPTIONAL_SENTINEL {
        ptr::null_mut()
    } else {
        chunk.column_ptr(index as usize)
    }
}
