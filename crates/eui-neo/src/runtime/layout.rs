use crate::layout::{layout_element_in_frame_with_text_system, layout_roots_with_text_system};
use crate::retained::{FullLayoutReason, LayoutMode, ScopeRoots, ScopeSet};
#[cfg(feature = "taffy-layout")]
use crate::taffy_layout::{
    layout_element_in_frame_with_taffy_text_system, layout_root_with_taffy_text_system,
    TaffyLayoutError,
};
use crate::text_measure::TextSystem;
use crate::{Element, LayoutRect, Screen};

use super::debug::neo_structure_trace_enabled;
use super::reconcile::{retained_layout_dirty_scopes, retained_layout_reuse_plan};
use super::tree::find_element;

pub(super) struct RuntimeLayoutInput {
    pub screen: Screen,
    pub can_reuse_scopes: bool,
    pub normalized_dirty_ids: ScopeSet,
    pub blocker: Option<FullLayoutReason>,
}

pub(super) struct RuntimeLayoutPlan<'a> {
    pub blocker: Option<FullLayoutReason>,
    pub can_reuse_scopes: bool,
    pub normalized_dirty_ids: &'a ScopeSet,
    pub previous_scope_roots: &'a ScopeRoots,
    pub previous_roots: &'a [Element],
    pub screen: Screen,
}

pub(super) struct RuntimeLayoutResult {
    pub used_partial_layout: bool,
    pub mode: LayoutMode,
}

pub(super) trait RuntimeLayoutBackend {
    fn layout_roots(
        &mut self,
        roots: &mut [Element],
        screen: Screen,
        text_system: &mut dyn TextSystem,
    );

    fn layout_element_in_frame(
        &mut self,
        element: &mut Element,
        frame: LayoutRect,
        text_system: &mut dyn TextSystem,
    ) -> bool;
}

#[derive(Debug, Default)]
pub(super) struct BuiltInLayoutBackend;

impl RuntimeLayoutBackend for BuiltInLayoutBackend {
    fn layout_roots(
        &mut self,
        roots: &mut [Element],
        screen: Screen,
        text_system: &mut dyn TextSystem,
    ) {
        layout_roots_with_text_system(roots, screen.width, screen.height, text_system);
    }

    fn layout_element_in_frame(
        &mut self,
        element: &mut Element,
        frame: LayoutRect,
        text_system: &mut dyn TextSystem,
    ) -> bool {
        layout_element_in_frame_with_text_system(element, frame, text_system)
    }
}

#[cfg(feature = "taffy-layout")]
#[derive(Debug, Default)]
pub(super) struct ExperimentalTaffyLayoutBackend {
    built_in: BuiltInLayoutBackend,
    pub(super) taffy_roots: usize,
    pub(super) fallback_roots: usize,
    pub(super) taffy_partial: usize,
    pub(super) fallback_partial: usize,
}

#[cfg(feature = "taffy-layout")]
impl RuntimeLayoutBackend for ExperimentalTaffyLayoutBackend {
    fn layout_roots(
        &mut self,
        roots: &mut [Element],
        screen: Screen,
        text_system: &mut dyn TextSystem,
    ) {
        for root in roots {
            match layout_root_with_taffy_text_system(root, screen.width, screen.height, text_system)
            {
                Ok(()) => self.taffy_roots += 1,
                Err(err) => {
                    trace_taffy_fallback("full", root.id.as_str(), &err);
                    self.fallback_roots += 1;
                    self.built_in
                        .layout_roots(std::slice::from_mut(root), screen, text_system);
                }
            }
        }
    }

    fn layout_element_in_frame(
        &mut self,
        element: &mut Element,
        frame: LayoutRect,
        text_system: &mut dyn TextSystem,
    ) -> bool {
        match layout_element_in_frame_with_taffy_text_system(element, frame, text_system) {
            Ok(()) => {
                self.taffy_partial += 1;
                true
            }
            Err(err) => {
                trace_taffy_fallback("partial", element.id.as_str(), &err);
                self.fallback_partial += 1;
                self.built_in
                    .layout_element_in_frame(element, frame, text_system)
            }
        }
    }
}

