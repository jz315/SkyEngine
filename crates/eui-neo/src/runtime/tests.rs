use super::Color;
use super::DirtyReason;
use super::EventTargetId;
use super::FullLayoutReason;
use super::InvalidationSource;
use super::InvalidationTarget;
use super::LayoutMode;
use super::NodeId;
use super::RetainedComposeAction;
use super::RetainedComposeReason;
use super::RetainedRoot;
use super::Runtime;
use super::ScopeId;
use crate::expert::DirtyInput;
use crate::expert::{UiDrawCommand, UiRectDraw};
use crate::test_support::{compose, compose_incremental_dirty};
use crate::widgets::{button, panel, text};
use crate::{
    Align, AnimProperty, Ease, Element, ElementKind, FontRef, FrameInput, HorizontalAlign,
    KeyboardEvent, LayoutRect, PointerEvent, Screen, ScrollEvent, Size, State, TextMeasure,
    TextMeasureRequest, TextSystem, Transform, Transition,
};
use crate::{DirtyFlags, ResourceDirty, SignalKey};
use rustc_hash::FxHashMap;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

fn dirty_scope(id: impl Into<String>) -> DirtyInput {
    DirtyInput::new(id, DirtyFlags::COMPOSE | DirtyFlags::DRAW)
}

#[test]
fn runtime_composes_and_lays_out_tree() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 800.0, 600.0, |ui, screen| {
        ui.stack("root")
            .size(screen.width, screen.height)
            .content(|ui| {
                ui.text("title").text("Hello").font_size(20.0).build();
            });
    });

    let root = runtime.diagnostics().find("root").unwrap();
    assert_eq!(root.frame.width, 800.0);
    assert_eq!(root.frame.height, 600.0);
    assert!(runtime.diagnostics().find("title").is_some());
    assert!(runtime.needs_render());
    assert!(runtime.full_redraw());
}

#[test]
fn refresh_scope_roots_updates_from_current_tree_by_id() {
    let mut root = Element::new(ElementKind::Stack, "page.root");
    let mut child = Element::new(ElementKind::Stack, "page.child");
    child.frame = LayoutRect::new(10.0, 20.0, 30.0, 40.0);
    let mut grandchild = Element::new(ElementKind::Rect, "page.grandchild");
    grandchild.frame = LayoutRect::new(11.0, 22.0, 33.0, 44.0);
    child.children.push(grandchild);
    root.children.push(child);

    let mut stale_child = Element::new(ElementKind::Stack, "page.child");
    stale_child.frame = LayoutRect::new(0.0, 0.0, 1.0, 1.0);
    stale_child
        .children
        .push(Element::new(ElementKind::Rect, "page.grandchild"));
    let mut scope_roots = FxHashMap::default();
    scope_roots.insert(
        ScopeId::new("page.child"),
        RetainedRoot::from_elements(&[stale_child]),
    );

    super::refresh_scope_roots_from_tree(&mut scope_roots, &[root]);

    assert_eq!(
        scope_roots["page.child"][0].frame,
        LayoutRect::new(10.0, 20.0, 30.0, 40.0)
    );
    assert_eq!(
        scope_roots["page.child"][0].children[0].frame,
        LayoutRect::new(11.0, 22.0, 33.0, 44.0)
    );
}

#[test]
fn font_source_reaches_draw_list() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 200.0, 80.0, |ui, _| {
        ui.text("title")
            .text("Hello")
            .font_source("ui/fonts/title.ttf")
            .build();
    });

    let draw = runtime.draw_list();
    let text = draw
        .commands()
        .iter()
        .find_map(|command| match command {
            UiDrawCommand::Text(text) => Some(text),
            _ => None,
        })
        .expect("text draw should exist");
    assert_eq!(text.font, FontRef::source("ui/fonts/title.ttf"));
}

#[test]
fn injected_text_system_controls_layout_measurement() {
    #[derive(Default)]
    struct FixedTextSystem;

    impl TextSystem for FixedTextSystem {
        fn register_font(&mut self, _font: &FontRef, _bytes: &[u8]) {}

        fn measure(&mut self, _request: TextMeasureRequest<'_>) -> TextMeasure {
            TextMeasure {
                width: 77.0,
                height: 19.0,
            }
        }
    }

    let mut runtime = Runtime::with_text_system("demo", FixedTextSystem);
    compose(&mut runtime, 200.0, 80.0, |ui, _| {
        ui.text("title")
            .text("Hello")
            .size(Size::wrap_content(), Size::wrap_content())
            .build();
    });

    let title = runtime
        .diagnostics()
        .find("title")
        .expect("title should exist");
    assert_eq!(title.frame.width, 77.0);
    assert_eq!(title.frame.height, 19.0);
}

#[test]
fn frame_updates_input_composes_and_returns_draw_list() {
    let mut runtime = Runtime::new("demo");
    let result = runtime.frame(
        FrameInput::new(Screen::new(320.0, 180.0), 1.0 / 60.0),
        |ui, _| {
            ui.rect("root").size(64.0, 32.0).build();
            42
        },
    );

    assert_eq!(result.value, 42);
    assert_eq!(result.frame.screen.width, 320.0);
    assert!(!result.frame.draw_list().is_empty());
    assert!(result.frame.needs_render);
}

#[test]
fn scoped_compose_rebuilds_dirty_scope_and_reuses_clean_sibling_with_callbacks() {
    #[derive(Default)]
    struct AppModel {
        selected: i32,
    }

    let state = State::new(AppModel::default());
    let left_builds = Rc::new(Cell::new(0));
    let right_builds = Rc::new(Cell::new(0));
    let right_clicks = Rc::new(Cell::new(0));
    let mut runtime = Runtime::new("page");

    let compose = |runtime: &mut Runtime, dirty: Vec<DirtyInput>| {
        let state = state.clone();
        let left_builds = left_builds.clone();
        let right_builds = right_builds.clone();
        let right_clicks = right_clicks.clone();
        compose_incremental_dirty(runtime, 240.0, 80.0, dirty, move |ui, _| {
            ui.row("root").size(240.0, 40.0).content(|ui| {
                ui.retained_scope("left", |ui| {
                    left_builds.set(left_builds.get() + 1);
                    let selected = state
                        .signal(
                            "selected",
                            |state| state.selected,
                            |state, value| state.selected = value,
                        )
                        .watch(ui);
                    ui.text("left.label")
                        .size(100.0, 40.0)
                        .text(format!("left {selected}"))
                        .build();
                });
                ui.retained_scope("right", |ui| {
                    right_builds.set(right_builds.get() + 1);
                    let right_clicks = right_clicks.clone();
                    button(ui, "right.button")
                        .size(100.0, 40.0)
                        .text("right")
                        .on_click(move || right_clicks.set(right_clicks.get() + 1))
                        .build();
                });
            });
        });
    };

    compose(&mut runtime, Vec::new());
    assert_eq!(left_builds.get(), 1);
    assert_eq!(right_builds.get(), 1);

    let selected = state.signal(
        "selected",
        |state| state.selected,
        |state, value| state.selected = value,
    );
    selected.set(1);
    compose(&mut runtime, state.take_dirty());

    assert_eq!(left_builds.get(), 2);
    assert_eq!(right_builds.get(), 1);
    assert_eq!(
        runtime.diagnostics().find("left.label").unwrap().text,
        "left 1"
    );
    assert!(runtime.diagnostics().find("right.button.bg").is_some());
    assert!(runtime.retained_compose_stats().built >= 1);
    assert!(runtime.retained_compose_stats().reused >= 1);
    assert!(runtime.retained_compose_stats().partial_layout);
    assert!(!runtime.retained_compose_stats().full_layout);
    let debug_snapshot = runtime.diagnostics().committed_snapshot();
    let right_reuse = debug_snapshot
        .scope_compose
        .iter()
        .find(|record| {
            record.id.as_str() == "page.right" && record.action == RetainedComposeAction::Reused
        })
        .expect("right scope should be reused");
    assert_eq!(right_reuse.callback_transfers.click, 1);
    assert_eq!(right_reuse.callback_transfers.total(), 1);

    let frame = runtime.diagnostics().find("right.button.bg").unwrap().frame;
    runtime.update_pointer(PointerEvent::pressed_at(frame.x + 1.0, frame.y + 1.0));
    runtime.update_pointer(PointerEvent::released_at(frame.x + 1.0, frame.y + 1.0));
    assert_eq!(right_clicks.get(), 1);
}

#[test]
fn rebuilt_scope_drops_stale_signal_dependencies() {
    #[derive(Default)]
    struct AppModel {
        use_a: bool,
        a: i32,
        b: i32,
    }

    let state = State::new(AppModel {
        use_a: true,
        ..AppModel::default()
    });
    let builds = Rc::new(Cell::new(0));
    let mut runtime = Runtime::new("page");
    let compose = |runtime: &mut Runtime, dirty: Vec<DirtyInput>| {
        let state = state.clone();
        let builds = builds.clone();
        compose_incremental_dirty(runtime, 240.0, 80.0, dirty, move |ui, _| {
            ui.retained_scope("panel", |ui| {
                builds.set(builds.get() + 1);
                let use_a = state
                    .signal(
                        "use_a",
                        |state| state.use_a,
                        |state, value| state.use_a = value,
                    )
                    .watch(ui);
                let value = if use_a {
                    state
                        .signal("a", |state| state.a, |state, value| state.a = value)
                        .watch(ui)
                } else {
                    state
                        .signal("b", |state| state.b, |state, value| state.b = value)
                        .watch(ui)
                };
                ui.text("panel.label")
                    .size(120.0, 40.0)
                    .text(format!("value {value}"))
                    .build();
            });
        });
    };

    compose(&mut runtime, Vec::new());
    assert_eq!(builds.get(), 1);

    state
        .signal(
            "use_a",
            |state| state.use_a,
            |state, value| state.use_a = value,
        )
        .set(false);
    compose(&mut runtime, state.take_dirty());
    assert_eq!(builds.get(), 2);
    assert_eq!(
        runtime.diagnostics().find("panel.label").unwrap().text,
        "value 0"
    );

    state
        .signal("a", |state| state.a, |state, value| state.a = value)
        .set(1);
    assert!(
        state.take_dirty().is_empty(),
        "rebuilt panel should no longer subscribe to signal a"
    );

    state
        .signal("b", |state| state.b, |state, value| state.b = value)
        .set(2);
    let dirty = state.take_dirty();
    assert_eq!(dirty.len(), 1);
    assert_eq!(dirty[0].id(), "page.panel");
    assert_eq!(dirty[0].source(), Some("b"));
}

#[test]
fn removed_scope_drops_signal_dependencies() {
    #[derive(Default)]
    struct AppModel {
        show_child: bool,
        child_value: i32,
    }

    let state = State::new(AppModel {
        show_child: true,
        ..AppModel::default()
    });
    let mut runtime = Runtime::new("page");
    let compose = |runtime: &mut Runtime, dirty: Vec<DirtyInput>| {
        let state = state.clone();
        compose_incremental_dirty(runtime, 240.0, 80.0, dirty, move |ui, _| {
            ui.retained_scope("panel", |ui| {
                let show_child = state
                    .signal(
                        "show_child",
                        |state| state.show_child,
                        |state, value| state.show_child = value,
                    )
                    .watch(ui);
                if show_child {
                    ui.retained_scope("child", |ui| {
                        let value = state
                            .signal(
                                "child_value",
                                |state| state.child_value,
                                |state, value| state.child_value = value,
                            )
                            .watch(ui);
                        ui.text("child.label")
                            .size(120.0, 40.0)
                            .text(format!("child {value}"))
                            .build();
                    });
                }
            });
        });
    };

    compose(&mut runtime, Vec::new());
    assert!(state
        .signal_dependencies()
        .iter()
        .any(|(key, scopes)| key.as_str() == "child_value"
            && scopes == &vec!["page.panel.child".to_string()]));

    state
        .signal(
            "show_child",
            |state| state.show_child,
            |state, value| state.show_child = value,
        )
        .set(false);
    compose(&mut runtime, state.take_dirty());
    assert!(runtime.diagnostics().find("child.label").is_none());

    state
        .signal(
            "child_value",
            |state| state.child_value,
            |state, value| state.child_value = value,
        )
        .set(1);
    assert!(
        state.take_dirty().is_empty(),
        "removed child scope should no longer subscribe to child_value"
    );
}

