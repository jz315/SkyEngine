use super::animation::cleanup_stale_animation_state;
use super::focus::sync_layer_blocked_focus;
use super::interaction::cleanup_stale_input_state;
use super::reconcile::committed_element_ids;
use super::timing::{cleanup_stale_timer_state, sync_clock_period_ticks};
use super::*;

impl Runtime {
    pub fn page_id(&self) -> &str {
        self.tree.page_id.as_str()
    }

    pub fn screen(&self) -> Screen {
        self.tree.screen
    }

    pub(crate) fn diagnostic_roots(&self) -> &[Element] {
        &self.tree.roots
    }

    pub(crate) fn element_count_hint(&self) -> usize {
        self.tree.structure.len().max(self.tree.roots.len())
    }

    pub(crate) fn diagnostic_find(&self, id: &str) -> Option<&Element> {
        let id = self.resolve_node_id(id);
        self.find_node(&id)
    }

    pub(super) fn resolve_id_ref<'a>(&self, id: &'a str) -> Cow<'a, str> {
        let page_id = self.tree.page_id.as_str();
        if id.is_empty() || page_id.is_empty() {
            return Cow::Borrowed(id);
        }
        if is_resolved_id(id, page_id) {
            Cow::Borrowed(id)
        } else {
            let mut resolved = String::with_capacity(page_id.len() + 1 + id.len());
            resolved.push_str(page_id);
            resolved.push('.');
            resolved.push_str(id);
            Cow::Owned(resolved)
        }
    }

    pub(crate) fn resolve_node_id(&self, id: &str) -> NodeId {
        NodeId::new(self.resolve_id_ref(id).as_ref())
    }

    pub(crate) fn find_node(&self, id: &NodeId) -> Option<&Element> {
        self.find_resolved(id.as_str())
    }

    pub(super) fn find_resolved(&self, id: &str) -> Option<&Element> {
        self.tree
            .roots
            .iter()
            .find_map(|root| find_element(root, id))
    }
}

pub(super) fn is_resolved_id(id: &str, page_id: &str) -> bool {
    id.len() > page_id.len()
        && id.as_bytes().get(page_id.len()) == Some(&b'.')
        && id.as_bytes().starts_with(page_id.as_bytes())
}

pub(super) fn sorted_z_indices(elements: &[Element]) -> SmallVec<[usize; 16]> {
    let mut order: SmallVec<[usize; 16]> = (0..elements.len()).collect();
    order.sort_by_key(|&index| (elements[index].z_index, index));
    order
}

pub(super) fn z_order_is_stable(elements: &[Element]) -> bool {
    elements
        .windows(2)
        .all(|pair| pair[0].z_index <= pair[1].z_index)
}

pub(super) fn collect_next_structure(
    runtime: &mut Runtime,
    screen: Screen,
    roots: &[Element],
) -> Vec<ElementSnapshot> {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.collect_structure");
    let next_structure = collect_structure(roots, runtime.tree.structure.len());
    let layout_structure_changed = runtime.tree.screen != screen
        || !layout_structures_match(&next_structure, &runtime.tree.structure);
    let visual_structure_changed =
        !visual_structures_match(&next_structure, &runtime.tree.structure);
    if layout_structure_changed {
        request_tree_structure_invalidation(runtime, "layout_structure", DirtyFlags::LAYOUT);
    } else if visual_structure_changed {
        request_tree_structure_invalidation(runtime, "visual_structure", DirtyFlags::VISUAL);
    }
    next_structure
}

fn request_tree_structure_invalidation(
    runtime: &mut Runtime,
    source: &'static str,
    flags: DirtyFlags,
) {
    runtime.request_invalidation(Invalidation::runtime(
        runtime.runtime_invalidation_target(),
        source,
        flags,
    ));
}

