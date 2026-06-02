use rustc_hash::{FxHashMap, FxHashSet};

use super::{LayerIntent, NodeId, ScopeLayerIntents, ScopeLayerRoots};
use crate::callbacks::UiCallbacks;
use crate::clock::{preserve_reused_scope_clock_dependency, ClockPeriodMap};
use crate::retained::{
    dirty_root_ids_for_scopes, previous_elements_for_scope, retained_scope_contains_dirty_root,
    structural_incompatibility_reports, structurally_incompatible_dirty_scopes,
    CallbackTransferStats, FullLayoutReason, RetainedComposeAction, RetainedComposeReason,
    RetainedRoot, ScopeComposeRecord, ScopeId, ScopeRoots, ScopeSet,
};
use crate::signal::{clear_scope_signal_dependencies, schedule_scope_dependency_reset};
use crate::Element;

pub(crate) type ElementIdSet = FxHashSet<NodeId>;

pub(crate) struct ScopeFrame {
    pub can_reuse_scopes: bool,
    pub input_dirty_scopes: ScopeSet,
    pub live_dirty_scopes: ScopeSet,
    pub dirty_scopes: ScopeSet,
    pub previous_scope_roots: ScopeRoots,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetainedReuseDecision {
    Reuse,
    Rebuild(RetainedComposeReason),
}

impl RetainedReuseDecision {
    #[cfg(test)]
    pub(crate) fn rebuild_reason(self) -> Option<RetainedComposeReason> {
        match self {
            Self::Reuse => None,
            Self::Rebuild(reason) => Some(reason),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum RetainedReusePlan {
    Reuse { elements: Vec<Element> },
    Rebuild(RetainedComposeReason),
}

#[derive(Debug, Clone)]
pub(crate) enum RetainedScopeComposePlan {
    Reuse(RetainedReusePlan),
    Build(RetainedScopeBuildPlan),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RetainedScopeBuildPlan {
    pub reason: RetainedComposeReason,
    pub dirty_owner: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct RetainedScopeApplied {
    pub retained_roots: Vec<RetainedRoot>,
    pub record: ScopeComposeRecord,
}

impl RetainedScopeApplied {
    pub(crate) fn action(&self) -> RetainedComposeAction {
        self.record.action
    }

    pub(crate) fn into_parts(self) -> (ScopeId, Vec<RetainedRoot>, ScopeComposeRecord) {
        let id = self.record.id.clone();
        (id, self.retained_roots, self.record)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RetainedReuseApplied {
    pub elements: Vec<Element>,
    pub scope: RetainedScopeApplied,
    pub preserved_scope_roots: ScopeRoots,
}

pub(crate) type RetainedBuildApplied = RetainedScopeApplied;

#[derive(Debug, Clone, Default)]
pub(crate) struct RetainedLayerReplay {
    pub intents: Vec<LayerIntent>,
    pub roots: Vec<Element>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RetainedDependencyResetApplied {
    pub scheduled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedUnmountApplied {
    pub removed_scopes: Vec<ScopeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetainedLayoutReusePlan {
    pub blocker: Option<FullLayoutReason>,
    pub structural_reports: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RetainedReuseState {
    enabled: bool,
    dirty_root_ids: ElementIdSet,
    dirty_scopes: ScopeSet,
}

impl RetainedReuseState {
    pub(crate) fn enabled(dirty_scopes: ScopeSet, previous_scope_roots: &ScopeRoots) -> Self {
        let dirty_root_ids = dirty_root_ids_for_scopes(&dirty_scopes, previous_scope_roots);
        Self {
            enabled: true,
            dirty_root_ids,
            dirty_scopes,
        }
    }

    pub(crate) fn context<'a>(
        &'a self,
        previous_roots: &'a [Element],
        previous_scope_roots: &'a ScopeRoots,
    ) -> RetainedReuseContext<'a> {
        RetainedReuseContext::new(
            self.enabled,
            &self.dirty_root_ids,
            previous_roots,
            previous_scope_roots,
            &self.dirty_scopes,
        )
    }

    #[cfg(test)]
    pub(crate) fn is_dirty(&self, id: &ScopeId) -> bool {
        self.dirty_scopes.contains(id)
    }
}

pub(crate) fn begin_scope_frame(
    can_reuse_scopes: bool,
    dirty_scopes: Option<ScopeSet>,
    scope_roots: &mut ScopeRoots,
    live_scopes: &mut ScopeSet,
) -> ScopeFrame {
    let mut dirty_scopes = dirty_scopes.unwrap_or_default();
    let input_dirty_scopes = dirty_scopes.clone();
    let (live_dirty_scopes, previous_scope_roots) = if can_reuse_scopes {
        (
            merge_live_scopes(&mut dirty_scopes, live_scopes),
            std::mem::take(scope_roots),
        )
    } else {
        live_scopes.clear();
        (ScopeSet::default(), ScopeRoots::default())
    };

    ScopeFrame {
        can_reuse_scopes,
        input_dirty_scopes,
        live_dirty_scopes,
        dirty_scopes,
        previous_scope_roots,
    }
}

/// Live scopes are intentionally dirty on every scoped compose. They cover
/// frame-time/procedural animation without pretending that time is app state.
fn merge_live_scopes(dirty_scopes: &mut ScopeSet, live_scopes: &mut ScopeSet) -> ScopeSet {
    let live_dirty_scopes = std::mem::take(live_scopes);
    dirty_scopes.extend(live_dirty_scopes.iter().cloned());
    live_dirty_scopes
}

pub(crate) struct RetainedReuseContext<'a> {
    scope_reuse_enabled: bool,
    dirty_root_ids: &'a ElementIdSet,
    previous_roots: &'a [Element],
    previous_scope_roots: &'a ScopeRoots,
    dirty_scopes: &'a ScopeSet,
}

impl<'a> RetainedReuseContext<'a> {
    pub(crate) fn new(
        scope_reuse_enabled: bool,
        dirty_root_ids: &'a ElementIdSet,
        previous_roots: &'a [Element],
        previous_scope_roots: &'a ScopeRoots,
        dirty_scopes: &'a ScopeSet,
    ) -> Self {
        Self {
            scope_reuse_enabled,
            dirty_root_ids,
            previous_roots,
            previous_scope_roots,
            dirty_scopes,
        }
    }

    pub(crate) fn decision(&self, id: &ScopeId) -> RetainedReuseDecision {
        if !self.scope_reuse_enabled {
            return RetainedReuseDecision::Rebuild(RetainedComposeReason::RetainedReuseUnavailable);
        }
        if !self.previous_scope_roots.contains_key(id) {
            return RetainedReuseDecision::Rebuild(RetainedComposeReason::MissingPreviousRoots);
        }
        if self.dirty_scopes.contains(id) {
            return RetainedReuseDecision::Rebuild(RetainedComposeReason::DirtyScope);
        }
        if retained_scope_contains_dirty_root(self.dirty_root_ids, self.previous_scope_roots, id) {
            return RetainedReuseDecision::Rebuild(RetainedComposeReason::DirtyDescendant);
        }
        RetainedReuseDecision::Reuse
    }

    pub(crate) fn decision_with_dirty_ancestor(
        &self,
        id: &ScopeId,
        has_dirty_ancestor: bool,
    ) -> RetainedReuseDecision {
        let decision = self.decision(id);
        if has_dirty_ancestor && decision == RetainedReuseDecision::Reuse {
            RetainedReuseDecision::Rebuild(RetainedComposeReason::DirtyAncestor)
        } else {
            decision
        }
    }

    #[cfg(test)]
    pub(crate) fn build_reason(
        &self,
        id: &ScopeId,
        has_dirty_ancestor: bool,
    ) -> Option<RetainedComposeReason> {
        self.decision_with_dirty_ancestor(id, has_dirty_ancestor)
            .rebuild_reason()
    }

    #[cfg(test)]
    pub(crate) fn build_reason_or_missing_element(
        &self,
        id: &ScopeId,
        has_dirty_ancestor: bool,
    ) -> RetainedComposeReason {
        self.build_reason(id, has_dirty_ancestor)
            .unwrap_or(RetainedComposeReason::MissingPreviousElement)
    }

    pub(crate) fn is_dirty_scope(&self, id: &ScopeId) -> bool {
        self.dirty_scopes.contains(id)
    }

    pub(crate) fn should_reset_rebuilt_scope_dependencies(&self, id: &ScopeId) -> bool {
        self.scope_reuse_enabled
            && self.previous_scope_roots.contains_key(id)
            && retained_scope_contains_dirty_root(
                self.dirty_root_ids,
                self.previous_scope_roots,
                id,
            )
    }

    pub(crate) fn plan(&self, id: &ScopeId, has_dirty_ancestor: bool) -> RetainedReusePlan {
        if let RetainedReuseDecision::Rebuild(reason) =
            self.decision_with_dirty_ancestor(id, has_dirty_ancestor)
        {
            return RetainedReusePlan::Rebuild(reason);
        }
        match previous_elements_for_scope(self.previous_roots, self.previous_scope_roots, id) {
            Some(elements) => RetainedReusePlan::Reuse { elements },
            None => RetainedReusePlan::Rebuild(RetainedComposeReason::MissingPreviousElement),
        }
    }
}

pub(crate) fn retained_scope_compose_plan(
    context: &RetainedReuseContext<'_>,
    id: &ScopeId,
    has_dirty_ancestor: bool,
) -> RetainedScopeComposePlan {
    let plan = context.plan(id, has_dirty_ancestor);
    match plan {
        RetainedReusePlan::Reuse { .. } => RetainedScopeComposePlan::Reuse(plan),
        RetainedReusePlan::Rebuild(reason) => {
            RetainedScopeComposePlan::Build(RetainedScopeBuildPlan {
                reason,
                dirty_owner: context.is_dirty_scope(id),
            })
        }
    }
}

pub(crate) fn retained_layer_replay_for_scope(
    id: &ScopeId,
    previous_roots: &[Element],
    previous_scope_layer_roots: &ScopeLayerRoots,
    previous_scope_layer_intents: &ScopeLayerIntents,
) -> RetainedLayerReplay {
    RetainedLayerReplay {
        intents: previous_scope_layer_intents
            .get(id)
            .cloned()
            .unwrap_or_default(),
        roots: previous_scope_layer_roots
            .get(id)
            .map(|roots| {
                roots
                    .iter()
                    .filter_map(|root| find_element(previous_roots, root.as_str()).cloned())
                    .collect()
            })
            .unwrap_or_default(),
    }
}

pub(crate) fn apply_retained_reuse_plan(
    id: &ScopeId,
    plan: RetainedReusePlan,
    callbacks: &mut UiCallbacks,
    previous_callbacks: &mut UiCallbacks,
    active_layers: &[LayerIntent],
    additional_callback_elements: &[Element],
    previous_scope_roots: &ScopeRoots,
    current_scope_roots: &ScopeRoots,
    previous_clock_periods: Option<&ClockPeriodMap>,
    clock_ids: &mut ScopeSet,
    clock_periods: &mut Option<ClockPeriodMap>,
) -> Result<RetainedReuseApplied, RetainedComposeReason> {
    let elements = match plan {
        RetainedReusePlan::Reuse { elements } => elements,
        RetainedReusePlan::Rebuild(reason) => return Err(reason),
    };
    let root_count = elements.len();
    let element_count = count_elements(&elements);
    let retained_roots = RetainedRoot::from_elements(&elements);
    let callback_transfers = if additional_callback_elements.is_empty() {
        callbacks.transfer_for_elements(previous_callbacks, &elements, active_layers)
    } else {
        let mut callback_elements =
            Vec::with_capacity(elements.len() + additional_callback_elements.len());
        callback_elements.extend_from_slice(&elements);
        callback_elements.extend_from_slice(additional_callback_elements);
        callbacks.transfer_for_elements(previous_callbacks, &callback_elements, active_layers)
    };
    let preserved_scope_roots = preserved_scope_roots_for_reused_elements(
        previous_scope_roots,
        current_scope_roots,
        &elements,
        additional_callback_elements,
    );
    preserve_reused_scope_clock_dependency(id, previous_clock_periods, clock_ids, clock_periods);
    let record = ScopeComposeRecord {
        id: id.clone(),
        action: RetainedComposeAction::Reused,
        reason: RetainedComposeReason::CleanReuse,
        build_ms: 0.0,
        self_build_ms: 0.0,
        previous_roots: root_count,
        current_roots: root_count,
        element_count,
        callback_transfers,
    };
    let scope = RetainedScopeApplied {
        retained_roots,
        record,
    };
    Ok(RetainedReuseApplied {
        elements,
        scope,
        preserved_scope_roots,
    })
}

pub(crate) fn apply_retained_build(
    id: ScopeId,
    roots: &[Element],
    reason: RetainedComposeReason,
    build_ms: f32,
    self_build_ms: f32,
    previous_scope_roots: &ScopeRoots,
) -> RetainedBuildApplied {
    let previous_roots = previous_scope_roots.get(&id).map_or(0, |roots| roots.len());
    let retained_roots = RetainedRoot::from_elements(roots);
    let record = ScopeComposeRecord {
        id,
        action: RetainedComposeAction::Built,
        reason,
        build_ms,
        self_build_ms,
        previous_roots,
        current_roots: roots.len(),
        element_count: count_elements(roots),
        callback_transfers: CallbackTransferStats::default(),
    };
    RetainedScopeApplied {
        retained_roots,
        record,
    }
}

pub(crate) fn apply_rebuilt_scope_dependency_reset(
    context: &RetainedReuseContext<'_>,
    scope: &ScopeId,
) -> RetainedDependencyResetApplied {
    let scheduled = context.should_reset_rebuilt_scope_dependencies(scope);
    if scheduled {
        schedule_scope_dependency_reset(scope);
    }
    RetainedDependencyResetApplied { scheduled }
}

pub(crate) fn apply_removed_retained_scopes(
    previous_scope_roots: &ScopeRoots,
    next_scope_roots: &ScopeRoots,
) -> RetainedUnmountApplied {
    let removed_scopes = removed_retained_scope_ids(previous_scope_roots, next_scope_roots);
    for scope in &removed_scopes {
        clear_scope_signal_dependencies(scope);
    }
    RetainedUnmountApplied { removed_scopes }
}

fn removed_retained_scope_ids(
    previous_scope_roots: &ScopeRoots,
    next_scope_roots: &ScopeRoots,
) -> Vec<ScopeId> {
    let mut removed = previous_scope_roots
        .keys()
        .filter(|scope| !next_scope_roots.contains_key(*scope))
        .cloned()
        .collect::<Vec<_>>();
    removed.sort();
    removed
}

pub(crate) fn retained_layout_reuse_plan(
    can_reuse_scopes: bool,
    normalized_dirty_ids: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
    next_scope_roots: &ScopeRoots,
    collect_structural_reports: bool,
) -> RetainedLayoutReusePlan {
    if !can_reuse_scopes {
        return RetainedLayoutReusePlan {
            blocker: Some(FullLayoutReason::RetainedReuseUnavailable),
            structural_reports: Vec::new(),
        };
    }
    if let Some(scope) = normalized_dirty_ids
        .iter()
        .find(|scope| !previous_scope_roots.contains_key(*scope))
    {
        return RetainedLayoutReusePlan {
            blocker: Some(FullLayoutReason::MissingPreviousRetainedRoot { id: scope.clone() }),
            structural_reports: Vec::new(),
        };
    }
    let structurally_incompatible = structurally_incompatible_dirty_scopes(
        normalized_dirty_ids,
        previous_scope_roots,
        next_scope_roots,
    );
    let structural_reports = if collect_structural_reports && !structurally_incompatible.is_empty()
    {
        structural_incompatibility_reports(
            normalized_dirty_ids,
            previous_scope_roots,
            next_scope_roots,
        )
    } else {
        Vec::new()
    };
    if !structurally_incompatible.is_empty() {
        return RetainedLayoutReusePlan {
            blocker: Some(FullLayoutReason::StructureChanged {
                ids: structurally_incompatible,
            }),
            structural_reports,
        };
    }
    if let Some(scope) = next_scope_roots
        .keys()
        .find(|scope| !previous_scope_roots.contains_key(*scope))
    {
        return RetainedLayoutReusePlan {
            blocker: Some(FullLayoutReason::MissingPreviousRetainedRoot { id: scope.clone() }),
            structural_reports: Vec::new(),
        };
    }
    RetainedLayoutReusePlan {
        blocker: None,
        structural_reports,
    }
}

pub(crate) fn retained_layout_dirty_scopes(
    compose_dirty_scopes: &ScopeSet,
    layout_dirty_scopes: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
) -> ScopeSet {
    let mut dirty_scopes = compose_dirty_scopes.clone();
    dirty_scopes.extend(layout_dirty_scopes.iter().cloned());
    crate::retained::normalize_dirty_scopes_with_roots(&dirty_scopes, previous_scope_roots)
}

pub(crate) fn refresh_scope_roots_from_tree(scope_roots: &mut ScopeRoots, roots: &[Element]) {
    let mut elements_by_id = FxHashMap::default();
    collect_elements_by_id(roots, &mut elements_by_id);
    for elements in scope_roots.values_mut() {
        for element in elements {
            if let Some(updated) = elements_by_id.get(&element.id) {
                refresh_retained_root_layout_frames(element, updated);
            }
        }
    }
}

pub(crate) fn committed_element_ids(elements: &[Element]) -> ElementIdSet {
    let mut ids = ElementIdSet::default();
    collect_element_ids(elements, &mut ids);
    ids
}

fn collect_elements_by_id<'a>(elements: &'a [Element], index: &mut FxHashMap<NodeId, &'a Element>) {
    for element in elements {
        index.entry(NodeId::new(&element.id)).or_insert(element);
        collect_elements_by_id(&element.children, index);
    }
}

fn collect_element_ids(elements: &[Element], ids: &mut ElementIdSet) {
    for element in elements {
        ids.insert(NodeId::new(&element.id));
        collect_element_ids(&element.children, ids);
    }
}

fn preserved_scope_roots_for_reused_elements(
    previous_scope_roots: &ScopeRoots,
    current_scope_roots: &ScopeRoots,
    elements: &[Element],
    additional_elements: &[Element],
) -> ScopeRoots {
    let mut ids = ElementIdSet::default();
    collect_element_ids(elements, &mut ids);
    collect_element_ids(additional_elements, &mut ids);
    previous_scope_roots
        .iter()
        .filter(|(scope, roots)| {
            !current_scope_roots.contains_key(*scope)
                && roots.iter().all(|root| ids.contains(&root.id))
        })
        .map(|(scope, roots)| (scope.clone(), roots.clone()))
        .collect()
}

fn find_element<'a>(elements: &'a [Element], id: &str) -> Option<&'a Element> {
    for element in elements {
        if element.id == id {
            return Some(element);
        }
        if let Some(found) = find_element(&element.children, id) {
            return Some(found);
        }
    }
    None
}

fn refresh_retained_root_layout_frames(element: &mut RetainedRoot, updated: &Element) {
    // Layout mutates only Element::frame. Retained scope roots keep the same
    // visual/callback data unless their structure changed, so refresh frames
    // in place and fall back to replacement only when the tree no longer matches.
    if element.kind != updated.kind
        || element.id() != updated.id
        || element.children.len() != updated.children.len()
    {
        *element = RetainedRoot::from_element(updated);
        return;
    }
    element.frame = updated.frame;
    for (child, updated_child) in element.children.iter_mut().zip(&updated.children) {
        refresh_retained_root_layout_frames(child, updated_child);
    }
}

fn count_elements(elements: &[Element]) -> usize {
    elements
        .iter()
        .map(|element| 1 + count_elements(&element.children))
        .sum()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use crate::callbacks::{ClickCallbackId, DragCallbackId, UiCallbacks};
    use crate::clock::ClockPeriodMap;
    use crate::retained::{
        dirty_root_ids_for_scopes, FullLayoutReason, RetainedComposeAction, RetainedRoot, ScopeId,
        ScopeRoots, ScopeSet,
    };
    use crate::runtime::{
        LayerId, LayerIntent, LayerKind, LayerPlacement, LayerSize, NodeId, OutsideClickPolicy,
        ScopeLayerIntents, ScopeLayerRoots,
    };
    use crate::signal::State;
    use crate::{DragEvent, Element, ElementKind, Ui};

    use super::{
        apply_rebuilt_scope_dependency_reset, apply_removed_retained_scopes, apply_retained_build,
        apply_retained_reuse_plan, begin_scope_frame, retained_layer_replay_for_scope,
        retained_layout_dirty_scopes, retained_layout_reuse_plan, retained_scope_compose_plan,
        RetainedComposeReason, RetainedReuseContext, RetainedReuseDecision, RetainedReusePlan,
        RetainedReuseState, RetainedScopeComposePlan,
    };

    fn scopes(ids: &[&str]) -> ScopeSet {
        ids.iter().map(|id| ScopeId::new(*id)).collect()
    }

    fn scope(id: &str) -> ScopeId {
        ScopeId::new(id)
    }

    fn apply_reuse_without_clock(
        plan: RetainedReusePlan,
        callbacks: &mut UiCallbacks,
        previous_callbacks: &mut UiCallbacks,
    ) -> Result<super::RetainedReuseApplied, RetainedComposeReason> {
        let id = ScopeId::new("page.scope");
        let mut clock_ids = ScopeSet::default();
        let mut clock_periods = None;
        let previous_scope_roots = ScopeRoots::default();
        let current_scope_roots = ScopeRoots::default();
        apply_retained_reuse_plan(
            &id,
            plan,
            callbacks,
            previous_callbacks,
            &[],
            &[],
            &previous_scope_roots,
            &current_scope_roots,
            None,
            &mut clock_ids,
            &mut clock_periods,
        )
    }

    #[test]
    fn retained_reuse_decision_reports_blocking_reason() {
        let mut parent_root = Element::new(ElementKind::Stack, "page.parent.root");
        let child_root = Element::new(ElementKind::Row, "page.child.root");
        parent_root.children.push(child_root.clone());
        let sibling_root = Element::new(ElementKind::Rect, "page.sibling.root");

        let mut roots = ScopeRoots::default();
        roots.insert(
            ScopeId::new("page.parent"),
            RetainedRoot::from_elements(&[parent_root]),
        );
        roots.insert(
            ScopeId::new("page.child"),
            RetainedRoot::from_elements(&[child_root]),
        );
        roots.insert(
            ScopeId::new("page.sibling"),
            RetainedRoot::from_elements(&[sibling_root]),
        );

        let dirty_scopes = scopes(&["page.child"]);
        let dirty_root_ids = dirty_root_ids_for_scopes(&dirty_scopes, &roots);
        let disabled_context =
            RetainedReuseContext::new(false, &dirty_root_ids, &[], &roots, &dirty_scopes);
        let context = RetainedReuseContext::new(true, &dirty_root_ids, &[], &roots, &dirty_scopes);
        let missing_scope = scope("page.missing");
        let parent_scope = scope("page.parent");
        let child_scope = scope("page.child");
        let sibling_scope = scope("page.sibling");

        assert_eq!(
            disabled_context.decision(&sibling_scope),
            RetainedReuseDecision::Rebuild(RetainedComposeReason::RetainedReuseUnavailable)
        );
        assert_eq!(
            context.decision(&missing_scope),
            RetainedReuseDecision::Rebuild(RetainedComposeReason::MissingPreviousRoots)
        );
        assert_eq!(
            context.decision(&child_scope),
            RetainedReuseDecision::Rebuild(RetainedComposeReason::DirtyScope)
        );
        assert_eq!(
            context.decision_with_dirty_ancestor(&child_scope, true),
            RetainedReuseDecision::Rebuild(RetainedComposeReason::DirtyScope)
        );
        assert!(context.should_reset_rebuilt_scope_dependencies(&child_scope));
        assert_eq!(
            context.decision(&parent_scope),
            RetainedReuseDecision::Rebuild(RetainedComposeReason::DirtyDescendant)
        );
        assert!(context.should_reset_rebuilt_scope_dependencies(&parent_scope));
        assert_eq!(
            context.decision(&sibling_scope),
            RetainedReuseDecision::Reuse
        );
        assert!(!context.should_reset_rebuilt_scope_dependencies(&sibling_scope));
        assert_eq!(
            context.build_reason(&sibling_scope, true),
            Some(RetainedComposeReason::DirtyAncestor)
        );
        assert_eq!(context.build_reason(&sibling_scope, false), None);
        assert_eq!(
            context.build_reason_or_missing_element(&sibling_scope, false),
            RetainedComposeReason::MissingPreviousElement
        );
        assert_eq!(
            context.build_reason_or_missing_element(&parent_scope, false),
            RetainedComposeReason::DirtyDescendant
        );
    }

    #[test]
    fn retained_reuse_state_builds_context_with_dirty_root_index() {
        let mut parent_root = Element::new(ElementKind::Stack, "page.parent.root");
        let child_root = Element::new(ElementKind::Row, "page.child.root");
        parent_root.children.push(child_root.clone());
        let sibling_root = Element::new(ElementKind::Rect, "page.sibling.root");

        let mut roots = ScopeRoots::default();
        roots.insert(
            ScopeId::new("page.parent"),
            RetainedRoot::from_elements(&[parent_root]),
        );
        roots.insert(
            ScopeId::new("page.child"),
            RetainedRoot::from_elements(&[child_root]),
        );
        roots.insert(
            ScopeId::new("page.sibling"),
            RetainedRoot::from_elements(&[sibling_root]),
        );

        let state = RetainedReuseState::enabled(scopes(&["page.child"]), &roots);
        let context = state.context(&[], &roots);
        let parent_scope = scope("page.parent");
        let child_scope = scope("page.child");
        let sibling_scope = scope("page.sibling");

        assert!(state.is_dirty(&child_scope));
        assert!(!state.is_dirty(&parent_scope));
        assert_eq!(
            context.decision(&child_scope),
            RetainedReuseDecision::Rebuild(RetainedComposeReason::DirtyScope)
        );
        assert_eq!(
            context.decision(&parent_scope),
            RetainedReuseDecision::Rebuild(RetainedComposeReason::DirtyDescendant)
        );
        assert_eq!(
            context.decision(&sibling_scope),
            RetainedReuseDecision::Reuse
        );
    }

    #[test]
    fn begin_scope_frame_moves_retained_frame_policy_into_reconciler() {
        let mut previous_roots = ScopeRoots::default();
        previous_roots.insert(
            ScopeId::new("page.kept"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Rect, "page.kept.root")]),
        );
        let mut live_scopes = scopes(&["page.live"]);

        let frame = begin_scope_frame(
            true,
            Some(scopes(&["page.input"])),
            &mut previous_roots,
            &mut live_scopes,
        );

        assert!(frame.can_reuse_scopes);
        assert_eq!(sorted_scopes(&frame.input_dirty_scopes), vec!["page.input"]);
        assert_eq!(sorted_scopes(&frame.live_dirty_scopes), vec!["page.live"]);
        assert_eq!(
            sorted_scopes(&frame.dirty_scopes),
            vec!["page.input", "page.live"]
        );
        assert!(previous_roots.is_empty());
        assert!(live_scopes.is_empty());
        assert!(frame.previous_scope_roots.contains_key("page.kept"));
    }

    #[test]
    fn begin_scope_frame_clears_live_scopes_without_reuse() {
        let mut previous_roots = ScopeRoots::default();
        previous_roots.insert(
            ScopeId::new("page.previous"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Rect, "page.previous.root")]),
        );
        let mut live_scopes = scopes(&["page.live"]);

        let frame = begin_scope_frame(
            false,
            Some(scopes(&["page.input"])),
            &mut previous_roots,
            &mut live_scopes,
        );

        assert!(!frame.can_reuse_scopes);
        assert_eq!(sorted_scopes(&frame.input_dirty_scopes), vec!["page.input"]);
        assert!(frame.live_dirty_scopes.is_empty());
        assert_eq!(sorted_scopes(&frame.dirty_scopes), vec!["page.input"]);
        assert!(frame.previous_scope_roots.is_empty());
        assert!(previous_roots.contains_key("page.previous"));
        assert!(live_scopes.is_empty());
    }

    #[test]
    fn retained_reuse_plan_includes_elements_or_rebuild_reason() {
        let previous_roots = vec![Element::new(ElementKind::Row, "page.scope.root")];
        let mut scope_roots = ScopeRoots::default();
        scope_roots.insert(
            ScopeId::new("page.scope"),
            RetainedRoot::from_elements(&previous_roots),
        );
        let dirty_scopes = ScopeSet::default();
        let dirty_root_ids = dirty_root_ids_for_scopes(&dirty_scopes, &scope_roots);
        let context = RetainedReuseContext::new(
            true,
            &dirty_root_ids,
            &previous_roots,
            &scope_roots,
            &dirty_scopes,
        );
        let scope_id = scope("page.scope");

        match context.plan(&scope_id, false) {
            RetainedReusePlan::Reuse { elements } => {
                assert_eq!(elements.len(), 1);
                assert_eq!(elements[0].id, "page.scope.root");
            }
            RetainedReusePlan::Rebuild(reason) => {
                panic!("expected reusable scope, got {reason:?}");
            }
        }

        match context.plan(&scope_id, true) {
            RetainedReusePlan::Rebuild(RetainedComposeReason::DirtyAncestor) => {}
            other => panic!("expected dirty ancestor rebuild, got {other:?}"),
        }

        let empty_previous_roots = Vec::new();
        let missing_context = RetainedReuseContext::new(
            true,
            &dirty_root_ids,
            &empty_previous_roots,
            &scope_roots,
            &dirty_scopes,
        );
        match missing_context.plan(&scope_id, false) {
            RetainedReusePlan::Rebuild(RetainedComposeReason::MissingPreviousElement) => {}
            other => panic!("expected missing element rebuild, got {other:?}"),
        }
    }

    #[test]
    fn retained_scope_compose_plan_reports_reuse_or_build_without_dsl_policy() {
        let mut parent_root = Element::new(ElementKind::Stack, "page.parent.root");
        let child_root = Element::new(ElementKind::Row, "page.child.root");
        parent_root.children.push(child_root.clone());
        let sibling_root = Element::new(ElementKind::Rect, "page.sibling.root");
        let previous_roots = vec![parent_root.clone(), sibling_root.clone()];

        let mut scope_roots = ScopeRoots::default();
        scope_roots.insert(
            ScopeId::new("page.parent"),
            RetainedRoot::from_elements(&[parent_root]),
        );
        scope_roots.insert(
            ScopeId::new("page.child"),
            RetainedRoot::from_elements(&[child_root]),
        );
        scope_roots.insert(
            ScopeId::new("page.sibling"),
            RetainedRoot::from_elements(&[sibling_root]),
        );

        let dirty_scopes = scopes(&["page.child"]);
        let dirty_root_ids = dirty_root_ids_for_scopes(&dirty_scopes, &scope_roots);
        let context = RetainedReuseContext::new(
            true,
            &dirty_root_ids,
            &previous_roots,
            &scope_roots,
            &dirty_scopes,
        );

        match retained_scope_compose_plan(&context, &scope("page.sibling"), false) {
            RetainedScopeComposePlan::Reuse(RetainedReusePlan::Reuse { elements }) => {
                assert_eq!(elements.len(), 1);
                assert_eq!(elements[0].id, "page.sibling.root");
            }
            other => panic!("expected sibling reuse, got {other:?}"),
        }

        match retained_scope_compose_plan(&context, &scope("page.child"), false) {
            RetainedScopeComposePlan::Build(plan) => {
                assert_eq!(plan.reason, RetainedComposeReason::DirtyScope);
                assert!(plan.dirty_owner);
            }
            other => panic!("expected dirty child rebuild, got {other:?}"),
        }

        match retained_scope_compose_plan(&context, &scope("page.parent"), false) {
            RetainedScopeComposePlan::Build(plan) => {
                assert_eq!(plan.reason, RetainedComposeReason::DirtyDescendant);
                assert!(!plan.dirty_owner);
            }
            other => panic!("expected dirty-descendant parent rebuild, got {other:?}"),
        }

        match retained_scope_compose_plan(&context, &scope("page.sibling"), true) {
            RetainedScopeComposePlan::Build(plan) => {
                assert_eq!(plan.reason, RetainedComposeReason::DirtyAncestor);
                assert!(!plan.dirty_owner);
            }
            other => panic!("expected dirty-ancestor sibling rebuild, got {other:?}"),
        }
    }

    #[test]
    fn retained_layer_replay_for_scope_reads_previous_roots_inside_reconciler() {
        let layer_root = Element::new(ElementKind::Rect, "page.menu.layer");
        let previous_roots = vec![Element::new(ElementKind::Rect, "page.anchor"), layer_root];
        let scope_id = scope("page.menu");
        let mut layer_roots = ScopeLayerRoots::default();
        layer_roots.insert(scope_id.clone(), vec![NodeId::new("page.menu.layer")]);
        let intent = LayerIntent {
            id: LayerId::new("page.menu.layer.intent"),
            owner: NodeId::new("page.menu"),
            root: NodeId::new("page.menu.layer"),
            anchor: Some(NodeId::new("page.anchor")),
            fallback_anchor: None,
            boundary: None,
            open: true,
            kind: LayerKind::Popover,
            placement: LayerPlacement::BottomStart,
            size: LayerSize::new(crate::Size::Fixed(80.0), crate::Size::Fixed(40.0)),
            gap: 0.0,
            offset: [0.0, 0.0],
            collision: Default::default(),
            z_index: 3,
            outside_click: OutsideClickPolicy::Close,
        };
        let mut layer_intents = ScopeLayerIntents::default();
        layer_intents.insert(scope_id.clone(), vec![intent]);

        let replay = retained_layer_replay_for_scope(
            &scope_id,
            &previous_roots,
            &layer_roots,
            &layer_intents,
        );

        assert_eq!(replay.roots.len(), 1);
        assert_eq!(replay.roots[0].id, "page.menu.layer");
        assert_eq!(replay.intents.len(), 1);
        assert_eq!(replay.intents[0].root.as_str(), "page.menu.layer");
    }

    #[test]
    fn apply_retained_reuse_plan_preserves_nested_and_layer_scope_roots() {
        let mut parent_root = Element::new(ElementKind::Stack, "page.parent.root");
        let child_root = Element::new(ElementKind::Rect, "page.child.root");
        parent_root.children.push(child_root.clone());
        let layer_root = Element::new(ElementKind::Rect, "page.layer.root");
        let mut previous_scope_roots = ScopeRoots::default();
        previous_scope_roots.insert(
            ScopeId::new("page.parent"),
            RetainedRoot::from_elements(&[parent_root.clone()]),
        );
        previous_scope_roots.insert(
            ScopeId::new("page.child"),
            RetainedRoot::from_elements(&[child_root]),
        );
        previous_scope_roots.insert(
            ScopeId::new("page.layer"),
            RetainedRoot::from_elements(&[layer_root.clone()]),
        );
        let current_scope_roots = ScopeRoots::default();
        let mut callbacks = UiCallbacks::default();
        let mut previous_callbacks = UiCallbacks::default();
        let mut clock_ids = ScopeSet::default();
        let mut clock_periods = None;

        let applied = apply_retained_reuse_plan(
            &scope("page.parent"),
            RetainedReusePlan::Reuse {
                elements: vec![parent_root],
            },
            &mut callbacks,
            &mut previous_callbacks,
            &[],
            &[layer_root],
            &previous_scope_roots,
            &current_scope_roots,
            None,
            &mut clock_ids,
            &mut clock_periods,
        )
        .expect("reuse plan should apply");

        assert!(applied
            .preserved_scope_roots
            .contains_key(&ScopeId::new("page.child")));
        assert!(applied
            .preserved_scope_roots
            .contains_key(&ScopeId::new("page.layer")));
    }

    #[test]
    fn apply_rebuilt_scope_dependency_reset_schedules_signal_dependency_reset() {
        #[derive(Default)]
        struct AppState {
            old_value: i32,
            new_value: i32,
        }

        let state = State::new(AppState::default());
        let old_signal = state.signal(
            "old",
            |state| state.old_value,
            |state, value| state.old_value = value,
        );
        let new_signal = state.signal(
            "new",
            |state| state.new_value,
            |state, value| state.new_value = value,
        );
        let has_dependency = |key: &str, scope: &str| {
            state.signal_dependencies().iter().any(|(signal, scopes)| {
                signal.as_str() == key && scopes.iter().any(|candidate| candidate == scope)
            })
        };

        let mut initial_ui = Ui::new("reset");
        initial_ui.retained_scope("parent", |ui| {
            old_signal.watch(ui);
        });
        assert!(has_dependency("old", "reset.parent"));

        let mut parent_root = Element::new(ElementKind::Stack, "reset.parent.root");
        parent_root
            .children
            .push(Element::new(ElementKind::Rect, "reset.child.root"));
        let mut previous_scope_roots = ScopeRoots::default();
        previous_scope_roots.insert(
            ScopeId::new("reset.parent"),
            RetainedRoot::from_elements(&[parent_root]),
        );
        previous_scope_roots.insert(
            ScopeId::new("reset.child"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Rect, "reset.child.root")]),
        );
        let dirty_scopes = scopes(&["reset.child"]);
        let dirty_root_ids = dirty_root_ids_for_scopes(&dirty_scopes, &previous_scope_roots);
        let context = RetainedReuseContext::new(
            true,
            &dirty_root_ids,
            &[],
            &previous_scope_roots,
            &dirty_scopes,
        );

