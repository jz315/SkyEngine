//! EUI-NEO component builders ported as primitive composition helpers.

pub mod badge;
pub mod bar_chart;
pub mod button;
pub mod checkbox;
pub mod color_picker;
pub mod context_menu;
pub mod data_table;
pub mod date_picker;
pub mod dialog;
pub mod dropdown;
pub mod image;
pub mod input;
pub(crate) mod layout;
pub mod line_chart;
pub mod nav_group;
pub mod panel;
pub mod pie_chart;
pub mod popover;
pub mod progress;
pub mod radio;
mod scroll;
pub mod segmented;
pub mod skin;
pub mod slider;
pub mod switch;
pub mod tabs;
pub mod text;
pub mod theme;
pub mod time_picker;
pub mod toast;
pub mod virtual_list;

pub use badge::{badge, BadgeBuilder, BadgeStyle};
pub use bar_chart::{bar_chart, BarChartBuilder, BarChartStyle};
pub use button::{button, ButtonBuilder, ButtonStyle};
pub use checkbox::{checkbox, CheckboxBuilder, CheckboxStyle};
pub use color_picker::{color_picker, ColorPickerBuilder, ColorPickerStyle};
pub use context_menu::{context_menu, ContextMenuBuilder, ContextMenuStyle};
pub use data_table::{data_table, DataTableBuilder, DataTableStyle};
pub use date_picker::{date_picker, DatePickerBuilder, DatePickerStyle};
pub use dialog::{dialog, DialogBuilder, DialogStyle};
pub use dropdown::{dropdown, DropdownBuilder, DropdownStyle};
pub use image::{image, image_with_style, image_with_theme, ImageStyle};
pub use input::{input, InputBuilder, InputStyle};
pub use line_chart::{line_chart, LineChartBuilder, LineChartStyle};
pub use nav_group::{nav_group, BoundNavGroupBuilder, NavGroupBuilder};
pub use panel::{panel, panel_with_style, panel_with_theme, PanelStyle};
pub use pie_chart::{pie_chart, PieChartBuilder, PieChartStyle};
pub use popover::{popover, PopoverBuilder, PopoverPlacement};
pub use progress::{progress, ProgressBuilder, ProgressStyle};
pub use radio::{radio, RadioBuilder, RadioStyle};
pub use scroll::{
    scroll_x, scroll_xy, scroll_y, ScrollXBuilder, ScrollXYBuilder, ScrollYBuilder, ScrollbarStyle,
};
pub use segmented::{segmented, SegmentedBuilder, SegmentedStyle};
pub use skin::{
    skin_button, skin_checkbox, skin_icon_button, skin_panel, skin_slider, skin_status_bar,
    skin_status_ribbon, ButtonSkinSource, CheckboxSkinSource, PanelSkinSource, SkinButtonBuilder,
    SkinCheckboxBuilder, SkinIconButtonBuilder, SkinPanelBuilder, SkinSliderBuilder,
    SkinStatusBarBuilder, SkinStatusRibbonBuilder, SliderSkinSource,
};
pub use slider::{slider, SliderBuilder, SliderStyle};
pub use switch::{switch, SwitchBuilder, SwitchStyle};
pub use tabs::{tabs, TabsBuilder, TabsStyle};
pub use text::{
    body_text_style, label, label_with_theme, measure_text_width, subtitle_text_style, text,
    text_with_style, text_with_theme, title_text_style, TextStyle,
};
pub use time_picker::{time_picker, TimePickerBuilder, TimePickerStyle};
pub use toast::{toast, ToastBuilder, ToastStyle};
pub use virtual_list::{virtual_list, VirtualListBuilder, VirtualListItem, VirtualListRange};

#[cfg(test)]
mod tests {
    use super::scroll::scrollbar;
    use super::{
        badge, button, checkbox, color_picker, context_menu, date_picker, dialog, dropdown,
        image_with_style, input, nav_group, popover, progress, radio, segmented, skin_button,
        slider, switch, tabs, time_picker, toast, PopoverPlacement,
    };
    use crate::expert::UiDrawCommand;
    use crate::test_support::compose;
    use crate::Color;
    use crate::{
        ButtonSkin, DirtyInput, EdgeInsets, FontRef, FrameInput, ImageFit, ImageRef,
        InvalidationTarget, KeyboardEvent, LayerAnchorSource, LayerKind, LayerLifecycleAction,
        LayerPlacement, LayerPointerAction, LayoutRect, NeoSkin, OutsideClickPolicy, PointerEvent,
        Runtime, Screen, ScrollEvent, Size, Slice, State,
    };
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn button_composes_source_shaped_stack_background_and_content() {
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 320.0, 180.0, |ui, _| {
            button(ui, "start").size(120.0, 40.0).text("Start").build();
        });