#[test]
fn live_scope_rebuilds_on_next_scoped_compose_without_state_dirty() {
    let mut runtime = Runtime::new("page");
    let live_builds = Rc::new(Cell::new(0));
    let static_builds = Rc::new(Cell::new(0));

    let compose = |runtime: &mut Runtime, dirty: Vec<DirtyInput>| {
        let live_builds = live_builds.clone();
        let static_builds = static_builds.clone();
        compose_incremental_dirty(runtime, 240.0, 80.0, dirty, move |ui, _| {
            ui.row("root").size(240.0, 40.0).content(|ui| {
                ui.retained_live_scope("live", |ui| {
                    live_builds.set(live_builds.get() + 1);
                    ui.text("live.label")
                        .size(100.0, 40.0)
                        .text(format!("live {}", live_builds.get()))
                        .build();
                });
                ui.retained_scope("static", |ui| {
                    static_builds.set(static_builds.get() + 1);
                    ui.text("static.label")
                        .size(100.0, 40.0)
                        .text("static")
                        .build();
                });
            });
        });
    };

    compose(&mut runtime, Vec::new());
    compose(&mut runtime, Vec::new());

    assert_eq!(live_builds.get(), 2);
    assert_eq!(static_builds.get(), 1);
    assert_eq!(
        runtime.diagnostics().find("live.label").unwrap().text,
        "live 2"
    );
    assert!(runtime.retained_compose_stats().built >= 1);
    assert!(runtime.retained_compose_stats().reused >= 1);
    assert!(runtime.retained_compose_stats().partial_layout);
    assert!(!runtime.retained_compose_stats().full_layout);
}

#[test]
fn clock_read_marks_active_scope_live_and_rebuilds_next_scoped_compose() {
    let mut runtime = Runtime::new("page");
    let builds = Rc::new(Cell::new(0));
    let mut sampled_seconds = 0.0;

    let compose = |runtime: &mut Runtime, dirty: Vec<DirtyInput>, sampled_seconds: &mut f32| {
        let builds = builds.clone();
        compose_incremental_dirty(runtime, 240.0, 80.0, dirty, move |ui, _| {
            ui.retained_scope("clocked", |ui| {
                builds.set(builds.get() + 1);
                let seconds = ui.clock().seconds();
                let tick = ui.clock().every(Duration::from_millis(250));
                assert_eq!(tick.period, Duration::from_millis(250));
                *sampled_seconds = seconds;
                ui.text("label")
                    .size(100.0, 40.0)
                    .text(format!("clock {seconds:.1}"))
                    .build();
            });
            ui.retained_scope("static", |ui| {
                ui.text("static.label")
                    .size(100.0, 40.0)
                    .text("static")
                    .build();
            });
        });
    };

    compose(&mut runtime, Vec::new(), &mut sampled_seconds);
    assert_eq!(builds.get(), 1);
    assert_eq!(sampled_seconds, 0.0);
    assert_eq!(
        runtime.diagnostics().committed_snapshot().clock_ids,
        vec!["page.clocked".to_string()]
    );

    runtime.update_events_and_timers(
        PointerEvent::default(),
        ScrollEvent::default(),
        KeyboardEvent::default(),
        0.5,
    );
    compose(&mut runtime, Vec::new(), &mut sampled_seconds);

    assert_eq!(builds.get(), 2);
    assert_eq!(sampled_seconds, 0.5);
    assert_eq!(
        runtime.diagnostics().find("label").unwrap().text,
        "clock 0.5"
    );
    assert_eq!(
        runtime.diagnostics().committed_snapshot().dirty_ids,
        vec!["page.clocked".to_string()]
    );
    assert_eq!(
        runtime.diagnostics().committed_snapshot().clock_ids,
        vec!["page.clocked".to_string()]
    );
    let clocked_record = runtime
        .diagnostics()
        .committed_snapshot()
        .retained
        .iter()
        .find(|scope| scope.id == "page.clocked")
        .expect("clocked scope should be reported");
    assert_eq!(clocked_record.dirty_reasons, vec![DirtyReason::Clock]);
    assert!(runtime.retained_compose_stats().partial_layout);
}

#[test]
fn clock_every_rebuilds_only_when_period_bucket_changes() {
    let mut runtime = Runtime::new("page");
    let builds = Rc::new(Cell::new(0));

    let compose = |runtime: &mut Runtime, dirty: Vec<DirtyInput>| {
        let builds = builds.clone();
        compose_incremental_dirty(runtime, 240.0, 80.0, dirty, move |ui, _| {
            ui.retained_scope("clocked", |ui| {
                builds.set(builds.get() + 1);
                let tick = ui.clock().every(Duration::from_millis(250));
                ui.text("label")
                    .size(100.0, 40.0)
                    .text(format!("tick {}", tick.frame_index))
                    .build();
            });
        });
    };

    compose(&mut runtime, Vec::new());
    assert_eq!(builds.get(), 1);
    assert_eq!(
        runtime.diagnostics().committed_snapshot().clock_ids,
        vec!["page.clocked".to_string()]
    );

    runtime.update_events_and_timers(
        PointerEvent::default(),
        ScrollEvent::default(),
        KeyboardEvent::default(),
        0.1,
    );
    compose(&mut runtime, Vec::new());
    assert_eq!(builds.get(), 1);
    assert!(runtime.retained_compose_stats().partial_layout);

    runtime.update_events_and_timers(
        PointerEvent::default(),
        ScrollEvent::default(),
        KeyboardEvent::default(),
        0.2,
    );
    compose(&mut runtime, Vec::new());
    assert_eq!(builds.get(), 2);
    assert_eq!(
        runtime.diagnostics().committed_snapshot().dirty_ids,
        vec!["page.clocked".to_string()]
    );
}

#[test]
fn clock_read_without_scope_uses_current_element_owner() {
    let mut runtime = Runtime::new("page");

    compose_incremental_dirty(
        &mut runtime,
        240.0,
        80.0,
        Vec::<DirtyInput>::new(),
        |ui, _| {
            ui.stack("panel").size(160.0, 40.0).content(|ui| {
                let seconds = ui.clock().seconds();
                ui.text("panel.label")
                    .size(120.0, 24.0)
                    .text(format!("{seconds:.1}"))
                    .build();
            });
        },
    );

    assert_eq!(
        runtime.diagnostics().committed_snapshot().clock_ids,
        vec!["page.panel".to_string()]
    );

    runtime.update_events_and_timers(
        PointerEvent::default(),
        ScrollEvent::default(),
        KeyboardEvent::default(),
        0.25,
    );
    compose_incremental_dirty(
        &mut runtime,
        240.0,
        80.0,
        Vec::<DirtyInput>::new(),
        |ui, _| {
            ui.stack("panel").size(160.0, 40.0).content(|ui| {
                let seconds = ui.clock().seconds();
                ui.text("panel.label")
                    .size(120.0, 24.0)
                    .text(format!("{seconds:.2}"))
                    .build();
            });
        },
    );

    assert_eq!(
        runtime.diagnostics().committed_snapshot().dirty_ids,
        vec!["page.panel".to_string()]
    );
    assert_eq!(
        runtime.diagnostics().find("panel.label").unwrap().text,
        "0.25"
    );
    let panel_record = runtime
        .diagnostics()
        .committed_snapshot()
        .retained
        .iter()
        .find(|scope| scope.id == "page.panel")
        .expect("automatic clock owner should be reported");
    assert_eq!(panel_record.dirty_reasons, vec![DirtyReason::Clock]);
}

#[test]
fn debug_snapshot_records_scope_and_element_ancestry() {
    let mut runtime = Runtime::new("page");

    compose(&mut runtime, 240.0, 120.0, |ui, _| {
        ui.retained_scope("panel", |ui| {
            ui.stack("panel.scroll")
                .size(200.0, 100.0)
                .clip()
                .on_scroll(|_| {})
                .content(|ui| {
                    ui.retained_scope("child", |ui| {
                        ui.rect("panel.child.leaf").size(40.0, 20.0).build();
                    });
                });
        });
    });

    let snapshot = runtime.diagnostics().committed_snapshot();
    let child_record = snapshot
        .retained
        .iter()
        .find(|scope| scope.id == "page.panel.child")
        .expect("child scope should be reported");
    assert_eq!(child_record.scope_id().as_str(), "page.panel.child");
    assert_eq!(child_record.parent_id.as_deref(), Some("page.panel.scroll"));
    assert_eq!(
        child_record.parent_scope_id().map(ScopeId::as_str),
        Some("page.panel.scroll")
    );
    assert_eq!(
        child_record.scroll_ancestor_id().map(NodeId::as_str),
        Some("page.panel.scroll")
    );
    assert_eq!(
        child_record.clip_ancestor_id().map(NodeId::as_str),
        Some("page.panel.scroll")
    );
    assert_eq!(child_record.current_roots, 1);
    assert_eq!(child_record.action, Some(RetainedComposeAction::Built));
    assert_eq!(
        child_record.compose_reason,
        Some(RetainedComposeReason::RetainedReuseUnavailable)
    );

    let leaf = snapshot
        .elements
        .iter()
        .find(|element| element.id == "page.panel.child.leaf")
        .expect("leaf element should be reported");
    assert_eq!(leaf.node_id().as_str(), "page.panel.child.leaf");
    assert_eq!(leaf.parent.as_deref(), Some("page.panel.scroll"));
    assert_eq!(leaf.retained_boundary.as_deref(), Some("page.panel.child"));
    assert_eq!(leaf.scroll_ancestor.as_deref(), Some("page.panel.scroll"));
    assert_eq!(leaf.clip_ancestor.as_deref(), Some("page.panel.scroll"));
    assert_eq!(
        leaf.parent_id().map(NodeId::as_str),
        Some("page.panel.scroll")
    );
    assert_eq!(
        leaf.retained_boundary_id().map(ScopeId::as_str),
        Some("page.panel.child")
    );
    assert_eq!(
        leaf.scroll_ancestor_id().map(NodeId::as_str),
        Some("page.panel.scroll")
    );
    assert_eq!(
        leaf.clip_ancestor_id().map(NodeId::as_str),
        Some("page.panel.scroll")
    );
    assert_eq!(leaf.draw_frame, Some(leaf.target_frame));
    assert_eq!(leaf.transformed_draw_frame, Some(leaf.target_frame));
    assert_eq!(leaf.draw_transform, Transform::default());
    assert_eq!(
        leaf.active_clip.map(|clip| clip.rect),
        Some(LayoutRect::new(0.0, 0.0, 200.0, 100.0))
    );
    assert!(leaf.draw_visible);
}