        let applied = apply_rebuilt_scope_dependency_reset(&context, &scope("reset.parent"));
        let ignored = apply_rebuilt_scope_dependency_reset(&context, &scope("reset.sibling"));

        assert!(applied.scheduled);
        assert!(!ignored.scheduled);

        let mut rebuilt_ui = Ui::new("reset");
        rebuilt_ui.retained_scope("parent", |ui| {
            new_signal.watch(ui);
        });

        assert!(!has_dependency("old", "reset.parent"));
        assert!(has_dependency("new", "reset.parent"));
    }

    #[test]
    fn committed_element_ids_collects_nested_tree_ids() {
        let mut root = Element::new(ElementKind::Column, "page.root");
        let mut child = Element::new(ElementKind::Row, "page.child");
        child
            .children
            .push(Element::new(ElementKind::Text, "page.label"));
        root.children.push(child);

        let ids =
            super::committed_element_ids(&[root, Element::new(ElementKind::Rect, "page.sibling")]);

        assert_eq!(ids.len(), 4);
        assert!(ids.contains("page.root"));
        assert!(ids.contains("page.child"));
        assert!(ids.contains("page.label"));
        assert!(ids.contains("page.sibling"));
    }

    #[test]
    fn apply_retained_reuse_plan_transfers_callbacks_for_reused_elements() {
        let click_count = Rc::new(Cell::new(0));
        let drag_count = Rc::new(Cell::new(0));
        let click_count_callback = click_count.clone();
        let drag_count_callback = drag_count.clone();
        let mut previous_callbacks = UiCallbacks::default();
        previous_callbacks.on_click.insert(
            ClickCallbackId::new(NodeId::new("page.hit")),
            Box::new(move || click_count_callback.set(click_count_callback.get() + 1)),
        );
        previous_callbacks.on_drag.insert(
            DragCallbackId::new(NodeId::new("page.hit")),
            Box::new(move |_| drag_count_callback.set(drag_count_callback.get() + 1)),
        );

        let mut callbacks = UiCallbacks::default();
        let applied = apply_reuse_without_clock(
            RetainedReusePlan::Reuse {
                elements: vec![Element::new(ElementKind::Rect, "page.hit")],
            },
            &mut callbacks,
            &mut previous_callbacks,
        )
        .expect("reuse plan should apply");

        assert_eq!(applied.elements.len(), 1);
        assert_eq!(applied.elements[0].id, "page.hit");
        assert_eq!(applied.scope.retained_roots.len(), 1);
        assert_eq!(applied.scope.retained_roots[0].id(), "page.hit");
        assert_eq!(applied.scope.record.previous_roots, 1);
        assert_eq!(applied.scope.record.current_roots, 1);
        assert_eq!(applied.scope.record.element_count, 1);
        assert_eq!(applied.scope.record.callback_transfers.click, 1);
        assert_eq!(applied.scope.record.callback_transfers.drag, 1);
        assert_eq!(applied.scope.record.callback_transfers.total(), 2);
        callbacks
            .on_click
            .get_mut(&ClickCallbackId::new(NodeId::new("page.hit")))
            .expect("click callback should transfer")();
        callbacks
            .on_drag
            .get_mut(&DragCallbackId::new(NodeId::new("page.hit")))
            .expect("drag callback should transfer")(DragEvent::default());
        assert_eq!(click_count.get(), 1);
        assert_eq!(drag_count.get(), 1);
        assert!(!previous_callbacks
            .on_click
            .contains_key(&ClickCallbackId::new(NodeId::new("page.hit"))));
        assert!(!previous_callbacks
            .on_drag
            .contains_key(&DragCallbackId::new(NodeId::new("page.hit"))));
    }

