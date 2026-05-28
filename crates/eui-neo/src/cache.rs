//! Small cache primitives for retained UI boundaries.
//!
//! The core rule is that a cache hit must be decided by a typed key supplied by
//! the caller. Cache cells own hit/miss accounting, but callers own the key
//! contract because only the boundary knows which revisions affect the value.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheAccess {
    Hit,
    Miss { had_previous_key: bool },
}

impl CacheAccess {
    pub fn is_hit(self) -> bool {
        matches!(self, Self::Hit)
    }

    pub fn is_miss(self) -> bool {
        !self.is_hit()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub invalidations: u64,
}

#[derive(Debug, Clone)]
pub struct CacheCell<K, V> {
    key: Option<K>,
    value: V,
    stats: CacheStats,
}

impl<K, V: Default> Default for CacheCell<K, V> {
    fn default() -> Self {
        Self::new(V::default())
    }
}

impl<K, V> CacheCell<K, V> {
    pub fn new(value: V) -> Self {
        Self {
            key: None,
            value,
            stats: CacheStats::default(),
        }
    }

    pub fn key(&self) -> Option<&K> {
        self.key.as_ref()
    }

    pub fn value(&self) -> &V {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut V {
        &mut self.value
    }

    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    pub fn invalidate(&mut self) {
        if self.key.take().is_some() {
            self.stats.invalidations = self.stats.invalidations.saturating_add(1);
        }
    }
}

impl<K: Eq, V> CacheCell<K, V> {
    pub fn get_or_rebuild(&mut self, key: K, rebuild: impl FnOnce(&mut V)) -> CacheAccess {
        if self.key.as_ref() == Some(&key) {
            self.stats.hits = self.stats.hits.saturating_add(1);
            return CacheAccess::Hit;
        }

        let had_previous_key = self.key.is_some();
        rebuild(&mut self.value);
        self.key = Some(key);
        self.stats.misses = self.stats.misses.saturating_add(1);
        CacheAccess::Miss { had_previous_key }
    }
}

#[cfg(test)]
mod tests {
    use super::{CacheAccess, CacheCell, CacheStats};

    #[test]
    fn cache_cell_rebuilds_only_when_key_changes() {
        let mut cell = CacheCell::new(String::new());

        let first = cell.get_or_rebuild(1, |value| value.push_str("one"));
        let second = cell.get_or_rebuild(1, |value| value.push_str("wrong"));
        let third = cell.get_or_rebuild(2, |value| {
            value.clear();
            value.push_str("two");
        });

        assert_eq!(
            first,
            CacheAccess::Miss {
                had_previous_key: false
            }
        );
        assert_eq!(second, CacheAccess::Hit);
        assert_eq!(
            third,
            CacheAccess::Miss {
                had_previous_key: true
            }
        );
        assert_eq!(cell.value(), "two");
        assert_eq!(
            cell.stats(),
            CacheStats {
                hits: 1,
                misses: 2,
                invalidations: 0,
            }
        );
    }

    #[test]
    fn invalidate_forces_the_next_lookup_to_rebuild() {
        let mut cell = CacheCell::new(0);

        assert!(cell.get_or_rebuild("a", |value| *value = 1).is_miss());
        cell.invalidate();
        assert_eq!(cell.key(), None);
        assert!(cell.get_or_rebuild("a", |value| *value = 2).is_miss());

        assert_eq!(*cell.value(), 2);
        assert_eq!(cell.stats().invalidations, 1);
    }
}
