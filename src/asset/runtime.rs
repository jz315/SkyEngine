use super::events::{self, AssetEventLog};
use super::registry::AssetRegistry;
use super::store::AssetStore;
use super::types::{Asset, AssetError, AssetId};

pub(crate) fn insert_runtime_asset<T: Asset>(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    id: AssetId,
    asset: T,
) {
    store.insert_runtime_asset(id, asset);
    events::push_installed_event(events, store, id);
}

pub(crate) fn replace_runtime_asset<T>(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    registry: &impl AssetRegistry,
    id: AssetId,
    asset: T,
    dependency_priority: i32,
) -> Result<(), AssetError>
where
    T: Asset,
{
    let update = store.replace_runtime_asset(id, asset)?;
    let release = store.apply_dependency_lease_update(update, dependency_priority, |dependency| {
        registry.asset_type(dependency)
    });
    events::push_release_events(events, store, release);
    events::push_installed_event(events, store, id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::registry::ManifestIndex;
    use crate::asset::store::AssetRecord;
    use crate::asset::types::{AssetEventKind, AssetRegistryManifest, AssetState};

    struct RuntimeTestAsset;

    impl Asset for RuntimeTestAsset {
        const TYPE: &'static str = "runtime.test";
    }

    #[test]
    fn runtime_insert_emits_installed_event() {
        let id = AssetId::new();
        let mut store = AssetStore::default();
        let mut events = AssetEventLog::default();
        let mut cursor = events.cursor();

        insert_runtime_asset(&mut store, &mut events, id, RuntimeTestAsset);

        assert_eq!(store.records[&id].state, AssetState::Installed);
        let emitted = events.events_since(&mut cursor);
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].id, id);
        assert_eq!(emitted[0].kind, AssetEventKind::Installed);
    }

    #[test]
    fn runtime_replace_emits_release_and_installed_events() {
        let parent = AssetId::new();
        let dependency = AssetId::new();
        let mut store = AssetStore::default();
        store.insert_runtime_asset(parent, RuntimeTestAsset);
        store
            .records
            .insert(dependency, AssetRecord::new(dependency, "dep".to_string()));
        let update = store.replace_held_dependencies(parent, vec![dependency]);
        let _ = store.apply_dependency_lease_update(update, 3, |_| Some("dep".to_string()));
        let mut events = AssetEventLog::default();
        let mut cursor = events.cursor();

        let registry = ManifestIndex::new(AssetRegistryManifest::default());
        replace_runtime_asset(
            &mut store,
            &mut events,
            &registry,
            parent,
            RuntimeTestAsset,
            3,
        )
        .expect("runtime replace should succeed");

        assert!(store.records[&parent].held_dependencies.ids().is_empty());
        assert_eq!(store.records[&dependency].dependency_ref_count, 0);

        let emitted = events.events_since(&mut cursor);
        assert_eq!(emitted.len(), 2);
        assert_eq!(emitted[0].id, dependency);
        assert_eq!(emitted[0].kind, AssetEventKind::Unloaded);
        assert_eq!(emitted[1].id, parent);
        assert_eq!(emitted[1].kind, AssetEventKind::Installed);
    }
}
