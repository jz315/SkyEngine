use std::hash::Hash;

use rustc_hash::FxHashMap;

pub(crate) struct BindGroupCache<K> {
    groups: FxHashMap<K, wgpu::BindGroup>,
}

impl<K> BindGroupCache<K>
where
    K: Eq + Hash + Copy,
{
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            groups: FxHashMap::default(),
        }
    }

    #[inline]
    pub(crate) fn get_or_create(
        &mut self,
        key: K,
        create: impl FnOnce() -> wgpu::BindGroup,
    ) -> &wgpu::BindGroup {
        self.groups.entry(key).or_insert_with(create)
    }
}

impl<K> Default for BindGroupCache<K>
where
    K: Eq + Hash + Copy,
{
    fn default() -> Self {
        Self::new()
    }
}