#[test]
fn debug_snapshot_records_draw_transform_and_transformed_frame() {
    let mut runtime = Runtime::new("page");

    compose(&mut runtime, 240.0, 120.0, |ui, _| {
        ui.stack("panel")
            .size(100.0, 100.0)
            .scale(0.5)
            .translate(10.0, 20.0)
            .content(|ui| {
                ui.rect("leaf").size(20.0, 10.0).build();
            });
    });

    let leaf = runtime
        .diagnostics()
        .committed_snapshot()
        .elements
        .iter()
        .find(|element| element.id == "page.leaf")
        .expect("leaf element should be reported");
    assert_eq!(leaf.target_frame, LayoutRect::new(0.0, 0.0, 20.0, 10.0));
    assert_eq!(leaf.draw_frame, Some(leaf.target_frame));
    assert_eq!(
        leaf.transformed_draw_frame,
        Some(LayoutRect::new(35.0, 45.0, 10.0, 5.0))
    );
    assert_eq!(leaf.draw_transform.scale, [0.5, 0.5]);
    assert_eq!(leaf.draw_transform.translate, [10.0, 20.0]);
    assert_eq!(leaf.active_clip, None);
    assert!(leaf.draw_visible);
}

#[test]
fn debug_snapshot_uses_tree_ancestry_for_non_prefixed_scope_ids() {
    let mut runtime = Runtime::new("page");

    compose(&mut runtime, 240.0, 120.0, |ui, _| {
        ui.retained_scope("panel", |ui| {
            ui.stack("host").size(200.0, 100.0).content(|ui| {
                ui.retained_scope("page.live", |ui| {
                    ui.rect("live.leaf").size(40.0, 20.0).build();
                });
            });
        });
    });

    let snapshot = runtime.diagnostics().committed_snapshot();
    let live_record = snapshot
        .retained
        .iter()
        .find(|scope| scope.id == "page.live")
        .expect("non-prefixed child scope should be reported");
    assert_eq!(live_record.parent_id.as_deref(), Some("page.host"));
    assert_eq!(
        live_record.parent_scope_id().map(ScopeId::as_str),
        Some("page.host")
    );

    let leaf = snapshot
        .elements
        .iter()
        .find(|element| element.id == "page.live.leaf")
        .expect("non-prefixed child leaf should be reported");
    assert_eq!(leaf.parent.as_deref(), Some("page.host"));
    assert_eq!(leaf.retained_boundary.as_deref(), Some("page.live"));
    assert_eq!(leaf.parent_id().map(NodeId::as_str), Some("page.host"));
    assert_eq!(
        leaf.retained_boundary_id().map(ScopeId::as_str),
        Some("page.live")
    );
}

#[test]
fn retained_scope_ids_are_typed_internally_but_report_readable_labels() {
    let mut runtime = Runtime::new("page");

    compose(&mut runtime, 240.0, 120.0, |ui, _| {
        ui.retained_scope("panel", |ui| {
            ui.stack("host").size(200.0, 100.0).content(|ui| {
                ui.retained_live_scope("page.panel_extra.live", |ui| {
                    let seconds = ui.clock().seconds();
                    ui.text("page.panel_extra.live.label")
                        .size(80.0, 20.0)
                        .text(format!("{seconds:.1}"))
                        .build();
                });
            });
        });
    });

    let snapshot = runtime.diagnostics().committed_snapshot();
    assert_eq!(
        snapshot.clock_ids,
        vec!["page.panel_extra.live".to_string()]
    );
    assert!(snapshot
        .retained_events
        .iter()
        .any(|event| event.id.as_str() == "page.panel_extra.live"));
    let live_record = snapshot
        .retained
        .iter()
        .find(|scope| scope.id == "page.panel_extra.live")
        .expect("typed scope id should still report a readable label");
    assert_eq!(live_record.scope_id().as_str(), "page.panel_extra.live");
    assert_eq!(live_record.parent_id.as_deref(), Some("page.host"));
    assert_eq!(
        live_record.parent_scope_id().map(ScopeId::as_str),
        Some("page.host")
    );
}

#[test]
fn dirty_non_prefixed_child_scope_prevents_parent_reuse_by_tree() {
    let mut runtime = Runtime::new("page");
    let parent_builds = Rc::new(Cell::new(0));
    let child_builds = Rc::new(Cell::new(0));

    let compose = |runtime: &mut Runtime, dirty: Vec<DirtyInput>| {
        let parent_builds = parent_builds.clone();
        let child_builds = child_builds.clone();
        compose_incremental_dirty(runtime, 240.0, 120.0, dirty, move |ui, _| {
            ui.retained_scope("panel", |ui| {
                parent_builds.set(parent_builds.get() + 1);
                ui.stack("host").size(200.0, 100.0).content(|ui| {
                    ui.retained_scope("page.live", |ui| {
                        child_builds.set(child_builds.get() + 1);
                        ui.text("live.label")
                            .size(120.0, 24.0)
                            .text(format!("child {}", child_builds.get()))
                            .build();
                    });
                });
            });
        });
    };

    compose(&mut runtime, Vec::new());
    compose(&mut runtime, vec![dirty_scope("page.live")]);

    assert_eq!(parent_builds.get(), 2);
    assert_eq!(child_builds.get(), 2);
    assert_eq!(
        runtime.diagnostics().find("live.label").unwrap().text,
        "child 2"
    );
    assert_eq!(
        runtime
            .diagnostics()
            .committed_snapshot()
            .normalized_dirty_ids,
        vec!["page.live".to_string()]
    );
}

#[test]
fn dirty_parent_normalizes_live_child_for_layout_but_still_rebuilds_child() {
    let mut runtime = Runtime::new("page");
    let parent_builds = Rc::new(Cell::new(0));
    let child_builds = Rc::new(Cell::new(0));

    let compose = |runtime: &mut Runtime, dirty: Vec<DirtyInput>| {
        let parent_builds = parent_builds.clone();
        let child_builds = child_builds.clone();
        compose_incremental_dirty(runtime, 240.0, 80.0, dirty, move |ui, _| {
            ui.retained_scope("parent", |ui| {
                parent_builds.set(parent_builds.get() + 1);
                ui.row("row").size(240.0, 40.0).content(|ui| {
                    ui.retained_live_scope("child", |ui| {
                        child_builds.set(child_builds.get() + 1);
                        ui.text("label")
                            .size(100.0, 40.0)
                            .text(format!("child {}", child_builds.get()))
                            .build();
                    });
                });
            });
        });
    };

    compose(&mut runtime, Vec::new());
    compose(&mut runtime, vec![dirty_scope("page.parent")]);

    assert_eq!(parent_builds.get(), 2);
    assert_eq!(child_builds.get(), 2);
    assert_eq!(runtime.diagnostics().find("label").unwrap().text, "child 2");
    assert_eq!(
        runtime.diagnostics().committed_snapshot().dirty_ids,
        vec!["page.parent".to_string(), "page.parent.child".to_string()]
    );
    assert_eq!(
        runtime
            .diagnostics()
            .committed_snapshot()
            .normalized_dirty_ids,
        vec!["page.parent".to_string()]
    );
    let child_record = runtime
        .diagnostics()
        .committed_snapshot()
        .retained
        .iter()
        .find(|scope| scope.id == "page.parent.child")
        .expect("child scope should be reported");
    assert_eq!(child_record.action, Some(RetainedComposeAction::Built));
    assert_eq!(
        child_record.compose_reason,
        Some(RetainedComposeReason::DirtyScope)
    );
    assert!(runtime.retained_compose_stats().partial_layout);
    assert_eq!(
        runtime.diagnostics().committed_snapshot().layout_mode,
        LayoutMode::Partial
    );
}

#[test]
fn live_child_inside_dirty_scroll_parent_tracks_scroll_offset() {
    let mut runtime = Runtime::new("page");
    let live_builds = Rc::new(Cell::new(0));

    let compose = |runtime: &mut Runtime, dirty: Vec<DirtyInput>, offset: f32| {
        let live_builds = live_builds.clone();
        compose_incremental_dirty(runtime, 240.0, 120.0, dirty, move |ui, _| {
            ui.retained_scope("panel", |ui| {
                ui.scroll_y("scroll")
                    .size(200.0, 80.0)
                    .content_height(180.0)
                    .offset(offset)
                    .content(|ui| {
                        ui.stack("top").size(Size::fill(), 60.0).build();
                        ui.retained_live_scope("secret.live", |ui| {
                            live_builds.set(live_builds.get() + 1);
                            ui.stack("secret").size(Size::fill(), 40.0).build();
                        });
                    });
            });
        });
    };

    compose(&mut runtime, Vec::new(), 0.0);
    let first = runtime.diagnostics().find("secret").unwrap().frame;
    runtime.mark_rendered();

    compose(&mut runtime, vec![dirty_scope("page.panel")], 30.0);
    let second = runtime.diagnostics().find("secret").unwrap().frame;

    assert_eq!(live_builds.get(), 2);
    assert!(
        (second.y - (first.y - 30.0)).abs() < 0.001,
        "live child should remain attached to scroll content: first={first:?} second={second:?}"
    );
    assert_eq!(
        runtime.diagnostics().committed_snapshot().dirty_ids,
        vec![
            "page.panel".to_string(),
            "page.panel.secret.live".to_string()
        ]
    );
    assert_eq!(
        runtime
            .diagnostics()
            .committed_snapshot()
            .normalized_dirty_ids,
        vec!["page.panel".to_string()]
    );
    assert!(runtime.retained_compose_stats().partial_layout);
    assert_eq!(
        runtime.diagnostics().committed_snapshot().layout_mode,
        LayoutMode::Partial
    );
    assert!(!runtime.full_redraw());
}

#[test]
fn partial_layout_preserves_parent_assigned_grow_frame_for_live_scope_root() {
    let mut runtime = Runtime::new("page");
    let live_builds = Rc::new(Cell::new(0));

    let compose = |runtime: &mut Runtime, dirty: Vec<DirtyInput>| {
        let live_builds = live_builds.clone();
        compose_incremental_dirty(runtime, 500.0, 100.0, dirty, move |ui, _| {
            ui.row("root").size(500.0, 80.0).gap(20.0).content(|ui| {
                ui.stack("left").size(100.0, 80.0).build();
                ui.retained_live_scope("live", |ui| {
                    live_builds.set(live_builds.get() + 1);
                    ui.stack("center")
                        .size(120.0, 80.0)
                        .grow(1.0)
                        .min_width(120.0)
                        .content(|ui| {
                            ui.stack("viewport")
                                .size(Size::fill(), Size::fill())
                                .clip()
                                .content(|ui| {
                                    ui.stack("child").size(Size::fill(), 40.0).build();
                                });
                        });
                });
                ui.stack("right").size(100.0, 80.0).build();
            });
        });
    };

    compose(&mut runtime, Vec::new());
    let first_center = runtime.diagnostics().find("center").unwrap().frame;
    let first_viewport = runtime.diagnostics().find("viewport").unwrap().frame;
    assert_frame(first_center, 120.0, 0.0, 260.0, 80.0);
    assert_frame(first_viewport, 120.0, 0.0, 260.0, 80.0);

    compose(&mut runtime, Vec::new());
    let second_center = runtime.diagnostics().find("center").unwrap().frame;
    let second_viewport = runtime.diagnostics().find("viewport").unwrap().frame;
    assert_frame(second_center, 120.0, 0.0, 260.0, 80.0);
    assert_frame(second_viewport, 120.0, 0.0, 260.0, 80.0);
    assert_eq!(live_builds.get(), 2);
    assert!(runtime.retained_compose_stats().partial_layout);
}

