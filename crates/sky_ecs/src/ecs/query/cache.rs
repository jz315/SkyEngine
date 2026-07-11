use super::parallel::{self, ParallelJobCache, ParallelJobSnapshot};
use super::{CachedArchetype, PreparedCache, QueryDescriptor, QueryFilter, QuerySpec};
use crate::ecs::World;
use rustc_hash::FxHashMap;
use std::any::TypeId;
use std::cell::RefCell;
use std::sync::Arc;

struct QueryCacheEntry {
    descriptor: QueryDescriptor,
    prepared: PreparedCache,
    parallel_jobs: ParallelJobCache,
}

#[derive(Default)]
pub(crate) struct QueryCacheStore {
    entries: RefCell<FxHashMap<TypeId, QueryCacheEntry>>,
}

impl QueryCacheStore {
    pub(crate) fn snapshot<Q, Flt>(&self, world: &World) -> Arc<Vec<CachedArchetype>>
    where
        Q: QuerySpec + 'static,
        Flt: QueryFilter + 'static,
    {
        let key = TypeId::of::<(Q, Flt)>();
        let mut entries = self.entries.borrow_mut();
        let entry = entries.entry(key).or_insert_with(|| QueryCacheEntry {
            descriptor: Q::descriptor(),
            prepared: PreparedCache::default(),
            parallel_jobs: ParallelJobCache::default(),
        });

        let QueryCacheEntry {
            descriptor,
            prepared,
            ..
        } = entry;
        prepared.prepare::<Flt>(world, descriptor);
        Arc::clone(&prepared.archetypes)
    }

    pub(crate) fn parallel_snapshot<Q, Flt>(
        &self,
        world: &World,
        archetypes: &[CachedArchetype],
    ) -> ParallelJobSnapshot
    where
        Q: QuerySpec + 'static,
        Flt: QueryFilter + 'static,
    {
        let key = TypeId::of::<(Q, Flt)>();
        let mut entries = self.entries.borrow_mut();
        let entry = entries
            .get_mut(&key)
            .expect("query plan must be prepared before parallel iteration");
        parallel::prepare_job_snapshot(&mut entry.parallel_jobs, archetypes, world)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    #[cfg(test)]
    pub(crate) fn parallel_rebuild_count<Q, Flt>(&self) -> usize
    where
        Q: QuerySpec + 'static,
        Flt: QueryFilter + 'static,
    {
        let key = TypeId::of::<(Q, Flt)>();
        self.entries
            .borrow()
            .get(&key)
            .map_or(0, |entry| parallel::rebuild_count(&entry.parallel_jobs))
    }
}
