use rustc_hash::FxHashMap;

use crate::ecs::{EntityId, World};

use super::{
    preferred_text_size, UiAlign, UiButton, UiLayout, UiLength, UiNode, UiRect, UiScroll, UiSlider,
    UiText, UiToggle,
};

#[derive(Debug, Clone)]
pub(crate) struct ResolvedUiNode {
    pub entity: EntityId,
    pub id: Option<super::UiId>,
    pub rect: UiRect,
    pub clip_rect: UiRect,
    pub visible: bool,
    pub enabled: bool,
    stack_path: Vec<(i32, u32)>,
}

#[derive(Debug, Clone)]
struct NodeSnapshot {
    entity: EntityId,
    node: UiNode,
    scroll: Option<UiScroll>,
    preferred_size: [f32; 2],
}

pub(crate) fn resolve_world_layout(world: &World, surface_size: [f32; 2]) -> Vec<ResolvedUiNode> {
    let mut query = world.query::<(
        &UiNode,
        Option<&UiText>,
        Option<&UiButton>,
        Option<&UiSlider>,
        Option<&UiToggle>,
        Option<&UiScroll>,
    )>();
    let mut nodes = Vec::new();
    query.for_each_with_entity(
        world,
        |entity, (node, text, button, slider, toggle, scroll)| {
            let preferred_size = preferred_widget_size(node, text, button, slider, toggle);
            nodes.push(NodeSnapshot {
                entity,
                node: node.clone(),
                scroll: scroll.copied(),
                preferred_size,
            });
        },
    );
    resolve_snapshots(&nodes, surface_size)
}

pub(crate) fn rect_map(nodes: &[ResolvedUiNode]) -> FxHashMap<EntityId, UiRect> {
    let mut rects = FxHashMap::default();
    for node in nodes {
        rects.insert(node.entity, node.rect);
    }
    rects
}

pub(crate) fn hit_test(nodes: &[ResolvedUiNode], point: [f32; 2]) -> Option<ResolvedUiNode> {
    nodes
        .iter()
        .filter(|node| {
            node.visible
                && node.enabled
                && node.rect.contains(point)
                && node.clip_rect.contains(point)
        })
        .max_by(|a, b| a.stack_path.cmp(&b.stack_path))
        .cloned()
}

fn preferred_widget_size(
    node: &UiNode,
    text: Option<&UiText>,
    button: Option<&UiButton>,
    slider: Option<&UiSlider>,
    toggle: Option<&UiToggle>,
) -> [f32; 2] {
    let mut preferred = node.min_size;
    if let Some(text) = text {
        let text_size = preferred_text_size(&text.text, text.font_size);
        preferred[0] = preferred[0].max(text_size[0]);
        preferred[1] = preferred[1].max(text_size[1]);
    }
    if let Some(button) = button {
        let text_size = preferred_text_size(&button.label, 18.0);
        preferred[0] = preferred[0].max(text_size[0] + 28.0);
        preferred[1] = preferred[1].max(text_size[1] + 14.0);
    }
    if let Some(slider) = slider {
        preferred[0] = preferred[0].max(120.0);
        preferred[1] = preferred[1].max(slider.thumb_height.max(slider.track_height));
    }
    if let Some(toggle) = toggle {
        let label_size = preferred_text_size(&toggle.label, 18.0);
        let label_width = if toggle.label.is_empty() {
            0.0
        } else {
            label_size[0] + 10.0
        };
        preferred[0] = preferred[0].max(toggle.track_width + label_width);
        preferred[1] = preferred[1].max(toggle.track_height.max(label_size[1]));
    }
    preferred
}

