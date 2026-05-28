//! Retained-scope bookkeeping for partial UI composition.
//!
//! Signals decide which scopes are dirty. Live scopes opt into rebuilding every
//! scoped compose. The runtime owns the previous frame's retained roots and
//! merges these two invalidation sources before `Ui` decides which scopes can
//! be reused.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::OnceLock;

use crate::{Element, ElementKind, LayoutRect};

pub(crate) type ScopeId = String;
pub(crate) type ScopeRoots = FxHashMap<ScopeId, Vec<RetainedRoot>>;
pub(crate) type ScopeSet = FxHashSet<ScopeId>;

/// Stable identity and layout-relevant shape for a retained root.
///
/// Full `Element` trees stay in `Runtime::roots`; retained roots keep only the
/// metadata needed to normalize dirty scopes, validate partial layout, and
/// anchor debug/layout refresh after layout has resolved frames.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RetainedRoot {
    pub id: String,
    pub kind: ElementKind,
    pub z_index: i32,
    pub clip: bool,
    pub clip_radius: f32,
    pub width: crate::Size,
    pub height: crate::Size,
    pub margin: crate::EdgeInsets,
    pub min_width: f32,
    pub max_layout_width: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub grow: f32,
    pub frame: LayoutRect,
    pub children: Vec<RetainedRoot>,
}

impl RetainedRoot {
    pub(crate) fn from_element(element: &Element) -> Self {
        Self {
            id: element.id.clone(),
            kind: element.kind,
            z_index: element.z_index,
            clip: element.clip,
            clip_radius: element.clip_radius,
            width: element.width,
            height: element.height,
            margin: element.margin,
            min_width: element.min_width,
            max_layout_width: element.max_layout_width,
            min_height: element.min_height,
            max_height: element.max_height,
            grow: element.grow,
            frame: element.frame,
            children: element.children.iter().map(Self::from_element).collect(),
        }
    }

    pub(crate) fn from_elements(elements: &[Element]) -> Vec<Self> {
        elements.iter().map(Self::from_element).collect()
    }
}

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

