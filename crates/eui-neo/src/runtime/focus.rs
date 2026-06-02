use super::layers::layer_blocks_element_target;
use super::reconcile::ElementIdSet;
use super::Runtime;

pub(super) fn sync_layer_blocked_focus(runtime: &mut Runtime, existing_ids: &ElementIdSet) -> bool {
    let mut changed = false;
    if runtime
        .layers
        .focus_restore
        .as_ref()
        .is_some_and(|id| !existing_ids.contains(id))
    {
        runtime.layers.focus_restore = None;
        changed = true;
    }

    if let Some(focused_id) = runtime.input.owners.keyboard_focus_node_id() {
        if layer_blocks_element_target(&runtime.layers.intents, &runtime.tree.roots, &focused_id) {
            if runtime.layers.focus_restore.is_none() {
                runtime.layers.focus_restore = Some(focused_id);
            }
            runtime.input.owners.set_keyboard_focus(None, false);
            return true;
        }
        return changed;
    }

    let Some(restore_id) = runtime.layers.focus_restore.clone() else {
        return changed;
    };
    if layer_blocks_element_target(&runtime.layers.intents, &runtime.tree.roots, &restore_id) {
        return changed;
    }
    if !existing_ids.contains(&restore_id) {
        runtime.layers.focus_restore = None;
        return true;
    }

    let text_enabled = runtime.input.callbacks.has_text_input(&restore_id);
    runtime
        .input
        .owners
        .set_keyboard_focus(Some(restore_id), text_enabled);
    runtime.layers.focus_restore = None;
    true
}

#[cfg(test)]
mod tests {
    use super::super::layers::LayerRuntimeIntent;
    use super::super::{LayerId, LayerIntent, LayerKind, LayerPlacement, LayerSize};
    use super::*;
    use crate::callbacks::TextInputCallbackId;
    use crate::runtime::NodeId;
    use crate::{Element, ElementKind, KeyboardEvent, LayoutRect, OutsideClickPolicy, Size};

    #[test]
    fn layer_blocked_focus_moves_focus_into_restore_slot() {
        let mut runtime = Runtime::new("page");
        runtime
            .input
            .owners
            .set_keyboard_focus(Some(NodeId::new("page.underlay.input")), true);
        runtime.layers.intents = vec![blocking_layer("page.popover")];
        runtime.tree.roots = vec![
            rect("page.underlay.input"),
            layer_root("page.popover", &[rect("page.popover.input")]),
        ];
        let existing_ids =
            element_ids(&["page.underlay.input", "page.popover", "page.popover.input"]);

        assert!(sync_layer_blocked_focus(&mut runtime, &existing_ids));

        assert_eq!(
            runtime.layers.focus_restore.as_ref().map(NodeId::as_str),
            Some("page.underlay.input")
        );
        assert_eq!(runtime.input.owners.keyboard_focus_id(), None);
        assert_eq!(runtime.input.owners.text_focus_id(), None);
        assert_eq!(runtime.input.owners.ime_owner_node_id(), None);
    }

    #[test]
    fn layer_unblocked_focus_restores_text_focus_when_callback_exists() {
        let mut runtime = Runtime::new("page");
        runtime.layers.focus_restore = Some(NodeId::new("page.underlay.input"));
        runtime.input.callbacks.on_text_input.insert(
            TextInputCallbackId::new(NodeId::new("page.underlay.input")),
            Box::new(|_: KeyboardEvent| {}),
        );
        runtime.tree.roots = vec![rect("page.underlay.input")];
        let existing_ids = element_ids(&["page.underlay.input"]);

        assert!(sync_layer_blocked_focus(&mut runtime, &existing_ids));

        assert_eq!(
            runtime.input.owners.keyboard_focus_id(),
            Some("page.underlay.input")
        );
        assert_eq!(
            runtime.input.owners.text_focus_id(),
            Some("page.underlay.input")
        );
        assert_eq!(
            runtime
                .input
                .owners
                .ime_owner_node_id()
                .as_ref()
                .map(NodeId::as_str),
            Some("page.underlay.input")
        );
        assert!(runtime.layers.focus_restore.is_none());
    }

    fn blocking_layer(id: &str) -> LayerRuntimeIntent {
        LayerRuntimeIntent::from_intent(LayerIntent {
            id: LayerId::new(id),
            owner: NodeId::new(format!("{id}.owner")),
            root: NodeId::new(id),
            anchor: None,
            fallback_anchor: None,
            boundary: None,
            open: true,
            kind: LayerKind::Popover,
            placement: LayerPlacement::BottomStart,
            size: LayerSize::new(Size::Fixed(100.0), Size::Fixed(100.0)),
            gap: 0.0,
            offset: [0.0, 0.0],
            collision: Default::default(),
            z_index: 10,
            outside_click: OutsideClickPolicy::Block,
        })
    }

    fn layer_root(id: &str, children: &[Element]) -> Element {
        let mut element = rect(id);
        element.frame = LayoutRect::new(0.0, 0.0, 100.0, 100.0);
        element.children = children.to_vec();
        element
    }

    fn rect(id: &str) -> Element {
        Element::new(ElementKind::Rect, id)
    }

    fn element_ids(ids: &[&str]) -> ElementIdSet {
        ids.iter().map(|id| NodeId::new(*id)).collect()
    }
}
