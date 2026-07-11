mod bound;
mod cache;
mod filter;
mod parallel;
mod param;
mod prepared;

use super::{Chunk, EntityId, World};
use crate::ecs::ComponentType;
use core::ops::Deref;
use core::ptr;
use smallvec::SmallVec;
use std::sync::Arc;

pub(crate) use bound::{
    count_matches, matches_nothing, run_for_each, run_for_each_chunk,
    run_for_each_chunk_with_entities, run_for_each_with_entity,
};
pub use bound::{Query, QueryMut};
pub(crate) use cache::QueryCacheStore;
pub use filter::{Any, QueryFilter, With, Without};
pub(crate) use parallel::{
    par_for_each, par_for_each_chunk, par_for_each_chunk_with_entities, par_for_each_with_entity,
    prepare_job_cache, prepared_job_snapshot, ParallelJobCache, ParallelJobSnapshot,
};
pub use param::{QueryParam, QuerySpec, ReadOnlyQuerySpec};
pub use prepared::PreparedQuery;

pub trait QueryFilterSealed {}

const INLINE_QUERY_COMPONENTS: usize = 8;
const MAX_QUERY_COMPONENTS: usize = 16;
const OPTIONAL_SENTINEL: u8 = u8::MAX;

#[derive(Clone, Copy)]
pub(crate) struct ComponentIndexMap {
    indices: [u8; MAX_QUERY_COMPONENTS],
    len: u8,
}

impl ComponentIndexMap {
    #[inline]
    fn new(len: usize) -> Self {
        debug_assert!(len <= MAX_QUERY_COMPONENTS);
        Self {
            indices: [OPTIONAL_SENTINEL; MAX_QUERY_COMPONENTS],
            len: len as u8,
        }
    }

    #[inline]
    fn push(&mut self, index: u8) {
        let slot = self.len as usize;
        debug_assert!(slot < MAX_QUERY_COMPONENTS);
        self.indices[slot] = index;
        self.len += 1;
    }

    #[inline(always)]
    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.indices[..self.len as usize]
    }
}

impl Deref for ComponentIndexMap {
    type Target = [u8];

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.indices[..self.len as usize]
    }
}

#[derive(Clone, Copy)]
struct FilterTerm {
    ty: ComponentType,
    present: bool,
}

enum FilterPlan {
    Legacy,
    Always,
    Never,
    Terms(SmallVec<[FilterTerm; INLINE_QUERY_COMPONENTS]>),
}

impl FilterPlan {
    fn compile<Flt: QueryFilter>(descriptor: &QueryDescriptor) -> Self {
        let mut raw_terms = SmallVec::<[FilterTerm; MAX_QUERY_COMPONENTS]>::new();
        let complete = Flt::collect_conjunctive_terms(&mut |ty, present| {
            raw_terms.push(FilterTerm { ty, present });
        });
        if !complete {
            return Self::Legacy;
        }
        raw_terms.sort_unstable_by_key(|term| term.ty.id());

        let mut terms = SmallVec::<[FilterTerm; INLINE_QUERY_COMPONENTS]>::new();
        for FilterTerm { ty, present } in raw_terms {
            if descriptor
                .components
                .iter()
                .any(|component| component.ty.id() == ty.id() && !component.optional)
            {
                if present {
                    continue;
                }
                return Self::Never;
            }

            if let Some(previous) = terms.last() {
                if previous.ty.id() == ty.id() {
                    if previous.present != present {
                        return Self::Never;
                    }
                    continue;
                }
            }
            terms.push(FilterTerm { ty, present });
        }

        if terms.is_empty() {
            Self::Always
        } else {
            Self::Terms(terms)
        }
    }

    #[inline]
    fn matches<Flt: QueryFilter>(&self, archetype: &super::InternalArchetype) -> bool {
        match self {
            Self::Legacy => Flt::matches_archetype(archetype),
            Self::Always => true,
            Self::Never => false,
            Self::Terms(terms) if terms.len() <= 2 => terms
                .iter()
                .all(|term| archetype.query_component_index(&term.ty).is_some() == term.present),
            Self::Terms(terms) => {
                let binary_search_cost = terms.len().saturating_mul(
                    usize::BITS as usize - archetype.components.len().leading_zeros() as usize,
                );
                let merge_cost = archetype.components.len().saturating_add(terms.len());
                if binary_search_cost <= merge_cost {
                    Self::matches_with_suffix_binary_search(terms, archetype)
                } else {
                    Self::matches_with_merge(terms, archetype)
                }
            }
        }
    }

