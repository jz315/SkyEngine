use rustc_hash::{FxHashMap, FxHashSet};

use crate::callbacks::UiCallbacks;
use crate::retained::{
    previous_elements_for_scope, retained_scope_contains_dirty_root,
    structural_incompatibility_reports, structurally_incompatible_dirty_scopes,
    CallbackTransferStats, FullLayoutReason, RetainedComposeAction, RetainedComposeReason,
    RetainedRoot, ScopeComposeRecord, ScopeId, ScopeRoots, ScopeSet,
};
use crate::signal::clear_scope_signal_dependencies;
use crate::Element;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetainedReuseDecision {
    Reuse,
    Rebuild(RetainedComposeReason),
}

impl RetainedReuseDecision {
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
pub(crate) struct RetainedReuseApplied {
    pub elements: Vec<Element>,
    pub retained_roots: Vec<RetainedRoot>,
    pub root_count: usize,
    pub element_count: usize,
    pub callback_transfers: CallbackTransferStats,
}

impl RetainedReuseApplied {
    pub(crate) fn compose_record(&self, id: ScopeId) -> ScopeComposeRecord {
        ScopeComposeRecord {
            id,
            action: RetainedComposeAction::Reused,
            reason: RetainedComposeReason::CleanReuse,
            build_ms: 0.0,
            self_build_ms: 0.0,
            previous_roots: self.root_count,
            current_roots: self.root_count,
            element_count: self.element_count,
            callback_transfers: self.callback_transfers,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RetainedBuildApplied {
    pub retained_roots: Vec<RetainedRoot>,
    pub record: ScopeComposeRecord,
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

pub(crate) struct RetainedReuseContext<'a> {
    scope_reuse_enabled: bool,
    dirty_root_ids: &'a FxHashSet<String>,
    previous_roots: &'a [Element],
    previous_scope_roots: &'a ScopeRoots,
    dirty_scopes: &'a ScopeSet,
}

impl<'a> RetainedReuseContext<'a> {
    pub(crate) fn new(
        scope_reuse_enabled: bool,
        dirty_root_ids: &'a FxHashSet<String>,
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

    pub(crate) fn decision(&self, id: &str) -> RetainedReuseDecision {
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
        id: &str,
        has_dirty_ancestor: bool,
    ) -> RetainedReuseDecision {
        let decision = self.decision(id);
        if has_dirty_ancestor && decision == RetainedReuseDecision::Reuse {
            RetainedReuseDecision::Rebuild(RetainedComposeReason::DirtyAncestor)
        } else {
            decision
        }
    }

    pub(crate) fn build_reason(
        &self,
        id: &str,
        has_dirty_ancestor: bool,
    ) -> Option<RetainedComposeReason> {
        self.decision_with_dirty_ancestor(id, has_dirty_ancestor)
            .rebuild_reason()
    }

    pub(crate) fn should_reset_rebuilt_scope_dependencies(&self, id: &str) -> bool {
        self.scope_reuse_enabled
            && self.previous_scope_roots.contains_key(id)
            && retained_scope_contains_dirty_root(
                self.dirty_root_ids,
                self.previous_scope_roots,
                id,
            )
    }

    pub(crate) fn plan(&self, id: &str, has_dirty_ancestor: bool) -> RetainedReusePlan {
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

pub(crate) fn apply_retained_reuse_plan(
    plan: RetainedReusePlan,
    callbacks: &mut UiCallbacks,
    previous_callbacks: &mut UiCallbacks,
) -> Result<RetainedReuseApplied, RetainedComposeReason> {
    let elements = match plan {
        RetainedReusePlan::Reuse { elements } => elements,
        RetainedReusePlan::Rebuild(reason) => return Err(reason),
    };
    let root_count = elements.len();
    let element_count = count_elements(&elements);
    let retained_roots = RetainedRoot::from_elements(&elements);
    let callback_transfers = callbacks.transfer_for_elements(previous_callbacks, &elements);
    Ok(RetainedReuseApplied {
        elements,
        retained_roots,
        root_count,
        element_count,
        callback_transfers,
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
    let previous_roots = previous_scope_roots
        .get(id.as_str())
        .map_or(0, |roots| roots.len());
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
    RetainedBuildApplied {
        retained_roots,
        record,
    }
}

pub(crate) fn apply_removed_retained_scopes(
    previous_scope_roots: &ScopeRoots,
    next_scope_roots: &ScopeRoots,
) -> RetainedUnmountApplied {
    let removed_scopes = removed_retained_scope_ids(previous_scope_roots, next_scope_roots);
    for scope in &removed_scopes {
        clear_scope_signal_dependencies(scope.as_str());
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
    RetainedLayoutReusePlan {
        blocker: (!structurally_incompatible.is_empty()).then_some(
            FullLayoutReason::StructureChanged {
                ids: structurally_incompatible,
            },
        ),
        structural_reports,
    }
}

pub(crate) fn refresh_scope_roots_from_tree(scope_roots: &mut ScopeRoots, roots: &[Element]) {
    let mut elements_by_id = FxHashMap::default();
    collect_elements_by_id(roots, &mut elements_by_id);
    for elements in scope_roots.values_mut() {
        for element in elements {
            if let Some(updated) = elements_by_id.get(element.id.as_str()) {
                refresh_retained_root_layout_frames(element, updated);
            }
        }
    }
}

fn collect_elements_by_id<'a>(
    elements: &'a [Element],
    index: &mut FxHashMap<&'a str, &'a Element>,
) {
    for element in elements {
        index.entry(element.id.as_str()).or_insert(element);
        collect_elements_by_id(&element.children, index);
    }
}

fn refresh_retained_root_layout_frames(element: &mut RetainedRoot, updated: &Element) {
    // Layout mutates only Element::frame. Retained scope roots keep the same
    // visual/callback data unless their structure changed, so refresh frames
    // in place and fall back to replacement only when the tree no longer matches.
    if element.kind != updated.kind
        || element.id != updated.id
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

    use crate::callbacks::{ClickCallbackId, DragCallbackId, UiCallbacks};
    use crate::retained::{
        dirty_root_ids_for_scopes, FullLayoutReason, RetainedComposeAction, RetainedRoot, ScopeId,
        ScopeRoots, ScopeSet,
    };
    use crate::signal::State;
    use crate::{DragEvent, Element, ElementKind, Ui};

    use super::{
        apply_removed_retained_scopes, apply_retained_build, apply_retained_reuse_plan,
        retained_layout_reuse_plan, RetainedComposeReason, RetainedReuseContext,
        RetainedReuseDecision, RetainedReusePlan,
    };

    fn scopes(ids: &[&str]) -> ScopeSet {
        ids.iter().map(|id| ScopeId::new(*id)).collect()
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

        assert_eq!(
            disabled_context.decision("page.sibling"),
            RetainedReuseDecision::Rebuild(RetainedComposeReason::RetainedReuseUnavailable)
        );
        assert_eq!(
            context.decision("page.missing"),
            RetainedReuseDecision::Rebuild(RetainedComposeReason::MissingPreviousRoots)
        );
        assert_eq!(
            context.decision("page.child"),
            RetainedReuseDecision::Rebuild(RetainedComposeReason::DirtyScope)
        );
        assert_eq!(
            context.decision_with_dirty_ancestor("page.child", true),
            RetainedReuseDecision::Rebuild(RetainedComposeReason::DirtyScope)
        );
        assert!(context.should_reset_rebuilt_scope_dependencies("page.child"));
        assert_eq!(
            context.decision("page.parent"),
            RetainedReuseDecision::Rebuild(RetainedComposeReason::DirtyDescendant)
        );
        assert!(context.should_reset_rebuilt_scope_dependencies("page.parent"));
        assert_eq!(
            context.decision("page.sibling"),
            RetainedReuseDecision::Reuse
        );
        assert!(!context.should_reset_rebuilt_scope_dependencies("page.sibling"));
        assert_eq!(
            context.build_reason("page.sibling", true),
            Some(RetainedComposeReason::DirtyAncestor)
        );
        assert_eq!(context.build_reason("page.sibling", false), None);
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

        match context.plan("page.scope", false) {
            RetainedReusePlan::Reuse { elements } => {
                assert_eq!(elements.len(), 1);
                assert_eq!(elements[0].id, "page.scope.root");
            }
            RetainedReusePlan::Rebuild(reason) => {
                panic!("expected reusable scope, got {reason:?}");
            }
        }

        match context.plan("page.scope", true) {
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
        match missing_context.plan("page.scope", false) {
            RetainedReusePlan::Rebuild(RetainedComposeReason::MissingPreviousElement) => {}
            other => panic!("expected missing element rebuild, got {other:?}"),
        }
    }

    #[test]
    fn apply_retained_reuse_plan_transfers_callbacks_for_reused_elements() {
        let click_count = Rc::new(Cell::new(0));
        let drag_count = Rc::new(Cell::new(0));
        let click_count_callback = click_count.clone();
        let drag_count_callback = drag_count.clone();
        let mut previous_callbacks = UiCallbacks::default();
        previous_callbacks.on_click.insert(
            ClickCallbackId::new("page.hit"),
            Box::new(move || click_count_callback.set(click_count_callback.get() + 1)),
        );
        previous_callbacks.on_drag.insert(
            DragCallbackId::new("page.hit"),
            Box::new(move |_| drag_count_callback.set(drag_count_callback.get() + 1)),
        );

        let mut callbacks = UiCallbacks::default();
        let applied = apply_retained_reuse_plan(
            RetainedReusePlan::Reuse {
                elements: vec![Element::new(ElementKind::Rect, "page.hit")],
            },
            &mut callbacks,
            &mut previous_callbacks,
        )
        .expect("reuse plan should apply");

        assert_eq!(applied.elements.len(), 1);
        assert_eq!(applied.elements[0].id, "page.hit");
        assert_eq!(applied.retained_roots.len(), 1);
        assert_eq!(applied.retained_roots[0].id, "page.hit");
        assert_eq!(applied.root_count, 1);
        assert_eq!(applied.element_count, 1);
        assert_eq!(applied.callback_transfers.click, 1);
        assert_eq!(applied.callback_transfers.drag, 1);
        assert_eq!(applied.callback_transfers.total(), 2);
        callbacks
            .on_click
            .get_mut(&ClickCallbackId::new("page.hit"))
            .expect("click callback should transfer")();
        callbacks
            .on_drag
            .get_mut(&DragCallbackId::new("page.hit"))
            .expect("drag callback should transfer")(DragEvent::default());
        assert_eq!(click_count.get(), 1);
        assert_eq!(drag_count.get(), 1);
        assert!(!previous_callbacks
            .on_click
            .contains_key(&ClickCallbackId::new("page.hit")));
        assert!(!previous_callbacks
            .on_drag
            .contains_key(&DragCallbackId::new("page.hit")));
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
        let applied = apply_retained_reuse_plan(
            RetainedReusePlan::Reuse {
                elements: vec![root, Element::new(ElementKind::Rect, "page.sibling")],
            },
            &mut callbacks,
            &mut previous_callbacks,
        )
        .expect("reuse plan should apply");

        assert_eq!(applied.root_count, 2);
        assert_eq!(applied.element_count, 4);
        assert_eq!(applied.retained_roots.len(), 2);
        assert_eq!(applied.retained_roots[0].id, "page.root");
        assert_eq!(applied.retained_roots[0].children[0].id, "page.child");
        assert_eq!(
            applied.retained_roots[0].children[0].children[0].id,
            "page.label"
        );
        assert_eq!(applied.retained_roots[1].id, "page.sibling");
        assert_eq!(applied.callback_transfers.total(), 0);

        let record = applied.compose_record(ScopeId::new("page.scope"));
        assert_eq!(record.id.as_str(), "page.scope");
        assert_eq!(record.action, RetainedComposeAction::Reused);
        assert_eq!(record.reason, RetainedComposeReason::CleanReuse);
        assert_eq!(record.previous_roots, 2);
        assert_eq!(record.current_roots, 2);
        assert_eq!(record.element_count, 4);
        assert_eq!(record.callback_transfers.total(), 0);
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
        assert_eq!(applied.retained_roots[0].id, "page.scope.root");
        assert_eq!(applied.retained_roots[0].children[0].id, "page.scope.label");
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
    fn apply_retained_reuse_plan_preserves_callbacks_when_rebuild_is_required() {
        let mut previous_callbacks = UiCallbacks::default();
        previous_callbacks
            .on_click
            .insert(ClickCallbackId::new("page.hit"), Box::new(|| {}));
        let mut callbacks = UiCallbacks::default();

        let reason = apply_retained_reuse_plan(
            RetainedReusePlan::Rebuild(RetainedComposeReason::DirtyScope),
            &mut callbacks,
            &mut previous_callbacks,
        )
        .expect_err("rebuild plan should not transfer callbacks");

        assert_eq!(reason, RetainedComposeReason::DirtyScope);
        assert!(callbacks.on_click.is_empty());
        assert!(previous_callbacks
            .on_click
            .contains_key(&ClickCallbackId::new("page.hit")));
    }
}