#[test]
fn dirty_scope_with_same_structure_uses_partial_layout() {
    let mut runtime = Runtime::new("page");
    let mut value = 0;

    compose_incremental_dirty(
        &mut runtime,
        240.0,
        80.0,
        Vec::<DirtyInput>::new(),
        |ui, _| {
            ui.retained_scope("body", |ui| {
                ui.text("label")
                    .size(100.0, 40.0)
                    .text(format!("value {value}"))
                    .build();
            });
        },
    );

    value = 1;
    compose_incremental_dirty(
        &mut runtime,
        240.0,
        80.0,
        [dirty_scope("page.body")],
        |ui, _| {
            ui.retained_scope("body", |ui| {
                ui.text("label")
                    .size(100.0, 40.0)
                    .text(format!("value {value}"))
                    .build();
            });
        },
    );

    assert_eq!(runtime.diagnostics().find("label").unwrap().text, "value 1");
    assert!(runtime.retained_compose_stats().partial_layout);
    assert!(!runtime.retained_compose_stats().full_layout);
    assert_eq!(
        runtime.diagnostics().committed_snapshot().layout_mode,
        LayoutMode::Partial
    );
    assert_eq!(
        runtime.diagnostics().committed_snapshot().dirty_ids,
        vec!["page.body".to_string()]
    );
    let body_record = runtime
        .diagnostics()
        .committed_snapshot()
        .retained
        .iter()
        .find(|scope| scope.id == "page.body")
        .expect("dirty body scope should be reported");
    assert_eq!(body_record.action, Some(RetainedComposeAction::Built));
    assert_eq!(
        body_record.compose_reason,
        Some(RetainedComposeReason::DirtyScope)
    );
}

#[test]
fn visual_dirty_records_invalidation_without_rebuilding_scope() {
    let mut runtime = Runtime::new("page");
    let builds = Rc::new(Cell::new(0));
    let mut value = 0;

    let compose_tree = |runtime: &mut Runtime, dirty: Vec<DirtyInput>, value: i32| {
        let builds = builds.clone();
        compose_incremental_dirty(runtime, 240.0, 80.0, dirty, move |ui, _| {
            ui.retained_scope("body", |ui| {
                builds.set(builds.get() + 1);
                ui.text("label")
                    .size(100.0, 40.0)
                    .text(format!("value {value}"))
                    .build();
            });
        });
    };

    compose_tree(&mut runtime, Vec::new(), value);
    value = 1;
    compose_tree(
        &mut runtime,
        vec![DirtyInput::new(
            "page.body",
            DirtyFlags::VISUAL | DirtyFlags::DRAW,
        )],
        value,
    );

    let snapshot = runtime.diagnostics().committed_snapshot();
    assert_eq!(builds.get(), 1);
    assert_eq!(runtime.diagnostics().find("label").unwrap().text, "value 0");
    assert!(runtime.retained_compose_stats().partial_layout);
    assert_eq!(snapshot.dirty_ids, Vec::<String>::new());
    assert!(snapshot.pass_flags.request_draw);
    assert!(!snapshot.pass_flags.request_compose_ui);
    assert!(!snapshot.pass_flags.request_reconcile);
    assert!(snapshot.invalidations.iter().any(|invalidation| {
        matches!(
            &invalidation.target,
            InvalidationTarget::Scope(scope) if scope.as_str() == "page.body"
        ) && invalidation.target.kind() == "scope"
            && invalidation.source.kind() == "runtime"
            && invalidation.source.label() == "dirty_input"
            && invalidation.pass_flags.request_draw
            && !invalidation.pass_flags.request_compose_ui
    }));
}

#[test]
fn pending_compose_with_visual_dirty_uses_full_compose_fallback() {
    let mut runtime = Runtime::new("page");
    let builds = Rc::new(Cell::new(0));
    let mut value = 0;

    let compose_tree = |runtime: &mut Runtime, dirty: Option<Vec<DirtyInput>>, value: i32| {
        let builds = builds.clone();
        let mut input = FrameInput::new(Screen::new(240.0, 80.0), 0.0);
        input.dirty = dirty;
        runtime.frame(input, move |ui, _| {
            ui.retained_scope("body", |ui| {
                builds.set(builds.get() + 1);
                ui.text("label")
                    .size(100.0, 40.0)
                    .text(format!("value {value}"))
                    .build();
            });
        });
    };

    compose_tree(&mut runtime, None, value);
    runtime.request_resource_dirty(ResourceDirty::new(
        "test:pending_compose",
        DirtyFlags::COMPOSE | DirtyFlags::DRAW,
    ));
    value = 1;
    compose_tree(
        &mut runtime,
        Some(vec![DirtyInput::new(
            "page.body",
            DirtyFlags::VISUAL | DirtyFlags::DRAW,
        )]),
        value,
    );

    let snapshot = runtime.diagnostics().committed_snapshot();
    assert_eq!(builds.get(), 2);
    assert_eq!(runtime.diagnostics().find("label").unwrap().text, "value 1");
    assert_eq!(
        snapshot.layout_mode,
        LayoutMode::Full(FullLayoutReason::RetainedReuseUnavailable)
    );
    assert!(snapshot.pass_flags.request_draw);
    assert!(snapshot.pass_flags.request_compose_ui);
    assert!(snapshot.invalidations.iter().any(|invalidation| {
        invalidation.source == InvalidationSource::Resource("test:pending_compose".into())
            && invalidation.pass_flags.request_compose_ui
    }));
}

#[test]
fn debug_trace_explains_clean_reuse_and_dirty_descendant_rebuild() {
    let mut runtime = Runtime::new("page");
    let mut value = 0;

    let compose_tree = |runtime: &mut Runtime, dirty: Vec<DirtyInput>, value: i32| {
        compose_incremental_dirty(runtime, 260.0, 100.0, dirty, move |ui, _| {
            ui.retained_scope("outer", |ui| {
                ui.row("outer.row").size(220.0, 40.0).content(|ui| {
                    ui.retained_scope("inner", |ui| {
                        ui.text("outer.inner.label")
                            .size(80.0, 24.0)
                            .text(format!("value {value}"))
                            .build();
                    });
                });
            });
            ui.retained_scope("sibling", |ui| {
                ui.text("sibling.label").size(80.0, 24.0).text("ok").build();
            });
        });
    };

    compose_tree(&mut runtime, Vec::new(), value);
    value = 1;
    compose_tree(&mut runtime, vec![dirty_scope("page.outer.inner")], value);

    let snapshot = runtime.diagnostics().committed_snapshot();
    let outer = snapshot
        .retained
        .iter()
        .find(|scope| scope.id == "page.outer")
        .expect("outer scope should be reported");
    let inner = snapshot
        .retained
        .iter()
        .find(|scope| scope.id == "page.outer.inner")
        .expect("inner scope should be reported");
    let sibling = snapshot
        .retained
        .iter()
        .find(|scope| scope.id == "page.sibling")
        .expect("sibling scope should be reported");

    assert_eq!(outer.action, Some(RetainedComposeAction::Built));
    assert_eq!(
        outer.compose_reason,
        Some(RetainedComposeReason::DirtyDescendant)
    );
    assert_eq!(inner.action, Some(RetainedComposeAction::Built));
    assert_eq!(
        inner.compose_reason,
        Some(RetainedComposeReason::DirtyScope)
    );
    assert_eq!(sibling.action, Some(RetainedComposeAction::Reused));
    assert_eq!(
        sibling.compose_reason,
        Some(RetainedComposeReason::CleanReuse)
    );
}

#[test]
fn debug_trace_explains_build_inside_dirty_ancestor() {
    let mut runtime = Runtime::new("page");
    let mut value = 0;

    let compose_tree = |runtime: &mut Runtime, dirty: Vec<DirtyInput>, value: i32| {
        compose_incremental_dirty(runtime, 260.0, 100.0, dirty, move |ui, _| {
            ui.stack("root").size(220.0, 50.0).content(|ui| {
                ui.stack("panel").size(180.0, 40.0).content(|ui| {
                    ui.text("panel.label")
                        .size(80.0, 24.0)
                        .text(format!("value {value}"))
                        .build();
                });
            });
        });
    };

    compose_tree(&mut runtime, Vec::new(), value);
    value = 1;
    compose_tree(&mut runtime, vec![dirty_scope("page.root")], value);

    let snapshot = runtime.diagnostics().committed_snapshot();
    let panel = snapshot
        .retained
        .iter()
        .find(|scope| scope.id == "page.panel")
        .expect("panel element scope should be reported");

    assert_eq!(panel.action, Some(RetainedComposeAction::Built));
    assert_eq!(
        panel.compose_reason,
        Some(RetainedComposeReason::DirtyAncestor)
    );
}

#[test]
fn dirty_scope_with_changed_structure_uses_full_layout_fallback() {
    let mut runtime = Runtime::new("page");

    compose_incremental_dirty(
        &mut runtime,
        240.0,
        80.0,
        Vec::<DirtyInput>::new(),
        |ui, _| {
            ui.retained_scope("body", |ui| {
                ui.text("motion").size(100.0, 40.0).text("motion").build();
            });
        },
    );

    compose_incremental_dirty(
        &mut runtime,
        240.0,
        80.0,
        [dirty_scope("page.body")],
        |ui, _| {
            ui.retained_scope("body", |ui| {
                ui.row("chart").size(120.0, 40.0).content(|ui| {
                    ui.text("bar").size(60.0, 40.0).text("bar").build();
                    ui.text("pie").size(60.0, 40.0).text("pie").build();
                });
            });
        },
    );

    assert!(runtime.diagnostics().find("chart").is_some());
    assert!(!runtime.retained_compose_stats().partial_layout);
    assert!(runtime.retained_compose_stats().full_layout);
    assert_eq!(
        runtime.diagnostics().committed_snapshot().layout_mode,
        LayoutMode::Full(FullLayoutReason::StructureChanged {
            ids: vec![ScopeId::new("page.body")]
        })
    );
}

#[test]
fn dirty_scope_with_changed_fixed_size_uses_full_layout_fallback() {
    let mut runtime = Runtime::new("page");

    compose(&mut runtime, 240.0, 80.0, |ui, _| {
        ui.row("root").size(240.0, 40.0).content(|ui| {
            ui.retained_scope("left", |ui| {
                ui.rect("left.box").size(40.0, 40.0).build();
            });
            ui.rect("right").size(40.0, 40.0).build();
        });
    });
    let right_before = runtime.diagnostics().find("right").unwrap().frame;

    compose_incremental_dirty(
        &mut runtime,
        240.0,
        80.0,
        vec![dirty_scope("page.left")],
        |ui, _| {
            ui.row("root").size(240.0, 40.0).content(|ui| {
                ui.retained_scope("left", |ui| {
                    ui.rect("left.box").size(80.0, 40.0).build();
                });
                ui.rect("right").size(40.0, 40.0).build();
            });
        },
    );

    let right_after = runtime.diagnostics().find("right").unwrap().frame;
    assert_eq!(right_before.x, 40.0);
    assert_eq!(right_after.x, 80.0);
    assert_eq!(
        runtime.diagnostics().committed_snapshot().layout_mode,
        LayoutMode::Full(FullLayoutReason::StructureChanged {
            ids: vec![ScopeId::new("page.left")]
        })
    );
}

#[test]
fn root_dirty_scope_without_retained_scope_roots_uses_full_layout_fallback() {
    let mut runtime = Runtime::new("page");
    compose(&mut runtime, 240.0, 80.0, |ui, _| {
        ui.row("root").size(240.0, 40.0).content(|ui| {
            for index in 0..8 {
                button(ui, format!("button.{index}"))
                    .size(24.0, 24.0)
                    .text(index.to_string())
                    .build();
            }
        });
    });

    compose_incremental_dirty(&mut runtime, 240.0, 80.0, [dirty_scope("page")], |ui, _| {
        ui.row("root").size(240.0, 40.0).content(|ui| {
            for index in 0..8 {
                button(ui, format!("button.{index}"))
                    .size(24.0, 24.0)
                    .text(index.to_string())
                    .build();
            }
        });
    });

    assert!(!runtime.retained_compose_stats().partial_layout);
    assert!(runtime.retained_compose_stats().full_layout);
}

