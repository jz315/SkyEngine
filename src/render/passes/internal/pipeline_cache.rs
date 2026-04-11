use std::hash::Hash;
use std::sync::Arc;

use rustc_hash::FxHashMap;

pub(crate) struct RenderPipelineCache<K> {
    pipelines: FxHashMap<K, Arc<wgpu::RenderPipeline>>,
}

impl<K> RenderPipelineCache<K>
where
    K: Eq + Hash + Copy,
{
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            pipelines: FxHashMap::default(),
        }
    }

    #[inline]
    pub(crate) fn get_or_create(
        &mut self,
        key: K,
        create: impl FnOnce() -> wgpu::RenderPipeline,
    ) -> Arc<wgpu::RenderPipeline> {
        self.pipelines
            .entry(key)
            .or_insert_with(|| Arc::new(create()))
            .clone()
    }

    #[cfg(test)]
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.pipelines.len()
    }
}

impl<K> Default for RenderPipelineCache<K>
where
    K: Eq + Hash + Copy,
{
    fn default() -> Self {
        Self::new()
    }
}
