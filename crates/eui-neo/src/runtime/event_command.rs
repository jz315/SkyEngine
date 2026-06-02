use super::*;
use crate::callbacks::{
    ClickCallbackId, ContextMenuCallbackId, DragCallbackId, FocusChangedCallbackId,
    LayerDismissCallbackId, PressCallbackId, ScrollCallbackId, TextInputCallbackId,
    TimerCallbackId, UiCallbacks,
};
use crate::DirtyFlags;

#[derive(Debug, Clone)]
pub(super) enum UiEventCommand {
    Press {
        target: NodeId,
        event: PointerEvent,
        frame: LayoutRect,
    },
    Click {
        target: NodeId,
    },
    ContextMenu {
        target: NodeId,
        event: PointerEvent,
        frame: LayoutRect,
    },
    Drag {
        target: NodeId,
        event: DragEvent,
    },
    TextInput {
        target: NodeId,
        event: KeyboardEvent,
    },
    Scroll {
        target: NodeId,
        event: ScrollEvent,
    },
    FocusChanged {
        target: NodeId,
        focused: bool,
    },
    LayerDismiss {
        target: LayerId,
    },
    Timer {
        target: NodeId,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct UiEventCommandReport {
    pub(super) command_count: usize,
    pub(super) callback_count: usize,
    pub(super) invalidation_count: usize,
    pub(super) pass_flags: PassFlags,
}

impl UiEventCommandReport {
    pub(super) fn changed(&self) -> bool {
        self.callback_count > 0
    }

    fn record_command(&mut self, callback: bool, invalidation: Option<&Invalidation>) {
        self.command_count += 1;
        if callback {
            self.callback_count += 1;
        }
        if let Some(invalidation) = invalidation {
            self.invalidation_count += 1;
            self.pass_flags.union(invalidation.pass_flags);
        }
    }
}

impl Runtime {
    pub(super) fn commit_frame_callbacks(&mut self, callbacks: UiCallbacks) {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo.runtime.commit_callbacks");
        self.input.callbacks = callbacks;
    }

    pub(super) fn execute_event_commands(
        &mut self,
        commands: Vec<UiEventCommand>,
    ) -> UiEventCommandReport {
        let mut report = UiEventCommandReport::default();
        for command in commands {
            match command {
                UiEventCommand::Press {
                    target,
                    event,
                    frame,
                } => {
                    let callback = if let Some(callback) = self
                        .input
                        .callbacks
                        .on_press
                        .get_mut(&PressCallbackId::node(&target))
                    {
                        callback(event, frame);
                        true
                    } else {
                        false
                    };
                    let invalidation = self.record_event_command_debug(
                        EventSource::Press,
                        EventTargetId::Node(target),
                        callback,
                        event_command_flags(callback),
                    );
                    report.record_command(callback, invalidation.as_ref());
                }
                UiEventCommand::Click { target } => {
                    let callback = if let Some(callback) = self
                        .input
                        .callbacks
                        .on_click
                        .get_mut(&ClickCallbackId::node(&target))
                    {
                        callback();
                        true
                    } else {
                        false
                    };
                    let invalidation = self.record_event_command_debug(
                        EventSource::Click,
                        EventTargetId::Node(target),
                        callback,
                        event_command_flags(callback),
                    );
                    report.record_command(callback, invalidation.as_ref());
                }
                UiEventCommand::ContextMenu {
                    target,
                    event,
                    frame,
                } => {
                    let callback = if let Some(callback) = self
                        .input
                        .callbacks
                        .on_context_menu
                        .get_mut(&ContextMenuCallbackId::node(&target))
                    {
                        callback(event, frame);
                        true
                    } else {
                        false
                    };
                    let invalidation = self.record_event_command_debug(
                        EventSource::ContextMenu,
                        EventTargetId::Node(target),
                        callback,
                        event_command_flags(callback),
                    );
                    report.record_command(callback, invalidation.as_ref());
                }
                UiEventCommand::Drag { target, event } => {
                    let callback = if let Some(callback) = self
                        .input
                        .callbacks
                        .on_drag
                        .get_mut(&DragCallbackId::node(&target))
                    {
                        callback(event);
                        true
                    } else {
                        false
                    };
                    let invalidation = self.record_event_command_debug(
                        EventSource::Drag,
                        EventTargetId::Node(target),
                        callback,
                        event_command_flags(callback),
                    );
                    report.record_command(callback, invalidation.as_ref());
                }
                UiEventCommand::TextInput { target, event } => {
                    let callback = if let Some(callback) = self
                        .input
                        .callbacks
                        .on_text_input
                        .get_mut(&TextInputCallbackId::node(&target))
                    {
                        callback(event);
                        true
                    } else {
                        false
                    };
                    let invalidation = self.record_event_command_debug(
                        EventSource::TextInput,
                        EventTargetId::Text(target),
                        callback,
                        event_command_flags(callback),
                    );
                    report.record_command(callback, invalidation.as_ref());
                }
                UiEventCommand::Scroll { target, event } => {
                    let callback = if let Some(callback) = self
                        .input
                        .callbacks
                        .on_scroll
                        .get_mut(&ScrollCallbackId::node(&target))
                    {
                        callback(event);
                        true
                    } else {
                        false
                    };
                    let invalidation = self.record_event_command_debug(
                        EventSource::Scroll,
                        EventTargetId::Scroll(target),
                        callback,
                        event_command_flags(callback),
                    );
                    report.record_command(callback, invalidation.as_ref());
                }
                UiEventCommand::FocusChanged { target, focused } => {
                    let callback = if let Some(callback) = self
                        .input
                        .callbacks
                        .on_focus_changed
                        .get_mut(&FocusChangedCallbackId::node(&target))
                    {
                        callback(focused);
                        true
                    } else {
                        false
                    };
                    let invalidation = self.record_event_command_debug(
                        EventSource::Focus,
                        EventTargetId::Focus(target),
                        callback,
                        event_command_flags(callback),
                    );
                    report.record_command(callback, invalidation.as_ref());
                }
                UiEventCommand::LayerDismiss { target } => {
                    let callback = if let Some(callback) = self
                        .input
                        .callbacks
                        .on_layer_dismiss
                        .get_mut(&LayerDismissCallbackId::layer(&target))
                    {
                        callback();
                        true
                    } else {
                        false
                    };
                    let invalidation = self.record_event_command_debug(
                        EventSource::LayerDismiss,
                        EventTargetId::Layer(target),
                        callback,
                        event_command_flags(callback),
                    );
                    report.record_command(callback, invalidation.as_ref());
                }
                UiEventCommand::Timer { target } => {
                    let callback = if let Some(callback) = self
                        .input
                        .callbacks
                        .on_timer
                        .get_mut(&TimerCallbackId::node(&target))
                    {
                        callback();
                        true
                    } else {
                        false
                    };
                    let invalidation =
                        self.record_timer_command_debug(EventTargetId::Node(target), callback);
                    report.record_command(callback, invalidation.as_ref());
                }
            }
        }
        report
    }

    fn record_event_command_debug(
        &mut self,
        source: EventSource,
        target: EventTargetId,
        callback: bool,
        flags: DirtyFlags,
    ) -> Option<Invalidation> {
        let invalidation = callback.then(|| Invalidation::event(target.clone(), source, flags));
        if let Some(invalidation) = invalidation.clone() {
            self.request_invalidation(invalidation);
        }
        let report_invalidation = invalidation.clone();
        let debug_source = EventDebugSource::Event(source);
        self.record_event_debug(EventDebugRecord {
            source: debug_source,
            raw_event: debug_source.raw_event(),
            target,
            command: debug_source.label(),
            callback,
            invalidation,
        });
        report_invalidation
    }

    fn record_timer_command_debug(
        &mut self,
        target: EventTargetId,
        callback: bool,
    ) -> Option<Invalidation> {
        let invalidation = callback
            .then(|| target.node_id().cloned())
            .flatten()
            .map(Invalidation::timer);
        if let Some(invalidation) = invalidation.clone() {
            self.request_invalidation(invalidation);
        }
        let report_invalidation = invalidation.clone();
        let debug_source = EventDebugSource::Timer(TimerSource::Timer);
        self.record_event_debug(EventDebugRecord {
            source: debug_source,
            raw_event: debug_source.raw_event(),
            target,
            command: debug_source.label(),
            callback,
            invalidation,
        });
        report_invalidation
    }
}

fn event_command_flags(callback: bool) -> DirtyFlags {
    if callback {
        DirtyFlags::COMPOSE | DirtyFlags::DRAW
    } else {
        DirtyFlags::empty()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn commit_frame_callbacks_replaces_runtime_callback_registry() {
        let mut runtime = Runtime::new("page");
        runtime.input.callbacks.on_click.insert(
            ClickCallbackId::new(NodeId::new("page.old")),
            Box::new(|| {}),
        );

        let mut callbacks = UiCallbacks::default();
        callbacks.on_timer.insert(
            TimerCallbackId::new(NodeId::new("page.new")),
            Box::new(|| {}),
        );

        runtime.commit_frame_callbacks(callbacks);

        assert!(runtime.input.callbacks.on_click.is_empty());
        assert!(runtime
            .input
            .callbacks
            .on_timer
            .contains_key(&TimerCallbackId::new(NodeId::new("page.new"))));
    }

    #[test]
    fn execute_event_commands_reports_commands_without_callbacks() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();

        let report = runtime.execute_event_commands(vec![UiEventCommand::Click {
            target: NodeId::new("page.missing"),
        }]);

        assert_eq!(report.command_count, 1);
        assert_eq!(report.callback_count, 0);
        assert_eq!(report.invalidation_count, 0);
        assert!(!report.changed());
        assert_eq!(report.pass_flags, PassFlags::default());
        assert!(!runtime.needs_render());
        assert!(!runtime.needs_compose());
        let events = &runtime.diagnostics().current_snapshot().events;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].command, "click");
        assert_eq!(
            events[0].source,
            EventDebugSource::Event(EventSource::Click)
        );
        assert!(!events[0].callback);
        assert!(events[0].invalidation.is_none());
    }