#[derive(Debug, Clone, PartialEq)]
pub struct ScopeComposeRecord {
    pub id: ScopeId,
    pub action: RetainedComposeAction,
    pub build_ms: f32,
    pub previous_roots: usize,
    pub current_roots: usize,
    pub element_count: usize,
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

/// Live scopes are intentionally dirty on every scoped compose. They cover
/// frame-time/procedural animation without pretending that time is app state.
fn merge_live_scopes(dirty_scopes: &mut ScopeSet, live_scopes: &mut ScopeSet) -> ScopeSet {
    let live_dirty_scopes = std::mem::take(live_scopes);
    dirty_scopes.extend(live_dirty_scopes.iter().cloned());
    live_dirty_scopes
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

pub(crate) fn structural_incompatibility_reports(
    dirty_scopes: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
    next_scope_roots: &ScopeRoots,
) -> Vec<String> {
    structurally_incompatible_dirty_scopes(dirty_scopes, previous_scope_roots, next_scope_roots)
        .into_iter()
        .map(|scope| {
            let reason = match (
                previous_scope_roots.get(&scope),
                next_scope_roots.get(&scope),
            ) {
                (Some(previous), Some(next)) => first_element_list_mismatch(previous, next)
                    .unwrap_or_else(|| "unknown structural mismatch".to_string()),
                (None, _) => "missing previous roots".to_string(),
                (_, None) => "missing current roots".to_string(),
            };
            format!("{scope}: {reason}")
        })
        .collect()
}

pub(crate) fn normalize_dirty_scopes_with_roots(
    dirty_scopes: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
) -> ScopeSet {
    let mut scopes: Vec<_> = dirty_scopes.iter().cloned().collect();
    scopes.sort();

    let mut normalized = ScopeSet::default();
    for scope in scopes {
        if dirty_scopes.iter().any(|candidate| {
            candidate != &scope && dirty_scope_contains(candidate, &scope, previous_scope_roots)
        }) {
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

fn element_contains_id(element: &RetainedRoot, id: &str) -> bool {
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

fn element_lists_are_structurally_compatible(
    previous: &[RetainedRoot],
    next: &[RetainedRoot],
) -> bool {
    previous.len() == next.len()
        && previous
            .iter()
            .zip(next)
            .all(|(previous, next)| elements_are_structurally_compatible(previous, next))
}

fn elements_are_structurally_compatible(previous: &RetainedRoot, next: &RetainedRoot) -> bool {
    previous.id == next.id
        && previous.kind == next.kind
        && previous.z_index == next.z_index
        && previous.clip == next.clip
        && element_parent_layout_inputs_are_compatible(previous, next)
        && previous.children.len() == next.children.len()
        && element_lists_are_structurally_compatible(&previous.children, &next.children)
}

fn first_element_list_mismatch(previous: &[RetainedRoot], next: &[RetainedRoot]) -> Option<String> {
    if previous.len() != next.len() {
        return Some(format!(
            "root count changed {} -> {}",
            previous.len(),
            next.len()
        ));
    }
    previous
        .iter()
        .zip(next)
        .find_map(|(previous, next)| first_element_mismatch(previous, next))
}

fn first_element_mismatch(previous: &RetainedRoot, next: &RetainedRoot) -> Option<String> {
    if previous.id != next.id {
        return Some(format!("id changed {} -> {}", previous.id, next.id));
    }
    if previous.kind != next.kind {
        return Some(format!(
            "{} kind changed {:?} -> {:?}",
            previous.id, previous.kind, next.kind
        ));
    }
    if previous.z_index != next.z_index {
        return Some(format!(
            "{} z_index changed {} -> {}",
            previous.id, previous.z_index, next.z_index
        ));
    }
    if previous.clip != next.clip {
        return Some(format!(
            "{} clip changed {} -> {}",
            previous.id, previous.clip, next.clip
        ));
    }
    if previous.clip_radius.to_bits() != next.clip_radius.to_bits() {
        return Some(format!(
            "{} clip_radius changed {} -> {}",
            previous.id, previous.clip_radius, next.clip_radius
        ));
    }
    first_parent_layout_input_mismatch(previous, next).or_else(|| {
        if previous.children.len() != next.children.len() {
            return Some(format!(
                "{} child count changed {} -> {}",
                previous.id,
                previous.children.len(),
                next.children.len()
            ));
        }
        previous
            .children
            .iter()
            .zip(&next.children)
            .find_map(|(previous, next)| first_element_mismatch(previous, next))
    })
}

fn first_parent_layout_input_mismatch(
    previous: &RetainedRoot,
    next: &RetainedRoot,
) -> Option<String> {
    if previous.width != next.width {
        return Some(format!(
            "{} width changed {:?} -> {:?}",
            previous.id, previous.width, next.width
        ));
    }
    if previous.height != next.height {
        return Some(format!(
            "{} height changed {:?} -> {:?}",
            previous.id, previous.height, next.height
        ));
    }
    if !same_edge_insets(previous.margin, next.margin) {
        return Some(format!(
            "{} margin changed {:?} -> {:?}",
            previous.id, previous.margin, next.margin
        ));
    }
    if !same_f32(previous.min_width, next.min_width) {
        return Some(format!(
            "{} min_width changed {} -> {}",
            previous.id, previous.min_width, next.min_width
        ));
    }
    if !same_f32(previous.max_layout_width, next.max_layout_width) {
        return Some(format!(
            "{} max_layout_width changed {} -> {}",
            previous.id, previous.max_layout_width, next.max_layout_width
        ));
    }
    if !same_f32(previous.min_height, next.min_height) {
        return Some(format!(
            "{} min_height changed {} -> {}",
            previous.id, previous.min_height, next.min_height
        ));
    }
    if !same_f32(previous.max_height, next.max_height) {
        return Some(format!(
            "{} max_height changed {} -> {}",
            previous.id, previous.max_height, next.max_height
        ));
    }
    if !same_f32(previous.grow, next.grow) {
        return Some(format!(
            "{} grow changed {} -> {}",
            previous.id, previous.grow, next.grow
        ));
    }
    None
}

fn element_parent_layout_inputs_are_compatible(
    previous: &RetainedRoot,
    next: &RetainedRoot,
) -> bool {
    same_size(previous.width, next.width)
        && same_size(previous.height, next.height)
        && same_edge_insets(previous.margin, next.margin)
        && same_f32(previous.min_width, next.min_width)
        && same_f32(previous.max_layout_width, next.max_layout_width)
        && same_f32(previous.min_height, next.min_height)
        && same_f32(previous.max_height, next.max_height)
        && same_f32(previous.grow, next.grow)
}

fn same_size(previous: crate::Size, next: crate::Size) -> bool {
    match (previous, next) {
        (crate::Size::Fixed(previous), crate::Size::Fixed(next)) => same_f32(previous, next),
        (crate::Size::WrapContent, crate::Size::WrapContent) => true,
        (crate::Size::Fill, crate::Size::Fill) => true,
        _ => false,
    }
}

fn same_edge_insets(previous: crate::EdgeInsets, next: crate::EdgeInsets) -> bool {
    same_f32(previous.left, next.left)
        && same_f32(previous.top, next.top)
        && same_f32(previous.right, next.right)
        && same_f32(previous.bottom, next.bottom)
}

fn same_f32(previous: f32, next: f32) -> bool {
    previous.to_bits() == next.to_bits()
}

pub(crate) fn scope_profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SKY_NEO_SCOPE_PROFILE").is_some())
}

#[cfg(test)]
mod tests {
    use crate::{Element, ElementKind};

    use super::{normalize_dirty_scopes_with_roots, RetainedRoot, ScopeRoots, ScopeSet};

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
            &scopes(&["page.panel", "page.panel_extra.child", "page.panel.child"]),
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
        roots.insert(
            "page.interactions.scroll".to_string(),
            RetainedRoot::from_elements(&[scroll]),
        );
        let cards = Element::new(ElementKind::Row, "page.interactions.cards");
        roots.insert(
            "page.interactions.cards".to_string(),
            RetainedRoot::from_elements(&[cards]),
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
