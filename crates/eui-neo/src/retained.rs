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
    pub dirty_scopes: ScopeSet,
    pub previous_scope_roots: ScopeRoots,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScopeComposeStats {
    pub built: usize,
    pub reused: usize,
    pub partial_layout: bool,
    pub full_layout: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeComposeAction {
    Built,
    Reused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeComposeEvent {
    pub scope: ScopeId,
    pub action: ScopeComposeAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutMode {
    Partial,
    Full(FullLayoutReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FullLayoutReason {
    ScopeReuseUnavailable,
    NoDirtyScopes,
    MissingPreviousScopeRoot { scope: ScopeId },
    StructureChanged { scopes: Vec<ScopeId> },
    DirtyScopeLayoutFailed,
}

pub(crate) fn begin_scope_frame(
    can_reuse_scopes: bool,
    dirty_scopes: Option<ScopeSet>,
    scope_roots: &mut ScopeRoots,
    live_scopes: &mut ScopeSet,
) -> ScopeFrame {
    let mut dirty_scopes = dirty_scopes.unwrap_or_default();
    let previous_scope_roots = if can_reuse_scopes {
        merge_live_scopes(&mut dirty_scopes, live_scopes);
        std::mem::take(scope_roots)
    } else {
        live_scopes.clear();
        ScopeRoots::default()
    };

    ScopeFrame {
        can_reuse_scopes,
        dirty_scopes,
        previous_scope_roots,
    }
}

impl ScopeFrame {
    pub fn partial_layout_blocker(&self) -> Option<FullLayoutReason> {
        if !self.can_reuse_scopes {
            return Some(FullLayoutReason::ScopeReuseUnavailable);
        }
        if self.dirty_scopes.is_empty() {
            return Some(FullLayoutReason::NoDirtyScopes);
        }
        self.dirty_scopes.iter().find_map(|scope| {
            (!self.previous_scope_roots.contains_key(scope)).then(|| {
                FullLayoutReason::MissingPreviousScopeRoot {
                    scope: scope.clone(),
                }
            })
        })
    }
}

/// Live scopes are intentionally dirty on every scoped compose. They cover
/// frame-time/procedural animation without pretending that time is app state.
fn merge_live_scopes(dirty_scopes: &mut ScopeSet, live_scopes: &mut ScopeSet) {
    dirty_scopes.extend(std::mem::take(live_scopes));
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