fn resolve_snapshots(nodes: &[NodeSnapshot], surface_size: [f32; 2]) -> Vec<ResolvedUiNode> {
    let mut by_entity = FxHashMap::default();
    let mut children: FxHashMap<EntityId, Vec<usize>> = FxHashMap::default();
    for (index, snapshot) in nodes.iter().enumerate() {
        by_entity.insert(snapshot.entity, index);
        if let Some(parent) = snapshot.node.parent {
            children.entry(parent).or_default().push(index);
        }
    }

    let screen = UiRect::new(0.0, 0.0, surface_size[0].max(0.0), surface_size[1].max(0.0));
    let mut rects: FxHashMap<EntityId, UiRect> = FxHashMap::default();
    let mut visiting = Vec::new();

    for (index, snapshot) in nodes.iter().enumerate() {
        let parent_missing = snapshot
            .node
            .parent
            .is_some_and(|parent| !by_entity.contains_key(&parent));
        if snapshot.node.parent.is_none() || parent_missing {
            let rect = resolve_node_rect(snapshot, screen);
            rects.insert(snapshot.entity, rect);
            resolve_children(index, nodes, &children, &mut rects, &mut visiting);
        }
    }

    for (index, snapshot) in nodes.iter().enumerate() {
        if !rects.contains_key(&snapshot.entity) {
            let parent_rect = snapshot
                .node
                .parent
                .and_then(|parent| rects.get(&parent).copied())
                .unwrap_or(screen);
            let rect = resolve_node_rect(snapshot, parent_rect);
            rects.insert(snapshot.entity, rect);
            resolve_children(index, nodes, &children, &mut rects, &mut visiting);
        }
    }

    let mut flag_cache: FxHashMap<EntityId, (bool, bool)> = FxHashMap::default();
    let mut stack_cache: FxHashMap<EntityId, Vec<(i32, u32)>> = FxHashMap::default();
    let mut clip_cache: FxHashMap<EntityId, UiRect> = FxHashMap::default();
    let mut resolved = Vec::with_capacity(nodes.len());
    for snapshot in nodes {
        if let Some(rect) = rects.get(&snapshot.entity).copied() {
            let (visible, enabled) = effective_flags(
                snapshot,
                nodes,
                &by_entity,
                &mut flag_cache,
                &mut Vec::new(),
            );
            let stack_path = stack_path_for(
                snapshot,
                nodes,
                &by_entity,
                &mut stack_cache,
                &mut Vec::new(),
            );
            let clip_rect = clip_rect_for(
                snapshot,
                nodes,
                &by_entity,
                &rects,
                screen,
                &mut clip_cache,
                &mut Vec::new(),
            );
            resolved.push(ResolvedUiNode {
                entity: snapshot.entity,
                id: snapshot.node.id.clone(),
                rect,
                clip_rect,
                visible,
                enabled,
                stack_path,
            });
        }
    }
    resolved.sort_by(|a, b| a.stack_path.cmp(&b.stack_path));
    resolved
}

fn effective_flags(
    snapshot: &NodeSnapshot,
    nodes: &[NodeSnapshot],
    by_entity: &FxHashMap<EntityId, usize>,
    cache: &mut FxHashMap<EntityId, (bool, bool)>,
    visiting: &mut Vec<EntityId>,
) -> (bool, bool) {
    if let Some(flags) = cache.get(&snapshot.entity).copied() {
        return flags;
    }
    if visiting.contains(&snapshot.entity) {
        return (snapshot.node.visible, snapshot.node.enabled);
    }
    visiting.push(snapshot.entity);
    let mut visible = snapshot.node.visible;
    let mut enabled = snapshot.node.enabled;
    if let Some(parent) = snapshot.node.parent {
        if let Some(parent_index) = by_entity.get(&parent).copied() {
            let (parent_visible, parent_enabled) =
                effective_flags(&nodes[parent_index], nodes, by_entity, cache, visiting);
            visible &= parent_visible;
            enabled &= parent_enabled;
        }
    }
    visiting.pop();
    let flags = (visible, enabled);
    cache.insert(snapshot.entity, flags);
    flags
}