#[test]
fn eui_demo_layout_matches_original_frames() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 800.0, 600.0, |ui, screen| {
        ui.stack("root")
            .size(screen.width, screen.height)
            .align(Align::Center, Align::Center)
            .content(|ui| {
                panel(ui, "card")
                    .size(360.0, 260.0)
                    .radius(18.0)
                    .gradient(
                        Color::new(0.10, 0.12, 0.16, 1.0),
                        Color::new(0.05, 0.07, 0.10, 1.0),
                    )
                    .border(1.0, Color::new(0.23, 0.29, 0.38, 1.0))
                    .shadow(26.0, 0.0, 8.0, Color::new(0.0, 0.0, 0.0, 0.26))
                    .build();

                ui.column("content")
                    .size(360.0, 260.0)
                    .gap(8.0)
                    .justify_content(Align::Center)
                    .align_items(Align::Center)
                    .content(|ui| {
                        text(ui, "title")
                            .size(300.0, 38.0)
                            .text("Hello EUI")
                            .font_size(30.0)
                            .line_height(38.0)
                            .color(Color::new(0.94, 0.97, 1.0, 1.0))
                            .horizontal_align(HorizontalAlign::Center)
                            .build();

                        text(ui, "subtitle")
                            .size(300.0, 30.0)
                            .margin_each(0.0, 0.0, 0.0, 16.0)
                            .text("Text Button Component")
                            .font_size(24.0)
                            .line_height(30.0)
                            .color(Color::new(0.62, 0.70, 0.82, 1.0))
                            .horizontal_align(HorizontalAlign::Center)
                            .build();

                        button(ui, "primary")
                            .size(240.0, 70.0)
                            .text("Click Me")
                            .build();
                    });
            });
    });

    assert_frame(
        runtime.diagnostics().find("root").unwrap().frame,
        0.0,
        0.0,
        800.0,
        600.0,
    );
    assert_frame(
        runtime.diagnostics().find("card").unwrap().frame,
        220.0,
        170.0,
        360.0,
        260.0,
    );
    assert_frame(
        runtime.diagnostics().find("content").unwrap().frame,
        220.0,
        170.0,
        360.0,
        260.0,
    );
    assert_frame(
        runtime.diagnostics().find("title").unwrap().frame,
        250.0,
        215.0,
        300.0,
        38.0,
    );
    assert_frame(
        runtime.diagnostics().find("subtitle").unwrap().frame,
        250.0,
        261.0,
        300.0,
        30.0,
    );
    assert_frame(
        runtime.diagnostics().find("primary").unwrap().frame,
        280.0,
        315.0,
        240.0,
        70.0,
    );
    assert_frame(
        runtime.diagnostics().find("primary.bg").unwrap().frame,
        280.0,
        315.0,
        240.0,
        70.0,
    );
    assert_frame(
        runtime.diagnostics().find("primary.content").unwrap().frame,
        280.0,
        315.0,
        240.0,
        70.0,
    );
    assert_frame(
        runtime.diagnostics().find("primary.text").unwrap().frame,
        280.0,
        315.0,
        240.0,
        70.0,
    );
}

#[test]
fn runtime_marks_no_redraw_for_same_structure_and_size_after_rendered() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("a").size(10.0, 10.0).build();
    });
    runtime.mark_rendered();

    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("a").size(10.0, 10.0).build();
    });

    assert!(!runtime.needs_render());
    assert!(!runtime.full_redraw());
}

#[test]
fn runtime_detects_structure_changes() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("a").size(10.0, 10.0).build();
    });
    runtime.mark_rendered();

    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("a").size(10.0, 10.0).build();
        ui.rect("b").size(10.0, 10.0).build();
    });

    assert!(runtime.needs_render());
    assert!(runtime.full_redraw());
    let snapshot = runtime.diagnostics().committed_snapshot();
    assert!(snapshot.pass_flags.request_layout);
    assert!(snapshot.pass_flags.request_hit);
    assert!(snapshot.pass_flags.request_draw);
    assert!(!snapshot.pass_flags.request_compose_ui);
    assert!(snapshot.invalidations.iter().any(|invalidation| {
        invalidation.target.id() == "demo"
            && invalidation.source.kind() == "runtime"
            && invalidation.source.label() == "layout_structure"
            && invalidation.flags == DirtyFlags::LAYOUT
            && invalidation.pass_flags.request_layout
            && invalidation.pass_flags.request_draw
    }));
}

#[test]
fn runtime_detects_visual_changes_without_full_layout_redraw() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("panel").size(40.0, 20.0).color(Color::RED).build();
    });
    runtime.mark_rendered();

    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("panel").size(40.0, 20.0).color(Color::BLUE).build();
    });

    assert!(runtime.needs_render());
    assert!(!runtime.full_redraw());
    let snapshot = runtime.diagnostics().committed_snapshot();
    assert!(snapshot.pass_flags.request_draw);
    assert!(!snapshot.pass_flags.request_layout);
    assert!(!snapshot.pass_flags.request_compose_ui);
    assert!(snapshot.invalidations.iter().any(|invalidation| {
        invalidation.target.id() == "demo"
            && invalidation.source.kind() == "runtime"
            && invalidation.source.label() == "visual_structure"
            && invalidation.flags == DirtyFlags::VISUAL
            && invalidation.pass_flags.request_draw
            && !invalidation.pass_flags.request_layout
    }));
}

#[test]
fn runtime_treats_fixed_text_color_as_visual_only() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.text("title")
            .size(80.0, 20.0)
            .text("A")
            .color(Color::RED)
            .build();
    });
    runtime.mark_rendered();

    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.text("title")
            .size(80.0, 20.0)
            .text("A")
            .color(Color::BLUE)
            .build();
    });

    assert!(runtime.needs_render());
    assert!(!runtime.full_redraw());
}

#[test]
fn runtime_treats_fixed_text_content_as_visual_only() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.text("title").size(80.0, 20.0).text("A").build();
    });
    runtime.mark_rendered();

    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.text("title").size(80.0, 20.0).text("B").build();
    });

    assert!(runtime.needs_render());
    assert!(!runtime.full_redraw());
}

#[test]
fn runtime_detects_wrap_content_text_content_as_layout_affecting() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.text("title").wrap_content().text("A").build();
    });
    runtime.mark_rendered();

    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.text("title").wrap_content().text("Wider").build();
    });

    assert!(runtime.needs_render());
    assert!(runtime.full_redraw());
}

#[test]
fn cached_draw_list_invalidates_when_visuals_change() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("panel").size(40.0, 20.0).color(Color::RED).build();
    });
    let first = runtime.draw_list();
    let first_cached = runtime.draw_list();
    let first_color = first
        .commands()
        .iter()
        .filter_map(rect_draw)
        .find(|draw| draw.id == "demo.panel")
        .map(|draw| draw.color)
        .expect("panel rect should draw");
    assert_eq!(first.revision(), first_cached.revision());
    assert_eq!(first_color, Color::RED);

    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("panel").size(40.0, 20.0).color(Color::BLUE).build();
    });
    let second = runtime.draw_list();
    let second_color = second
        .commands()
        .iter()
        .filter_map(rect_draw)
        .find(|draw| draw.id == "demo.panel")
        .map(|draw| draw.color)
        .expect("panel rect should draw");

    assert_ne!(first.revision(), second.revision());
    assert_eq!(second_color, Color::BLUE);
}

#[test]
fn frame_transition_interpolates_draw_list_frame() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 200.0, 100.0, |ui, _| {
        ui.rect("bar")
            .size(10.0, 10.0)
            .transition(Transition::ease(1.0, Ease::Linear))
            .animate(AnimProperty::FRAME)
            .build();
    });
    runtime.tick_animations(0.0);
    runtime.mark_rendered();

    compose(&mut runtime, 200.0, 100.0, |ui, _| {
        ui.rect("bar")
            .size(110.0, 10.0)
            .transition(Transition::ease(1.0, Ease::Linear))
            .animate(AnimProperty::FRAME)
            .build();
    });
    assert!(runtime.tick_animations(0.5));
    assert!(runtime
        .diagnostics()
        .current_snapshot()
        .invalidations
        .iter()
        .any(|invalidation| {
            invalidation.target.id() == "demo"
                && invalidation.source.kind() == "runtime"
                && invalidation.source.label() == "animation_tick"
                && invalidation.flags == DirtyFlags::DRAW
                && invalidation.pass_flags.request_draw
                && !invalidation.pass_flags.request_compose_ui
        }));

    let draw = runtime.draw_list();
    let bar = draw
        .commands()
        .iter()
        .filter_map(rect_draw)
        .find(|draw| draw.id == "demo.bar")
        .unwrap();

    assert!((bar.frame.width - 60.0).abs() < 0.001);
    assert!(runtime.needs_render());
}

#[test]
fn active_animation_without_value_change_keeps_draw_list_cache() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 200.0, 100.0, |ui, _| {
        ui.rect("bar")
            .size(10.0, 10.0)
            .transition(Transition::ease(1.0, Ease::Linear))
            .animate(AnimProperty::FRAME)
            .build();
    });
    runtime.tick_animations(0.0);
    runtime.mark_rendered();

    compose(&mut runtime, 200.0, 100.0, |ui, _| {
        ui.rect("bar")
            .size(110.0, 10.0)
            .transition(Transition::ease(1.0, Ease::Linear))
            .animate(AnimProperty::FRAME)
            .build();
    });
    assert!(runtime.tick_animations(0.5));
    let first = runtime.draw_list();
    runtime.mark_rendered();

    assert!(!runtime.tick_animations(0.0));
    assert!(runtime.needs_render());
    let second = runtime.draw_list();

    assert_eq!(first.revision(), second.revision());
}

#[test]
fn removed_animated_element_clears_active_animation_state_on_commit() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 200.0, 100.0, |ui, _| {
        ui.rect("bar")
            .size(10.0, 10.0)
            .transition(Transition::ease(1.0, Ease::Linear))
            .animate(AnimProperty::FRAME)
            .build();
    });
    runtime.tick_animations(0.0);

    compose(&mut runtime, 200.0, 100.0, |ui, _| {
        ui.rect("bar")
            .size(110.0, 10.0)
            .transition(Transition::ease(1.0, Ease::Linear))
            .animate(AnimProperty::FRAME)
            .build();
    });
    assert!(runtime.tick_animations(0.5));
    assert_eq!(
        runtime
            .diagnostics()
            .current_snapshot()
            .active_animation_count,
        1
    );

    compose(&mut runtime, 200.0, 100.0, |ui, _| {
        ui.stack("root").size(200.0, 100.0).build();
    });

    assert_eq!(
        runtime
            .diagnostics()
            .current_snapshot()
            .active_animation_count,
        0
    );
}

#[test]
fn ancestor_frame_change_snaps_descendant_frame_animation() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 200.0, 100.0, |ui, _| {
        ui.stack("viewport").size(100.0, 80.0).clip().content(|ui| {
            ui.stack("content").size(100.0, 160.0).content(|ui| {
                ui.rect("indicator")
                    .size(20.0, 10.0)
                    .transition(Transition::ease(1.0, Ease::Linear))
                    .animate(AnimProperty::FRAME)
                    .build();
            });
        });
    });
    runtime.tick_animations(0.0);
    runtime.mark_rendered();

    compose(&mut runtime, 200.0, 100.0, |ui, _| {
        ui.stack("viewport").size(100.0, 80.0).clip().content(|ui| {
            ui.stack("content")
                .y(-4.0)
                .size(100.0, 160.0)
                .content(|ui| {
                    ui.rect("indicator")
                        .size(20.0, 10.0)
                        .transition(Transition::ease(1.0, Ease::Linear))
                        .animate(AnimProperty::FRAME)
                        .build();
                });
        });
    });

    assert!(runtime.tick_animations(0.016));

    let draw = runtime.draw_list();
    let indicator = draw
        .commands()
        .iter()
        .filter_map(rect_draw)
        .find(|draw| draw.id == "demo.indicator")
        .unwrap();

    assert_frame(indicator.frame, 0.0, -4.0, 20.0, 10.0);
}