#[cfg(feature = "taffy-layout")]
fn trace_taffy_fallback(pass: &str, element_id: &str, err: &TaffyLayoutError) {
    if std::env::var_os("EUI_NEO_TAFFY_TRACE").is_some() {
        eprintln!("[eui-neo taffy] {pass} layout fell back for {element_id}: {err:?}");
    }
}

pub(super) fn prepare_runtime_layout_input(
    screen: Screen,
    can_reuse_scopes: bool,
    compose_dirty_scopes: &ScopeSet,
    layout_dirty_scopes: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
    next_scope_roots: &ScopeRoots,
) -> RuntimeLayoutInput {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.layout_input");
    let normalized_dirty_ids = retained_layout_dirty_scopes(
        compose_dirty_scopes,
        layout_dirty_scopes,
        previous_scope_roots,
    );
    let plan = retained_layout_reuse_plan(
        can_reuse_scopes,
        &normalized_dirty_ids,
        previous_scope_roots,
        next_scope_roots,
        neo_structure_trace_enabled(),
    );
    for report in plan.structural_reports {
        eprintln!("[eui-neo structure] {report}");
    }
    RuntimeLayoutInput {
        screen,
        can_reuse_scopes,
        normalized_dirty_ids,
        blocker: plan.blocker,
    }
}

pub(super) fn execute_runtime_layout(
    roots: &mut Vec<Element>,
    plan: RuntimeLayoutPlan<'_>,
    text_system: &mut dyn TextSystem,
) -> RuntimeLayoutResult {
    #[cfg(feature = "taffy-layout")]
    if std::env::var_os("EUI_NEO_TAFFY_LAYOUT").is_some() {
        let mut backend = ExperimentalTaffyLayoutBackend::default();
        return execute_runtime_layout_with_backend(roots, plan, text_system, &mut backend);
    }

    let mut backend = BuiltInLayoutBackend;
    execute_runtime_layout_with_backend(roots, plan, text_system, &mut backend)
}

pub(super) fn execute_runtime_layout_with_backend(
    roots: &mut Vec<Element>,
    plan: RuntimeLayoutPlan<'_>,
    text_system: &mut dyn TextSystem,
    backend: &mut dyn RuntimeLayoutBackend,
) -> RuntimeLayoutResult {
    let partial_layout = plan.blocker.is_none();
    let mut used_partial_layout = false;
    if partial_layout && plan.can_reuse_scopes {
        copy_previous_frames(roots, plan.previous_roots);
        used_partial_layout = layout_dirty_ids_with_text_system(
            roots,
            plan.normalized_dirty_ids,
            plan.previous_scope_roots,
            text_system,
            backend,
        );
    }

    let mode = if used_partial_layout {
        LayoutMode::Partial
    } else if partial_layout {
        LayoutMode::Full(FullLayoutReason::DirtyRetainedLayoutFailed)
    } else {
        LayoutMode::Full(
            plan.blocker
                .expect("full layout blocker should be known here"),
        )
    };

    if !used_partial_layout {
        backend.layout_roots(roots, plan.screen, text_system);
    }

    RuntimeLayoutResult {
        used_partial_layout,
        mode,
    }
}

fn layout_dirty_ids_with_text_system(
    roots: &mut [Element],
    dirty_ids: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
    text_system: &mut dyn TextSystem,
    backend: &mut dyn RuntimeLayoutBackend,
) -> bool {
    for scope in dirty_ids {
        let Some(previous_roots) = previous_scope_roots.get(scope) else {
            return false;
        };
        for previous in previous_roots {
            let Some(current) = find_element_mut(roots, previous.id.as_str()) else {
                return false;
            };
            if !backend.layout_element_in_frame(current, previous.frame, text_system) {
                return false;
            }
        }
    }
    true
}