        assert!(runtime.find("start").is_some());
        assert!(runtime.find("start.bg").is_some());
        assert!(runtime.find("start.content").is_some());
        assert!(runtime.find("start.text").is_some());
    }

    #[test]
    fn button_layout_supports_natural_width_min_width_and_grow() {
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 420.0, 80.0, |ui, _| {
            ui.row("toolbar").size(420.0, 44.0).gap(12.0).content(|ui| {
                button(ui, "filter")
                    .text("Filter")
                    .height(40.0)
                    .min_width(96.0)
                    .build();
                button(ui, "primary")
                    .text("New Task")
                    .height(40.0)
                    .min_width(112.0)
                    .grow(1.0)
                    .build();
            });
        });

        assert_eq!(runtime.find("filter").unwrap().frame.width, 96.0);
        assert_eq!(runtime.find("primary").unwrap().frame.width, 312.0);
        assert_eq!(runtime.find("primary.bg").unwrap().frame.width, 312.0);
    }

    #[test]
    fn button_selected_promotes_secondary_theme_to_primary_visual_target() {
        let tokens = super::theme::dark_theme_colors();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 240.0, 80.0, move |ui, _| {
            button(ui, "page")
                .size(120.0, 40.0)
                .text("Page")
                .secondary_theme(tokens)
                .selected(true)
                .build();
        });

        assert_eq!(runtime.find("page.bg").unwrap().color, tokens.primary);
    }

    #[test]
    fn input_layout_supports_fill_children_when_grown() {
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 460.0, 80.0, |ui, _| {
            ui.row("toolbar").size(460.0, 44.0).gap(12.0).content(|ui| {
                input(ui, "search")
                    .height(40.0)
                    .min_width(180.0)
                    .grow(1.0)
                    .build();
                button(ui, "new").size(96.0, 40.0).text("New").build();
            });
        });

        assert_eq!(runtime.find("search").unwrap().frame.width, 352.0);
        assert_eq!(runtime.find("search.hit").unwrap().frame.width, 352.0);
    }

    #[test]
    fn badge_uses_natural_width_and_shared_layout_constraints() {
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 420.0, 80.0, |ui, _| {
            ui.row("toolbar").size(420.0, 34.0).gap(10.0).content(|ui| {
                badge(ui, "ready").text("Ready").build();
                badge(ui, "state")
                    .text("Running")
                    .min_width(120.0)
                    .grow(1.0)
                    .build();
            });
        });

        let ready = runtime.find("ready").unwrap().frame;
        assert!(ready.width > 48.0);
        assert!(ready.width < 120.0);
        assert_eq!(
            runtime.find("state").unwrap().frame.width,
            420.0 - ready.width - 10.0
        );
        assert_eq!(
            runtime.find("state.bg").unwrap().frame.width,
            runtime.find("state").unwrap().frame.width
        );
    }

    #[test]
    fn progress_clamps_value_to_unit_range() {
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 400.0, 80.0, |ui, _| {
            progress(ui, "loading").size(200.0, 10.0).value(2.0).build();
        });

        let fill = runtime.find("loading.fill").unwrap();
        assert_eq!(fill.frame.width, 200.0);
    }

    #[test]
    fn slider_press_dispatches_clamped_value_callback() {
        let value = Rc::new(Cell::new(-1.0));
        let callback_value = value.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 400.0, 80.0, move |ui, _| {
            let callback_value = callback_value.clone();
            slider(ui, "volume")
                .size(200.0, 20.0)
                .on_change(move |next| callback_value.set(next))
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(100.0, 10.0));

        assert!((value.get() - 0.5).abs() < 0.001);
    }

    #[test]
    fn checkbox_click_reports_next_checked_value() {
        let checked = Rc::new(Cell::new(false));
        let callback_checked = checked.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 240.0, 80.0, move |ui, _| {
            let callback_checked = callback_checked.clone();
            checkbox(ui, "sound")
                .checked(false)
                .on_change(move |next| callback_checked.set(next))
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(2.0, 2.0));
        runtime.update_pointer(PointerEvent::released_at(2.0, 2.0));

        assert!(checked.get());
    }

    struct BoundWidgetState {
        checked: bool,
        slider: f32,
        text: String,
        selected: i32,
        open: bool,
        offset: f32,
        date: [i32; 3],
        time: [i32; 2],
        color: Color,
    }

    impl Default for BoundWidgetState {
        fn default() -> Self {
            Self {
                checked: false,
                slider: 0.0,
                text: String::new(),
                selected: 0,
                open: false,
                offset: 0.0,
                date: [0, 0, 0],
                time: [0, 0],
                color: Color::WHITE,
            }
        }
    }

    #[test]
    fn checkbox_signal_writes_clicked_value_to_state() {
        let state = State::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 240.0, 80.0, move |ui, _| {
            let checked = compose_state.signal(
                "test.signal",
                |state| state.checked,
                |state, value| state.checked = value,
            );
            checkbox(ui, "sound").signal(checked).text("Sound").build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(2.0, 2.0));
        runtime.update_pointer(PointerEvent::released_at(2.0, 2.0));

        assert!(state.read(|state| state.checked));
    }

    #[test]
    fn switch_signal_writes_clicked_value_to_state() {
        let state = State::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 240.0, 80.0, move |ui, _| {
            let checked = compose_state.signal(
                "test.signal",
                |state| state.checked,
                |state, value| state.checked = value,
            );
            switch(ui, "night").signal(checked).label("Night").build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(2.0, 2.0));
        runtime.update_pointer(PointerEvent::released_at(2.0, 2.0));

        assert!(state.read(|state| state.checked));
    }

    #[test]
    fn slider_signal_writes_pressed_value_to_state() {
        let state = State::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 320.0, 80.0, move |ui, _| {
            let slider_value = compose_state.signal(
                "test.signal",
                |state| state.slider,
                |state, value| state.slider = value,
            );
            slider(ui, "volume")
                .size(200.0, 20.0)
                .signal(slider_value)
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(100.0, 10.0));

        assert!((state.read(|state| state.slider) - 0.5).abs() < 0.001);
    }

    #[test]
    fn input_signal_writes_text_events_to_state() {
        let state = State::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 320.0, 120.0, move |ui, _| {
            let text = compose_state.signal(
                "test.signal",
                |state| state.text.clone(),
                |state, value| state.text = value,
            );
            input(ui, "name").text_signal(text).build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(4.0, 4.0));
        runtime.update_keyboard(KeyboardEvent {
            text: "Sky".to_string(),
            ..KeyboardEvent::default()
        });

        assert_eq!(state.read(|state| state.text.clone()), "Sky");
    }

    #[test]
    fn input_frame_order_focuses_types_and_recomposes_dirty_text() {
        let state = State::new(BoundWidgetState::default());
        let mut runtime = Runtime::new("page");

        let frame = |runtime: &mut Runtime, input_frame: FrameInput| {
            let dirty_state = state.clone();
            let compose_state = state.clone();
            runtime.frame_incremental(
                input_frame,
                move || dirty_state.take_dirty(),
                move |ui, _| {
                    let text = compose_state.signal(
                        "text",
                        |state| state.text.clone(),
                        |state, value| state.text = value,
                    );
                    input(ui, "name")
                        .size(160.0, 40.0)
                        .text_signal(text)
                        .build();
                },
            );
        };

        frame(&mut runtime, FrameInput::new(Screen::new(320.0, 80.0), 0.0));
        frame(
            &mut runtime,
            FrameInput::new(Screen::new(320.0, 80.0), 0.0).pointer_events([
                PointerEvent::pressed_at(8.0, 8.0),
                PointerEvent::released_at(8.0, 8.0),
            ]),
        );
        assert_eq!(runtime.text_focused_id(), Some("page.name.hit"));

        frame(
            &mut runtime,
            FrameInput::new(Screen::new(320.0, 80.0), 0.0).keyboard(KeyboardEvent {
                text: "abc".to_string(),
                ..KeyboardEvent::default()
            }),
        );

        assert_eq!(state.read(|state| state.text.clone()), "abc");
        assert_eq!(
            runtime
                .find("name.text")
                .expect("input text element should be present")
                .text,
            "abc"
        );
        assert_eq!(runtime.debug_snapshot().dirty_ids, vec!["page.name"]);
        assert!(runtime
            .debug_snapshot()
            .invalidations
            .iter()
            .any(|invalidation| {
                invalidation.target.id() == "page.name.hit"
                    && invalidation.source.kind() == "event"
                    && invalidation.source.label() == "text_input"
            }));
    }

    #[test]
    fn scrollbar_wheel_reports_clamped_offset_change() {
        let offset = Rc::new(Cell::new(-1.0));
        let callback_offset = offset.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 80.0, 240.0, move |ui, _| {
            let callback_offset = callback_offset.clone();
            scrollbar(ui, "list.scrollbar")
                .size(8.0, 100.0)
                .offset(20.0)
                .viewport(100.0)
                .content(300.0)
                .step(10.0)
                .on_change(move |next| callback_offset.set(next))
                .build();
        });

        runtime.update_pointer(PointerEvent::at(2.0, 2.0));
        runtime.update_scroll(ScrollEvent { x: 0.0, y: -3.0 });

        assert_eq!(offset.get(), 50.0);
    }

    #[test]
    fn scroll_y_explicit_content_height_composes_viewport_content_and_scrollbar() {
        let offset = Rc::new(Cell::new(-1.0));
        let callback_offset = offset.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 160.0, 120.0, move |ui, _| {
            let callback_offset = callback_offset.clone();
            ui.scroll_y("list")
                .size(120.0, 80.0)
                .content_height(200.0)
                .offset(24.0)
                .step(10.0)
                .padding(8.0)
                .gap(6.0)
                .on_change(move |next| callback_offset.set(next))
                .content(|ui| {
                    ui.rect("row.a").size(Size::fill(), 30.0).build();
                    ui.rect("row.b").size(Size::fill(), 30.0).build();
                });
        });

        assert_eq!(runtime.find("list").unwrap().frame.width, 120.0);
        assert_eq!(runtime.find("list.viewport").unwrap().frame.height, 80.0);
        assert_eq!(runtime.find("list.content").unwrap().frame.y, -24.0);
        assert!(runtime.find("list.scrollbar").is_some());

        runtime.update_pointer(PointerEvent::at(10.0, 10.0));
        runtime.update_scroll(ScrollEvent { x: 0.0, y: -2.0 });
        assert_eq!(offset.get(), 44.0);
    }

    #[test]
    fn scroll_y_signal_writes_wheel_offset_to_state() {
        let state = State::new(BoundWidgetState {
            offset: 24.0,
            ..BoundWidgetState::default()
        });
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 160.0, 120.0, move |ui, _| {
            let offset = compose_state.signal(
                "test.signal",
                |state| state.offset,
                |state, value| state.offset = value,
            );
            ui.scroll_y("list")
                .size(120.0, 80.0)
                .content_height(200.0)
                .offset_signal(offset)
                .step(10.0)
                .content(|ui| {
                    ui.rect("row.a").size(Size::fill(), 30.0).build();
                    ui.rect("row.b").size(Size::fill(), 30.0).build();
                });
        });

        runtime.update_pointer(PointerEvent::at(10.0, 10.0));
        runtime.update_scroll(ScrollEvent { x: 0.0, y: -2.0 });

        assert_eq!(state.read(|state| state.offset), 44.0);
    }

    #[test]
    fn scroll_y_uses_resolved_fill_viewport_and_auto_content_height() {
        let offset = Rc::new(Cell::new(-1.0));
        let mut runtime = Runtime::new("page");

        for _ in 0..2 {
            let callback_offset = offset.clone();
            compose(&mut runtime, 160.0, 120.0, move |ui, _| {
                ui.stack("root").size(120.0, 80.0).content(|ui| {
                    ui.scroll_y("list")
                        .size(Size::fill(), Size::fill())
                        .offset(24.0)
                        .step(10.0)
                        .on_change(move |next| callback_offset.set(next))
                        .content(|ui| {
                            ui.rect("row.a").size(Size::fill(), 50.0).build();
                            ui.rect("row.b").size(Size::fill(), 50.0).build();
                            ui.rect("row.c").size(Size::fill(), 50.0).build();
                        });
                });
            });
        }

        assert_eq!(runtime.find("list.viewport").unwrap().frame.height, 80.0);
        assert_eq!(runtime.find("list.content").unwrap().frame.height, 150.0);
        assert_eq!(runtime.find("list.content").unwrap().frame.y, -24.0);
        assert!(runtime.find("list.scrollbar").is_some());

        runtime.update_pointer(PointerEvent::at(10.0, 10.0));
        runtime.update_scroll(ScrollEvent { x: 0.0, y: -2.0 });

        assert_eq!(offset.get(), 44.0);
    }

    #[test]
    fn scroll_y_fill_viewport_without_previous_frame_does_not_guess_default_track() {
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 160.0, 120.0, |ui, _| {
            ui.stack("root").size(120.0, 80.0).content(|ui| {
                ui.scroll_y("list")
                    .size(Size::fill(), Size::fill())
                    .content_height(150.0)
                    .content(|ui| {
                        ui.rect("row").size(Size::fill(), 150.0).build();
                    });
            });
        });

        assert_eq!(runtime.find("list.viewport").unwrap().frame.height, 80.0);
        assert!(runtime.find("list.scrollbar").is_none());
    }

    #[test]
    fn scroll_y_inset_keeps_viewport_and_scrollbar_inside_outer_shell() {
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 160.0, 120.0, |ui, _| {
            ui.scroll_y("panel")
                .size(120.0, 80.0)
                .inset(10.0)
                .content_height(200.0)
                .content(|ui| {
                    ui.rect("row").size(Size::fill(), 200.0).build();
                });
        });

        let viewport = runtime.find("panel.viewport").unwrap().frame;
        let scrollbar = runtime.find("panel.scrollbar").unwrap().frame;

        assert_eq!(viewport.x, 10.0);
        assert_eq!(viewport.y, 10.0);
        assert_eq!(viewport.width, 100.0);
        assert_eq!(viewport.height, 60.0);
        assert_eq!(scrollbar.x, 102.0);
        assert_eq!(scrollbar.y, 10.0);
        assert_eq!(scrollbar.height, 60.0);
    }

    #[test]
    fn scroll_x_explicit_content_width_composes_viewport_content_and_scrollbar() {
        let offset = Rc::new(Cell::new(-1.0));
        let callback_offset = offset.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 180.0, 120.0, move |ui, _| {
            let callback_offset = callback_offset.clone();
            ui.scroll_x("strip")
                .size(140.0, 80.0)
                .content_width(260.0)
                .offset(30.0)
                .step(10.0)
                .padding(8.0)
                .gap(6.0)
                .on_change(move |next| callback_offset.set(next))
                .content(|ui| {
                    ui.rect("card.a").size(80.0, Size::fill()).build();
                    ui.rect("card.b").size(80.0, Size::fill()).build();
                });
        });

        assert_eq!(runtime.find("strip").unwrap().frame.width, 140.0);
        assert_eq!(runtime.find("strip.viewport").unwrap().frame.width, 140.0);
        assert_eq!(runtime.find("strip.content").unwrap().frame.x, -30.0);
        assert_eq!(runtime.find("strip.content").unwrap().frame.width, 260.0);
        assert!(runtime.find("strip.scrollbar").is_some());

        runtime.update_pointer(PointerEvent::at(10.0, 70.0));
        runtime.update_scroll(ScrollEvent { x: -2.0, y: 0.0 });

        assert_eq!(offset.get(), 50.0);
    }

    #[test]
    fn scroll_x_inset_keeps_viewport_and_scrollbar_inside_outer_shell() {
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 180.0, 120.0, |ui, _| {
            ui.scroll_x("panel")
                .size(140.0, 80.0)
                .inset(10.0)
                .content_width(260.0)
                .content(|ui| {
                    ui.rect("card").size(260.0, Size::fill()).build();
                });
        });

        let viewport = runtime.find("panel.viewport").unwrap().frame;
        let scrollbar = runtime.find("panel.scrollbar").unwrap().frame;

        assert_eq!(viewport.x, 10.0);
        assert_eq!(viewport.y, 10.0);
        assert_eq!(viewport.width, 120.0);
        assert_eq!(viewport.height, 60.0);
        assert_eq!(scrollbar.x, 10.0);
        assert_eq!(scrollbar.y, 62.0);
        assert_eq!(scrollbar.width, 120.0);
    }

    #[test]
    fn scroll_xy_explicit_content_size_composes_viewport_content_and_scrollbars() {
        let offset = Rc::new(Cell::new((-1.0, -1.0)));
        let callback_offset = offset.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 220.0, 160.0, move |ui, _| {
            let callback_offset = callback_offset.clone();
            ui.scroll_xy("grid")
                .size(160.0, 100.0)
                .content_size(320.0, 260.0)
                .offset(24.0, 40.0)
                .step_xy(10.0, 20.0)
                .padding(6.0)
                .on_change(move |next_x, next_y| callback_offset.set((next_x, next_y)))
                .content(|ui| {
                    ui.rect("cell.a").size(80.0, 60.0).build();
                    ui.rect("cell.b")
                        .position(180.0, 160.0)
                        .size(80.0, 60.0)
                        .build();
                });
        });

        assert_eq!(runtime.find("grid").unwrap().frame.width, 160.0);
        assert_eq!(runtime.find("grid.viewport").unwrap().frame.width, 160.0);
        assert_eq!(runtime.find("grid.viewport").unwrap().frame.height, 100.0);
        assert_eq!(runtime.find("grid.content").unwrap().frame.x, -24.0);
        assert_eq!(runtime.find("grid.content").unwrap().frame.y, -40.0);
        assert_eq!(runtime.find("grid.content").unwrap().frame.width, 320.0);
        assert_eq!(runtime.find("grid.content").unwrap().frame.height, 260.0);
        assert!(runtime.find("grid.scrollbar.x").is_some());
        assert!(runtime.find("grid.scrollbar.y").is_some());
        assert!(runtime.find("grid.scrollbar.corner").is_some());

        runtime.update_pointer(PointerEvent::at(10.0, 10.0));
        runtime.update_scroll(ScrollEvent { x: -2.0, y: -1.0 });

        assert_eq!(offset.get(), (44.0, 60.0));
    }

    #[test]
    fn scroll_xy_inset_keeps_viewport_and_scrollbars_inside_outer_shell() {
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 220.0, 160.0, |ui, _| {
            ui.scroll_xy("panel")
                .size(160.0, 110.0)
                .inset(10.0)
                .content_size(320.0, 260.0)
                .content(|ui| {
                    ui.rect("cell").size(320.0, 260.0).build();
                });
        });

        let viewport = runtime.find("panel.viewport").unwrap().frame;
        let scrollbar_x = runtime.find("panel.scrollbar.x").unwrap().frame;
        let scrollbar_y = runtime.find("panel.scrollbar.y").unwrap().frame;
        let corner = runtime.find("panel.scrollbar.corner").unwrap().frame;

        assert_eq!(viewport.x, 10.0);
        assert_eq!(viewport.y, 10.0);
        assert_eq!(viewport.width, 140.0);
        assert_eq!(viewport.height, 90.0);
        assert_eq!(scrollbar_x.x, 10.0);
        assert_eq!(scrollbar_x.y, 92.0);
        assert_eq!(scrollbar_x.width, 124.0);
        assert_eq!(scrollbar_y.x, 142.0);
        assert_eq!(scrollbar_y.y, 10.0);
        assert_eq!(scrollbar_y.height, 74.0);
        assert_eq!(corner.x, 142.0);
        assert_eq!(corner.y, 92.0);
    }

    #[test]
    fn popover_composes_on_root_layer_from_previous_anchor_frame() {
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 320.0, 180.0, |ui, _| {
            ui.column("panel")
                .position(20.0, 30.0)
                .size(120.0, 80.0)
                .content(|ui| {
                    ui.rect("anchor").size(50.0, 20.0).build();
                    popover(ui, "menu")
                        .anchor("anchor")
                        .placement(PopoverPlacement::BottomStart)
                        .gap(4.0)
                        .size(80.0, 60.0)
                        .content(|ui| {
                            ui.rect("menu.bg").size(Size::fill(), Size::fill()).build();
                        });
                });
        });
        assert_eq!(runtime.debug_snapshot().layers[0].id, "page.menu");
        assert_eq!(
            runtime.debug_snapshot().layers[0].anchor_source,
            LayerAnchorSource::Missing
        );
        assert_eq!(
            runtime.debug_snapshot().layers[0].action,
            LayerLifecycleAction::Created
        );

        compose(&mut runtime, 320.0, 180.0, |ui, _| {
            ui.column("panel")
                .position(20.0, 30.0)
                .size(120.0, 80.0)
                .content(|ui| {
                    ui.rect("anchor").size(50.0, 20.0).build();
                    popover(ui, "menu")
                        .anchor("anchor")
                        .placement(PopoverPlacement::BottomStart)
                        .gap(4.0)
                        .size(80.0, 60.0)
                        .content(|ui| {
                            ui.rect("menu.bg").size(Size::fill(), Size::fill()).build();
                        });
                });
        });

        assert_eq!(runtime.roots().len(), 2);
        assert_eq!(runtime.roots()[0].id, "page.panel");
        assert_eq!(runtime.roots()[1].id, "page.menu");
        assert_eq!(runtime.find("menu").unwrap().frame.x, 20.0);
        assert_eq!(runtime.find("menu").unwrap().frame.y, 54.0);
        assert_eq!(runtime.debug_snapshot().layers.len(), 1);
        let layer = &runtime.debug_snapshot().layers[0];
        assert_eq!(layer.id, "page.menu");
        assert_eq!(layer.owner, "page.menu");
        assert_eq!(layer.anchor.as_deref(), Some("page.anchor"));
        assert_eq!(layer.anchor_source, LayerAnchorSource::PreviousFrame);
        assert_eq!(layer.kind, LayerKind::Popover);
        assert_eq!(layer.placement, LayerPlacement::BottomStart);
        assert_eq!(layer.action, LayerLifecycleAction::Reused);

        compose(&mut runtime, 320.0, 180.0, |ui, _| {
            ui.column("panel")
                .position(20.0, 30.0)
                .size(120.0, 80.0)
                .content(|ui| {
                    ui.rect("anchor").size(50.0, 20.0).build();
                });
        });
        assert!(runtime.debug_snapshot().layers.iter().any(|layer| {
            layer.id == "page.menu" && layer.action == LayerLifecycleAction::Removed
        }));
    }

    #[test]
    fn closed_popover_records_closed_layer_intent_without_root_content() {
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 320.0, 180.0, |ui, _| {
            ui.column("panel")
                .position(20.0, 30.0)
                .size(120.0, 80.0)
                .content(|ui| {
                    ui.rect("anchor").size(50.0, 20.0).build();
                    popover(ui, "menu")
                        .anchor("anchor")
                        .placement(PopoverPlacement::BottomStart)
                        .gap(4.0)
                        .size(80.0, 60.0)
                        .open(false)
                        .content(|ui| {
                            ui.rect("menu.bg").size(Size::fill(), Size::fill()).build();
                        });
                });
        });

        assert!(runtime.find("menu").is_none());
        assert_eq!(runtime.debug_snapshot().layers.len(), 1);
        let layer = &runtime.debug_snapshot().layers[0];
        assert_eq!(layer.id, "page.menu");
        assert!(!layer.open);
        assert_eq!(layer.anchor_source, LayerAnchorSource::Closed);
        assert_eq!(layer.action, LayerLifecycleAction::Closed);
    }

    #[test]
    fn blocking_popover_blocks_outside_pointer_without_blocking_inside_pointer() {
        let mut runtime = Runtime::new("page");
        let under_clicks = Rc::new(Cell::new(0));
        let menu_clicks = Rc::new(Cell::new(0));

        compose(&mut runtime, 320.0, 180.0, |ui, _| {
            ui.rect("under")
                .position(0.0, 0.0)
                .size(320.0, 180.0)
                .on_click({
                    let under_clicks = under_clicks.clone();
                    move || under_clicks.set(under_clicks.get() + 1)
                })
                .build();
            ui.rect("anchor")
                .position(20.0, 20.0)
                .size(40.0, 20.0)
                .build();
            popover(ui, "menu")
                .anchor("anchor")
                .fallback_anchor(LayoutRect::new(20.0, 20.0, 40.0, 20.0))
                .placement(PopoverPlacement::BottomStart)
                .gap(0.0)
                .size(80.0, 60.0)
                .outside_click(OutsideClickPolicy::Block)
                .content(|ui| {
                    ui.rect("menu.hit")
                        .size(Size::fill(), Size::fill())
                        .on_click({
                            let menu_clicks = menu_clicks.clone();
                            move || menu_clicks.set(menu_clicks.get() + 1)
                        })
                        .build();
                });
        });

        runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));
        assert_eq!(under_clicks.get(), 0);
        assert_eq!(menu_clicks.get(), 0);
        assert_eq!(
            runtime.debug_snapshot().layers[0].outside_click,
            OutsideClickPolicy::Block
        );
        let layer_pointer = &runtime.debug_snapshot_current().layer_pointer[0];
        assert_eq!(layer_pointer.action, LayerPointerAction::Blocked);
        assert_eq!(layer_pointer.layer.as_deref(), Some("page.menu"));
        assert_eq!(layer_pointer.policy, OutsideClickPolicy::Block);

        runtime.update_pointer(PointerEvent::pressed_at(30.0, 45.0));
        runtime.update_pointer(PointerEvent::released_at(30.0, 45.0));
        assert_eq!(under_clicks.get(), 0);
        assert_eq!(menu_clicks.get(), 1);
        let layer_pointer = &runtime.debug_snapshot_current().layer_pointer[0];
        assert_eq!(layer_pointer.action, LayerPointerAction::HitLayer);
        assert_eq!(layer_pointer.hit_layer.as_deref(), Some("page.menu"));
    }

    #[test]
    fn blocking_popover_blocks_outside_scroll_without_blocking_inside_scroll() {
        let mut runtime = Runtime::new("page");
        let under_scroll = Rc::new(Cell::new(0.0));
        let menu_scroll = Rc::new(Cell::new(0.0));

        compose(&mut runtime, 320.0, 180.0, |ui, _| {
            ui.rect("under")
                .position(0.0, 0.0)
                .size(320.0, 180.0)
                .on_scroll({
                    let under_scroll = under_scroll.clone();
                    move |event| under_scroll.set(under_scroll.get() + event.y)
                })
                .build();
            ui.rect("anchor")
                .position(20.0, 20.0)
                .size(40.0, 20.0)
                .build();
            popover(ui, "menu")
                .anchor("anchor")
                .fallback_anchor(LayoutRect::new(20.0, 20.0, 40.0, 20.0))
                .placement(PopoverPlacement::BottomStart)
                .gap(0.0)
                .size(80.0, 60.0)
                .outside_click(OutsideClickPolicy::Block)
                .content(|ui| {
                    ui.rect("menu.scroll")
                        .size(Size::fill(), Size::fill())
                        .on_scroll({
                            let menu_scroll = menu_scroll.clone();
                            move |event| menu_scroll.set(menu_scroll.get() + event.y)
                        })
                        .build();
                });
        });

        runtime.update_pointer(PointerEvent::at(10.0, 10.0));
        runtime.update_scroll(ScrollEvent { x: 0.0, y: 4.0 });
        assert_eq!(under_scroll.get(), 0.0);
        assert_eq!(menu_scroll.get(), 0.0);
        let layer_pointer = &runtime.debug_snapshot_current().layer_pointer[0];
        assert_eq!(layer_pointer.action, LayerPointerAction::Blocked);
        assert_eq!(layer_pointer.layer.as_deref(), Some("page.menu"));

        runtime.update_pointer(PointerEvent::at(30.0, 45.0));
        runtime.update_scroll(ScrollEvent { x: 0.0, y: 5.0 });
        assert_eq!(under_scroll.get(), 0.0);
        assert_eq!(menu_scroll.get(), 5.0);
        let layer_pointer = &runtime.debug_snapshot_current().layer_pointer[0];
        assert_eq!(layer_pointer.action, LayerPointerAction::HitLayer);
        assert_eq!(layer_pointer.hit_layer.as_deref(), Some("page.menu"));
        assert!(runtime.debug_snapshot_current().events.iter().any(|event| {
            event.raw_event == "scroll"
                && event.target.role() == "scroll"
                && event.target.id() == "page.menu.scroll"
                && event.command == "scroll"
                && event.callback
        }));
    }

    #[test]
    fn blocking_popover_clears_underlying_focus_allows_layer_focus_and_restores_on_close() {
        let mut runtime = Runtime::new("page");
        let under_text = Rc::new(std::cell::RefCell::new(String::new()));
        let menu_text = Rc::new(std::cell::RefCell::new(String::new()));

        compose(&mut runtime, 320.0, 180.0, |ui, _| {
            ui.rect("under.input")
                .position(0.0, 0.0)
                .size(80.0, 24.0)
                .on_text_input({
                    let under_text = under_text.clone();
                    move |event| under_text.borrow_mut().push_str(&event.text)
                })
                .build();
        });
        runtime.update_pointer(PointerEvent::pressed_at(5.0, 5.0));
        runtime.update_keyboard(KeyboardEvent {
            text: "A".to_string(),
            ..KeyboardEvent::default()
        });
        assert_eq!(under_text.borrow().as_str(), "A");
        assert_eq!(runtime.text_focused_id(), Some("page.under.input"));

        compose(&mut runtime, 320.0, 180.0, |ui, _| {
            ui.rect("under.input")
                .position(0.0, 0.0)
                .size(80.0, 24.0)
                .on_text_input({
                    let under_text = under_text.clone();
                    move |event| under_text.borrow_mut().push_str(&event.text)
                })
                .build();
            ui.rect("anchor")
                .position(100.0, 20.0)
                .size(40.0, 20.0)
                .build();
            popover(ui, "menu")
                .anchor("anchor")
                .fallback_anchor(LayoutRect::new(100.0, 20.0, 40.0, 20.0))
                .placement(PopoverPlacement::BottomStart)
                .gap(0.0)
                .size(90.0, 44.0)
                .outside_click(OutsideClickPolicy::Block)
                .content(|ui| {
                    ui.rect("menu.input")
                        .size(Size::fill(), Size::fill())
                        .on_text_input({
                            let menu_text = menu_text.clone();
                            move |event| menu_text.borrow_mut().push_str(&event.text)
                        })
                        .build();
                });
        });

        assert!(runtime.has_keyboard_capture());
        assert_eq!(runtime.focused_id(), None);
        assert_eq!(runtime.text_focused_id(), None);
        assert!(!runtime.update_keyboard(KeyboardEvent {
            text: "B".to_string(),
            ..KeyboardEvent::default()
        }));
        assert_eq!(under_text.borrow().as_str(), "A");
        assert_eq!(menu_text.borrow().as_str(), "");

        runtime.update_pointer(PointerEvent::pressed_at(110.0, 50.0));
        assert_eq!(runtime.text_focused_id(), Some("page.menu.input"));
        assert!(runtime.update_keyboard(KeyboardEvent {
            text: "C".to_string(),
            ..KeyboardEvent::default()
        }));
        assert_eq!(under_text.borrow().as_str(), "A");
        assert_eq!(menu_text.borrow().as_str(), "C");

        compose(&mut runtime, 320.0, 180.0, |ui, _| {
            ui.rect("under.input")
                .position(0.0, 0.0)
                .size(80.0, 24.0)
                .on_text_input({
                    let under_text = under_text.clone();
                    move |event| under_text.borrow_mut().push_str(&event.text)
                })
                .build();
        });

        assert_eq!(runtime.text_focused_id(), Some("page.under.input"));
        assert!(runtime.update_keyboard(KeyboardEvent {
            text: "D".to_string(),
            ..KeyboardEvent::default()
        }));
        assert_eq!(under_text.borrow().as_str(), "AD");
        assert_eq!(menu_text.borrow().as_str(), "C");
    }

    #[test]
    fn close_popover_records_dismissal_and_blocks_outside_pointer() {
        let mut runtime = Runtime::new("page");
        let under_clicks = Rc::new(Cell::new(0));

        compose(&mut runtime, 320.0, 180.0, |ui, _| {
            ui.rect("under")
                .position(0.0, 0.0)
                .size(320.0, 180.0)
                .on_click({
                    let under_clicks = under_clicks.clone();
                    move || under_clicks.set(under_clicks.get() + 1)
                })
                .build();
            ui.rect("anchor")
                .position(20.0, 20.0)
                .size(40.0, 20.0)
                .build();
            popover(ui, "menu")
                .anchor("anchor")
                .fallback_anchor(LayoutRect::new(20.0, 20.0, 40.0, 20.0))
                .placement(PopoverPlacement::BottomStart)
                .gap(0.0)
                .size(80.0, 60.0)
                .outside_click(OutsideClickPolicy::Close)
                .content(|ui| {
                    ui.rect("menu.bg").size(Size::fill(), Size::fill()).build();
                });
        });

        runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));

        assert_eq!(under_clicks.get(), 0);
        assert_eq!(runtime.debug_snapshot_current().layer_dismissals.len(), 1);
        let dismissal = &runtime.debug_snapshot_current().layer_dismissals[0];
        assert_eq!(dismissal.id, "page.menu");
        assert_eq!(dismissal.owner, "page.menu");
        assert_eq!(dismissal.policy, OutsideClickPolicy::Close);
        let layer_pointer = &runtime.debug_snapshot_current().layer_pointer[0];
        assert_eq!(layer_pointer.action, LayerPointerAction::Dismissed);
        assert_eq!(layer_pointer.layer.as_deref(), Some("page.menu"));
    }

    #[test]
    fn upper_layer_hit_does_not_dismiss_lower_close_layer() {
        let mut runtime = Runtime::new("page");
        let lower_dismissed = Rc::new(Cell::new(false));
        let upper_clicks = Rc::new(Cell::new(0));

        compose(&mut runtime, 240.0, 120.0, |ui, _| {
            popover(ui, "lower")
                .fallback_anchor(LayoutRect::new(20.0, 20.0, 0.0, 0.0))
                .placement(PopoverPlacement::BottomStart)
                .gap(0.0)
                .size(50.0, 50.0)
                .z_index(10)
                .outside_click(OutsideClickPolicy::Close)
                .on_dismiss({
                    let lower_dismissed = lower_dismissed.clone();
                    move || lower_dismissed.set(true)
                })
                .content(|ui| {
                    ui.rect("lower.bg").size(Size::fill(), Size::fill()).build();
                });
            popover(ui, "upper")
                .fallback_anchor(LayoutRect::new(100.0, 20.0, 0.0, 0.0))
                .placement(PopoverPlacement::BottomStart)
                .gap(0.0)
                .size(50.0, 50.0)
                .z_index(20)
                .outside_click(OutsideClickPolicy::Ignore)
                .content(|ui| {
                    ui.rect("upper.hit")
                        .size(Size::fill(), Size::fill())
                        .on_click({
                            let upper_clicks = upper_clicks.clone();
                            move || upper_clicks.set(upper_clicks.get() + 1)
                        })
                        .build();
                });
        });

        runtime.update_pointer(PointerEvent::pressed_at(110.0, 30.0));
        runtime.update_pointer(PointerEvent::released_at(110.0, 30.0));

        assert!(!lower_dismissed.get());
        assert_eq!(upper_clicks.get(), 1);
        assert!(runtime.debug_snapshot_current().layer_dismissals.is_empty());
        let layer_pointer = &runtime.debug_snapshot_current().layer_pointer[0];
        assert_eq!(layer_pointer.action, LayerPointerAction::HitLayer);
        assert_eq!(layer_pointer.layer.as_deref(), Some("page.upper"));
        assert_eq!(layer_pointer.hit_layer.as_deref(), Some("page.upper"));
        assert_eq!(layer_pointer.policy, OutsideClickPolicy::Ignore);
        assert!(runtime.debug_snapshot_current().events.iter().any(|event| {
            event.target.role() == "node"
                && event.target.id() == "page.upper.hit"
                && event.command == "click"
                && event.callback
        }));
    }

    #[test]
    fn popover_without_anchor_frame_does_not_guess_zero_anchor() {
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 320.0, 180.0, |ui, _| {
            ui.column("panel")
                .position(20.0, 30.0)
                .size(120.0, 80.0)
                .content(|ui| {
                    ui.rect("anchor").size(50.0, 20.0).build();
                    popover(ui, "menu")
                        .anchor("anchor")
                        .placement(PopoverPlacement::BottomStart)
                        .gap(4.0)
                        .size(80.0, 60.0)
                        .content(|ui| {
                            ui.rect("menu.bg").size(Size::fill(), Size::fill()).build();
                        });
                });
        });

        assert!(runtime.find("anchor").is_some());
        assert!(runtime.find("menu").is_none());
    }

    #[test]
    fn input_text_event_reports_changed_text() {
        let value = Rc::new(std::cell::RefCell::new(String::new()));
        let callback_value = value.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 320.0, 120.0, move |ui, _| {
            let callback_value = callback_value.clone();
            input(ui, "name")
                .text("")
                .on_change(move |next| {
                    *callback_value.borrow_mut() = next.to_string();
                })
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(4.0, 4.0));
        runtime.update_keyboard(KeyboardEvent {
            text: "Sky".to_string(),
            ..KeyboardEvent::default()
        });

        assert_eq!(value.borrow().as_str(), "Sky");
    }

    #[test]
    fn segmented_click_reports_index_change() {
        let selected = Rc::new(Cell::new(-1));
        let callback_selected = selected.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 320.0, 80.0, move |ui, _| {
            let callback_selected = callback_selected.clone();
            segmented(ui, "mode")
                .size(180.0, 30.0)
                .items(["A", "B", "C"])
                .selected(0)
                .on_change(move |index| callback_selected.set(index))
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(70.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(70.0, 10.0));

        assert_eq!(selected.get(), 1);
    }

    #[test]
    fn tabs_click_reports_index_change() {
        let selected = Rc::new(Cell::new(-1));
        let callback_selected = selected.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 360.0, 80.0, move |ui, _| {
            let callback_selected = callback_selected.clone();
            tabs(ui, "tabs")
                .size(240.0, 40.0)
                .items(["Home", "Logs", "About"])
                .selected(0)
                .on_change(move |index| callback_selected.set(index))
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(170.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(170.0, 10.0));

        assert_eq!(selected.get(), 2);
    }

    #[test]
    fn segmented_signal_writes_selected_index_to_state() {
        let state = State::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 320.0, 80.0, move |ui, _| {
            let selected = compose_state.signal(
                "test.signal",
                |state| state.selected,
                |state, value| state.selected = value,
            );
            segmented(ui, "mode")
                .size(180.0, 30.0)
                .items(["A", "B", "C"])
                .signal(selected)
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(70.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(70.0, 10.0));

        assert_eq!(state.read(|state| state.selected), 1);
    }

    #[test]
    fn tabs_signal_writes_selected_index_to_state() {
        let state = State::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 360.0, 80.0, move |ui, _| {
            let selected = compose_state.signal(
                "test.signal",
                |state| state.selected,
                |state, value| state.selected = value,
            );
            tabs(ui, "tabs")
                .size(240.0, 40.0)
                .items(["Home", "Logs", "About"])
                .signal(selected)
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(170.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(170.0, 10.0));

        assert_eq!(state.read(|state| state.selected), 2);
    }

    #[test]
    fn nav_group_signal_writes_selected_value_to_state() {
        let state = State::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 260.0, 220.0, move |ui, _| {
            let selected = compose_state.signal(
                "test.nav",
                |state| state.selected,
                |state, value| state.selected = value,
            );
            nav_group(ui, "nav")
                .size(180.0, 150.0)
                .signal(selected)
                .item(0, "Overview")
                .item(1, "Tasks")
                .item(2, "Settings")
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(8.0, 74.0));
        runtime.update_pointer(PointerEvent::released_at(8.0, 74.0));

        assert_eq!(state.read(|state| state.selected), 1);
        assert_eq!(state.dirty()[0].id, "page.nav");
    }

    #[test]
    fn radio_signal_writes_selected_value_to_state() {
        let state = State::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 240.0, 80.0, move |ui, _| {
            let selected = compose_state.signal(
                "test.signal",
                |state| state.selected == 2,
                |state, value| {
                    if value {
                        state.selected = 2;
                    }
                },
            );
            radio(ui, "choice.c")
                .size(120.0, 28.0)
                .signal(selected)
                .text("Choice C")
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(2.0, 2.0));
        runtime.update_pointer(PointerEvent::released_at(2.0, 2.0));

        assert_eq!(state.read(|state| state.selected), 2);
    }

    #[test]
    fn dropdown_signals_write_open_and_selection_to_state() {
        let state = State::new(BoundWidgetState {
            open: true,
            selected: 0,
            ..BoundWidgetState::default()
        });
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        for _ in 0..2 {
            let compose_state = compose_state.clone();
            compose(&mut runtime, 320.0, 220.0, move |ui, _| {
                let selected = compose_state.signal(
                    "test.signal",
                    |state| state.selected,
                    |state, value| state.selected = value,
                );
                let open = compose_state.signal(
                    "test.signal",
                    |state| state.open,
                    |state, value| state.open = value,
                );
                dropdown(ui, "quality")
                    .items(["Low", "Medium", "High"])
                    .value_signal(selected)
                    .open_signal(open)
                    .build();
            });
        }

        runtime.update_pointer(PointerEvent::pressed_at(16.0, 100.0));
        runtime.update_pointer(PointerEvent::released_at(16.0, 100.0));

        assert_eq!(state.read(|state| state.selected), 1);
        assert!(!state.read(|state| state.open));
    }

    #[test]
    fn dropdown_frame_order_opens_and_selects_through_dirty_recompose() {
        let state = State::new(BoundWidgetState::default());
        let mut runtime = Runtime::new("page");

        let frame = |runtime: &mut Runtime, dirty: Vec<DirtyInput>| {
            let dirty_state = state.clone();
            let compose_state = state.clone();
            runtime.frame_incremental(
                FrameInput::new(Screen::new(320.0, 220.0), 0.0).dirty(dirty),
                move || dirty_state.take_dirty(),
                move |ui, _| {
                    let selected = compose_state.signal(
                        "selected",
                        |state| state.selected,
                        |state, value| state.selected = value,
                    );
                    let open = compose_state.signal(
                        "open",
                        |state| state.open,
                        |state, value| state.open = value,
                    );
                    dropdown(ui, "quality")
                        .items(["Low", "Medium", "High"])
                        .value_signal(selected)
                        .open_signal(open)
                        .build();
                },
            );
        };

        frame(&mut runtime, Vec::new());
        runtime.frame_incremental(
            FrameInput::new(Screen::new(320.0, 220.0), 0.0).pointer_events([
                PointerEvent::pressed_at(8.0, 8.0),
                PointerEvent::released_at(8.0, 8.0),
            ]),
            || state.take_dirty(),
            {
                let state = state.clone();
                move |ui, _| {
                    let selected = state.signal(
                        "selected",
                        |state| state.selected,
                        |state, value| state.selected = value,
                    );
                    let open = state.signal(
                        "open",
                        |state| state.open,
                        |state, value| state.open = value,
                    );
                    dropdown(ui, "quality")
                        .items(["Low", "Medium", "High"])
                        .value_signal(selected)
                        .open_signal(open)
                        .build();
                }
            },
        );

        assert!(state.read(|state| state.open));
        assert!(runtime.find("quality.popup.surface").is_some());
        assert_eq!(
            runtime.debug_snapshot().dirty_ids,
            vec!["page.quality", "page.quality.popup"]
        );

        runtime.frame_incremental(
            FrameInput::new(Screen::new(320.0, 220.0), 0.0).pointer_events([
                PointerEvent::pressed_at(16.0, 100.0),
                PointerEvent::released_at(16.0, 100.0),
            ]),
            || state.take_dirty(),
            {
                let state = state.clone();
                move |ui, _| {
                    let selected = state.signal(
                        "selected",
                        |state| state.selected,
                        |state, value| state.selected = value,
                    );
                    let open = state.signal(
                        "open",
                        |state| state.open,
                        |state, value| state.open = value,
                    );
                    dropdown(ui, "quality")
                        .items(["Low", "Medium", "High"])
                        .value_signal(selected)
                        .open_signal(open)
                        .build();
                }
            },
        );

        assert_eq!(state.read(|state| state.selected), 1);
        assert!(!state.read(|state| state.open));
        assert!(runtime.find("quality.popup.surface").is_none());
        assert_eq!(
            runtime.debug_snapshot().dirty_ids,
            vec!["page.quality", "page.quality.popup"]
        );
    }

    #[test]
    fn dropdown_outside_click_dismisses_open_signal_through_layer_policy() {
        let state = State::new(BoundWidgetState {
            open: true,
            ..BoundWidgetState::default()
        });
        let mut runtime = Runtime::new("page");

        let frame = |runtime: &mut Runtime, pointer_events: Vec<PointerEvent>| {
            let dirty_state = state.clone();
            let compose_state = state.clone();
            runtime.frame_incremental(
                FrameInput::new(Screen::new(320.0, 220.0), 0.0).pointer_events(pointer_events),
                move || dirty_state.take_dirty(),
                move |ui, _| {
                    let selected = compose_state.signal(
                        "selected",
                        |state| state.selected,
                        |state, value| state.selected = value,
                    );
                    let open = compose_state.signal(
                        "open",
                        |state| state.open,
                        |state, value| state.open = value,
                    );
                    dropdown(ui, "quality")
                        .items(["Low", "Medium", "High"])
                        .value_signal(selected)
                        .open_signal(open)
                        .build();
                },
            );
        };

        frame(&mut runtime, Vec::new());
        frame(&mut runtime, Vec::new());
        assert!(runtime.find("quality.popup.surface").is_some());

        frame(
            &mut runtime,
            vec![
                PointerEvent::pressed_at(300.0, 200.0),
                PointerEvent::released_at(300.0, 200.0),
            ],
        );

        assert!(!state.read(|state| state.open));
        assert!(runtime.find("quality.popup.surface").is_none());
        assert_eq!(runtime.debug_snapshot_current().layer_dismissals.len(), 1);
        assert_eq!(
            runtime.debug_snapshot_current().layer_dismissals[0].id,
            "page.quality.popup"
        );
        assert!(runtime.debug_snapshot().events.iter().any(|event| {
            event.target.role() == "layer"
                && event.target.id() == "page.quality.popup"
                && matches!(
                    &event.target,
                    crate::runtime::EventTargetId::Layer(layer)
                        if layer.as_str() == "page.quality.popup"
                )
                && event.command == "dismiss"
                && event.callback
                && event.invalidation.as_ref().is_some_and(|invalidation| {
                    invalidation.target.kind() == "layer"
                        && invalidation.target.id() == "page.quality.popup"
                        && invalidation.source.kind() == "event"
                        && invalidation.source.label() == "dismiss"
                })
        }));
        assert!(runtime
            .debug_snapshot()
            .invalidations
            .iter()
            .any(|invalidation| {
                matches!(
                    &invalidation.target,
                    InvalidationTarget::Layer(layer) if layer.as_str() == "page.quality.popup"
                ) && invalidation.target.kind() == "layer"
                    && invalidation.source.kind() == "event"
                    && invalidation.source.label() == "dismiss"
            }));
    }

    #[test]
    fn dialog_backdrop_dismisses_through_layer_policy() {
        let closed = Rc::new(Cell::new(false));
        let under_clicks = Rc::new(Cell::new(0));
        let callback_closed = closed.clone();
        let callback_under = under_clicks.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 400.0, 300.0, move |ui, _| {
            let callback_closed = callback_closed.clone();
            let callback_under = callback_under.clone();
            ui.rect("under")
                .position(0.0, 0.0)
                .size(400.0, 300.0)
                .on_click(move || callback_under.set(callback_under.get() + 1))
                .build();
            dialog(ui, "confirm")
                .open(true)
                .screen(400.0, 300.0)
                .on_close(move || callback_closed.set(true))
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(8.0, 8.0));
        runtime.update_pointer(PointerEvent::released_at(8.0, 8.0));

        assert!(closed.get());
        assert_eq!(under_clicks.get(), 0);
        assert_eq!(runtime.debug_snapshot_current().layer_dismissals.len(), 1);
        let dismissal = &runtime.debug_snapshot_current().layer_dismissals[0];
        assert_eq!(dismissal.id, "page.confirm.panel");
        assert_eq!(dismissal.owner, "page.confirm");
        assert_eq!(dismissal.policy, OutsideClickPolicy::Close);
        assert!(runtime.debug_snapshot().layers.iter().any(|layer| {
            layer.id == "page.confirm.panel"
                && layer.owner == "page.confirm"
                && layer.kind == LayerKind::Modal
                && layer.outside_click == OutsideClickPolicy::Close
        }));
        assert!(runtime.debug_snapshot_current().events.iter().any(|event| {
            event.target.role() == "layer"
                && event.target.id() == "page.confirm.panel"
                && event.command == "dismiss"
                && event.callback
        }));
    }

    #[test]
    fn dropdown_field_toggles_open_state_callback() {
        let opened = Rc::new(Cell::new(false));
        let callback_opened = opened.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 320.0, 160.0, move |ui, _| {
            let callback_opened = callback_opened.clone();
            dropdown(ui, "quality")
                .items(["Low", "High"])
                .open(false)
                .on_open_change(move |next| callback_opened.set(next))
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(8.0, 8.0));
        runtime.update_pointer(PointerEvent::released_at(8.0, 8.0));

        assert!(opened.get());
    }

    #[test]
    fn dropdown_item_reports_selection_and_close() {
        let selected = Rc::new(Cell::new(-1));
        let opened = Rc::new(Cell::new(true));
        let callback_selected = selected.clone();
        let callback_opened = opened.clone();
        let mut runtime = Runtime::new("page");
        for _ in 0..2 {
            let callback_selected = callback_selected.clone();
            let callback_opened = callback_opened.clone();
            compose(&mut runtime, 320.0, 220.0, move |ui, _| {
                let callback_selected = callback_selected.clone();
                let callback_opened = callback_opened.clone();
                dropdown(ui, "quality")
                    .items(["Low", "Medium", "High"])
                    .open(true)
                    .on_change(move |index| callback_selected.set(index))
                    .on_open_change(move |next| callback_opened.set(next))
                    .build();
            });
        }

        runtime.update_pointer(PointerEvent::pressed_at(16.0, 100.0));
        runtime.update_pointer(PointerEvent::released_at(16.0, 100.0));

        assert_eq!(selected.get(), 1);
        assert!(!opened.get());
    }

    #[test]
    fn dropdown_open_popup_draws_selected_option_background() {
        let mut runtime = Runtime::new("page");
        for _ in 0..2 {
            compose(&mut runtime, 320.0, 220.0, move |ui, _| {
                dropdown(ui, "quality")
                    .items(["Drift", "Burst", "Quiet"])
                    .selected(0)
                    .open(true)
                    .build();
            });
        }

        let selected = runtime
            .find("quality.item.selected.0")
            .expect("selected option background should exist");
        let item = runtime
            .find("quality.item.0")
            .expect("selected option hit rect should exist");

        assert_eq!(selected.frame, item.frame);
        assert!(runtime.find("quality.item.selected.1").is_none());
    }

    #[test]
    fn context_menu_dismiss_and_select_callbacks_run() {
        let dismissed = Rc::new(Cell::new(false));
        let selected = Rc::new(Cell::new(-1));
        let callback_dismissed = dismissed.clone();
        let callback_selected = selected.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 320.0, 220.0, move |ui, _| {
            let callback_dismissed = callback_dismissed.clone();
            let callback_selected = callback_selected.clone();
            context_menu(ui, "menu")
                .open(true)
                .screen(320.0, 220.0)
                .position(40.0, 40.0)
                .items(["Copy", "Delete"])
                .on_dismiss(move || callback_dismissed.set(true))
                .on_select(move |index| callback_selected.set(index))
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(60.0, 54.0));
        runtime.update_pointer(PointerEvent::released_at(60.0, 54.0));
        assert_eq!(selected.get(), 0);

        runtime.update_pointer(PointerEvent::pressed_at(4.0, 4.0));
        runtime.update_pointer(PointerEvent::released_at(4.0, 4.0));
        assert!(dismissed.get());
    }

    #[test]
    fn context_menu_outside_click_dismisses_through_layer_policy() {
        let dismissed = Rc::new(Cell::new(false));
        let under_clicks = Rc::new(Cell::new(0));
        let callback_dismissed = dismissed.clone();
        let callback_under = under_clicks.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 320.0, 220.0, move |ui, _| {
            let callback_dismissed = callback_dismissed.clone();
            let callback_under = callback_under.clone();
            ui.rect("under")
                .position(0.0, 0.0)
                .size(320.0, 220.0)
                .on_click(move || callback_under.set(callback_under.get() + 1))
                .build();
            context_menu(ui, "menu")
                .open(true)
                .screen(320.0, 220.0)
                .position(40.0, 40.0)
                .items(["Copy", "Delete"])
                .on_dismiss(move || callback_dismissed.set(true))
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(4.0, 4.0));
        runtime.update_pointer(PointerEvent::released_at(4.0, 4.0));

        assert!(dismissed.get());
        assert_eq!(under_clicks.get(), 0);
        assert_eq!(runtime.debug_snapshot_current().layer_dismissals.len(), 1);
        let dismissal = &runtime.debug_snapshot_current().layer_dismissals[0];
        assert_eq!(dismissal.id, "page.menu");
        assert_eq!(dismissal.owner, "page.menu");
        assert_eq!(dismissal.policy, OutsideClickPolicy::Close);
        assert!(runtime.debug_snapshot_current().events.iter().any(|event| {
            event.target.role() == "layer"
                && event.target.id() == "page.menu"
                && event.command == "dismiss"
                && event.callback
        }));
    }

    #[test]
    fn date_picker_signal_writes_done_value_to_state() {
        let state = State::new(BoundWidgetState {
            open: true,
            date: [2026, 4, 28],
            ..BoundWidgetState::default()
        });
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 480.0, 360.0, move |ui, _| {
            let open = compose_state.signal(
                "test.signal",
                |state| state.open,
                |state, value| state.open = value,
            );
            let date = compose_state.signal(
                "test.signal",
                |state| state.date,
                |state, value| state.date = value,
            );
            date_picker(ui, "date")
                .open_signal(open)
                .value_signal(date)
                .screen(480.0, 360.0)
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(80.0, 239.0));
        runtime.update_pointer(PointerEvent::released_at(80.0, 239.0));
        runtime.update_pointer(PointerEvent::pressed_at(380.0, 70.0));
        runtime.update_pointer(PointerEvent::released_at(380.0, 70.0));

        assert_eq!(state.read(|state| state.date), [2026, 5, 28]);
        assert!(!state.read(|state| state.open));
    }

    #[test]
    fn date_picker_outside_click_dismisses_open_signal_through_layer_policy() {
        let state = State::new(BoundWidgetState {
            open: true,
            date: [2026, 4, 28],
            ..BoundWidgetState::default()
        });
        let under_clicks = Rc::new(Cell::new(0));
        let mut runtime = Runtime::new("page");

        let frame = |runtime: &mut Runtime, pointer_events: Vec<PointerEvent>| {
            let dirty_state = state.clone();
            let compose_state = state.clone();
            let callback_under = under_clicks.clone();
            runtime.frame_incremental(
                FrameInput::new(Screen::new(480.0, 360.0), 0.0).pointer_events(pointer_events),
                move || dirty_state.take_dirty(),
                move |ui, _| {
                    let callback_under = callback_under.clone();
                    ui.rect("under")
                        .position(0.0, 0.0)
                        .size(480.0, 360.0)
                        .on_click(move || callback_under.set(callback_under.get() + 1))
                        .build();
                    let open = compose_state.signal(
                        "test.open",
                        |state| state.open,
                        |state, value| state.open = value,
                    );
                    let date = compose_state.signal(
                        "test.date",
                        |state| state.date,
                        |state, value| state.date = value,
                    );
                    date_picker(ui, "date")
                        .open_signal(open)
                        .value_signal(date)
                        .screen(480.0, 360.0)
                        .build();
                },
            );
        };

        frame(&mut runtime, Vec::new());
        assert!(runtime.find("date.panel").is_some());

        frame(
            &mut runtime,
            vec![
                PointerEvent::pressed_at(8.0, 8.0),
                PointerEvent::released_at(8.0, 8.0),
            ],
        );

        assert!(!state.read(|state| state.open));
        assert_eq!(under_clicks.get(), 0);
        assert_eq!(runtime.debug_snapshot_current().layer_dismissals.len(), 1);
        let dismissal = &runtime.debug_snapshot_current().layer_dismissals[0];
        assert_eq!(dismissal.id, "page.date");
        assert_eq!(dismissal.owner, "page.date");
        assert_eq!(dismissal.policy, OutsideClickPolicy::Close);
        assert!(runtime.debug_snapshot().events.iter().any(|event| {
            event.target.role() == "layer"
                && event.target.id() == "page.date"
                && event.command == "dismiss"
                && event.callback
        }));

        frame(&mut runtime, Vec::new());
        assert!(runtime.find("date.panel").is_none());
    }

    #[test]
    fn time_picker_signal_writes_done_value_to_state() {
        let state = State::new(BoundWidgetState {
            open: true,
            time: [9, 30],
            ..BoundWidgetState::default()
        });
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 420.0, 340.0, move |ui, _| {
            let open = compose_state.signal(
                "test.signal",
                |state| state.open,
                |state, value| state.open = value,
            );
            let time = compose_state.signal(
                "test.signal",
                |state| state.time,
                |state, value| state.time = value,
            );
            time_picker(ui, "time")
                .open_signal(open)
                .value_signal(time)
                .screen(420.0, 340.0)
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(90.0, 229.0));
        runtime.update_pointer(PointerEvent::released_at(90.0, 229.0));
        runtime.update_pointer(PointerEvent::pressed_at(300.0, 60.0));
        runtime.update_pointer(PointerEvent::released_at(300.0, 60.0));

        assert_eq!(state.read(|state| state.time), [10, 30]);
        assert!(!state.read(|state| state.open));
    }

    #[test]
    fn time_picker_outside_click_dismisses_open_signal_through_layer_policy() {
        let state = State::new(BoundWidgetState {
            open: true,
            time: [9, 30],
            ..BoundWidgetState::default()
        });
        let under_clicks = Rc::new(Cell::new(0));
        let mut runtime = Runtime::new("page");

        let frame = |runtime: &mut Runtime, pointer_events: Vec<PointerEvent>| {
            let dirty_state = state.clone();
            let compose_state = state.clone();
            let callback_under = under_clicks.clone();
            runtime.frame_incremental(
                FrameInput::new(Screen::new(480.0, 360.0), 0.0).pointer_events(pointer_events),
                move || dirty_state.take_dirty(),
                move |ui, _| {
                    let callback_under = callback_under.clone();
                    ui.rect("under")
                        .position(0.0, 0.0)
                        .size(480.0, 360.0)
                        .on_click(move || callback_under.set(callback_under.get() + 1))
                        .build();
                    let open = compose_state.signal(
                        "test.open",
                        |state| state.open,
                        |state, value| state.open = value,
                    );
                    let time = compose_state.signal(
                        "test.time",
                        |state| state.time,
                        |state, value| state.time = value,
                    );
                    time_picker(ui, "time")
                        .open_signal(open)
                        .value_signal(time)
                        .screen(480.0, 360.0)
                        .build();
                },
            );
        };

        frame(&mut runtime, Vec::new());
        assert!(runtime.find("time.panel").is_some());

        frame(
            &mut runtime,
            vec![
                PointerEvent::pressed_at(8.0, 8.0),
                PointerEvent::released_at(8.0, 8.0),
            ],
        );

        assert!(!state.read(|state| state.open));
        assert_eq!(under_clicks.get(), 0);
        assert_eq!(runtime.debug_snapshot_current().layer_dismissals.len(), 1);
        let dismissal = &runtime.debug_snapshot_current().layer_dismissals[0];
        assert_eq!(dismissal.id, "page.time");
        assert_eq!(dismissal.owner, "page.time");
        assert_eq!(dismissal.policy, OutsideClickPolicy::Close);
        assert!(runtime.debug_snapshot().events.iter().any(|event| {
            event.target.role() == "layer"
                && event.target.id() == "page.time"
                && event.command == "dismiss"
                && event.callback
        }));

        frame(&mut runtime, Vec::new());
        assert!(runtime.find("time.panel").is_none());
    }

    #[test]
    fn color_picker_outside_click_dismisses_open_signal_through_layer_policy() {
        let state = State::new(BoundWidgetState {
            open: true,
            color: Color::rgba8(64, 128, 192, 255),
            ..BoundWidgetState::default()
        });
        let under_clicks = Rc::new(Cell::new(0));
        let mut runtime = Runtime::new("page");

        let frame = |runtime: &mut Runtime, pointer_events: Vec<PointerEvent>| {
            let dirty_state = state.clone();
            let compose_state = state.clone();
            let callback_under = under_clicks.clone();
            runtime.frame_incremental(
                FrameInput::new(Screen::new(480.0, 360.0), 0.0).pointer_events(pointer_events),
                move || dirty_state.take_dirty(),
                move |ui, _| {
                    let callback_under = callback_under.clone();
                    ui.rect("under")
                        .position(0.0, 0.0)
                        .size(480.0, 360.0)
                        .on_click(move || callback_under.set(callback_under.get() + 1))
                        .build();
                    let open = compose_state.signal(
                        "test.open",
                        |state| state.open,
                        |state, value| state.open = value,
                    );
                    let color = compose_state.signal(
                        "test.color",
                        |state| state.color,
                        |state, value| state.color = value,
                    );
                    color_picker(ui, "color")
                        .open_signal(open)
                        .value_signal(color)
                        .screen(480.0, 360.0)
                        .build();
                },
            );
        };

        frame(&mut runtime, Vec::new());
        assert!(runtime.find("color.panel").is_some());

        frame(
            &mut runtime,
            vec![
                PointerEvent::pressed_at(8.0, 8.0),
                PointerEvent::released_at(8.0, 8.0),
            ],
        );

        assert!(!state.read(|state| state.open));
        assert_eq!(under_clicks.get(), 0);
        assert_eq!(runtime.debug_snapshot_current().layer_dismissals.len(), 1);
        let dismissal = &runtime.debug_snapshot_current().layer_dismissals[0];
        assert_eq!(dismissal.id, "page.color");
        assert_eq!(dismissal.owner, "page.color");
        assert_eq!(dismissal.policy, OutsideClickPolicy::Close);
        assert!(runtime.debug_snapshot().events.iter().any(|event| {
            event.target.role() == "layer"
                && event.target.id() == "page.color"
                && event.command == "dismiss"
                && event.callback
        }));

        frame(&mut runtime, Vec::new());
        assert!(runtime.find("color.panel").is_none());
    }

    #[test]
    fn toast_auto_dismiss_timer_runs_when_visible() {
        let dismissed = Rc::new(Cell::new(false));
        let callback_dismissed = dismissed.clone();
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 480.0, 320.0, move |ui, _| {
            let callback_dismissed = callback_dismissed.clone();
            toast(ui, "saved")
                .visible(true)
                .screen(480.0, 320.0)
                .duration(0.1)
                .on_auto_dismiss(move || callback_dismissed.set(true))
                .build();
        });

        let layer = runtime
            .debug_snapshot()
            .layers
            .iter()
            .find(|layer| layer.id == "page.saved")
            .expect("toast should register a layer intent");
        assert!(layer.open);
        assert_eq!(layer.kind, LayerKind::Toast);
        assert_eq!(layer.placement, LayerPlacement::BottomEnd);
        assert_eq!(layer.outside_click, OutsideClickPolicy::Ignore);
        assert!(!runtime.tick_timers(0.05));
        assert!(runtime.tick_timers(0.05));
        assert!(dismissed.get());
    }

    #[test]
    fn image_with_style_emits_source_shaped_draw_command() {
        let style = super::image::ImageStyle {
            tint: Color::new(0.5, 0.75, 1.0, 0.8),
            radius: 7.0,
            opacity: 0.6,
        };
        let mut runtime = Runtime::new("page");
        compose(&mut runtime, 200.0, 100.0, move |ui, _| {
            image_with_style(ui, "avatar", style)
                .position(10.0, 20.0)
                .size(80.0, 40.0)
                .source("avatar.png")
                .contain()
                .flip_vertically(true)
                .build();
        });

        let draw = runtime.draw_list();
        let image = draw
            .commands()
            .iter()
            .find_map(|command| match command {
                UiDrawCommand::Image(draw) => Some(draw),
                _ => None,
            })
            .unwrap();

        assert_eq!(image.id, "page.avatar");
        assert_eq!(image.frame.x, 10.0);
        assert_eq!(image.frame.y, 20.0);
        assert_eq!(image.image.source(), "avatar.png");
        assert!(image.image.flip_vertically());
        assert_eq!(image.fit, ImageFit::Contain);
        assert_eq!(image.tint.to_array(), style.tint.to_array());
        assert_eq!(image.radius, 7.0);
        assert_eq!(image.opacity, 0.6);
    }

    #[test]
    fn skin_button_uses_registered_nine_slice_image_and_font_keys() {
        let mut runtime = Runtime::new("page");
        runtime.register_skin(
            NeoSkin::new("kenney")
                .image("button.green.normal", ImageRef::path("ui/button_green.png"))
                .font("future", FontRef::path("ui/Kenney Future.ttf"))
                .button(
                    "button.green",
                    ButtonSkin {
                        normal: ImageRef::key("kenney.button.green.normal"),
                        font: FontRef::key("kenney.future"),
                        slice: Slice::px4(14.0, 12.0, 14.0, 18.0),
                        content_inset: EdgeInsets::px4(24.0, 8.0, 24.0, 12.0),
                        ..ButtonSkin::default()
                    },
                ),
        );

        compose(&mut runtime, 320.0, 120.0, |ui, _| {
            skin_button(ui, "play")
                .skin("kenney.button.green")
                .text("PLAY")
                .size(180.0, 56.0)
                .build();
        });

        let draw = runtime.draw_list();
        let nine_slice = draw
            .commands()
            .iter()
            .find_map(|command| match command {
                UiDrawCommand::NineSlice(draw) => Some(draw),
                _ => None,
            })
            .unwrap();
        assert_eq!(nine_slice.image.source(), "ui/button_green.png");
        assert_eq!(nine_slice.slice.left, 14.0);

        let text = draw
            .commands()
            .iter()
            .find_map(|command| match command {
                UiDrawCommand::Text(draw) if draw.text == "PLAY" => Some(draw),
                _ => None,
            })
            .unwrap();
        assert_eq!(text.font.as_source(), Some("ui/Kenney Future.ttf"));
    }
}