    #[test]
    fn apply_retained_reuse_plan_reports_reused_tree_metrics() {
        let mut root = Element::new(ElementKind::Column, "page.root");
        let mut child = Element::new(ElementKind::Row, "page.child");
        child
            .children
            .push(Element::new(ElementKind::Text, "page.label"));
        root.children.push(child);

        let mut callbacks = UiCallbacks::default();
        let mut previous_callbacks = UiCallbacks::default();
        let applied = apply_reuse_without_clock(
            RetainedReusePlan::Reuse {
                elements: vec![root, Element::new(ElementKind::Rect, "page.sibling")],
            },
            &mut callbacks,
            &mut previous_callbacks,
        )
        .expect("reuse plan should apply");

        assert_eq!(applied.scope.record.previous_roots, 2);
        assert_eq!(applied.scope.record.current_roots, 2);
        assert_eq!(applied.scope.record.element_count, 4);
        assert_eq!(applied.scope.retained_roots.len(), 2);
        assert_eq!(applied.scope.retained_roots[0].id(), "page.root");
        assert_eq!(
            applied.scope.retained_roots[0].children[0].id(),
            "page.child"
        );
        assert_eq!(
            applied.scope.retained_roots[0].children[0].children[0].id(),
            "page.label"
        );
        assert_eq!(applied.scope.retained_roots[1].id(), "page.sibling");
        assert_eq!(applied.scope.record.callback_transfers.total(), 0);

        let record = applied.scope.record;
        assert_eq!(record.id.as_str(), "page.scope");
        assert_eq!(record.action, RetainedComposeAction::Reused);
        assert_eq!(record.reason, RetainedComposeReason::CleanReuse);
        assert_eq!(record.previous_roots, 2);
        assert_eq!(record.current_roots, 2);
        assert_eq!(record.element_count, 4);
        assert_eq!(record.callback_transfers.total(), 0);
    }

