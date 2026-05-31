use smallvec::SmallVec;

use crate::{LayoutRect, Size};

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
    pub owner: String,
    pub anchor: Option<String>,
    pub fallback_anchor: Option<LayoutRect>,
    pub open: bool,
    pub kind: LayerKind,
    pub placement: LayerPlacement,
    pub size: LayerSize,
    pub z_index: i32,
    pub outside_click: OutsideClickPolicy,
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
    pub id: String,
    pub owner: String,
    pub anchor: Option<String>,
    pub fallback_anchor: Option<LayoutRect>,
    pub anchor_source: LayerAnchorSource,
    pub open: bool,
    pub kind: LayerKind,
    pub placement: LayerPlacement,
    pub size: LayerSize,
    pub z_index: i32,
    pub outside_click: OutsideClickPolicy,
    pub action: LayerLifecycleAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerDismissalRecord {
    pub id: String,
    pub owner: String,
    pub policy: OutsideClickPolicy,
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
    pub action: LayerPointerAction,
    pub layer: Option<String>,
    pub hit_layer: Option<String>,
    pub policy: OutsideClickPolicy,
}

pub(super) fn collect_layer_debug_records(
    previous: &[LayerIntent],
    current: &[LayerIntent],
    previous_roots: &[crate::Element],
) -> Vec<LayerDebugRecord> {
    let mut records = Vec::new();
    for intent in current {
        let previous = previous.iter().find(|previous| previous.id == intent.id);
        records.push(LayerDebugRecord {
            id: intent.id.as_str().to_string(),
            owner: intent.owner.clone(),
            anchor: intent.anchor.clone(),
            fallback_anchor: intent.fallback_anchor,
            anchor_source: layer_anchor_source(intent, previous_roots),
            open: intent.open,
            kind: intent.kind,
            placement: intent.placement,
            size: intent.size.clone(),
            z_index: intent.z_index,
            outside_click: intent.outside_click,
            action: layer_action(previous, intent),
        });
    }

    for previous in previous
        .iter()
        .filter(|previous| previous.open && !current.iter().any(|intent| intent.id == previous.id))
    {
        records.push(LayerDebugRecord {
            id: previous.id.as_str().to_string(),
            owner: previous.owner.clone(),
            anchor: previous.anchor.clone(),
            fallback_anchor: previous.fallback_anchor,
            anchor_source: LayerAnchorSource::Removed,
            open: false,
            kind: previous.kind,
            placement: previous.placement,
            size: previous.size.clone(),
            z_index: previous.z_index,
            outside_click: previous.outside_click,
            action: LayerLifecycleAction::Removed,
        });
    }

    records.sort_by(|a, b| a.id.cmp(&b.id));
    records
}

pub(super) fn layer_pointer_policy(
    layers: &[LayerIntent],
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
                debug: Some(LayerPointerDebugRecord {
                    action: LayerPointerAction::HitLayer,
                    layer: Some(layer.id.as_str().to_string()),
                    hit_layer: Some(layer.id.as_str().to_string()),
                    policy: layer.outside_click,
                }),
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
                    LayerDismissalRecord {
                        id: layer.id.as_str().to_string(),
                        owner: layer.owner.clone(),
                        policy: layer.outside_click,
                    }
                }),
                debug: Some(LayerPointerDebugRecord {
                    action,
                    layer: Some(layer.id.as_str().to_string()),
                    hit_layer: None,
                    policy: layer.outside_click,
                }),
            };
        }
    }

    LayerPointerPolicy {
        debug: Some(LayerPointerDebugRecord {
            action: LayerPointerAction::PassThrough,
            layer: None,
            hit_layer: None,
            policy: OutsideClickPolicy::Ignore,
        }),
        ..LayerPointerPolicy::default()
    }
}

pub(super) fn layer_requests_keyboard_capture(layers: &[LayerIntent]) -> bool {
    layers.iter().any(|layer| {
        layer.open
            && matches!(
                layer.outside_click,
                OutsideClickPolicy::Block | OutsideClickPolicy::Close
            )
    })
}

pub(super) fn layer_blocks_element_target(
    layers: &[LayerIntent],
    roots: &[crate::Element],
    target_id: &str,
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
    pub(super) dismissal: Option<LayerDismissalRecord>,
    pub(super) debug: Option<LayerPointerDebugRecord>,
}