    #[inline]
    fn matches_with_suffix_binary_search(
        terms: &[FilterTerm],
        archetype: &super::InternalArchetype,
    ) -> bool {
        let mut search_start = 0;
        for term in terms {
            let target = term.ty.id();
            match archetype.components[search_start..]
                .binary_search_by_key(&target, |component| component.id())
            {
                Ok(relative_index) => {
                    if !term.present {
                        return false;
                    }
                    search_start += relative_index + 1;
                }
                Err(insertion_index) => {
                    if term.present {
                        return false;
                    }
                    search_start += insertion_index;
                }
            }
        }
        true
    }

    #[inline]
    fn matches_with_merge(terms: &[FilterTerm], archetype: &super::InternalArchetype) -> bool {
        let mut archetype_index = 0;
        for term in terms {
            let target = term.ty.id();
            while archetype_index < archetype.components.len()
                && archetype.components[archetype_index].id() < target
            {
                archetype_index += 1;
            }
            let found = archetype_index < archetype.components.len()
                && archetype.components[archetype_index].id() == target;
            if found != term.present {
                return false;
            }
            if found {
                archetype_index += 1;
            }
        }
        true
    }
}

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
    match_order: SmallVec<[MatchComponent; INLINE_QUERY_COMPONENTS]>,
}

#[derive(Clone, Copy)]
struct MatchComponent {
    component: QueryComponent,
    query_slot: u8,
}

impl QueryDescriptor {
    pub(crate) fn new(components: SmallVec<[QueryComponent; INLINE_QUERY_COMPONENTS]>) -> Self {
        assert!(
            components.len() <= MAX_QUERY_COMPONENTS,
            "typed queries support at most {MAX_QUERY_COMPONENTS} component parameters"
        );
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

        let mut match_order = SmallVec::<[MatchComponent; INLINE_QUERY_COMPONENTS]>::new();
        if components.len() > 2 {
            match_order.extend(components.iter().copied().enumerate().map(
                |(query_slot, component)| MatchComponent {
                    component,
                    query_slot: query_slot as u8,
                },
            ));
            match_order.sort_unstable_by_key(|entry| entry.component.ty.id());
        }

        Self {
            components,
            match_order,
        }
    }

    #[inline]
    fn match_archetype(&self, archetype: &super::InternalArchetype) -> Option<ComponentIndexMap> {
        let query_len = self.match_order.len();
        debug_assert!(query_len > 2);
        let archetype_len = archetype.components.len();
        let mut indices = ComponentIndexMap::new(query_len);

        let binary_search_cost =
            query_len.saturating_mul(usize::BITS as usize - archetype_len.leading_zeros() as usize);
        let merge_cost = archetype_len.saturating_add(query_len);

        let matches = if binary_search_cost <= merge_cost {
            self.match_with_suffix_binary_search(archetype, indices.as_mut_slice())
        } else {
            self.match_with_merge(archetype, indices.as_mut_slice())
        };

        matches.then_some(indices)
    }

    #[inline]
    fn match_with_suffix_binary_search(
        &self,
        archetype: &super::InternalArchetype,
        indices: &mut [u8],
    ) -> bool {
        let mut search_start = 0;
        for entry in &self.match_order {
            let target = entry.component.ty.id();
            match archetype.components[search_start..]
                .binary_search_by_key(&target, |component| component.id())
            {
                Ok(relative_index) => {
                    let index = search_start + relative_index;
                    debug_assert!(index < OPTIONAL_SENTINEL as usize);
                    indices[entry.query_slot as usize] = index as u8;
                    search_start = index + 1;
                }
                Err(insertion_index) => {
                    if !entry.component.optional {
                        return false;
                    }
                    search_start += insertion_index;
                }
            }
        }
        true
    }

    #[inline]
    fn match_with_merge(&self, archetype: &super::InternalArchetype, indices: &mut [u8]) -> bool {
        let mut archetype_index = 0;
        for entry in &self.match_order {
            let target = entry.component.ty.id();
            while archetype_index < archetype.components.len()
                && archetype.components[archetype_index].id() < target
            {
                archetype_index += 1;
            }

            if archetype_index < archetype.components.len()
                && archetype.components[archetype_index].id() == target
            {
                debug_assert!(archetype_index < OPTIONAL_SENTINEL as usize);
                indices[entry.query_slot as usize] = archetype_index as u8;
                archetype_index += 1;
            } else if !entry.component.optional {
                return false;
            }
        }
        true
    }
}

