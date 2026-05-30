use super::*;

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
    pub id: String,
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

#[derive(Debug, Clone, PartialEq)]
pub struct LayerDebugRecord {
    pub id: String,
    pub owner: String,
    pub anchor: Option<String>,
    pub fallback_anchor: Option<LayoutRect>,
    pub open: bool,
    pub kind: LayerKind,
    pub placement: LayerPlacement,
    pub size: LayerSize,
    pub z_index: i32,
    pub outside_click: OutsideClickPolicy,
    pub action: LayerLifecycleAction,
}

pub(super) fn collect_layer_debug_records(
    previous: &[LayerIntent],
    current: &[LayerIntent],
) -> Vec<LayerDebugRecord> {
    let mut records = Vec::new();
    for intent in current {
        let previous = previous.iter().find(|previous| previous.id == intent.id);
        records.push(LayerDebugRecord {
            id: intent.id.clone(),
            owner: intent.owner.clone(),
            anchor: intent.anchor.clone(),
            fallback_anchor: intent.fallback_anchor,
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
            id: previous.id.clone(),
            owner: previous.owner.clone(),
            anchor: previous.anchor.clone(),
            fallback_anchor: previous.fallback_anchor,
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

fn layer_action(
    previous: Option<&LayerIntent>,
    current: &LayerIntent,
) -> LayerLifecycleAction {
    match (previous.map(|intent| intent.open), current.open) {
        (Some(true), true) => LayerLifecycleAction::Reused,
        (Some(true), false) => LayerLifecycleAction::Removed,
        (Some(false), true) | (None, true) => LayerLifecycleAction::Created,
        (Some(false), false) | (None, false) => LayerLifecycleAction::Closed,
    }
}
