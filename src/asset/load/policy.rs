use super::AssetLoadQueue;
use crate::asset::store::AssetStore;
use crate::asset::types::{AssetConfig, AssetId};

pub(crate) fn should_load_record_in_background(
    config: &AssetConfig,
    store: &AssetStore,
    id: AssetId,
) -> bool {
    config.background_loading || store.prefers_background_load(id)
}

pub(crate) fn has_current_inflight_load(
    store: &AssetStore,
    load_queue: &AssetLoadQueue,
    id: AssetId,
) -> bool {
    store
        .load_generation(id)
        .is_some_and(|generation| load_queue.contains(id, generation))
}