    #[test]
    fn apply_retained_reuse_plan_preserves_periodic_clock_dependency() {
        let id = ScopeId::new("page.clock");
        let mut previous_clock_periods = ClockPeriodMap::default();
        previous_clock_periods.insert(id.clone(), Duration::from_millis(250));
        let mut clock_ids = ScopeSet::default();
        let mut clock_periods = None;
        let mut callbacks = UiCallbacks::default();
        let mut previous_callbacks = UiCallbacks::default();
        let previous_scope_roots = ScopeRoots::default();
        let current_scope_roots = ScopeRoots::default();

        apply_retained_reuse_plan(
            &id,
            RetainedReusePlan::Reuse {
                elements: vec![Element::new(ElementKind::Rect, "page.clock.root")],
            },
            &mut callbacks,
            &mut previous_callbacks,
            &[],
            &[],
            &previous_scope_roots,
            &current_scope_roots,
            Some(&previous_clock_periods),
            &mut clock_ids,
            &mut clock_periods,
        )
        .expect("reuse plan should apply");

        assert!(clock_ids.contains(&id));
        assert_eq!(
            clock_periods.as_ref().and_then(|periods| periods.get(&id)),
            Some(&Duration::from_millis(250))
        );
    }

    #[test]
    fn apply_retained_build_reports_built_tree_metrics() {
        let mut previous_scope_roots = ScopeRoots::default();
        previous_scope_roots.insert(
            ScopeId::new("page.scope"),
            vec![
                RetainedRoot::from_element(&Element::new(ElementKind::Rect, "page.old.a")),
                RetainedRoot::from_element(&Element::new(ElementKind::Rect, "page.old.b")),
            ],
        );
        let mut root = Element::new(ElementKind::Column, "page.scope.root");
        root.children
            .push(Element::new(ElementKind::Text, "page.scope.label"));

        let applied = apply_retained_build(
            ScopeId::new("page.scope"),
            &[root],
            RetainedComposeReason::DirtyScope,
            1.5,
            0.25,
            &previous_scope_roots,
        );

        assert_eq!(applied.retained_roots.len(), 1);
        assert_eq!(applied.retained_roots[0].id(), "page.scope.root");
        assert_eq!(
            applied.retained_roots[0].children[0].id(),
            "page.scope.label"
        );
        assert_eq!(applied.record.id.as_str(), "page.scope");
        assert_eq!(applied.record.action, RetainedComposeAction::Built);
        assert_eq!(applied.record.reason, RetainedComposeReason::DirtyScope);
        assert_eq!(applied.record.build_ms, 1.5);
        assert_eq!(applied.record.self_build_ms, 0.25);
        assert_eq!(applied.record.previous_roots, 2);
        assert_eq!(applied.record.current_roots, 1);
        assert_eq!(applied.record.element_count, 2);
        assert_eq!(applied.record.callback_transfers.total(), 0);
    }

