use super::*;
use crate::callbacks::{
    ClickCallbackId, ContextMenuCallbackId, DragCallbackId, FocusChangedCallbackId,
    LayerDismissCallbackId, PressCallbackId, ScrollCallbackId, TextInputCallbackId,
};
use crate::DirtyFlags;

#[derive(Debug, Clone)]
pub(super) enum UiEventCommand {
    Press {
        target: EventTargetId,
        event: PointerEvent,
        frame: LayoutRect,
    },
    Click {
        target: EventTargetId,
    },
    ContextMenu {
        target: EventTargetId,
        event: PointerEvent,
        frame: LayoutRect,
    },
    Drag {
        target: EventTargetId,
        event: DragEvent,
    },
    TextInput {
        target: EventTargetId,
        event: KeyboardEvent,
    },
    Scroll {
        target: EventTargetId,
        event: ScrollEvent,
    },
    FocusChanged {
        target: EventTargetId,
        focused: bool,
    },
    LayerDismiss {
        target: EventTargetId,
    },
}

impl Runtime {
    pub(super) fn execute_event_commands(&mut self, commands: Vec<UiEventCommand>) -> bool {
        let mut changed = false;
        for command in commands {
            match command {
                UiEventCommand::Press {
                    target,
                    event,
                    frame,
                } => {
                    let id = target.id();
                    if let Some(callback) = self
                        .input
                        .callbacks
                        .on_press
                        .get_mut(&PressCallbackId::new(id))
                    {
                        callback(event, frame);
                        self.record_event_command_debug(
                            "pointer",
                            target,
                            "press",
                            true,
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        );
                        changed = true;
                    } else {
                        self.record_event_command_debug(
                            "pointer",
                            target,
                            "press",
                            false,
                            DirtyFlags::empty(),
                        );
                    }
                }
                UiEventCommand::Click { target } => {
                    let id = target.id();
                    if let Some(callback) = self
                        .input
                        .callbacks
                        .on_click
                        .get_mut(&ClickCallbackId::new(id))
                    {
                        callback();
                        self.record_event_command_debug(
                            "pointer",
                            target,
                            "click",
                            true,
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        );
                        changed = true;
                    } else {
                        self.record_event_command_debug(
                            "pointer",
                            target,
                            "click",
                            false,
                            DirtyFlags::empty(),
                        );
                    }
                }
                UiEventCommand::ContextMenu {
                    target,
                    event,
                    frame,
                } => {
                    let id = target.id();
                    if let Some(callback) = self
                        .input
                        .callbacks
                        .on_context_menu
                        .get_mut(&ContextMenuCallbackId::new(id))
                    {
                        callback(event, frame);
                        self.record_event_command_debug(
                            "pointer",
                            target,
                            "context_menu",
                            true,
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        );
                        changed = true;
                    } else {
                        self.record_event_command_debug(
                            "pointer",
                            target,
                            "context_menu",
                            false,
                            DirtyFlags::empty(),
                        );
                    }
                }
                UiEventCommand::Drag { target, event } => {
                    let id = target.id();
                    if let Some(callback) = self
                        .input
                        .callbacks
                        .on_drag
                        .get_mut(&DragCallbackId::new(id))
                    {
                        callback(event);
                        self.record_event_command_debug(
                            "pointer",
                            target,
                            "drag",
                            true,
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        );
                        changed = true;
                    } else {
                        self.record_event_command_debug(
                            "pointer",
                            target,
                            "drag",
                            false,
                            DirtyFlags::empty(),
                        );
                    }
                }
                UiEventCommand::TextInput { target, event } => {
                    let id = target.id();
                    if let Some(callback) = self
                        .input
                        .callbacks
                        .on_text_input
                        .get_mut(&TextInputCallbackId::new(id))
                    {
                        callback(event);
                        self.record_event_command_debug(
                            "keyboard",
                            target,
                            "text_input",
                            true,
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        );
                        changed = true;
                    } else {
                        self.record_event_command_debug(
                            "keyboard",
                            target,
                            "text_input",
                            false,
                            DirtyFlags::empty(),
                        );
                    }
                }
                UiEventCommand::Scroll { target, event } => {
                    let id = target.id();
                    if let Some(callback) = self
                        .input
                        .callbacks
                        .on_scroll
                        .get_mut(&ScrollCallbackId::new(id))
                    {
                        callback(event);
                        self.record_event_command_debug(
                            "scroll",
                            target,
                            "scroll",
                            true,
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        );
                        changed = true;
                    } else {
                        self.record_event_command_debug(
                            "scroll",
                            target,
                            "scroll",
                            false,
                            DirtyFlags::empty(),
                        );
                    }
                }
                UiEventCommand::FocusChanged { target, focused } => {
                    let id = target.id();
                    if let Some(callback) = self
                        .input
                        .callbacks
                        .on_focus_changed
                        .get_mut(&FocusChangedCallbackId::new(id))
                    {
                        callback(focused);
                        self.record_event_command_debug(
                            "focus",
                            target,
                            "focus",
                            true,
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        );
                        changed = true;
                    } else {
                        self.record_event_command_debug(
                            "focus",
                            target,
                            "focus",
                            false,
                            DirtyFlags::empty(),
                        );
                    }
                }
                UiEventCommand::LayerDismiss { target } => {
                    let id = target.id();
                    if let Some(callback) = self
                        .input
                        .callbacks
                        .on_layer_dismiss
                        .get_mut(&LayerDismissCallbackId::new(id))
                    {
                        callback();
                        self.record_event_command_debug(
                            "layer",
                            target,
                            "dismiss",
                            true,
                            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
                        );
                        changed = true;
                    } else {
                        self.record_event_command_debug(
                            "layer",
                            target,
                            "dismiss",
                            false,
                            DirtyFlags::empty(),
                        );
                    }
                }
            }
        }
        if changed {
            self.mark_compose_dirty();
        }
        changed
    }

    fn record_event_command_debug(
        &mut self,
        raw_event: &'static str,
        target: EventTargetId,
        command: &'static str,
        callback: bool,
        flags: DirtyFlags,
    ) {
        let invalidation = callback.then(|| Invalidation::event(target.clone(), command, flags));
        if let Some(invalidation) = invalidation.clone() {
            self.record_invalidation(invalidation);
        }
        self.record_event_debug(EventDebugRecord {
            raw_event,
            target,
            command,
            callback,
            invalidation,
        });
    }
}
