use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use super::{NodeId, Runtime};
use crate::retained::ScopeId;
use crate::{Element, LayoutRect, Screen, Size};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId(String);

impl LayerId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for LayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayerKind {
    #[default]
    Popover,
    Modal,
    Tooltip,
    Toast,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayerPlacement {
    #[default]
    BottomStart,
    BottomEnd,
    TopStart,
    TopEnd,
    RightStart,
    RightEnd,
    LeftStart,
    LeftEnd,
    Center,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutsideClickPolicy {
    #[default]
    Ignore,
    Close,
    Block,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayerCollision {
    #[default]
    None,
    Shift,
    FlipShift,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerSize {
    pub width: Size,
    pub height: Size,
}

impl LayerSize {
    pub fn new(width: Size, height: Size) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerIntent {
    pub id: LayerId,
    pub owner: NodeId,
    pub root: NodeId,
    pub anchor: Option<NodeId>,
    pub fallback_anchor: Option<LayoutRect>,
    pub boundary: Option<NodeId>,
    pub open: bool,
    pub kind: LayerKind,
    pub placement: LayerPlacement,
    pub size: LayerSize,
    pub gap: f32,
    pub offset: [f32; 2],
    pub collision: LayerCollision,
    pub z_index: i32,
    pub outside_click: OutsideClickPolicy,
}

pub(crate) type ScopeLayerIntents = FxHashMap<ScopeId, Vec<LayerIntent>>;
pub(crate) type ScopeLayerRoots = FxHashMap<ScopeId, Vec<NodeId>>;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct LayerRuntimeIntent {
    pub(super) id: LayerId,
    owner: NodeId,
    root: NodeId,
    anchor: Option<NodeId>,
    fallback_anchor: Option<LayoutRect>,
    boundary: Option<NodeId>,
    open: bool,
    kind: LayerKind,
    placement: LayerPlacement,
    size: LayerSize,
    gap: f32,
    offset: [f32; 2],
    collision: LayerCollision,
    z_index: i32,
    outside_click: OutsideClickPolicy,
}

impl LayerRuntimeIntent {
    pub(super) fn from_intent(intent: LayerIntent) -> Self {
        Self {
            id: intent.id,
            owner: intent.owner,
            root: intent.root,
            anchor: intent.anchor,
            fallback_anchor: intent.fallback_anchor,
            boundary: intent.boundary,
            open: intent.open,
            kind: intent.kind,
            placement: intent.placement,
            size: intent.size,
            gap: intent.gap.max(0.0),
            offset: intent.offset,
            collision: intent.collision,
            z_index: intent.z_index,
            outside_click: intent.outside_click,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerLifecycleAction {
    Created,
    Reused,
    Removed,
    Closed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayerAnchorSource {
    #[default]
    None,
    PreviousFrame,
    Fallback,
    Missing,
    Closed,
    Removed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerDebugRecord {
    layer_id: LayerId,
    owner_id: NodeId,
    root_id: NodeId,
    anchor_id: Option<NodeId>,
    pub id: String,
    pub owner: String,
    pub root: String,
    pub anchor: Option<String>,
    pub fallback_anchor: Option<LayoutRect>,
    pub boundary: Option<String>,
    pub anchor_source: LayerAnchorSource,
    pub open: bool,
    pub kind: LayerKind,
    pub placement: LayerPlacement,
    pub size: LayerSize,
    pub gap: f32,
    pub offset: [f32; 2],
    pub collision: LayerCollision,
    pub z_index: i32,
    pub outside_click: OutsideClickPolicy,
    pub action: LayerLifecycleAction,
}

impl LayerDebugRecord {
    pub fn layer_id(&self) -> &LayerId {
        &self.layer_id
    }

    pub fn owner_id(&self) -> &NodeId {
        &self.owner_id
    }

    pub fn root_id(&self) -> &NodeId {
        &self.root_id
    }

    pub fn anchor_id(&self) -> Option<&NodeId> {
        self.anchor_id.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerDismissalRecord {
    layer_id: LayerId,
    owner_id: NodeId,
    pub id: String,
    pub owner: String,
    pub policy: OutsideClickPolicy,
}

impl LayerDismissalRecord {
    pub fn layer_id(&self) -> &LayerId {
        &self.layer_id
    }

    pub fn owner_id(&self) -> &NodeId {
        &self.owner_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LayerDismissal {
    id: LayerId,
    owner: NodeId,
    policy: OutsideClickPolicy,
}

impl LayerDismissal {
    pub(super) fn target(&self) -> LayerId {
        self.id.clone()
    }

    pub(super) fn into_record(self) -> LayerDismissalRecord {
        LayerDismissalRecord {
            layer_id: self.id.clone(),
            owner_id: self.owner.clone(),
            id: self.id.as_str().to_string(),
            owner: self.owner.as_str().to_string(),
            policy: self.policy,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayerPointerAction {
    #[default]
    PassThrough,
    HitLayer,
    Blocked,
    Dismissed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerPointerDebugRecord {
    layer_id: Option<LayerId>,
    hit_layer_id: Option<LayerId>,
    pub action: LayerPointerAction,
    pub layer: Option<String>,
    pub hit_layer: Option<String>,
    pub policy: OutsideClickPolicy,
}

impl LayerPointerDebugRecord {
    pub fn layer_id(&self) -> Option<&LayerId> {
        self.layer_id.as_ref()
    }

    pub fn hit_layer_id(&self) -> Option<&LayerId> {
        self.hit_layer_id.as_ref()
    }
}

pub(super) struct LayerFrameCommit {
    intents: Vec<LayerRuntimeIntent>,
    debug_records: Vec<LayerDebugRecord>,
}

pub(super) fn prepare_layer_frame_commit(
    previous_intents: &mut Vec<LayerRuntimeIntent>,
    next_intents: Vec<LayerIntent>,
    previous_roots: &[crate::Element],
) -> LayerFrameCommit {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.layer_commit_prepare");
    let previous_intents = std::mem::take(previous_intents);
    let next_intents: Vec<_> = next_intents
        .into_iter()
        .map(LayerRuntimeIntent::from_intent)
        .collect();
    let debug_records =
        collect_layer_debug_records(&previous_intents, &next_intents, previous_roots);
    LayerFrameCommit {
        intents: next_intents,
        debug_records,
    }
}

pub(super) fn apply_layer_frame_commit(runtime: &mut Runtime, commit: LayerFrameCommit) {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.layer_commit_apply");
    runtime.layers.intents = commit.intents;
    runtime.layers.debug_records = commit.debug_records;
}

pub(super) fn track_anchored_layer_roots(
    roots: &mut [Element],
    intents: &[LayerIntent],
    screen: Screen,
) {
    #[cfg(feature = "profile")]
    ::profiling::scope!("eui_neo.runtime.layer_anchor_tracking");
    for intent in intents.iter().filter(|intent| intent.open) {
        let Some(anchor) = intent.anchor.as_ref() else {
            continue;
        };
        let Some(current_anchor) = find_frame(roots, anchor) else {
            continue;
        };
        let boundary = intent
            .boundary
            .as_ref()
            .and_then(|boundary| find_frame(roots, boundary))
            .unwrap_or_else(|| LayoutRect::new(0.0, 0.0, screen.width, screen.height));
        let Some(root) = find_element_mut(roots, &intent.root) else {
            continue;
        };
        let next_position = resolve_layer_position_with_collision(
            current_anchor,
            boundary,
            root.frame.width,
            root.frame.height,
            intent.placement,
            intent.gap.max(0.0),
            intent.offset,
            intent.collision,
        );
        let dx = next_position[0] - root.frame.x;
        let dy = next_position[1] - root.frame.y;
        if dx.abs() > f32::EPSILON || dy.abs() > f32::EPSILON {
            translate_element_tree(root, dx, dy);
        }
    }
}

pub(super) fn collect_layer_debug_records(
    previous: &[LayerRuntimeIntent],
    current: &[LayerRuntimeIntent],
    previous_roots: &[crate::Element],
) -> Vec<LayerDebugRecord> {
    let mut records = Vec::new();
    for intent in current {
        let previous = previous.iter().find(|previous| previous.id == intent.id);
        records.push(layer_debug_record(
            intent,
            layer_anchor_source(intent, previous_roots),
            layer_action(previous, intent),
            intent.open,
        ));
    }

    for previous in previous
        .iter()
        .filter(|previous| previous.open && !current.iter().any(|intent| intent.id == previous.id))
    {
        records.push(layer_debug_record(
            previous,
            LayerAnchorSource::Removed,
            LayerLifecycleAction::Removed,
            false,
        ));
    }

    records.sort_by(|a, b| a.layer_id.cmp(&b.layer_id));
    records
}

fn layer_debug_record(
    intent: &LayerRuntimeIntent,
    anchor_source: LayerAnchorSource,
    action: LayerLifecycleAction,
    open: bool,
) -> LayerDebugRecord {
    LayerDebugRecord {
        layer_id: intent.id.clone(),
        owner_id: intent.owner.clone(),
        root_id: intent.root.clone(),
        anchor_id: intent.anchor.clone(),
        id: intent.id.as_str().to_string(),
        owner: intent.owner.as_str().to_string(),
        root: intent.root.as_str().to_string(),
        anchor: intent
            .anchor
            .as_ref()
            .map(|anchor| anchor.as_str().to_string()),
        fallback_anchor: intent.fallback_anchor,
        boundary: intent
            .boundary
            .as_ref()
            .map(|boundary| boundary.as_str().to_string()),
        anchor_source,
        open,
        kind: intent.kind,
        placement: intent.placement,
        size: intent.size.clone(),
        gap: intent.gap,
        offset: intent.offset,
        collision: intent.collision,
        z_index: intent.z_index,
        outside_click: intent.outside_click,
        action,
    }
}

pub(super) fn layer_pointer_policy(
    layers: &[LayerRuntimeIntent],
    roots: &[crate::Element],
    position: Option<[f32; 2]>,
) -> LayerPointerPolicy {
    let Some(position) = position else {
        return LayerPointerPolicy::default();
    };

    let mut order: SmallVec<[usize; 8]> = layers
        .iter()
        .enumerate()
        .filter_map(|(index, layer)| layer.open.then_some(index))
        .collect();
    order.sort_by_key(|&index| (layers[index].z_index, index));

    for index in order.into_iter().rev() {
        let layer = &layers[index];
        if point_hits_layer(layer, roots, position) {
            return LayerPointerPolicy {
                debug: Some(layer_pointer_debug_record(
                    LayerPointerAction::HitLayer,
                    Some(layer),
                    Some(layer),
                    layer.outside_click,
                )),
                ..LayerPointerPolicy::default()
            };
        }
        if matches!(
            layer.outside_click,
            OutsideClickPolicy::Block | OutsideClickPolicy::Close
        ) {
            let action = match layer.outside_click {
                OutsideClickPolicy::Close => LayerPointerAction::Dismissed,
                OutsideClickPolicy::Block => LayerPointerAction::Blocked,
                OutsideClickPolicy::Ignore => LayerPointerAction::PassThrough,
            };
            return LayerPointerPolicy {
                block_pointer: true,
                dismissal: matches!(layer.outside_click, OutsideClickPolicy::Close).then(|| {
                    LayerDismissal {
                        id: layer.id.clone(),
                        owner: layer.owner.clone(),
                        policy: layer.outside_click,
                    }
                }),
                debug: Some(layer_pointer_debug_record(
                    action,
                    Some(layer),
                    None,
                    layer.outside_click,
                )),
            };
        }
    }

    LayerPointerPolicy {
        debug: Some(layer_pointer_debug_record(
            LayerPointerAction::PassThrough,
            None,
            None,
            OutsideClickPolicy::Ignore,
        )),
        ..LayerPointerPolicy::default()
    }
}

fn layer_pointer_debug_record(
    action: LayerPointerAction,
    layer: Option<&LayerRuntimeIntent>,
    hit_layer: Option<&LayerRuntimeIntent>,
    policy: OutsideClickPolicy,
) -> LayerPointerDebugRecord {
    LayerPointerDebugRecord {
        action,
        layer_id: layer.map(|layer| layer.id.clone()),
        hit_layer_id: hit_layer.map(|layer| layer.id.clone()),
        layer: layer.map(|layer| layer.id.as_str().to_string()),
        hit_layer: hit_layer.map(|layer| layer.id.as_str().to_string()),
        policy,
    }
}

pub(super) fn layer_requests_keyboard_capture(layers: &[LayerRuntimeIntent]) -> bool {
    layers.iter().any(|layer| {
        layer.open
            && matches!(
                layer.outside_click,
                OutsideClickPolicy::Block | OutsideClickPolicy::Close
            )
    })
}

pub(super) fn layer_blocks_element_target(
    layers: &[LayerRuntimeIntent],
    roots: &[crate::Element],
    target_id: &NodeId,
) -> bool {
    let mut order: SmallVec<[usize; 8]> = layers
        .iter()
        .enumerate()
        .filter_map(|(index, layer)| layer.open.then_some(index))
        .collect();
    order.sort_by_key(|&index| (layers[index].z_index, index));

    for index in order.into_iter().rev() {
        let layer = &layers[index];
        if element_is_in_layer(layer, roots, target_id) {
            return false;
        }
        if matches!(
            layer.outside_click,
            OutsideClickPolicy::Block | OutsideClickPolicy::Close
        ) {
            return true;
        }
    }

    false
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct LayerPointerPolicy {
    pub(super) block_pointer: bool,
    pub(super) dismissal: Option<LayerDismissal>,
    pub(super) debug: Option<LayerPointerDebugRecord>,
}

fn layer_action(
    previous: Option<&LayerRuntimeIntent>,
    current: &LayerRuntimeIntent,
) -> LayerLifecycleAction {
    match (previous.map(|intent| intent.open), current.open) {
        (Some(true), true) => LayerLifecycleAction::Reused,
        (Some(true), false) => LayerLifecycleAction::Removed,
        (Some(false), true) | (None, true) => LayerLifecycleAction::Created,
        (Some(false), false) | (None, false) => LayerLifecycleAction::Closed,
    }
}

fn layer_anchor_source(
    intent: &LayerRuntimeIntent,
    previous_roots: &[crate::Element],
) -> LayerAnchorSource {
    if !intent.open {
        return LayerAnchorSource::Closed;
    }
    if let Some(anchor) = intent.anchor.as_ref() {
        if element_id_exists(previous_roots, anchor) {
            return LayerAnchorSource::PreviousFrame;
        }
        if intent.fallback_anchor.is_some() {
            return LayerAnchorSource::Fallback;
        }
        return LayerAnchorSource::Missing;
    }
    if intent.fallback_anchor.is_some() {
        return LayerAnchorSource::Fallback;
    }
    LayerAnchorSource::None
}

fn element_id_exists(elements: &[crate::Element], id: &NodeId) -> bool {
    elements
        .iter()
        .any(|element| element.id == id.as_str() || element_id_exists(&element.children, id))
}

fn find_frame(elements: &[Element], id: &NodeId) -> Option<LayoutRect> {
    elements.iter().find_map(|element| {
        if element.id == id.as_str() {
            Some(element.frame)
        } else {
            find_frame(&element.children, id)
        }
    })
}

fn find_element_mut<'a>(elements: &'a mut [Element], id: &NodeId) -> Option<&'a mut Element> {
    for element in elements {
        if element.id == id.as_str() {
            return Some(element);
        }
        if let Some(found) = find_element_mut(&mut element.children, id) {
            return Some(found);
        }
    }
    None
}

fn translate_element_tree(element: &mut Element, dx: f32, dy: f32) {
    element.frame.x += dx;
    element.frame.y += dy;
    for child in &mut element.children {
        translate_element_tree(child, dx, dy);
    }
}

fn resolve_layer_position_with_collision(
    anchor: LayoutRect,
    boundary: LayoutRect,
    width: f32,
    height: f32,
    placement: LayerPlacement,
    gap: f32,
    offset: [f32; 2],
    collision: LayerCollision,
) -> [f32; 2] {
    let mut placement = placement;
    if matches!(collision, LayerCollision::FlipShift) {
        placement = flipped_placement_for_boundary(anchor, boundary, width, height, placement, gap);
    }
    let mut position = layer_anchor_position(anchor, width, height, placement, gap, offset);
    if matches!(collision, LayerCollision::Shift | LayerCollision::FlipShift) {
        position[0] = clamp_axis(position[0], width, boundary.x, boundary.right());
        position[1] = clamp_axis(position[1], height, boundary.y, boundary.bottom());
    }
    position
}

fn layer_anchor_position(
    anchor: LayoutRect,
    width: f32,
    height: f32,
    placement: LayerPlacement,
    gap: f32,
    offset: [f32; 2],
) -> [f32; 2] {
    let [mut x, mut y] = match placement {
        LayerPlacement::BottomStart => [anchor.x, anchor.bottom() + gap],
        LayerPlacement::BottomEnd => [anchor.right() - width, anchor.bottom() + gap],
        LayerPlacement::TopStart => [anchor.x, anchor.y - height - gap],
        LayerPlacement::TopEnd => [anchor.right() - width, anchor.y - height - gap],
        LayerPlacement::RightStart => [anchor.right() + gap, anchor.y],
        LayerPlacement::RightEnd => [anchor.right() + gap, anchor.bottom() - height],
        LayerPlacement::LeftStart => [anchor.x - width - gap, anchor.y],
        LayerPlacement::LeftEnd => [anchor.x - width - gap, anchor.bottom() - height],
        LayerPlacement::Center => [
            anchor.x + (anchor.width - width) * 0.5,
            anchor.y + (anchor.height - height) * 0.5,
        ],
    };
    x += offset[0];
    y += offset[1];
    [x, y]
}

fn flipped_placement_for_boundary(
    anchor: LayoutRect,
    boundary: LayoutRect,
    width: f32,
    height: f32,
    placement: LayerPlacement,
    gap: f32,
) -> LayerPlacement {
    let below = boundary.bottom() - anchor.bottom() - gap;
    let above = anchor.y - boundary.y - gap;
    let right = boundary.right() - anchor.right() - gap;
    let left = anchor.x - boundary.x - gap;
    match placement {
        LayerPlacement::BottomStart if below < height && above > below => LayerPlacement::TopStart,
        LayerPlacement::BottomEnd if below < height && above > below => LayerPlacement::TopEnd,
        LayerPlacement::TopStart if above < height && below > above => LayerPlacement::BottomStart,
        LayerPlacement::TopEnd if above < height && below > above => LayerPlacement::BottomEnd,
        LayerPlacement::RightStart if right < width && left > right => LayerPlacement::LeftStart,
        LayerPlacement::RightEnd if right < width && left > right => LayerPlacement::LeftEnd,
        LayerPlacement::LeftStart if left < width && right > left => LayerPlacement::RightStart,
        LayerPlacement::LeftEnd if left < width && right > left => LayerPlacement::RightEnd,
        _ => placement,
    }
}

fn clamp_axis(value: f32, extent: f32, min: f32, max: f32) -> f32 {
    if max - min <= extent {
        min
    } else {
        value.clamp(min, max - extent)
    }
}

fn point_hits_layer(
    layer: &LayerRuntimeIntent,
    roots: &[crate::Element],
    position: [f32; 2],
) -> bool {
    roots
        .iter()
        .any(|root| root.id == layer.root.as_str() && element_contains_point(root, position))
}

fn element_is_in_layer(layer: &LayerRuntimeIntent, roots: &[crate::Element], id: &NodeId) -> bool {
    roots.iter().any(|root| {
        root.id == layer.root.as_str() && element_id_exists(std::slice::from_ref(root), id)
    })
}

fn element_contains_point(element: &crate::Element, position: [f32; 2]) -> bool {
    element.frame.contains(position)
        || element
            .children
            .iter()
            .any(|child| element_contains_point(child, position))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Element, ElementKind};

    #[test]
    fn pointer_inside_upper_layer_does_not_dismiss_lower_close_layer() {
        let layers = vec![
            runtime_intent("lower", 10, OutsideClickPolicy::Close),
            runtime_intent("upper", 20, OutsideClickPolicy::Block),
        ];
        let roots = vec![
            root("lower", LayoutRect::new(0.0, 0.0, 50.0, 50.0)),
            root("upper", LayoutRect::new(100.0, 0.0, 50.0, 50.0)),
        ];

        let policy = layer_pointer_policy(&layers, &roots, Some([110.0, 10.0]));

        assert_pointer_debug(
            &policy,
            LayerPointerAction::HitLayer,
            Some("upper"),
            Some("upper"),
            OutsideClickPolicy::Block,
        );
        assert!(!policy.block_pointer);
        assert!(policy.dismissal.is_none());
    }

    #[test]
    fn pointer_inside_upper_ignore_layer_does_not_dismiss_lower_close_layer() {
        let layers = vec![
            runtime_intent("lower", 10, OutsideClickPolicy::Close),
            runtime_intent("upper", 20, OutsideClickPolicy::Ignore),
        ];
        let roots = vec![
            root("lower", LayoutRect::new(0.0, 0.0, 50.0, 50.0)),
            root("upper", LayoutRect::new(100.0, 0.0, 50.0, 50.0)),
        ];

        let policy = layer_pointer_policy(&layers, &roots, Some([110.0, 10.0]));

        assert_pointer_debug(
            &policy,
            LayerPointerAction::HitLayer,
            Some("upper"),
            Some("upper"),
            OutsideClickPolicy::Ignore,
        );
        assert!(!policy.block_pointer);
        assert!(policy.dismissal.is_none());
    }

    #[test]
    fn outside_pointer_uses_topmost_blocking_layer_with_declaration_tie_break() {
        let layers = vec![
            runtime_intent("first", 10, OutsideClickPolicy::Close),
            runtime_intent("second", 10, OutsideClickPolicy::Close),
        ];
        let roots = vec![
            root("first", LayoutRect::new(0.0, 0.0, 50.0, 50.0)),
            root("second", LayoutRect::new(60.0, 0.0, 50.0, 50.0)),
        ];

        let policy = layer_pointer_policy(&layers, &roots, Some([120.0, 10.0]));

        assert!(policy.block_pointer);
        assert_pointer_debug(
            &policy,
            LayerPointerAction::Dismissed,
            Some("second"),
            None,
            OutsideClickPolicy::Close,
        );
        assert_eq!(
            policy.dismissal,
            Some(LayerDismissal {
                id: LayerId::new("second"),
                owner: NodeId::new("second-owner"),
                policy: OutsideClickPolicy::Close,
            })
        );
        let dismissal = policy
            .dismissal
            .clone()
            .expect("topmost close layer dismissal")
            .into_record();
        assert_eq!(dismissal.id, "second");
        assert_eq!(dismissal.owner, "second-owner");
        assert_eq!(dismissal.layer_id().as_str(), "second");
        assert_eq!(dismissal.owner_id().as_str(), "second-owner");
    }

    #[test]
    fn outside_pointer_records_blocking_decision_without_dismissal() {
        let layers = vec![runtime_intent("blocker", 10, OutsideClickPolicy::Block)];
        let roots = vec![root("blocker", LayoutRect::new(0.0, 0.0, 50.0, 50.0))];

        let policy = layer_pointer_policy(&layers, &roots, Some([60.0, 10.0]));

        assert!(policy.block_pointer);
        assert!(policy.dismissal.is_none());
        assert_pointer_debug(
            &policy,
            LayerPointerAction::Blocked,
            Some("blocker"),
            None,
            OutsideClickPolicy::Block,
        );
    }

    #[test]
    fn pointer_without_blocking_layers_records_passthrough_decision() {
        let layers = vec![runtime_intent("toast", 10, OutsideClickPolicy::Ignore)];
        let roots = vec![root("toast", LayoutRect::new(0.0, 0.0, 50.0, 50.0))];

        let policy = layer_pointer_policy(&layers, &roots, Some([60.0, 10.0]));

        assert!(!policy.block_pointer);
        assert!(policy.dismissal.is_none());
        assert_pointer_debug(
            &policy,
            LayerPointerAction::PassThrough,
            None,
            None,
            OutsideClickPolicy::Ignore,
        );
    }

    #[test]
    fn layer_keyboard_capture_follows_blocking_layers() {
        assert!(!layer_requests_keyboard_capture(&[intent(
            "toast",
            10,
            OutsideClickPolicy::Ignore,
        )
        .into_runtime()]));
        assert!(layer_requests_keyboard_capture(&[intent(
            "modal",
            10,
            OutsideClickPolicy::Block,
        )
        .into_runtime()]));
        let mut closed = runtime_intent("modal", 10, OutsideClickPolicy::Close);
        closed.open = false;
        assert!(!layer_requests_keyboard_capture(&[closed]));
    }

    #[test]
    fn layer_blocks_element_targets_outside_topmost_blocking_layer() {
        let layers = vec![runtime_intent("modal", 10, OutsideClickPolicy::Block)];
        let mut modal = root("modal", LayoutRect::new(0.0, 0.0, 100.0, 100.0));
        modal
            .children
            .push(root("modal.input", LayoutRect::new(10.0, 10.0, 20.0, 20.0)));
        let roots = vec![
            root("under.input", LayoutRect::new(120.0, 0.0, 20.0, 20.0)),
            modal,
        ];

        assert!(layer_blocks_element_target(
            &layers,
            &roots,
            &NodeId::new("under.input")
        ));
        assert!(!layer_blocks_element_target(
            &layers,
            &roots,
            &NodeId::new("modal.input")
        ));
    }

    #[test]
    fn layer_geometry_uses_explicit_root_identity() {
        let layers = vec![runtime_intent_with_root(
            "menu.layer",
            "menu.root",
            10,
            OutsideClickPolicy::Close,
        )];
        let mut layer_root = root("menu.root", LayoutRect::new(0.0, 0.0, 100.0, 100.0));
        layer_root.children.push(root(
            "menu.root.item",
            LayoutRect::new(10.0, 10.0, 20.0, 20.0),
        ));
        let roots = vec![
            root("menu.layer", LayoutRect::new(200.0, 0.0, 100.0, 100.0)),
            layer_root,
        ];

        let inside = layer_pointer_policy(&layers, &roots, Some([15.0, 15.0]));
        assert_pointer_debug(
            &inside,
            LayerPointerAction::HitLayer,
            Some("menu.layer"),
            Some("menu.layer"),
            OutsideClickPolicy::Close,
        );
        assert!(!inside.block_pointer);
        assert!(inside.dismissal.is_none());
        assert!(!layer_blocks_element_target(
            &layers,
            &roots,
            &NodeId::new("menu.root.item")
        ));

        let outside = layer_pointer_policy(&layers, &roots, Some([220.0, 10.0]));
        assert!(outside.block_pointer);
        assert_eq!(
            outside.dismissal.as_ref().map(LayerDismissal::target),
            Some(LayerId::new("menu.layer"))
        );
        assert_pointer_debug(
            &outside,
            LayerPointerAction::Dismissed,
            Some("menu.layer"),
            None,
            OutsideClickPolicy::Close,
        );
    }

    #[test]
    fn layer_debug_records_previous_frame_anchor_source() {
        let mut layer = runtime_intent("menu", 10, OutsideClickPolicy::Ignore);
        layer.anchor = Some(NodeId::new("anchor"));
        let roots = vec![root("anchor", LayoutRect::new(0.0, 0.0, 10.0, 10.0))];

        let records = collect_layer_debug_records(&[], &[layer], &roots);

        assert_eq!(records[0].anchor_source, LayerAnchorSource::PreviousFrame);
        assert_eq!(records[0].layer_id().as_str(), "menu");
        assert_eq!(records[0].owner_id().as_str(), "menu-owner");
        assert_eq!(records[0].root_id().as_str(), "menu");
        assert_eq!(records[0].anchor_id().map(NodeId::as_str), Some("anchor"));
    }

    #[test]
    fn layer_debug_records_fallback_and_missing_anchor_sources() {
        let mut fallback = runtime_intent("fallback", 10, OutsideClickPolicy::Ignore);
        fallback.anchor = Some(NodeId::new("missing"));
        fallback.fallback_anchor = Some(LayoutRect::new(1.0, 2.0, 3.0, 4.0));
        let mut missing = runtime_intent("missing", 20, OutsideClickPolicy::Ignore);
        missing.anchor = Some(NodeId::new("missing"));

        let records = collect_layer_debug_records(&[], &[fallback, missing], &[]);

        let fallback_record = records
            .iter()
            .find(|record| record.id == "fallback")
            .expect("fallback layer debug record");
        let missing_record = records
            .iter()
            .find(|record| record.id == "missing")
            .expect("missing layer debug record");
        assert_eq!(fallback_record.anchor_source, LayerAnchorSource::Fallback);
        assert_eq!(missing_record.anchor_source, LayerAnchorSource::Missing);
    }

    #[test]
    fn layer_debug_records_closed_and_removed_anchor_sources() {
        let mut closed = runtime_intent("closed", 10, OutsideClickPolicy::Ignore);
        closed.open = false;
        let previous = vec![runtime_intent("removed", 20, OutsideClickPolicy::Ignore)];

        let records = collect_layer_debug_records(&previous, &[closed], &[]);

        let closed_record = records
            .iter()
            .find(|record| record.id == "closed")
            .expect("closed layer debug record");
        let removed_record = records
            .iter()
            .find(|record| record.id == "removed")
            .expect("removed layer debug record");
        assert_eq!(closed_record.anchor_source, LayerAnchorSource::Closed);
        assert_eq!(removed_record.anchor_source, LayerAnchorSource::Removed);
    }

    #[test]
    fn prepare_layer_frame_commit_takes_previous_and_records_lifecycle() {
        let mut previous = vec![runtime_intent("menu", 10, OutsideClickPolicy::Close)];
        let next = vec![intent("menu", 10, OutsideClickPolicy::Close)];
        let roots = vec![root("anchor", LayoutRect::new(0.0, 0.0, 10.0, 10.0))];

        let commit = prepare_layer_frame_commit(&mut previous, next, &roots);

        assert!(previous.is_empty());
        assert_eq!(commit.intents.len(), 1);
        assert_eq!(commit.debug_records.len(), 1);
        assert_eq!(commit.debug_records[0].id, "menu");
        assert_eq!(commit.debug_records[0].layer_id().as_str(), "menu");
        assert_eq!(commit.debug_records[0].owner_id().as_str(), "menu-owner");
        assert_eq!(commit.debug_records[0].action, LayerLifecycleAction::Reused);
    }

    #[test]
    fn apply_layer_frame_commit_updates_runtime_layer_state() {
        let mut runtime = Runtime::new("page");
        let toast = runtime_intent("toast", 20, OutsideClickPolicy::Ignore);
        let commit = LayerFrameCommit {
            intents: vec![toast.clone()],
            debug_records: vec![layer_debug_record(
                &toast,
                LayerAnchorSource::None,
                LayerLifecycleAction::Created,
                true,
            )],
        };

        apply_layer_frame_commit(&mut runtime, commit);

        assert_eq!(runtime.layers.intents.len(), 1);
        assert_eq!(runtime.layers.intents[0].id.as_str(), "toast");
        assert_eq!(runtime.layers.debug_records.len(), 1);
        assert_eq!(runtime.layers.debug_records[0].id, "toast");
    }

    fn assert_pointer_debug(
        policy: &LayerPointerPolicy,
        action: LayerPointerAction,
        layer: Option<&str>,
        hit_layer: Option<&str>,
        outside_click: OutsideClickPolicy,
    ) {
        let debug = policy.debug.as_ref().expect("layer pointer debug record");
        assert_eq!(debug.action, action);
        assert_eq!(debug.layer.as_deref(), layer);
        assert_eq!(debug.hit_layer.as_deref(), hit_layer);
        assert_eq!(debug.layer_id().map(LayerId::as_str), layer);
        assert_eq!(debug.hit_layer_id().map(LayerId::as_str), hit_layer);
        assert_eq!(debug.policy, outside_click);
    }

    fn intent(id: &str, z_index: i32, outside_click: OutsideClickPolicy) -> LayerIntent {
        LayerIntent {
            id: LayerId::new(id),
            owner: NodeId::new(format!("{id}-owner")),
            root: NodeId::new(id),
            anchor: None,
            fallback_anchor: None,
            boundary: None,
            open: true,
            kind: LayerKind::Popover,
            placement: LayerPlacement::BottomStart,
            size: LayerSize::new(Size::Fixed(50.0), Size::Fixed(50.0)),
            gap: 0.0,
            offset: [0.0, 0.0],
            collision: LayerCollision::None,
            z_index,
            outside_click,
        }
    }

    trait IntoRuntimeIntent {
        fn into_runtime(self) -> LayerRuntimeIntent;
    }

    impl IntoRuntimeIntent for LayerIntent {
        fn into_runtime(self) -> LayerRuntimeIntent {
            LayerRuntimeIntent::from_intent(self)
        }
    }

    fn runtime_intent(
        id: &str,
        z_index: i32,
        outside_click: OutsideClickPolicy,
    ) -> LayerRuntimeIntent {
        intent(id, z_index, outside_click).into_runtime()
    }

    fn runtime_intent_with_root(
        id: &str,
        root: &str,
        z_index: i32,
        outside_click: OutsideClickPolicy,
    ) -> LayerRuntimeIntent {
        let mut intent = intent(id, z_index, outside_click);
        intent.root = NodeId::new(root);
        intent.into_runtime()
    }

    fn root(id: &str, frame: LayoutRect) -> Element {
        let mut element = Element::new(ElementKind::Stack, id);
        element.frame = frame;
        element
    }
}