#[test]
fn state_color_uses_smoothed_hover_blend() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("button")
            .size(40.0, 40.0)
            .states(Color::BLACK, Color::RED, Color::GREEN)
            .build();
    });
    runtime.tick_animations(0.0);

    runtime.update_pointer(PointerEvent::at(5.0, 5.0));
    assert!(runtime.tick_animations(0.016));

    let draw = runtime.draw_list();
    let button = draw
        .commands()
        .iter()
        .filter_map(rect_draw)
        .find(|draw| draw.id == "demo.button")
        .unwrap();

    assert!(button.color.r > 0.0);
    assert!(button.color.r < 1.0);
}

#[test]
fn pointer_press_and_release_produces_click_response() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("button").size(40.0, 30.0).interactive(true).build();
    });
    runtime.mark_rendered();

    runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
    assert_eq!(
        runtime
            .input
            .owners
            .pointer_active
            .as_ref()
            .map(|id| id.node_id().as_str()),
        Some("demo.button")
    );
    assert_eq!(
        runtime
            .input
            .owners
            .pointer_capture
            .as_ref()
            .map(|id| id.node_id().as_str()),
        Some("demo.button")
    );
    runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));

    assert!(runtime.diagnostics().response("button").clicked());
    assert!(runtime.input.owners.pointer_active.is_none());
    assert!(runtime.input.owners.pointer_capture.is_none());
    assert!(runtime.needs_render());
}

#[test]
fn pointer_hover_leave_reports_changed_response() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("button").size(40.0, 30.0).interactive(true).build();
    });
    runtime.mark_rendered();

    runtime.update_pointer(PointerEvent::at(10.0, 10.0));
    assert!(runtime.diagnostics().response("button").hovered());
    assert_eq!(
        runtime
            .input
            .owners
            .pointer_hover
            .as_ref()
            .map(|id| id.node_id().as_str()),
        Some("demo.button")
    );
    assert!(runtime.needs_render());
    assert!(runtime
        .diagnostics()
        .current_snapshot()
        .invalidations
        .iter()
        .any(|invalidation| {
            invalidation.target.id() == "demo"
                && invalidation.source.kind() == "runtime"
                && invalidation.source.label() == "pointer_input"
                && invalidation.flags == DirtyFlags::DRAW
                && invalidation.pass_flags.request_draw
                && !invalidation.pass_flags.request_compose_ui
        }));

    runtime.update_pointer(PointerEvent::at(90.0, 90.0));
    let response = runtime.diagnostics().response("button");
    assert!(!response.hovered());
    assert!(response.changed());
}

#[test]
fn removed_hovered_element_clears_response_and_interaction_state() {
    #[derive(Default)]
    struct AppModel {
        show_button: bool,
    }

    let state = State::new(AppModel { show_button: true });
    let mut runtime = Runtime::new("demo");
    let compose_tree = |runtime: &mut Runtime, dirty: Vec<DirtyInput>| {
        let state = state.clone();
        compose_incremental_dirty(runtime, 120.0, 80.0, dirty, move |ui, _| {
            ui.retained_scope("panel", |ui| {
                let show_button = state
                    .signal(
                        "show_button",
                        |state| state.show_button,
                        |state, value| state.show_button = value,
                    )
                    .watch(ui);
                if show_button {
                    ui.rect("button").size(40.0, 30.0).interactive(true).build();
                }
            });
        });
    };

    compose_tree(&mut runtime, Vec::new());
    runtime.update_pointer(PointerEvent::at(10.0, 10.0));
    assert!(runtime.diagnostics().response("button").hovered());
    assert!(runtime.diagnostics().interaction("button").hovered);

    state
        .signal(
            "show_button",
            |state| state.show_button,
            |state, value| state.show_button = value,
        )
        .set(false);
    compose_tree(&mut runtime, state.take_dirty());

    assert!(runtime.diagnostics().find("button").is_none());
    assert!(!runtime.diagnostics().response("button").hovered());
    assert!(!runtime.diagnostics().response("button").pressed());
    assert!(!runtime.diagnostics().response("button").changed());
    let interaction = runtime.diagnostics().interaction("button");
    assert!(!interaction.hovered);
    assert!(!interaction.pressed);
    assert!(!interaction.active);
}

#[test]
fn press_capture_prevents_other_element_click() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.stack("root").size(100.0, 100.0).content(|ui| {
            ui.rect("a")
                .position(0.0, 0.0)
                .size(40.0, 40.0)
                .interactive(true)
                .on_drag(|_| {})
                .build();
            ui.rect("b")
                .position(50.0, 0.0)
                .size(40.0, 40.0)
                .interactive(true)
                .build();
        });
    });

    runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
    runtime.update_pointer(PointerEvent::dragged_to(60.0, 10.0, 50.0, 0.0));
    assert_eq!(
        runtime
            .input
            .owners
            .drag_owner
            .as_ref()
            .map(|id| id.node_id().as_str()),
        Some("demo.a")
    );
    runtime.update_pointer(PointerEvent::released_at(60.0, 10.0));

    assert!(!runtime.diagnostics().response("a").clicked());
    assert!(!runtime.diagnostics().response("b").clicked());
    assert!(runtime.input.owners.drag_owner.is_none());
    assert!(runtime.diagnostics().interaction("a").released);
}

#[test]
fn z_index_controls_topmost_hit_test() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.stack("root").size(100.0, 100.0).content(|ui| {
            ui.rect("low")
                .size(50.0, 50.0)
                .interactive(true)
                .z_index(0)
                .build();
            ui.rect("high")
                .size(50.0, 50.0)
                .interactive(true)
                .z_index(10)
                .build();
        });
    });

    runtime.update_pointer(PointerEvent::at(10.0, 10.0));

    assert!(!runtime.diagnostics().response("low").hovered());
    assert!(runtime.diagnostics().response("high").hovered());
}

#[test]
fn click_callback_runs_from_runtime_dispatch() {
    let clicks = Rc::new(Cell::new(0));
    let callback_clicks = clicks.clone();
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, move |ui, _| {
        let callback_clicks = callback_clicks.clone();
        ui.rect("button")
            .size(40.0, 30.0)
            .on_click(move || {
                callback_clicks.set(callback_clicks.get() + 1);
            })
            .build();
    });

    runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
    runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));

    assert_eq!(clicks.get(), 1);
    assert!(runtime.needs_compose());
}

#[test]
fn event_callback_records_typed_invalidation() {
    let clicks = Rc::new(Cell::new(0));
    let callback_clicks = clicks.clone();
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, move |ui, _| {
        let callback_clicks = callback_clicks.clone();
        ui.rect("button")
            .size(40.0, 30.0)
            .on_click(move || {
                callback_clicks.set(callback_clicks.get() + 1);
            })
            .build();
    });

    runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
    runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));

    let snapshot = runtime.diagnostics().current_snapshot();
    assert_eq!(clicks.get(), 1);
    assert!(snapshot.pass_flags.request_compose_ui);
    assert!(snapshot.pass_flags.request_reconcile);
    assert!(snapshot.pass_flags.request_draw);
    assert!(snapshot.invalidations.iter().any(|invalidation| {
        matches!(
            &invalidation.target,
            InvalidationTarget::Node(node) if node.as_str() == "demo.button"
        ) && invalidation.target.kind() == "node"
            && invalidation.source.kind() == "event"
            && invalidation.source.label() == "click"
    }));
    assert!(snapshot.events.iter().any(|event| {
        event.raw_event == "pointer"
            && event.target.role() == "node"
            && event.target.id() == "demo.button"
            && matches!(
                &event.target,
                EventTargetId::Node(node) if node.as_str() == "demo.button"
            )
            && event.command == "click"
            && event.callback
            && event.invalidation.as_ref().is_some_and(|invalidation| {
                matches!(
                    &invalidation.target,
                    InvalidationTarget::Node(node) if node.as_str() == "demo.button"
                ) && invalidation.target.kind() == "node"
                    && invalidation.source.kind() == "event"
                    && invalidation.source.label() == "click"
            })
    }));

    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("button").size(40.0, 30.0).on_click(|| {}).build();
    });
    assert!(runtime
        .diagnostics()
        .committed_snapshot()
        .invalidations
        .iter()
        .any(|invalidation| {
            invalidation.target.id() == "demo.button" && invalidation.source.label() == "click"
        }));
    assert!(runtime
        .diagnostics()
        .committed_snapshot()
        .events
        .iter()
        .any(|event| {
            event.target.id() == "demo.button" && event.command == "click" && event.callback
        }));
    assert!(runtime
        .diagnostics()
        .current_snapshot()
        .invalidations
        .is_empty());
    assert!(runtime.diagnostics().current_snapshot().events.is_empty());
}

#[test]
fn focus_callback_records_focus_invalidation_target() {
    let focused = Rc::new(Cell::new(None));
    let callback_focused = focused.clone();
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, move |ui, _| {
        let callback_focused = callback_focused.clone();
        ui.rect("input")
            .size(80.0, 24.0)
            .on_focus_changed(move |value| callback_focused.set(Some(value)))
            .build();
    });

    runtime.update_pointer(PointerEvent::pressed_at(5.0, 5.0));

    let snapshot = runtime.diagnostics().current_snapshot();
    assert_eq!(focused.get(), Some(true));
    assert!(snapshot.events.iter().any(|event| {
        event.raw_event == "focus"
            && event.target.role() == "focus"
            && event.target.id() == "demo.input"
            && matches!(
                &event.target,
                EventTargetId::Focus(node) if node.as_str() == "demo.input"
            )
            && event.command == "focus"
            && event.callback
            && event.invalidation.as_ref().is_some_and(|invalidation| {
                matches!(
                    &invalidation.target,
                    InvalidationTarget::Focus(node) if node.as_str() == "demo.input"
                ) && invalidation.target.kind() == "focus"
                    && invalidation.source.kind() == "event"
                    && invalidation.source.label() == "focus"
            })
    }));
    assert!(snapshot.invalidations.iter().any(|invalidation| {
        invalidation.target.kind() == "focus"
            && invalidation.target.id() == "demo.input"
            && invalidation.source.kind() == "event"
            && invalidation.source.label() == "focus"
    }));
}

#[test]
fn event_trace_records_commands_without_callbacks() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("button").size(40.0, 30.0).interactive(true).build();
    });

    runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
    runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));

    let snapshot = runtime.diagnostics().current_snapshot();
    assert!(snapshot.events.iter().any(|event| {
        event.raw_event == "pointer"
            && event.target.role() == "node"
            && event.target.id() == "demo.button"
            && event.command == "click"
            && !event.callback
            && event.invalidation.is_none()
    }));
    assert!(!snapshot.invalidations.iter().any(|invalidation| {
        invalidation.target.id() == "demo.button" && invalidation.source.kind() == "event"
    }));
    assert!(snapshot.invalidations.iter().any(|invalidation| {
        invalidation.target.id() == "demo"
            && invalidation.source.kind() == "runtime"
            && invalidation.source.label() == "pointer_input"
            && invalidation.flags == DirtyFlags::DRAW
    }));
}