pub(super) struct ComposedTreeCommit {
    pub(super) screen: Screen,
    pub(super) roots: Vec<Element>,
    pub(super) scope_roots: ScopeRoots,
    pub(super) live_scopes: ScopeSet,
    pub(super) clock_ids: ScopeSet,
    pub(super) clock_periods: Option<ClockPeriodMap>,
    pub(super) scope_layer_roots: ScopeLayerRoots,
    pub(super) scope_layer_intents: ScopeLayerIntents,
    pub(super) retained_stats: RetainedComposeStats,
    pub(super) structure: Vec<ElementSnapshot>,
}

pub(super) fn commit_composed_tree(runtime: &mut Runtime, commit: ComposedTreeCommit) {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.commit_tree");
    runtime.tree.structure = commit.structure;
    runtime.tree.screen = commit.screen;
    runtime.tree.roots = commit.roots;
    runtime.tree.scope_roots = commit.scope_roots;
    runtime.tree.live_ids = commit.live_scopes;
    runtime.tree.clock_ids = commit.clock_ids;
    runtime.tree.clock_periods = commit.clock_periods;
    runtime.tree.scope_layer_roots = commit.scope_layer_roots;
    runtime.tree.scope_layer_intents = commit.scope_layer_intents;
    sync_clock_period_ticks(
        runtime.timing.clock_seconds,
        runtime.tree.clock_periods.as_ref(),
        &mut runtime.tree.clock_period_ticks,
    );
    runtime.tree.retained_stats = commit.retained_stats;
    runtime.tree.frame_index = runtime.tree.frame_index.saturating_add(1);
}

pub(super) fn cleanup_stale_committed_state(runtime: &mut Runtime) {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.cleanup_stale_committed_state");
    let existing_ids = committed_element_ids(&runtime.tree.roots);
    let mut changed = cleanup_stale_input_state(runtime, &existing_ids);
    changed |= sync_layer_blocked_focus(runtime, &existing_ids);
    changed |= cleanup_stale_animation_state(runtime, &existing_ids);
    changed |= cleanup_stale_timer_state(runtime, &existing_ids);
    if changed {
        request_stale_state_cleanup_invalidation(runtime);
    }
}

fn request_stale_state_cleanup_invalidation(runtime: &mut Runtime) {
    runtime.request_invalidation(Invalidation::runtime(
        runtime.runtime_invalidation_target(),
        "stale_state_cleanup",
        DirtyFlags::DRAW,
    ));
}

pub(super) fn collect_structure(roots: &[Element], previous_len: usize) -> Vec<ElementSnapshot> {
    let mut snapshots = Vec::with_capacity(previous_len);
    for root in roots {
        collect_element_structure(root, &mut snapshots);
    }
    snapshots
}

pub(super) fn collect_element_structure(element: &Element, snapshots: &mut Vec<ElementSnapshot>) {
    snapshots.push(ElementSnapshot {
        id: NodeId::new(&element.id),
        kind: element.kind,
        z_index: element.z_index,
        clip: element.clip,
        clip_radius_bits: element.clip_radius.to_bits(),
        child_count: element.children.len(),
        layout_signature: element_layout_signature(element),
        visual_signature: element_visual_signature(element),
    });
    for child in &element.children {
        collect_element_structure(child, snapshots);
    }
}

pub(super) fn layout_structures_match(
    next: &[ElementSnapshot],
    previous: &[ElementSnapshot],
) -> bool {
    next.len() == previous.len()
        && next.iter().zip(previous).all(|(next, previous)| {
            next.id == previous.id
                && next.kind == previous.kind
                && next.child_count == previous.child_count
                && next.layout_signature == previous.layout_signature
        })
}

pub(super) fn visual_structures_match(
    next: &[ElementSnapshot],
    previous: &[ElementSnapshot],
) -> bool {
    next.len() == previous.len()
        && next.iter().zip(previous).all(|(next, previous)| {
            next.id == previous.id
                && next.kind == previous.kind
                && next.z_index == previous.z_index
                && next.clip == previous.clip
                && next.clip_radius_bits == previous.clip_radius_bits
                && next.child_count == previous.child_count
                && next.visual_signature == previous.visual_signature
        })
}

