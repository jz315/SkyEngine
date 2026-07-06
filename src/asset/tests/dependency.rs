use crate::asset::dependency::*;
use crate::asset::events::AssetEventLog;
use crate::asset::lease::AssetDependencyLeases;
use crate::asset::registry::ManifestIndex;
use crate::asset::request::{AssetRequestPhase, AssetRequests};
use crate::asset::store::{AssetRecord, AssetStore};
use crate::asset::types::{
    AssetEventKind, AssetFailurePhase, AssetManifestEntry, AssetRegistryManifest,
    AssetRequestStatus, AssetState,
};
use crate::asset::{AssetError, AssetId};
use std::time::Instant;

fn entry(id: AssetId, dependencies: Vec<AssetId>) -> AssetManifestEntry {
    AssetManifestEntry {
        asset_id: id,
        asset_type: "dummy".to_string(),
        importer: "dummy.importer".to_string(),
        cooker: "dummy.cooker".to_string(),
        version: 1,
        source_path: format!("{id}.dummy"),
        cooked_path: format!("{id}.dummyc"),
        dependencies,
        import_settings: serde_json::Value::Null,
    }
}

fn manifest(entries: Vec<AssetManifestEntry>) -> ManifestIndex {
    ManifestIndex::new(AssetRegistryManifest {
        version: 1,
        target: "test".to_string(),
        provenance: Vec::new(),
        assets: entries,
    })
}

#[test]
fn blocking_relevant_ids_follow_manifest_dependencies_when_record_has_none() {
    let parent = AssetId::new();
    let dependency = AssetId::new();
    let store = AssetStore::default();
    let manifest = manifest(vec![
        entry(parent, vec![dependency]),
        entry(dependency, vec![]),
    ]);

    assert_eq!(
        blocking_relevant_ids(&store, &manifest, parent),
        vec![parent, dependency]
    );
}

#[test]
fn dependent_reload_closure_uses_installed_and_held_dependencies() {
    let root = AssetId::new();
    let installed_parent = AssetId::new();
    let loading_parent = AssetId::new();
    let mut store = AssetStore::default();

    let mut installed = AssetRecord::new(installed_parent, "dummy".to_string());
    installed.dependencies = vec![root];
    store.records.insert(installed_parent, installed);

    let mut loading = AssetRecord::new(loading_parent, "dummy".to_string());
    loading.held_dependencies = AssetDependencyLeases::new(vec![root]);
    store.records.insert(loading_parent, loading);

    let closure = dependent_reload_closure(&store, &[root]);

    assert_eq!(closure.len(), 3);
    assert_eq!(closure[0], root);
    assert!(closure.contains(&installed_parent));
    assert!(closure.contains(&loading_parent));
}

#[test]
fn resolve_dependencies_reports_missing_failed_waiting_ready_and_cycles() {
    let target = AssetId::new();
    let missing = AssetId::new();
    let mut store = AssetStore::default();
    let mut target_record = AssetRecord::new(target, "dummy".to_string());
    target_record.dependencies = vec![missing];
    store.records.insert(target, target_record);
    let empty_manifest = manifest(Vec::new());

    assert!(matches!(
        resolve_dependencies(&mut store, &empty_manifest, target, 0),
        Err(AssetError::MissingDependency { dependency, .. }) if dependency == missing
    ));

    let failed = AssetId::new();
    let mut store = AssetStore::default();
    let mut target_record = AssetRecord::new(target, "dummy".to_string());
    target_record.dependencies = vec![failed];
    store.records.insert(target, target_record);
    let mut failed_record = AssetRecord::new(failed, "dummy".to_string());
    failed_record.state = AssetState::Failed;
    store.records.insert(failed, failed_record);
    let failed_manifest = manifest(vec![entry(failed, Vec::new())]);

    assert!(matches!(
        resolve_dependencies(&mut store, &failed_manifest, target, 0),
        Err(AssetError::DependencyFailed { dependency, .. }) if dependency == failed
    ));

    let waiting = AssetId::new();
    let mut store = AssetStore::default();
    let mut target_record = AssetRecord::new(target, "dummy".to_string());
    target_record.dependencies = vec![waiting];
    store.records.insert(target, target_record);
    let waiting_manifest = manifest(vec![entry(waiting, Vec::new())]);

    assert_eq!(
        resolve_dependencies(&mut store, &waiting_manifest, target, 5),
        Ok(AssetState::WaitingDependencies)
    );
    assert_eq!(store.records[&waiting].state, AssetState::Loading);
    assert_eq!(store.records[&waiting].load_priority, 5);

    store
        .records
        .get_mut(&waiting)
        .expect("dependency should exist")
        .state = AssetState::Installed;
    assert_eq!(
        resolve_dependencies(&mut store, &waiting_manifest, target, 5),
        Ok(AssetState::Installing)
    );

    let other = AssetId::new();
    let mut store = AssetStore::default();
    store
        .records
        .insert(target, AssetRecord::new(target, "dummy".to_string()));
    let cycle_manifest = manifest(vec![entry(target, vec![other]), entry(other, vec![target])]);

    assert!(matches!(
        resolve_dependencies(&mut store, &cycle_manifest, target, 0),
        Err(AssetError::DependencyCycle { cycle }) if cycle == vec![target, other, target]
    ));
}

#[test]
fn dependency_failure_helper_marks_record_event_and_request() {
    let target = AssetId::new();
    let missing = AssetId::new();
    let now = Instant::now();
    let mut store = AssetStore::default();
    let mut record = AssetRecord::new(target, "dummy".to_string());
    record.state = AssetState::WaitingDependencies;
    record.strong_ref_count = 1;
    record.load_generation = 5;
    record.dependencies = vec![missing];
    store.records.insert(target, record);
    let manifest = manifest(Vec::new());
    let mut events = AssetEventLog::new(8);
    let mut cursor = events.cursor();
    let mut requests = AssetRequests::default();
    requests.enqueue(target, 5, None, 13, now);
    let request = requests.pop_queued().expect("queued request");
    requests.activate(request, AssetRequestPhase::WaitingDependencies, 5, now);

    let error = resolve_dependencies_or_fail(
        &mut store,
        &mut events,
        &mut requests,
        &manifest,
        target,
        13,
        now,
    )
    .expect_err("missing dependency should fail");

    assert!(matches!(
        error,
        AssetError::MissingDependency { id, dependency }
            if id == target && dependency == missing
    ));
    assert_eq!(store.records[&target].state, AssetState::Failed);
    assert_eq!(
        store.records[&target].failure_phase,
        Some(AssetFailurePhase::Dependency)
    );
    let emitted = events.events_since(&mut cursor);
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].id, target);
    assert_eq!(emitted[0].kind, AssetEventKind::Failed);
    assert_eq!(
        emitted[0].failure_phase,
        Some(AssetFailurePhase::Dependency)
    );
    let failed = requests.failed_snapshots();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].asset_id, target);
    assert_eq!(failed[0].generation, 5);
    assert_eq!(failed[0].status, AssetRequestStatus::Failed);
}