    #[test]
    fn apply_removed_retained_scopes_reports_missing_previous_scopes_in_order() {
        let mut previous_scope_roots = ScopeRoots::default();
        previous_scope_roots.insert(
            ScopeId::new("page.removed_b"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Rect, "page.removed_b.root")]),
        );
        previous_scope_roots.insert(
            ScopeId::new("page.kept"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Rect, "page.kept.root")]),
        );
        previous_scope_roots.insert(
            ScopeId::new("page.removed_a"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Rect, "page.removed_a.root")]),
        );
        let mut next_scope_roots = ScopeRoots::default();
        next_scope_roots.insert(
            ScopeId::new("page.kept"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Rect, "page.kept.root")]),
        );
        next_scope_roots.insert(
            ScopeId::new("page.new"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Rect, "page.new.root")]),
        );

        let applied = apply_removed_retained_scopes(&previous_scope_roots, &next_scope_roots);

        assert_eq!(applied.removed_scopes[0].as_str(), "page.removed_a");
        assert_eq!(applied.removed_scopes[1].as_str(), "page.removed_b");
    }

    #[test]
    fn apply_removed_retained_scopes_clears_signal_dependencies_for_unmounted_scopes() {
        #[derive(Default)]
        struct AppState {
            value: i32,
        }

        let state = State::new(AppState::default());
        let value = state.signal(
            "value",
            |state| state.value,
            |state, value| state.value = value,
        );
        let mut ui = Ui::new("page");
        ui.retained_scope("removed", |ui| {
            value.watch(ui);
        });

        assert_eq!(state.signal_dependencies().len(), 1);

        let mut previous_scope_roots = ScopeRoots::default();
        previous_scope_roots.insert(
            ScopeId::new("page.removed"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Rect, "page.removed.root")]),
        );
        let next_scope_roots = ScopeRoots::default();

        let applied = apply_removed_retained_scopes(&previous_scope_roots, &next_scope_roots);

        assert_eq!(applied.removed_scopes, vec![ScopeId::new("page.removed")]);
        assert!(state.signal_dependencies().is_empty());
        value.set(1);
        assert!(state.dirty().is_empty());
    }

    #[test]
    fn retained_layout_reuse_plan_reports_reuse_blockers() {
        let dirty_scopes = scopes(&["page.scope"]);
        let mut previous_scope_roots = ScopeRoots::default();
        previous_scope_roots.insert(
            ScopeId::new("page.scope"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Rect, "page.scope.root")]),
        );
        let mut next_scope_roots = ScopeRoots::default();
        next_scope_roots.insert(
            ScopeId::new("page.scope"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Row, "page.scope.root")]),
        );

        let unavailable = retained_layout_reuse_plan(
            false,
            &dirty_scopes,
            &previous_scope_roots,
            &next_scope_roots,
            true,
        );
        assert_eq!(
            unavailable.blocker,
            Some(FullLayoutReason::RetainedReuseUnavailable)
        );
        assert!(unavailable.structural_reports.is_empty());

        let mut missing_dirty_scopes = ScopeSet::default();
        missing_dirty_scopes.insert(ScopeId::new("page.missing"));
        let missing = retained_layout_reuse_plan(
            true,
            &missing_dirty_scopes,
            &previous_scope_roots,
            &next_scope_roots,
            true,
        );
        assert_eq!(
            missing.blocker,
            Some(FullLayoutReason::MissingPreviousRetainedRoot {
                id: ScopeId::new("page.missing")
            })
        );
        assert!(missing.structural_reports.is_empty());

        let structural = retained_layout_reuse_plan(
            true,
            &dirty_scopes,
            &previous_scope_roots,
            &next_scope_roots,
            true,
        );
        assert_eq!(
            structural.blocker,
            Some(FullLayoutReason::StructureChanged {
                ids: vec![ScopeId::new("page.scope")]
            })
        );
        assert_eq!(structural.structural_reports.len(), 1);
        assert!(structural.structural_reports[0].contains("page.scope"));

        let clean = retained_layout_reuse_plan(
            true,
            &ScopeSet::default(),
            &previous_scope_roots,
            &next_scope_roots,
            true,
        );
        assert_eq!(clean.blocker, None);
        assert!(clean.structural_reports.is_empty());
    }

    #[test]
    fn retained_layout_dirty_scopes_unions_and_normalizes_dirty_sets() {
        let mut parent_root = Element::new(ElementKind::Stack, "page.parent.root");
        let child_root = Element::new(ElementKind::Row, "page.child.root");
        parent_root.children.push(child_root.clone());
        let sibling_root = Element::new(ElementKind::Rect, "page.sibling.root");

        let mut previous_scope_roots = ScopeRoots::default();
        previous_scope_roots.insert(
            ScopeId::new("page.parent"),
            RetainedRoot::from_elements(&[parent_root]),
        );
        previous_scope_roots.insert(
            ScopeId::new("page.child"),
            RetainedRoot::from_elements(&[child_root]),
        );
        previous_scope_roots.insert(
            ScopeId::new("page.sibling"),
            RetainedRoot::from_elements(&[sibling_root]),
        );

        let compose_dirty_scopes = scopes(&["page.child"]);
        let layout_dirty_scopes = scopes(&["page.parent", "page.sibling"]);

        let normalized = retained_layout_dirty_scopes(
            &compose_dirty_scopes,
            &layout_dirty_scopes,
            &previous_scope_roots,
        );

        assert_eq!(
            sorted_scopes(&normalized),
            vec!["page.parent".to_string(), "page.sibling".to_string()]
        );
    }

    #[test]
    fn apply_retained_reuse_plan_preserves_callbacks_when_rebuild_is_required() {
        let mut previous_callbacks = UiCallbacks::default();
        previous_callbacks.on_click.insert(
            ClickCallbackId::new(NodeId::new("page.hit")),
            Box::new(|| {}),
        );
        let mut callbacks = UiCallbacks::default();

        let reason = apply_reuse_without_clock(
            RetainedReusePlan::Rebuild(RetainedComposeReason::DirtyScope),
            &mut callbacks,
            &mut previous_callbacks,
        )
        .expect_err("rebuild plan should not transfer callbacks");

        assert_eq!(reason, RetainedComposeReason::DirtyScope);
        assert!(callbacks.on_click.is_empty());
        assert!(previous_callbacks
            .on_click
            .contains_key(&ClickCallbackId::new(NodeId::new("page.hit"))));
    }

    fn sorted_scopes(scopes: &ScopeSet) -> Vec<String> {
        let mut scopes = scopes
            .iter()
            .map(|scope| scope.as_str().to_string())
            .collect::<Vec<_>>();
        scopes.sort();
        scopes
    }
}