pub(super) fn find_element<'a>(element: &'a Element, id: &str) -> Option<&'a Element> {
    if element.id == id {
        return Some(element);
    }
    element
        .children
        .iter()
        .find_map(|child| find_element(child, id))
}

pub(super) fn element_layout_signature(element: &Element) -> u64 {
    let mut hasher = FxHasher::default();
    element.layout_position_affects_structure.hash(&mut hasher);
    if element.layout_position_affects_structure {
        element.has_x.hash(&mut hasher);
        element.has_y.hash(&mut hasher);
        hash_f32(element.x, &mut hasher);
        hash_f32(element.y, &mut hasher);
    }
    hash_size(element.width, &mut hasher);
    hash_size(element.height, &mut hasher);
    hash_edge_insets(element.margin, &mut hasher);
    hash_edge_insets(element.padding, &mut hasher);
    hash_f32(element.min_width, &mut hasher);
    hash_f32(element.max_layout_width, &mut hasher);
    hash_f32(element.min_height, &mut hasher);
    hash_f32(element.max_height, &mut hasher);
    hash_f32(element.grow, &mut hasher);
    hash_f32(element.spacing, &mut hasher);
    element.main_align.hash(&mut hasher);
    element.cross_align.hash(&mut hasher);

    match element.kind {
        ElementKind::Row | ElementKind::Column | ElementKind::Stack | ElementKind::Rect => {}
        ElementKind::Polygon => {}
        ElementKind::Text => {
            if text_measure_affects_layout(element) {
                element.text.hash(&mut hasher);
                element.font.hash(&mut hasher);
                hash_f32(element.font_size, &mut hasher);
                element.font_weight.hash(&mut hasher);
                hash_f32(element.text_max_width, &mut hasher);
                element.wrap.hash(&mut hasher);
                element.horizontal_align.hash(&mut hasher);
                element.vertical_align.hash(&mut hasher);
                hash_f32(element.line_height, &mut hasher);
            }
        }
        ElementKind::Image | ElementKind::NineSlice => {}
    }

    hash_f32(element.timer_seconds, &mut hasher);
    hasher.finish()
}

