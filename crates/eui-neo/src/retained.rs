//! Retained-scope bookkeeping for partial UI composition.
//!
//! Signals decide which scopes are dirty. Live scopes opt into rebuilding every
//! scoped compose. The runtime owns the previous frame's retained roots and
//! merges these two invalidation sources before `Ui` decides which scopes can
//! be reused.

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::time::Instant;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{Element, ElementKind, LayoutRect};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScopeId(String);

impl ScopeId {
    pub(crate) fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for ScopeId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for ScopeId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Deref for ScopeId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Display for ScopeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for ScopeId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for ScopeId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetainedComposeReason {
    RetainedReuseUnavailable,
    DirtyScope,
    DirtyDescendant,
    DirtyAncestor,
    MissingPreviousRoots,
    MissingPreviousElement,
    CleanReuse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedComposeEvent {
    pub id: ScopeId,
    pub action: RetainedComposeAction,
    pub reason: RetainedComposeReason,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CallbackTransferStats {
    pub click: usize,
    pub press: usize,
    pub context_menu: usize,
    pub focus_changed: usize,
    pub text_input: usize,
    pub scroll: usize,
    pub drag: usize,
    pub layer_dismiss: usize,
    pub timer: usize,
}

impl CallbackTransferStats {
    pub fn total(self) -> usize {
        self.click
            + self.press
            + self.context_menu
            + self.focus_changed
            + self.text_input
            + self.scroll
            + self.drag
            + self.layer_dismiss
            + self.timer
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScopeComposeRecord {
    pub id: ScopeId,
    pub action: RetainedComposeAction,
    pub reason: RetainedComposeReason,
    pub build_ms: f32,
    pub self_build_ms: f32,
    pub previous_roots: usize,
    pub current_roots: usize,
    pub element_count: usize,
    pub callback_transfers: CallbackTransferStats,
}

#[derive(Debug, Default)]
#[cfg(feature = "profile")]
pub(crate) struct RetainedTiming {
    enabled: bool,
    scope_timing_stack: Vec<f32>,
}

#[cfg(feature = "profile")]
impl RetainedTiming {
    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub(crate) fn metadata<T>(&mut self, op: impl FnOnce() -> T) -> T {
        op()
    }

    pub(crate) fn lookup<T>(&mut self, op: impl FnOnce() -> T) -> T {
        op()
    }

    pub(crate) fn begin_scope(&mut self) -> Option<Instant> {
        if self.enabled {
            self.scope_timing_stack.push(0.0);
        }
        self.enabled.then(Instant::now)
    }

    pub(crate) fn finish_scope(&mut self, start: Option<Instant>) -> (f32, f32) {
        let build_ms = elapsed_ms(start);
        let child_ms = if start.is_some() {
            self.scope_timing_stack
                .pop()
                .expect("scope timing stack should contain active scope")
        } else {
            0.0
        };
        if start.is_some() {
            if let Some(parent_child_ms) = self.scope_timing_stack.last_mut() {
                *parent_child_ms += build_ms;
            }
        }
        (build_ms, (build_ms - child_ms).max(0.0))
    }
}

#[derive(Debug, Default)]
#[cfg(not(feature = "profile"))]
pub(crate) struct RetainedTiming;

#[cfg(not(feature = "profile"))]
impl RetainedTiming {
    pub(crate) fn set_enabled(&mut self, _enabled: bool) {}

    pub(crate) fn metadata<T>(&mut self, op: impl FnOnce() -> T) -> T {
        op()
    }

    pub(crate) fn lookup<T>(&mut self, op: impl FnOnce() -> T) -> T {
        op()
    }

    pub(crate) fn begin_scope(&mut self) -> Option<Instant> {
        None
    }

    pub(crate) fn finish_scope(&mut self, _start: Option<Instant>) -> (f32, f32) {
        (0.0, 0.0)
    }
}

#[cfg(feature = "profile")]
fn elapsed_ms(start: Option<Instant>) -> f32 {
    start
        .map(|start| start.elapsed().as_secs_f32() * 1000.0)
        .unwrap_or(0.0)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutMode {
    Partial,
    Full(FullLayoutReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FullLayoutReason {
    RetainedReuseUnavailable,
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

pub(crate) fn dirty_root_ids_for_scopes(
    dirty_scopes: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
) -> FxHashSet<String> {
    dirty_scopes
        .iter()
        .filter_map(|dirty| previous_scope_roots.get(dirty))
        .flat_map(|roots| roots.iter().map(|root| root.id.clone()))
        .collect()
}

pub(crate) fn retained_scope_contains_dirty_root(
    dirty_root_ids: &FxHashSet<String>,
    previous_scope_roots: &ScopeRoots,
    id: &str,
) -> bool {
    previous_scope_roots
        .get(id)
        .is_some_and(|elements| element_tree_contains_any(elements, dirty_root_ids))
}

pub(crate) fn previous_elements_for_scope(
    previous_roots: &[Element],
    previous_scope_roots: &ScopeRoots,
    id: &str,
) -> Option<Vec<Element>> {
    let roots = previous_scope_roots.get(id)?;
    let mut elements = Vec::with_capacity(roots.len());
    for root in roots {
        elements.push(find_element(previous_roots, &root.id)?.clone());
    }
    Some(elements)
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

fn element_tree_contains_any(elements: &[RetainedRoot], ids: &FxHashSet<String>) -> bool {
    elements.iter().any(|element| {
        ids.contains(&element.id) || element_tree_contains_any(&element.children, ids)
    })
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

#[cfg(test)]
mod tests {
    use crate::{Element, ElementKind};

    use super::{
        normalize_dirty_scopes_with_roots, previous_elements_for_scope, RetainedRoot, ScopeId,
        ScopeRoots, ScopeSet,
    };

    fn scopes(ids: &[&str]) -> ScopeSet {
        ids.iter().map(|id| ScopeId::new(*id)).collect()
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
            ScopeId::new("page.interactions.scroll"),
            RetainedRoot::from_elements(&[scroll]),
        );
        let cards = Element::new(ElementKind::Row, "page.interactions.cards");
        roots.insert(
            ScopeId::new("page.interactions.cards"),
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

    #[test]
    fn previous_elements_for_scope_clones_roots_from_committed_tree() {
        let mut retained_a = Element::new(ElementKind::Row, "page.scope.a");
        retained_a
            .children
            .push(Element::new(ElementKind::Text, "page.scope.a.label"));
        let retained_b = Element::new(ElementKind::Rect, "page.scope.b");
        let mut unrelated = Element::new(ElementKind::Column, "page.unrelated");
        unrelated.children.push(retained_b.clone());

        let previous_roots = vec![retained_a.clone(), unrelated];
        let mut scope_roots = ScopeRoots::default();
        scope_roots.insert(
            ScopeId::new("page.scope"),
            RetainedRoot::from_elements(&[retained_a, retained_b]),
        );

        let elements = previous_elements_for_scope(&previous_roots, &scope_roots, "page.scope")
            .expect("scope roots should resolve to committed elements");

        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].id, "page.scope.a");
        assert_eq!(elements[0].children[0].id, "page.scope.a.label");
        assert_eq!(elements[1].id, "page.scope.b");
    }

    #[test]
    fn previous_elements_for_scope_fails_when_a_root_is_missing() {
        let previous_roots = vec![Element::new(ElementKind::Row, "page.scope.a")];
        let mut scope_roots = ScopeRoots::default();
        scope_roots.insert(
            ScopeId::new("page.scope"),
            RetainedRoot::from_elements(&[
                Element::new(ElementKind::Row, "page.scope.a"),
                Element::new(ElementKind::Rect, "page.scope.missing"),
            ]),
        );

        assert!(previous_elements_for_scope(&previous_roots, &scope_roots, "page.scope").is_none());
    }

    fn sorted(scopes: &ScopeSet) -> Vec<String> {
        let mut scopes: Vec<_> = scopes
            .iter()
            .map(|scope| scope.as_str().to_string())
            .collect();
        scopes.sort();
        scopes
    }
}