#[derive(Clone)]
pub(crate) struct CachedArchetype {
    pub data_index: usize,
    pub component_indices: ComponentIndexMap,
}

#[derive(Default)]
pub(crate) struct PreparedCache {
    cached_world: Option<Arc<()>>,
    cached_epoch: Option<usize>,
    scanned_data_len: usize,
    pub archetypes: Arc<Vec<CachedArchetype>>,
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
        let same_world = self
            .cached_world
            .as_ref()
            .is_some_and(|cached| Arc::ptr_eq(cached, world.cache_token()));
        let current_epoch = world.archetype_epoch();
        if same_world && self.cached_epoch == Some(current_epoch) {
            return;
        }

        let data_len = world.data.len();
        let filter_plan = (Flt::IS_CONJUNCTIVE && Flt::TERM_COUNT >= 2)
            .then(|| FilterPlan::compile::<Flt>(descriptor));
        // A new archetype increments the epoch and appends exactly one Data.
        // If both deltas agree, only the appended suffix can be new. `clear`
        // increments the epoch without preserving that relation, forcing a
        // full rebuild even if the world is repopulated to the same length.
        let scan_start = match self.cached_epoch {
            Some(cached_epoch)
                if same_world
                    && data_len >= self.scanned_data_len
                    && current_epoch.wrapping_sub(cached_epoch)
                        == data_len - self.scanned_data_len =>
            {
                self.scanned_data_len
            }
            _ => 0,
        };

        let archetypes = Arc::make_mut(&mut self.archetypes);
        if scan_start == 0 {
            archetypes.clear();
        }

        if descriptor.components.len() <= 2 {
            for (data_index, data) in world.data.iter().enumerate().skip(scan_start) {
                let archetype = data.archetype;
                let filter_matches = if Flt::IS_TRIVIAL {
                    true
                } else if Flt::IS_CONJUNCTIVE && Flt::TERM_COUNT >= 2 {
                    filter_plan
                        .as_ref()
                        .expect("conjunctive filter plan must be compiled")
                        .matches::<Flt>(&archetype)
                } else {
                    Flt::matches_archetype(&archetype)
                };
                if !filter_matches {
                    continue;
                }

                let mut component_indices = ComponentIndexMap::new(0);
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
                    archetypes.push(CachedArchetype {
                        data_index,
                        component_indices,
                    });
                }
            }
        } else {
            for (data_index, data) in world.data.iter().enumerate().skip(scan_start) {
                let archetype = data.archetype;
                let filter_matches = if Flt::IS_TRIVIAL {
                    true
                } else if Flt::IS_CONJUNCTIVE && Flt::TERM_COUNT >= 2 {
                    filter_plan
                        .as_ref()
                        .expect("conjunctive filter plan must be compiled")
                        .matches::<Flt>(&archetype)
                } else {
                    Flt::matches_archetype(&archetype)
                };
                if !filter_matches {
                    continue;
                }

                if let Some(component_indices) = descriptor.match_archetype(&archetype) {
                    archetypes.push(CachedArchetype {
                        data_index,
                        component_indices,
                    });
                }
            }
        }

        self.scanned_data_len = data_len;
        if !same_world {
            self.cached_world = Some(Arc::clone(world.cache_token()));
        }
        self.cached_epoch = Some(current_epoch);
    }

    #[inline(always)]
    pub fn visit_chunks<'w, F>(&self, world: &'w World, mut f: F)
    where
        F: FnMut(&CachedArchetype, &'w Chunk),
    {
        for cached in self.archetypes.iter() {
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

#[cfg(test)]
mod layout_tests {
    use super::{ComponentIndexMap, MAX_QUERY_COMPONENTS};

    #[test]
    fn component_index_map_is_inline_fixed_capacity_storage() {
        assert_eq!(
            std::mem::size_of::<ComponentIndexMap>(),
            MAX_QUERY_COMPONENTS + 1
        );
        assert_eq!(std::mem::align_of::<ComponentIndexMap>(), 1);
        assert!(!std::mem::needs_drop::<ComponentIndexMap>());
    }
}