fn copy_previous_frames(elements: &mut [Element], previous_roots: &[Element]) {
    for element in elements {
        if let Some(previous) = find_element_in_slice(previous_roots, &element.id) {
            element.frame = previous.frame;
        }
        copy_previous_frames(&mut element.children, previous_roots);
    }
}

fn find_element_in_slice<'a>(elements: &'a [Element], id: &str) -> Option<&'a Element> {
    elements
        .iter()
        .find_map(|element| find_element(element, id))
}

fn find_element_mut<'a>(elements: &'a mut [Element], id: &str) -> Option<&'a mut Element> {
    for element in elements {
        if element.id == id {
            return Some(element);
        }
        if let Some(found) = find_element_mut(&mut element.children, id) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::retained::{RetainedRoot, ScopeId};
    use crate::text_measure::DefaultTextSystem;
    use crate::{ElementKind, LayoutRect, Size};

    use super::*;

    #[test]
    fn runtime_layout_uses_partial_layout_for_clean_retained_shape() {
        let mut previous = fixed_rect("page.scope.root", 40.0, 20.0);
        previous.frame = LayoutRect::new(8.0, 12.0, 40.0, 20.0);
        let previous_roots = vec![previous.clone()];
        let mut previous_scope_roots = ScopeRoots::default();
        previous_scope_roots.insert(
            ScopeId::new("page.scope"),
            RetainedRoot::from_elements(&previous_roots),
        );
        let dirty_ids = scopes(&["page.scope"]);
        let mut roots = vec![fixed_rect("page.scope.root", 40.0, 20.0)];
        let mut text_system = DefaultTextSystem::default();

        let result = execute_runtime_layout(
            &mut roots,
            RuntimeLayoutPlan {
                blocker: None,
                can_reuse_scopes: true,
                normalized_dirty_ids: &dirty_ids,
                previous_scope_roots: &previous_scope_roots,
                previous_roots: &previous_roots,
                screen: Screen::new(320.0, 200.0),
            },
            &mut text_system,
        );

        assert!(result.used_partial_layout);
        assert_eq!(result.mode, LayoutMode::Partial);
        assert_eq!(roots[0].frame, previous.frame);
    }

    #[test]
    fn runtime_layout_degrades_when_dirty_scope_root_is_missing() {
        let previous_roots = Vec::new();
        let previous_scope_roots = ScopeRoots::default();
        let dirty_ids = scopes(&["page.missing"]);
        let mut roots = vec![fixed_rect("page.scope.root", 40.0, 20.0)];
        let mut text_system = DefaultTextSystem::default();

        let result = execute_runtime_layout(
            &mut roots,
            RuntimeLayoutPlan {
                blocker: None,
                can_reuse_scopes: true,
                normalized_dirty_ids: &dirty_ids,
                previous_scope_roots: &previous_scope_roots,
                previous_roots: &previous_roots,
                screen: Screen::new(320.0, 200.0),
            },
            &mut text_system,
        );

        assert!(!result.used_partial_layout);
        assert_eq!(
            result.mode,
            LayoutMode::Full(FullLayoutReason::DirtyRetainedLayoutFailed)
        );
        assert_eq!(roots[0].frame, LayoutRect::new(0.0, 0.0, 40.0, 20.0));
    }

    #[test]
    fn runtime_layout_routes_through_backend_facade() {
        let mut previous = fixed_rect("page.scope.root", 40.0, 20.0);
        previous.frame = LayoutRect::new(8.0, 12.0, 40.0, 20.0);
        let previous_roots = vec![previous.clone()];
        let mut previous_scope_roots = ScopeRoots::default();
        previous_scope_roots.insert(
            ScopeId::new("page.scope"),
            RetainedRoot::from_elements(&previous_roots),
        );
        let dirty_ids = scopes(&["page.scope"]);
        let mut roots = vec![fixed_rect("page.scope.root", 40.0, 20.0)];
        let mut text_system = DefaultTextSystem::default();
        let mut backend = RecordingLayoutBackend::default();

        let partial = execute_runtime_layout_with_backend(
            &mut roots,
            RuntimeLayoutPlan {
                blocker: None,
                can_reuse_scopes: true,
                normalized_dirty_ids: &dirty_ids,
                previous_scope_roots: &previous_scope_roots,
                previous_roots: &previous_roots,
                screen: Screen::new(320.0, 200.0),
            },
            &mut text_system,
            &mut backend,
        );

        assert!(partial.used_partial_layout);
        assert_eq!(partial.mode, LayoutMode::Partial);
        assert_eq!(backend.partial_elements, 1);
        assert_eq!(backend.full_roots, 0);

        let mut roots = vec![fixed_rect("page.scope.root", 40.0, 20.0)];
        let mut backend = RecordingLayoutBackend::default();
        let full = execute_runtime_layout_with_backend(
            &mut roots,
            RuntimeLayoutPlan {
                blocker: Some(FullLayoutReason::RetainedReuseUnavailable),
                can_reuse_scopes: true,
                normalized_dirty_ids: &dirty_ids,
                previous_scope_roots: &previous_scope_roots,
                previous_roots: &previous_roots,
                screen: Screen::new(320.0, 200.0),
            },
            &mut text_system,
            &mut backend,
        );

        assert!(!full.used_partial_layout);
        assert_eq!(
            full.mode,
            LayoutMode::Full(FullLayoutReason::RetainedReuseUnavailable)
        );
        assert_eq!(backend.partial_elements, 0);
        assert_eq!(backend.full_roots, 1);
    }

    #[cfg(feature = "taffy-layout")]
    #[test]
    fn taffy_backend_falls_back_for_scroll_and_popover_shapes() {
        use crate::Ui;

        let mut ui = Ui::new("test");
        ui.scroll_y("list")
            .size(120.0, 80.0)
            .content_height(200.0)
            .offset(24.0)
            .padding(8.0)
            .gap(6.0)
            .content(|ui| {
                ui.rect("row.a").size(Size::fill(), 30.0).build();
                ui.rect("row.b").size(Size::fill(), 30.0).build();
            });
        ui.popover("menu")
            .fallback_anchor(LayoutRect::new(10.0, 20.0, 40.0, 18.0))
            .size(90.0, 48.0)
            .content(|ui| {
                ui.rect("item").size(Size::fill(), 20.0).build();
            });

        let roots = ui.into_roots();
        let mut expected = roots.clone();
        let mut actual = roots;
        let empty_scopes = ScopeRoots::default();
        let dirty = ScopeSet::default();
        let previous_roots = Vec::new();
        let screen = Screen::new(240.0, 180.0);
        let mut text_system = DefaultTextSystem::default();
        let mut built_in = BuiltInLayoutBackend;
        execute_runtime_layout_with_backend(
            &mut expected,
            RuntimeLayoutPlan {
                blocker: Some(FullLayoutReason::RetainedReuseUnavailable),
                can_reuse_scopes: false,
                normalized_dirty_ids: &dirty,
                previous_scope_roots: &empty_scopes,
                previous_roots: &previous_roots,
                screen,
            },
            &mut text_system,
            &mut built_in,
        );

        let mut backend = ExperimentalTaffyLayoutBackend::default();
        execute_runtime_layout_with_backend(
            &mut actual,
            RuntimeLayoutPlan {
                blocker: Some(FullLayoutReason::RetainedReuseUnavailable),
                can_reuse_scopes: false,
                normalized_dirty_ids: &dirty,
                previous_scope_roots: &empty_scopes,
                previous_roots: &previous_roots,
                screen,
            },
            &mut text_system,
            &mut backend,
        );

        assert_eq!(backend.taffy_roots, 0);
        assert_eq!(backend.fallback_roots, actual.len());
        assert_element_frames_close(&actual, &expected);
    }

    #[test]
    fn prepare_runtime_layout_input_normalizes_dirty_scopes_and_reports_blocker() {
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
            RetainedRoot::from_elements(&[sibling_root.clone()]),
        );

        let mut changed_parent_root = Element::new(ElementKind::Column, "page.parent.root");
        changed_parent_root
            .children
            .push(Element::new(ElementKind::Row, "page.child.root"));
        let mut next_scope_roots = ScopeRoots::default();
        next_scope_roots.insert(
            ScopeId::new("page.parent"),
            RetainedRoot::from_elements(&[changed_parent_root]),
        );
        next_scope_roots.insert(
            ScopeId::new("page.child"),
            RetainedRoot::from_elements(&[Element::new(ElementKind::Row, "page.child.root")]),
        );
        next_scope_roots.insert(
            ScopeId::new("page.sibling"),
            RetainedRoot::from_elements(&[sibling_root]),
        );

        let input = prepare_runtime_layout_input(
            Screen::new(320.0, 200.0),
            true,
            &scopes(&["page.child"]),
            &scopes(&["page.parent", "page.sibling"]),
            &previous_scope_roots,
            &next_scope_roots,
        );

        assert_eq!(input.screen, Screen::new(320.0, 200.0));
        assert!(input.can_reuse_scopes);
        assert_eq!(
            sorted_scopes(&input.normalized_dirty_ids),
            vec!["page.parent".to_string(), "page.sibling".to_string()]
        );
        assert_eq!(
            input.blocker,
            Some(FullLayoutReason::StructureChanged {
                ids: vec![ScopeId::new("page.parent")]
            })
        );
    }

    fn fixed_rect(id: &str, width: f32, height: f32) -> Element {
        let mut element = Element::new(ElementKind::Rect, id);
        element.width = Size::Fixed(width);
        element.height = Size::Fixed(height);
        element
    }

    fn scopes(ids: &[&str]) -> ScopeSet {
        ids.iter().map(|id| ScopeId::new(*id)).collect()
    }

    fn sorted_scopes(scopes: &ScopeSet) -> Vec<String> {
        let mut scopes = scopes
            .iter()
            .map(|scope| scope.as_str().to_string())
            .collect::<Vec<_>>();
        scopes.sort();
        scopes
    }

    #[cfg(feature = "taffy-layout")]
    fn assert_element_frames_close(actual: &[Element], expected: &[Element]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert_rect_close(actual.frame, expected.frame, actual.id.as_str());
            assert_element_frames_close(&actual.children, &expected.children);
        }
    }

    #[cfg(feature = "taffy-layout")]
    fn assert_rect_close(actual: LayoutRect, expected: LayoutRect, id: &str) {
        assert_close(actual.x, expected.x, id, "x");
        assert_close(actual.y, expected.y, id, "y");
        assert_close(actual.width, expected.width, id, "width");
        assert_close(actual.height, expected.height, id, "height");
    }

    #[cfg(feature = "taffy-layout")]
    fn assert_close(actual: f32, expected: f32, id: &str, field: &str) {
        assert!(
            (actual - expected).abs() <= 0.01,
            "{id}.{field}: expected {expected}, got {actual}"
        );
    }

    #[derive(Default)]
    struct RecordingLayoutBackend {
        partial_elements: usize,
        full_roots: usize,
    }

    impl RuntimeLayoutBackend for RecordingLayoutBackend {
        fn layout_roots(
            &mut self,
            roots: &mut [Element],
            screen: Screen,
            _text_system: &mut dyn TextSystem,
        ) {
            self.full_roots += roots.len();
            for root in roots {
                root.frame = LayoutRect::new(0.0, 0.0, screen.width, screen.height);
            }
        }

        fn layout_element_in_frame(
            &mut self,
            element: &mut Element,
            frame: LayoutRect,
            _text_system: &mut dyn TextSystem,
        ) -> bool {
            self.partial_elements += 1;
            element.frame = frame;
            true
        }
    }
}