#[test]
fn frame_pointer_event_queue_executes_commands_after_collection() {
    let clicks = Rc::new(Cell::new(0));
    let callback_clicks = clicks.clone();
    let mut runtime = Runtime::new("demo");
    runtime.frame(
        FrameInput::new(Screen::new(100.0, 100.0), 0.0),
        move |ui, _| {
            let callback_clicks = callback_clicks.clone();
            ui.rect("button")
                .size(40.0, 30.0)
                .on_click(move || {
                    callback_clicks.set(callback_clicks.get() + 1);
                })
                .build();
        },
    );

    runtime.frame(
        FrameInput::new(Screen::new(100.0, 100.0), 0.0).pointer_events([
            PointerEvent::pressed_at(10.0, 10.0),
            PointerEvent::released_at(10.0, 10.0),
        ]),
        |ui, _| {
            ui.rect("button").size(40.0, 30.0).on_click(|| {}).build();
        },
    );

    let snapshot = runtime.diagnostics().committed_snapshot();
    assert_eq!(clicks.get(), 1);
    assert!(snapshot.events.iter().any(|event| {
        event.raw_event == "pointer"
            && event.target.id() == "demo.button"
            && event.command == "press"
            && !event.callback
    }));
    assert!(snapshot.events.iter().any(|event| {
        event.raw_event == "pointer"
            && event.target.id() == "demo.button"
            && event.command == "click"
            && event.callback
    }));
    assert!(snapshot.invalidations.iter().any(|invalidation| {
        invalidation.target.id() == "demo.button" && invalidation.source.label() == "click"
    }));
}

#[test]
fn signal_dirty_records_enter_typed_invalidations() {
    #[derive(Default)]
    struct AppModel {
        selected: i32,
    }

    let state = State::new(AppModel::default());
    let mut runtime = Runtime::new("page");
    let compose = |runtime: &mut Runtime, dirty: Vec<DirtyInput>| {
        let state = state.clone();
        compose_incremental_dirty(runtime, 240.0, 80.0, dirty, move |ui, _| {
            ui.retained_scope("nav", |ui| {
                let selected = state
                    .signal(
                        "selected",
                        |state| state.selected,
                        |state, value| state.selected = value,
                    )
                    .watch(ui);
                ui.text("nav.label")
                    .size(120.0, 40.0)
                    .text(format!("selected {selected}"))
                    .build();
            });
        });
    };

    compose(&mut runtime, Vec::new());
    state
        .signal(
            "selected",
            |state| state.selected,
            |state, value| state.selected = value,
        )
        .set(1);

    let dirty = state.take_dirty();
    assert_eq!(dirty.len(), 1);
    assert_eq!(dirty[0].id(), "page.nav");
    assert_eq!(dirty[0].source(), Some("selected"));
    assert_eq!(
        dirty[0].source_key().map(SignalKey::as_str),
        Some("selected")
    );

    compose(&mut runtime, dirty);
    let snapshot = runtime.diagnostics().committed_snapshot();
    let signal_invalidation = snapshot
        .invalidations
        .iter()
        .find(|invalidation| {
            matches!(
                &invalidation.target,
                InvalidationTarget::Scope(scope) if scope.as_str() == "page.nav"
            ) && invalidation.target.kind() == "scope"
                && invalidation.source.kind() == "signal"
                && invalidation.source.label() == "selected"
        })
        .expect("signal dirty should produce typed invalidation");
    assert_eq!(
        signal_invalidation.source,
        InvalidationSource::Signal(SignalKey::static_str("selected"))
    );
    assert!(signal_invalidation.pass_flags.request_compose_ui);
    assert!(signal_invalidation.pass_flags.request_reconcile);
    assert!(signal_invalidation.pass_flags.request_draw);
    assert!(snapshot.pass_flags.request_compose_ui);
    assert!(snapshot.pass_flags.request_reconcile);
    assert!(snapshot.pass_flags.request_draw);
}

#[test]
fn right_click_dispatches_context_menu_callback() {
    let opened = Rc::new(Cell::new(false));
    let callback_opened = opened.clone();
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, move |ui, _| {
        let callback_opened = callback_opened.clone();
        ui.rect("target")
            .size(40.0, 30.0)
            .on_context_menu(move |_, _| {
                callback_opened.set(true);
            })
            .build();
    });

    runtime.update_pointer(PointerEvent::right_pressed_at(10.0, 10.0));

    assert!(opened.get());
}

#[test]
fn focused_element_receives_keyboard_input() {
    let text = Rc::new(RefCell::new(String::new()));
    let callback_text = text.clone();
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, move |ui, _| {
        let callback_text = callback_text.clone();
        ui.rect("input")
            .size(80.0, 24.0)
            .on_text_input(move |event| {
                callback_text.borrow_mut().push_str(&event.text);
            })
            .build();
    });

    runtime.update_pointer(PointerEvent::pressed_at(5.0, 5.0));
    runtime.update_keyboard(KeyboardEvent {
        text: "A".to_string(),
        ..KeyboardEvent::default()
    });

    assert_eq!(text.borrow().as_str(), "A");
    assert_eq!(runtime.diagnostics().focused_id(), Some("demo.input"));
    assert_eq!(
        runtime
            .input
            .owners
            .keyboard_focus
            .as_ref()
            .map(|id| id.node_id().as_str()),
        Some("demo.input")
    );
    assert_eq!(
        runtime
            .input
            .owners
            .text_focus
            .as_ref()
            .map(|id| id.node_id().as_str()),
        Some("demo.input")
    );
    assert_eq!(
        runtime
            .input
            .owners
            .ime_owner
            .as_ref()
            .map(|id| id.node_id().as_str()),
        Some("demo.input")
    );
    let snapshot = runtime.diagnostics().current_snapshot();
    assert!(snapshot.events.iter().any(|event| {
        event.raw_event == "focus"
            && event.target.role() == "focus"
            && event.target.id() == "demo.input"
            && event.command == "focus"
    }));
    assert!(snapshot.events.iter().any(|event| {
        event.raw_event == "keyboard"
            && event.target.role() == "text"
            && event.target.id() == "demo.input"
            && matches!(
                &event.target,
                EventTargetId::Text(node) if node.as_str() == "demo.input"
            )
            && event.command == "text_input"
            && event.callback
            && event.invalidation.as_ref().is_some_and(|invalidation| {
                matches!(
                    &invalidation.target,
                    InvalidationTarget::Text(node) if node.as_str() == "demo.input"
                ) && invalidation.target.kind() == "text"
                    && invalidation.source.kind() == "event"
                    && invalidation.source.label() == "text_input"
            })
    }));
    assert!(snapshot.invalidations.iter().any(|invalidation| {
        invalidation.target.kind() == "text"
            && invalidation.target.id() == "demo.input"
            && invalidation.source.kind() == "event"
            && invalidation.source.label() == "text_input"
    }));
}

#[test]
fn debug_snapshot_reports_separate_input_owners() {
    let mut runtime = Runtime::new("demo");

    compose(&mut runtime, 240.0, 120.0, |ui, _| {
        ui.rect("input")
            .position(0.0, 0.0)
            .size(80.0, 30.0)
            .on_text_input(|_| {})
            .build();
        ui.rect("scroll")
            .position(0.0, 50.0)
            .size(120.0, 50.0)
            .on_scroll(|_| {})
            .build();
        ui.rect("drag")
            .position(140.0, 0.0)
            .size(80.0, 40.0)
            .on_drag(|_| {})
            .build();
    });

    runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
    runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));
    runtime.update_pointer(PointerEvent::at(10.0, 60.0));
    runtime.update_scroll(ScrollEvent { x: 0.0, y: 4.0 });

    let snapshot = runtime.diagnostics().current_snapshot();
    assert_eq!(snapshot.focused_id.as_deref(), Some("demo.input"));
    assert_eq!(snapshot.keyboard_focus_id.as_deref(), Some("demo.input"));
    assert_eq!(snapshot.text_focus_id.as_deref(), Some("demo.input"));
    assert_eq!(snapshot.ime_owner_id.as_deref(), Some("demo.input"));
    assert_eq!(snapshot.pointer_hover_id.as_deref(), Some("demo.scroll"));
    assert_eq!(snapshot.hovered_id.as_deref(), Some("demo.scroll"));
    assert_eq!(snapshot.scroll_owner_id.as_deref(), Some("demo.scroll"));
    assert_eq!(
        snapshot.hovered_node_id.as_ref().map(NodeId::as_str),
        Some("demo.scroll")
    );
    assert_eq!(
        snapshot
            .input_owners
            .keyboard_focus
            .as_ref()
            .map(NodeId::as_str),
        Some("demo.input")
    );
    assert_eq!(
        snapshot
            .input_owners
            .text_focus
            .as_ref()
            .map(NodeId::as_str),
        Some("demo.input")
    );
    assert_eq!(
        snapshot.input_owners.ime_owner.as_ref().map(NodeId::as_str),
        Some("demo.input")
    );
    assert_eq!(
        snapshot
            .input_owners
            .pointer_hover
            .as_ref()
            .map(NodeId::as_str),
        Some("demo.scroll")
    );
    assert_eq!(
        snapshot
            .input_owners
            .scroll_owner
            .as_ref()
            .map(NodeId::as_str),
        Some("demo.scroll")
    );
    assert_eq!(snapshot.active_id, None);
    assert_eq!(snapshot.pointer_active_id, None);
    assert_eq!(snapshot.pointer_capture_id, None);
    assert_eq!(snapshot.drag_owner_id, None);
    assert_eq!(snapshot.input_owners.pointer_active, None);
    assert_eq!(snapshot.input_owners.pointer_capture, None);
    assert_eq!(snapshot.input_owners.drag_owner, None);

    runtime.update_pointer(PointerEvent::pressed_at(150.0, 10.0));
    runtime.update_pointer(PointerEvent::dragged_to(160.0, 20.0, 10.0, 10.0));

    let snapshot = runtime.diagnostics().current_snapshot();
    assert_eq!(snapshot.focused_id, None);
    assert_eq!(snapshot.keyboard_focus_id, None);
    assert_eq!(snapshot.text_focus_id, None);
    assert_eq!(snapshot.ime_owner_id, None);
    assert_eq!(snapshot.active_id.as_deref(), Some("demo.drag"));
    assert_eq!(snapshot.pointer_active_id.as_deref(), Some("demo.drag"));
    assert_eq!(snapshot.pointer_capture_id.as_deref(), Some("demo.drag"));
    assert_eq!(snapshot.drag_owner_id.as_deref(), Some("demo.drag"));
    assert_eq!(snapshot.input_owners.keyboard_focus, None);
    assert_eq!(snapshot.input_owners.text_focus, None);
    assert_eq!(snapshot.input_owners.ime_owner, None);
    assert_eq!(
        snapshot
            .input_owners
            .pointer_active
            .as_ref()
            .map(NodeId::as_str),
        Some("demo.drag")
    );
    assert_eq!(
        snapshot
            .input_owners
            .pointer_capture
            .as_ref()
            .map(NodeId::as_str),
        Some("demo.drag")
    );
    assert_eq!(
        snapshot
            .input_owners
            .drag_owner
            .as_ref()
            .map(NodeId::as_str),
        Some("demo.drag")
    );
}

