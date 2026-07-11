use std::collections::{HashSet, VecDeque};
use std::time::Instant;

use super::events::AssetEventLog;
use super::failure;
use super::registry::AssetRegistry;
use super::request::AssetRequests;
use super::store::{AssetDependencyEvaluation, AssetStore};
use super::types::{AssetError, AssetFailurePhase, AssetId, AssetState};

pub(crate) fn dependency_links(
    store: &AssetStore,
    manifest: &impl AssetRegistry,
    id: AssetId,
) -> Vec<AssetId> {
    if let Some(dependencies) = store.record_dependency_links(id) {
        return dependencies;
    }

    manifest
        .dependencies(id)
        .map(|dependencies| dependencies.to_vec())
        .unwrap_or_default()
}

pub(crate) fn blocking_relevant_ids(
    store: &AssetStore,
    manifest: &impl AssetRegistry,
    target_id: AssetId,
) -> Vec<AssetId> {
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![target_id];

    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        ids.push(id);

        let mut dependencies = dependency_links(store, manifest, id);
        dependencies.reverse();
        stack.extend(dependencies);
    }

    ids
}

pub(crate) fn dependent_reload_closure(store: &AssetStore, roots: &[AssetId]) -> Vec<AssetId> {
    let mut impacted = Vec::new();
    let mut seen = HashSet::new();
    let mut queue: VecDeque<_> = roots.iter().copied().collect();

    while let Some(id) = queue.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        impacted.push(id);

        let mut dependents = store.record_ids_referencing_dependency(id);
        sort_asset_ids(&mut dependents);
        for dependent in dependents {
            queue.push_back(dependent);
        }
    }

    impacted
}

pub(crate) fn resolve_dependencies(
    store: &mut AssetStore,
    manifest: &impl AssetRegistry,
    id: AssetId,
    priority: i32,
) -> Result<AssetState, AssetError> {
    let evaluation = store
        .evaluate_dependency_records(id, priority, |dependency| manifest.asset_type(dependency));

    match evaluation {
        AssetDependencyEvaluation::Ready | AssetDependencyEvaluation::Waiting => {}
        AssetDependencyEvaluation::MissingManifest { dependency } => {
            return Err(AssetError::MissingDependency { id, dependency });
        }
        AssetDependencyEvaluation::Failed { dependency } => {
            return Err(AssetError::DependencyFailed { id, dependency });
        }
    }

    if let Some(cycle) = find_dependency_cycle(store, manifest, id) {
        return Err(AssetError::DependencyCycle { cycle });
    }

    Ok(match evaluation {
        AssetDependencyEvaluation::Waiting => AssetState::WaitingDependencies,
        AssetDependencyEvaluation::Ready => AssetState::Installing,
        AssetDependencyEvaluation::MissingManifest { .. }
        | AssetDependencyEvaluation::Failed { .. } => unreachable!("handled above"),
    })
}

pub(crate) fn resolve_dependencies_or_fail(
    store: &mut AssetStore,
    events: &mut AssetEventLog,
    requests: &mut AssetRequests,
    manifest: &impl AssetRegistry,
    id: AssetId,
    priority: i32,
    failed_at: Instant,
) -> Result<AssetState, AssetError> {
    match resolve_dependencies(store, manifest, id, priority) {
        Ok(next_state) => Ok(next_state),
        Err(error) => {
            failure::fail_record_and_request(
                store,
                events,
                requests,
                id,
                error.clone(),
                AssetFailurePhase::Dependency,
                priority,
                failed_at,
                |dependency| manifest.asset_type(dependency),
            );
            Err(error)
        }
    }
}

fn find_dependency_cycle(
    store: &AssetStore,
    manifest: &impl AssetRegistry,
    id: AssetId,
) -> Option<Vec<AssetId>> {
    let mut stack = Vec::new();
    let mut visited = HashSet::new();
    find_dependency_cycle_from(store, manifest, id, &mut stack, &mut visited)
}

fn find_dependency_cycle_from(
    store: &AssetStore,
    manifest: &impl AssetRegistry,
    id: AssetId,
    stack: &mut Vec<AssetId>,
    visited: &mut HashSet<AssetId>,
) -> Option<Vec<AssetId>> {
    if let Some(index) = stack.iter().position(|existing| *existing == id) {
        let mut cycle = stack[index..].to_vec();
        cycle.push(id);
        return Some(cycle);
    }

    if !visited.insert(id) {
        return None;
    }

    stack.push(id);
    for dependency in dependency_links(store, manifest, id) {
        if let Some(cycle) = find_dependency_cycle_from(store, manifest, dependency, stack, visited)
        {
            return Some(cycle);
        }
    }
    stack.pop();
    None
}

fn sort_asset_ids(ids: &mut [AssetId]) {
    ids.sort_by_key(|left| left.to_string());
}