    #[test]
    fn execute_event_commands_reports_callback_invalidations() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();
        let clicked = Rc::new(Cell::new(false));
        let clicked_callback = clicked.clone();
        runtime.input.callbacks.on_click.insert(
            ClickCallbackId::new(NodeId::new("page.button")),
            Box::new(move || clicked_callback.set(true)),
        );

        let report = runtime.execute_event_commands(vec![UiEventCommand::Click {
            target: NodeId::new("page.button"),
        }]);

        assert!(clicked.get());
        assert_eq!(report.command_count, 1);
        assert_eq!(report.callback_count, 1);
        assert_eq!(report.invalidation_count, 1);
        assert!(report.changed());
        assert!(report.pass_flags.request_compose_ui);
        assert!(report.pass_flags.request_reconcile);
        assert!(report.pass_flags.request_draw);
        assert!(runtime.needs_render());
        assert!(runtime.needs_compose());
        let events = &runtime.diagnostics().current_snapshot().events;
        assert_eq!(events.len(), 1);
        assert!(events[0].callback);
        assert_eq!(
            events[0].source,
            EventDebugSource::Event(EventSource::Click)
        );
        let invalidation = events[0]
            .invalidation
            .as_ref()
            .expect("callback event should carry typed invalidation");
        assert_eq!(invalidation.target.kind(), "node");
        assert_eq!(invalidation.target.id(), "page.button");
        assert_eq!(
            invalidation.source,
            InvalidationSource::Event(EventSource::Click)
        );
    }

    #[test]
    fn execute_event_commands_encode_role_specific_targets() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();
        let clicked = Rc::new(Cell::new(0));
        let scrolled = Rc::new(Cell::new(0));
        let typed = Rc::new(Cell::new(0));
        let timer_fired = Rc::new(Cell::new(0));
        let clicked_callback = clicked.clone();
        let scrolled_callback = scrolled.clone();
        let typed_callback = typed.clone();
        let timer_callback = timer_fired.clone();
        runtime.input.callbacks.on_click.insert(
            ClickCallbackId::new(NodeId::new("page.shared")),
            Box::new(move || clicked_callback.set(clicked_callback.get() + 1)),
        );
        runtime.input.callbacks.on_scroll.insert(
            ScrollCallbackId::new(NodeId::new("page.shared")),
            Box::new(move |_| scrolled_callback.set(scrolled_callback.get() + 1)),
        );
        runtime.input.callbacks.on_text_input.insert(
            TextInputCallbackId::new(NodeId::new("page.shared")),
            Box::new(move |_| typed_callback.set(typed_callback.get() + 1)),
        );
        runtime.input.callbacks.on_timer.insert(
            TimerCallbackId::new(NodeId::new("page.shared")),
            Box::new(move || timer_callback.set(timer_callback.get() + 1)),
        );

        let report = runtime.execute_event_commands(vec![
            UiEventCommand::Click {
                target: NodeId::new("page.shared"),
            },
            UiEventCommand::Scroll {
                target: NodeId::new("page.shared"),
                event: ScrollEvent { x: 0.0, y: 1.0 },
            },
            UiEventCommand::TextInput {
                target: NodeId::new("page.shared"),
                event: KeyboardEvent {
                    text: "x".to_string(),
                    ..KeyboardEvent::default()
                },
            },
            UiEventCommand::Timer {
                target: NodeId::new("page.shared"),
            },
        ]);

        assert_eq!(clicked.get(), 1);
        assert_eq!(scrolled.get(), 1);
        assert_eq!(typed.get(), 1);
        assert_eq!(timer_fired.get(), 1);
        assert_eq!(report.command_count, 4);
        assert_eq!(report.callback_count, 4);
        assert_eq!(report.invalidation_count, 4);
        let events = &runtime.diagnostics().current_snapshot().events;
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].command, "click");
        assert_eq!(
            events[0].source,
            EventDebugSource::Event(EventSource::Click)
        );
        assert_eq!(events[0].target.role(), "node");
        assert!(events[0].callback);
        assert_eq!(events[1].command, "scroll");
        assert_eq!(
            events[1].source,
            EventDebugSource::Event(EventSource::Scroll)
        );
        assert_eq!(events[1].target.role(), "scroll");
        assert!(events[1].callback);
        assert_eq!(events[2].command, "text_input");
        assert_eq!(
            events[2].source,
            EventDebugSource::Event(EventSource::TextInput)
        );
        assert_eq!(events[2].target.role(), "text");
        assert!(events[2].callback);
        assert_eq!(events[3].command, "timer");
        assert_eq!(
            events[3].source,
            EventDebugSource::Timer(TimerSource::Timer)
        );
        assert_eq!(events[3].target.role(), "node");
        assert!(events[3].callback);
        assert!(events[3].invalidation.as_ref().is_some_and(|invalidation| {
            matches!(&invalidation.target, InvalidationTarget::Node(node) if node.as_str() == "page.shared")
                && invalidation.source == InvalidationSource::Timer(TimerSource::Timer)
        }));
    }
}