pub(super) fn element_visual_signature(element: &Element) -> u64 {
    let mut hasher = FxHasher::default();
    if !element.layout_position_affects_structure {
        element.has_x.hash(&mut hasher);
        element.has_y.hash(&mut hasher);
        hash_f32(element.x, &mut hasher);
        hash_f32(element.y, &mut hasher);
    }
    element.z_index.hash(&mut hasher);
    element.clip.hash(&mut hasher);
    hash_f32(element.clip_radius, &mut hasher);

    match element.kind {
        ElementKind::Row | ElementKind::Column | ElementKind::Stack => {
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
        ElementKind::Rect => {
            hash_color(element.color, &mut hasher);
            hash_gradient(element.gradient, &mut hasher);
            hash_border(element.border, &mut hasher);
            hash_shadow(element.shadow, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.radius, &mut hasher);
            hash_f32(element.blur, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
        ElementKind::Polygon => {
            hash_color(element.color, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
            for point in &element.polygon_points {
                hash_f32(point[0], &mut hasher);
                hash_f32(point[1], &mut hasher);
            }
        }
        ElementKind::Text => {
            element.text.hash(&mut hasher);
            element.font.hash(&mut hasher);
            hash_f32(element.font_size, &mut hasher);
            element.font_weight.hash(&mut hasher);
            hash_f32(element.text_max_width, &mut hasher);
            element.wrap.hash(&mut hasher);
            element.horizontal_align.hash(&mut hasher);
            element.vertical_align.hash(&mut hasher);
            hash_f32(element.line_height, &mut hasher);
            hash_color(element.text_color, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
        ElementKind::Image => {
            element.image.hash(&mut hasher);
            element.image_fit.hash(&mut hasher);
            hash_color(element.tint, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.radius, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
        ElementKind::NineSlice => {
            element.image.hash(&mut hasher);
            hash_slice(element.slice, &mut hasher);
            hash_edge_insets(element.content_inset, &mut hasher);
            element.center_mode.hash(&mut hasher);
            element.edge_mode.hash(&mut hasher);
            hash_color(element.tint, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
    }

    element.interactive.hash(&mut hasher);
    element.focusable.hash(&mut hasher);
    element.disabled.hash(&mut hasher);
    element.cursor.hash(&mut hasher);
    element.has_ime_rect.hash(&mut hasher);
    hash_rect(element.ime_rect, &mut hasher);
    hash_color(element.hover_color, &mut hasher);
    hash_color(element.pressed_color, &mut hasher);
    element.has_state_colors.hash(&mut hasher);
    element.smooth_state_colors.hash(&mut hasher);
    element.visual_state_source_id.hash(&mut hasher);
    element.hover_opacity_source_id.hash(&mut hasher);
    hash_f32(element.pressed_scale, &mut hasher);
    hash_f32(element.hover_hidden_opacity, &mut hasher);
    hash_f32(element.hover_visible_opacity, &mut hasher);
    element.transition.enabled.hash(&mut hasher);
    hash_f32(element.transition.duration_seconds, &mut hasher);
    hash_f32(element.transition.delay_seconds, &mut hasher);
    element.transition.ease.hash(&mut hasher);
    element.transition.properties.hash(&mut hasher);
    hash_motion(element.transition.motion, &mut hasher);
    hash_f32(element.transition.damping_ratio, &mut hasher);
    element.explicit_frame_animation.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn text_measure_affects_layout(element: &Element) -> bool {
    matches!(element.width, crate::Size::WrapContent)
        || matches!(element.height, crate::Size::WrapContent)
}

pub(super) fn hash_rect(rect: LayoutRect, hasher: &mut impl Hasher) {
    hash_f32(rect.x, hasher);
    hash_f32(rect.y, hasher);
    hash_f32(rect.width, hasher);
    hash_f32(rect.height, hasher);
}

pub(super) fn hash_size(size: crate::Size, hasher: &mut impl Hasher) {
    match size {
        crate::Size::Fixed(value) => {
            0_u8.hash(hasher);
            hash_f32(value, hasher);
        }
        crate::Size::WrapContent => 1_u8.hash(hasher),
        crate::Size::Fill => 2_u8.hash(hasher),
    }
}

pub(super) fn hash_edge_insets(insets: crate::EdgeInsets, hasher: &mut impl Hasher) {
    hash_f32(insets.left, hasher);
    hash_f32(insets.top, hasher);
    hash_f32(insets.right, hasher);
    hash_f32(insets.bottom, hasher);
}

pub(super) fn hash_gradient(gradient: crate::Gradient, hasher: &mut impl Hasher) {
    gradient.enabled.hash(hasher);
    hash_color(gradient.start, hasher);
    hash_color(gradient.end, hasher);
    gradient.direction.hash(hasher);
}

pub(super) fn hash_border(border: Border, hasher: &mut impl Hasher) {
    hash_f32(border.width, hasher);
    hash_color(border.color, hasher);
}

pub(super) fn hash_shadow(shadow: Shadow, hasher: &mut impl Hasher) {
    shadow.enabled.hash(hasher);
    hash_f32(shadow.offset[0], hasher);
    hash_f32(shadow.offset[1], hasher);
    hash_f32(shadow.blur, hasher);
    hash_f32(shadow.spread, hasher);
    hash_color(shadow.color, hasher);
}

pub(super) fn hash_transform(transform: Transform, hasher: &mut impl Hasher) {
    hash_f32(transform.translate[0], hasher);
    hash_f32(transform.translate[1], hasher);
    hash_f32(transform.scale[0], hasher);
    hash_f32(transform.scale[1], hasher);
    hash_f32(transform.rotation, hasher);
    hash_f32(transform.origin[0], hasher);
    hash_f32(transform.origin[1], hasher);
}

pub(super) fn hash_slice(slice: crate::Slice, hasher: &mut impl Hasher) {
    hash_f32(slice.left, hasher);
    hash_f32(slice.top, hasher);
    hash_f32(slice.right, hasher);
    hash_f32(slice.bottom, hasher);
}

pub(super) fn hash_motion(motion: Motion, hasher: &mut impl Hasher) {
    match motion {
        Motion::Ease => 0_u8.hash(hasher),
        Motion::Spring => 1_u8.hash(hasher),
    }
}

pub(super) fn hash_color(color: Color, hasher: &mut impl Hasher) {
    hash_f32(color.r, hasher);
    hash_f32(color.g, hasher);
    hash_f32(color.b, hasher);
    hash_f32(color.a, hasher);
}

pub(super) fn hash_f32(value: f32, hasher: &mut impl Hasher) {
    value.to_bits().hash(hasher);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ClockPeriodMap;
    use crate::retained::{RetainedComposeStats, RetainedRoot, ScopeId, ScopeRoots, ScopeSet};
    use crate::Size;

    #[test]
    fn collect_next_structure_marks_full_redraw_for_layout_change() {
        let mut runtime = Runtime::new("page");
        runtime.tree.screen = Screen::new(320.0, 200.0);
        let previous = fixed_rect("page.root", 20.0, 20.0, Color::WHITE);
        runtime.tree.structure = collect_structure(&[previous], 0);
        runtime.mark_rendered();

        let next = vec![fixed_rect("page.root", 40.0, 20.0, Color::WHITE)];
        let structure = collect_next_structure(&mut runtime, Screen::new(320.0, 200.0), &next);

        assert_eq!(structure.len(), 1);
        assert!(runtime.needs_render());
        assert!(runtime.full_redraw());
    }

    #[test]
    fn collect_next_structure_marks_render_only_for_visual_change() {
        let mut runtime = Runtime::new("page");
        runtime.tree.screen = Screen::new(320.0, 200.0);
        let previous = fixed_rect("page.root", 20.0, 20.0, Color::WHITE);
        runtime.tree.structure = collect_structure(&[previous], 0);
        runtime.mark_rendered();

        let next = vec![fixed_rect("page.root", 20.0, 20.0, Color::BLACK)];
        let structure = collect_next_structure(&mut runtime, Screen::new(320.0, 200.0), &next);

        assert_eq!(structure.len(), 1);
        assert!(runtime.needs_render());
        assert!(!runtime.full_redraw());
    }

    #[test]
    fn layout_structure_ignores_resolved_frame_output() {
        let mut previous = fixed_rect("page.root", 20.0, 20.0, Color::WHITE);
        previous.frame = LayoutRect::new(0.0, 0.0, 20.0, 20.0);
        let mut next = fixed_rect("page.root", 20.0, 20.0, Color::WHITE);
        next.frame = LayoutRect::new(40.0, 24.0, 20.0, 20.0);

        let previous = collect_structure(&[previous], 0);
        let next = collect_structure(&[next], previous.len());

        assert!(layout_structures_match(&next, &previous));
        assert!(visual_structures_match(&next, &previous));
    }

    #[test]
    fn cleanup_stale_committed_state_prunes_removed_ids_and_marks_render_dirty() {
        let mut runtime = Runtime::new("page");
        runtime.tree.roots = vec![fixed_rect("page.live", 20.0, 20.0, Color::WHITE)];
        runtime.input.responses.insert(
            NodeId::new("page.live"),
            Response {
                hovered: true,
                ..Response::default()
            },
        );
        runtime.input.responses.insert(
            NodeId::new("page.stale"),
            Response {
                clicked: true,
                ..Response::default()
            },
        );
        runtime.mark_rendered();

        cleanup_stale_committed_state(&mut runtime);

        assert!(runtime
            .input
            .responses
            .contains_key(&NodeId::new("page.live")));
        assert!(!runtime
            .input
            .responses
            .contains_key(&NodeId::new("page.stale")));
        assert!(runtime.needs_render());
        let invalidations = runtime.invalidation.snapshot();
        assert!(invalidations.iter().any(|invalidation| {
            invalidation.target.id() == "page"
                && invalidation.source == InvalidationSource::Runtime("stale_state_cleanup")
                && invalidation.flags == DirtyFlags::DRAW
                && invalidation.pass_flags.request_draw
                && !invalidation.pass_flags.request_compose_ui
        }));
    }

    #[test]
    fn cleanup_stale_committed_state_keeps_render_clean_when_nothing_changes() {
        let mut runtime = Runtime::new("page");
        runtime.tree.roots = vec![fixed_rect("page.live", 20.0, 20.0, Color::WHITE)];
        runtime.input.responses.insert(
            NodeId::new("page.live"),
            Response {
                hovered: true,
                ..Response::default()
            },
        );
        runtime.mark_rendered();

        cleanup_stale_committed_state(&mut runtime);

        assert!(runtime
            .input
            .responses
            .contains_key(&NodeId::new("page.live")));
        assert!(!runtime.needs_render());
    }

    #[test]
    fn commit_composed_tree_updates_tree_state_and_clock_ticks() {
        use std::time::Duration;

        let mut runtime = Runtime::new("page");
        runtime.timing.clock_seconds = 2.25;
        let mut clock_periods = ClockPeriodMap::default();
        clock_periods.insert(ScopeId::new("page.clock"), Duration::from_secs(1));
        let roots = vec![fixed_rect("page.root", 20.0, 20.0, Color::WHITE)];
        let structure = collect_structure(&roots, 0);
        let mut scope_roots = ScopeRoots::default();
        scope_roots.insert(
            ScopeId::new("page.scope"),
            RetainedRoot::from_elements(&roots),
        );
        let mut live_scopes = ScopeSet::default();
        live_scopes.insert(ScopeId::new("page.live"));
        let mut clock_ids = ScopeSet::default();
        clock_ids.insert(ScopeId::new("page.clock"));
        let retained_stats = RetainedComposeStats {
            reused: 1,
            partial_layout: true,
            ..RetainedComposeStats::default()
        };

        commit_composed_tree(
            &mut runtime,
            ComposedTreeCommit {
                screen: Screen::new(320.0, 200.0),
                roots,
                scope_roots,
                live_scopes,
                clock_ids,
                clock_periods: Some(clock_periods),
                scope_layer_roots: ScopeLayerRoots::default(),
                scope_layer_intents: ScopeLayerIntents::default(),
                retained_stats,
                structure,
            },
        );

        assert_eq!(runtime.tree.screen, Screen::new(320.0, 200.0));
        assert_eq!(runtime.tree.roots.len(), 1);
        assert_eq!(runtime.tree.structure.len(), 1);
        assert!(runtime.tree.scope_roots.contains_key("page.scope"));
        assert!(runtime.tree.live_ids.contains("page.live"));
        assert!(runtime.tree.clock_ids.contains("page.clock"));
        assert_eq!(
            runtime
                .tree
                .clock_period_ticks
                .as_ref()
                .and_then(|ticks| ticks.get(&ScopeId::new("page.clock"))),
            Some(&2)
        );
        assert_eq!(runtime.tree.retained_stats.reused, 1);
        assert!(runtime.tree.retained_stats.partial_layout);
        assert_eq!(runtime.tree.frame_index, 1);
    }

    fn fixed_rect(id: &str, width: f32, height: f32, color: Color) -> Element {
        let mut element = Element::new(ElementKind::Rect, id);
        element.width = Size::Fixed(width);
        element.height = Size::Fixed(height);
        element.color = color;
        element
    }
}