fn stack_path_for(
    snapshot: &NodeSnapshot,
    nodes: &[NodeSnapshot],
    by_entity: &FxHashMap<EntityId, usize>,
    cache: &mut FxHashMap<EntityId, Vec<(i32, u32)>>,
    visiting: &mut Vec<EntityId>,
) -> Vec<(i32, u32)> {
    if let Some(stack) = cache.get(&snapshot.entity).cloned() {
        return stack;
    }
    let own_layer = (snapshot.node.z, snapshot.entity.index());
    if visiting.contains(&snapshot.entity) {
        return vec![own_layer];
    }
    visiting.push(snapshot.entity);

    let mut stack = if let Some(parent) = snapshot.node.parent {
        if let Some(parent_index) = by_entity.get(&parent).copied() {
            stack_path_for(&nodes[parent_index], nodes, by_entity, cache, visiting)
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    stack.push(own_layer);

    visiting.pop();
    cache.insert(snapshot.entity, stack.clone());
    stack
}

fn clip_rect_for(
    snapshot: &NodeSnapshot,
    nodes: &[NodeSnapshot],
    by_entity: &FxHashMap<EntityId, usize>,
    rects: &FxHashMap<EntityId, UiRect>,
    screen: UiRect,
    cache: &mut FxHashMap<EntityId, UiRect>,
    visiting: &mut Vec<EntityId>,
) -> UiRect {
    if let Some(clip) = cache.get(&snapshot.entity).copied() {
        return clip;
    }
    if visiting.contains(&snapshot.entity) {
        return rects.get(&snapshot.entity).copied().unwrap_or(screen);
    }
    visiting.push(snapshot.entity);

    let mut clip = if let Some(parent) = snapshot.node.parent {
        if let Some(parent_index) = by_entity.get(&parent).copied() {
            clip_rect_for(
                &nodes[parent_index],
                nodes,
                by_entity,
                rects,
                screen,
                cache,
                visiting,
            )
        } else {
            screen
        }
    } else {
        screen
    };

    if snapshot.scroll.is_some_and(|scroll| scroll.clip) {
        let own = rects.get(&snapshot.entity).copied().unwrap_or(screen);
        clip = clip.intersection(own).unwrap_or(UiRect::ZERO);
    }

    visiting.pop();
    cache.insert(snapshot.entity, clip);
    clip
}

fn resolve_children(
    parent_index: usize,
    nodes: &[NodeSnapshot],
    children: &FxHashMap<EntityId, Vec<usize>>,
    rects: &mut FxHashMap<EntityId, UiRect>,
    visiting: &mut Vec<EntityId>,
) {
    let parent = &nodes[parent_index];
    if visiting.contains(&parent.entity) {
        return;
    }
    visiting.push(parent.entity);

    let Some(child_indices) = children.get(&parent.entity) else {
        visiting.pop();
        return;
    };
    let Some(parent_rect) = rects.get(&parent.entity).copied() else {
        visiting.pop();
        return;
    };

    match parent.node.layout {
        UiLayout::None => {
            let parent_rect = scrolled_content_rect(parent_rect, parent.scroll);
            for &child_index in child_indices {
                let child = &nodes[child_index];
                let rect = resolve_node_rect(child, parent_rect);
                rects.insert(child.entity, rect);
                resolve_children(child_index, nodes, children, rects, visiting);
            }
        }
        UiLayout::Row {
            padding,
            gap,
            align,
        } => {
            let inner = scrolled_content_rect(parent_rect.inset(padding), parent.scroll);
            let mut cursor = inner.x;
            let mut ordered = child_indices.clone();
            ordered.sort_by_key(|index| nodes[*index].entity.index());
            let sizes = resolve_linear_sizes(&ordered, nodes, inner, 0, gap);
            for (child_index, mut size) in ordered.into_iter().zip(sizes) {
                let child = &nodes[child_index];
                if align == UiAlign::Stretch {
                    size[1] = inner.height;
                }
                let y = cross_axis_position(inner.y, inner.height, size[1], align);
                let rect = UiRect::new(cursor, y, size[0], size[1]);
                rects.insert(child.entity, rect);
                cursor += size[0] + gap;
                resolve_children(child_index, nodes, children, rects, visiting);
            }
        }
        UiLayout::Column {
            padding,
            gap,
            align,
        } => {
            let inner = scrolled_content_rect(parent_rect.inset(padding), parent.scroll);
            let mut cursor = inner.y;
            let mut ordered = child_indices.clone();
            ordered.sort_by_key(|index| nodes[*index].entity.index());
            let sizes = resolve_linear_sizes(&ordered, nodes, inner, 1, gap);
            for (child_index, mut size) in ordered.into_iter().zip(sizes) {
                let child = &nodes[child_index];
                if align == UiAlign::Stretch {
                    size[0] = inner.width;
                }
                let x = cross_axis_position(inner.x, inner.width, size[0], align);
                let rect = UiRect::new(x, cursor, size[0], size[1]);
                rects.insert(child.entity, rect);
                cursor += size[1] + gap;
                resolve_children(child_index, nodes, children, rects, visiting);
            }
        }
    }

    visiting.pop();
}

fn scrolled_content_rect(mut rect: UiRect, scroll: Option<UiScroll>) -> UiRect {
    if let Some(scroll) = scroll {
        rect.x -= scroll.offset[0];
        rect.y -= scroll.offset[1];
    }
    rect
}

fn resolve_linear_sizes(
    ordered: &[usize],
    nodes: &[NodeSnapshot],
    inner: UiRect,
    main_axis: usize,
    gap: f32,
) -> Vec<[f32; 2]> {
    let mut sizes: Vec<_> = ordered
        .iter()
        .map(|index| resolve_size(&nodes[*index], inner))
        .collect();
    let mut fill_weight = 0.0;
    let mut fixed = 0.0;

    for (&index, size) in ordered.iter().zip(sizes.iter()) {
        if let Some(weight) = fill_weight_for(nodes[index].node.size[main_axis]) {
            fill_weight += weight;
        } else {
            fixed += size[main_axis];
        }
    }

    if fill_weight <= f32::EPSILON {
        return sizes;
    }

    let available = if main_axis == 0 {
        inner.width
    } else {
        inner.height
    };
    let gap_total = gap * ordered.len().saturating_sub(1) as f32;
    let remaining = (available - fixed - gap_total).max(0.0);

    for (&index, size) in ordered.iter().zip(sizes.iter_mut()) {
        if let Some(weight) = fill_weight_for(nodes[index].node.size[main_axis]) {
            let allocated = remaining * (weight / fill_weight);
            size[main_axis] = allocated.max(nodes[index].node.min_size[main_axis]);
        }
    }

    sizes
}

fn fill_weight_for(length: UiLength) -> Option<f32> {
    match length {
        UiLength::Fill(weight) => Some(if weight > f32::EPSILON { weight } else { 1.0 }),
        _ => None,
    }
}

fn resolve_node_rect(snapshot: &NodeSnapshot, parent: UiRect) -> UiRect {
    let node = &snapshot.node;
    let mut size = resolve_size(snapshot, parent);
    if node.anchor == super::UiAnchor::Stretch {
        if matches!(node.size[0], UiLength::Auto) {
            size[0] = (parent.width - node.position[0] * 2.0).max(node.min_size[0]);
        }
        if matches!(node.size[1], UiLength::Auto) {
            size[1] = (parent.height - node.position[1] * 2.0).max(node.min_size[1]);
        }
    }

    let [offset_x, offset_y] = node.position;
    match node.anchor {
        super::UiAnchor::TopLeft | super::UiAnchor::Stretch => {
            UiRect::new(parent.x + offset_x, parent.y + offset_y, size[0], size[1])
        }
        super::UiAnchor::TopRight => UiRect::new(
            parent.right() - size[0] - offset_x,
            parent.y + offset_y,
            size[0],
            size[1],
        ),
        super::UiAnchor::BottomLeft => UiRect::new(
            parent.x + offset_x,
            parent.bottom() - size[1] - offset_y,
            size[0],
            size[1],
        ),
        super::UiAnchor::BottomRight => UiRect::new(
            parent.right() - size[0] - offset_x,
            parent.bottom() - size[1] - offset_y,
            size[0],
            size[1],
        ),
        super::UiAnchor::Center => UiRect::new(
            parent.x + (parent.width - size[0]) * 0.5 + offset_x,
            parent.y + (parent.height - size[1]) * 0.5 + offset_y,
            size[0],
            size[1],
        ),
    }
}

fn resolve_size(snapshot: &NodeSnapshot, parent: UiRect) -> [f32; 2] {
    [
        resolve_length(
            snapshot.node.size[0],
            parent.width,
            snapshot.preferred_size[0],
            snapshot.node.min_size[0],
        ),
        resolve_length(
            snapshot.node.size[1],
            parent.height,
            snapshot.preferred_size[1],
            snapshot.node.min_size[1],
        ),
    ]
}

fn resolve_length(length: UiLength, parent: f32, preferred: f32, min: f32) -> f32 {
    match length {
        UiLength::Px(value) => value.max(min),
        UiLength::Percent(value) => (parent * value).max(min),
        UiLength::Fill(_) => parent.max(min),
        UiLength::Auto => preferred.max(min),
    }
}

fn cross_axis_position(start: f32, available: f32, size: f32, align: UiAlign) -> f32 {
    match align {
        UiAlign::Start | UiAlign::Stretch => start,
        UiAlign::Center => start + (available - size) * 0.5,
        UiAlign::End => start + available - size,
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs::World;

    use super::super::{UiAlign, UiAnchor, UiLayout, UiLength, UiNode, UiPanel, UiRect, UiScroll};
    use super::hit_test;
    use super::resolve_world_layout;
    use crate::render::Color;

    #[test]
    fn anchors_compute_screen_rects() {
        let mut world = World::new();
        let a = world.spawn((
            UiNode::panel(100.0, 50.0)
                .anchor(UiAnchor::BottomRight)
                .at(10.0, 20.0),
            UiPanel::new(Color::WHITE),
        ));
        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let rect = nodes.iter().find(|node| node.entity == a).unwrap().rect;
        assert_eq!(rect, UiRect::new(690.0, 530.0, 100.0, 50.0));
    }

    #[test]
    fn row_layout_uses_padding_and_gap() {
        let mut world = World::new();
        let parent = world.spawn((
            UiNode::panel(300.0, 80.0).layout(UiLayout::row(
                UiRect::new(10.0, 8.0, 10.0, 8.0),
                6.0,
                UiAlign::Start,
            )),
            UiPanel::new(Color::WHITE),
        ));
        let first = world.spawn((UiNode::panel(50.0, 20.0).child_of(parent),));
        let second = world.spawn((UiNode::panel(40.0, 20.0).child_of(parent),));
        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let first_rect = nodes.iter().find(|node| node.entity == first).unwrap().rect;
        let second_rect = nodes
            .iter()
            .find(|node| node.entity == second)
            .unwrap()
            .rect;
        assert_eq!(first_rect, UiRect::new(10.0, 8.0, 50.0, 20.0));
        assert_eq!(second_rect, UiRect::new(66.0, 8.0, 40.0, 20.0));
    }

    #[test]
    fn row_layout_order_is_independent_from_z() {
        let mut world = World::new();
        let parent = world.spawn((
            UiNode::panel(300.0, 80.0).layout(UiLayout::row(UiRect::ZERO, 6.0, UiAlign::Start)),
            UiPanel::new(Color::WHITE),
        ));
        let first = world.spawn((UiNode::panel(50.0, 20.0).child_of(parent).z(10),));
        let second = world.spawn((UiNode::panel(40.0, 20.0).child_of(parent).z(0),));

        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let first_rect = nodes.iter().find(|node| node.entity == first).unwrap().rect;
        let second_rect = nodes
            .iter()
            .find(|node| node.entity == second)
            .unwrap()
            .rect;

        assert_eq!(first_rect, UiRect::new(0.0, 0.0, 50.0, 20.0));
        assert_eq!(second_rect, UiRect::new(56.0, 0.0, 40.0, 20.0));
    }

    #[test]
    fn row_layout_distributes_fill_width_by_weight() {
        let mut world = World::new();
        let parent = world.spawn((
            UiNode::panel(300.0, 80.0).layout(UiLayout::row(
                UiRect::new(10.0, 8.0, 10.0, 8.0),
                5.0,
                UiAlign::Start,
            )),
            UiPanel::new(Color::WHITE),
        ));
        let fixed = world.spawn((UiNode::panel(50.0, 20.0).child_of(parent),));
        let fill_a = world.spawn((UiNode::panel(1.0, 20.0)
            .child_of(parent)
            .width(UiLength::Fill(1.0)),));
        let fill_b = world.spawn((UiNode::panel(1.0, 20.0)
            .child_of(parent)
            .width(UiLength::Fill(2.0)),));

        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let fixed_rect = nodes.iter().find(|node| node.entity == fixed).unwrap().rect;
        let fill_a_rect = nodes
            .iter()
            .find(|node| node.entity == fill_a)
            .unwrap()
            .rect;
        let fill_b_rect = nodes
            .iter()
            .find(|node| node.entity == fill_b)
            .unwrap()
            .rect;

        assert_eq!(fixed_rect, UiRect::new(10.0, 8.0, 50.0, 20.0));
        assert!((fill_a_rect.width - 73.333).abs() < 0.01);
        assert!((fill_b_rect.width - 146.667).abs() < 0.01);
        assert!((fill_b_rect.right() - 290.0).abs() < 0.01);
    }

    #[test]
    fn column_layout_distributes_fill_height_by_weight() {
        let mut world = World::new();
        let parent = world.spawn((
            UiNode::panel(80.0, 300.0).layout(UiLayout::column(
                UiRect::new(8.0, 10.0, 8.0, 10.0),
                5.0,
                UiAlign::Start,
            )),
            UiPanel::new(Color::WHITE),
        ));
        let fixed = world.spawn((UiNode::panel(20.0, 50.0).child_of(parent),));
        let fill = world.spawn((UiNode::panel(20.0, 1.0)
            .child_of(parent)
            .height(UiLength::Fill(1.0)),));

        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let fixed_rect = nodes.iter().find(|node| node.entity == fixed).unwrap().rect;
        let fill_rect = nodes.iter().find(|node| node.entity == fill).unwrap().rect;

        assert_eq!(fixed_rect, UiRect::new(8.0, 10.0, 20.0, 50.0));
        assert_eq!(fill_rect, UiRect::new(8.0, 65.0, 20.0, 225.0));
    }

    #[test]
    fn scroll_offset_moves_children_inside_clipped_parent() {
        let mut world = World::new();
        let parent = world.spawn((
            UiNode::panel(120.0, 80.0),
            UiScroll::vertical()
                .content_size(120.0, 180.0)
                .offset(0.0, 40.0),
        ));
        let child = world.spawn((UiNode::panel(100.0, 30.0).child_of(parent).at(10.0, 90.0),));

        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let child_node = nodes.iter().find(|node| node.entity == child).unwrap();
        assert_eq!(child_node.rect, UiRect::new(10.0, 50.0, 100.0, 30.0));
        assert_eq!(child_node.clip_rect, UiRect::new(0.0, 0.0, 120.0, 80.0));
        assert_eq!(hit_test(&nodes, [20.0, 60.0]).unwrap().entity, child);
    }

    #[test]
    fn scrolled_children_do_not_hit_outside_clip_rect() {
        let mut world = World::new();
        let parent = world.spawn((UiNode::panel(120.0, 80.0), UiScroll::vertical()));
        world.spawn((UiNode::panel(100.0, 30.0).child_of(parent).at(10.0, 90.0),));

        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        assert!(hit_test(&nodes, [20.0, 95.0]).is_none());
    }

    #[test]
    fn topmost_hit_test_prefers_z_then_entity_order() {
        let mut world = World::new();
        world.spawn((UiNode::panel(100.0, 100.0).z(1),));
        let top = world.spawn((UiNode::panel(100.0, 100.0).z(2),));
        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let hit = hit_test(&nodes, [20.0, 20.0]).unwrap();
        assert_eq!(hit.entity, top);
    }

    #[test]
    fn children_inherit_parent_z_and_hit_above_parent() {
        let mut world = World::new();
        world.spawn((UiNode::panel(100.0, 100.0).z(30),));
        let parent = world.spawn((UiNode::panel(100.0, 100.0).z(40),));
        let child = world.spawn((UiNode::panel(100.0, 100.0).child_of(parent),));

        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let parent_node = nodes.iter().find(|node| node.entity == parent).unwrap();
        let child_node = nodes.iter().find(|node| node.entity == child).unwrap();
        assert_eq!(parent_node.stack_path, vec![(40, parent.index())]);
        assert_eq!(
            child_node.stack_path,
            vec![(40, parent.index()), (0, child.index())]
        );

        let hit = hit_test(&nodes, [20.0, 20.0]).unwrap();
        assert_eq!(hit.entity, child);
    }

    #[test]
    fn child_z_stays_inside_parent_stack() {
        let mut world = World::new();
        let parent = world.spawn((UiNode::panel(100.0, 100.0).z(10),));
        let child = world.spawn((UiNode::panel(100.0, 100.0).child_of(parent).z(5),));
        let sibling = world.spawn((UiNode::panel(100.0, 100.0).z(14),));

        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let child_node = nodes.iter().find(|node| node.entity == child).unwrap();
        assert_eq!(
            child_node.stack_path,
            vec![(10, parent.index()), (5, child.index())]
        );

        let hit = hit_test(&nodes, [20.0, 20.0]).unwrap();
        assert_eq!(hit.entity, sibling);
    }

    #[test]
    fn later_same_z_root_can_cover_earlier_root_children() {
        let mut world = World::new();
        let parent = world.spawn((UiNode::panel(100.0, 100.0).z(10),));
        world.spawn((UiNode::panel(100.0, 100.0).child_of(parent).z(100),));
        let sibling = world.spawn((UiNode::panel(100.0, 100.0).z(10),));

        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let hit = hit_test(&nodes, [20.0, 20.0]).unwrap();
        assert_eq!(hit.entity, sibling);
    }

    #[test]
    fn hidden_and_disabled_do_not_hit() {
        let mut world = World::new();
        world.spawn((UiNode::panel(100.0, 100.0).z(3).hidden(),));
        world.spawn((UiNode::panel(100.0, 100.0).z(2).disabled(),));
        let active = world.spawn((UiNode::panel(100.0, 100.0).z(1),));
        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let hit = hit_test(&nodes, [20.0, 20.0]).unwrap();
        assert_eq!(hit.entity, active);
    }

    #[test]
    fn hidden_parent_hides_children_from_hit_test() {
        let mut world = World::new();
        let parent = world.spawn((UiNode::panel(100.0, 100.0).z(10).hidden(),));
        world.spawn((UiNode::panel(100.0, 100.0).child_of(parent).z(11),));
        let active = world.spawn((UiNode::panel(100.0, 100.0).z(1),));
        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let hit = hit_test(&nodes, [20.0, 20.0]).unwrap();
        assert_eq!(hit.entity, active);
    }

    #[test]
    fn disabled_parent_disables_children_from_hit_test() {
        let mut world = World::new();
        let parent = world.spawn((UiNode::panel(100.0, 100.0).z(10).disabled(),));
        world.spawn((UiNode::panel(100.0, 100.0).child_of(parent).z(100),));
        let active = world.spawn((UiNode::panel(100.0, 100.0).z(1),));
        let nodes = resolve_world_layout(&world, [800.0, 600.0]);
        let hit = hit_test(&nodes, [20.0, 20.0]).unwrap();
        assert_eq!(hit.entity, active);
    }
}