fn layer_action(previous: Option<&LayerIntent>, current: &LayerIntent) -> LayerLifecycleAction {
    match (previous.map(|intent| intent.open), current.open) {
        (Some(true), true) => LayerLifecycleAction::Reused,
        (Some(true), false) => LayerLifecycleAction::Removed,
        (Some(false), true) | (None, true) => LayerLifecycleAction::Created,
        (Some(false), false) | (None, false) => LayerLifecycleAction::Closed,
    }
}

fn layer_anchor_source(
    intent: &LayerIntent,
    previous_roots: &[crate::Element],
) -> LayerAnchorSource {
    if !intent.open {
        return LayerAnchorSource::Closed;
    }
    if let Some(anchor) = intent.anchor.as_deref() {
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

fn element_id_exists(elements: &[crate::Element], id: &str) -> bool {
    elements
        .iter()
        .any(|element| element.id == id || element_id_exists(&element.children, id))
}

fn point_hits_layer(layer: &LayerIntent, roots: &[crate::Element], position: [f32; 2]) -> bool {
    roots
        .iter()
        .any(|root| root.id == layer.id.as_str() && element_contains_point(root, position))
}

fn element_is_in_layer(layer: &LayerIntent, roots: &[crate::Element], id: &str) -> bool {
    roots.iter().any(|root| {
        root.id == layer.id.as_str() && element_id_exists(std::slice::from_ref(root), id)
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
            intent("lower", 10, OutsideClickPolicy::Close),
            intent("upper", 20, OutsideClickPolicy::Block),
        ];
        let roots = vec![
            root("lower", LayoutRect::new(0.0, 0.0, 50.0, 50.0)),
            root("upper", LayoutRect::new(100.0, 0.0, 50.0, 50.0)),
        ];

        let policy = layer_pointer_policy(&layers, &roots, Some([110.0, 10.0]));

        assert_eq!(
            policy.debug,
            Some(LayerPointerDebugRecord {
                action: LayerPointerAction::HitLayer,
                layer: Some("upper".to_string()),
                hit_layer: Some("upper".to_string()),
                policy: OutsideClickPolicy::Block,
            })
        );
        assert!(!policy.block_pointer);
        assert!(policy.dismissal.is_none());
    }

    #[test]
    fn pointer_inside_upper_ignore_layer_does_not_dismiss_lower_close_layer() {
        let layers = vec![
            intent("lower", 10, OutsideClickPolicy::Close),
            intent("upper", 20, OutsideClickPolicy::Ignore),
        ];
        let roots = vec![
            root("lower", LayoutRect::new(0.0, 0.0, 50.0, 50.0)),
            root("upper", LayoutRect::new(100.0, 0.0, 50.0, 50.0)),
        ];

        let policy = layer_pointer_policy(&layers, &roots, Some([110.0, 10.0]));

        assert_eq!(
            policy.debug,
            Some(LayerPointerDebugRecord {
                action: LayerPointerAction::HitLayer,
                layer: Some("upper".to_string()),
                hit_layer: Some("upper".to_string()),
                policy: OutsideClickPolicy::Ignore,
            })
        );
        assert!(!policy.block_pointer);
        assert!(policy.dismissal.is_none());
    }

    #[test]
    fn outside_pointer_uses_topmost_blocking_layer_with_declaration_tie_break() {
        let layers = vec![
            intent("first", 10, OutsideClickPolicy::Close),
            intent("second", 10, OutsideClickPolicy::Close),
        ];
        let roots = vec![
            root("first", LayoutRect::new(0.0, 0.0, 50.0, 50.0)),
            root("second", LayoutRect::new(60.0, 0.0, 50.0, 50.0)),
        ];

        let policy = layer_pointer_policy(&layers, &roots, Some([120.0, 10.0]));

        assert!(policy.block_pointer);
        assert_eq!(
            policy.debug,
            Some(LayerPointerDebugRecord {
                action: LayerPointerAction::Dismissed,
                layer: Some("second".to_string()),
                hit_layer: None,
                policy: OutsideClickPolicy::Close,
            })
        );
        assert_eq!(
            policy.dismissal,
            Some(LayerDismissalRecord {
                id: "second".to_string(),
                owner: "second-owner".to_string(),
                policy: OutsideClickPolicy::Close,
            })
        );
    }

    #[test]
    fn outside_pointer_records_blocking_decision_without_dismissal() {
        let layers = vec![intent("blocker", 10, OutsideClickPolicy::Block)];
        let roots = vec![root("blocker", LayoutRect::new(0.0, 0.0, 50.0, 50.0))];

        let policy = layer_pointer_policy(&layers, &roots, Some([60.0, 10.0]));

        assert!(policy.block_pointer);
        assert!(policy.dismissal.is_none());
        assert_eq!(
            policy.debug,
            Some(LayerPointerDebugRecord {
                action: LayerPointerAction::Blocked,
                layer: Some("blocker".to_string()),
                hit_layer: None,
                policy: OutsideClickPolicy::Block,
            })
        );
    }

    #[test]
    fn pointer_without_blocking_layers_records_passthrough_decision() {
        let layers = vec![intent("toast", 10, OutsideClickPolicy::Ignore)];
        let roots = vec![root("toast", LayoutRect::new(0.0, 0.0, 50.0, 50.0))];

        let policy = layer_pointer_policy(&layers, &roots, Some([60.0, 10.0]));

        assert!(!policy.block_pointer);
        assert!(policy.dismissal.is_none());
        assert_eq!(
            policy.debug,
            Some(LayerPointerDebugRecord {
                action: LayerPointerAction::PassThrough,
                layer: None,
                hit_layer: None,
                policy: OutsideClickPolicy::Ignore,
            })
        );
    }

    #[test]
    fn layer_keyboard_capture_follows_blocking_layers() {
        assert!(!layer_requests_keyboard_capture(&[intent(
            "toast",
            10,
            OutsideClickPolicy::Ignore,
        )]));
        assert!(layer_requests_keyboard_capture(&[intent(
            "modal",
            10,
            OutsideClickPolicy::Block,
        )]));
        let mut closed = intent("modal", 10, OutsideClickPolicy::Close);
        closed.open = false;
        assert!(!layer_requests_keyboard_capture(&[closed]));
    }

    #[test]
    fn layer_blocks_element_targets_outside_topmost_blocking_layer() {
        let layers = vec![intent("modal", 10, OutsideClickPolicy::Block)];
        let mut modal = root("modal", LayoutRect::new(0.0, 0.0, 100.0, 100.0));
        modal
            .children
            .push(root("modal.input", LayoutRect::new(10.0, 10.0, 20.0, 20.0)));
        let roots = vec![
            root("under.input", LayoutRect::new(120.0, 0.0, 20.0, 20.0)),
            modal,
        ];

        assert!(layer_blocks_element_target(&layers, &roots, "under.input"));
        assert!(!layer_blocks_element_target(&layers, &roots, "modal.input"));
    }

    #[test]
    fn layer_debug_records_previous_frame_anchor_source() {
        let mut layer = intent("menu", 10, OutsideClickPolicy::Ignore);
        layer.anchor = Some("anchor".to_string());
        let roots = vec![root("anchor", LayoutRect::new(0.0, 0.0, 10.0, 10.0))];

        let records = collect_layer_debug_records(&[], &[layer], &roots);

        assert_eq!(records[0].anchor_source, LayerAnchorSource::PreviousFrame);
    }

    #[test]
    fn layer_debug_records_fallback_and_missing_anchor_sources() {
        let mut fallback = intent("fallback", 10, OutsideClickPolicy::Ignore);
        fallback.anchor = Some("missing".to_string());
        fallback.fallback_anchor = Some(LayoutRect::new(1.0, 2.0, 3.0, 4.0));
        let mut missing = intent("missing", 20, OutsideClickPolicy::Ignore);
        missing.anchor = Some("missing".to_string());

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
        let mut closed = intent("closed", 10, OutsideClickPolicy::Ignore);
        closed.open = false;
        let previous = vec![intent("removed", 20, OutsideClickPolicy::Ignore)];

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

    fn intent(id: &str, z_index: i32, outside_click: OutsideClickPolicy) -> LayerIntent {
        LayerIntent {
            id: LayerId::new(id),
            owner: format!("{id}-owner"),
            anchor: None,
            fallback_anchor: None,
            open: true,
            kind: LayerKind::Popover,
            placement: LayerPlacement::BottomStart,
            size: LayerSize::new(Size::Fixed(50.0), Size::Fixed(50.0)),
            z_index,
            outside_click,
        }
    }

    fn root(id: &str, frame: LayoutRect) -> Element {
        let mut element = Element::new(ElementKind::Stack, id);
        element.frame = frame;
        element
    }
}