#[test]
fn focused_non_text_element_does_not_receive_text_input() {
    let text = Rc::new(RefCell::new(String::new()));
    let callback_text = text.clone();
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 140.0, 40.0, move |ui, _| {
        ui.row("root").size(140.0, 30.0).content(|ui| {
            ui.rect("button")
                .size(40.0, 24.0)
                .focusable(true)
                .on_click(|| {})
                .build();
            let callback_text = callback_text.clone();
            ui.rect("input")
                .size(80.0, 24.0)
                .on_text_input(move |event| {
                    callback_text.borrow_mut().push_str(&event.text);
                })
                .build();
        });
    });

    runtime.update_pointer(PointerEvent::pressed_at(5.0, 5.0));
    assert_eq!(runtime.diagnostics().focused_id(), Some("demo.button"));
    assert_eq!(runtime.diagnostics().text_focused_id(), None);
    assert!(runtime.has_keyboard_capture());

    let changed = runtime.update_keyboard(KeyboardEvent {
        text: "A".to_string(),
        ..KeyboardEvent::default()
    });

    assert!(!changed);
    assert_eq!(text.borrow().as_str(), "");
}

#[test]
fn frame_input_pass_clears_text_focus_before_keyboard_dispatch() {
    let text = Rc::new(RefCell::new(String::new()));
    let callback_text = text.clone();
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 140.0, 60.0, move |ui, _| {
        let callback_text = callback_text.clone();
        ui.rect("input")
            .position(0.0, 0.0)
            .size(80.0, 24.0)
            .on_text_input(move |event| {
                callback_text.borrow_mut().push_str(&event.text);
            })
            .build();
    });

    runtime.update_pointer(PointerEvent::pressed_at(5.0, 5.0));
    assert_eq!(runtime.diagnostics().text_focused_id(), Some("demo.input"));

    runtime.frame(
        FrameInput::new(Screen::new(140.0, 60.0), 0.0)
            .pointer(PointerEvent::pressed_at(120.0, 40.0))
            .keyboard(KeyboardEvent {
                text: "A".to_string(),
                ..KeyboardEvent::default()
            }),
        |ui, _| {
            ui.rect("input")
                .position(0.0, 0.0)
                .size(80.0, 24.0)
                .on_text_input(|_| {})
                .build();
        },
    );

    assert_eq!(runtime.diagnostics().focused_id(), None);
    assert_eq!(runtime.diagnostics().text_focused_id(), None);
    assert_eq!(text.borrow().as_str(), "");
}

#[test]
fn removed_focused_text_element_clears_keyboard_and_ime_owners() {
    #[derive(Default)]
    struct AppModel {
        show_input: bool,
    }

    let state = State::new(AppModel { show_input: true });
    let mut runtime = Runtime::new("demo");
    let compose_tree = |runtime: &mut Runtime, dirty: Vec<DirtyInput>| {
        let state = state.clone();
        compose_incremental_dirty(runtime, 140.0, 60.0, dirty, move |ui, _| {
            ui.retained_scope("panel", |ui| {
                let show_input = state
                    .signal(
                        "show_input",
                        |state| state.show_input,
                        |state, value| state.show_input = value,
                    )
                    .watch(ui);
                if show_input {
                    ui.rect("input")
                        .position(0.0, 0.0)
                        .size(80.0, 24.0)
                        .ime_rect(4.0, 4.0, 8.0, 16.0)
                        .on_text_input(|_| {})
                        .build();
                }
            });
        });
    };

    compose_tree(&mut runtime, Vec::new());
    runtime.update_pointer(PointerEvent::pressed_at(5.0, 5.0));
    assert_eq!(runtime.diagnostics().focused_id(), Some("demo.input"));
    assert_eq!(runtime.diagnostics().text_focused_id(), Some("demo.input"));
    assert!(runtime.focused_ime_rect().is_some());
    assert!(runtime.has_keyboard_capture());

    state
        .signal(
            "show_input",
            |state| state.show_input,
            |state, value| state.show_input = value,
        )
        .set(false);
    compose_tree(&mut runtime, state.take_dirty());

    assert!(runtime.diagnostics().find("input").is_none());
    assert_eq!(runtime.diagnostics().focused_id(), None);
    assert_eq!(runtime.diagnostics().text_focused_id(), None);
    assert_eq!(runtime.focused_ime_rect(), None);
    assert!(!runtime.has_keyboard_capture());
}

#[test]
fn focused_ime_rect_tracks_committed_layout_after_recompose() {
    let mut runtime = Runtime::new("demo");
    let compose_input = |runtime: &mut Runtime, x: f32| {
        compose(runtime, 160.0, 80.0, move |ui, _| {
            ui.rect("input")
                .position(x, 10.0)
                .size(80.0, 24.0)
                .ime_rect(3.0, 4.0, 8.0, 16.0)
                .on_text_input(|_| {})
                .build();
        });
    };

    compose_input(&mut runtime, 4.0);
    runtime.update_pointer(PointerEvent::pressed_at(8.0, 14.0));
    assert_eq!(
        runtime.focused_ime_rect(),
        Some(LayoutRect::new(7.0, 14.0, 8.0, 16.0))
    );

    compose_input(&mut runtime, 30.0);

    assert_eq!(runtime.diagnostics().text_focused_id(), Some("demo.input"));
    assert_eq!(
        runtime.focused_ime_rect(),
        Some(LayoutRect::new(33.0, 14.0, 8.0, 16.0))
    );
}

#[test]
fn scroll_dispatches_to_topmost_scrollable_element() {
    let amount = Rc::new(Cell::new(0.0));
    let callback_amount = amount.clone();
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, move |ui, _| {
        let callback_amount = callback_amount.clone();
        ui.rect("scroll")
            .size(80.0, 80.0)
            .on_scroll(move |event| {
                callback_amount.set(callback_amount.get() + event.y);
            })
            .build();
    });

    runtime.update_pointer(PointerEvent::at(5.0, 5.0));
    runtime.update_scroll(ScrollEvent { x: 0.0, y: 3.0 });

    assert_eq!(amount.get(), 3.0);
    assert_eq!(
        runtime
            .input
            .owners
            .scroll_owner
            .as_ref()
            .map(|id| id.node_id().as_str()),
        Some("demo.scroll")
    );
    let snapshot = runtime.diagnostics().current_snapshot();
    assert!(snapshot.events.iter().any(|event| {
        event.raw_event == "scroll"
            && event.target.role() == "scroll"
            && event.target.id() == "demo.scroll"
            && matches!(
                &event.target,
                EventTargetId::Scroll(node) if node.as_str() == "demo.scroll"
            )
            && event.command == "scroll"
            && event.callback
            && event.invalidation.as_ref().is_some_and(|invalidation| {
                matches!(
                    &invalidation.target,
                    InvalidationTarget::Scroll(node) if node.as_str() == "demo.scroll"
                ) && invalidation.target.kind() == "scroll"
                    && invalidation.source.kind() == "event"
                    && invalidation.source.label() == "scroll"
            })
    }));
    assert!(snapshot.invalidations.iter().any(|invalidation| {
        invalidation.target.kind() == "scroll"
            && invalidation.target.id() == "demo.scroll"
            && invalidation.source.kind() == "event"
            && invalidation.source.label() == "scroll"
    }));
}

#[test]
fn rounded_clip_excludes_corner_hits() {
    let clicks = Rc::new(Cell::new(0));
    let callback_clicks = clicks.clone();
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 120.0, 120.0, move |ui, _| {
        let callback_clicks = callback_clicks.clone();
        ui.stack("viewport")
            .size(100.0, 100.0)
            .rounded_clip(20.0)
            .content(|ui| {
                ui.rect("child")
                    .size(100.0, 100.0)
                    .on_click(move || callback_clicks.set(callback_clicks.get() + 1))
                    .build();
            });
    });

    runtime.update_pointer(PointerEvent::pressed_at(1.0, 1.0));
    runtime.update_pointer(PointerEvent::released_at(1.0, 1.0));
    assert_eq!(clicks.get(), 0);

    runtime.update_pointer(PointerEvent::pressed_at(20.0, 20.0));
    runtime.update_pointer(PointerEvent::released_at(20.0, 20.0));
    assert_eq!(clicks.get(), 1);
}

#[test]
fn fullscreen_modal_layer_blocks_underlying_hits() {
    let under_clicks = Rc::new(Cell::new(0));
    let modal_clicks = Rc::new(Cell::new(0));
    let under = under_clicks.clone();
    let modal = modal_clicks.clone();
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 200.0, 120.0, move |ui, _| {
        let under = under.clone();
        let modal = modal.clone();
        ui.rect("under")
            .size(200.0, 120.0)
            .z_index(0)
            .on_click(move || under.set(under.get() + 1))
            .build();
        ui.stack("modal")
            .size(200.0, 120.0)
            .z_index(1000)
            .content(|ui| {
                ui.rect("modal.backdrop")
                    .size(200.0, 120.0)
                    .on_click(move || modal.set(modal.get() + 1))
                    .build();
            });
    });

    runtime.update_pointer(PointerEvent::pressed_at(20.0, 20.0));
    runtime.update_pointer(PointerEvent::released_at(20.0, 20.0));

    assert_eq!(under_clicks.get(), 0);
    assert_eq!(modal_clicks.get(), 1);
    assert!(!runtime.diagnostics().response("under").hovered());
    assert!(runtime.diagnostics().response("modal.backdrop").clicked());
}

#[test]
fn timer_callback_runs_after_elapsed_duration() {
    let fired = Rc::new(Cell::new(false));
    let callback_fired = fired.clone();
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, move |ui, _| {
        let callback_fired = callback_fired.clone();
        ui.rect("timer")
            .size(1.0, 1.0)
            .on_timer(0.10, move || {
                callback_fired.set(true);
            })
            .build();
    });

    assert!(!runtime.tick_timers(0.05));
    assert!(runtime.tick_timers(0.05));
    assert!(fired.get());

    let snapshot = runtime.diagnostics().current_snapshot();
    assert!(snapshot.pass_flags.request_compose_ui);
    assert!(snapshot.invalidations.iter().any(|invalidation| {
        matches!(
            &invalidation.target,
            InvalidationTarget::Node(node) if node.as_str() == "demo.timer"
        ) && invalidation.target.kind() == "node"
            && invalidation.source.kind() == "timer"
            && invalidation.source.label() == "timer"
    }));
    assert!(snapshot.events.iter().any(|event| {
        event.raw_event == "timer"
            && event.target.role() == "node"
            && event.target.id() == "demo.timer"
            && event.command == "timer"
            && event.callback
            && event.invalidation.as_ref().is_some_and(|invalidation| {
                matches!(
                    &invalidation.target,
                    InvalidationTarget::Node(node) if node.as_str() == "demo.timer"
                ) && invalidation.source.kind() == "timer"
            })
    }));
}

#[test]
fn removed_timer_element_clears_timer_state_on_commit() {
    let mut runtime = Runtime::new("demo");
    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.rect("timer")
            .size(10.0, 10.0)
            .on_timer(1.0, || {})
            .build();
    });
    assert!(!runtime.tick_timers(0.1));
    assert!(runtime
        .timing
        .timers
        .contains_key(&NodeId::new("demo.timer")));

    compose(&mut runtime, 100.0, 100.0, |ui, _| {
        ui.stack("root").size(100.0, 100.0).build();
    });

    assert!(!runtime
        .timing
        .timers
        .contains_key(&NodeId::new("demo.timer")));
}

fn assert_frame(frame: LayoutRect, x: f32, y: f32, width: f32, height: f32) {
    const EPSILON: f32 = 0.001;
    assert!(
        (frame.x - x).abs() < EPSILON
            && (frame.y - y).abs() < EPSILON
            && (frame.width - width).abs() < EPSILON
            && (frame.height - height).abs() < EPSILON,
        "frame mismatch: got {:?}, expected ({x}, {y}, {width}, {height})",
        frame
    );
}

fn rect_draw(command: &UiDrawCommand) -> Option<&UiRectDraw> {
    match command {
        UiDrawCommand::Rect(draw) => Some(draw),
        _ => None,
    }
}
