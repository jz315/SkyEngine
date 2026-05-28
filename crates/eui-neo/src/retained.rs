//! Retained-scope bookkeeping for partial UI composition.
//!
//! Signals decide which scopes are dirty. Live scopes opt into rebuilding every
//! scoped compose. The runtime owns the previous frame's retained roots and
//! merges these two invalidation sources before `Ui` decides which scopes can
//! be reused.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::Element;

pub(crate) type ScopeId = String;
pub(crate) type ScopeRoots = FxHashMap<ScopeId, Vec<Element>>;
pub(crate) type ScopeSet = FxHashSet<ScopeId>;

pub(crate) struct ScopeFrame {
    pub can_reuse_scopes: bool,
    pub input_dirty_scopes: ScopeSet,
    pub live_dirty_scopes: ScopeSet,
    pub dirty_scopes: ScopeSet,
    pub previous_scope_roots: ScopeRoots,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetainedComposeStats {
    pub built: usize,
    pub reused: usize,
    pub partial_layout: bool,
    pub full_layout: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetainedComposeAction {
    Built,
    Reused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedComposeEvent {
    pub id: ScopeId,
    pub action: RetainedComposeAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutMode {
    Partial,
    Full(FullLayoutReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FullLayoutReason {
    RetainedReuseUnavailable,
    NoDirtyIds,
    MissingPreviousRetainedRoot { id: ScopeId },
    StructureChanged { ids: Vec<ScopeId> },
    DirtyRetainedLayoutFailed,
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

impl ScopeFrame {
    pub fn partial_layout_blocker(
        &self,
        layout_dirty_scopes: &ScopeSet,
    ) -> Option<FullLayoutReason> {
        if !self.can_reuse_scopes {
            return Some(FullLayoutReason::RetainedReuseUnavailable);
        }
        if layout_dirty_scopes.is_empty() {
            return Some(FullLayoutReason::NoDirtyIds);
        }
        layout_dirty_scopes.iter().find_map(|scope| {
            (!self.previous_scope_roots.contains_key(scope))
                .then(|| FullLayoutReason::MissingPreviousRetainedRoot { id: scope.clone() })
        })
    }
}

/// Live scopes are intentionally dirty on every scoped compose. They cover
/// frame-time/procedural animation without pretending that time is app state.
fn merge_live_scopes(dirty_scopes: &mut ScopeSet, live_scopes: &mut ScopeSet) -> ScopeSet {
    let live_dirty_scopes = std::mem::take(live_scopes);
    dirty_scopes.extend(live_dirty_scopes.iter().cloned());
    live_dirty_scopes
}

pub(crate) fn scope_contains_id(scope: &str, id: &str) -> bool {
    scope == id || is_resolved_id(id, scope)
}

pub(crate) fn scope_parent<'a>(
    scope: &str,
    scopes: impl IntoIterator<Item = &'a String>,
) -> Option<String> {
    scopes
        .into_iter()
        .filter(|candidate| candidate.as_str() != scope && scope_contains_id(candidate, scope))
        .max_by_key(|candidate| scope_depth(candidate))
        .cloned()
}

pub(crate) fn scope_has_dirty_descendant(dirty_scopes: &ScopeSet, scope: &str) -> bool {
    dirty_scopes
        .iter()
        .any(|dirty| dirty == scope || is_resolved_id(dirty, scope))
}

pub(crate) fn structurally_incompatible_dirty_scopes(
    dirty_scopes: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
    next_scope_roots: &ScopeRoots,
) -> Vec<ScopeId> {
    let mut scopes: Vec<_> = dirty_scopes
        .iter()
        .filter_map(|scope| {
            (!dirty_scope_is_structurally_compatible(scope, previous_scope_roots, next_scope_roots))
                .then(|| scope.clone())
        })
        .collect();
    scopes.sort();
    scopes
}

pub(crate) fn normalize_dirty_scopes_with_roots(
    dirty_scopes: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
) -> ScopeSet {
    let mut scopes: Vec<_> = dirty_scopes.iter().cloned().collect();
    scopes.sort_by(|left, right| {
        scope_depth(left)
            .cmp(&scope_depth(right))
            .then_with(|| left.cmp(right))
    });

    let mut normalized = ScopeSet::default();
    for scope in scopes {
        if dirty_scopes.iter().any(|candidate| {
            candidate != &scope
                && dirty_scope_contains(candidate, &scope, previous_scope_roots)
        })
        {
            continue;
        }
        normalized.insert(scope);
    }
    normalized
}

fn dirty_scope_contains(candidate: &str, scope: &str, previous_scope_roots: &ScopeRoots) -> bool {
    let Some(scope_roots) = previous_scope_roots.get(scope) else {
        return false;
    };
    let Some(candidate_roots) = previous_scope_roots.get(candidate) else {
        return false;
    };
    scope_roots.iter().any(|scope_root| {
        candidate_roots
            .iter()
            .any(|candidate_root| element_contains_id(candidate_root, &scope_root.id))
    })
}

fn element_contains_id(element: &Element, id: &str) -> bool {
    element.id == id
        || element
            .children
            .iter()
            .any(|child| element_contains_id(child, id))
}

fn dirty_scope_is_structurally_compatible(
    scope: &str,
    previous_scope_roots: &ScopeRoots,
    next_scope_roots: &ScopeRoots,
) -> bool {
    let Some(previous_roots) = previous_scope_roots.get(scope) else {
        return false;
    };
    let Some(next_roots) = next_scope_roots.get(scope) else {
        return false;
    };
    element_lists_are_structurally_compatible(previous_roots, next_roots)
}

fn element_lists_are_structurally_compatible(previous: &[Element], next: &[Element]) -> bool {
    previous.len() == next.len()
        && previous
            .iter()
            .zip(next)
            .all(|(previous, next)| elements_are_structurally_compatible(previous, next))
}

fn elements_are_structurally_compatible(previous: &Element, next: &Element) -> bool {
    previous.id == next.id
        && previous.kind == next.kind
        && previous.z_index == next.z_index
        && previous.clip == next.clip
        && previous.children.len() == next.children.len()
        && element_lists_are_structurally_compatible(&previous.children, &next.children)
}

fn is_resolved_id(id: &str, parent: &str) -> bool {
    id.len() > parent.len()
        && id.starts_with(parent)
        && id.as_bytes().get(parent.len()) == Some(&b'.')
}

fn scope_depth(scope: &str) -> usize {
    scope
        .as_bytes()
        .iter()
        .filter(|byte| **byte == b'.')
        .count()
}

#[cfg(test)]
mod tests {
    use crate::{Element, ElementKind};

    use super::{normalize_dirty_scopes_with_roots, ScopeRoots, ScopeSet};

    fn scopes(ids: &[&str]) -> ScopeSet {
        ids.iter().map(|id| (*id).to_string()).collect()
    }

    #[test]
    fn dirty_normalization_does_not_infer_ancestry_from_id_prefixes() {
        let normalized = normalize_dirty_scopes_with_roots(
            &scopes(&[
                "page.panel.child.live",
                "page.panel",
                "page.sidebar.item",
                "page.sidebar",
                "page.other",
            ]),
            &ScopeRoots::default(),
        );

        assert_eq!(
            sorted(&normalized),
            vec![
                "page.other".to_string(),
                "page.panel".to_string(),
                "page.panel.child.live".to_string(),
                "page.sidebar".to_string(),
                "page.sidebar.item".to_string(),
            ]
        );
    }

    #[test]
    fn dirty_normalization_keeps_all_ids_without_tree_containment() {
        let normalized = normalize_dirty_scopes_with_roots(
            &scopes(&[
                "page.panel",
                "page.panel_extra.child",
                "page.panel.child",
            ]),
            &ScopeRoots::default(),
        );

        assert_eq!(
            sorted(&normalized),
            vec![
                "page.panel".to_string(),
                "page.panel.child".to_string(),
                "page.panel_extra.child".to_string()
            ]
        );
    }

    #[test]
    fn dirty_normalization_removes_layout_descendants_from_retained_roots() {
        let mut scroll = Element::new(ElementKind::Stack, "page.interactions.scroll");
        let mut content = Element::new(ElementKind::Column, "page.interactions.scroll.content");
        content
            .children
            .push(Element::new(ElementKind::Row, "page.interactions.cards"));
        scroll.children.push(content);

        let mut roots = ScopeRoots::default();
        roots.insert("page.interactions.scroll".to_string(), vec![scroll]);
        roots.insert(
            "page.interactions.cards".to_string(),
            vec![Element::new(ElementKind::Row, "page.interactions.cards")],
        );

        let normalized = normalize_dirty_scopes_with_roots(
            &scopes(&["page.interactions.cards", "page.interactions.scroll"]),
            &roots,
        );

        assert_eq!(
            sorted(&normalized),
            vec!["page.interactions.scroll".to_string()]
        );
    }

    fn sorted(scopes: &ScopeSet) -> Vec<String> {
        let mut scopes: Vec<_> = scopes.iter().cloned().collect();
        scopes.sort();
        scopes
    }
}
