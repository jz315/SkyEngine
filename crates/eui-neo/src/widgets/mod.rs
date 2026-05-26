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
        badge, button, checkbox, context_menu, date_picker, dialog, dropdown, image_with_style,
        input, popover, progress, radio, segmented, skin_button, slider, switch, tabs, time_picker,
        toast, PopoverPlacement,
    };
    use crate::expert::UiDrawCommand;
    use crate::Color;
    use crate::{
        ButtonSkin, EdgeInsets, FontRef, ImageFit, ImageRef, KeyboardEvent, NeoSkin, NeoState,
        PointerEvent, Runtime, ScrollEvent, Size, Slice,
    };
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn button_composes_source_shaped_stack_background_and_content() {
        let mut runtime = Runtime::new("page");
        runtime.compose(320.0, 180.0, |ui, _| {
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
        runtime.compose(420.0, 80.0, |ui, _| {
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
    fn input_layout_supports_fill_children_when_grown() {
        let mut runtime = Runtime::new("page");
        runtime.compose(460.0, 80.0, |ui, _| {
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
        runtime.compose(420.0, 80.0, |ui, _| {
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
        runtime.compose(400.0, 80.0, |ui, _| {
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
        runtime.compose(400.0, 80.0, move |ui, _| {
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
        runtime.compose(240.0, 80.0, move |ui, _| {
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

    #[derive(Default)]
    struct BoundWidgetState {
        checked: bool,
        slider: f32,
        text: String,
        selected: i32,
        open: bool,
        offset: f32,
        date: [i32; 3],
        time: [i32; 2],
    }

    #[test]
    fn checkbox_binding_writes_clicked_value_to_state() {
        let state = NeoState::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(240.0, 80.0, move |ui, _| {
            let checked =
                compose_state.bind(|state| state.checked, |state, value| state.checked = value);
            checkbox(ui, "sound")
                .checked_bind(checked)
                .text("Sound")
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(2.0, 2.0));
        runtime.update_pointer(PointerEvent::released_at(2.0, 2.0));

        assert!(state.read(|state| state.checked));
    }

    #[test]
    fn switch_binding_writes_clicked_value_to_state() {
        let state = NeoState::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(240.0, 80.0, move |ui, _| {
            let checked =
                compose_state.bind(|state| state.checked, |state, value| state.checked = value);
            switch(ui, "night")
                .checked_bind(checked)
                .label("Night")
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(2.0, 2.0));
        runtime.update_pointer(PointerEvent::released_at(2.0, 2.0));

        assert!(state.read(|state| state.checked));
    }

    #[test]
    fn slider_binding_writes_pressed_value_to_state() {
        let state = NeoState::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(320.0, 80.0, move |ui, _| {
            let slider_value =
                compose_state.bind(|state| state.slider, |state, value| state.slider = value);
            slider(ui, "volume")
                .size(200.0, 20.0)
                .value_bind(slider_value)
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(100.0, 10.0));

        assert!((state.read(|state| state.slider) - 0.5).abs() < 0.001);
    }

    #[test]
    fn input_binding_writes_text_events_to_state() {
        let state = NeoState::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(320.0, 120.0, move |ui, _| {
            let text = compose_state.bind_clone(
                |state| state.text.clone(),
                |state, value| state.text = value,
            );
            input(ui, "name").text_bind(text).build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(4.0, 4.0));
        runtime.update_keyboard(KeyboardEvent {
            text: "Sky".to_string(),
            ..KeyboardEvent::default()
        });

        assert_eq!(state.read(|state| state.text.clone()), "Sky");
    }

    #[test]
    fn scrollbar_wheel_reports_clamped_offset_change() {
        let offset = Rc::new(Cell::new(-1.0));
        let callback_offset = offset.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(80.0, 240.0, move |ui, _| {
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
        runtime.compose(160.0, 120.0, move |ui, _| {
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
    fn scroll_y_binding_writes_wheel_offset_to_state() {
        let state = NeoState::new(BoundWidgetState {
            offset: 24.0,
            ..BoundWidgetState::default()
        });
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(160.0, 120.0, move |ui, _| {
            let offset =
                compose_state.bind(|state| state.offset, |state, value| state.offset = value);
            ui.scroll_y("list")
                .size(120.0, 80.0)
                .content_height(200.0)
                .offset_bind(offset)
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
            runtime.compose(160.0, 120.0, move |ui, _| {
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
    fn scroll_y_inset_keeps_viewport_and_scrollbar_inside_outer_shell() {
        let mut runtime = Runtime::new("page");
        runtime.compose(160.0, 120.0, |ui, _| {
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
        runtime.compose(180.0, 120.0, move |ui, _| {
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
        runtime.compose(180.0, 120.0, |ui, _| {
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
        runtime.compose(220.0, 160.0, move |ui, _| {
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
        runtime.compose(220.0, 160.0, |ui, _| {
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
        for _ in 0..2 {
            runtime.compose(320.0, 180.0, |ui, _| {
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
        }

        assert_eq!(runtime.roots().len(), 2);
        assert_eq!(runtime.roots()[0].id, "page.panel");
        assert_eq!(runtime.roots()[1].id, "page.menu");
        assert_eq!(runtime.find("menu").unwrap().frame.x, 20.0);
        assert_eq!(runtime.find("menu").unwrap().frame.y, 54.0);
    }

    #[test]
    fn input_text_event_reports_changed_text() {
        let value = Rc::new(std::cell::RefCell::new(String::new()));
        let callback_value = value.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(320.0, 120.0, move |ui, _| {
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
        runtime.compose(320.0, 80.0, move |ui, _| {
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
        runtime.compose(360.0, 80.0, move |ui, _| {
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
    fn segmented_binding_writes_selected_index_to_state() {
        let state = NeoState::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(320.0, 80.0, move |ui, _| {
            let selected = compose_state.bind(
                |state| state.selected,
                |state, value| state.selected = value,
            );
            segmented(ui, "mode")
                .size(180.0, 30.0)
                .items(["A", "B", "C"])
                .selected_bind(selected)
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(70.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(70.0, 10.0));

        assert_eq!(state.read(|state| state.selected), 1);
    }

    #[test]
    fn tabs_binding_writes_selected_index_to_state() {
        let state = NeoState::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(360.0, 80.0, move |ui, _| {
            let selected = compose_state.bind(
                |state| state.selected,
                |state, value| state.selected = value,
            );
            tabs(ui, "tabs")
                .size(240.0, 40.0)
                .items(["Home", "Logs", "About"])
                .selected_bind(selected)
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(170.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(170.0, 10.0));

        assert_eq!(state.read(|state| state.selected), 2);
    }

    #[test]
    fn radio_binding_writes_selected_value_to_state() {
        let state = NeoState::new(BoundWidgetState::default());
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(240.0, 80.0, move |ui, _| {
            let selected = compose_state.bind(
                |state| state.selected == 2,
                |state, value| {
                    if value {
                        state.selected = 2;
                    }
                },
            );
            radio(ui, "choice.c")
                .size(120.0, 28.0)
                .selected_bind(selected)
                .text("Choice C")
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(2.0, 2.0));
        runtime.update_pointer(PointerEvent::released_at(2.0, 2.0));

        assert_eq!(state.read(|state| state.selected), 2);
    }

    #[test]
    fn dropdown_bindings_write_open_and_selection_to_state() {
        let state = NeoState::new(BoundWidgetState {
            open: true,
            selected: 0,
            ..BoundWidgetState::default()
        });
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(320.0, 220.0, move |ui, _| {
            let selected = compose_state.bind(
                |state| state.selected,
                |state, value| state.selected = value,
            );
            let open = compose_state.bind(|state| state.open, |state, value| state.open = value);
            dropdown(ui, "quality")
                .items(["Low", "Medium", "High"])
                .selected_bind(selected)
                .open_bind(open)
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(16.0, 100.0));
        runtime.update_pointer(PointerEvent::released_at(16.0, 100.0));

        assert_eq!(state.read(|state| state.selected), 1);
        assert!(!state.read(|state| state.open));
    }

    #[test]
    fn dialog_backdrop_reports_close() {
        let closed = Rc::new(Cell::new(false));
        let callback_closed = closed.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(400.0, 300.0, move |ui, _| {
            let callback_closed = callback_closed.clone();
            dialog(ui, "confirm")
                .open(true)
                .screen(400.0, 300.0)
                .on_close(move || callback_closed.set(true))
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(8.0, 8.0));
        runtime.update_pointer(PointerEvent::released_at(8.0, 8.0));

        assert!(closed.get());
    }

    #[test]
    fn dropdown_field_toggles_open_state_callback() {
        let opened = Rc::new(Cell::new(false));
        let callback_opened = opened.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(320.0, 160.0, move |ui, _| {
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
        runtime.compose(320.0, 220.0, move |ui, _| {
            let callback_selected = callback_selected.clone();
            let callback_opened = callback_opened.clone();
            dropdown(ui, "quality")
                .items(["Low", "Medium", "High"])
                .open(true)
                .on_change(move |index| callback_selected.set(index))
                .on_open_change(move |next| callback_opened.set(next))
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(16.0, 100.0));
        runtime.update_pointer(PointerEvent::released_at(16.0, 100.0));

        assert_eq!(selected.get(), 1);
        assert!(!opened.get());
    }

    #[test]
    fn context_menu_dismiss_and_select_callbacks_run() {
        let dismissed = Rc::new(Cell::new(false));
        let selected = Rc::new(Cell::new(-1));
        let callback_dismissed = dismissed.clone();
        let callback_selected = selected.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(320.0, 220.0, move |ui, _| {
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
    fn date_picker_binding_writes_done_value_to_state() {
        let state = NeoState::new(BoundWidgetState {
            open: true,
            date: [2026, 4, 28],
            ..BoundWidgetState::default()
        });
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(480.0, 360.0, move |ui, _| {
            let open = compose_state.bind(|state| state.open, |state, value| state.open = value);
            let date = compose_state.bind(|state| state.date, |state, value| state.date = value);
            date_picker(ui, "date")
                .open_bind(open)
                .date_bind(date)
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
    fn time_picker_binding_writes_done_value_to_state() {
        let state = NeoState::new(BoundWidgetState {
            open: true,
            time: [9, 30],
            ..BoundWidgetState::default()
        });
        let compose_state = state.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(420.0, 340.0, move |ui, _| {
            let open = compose_state.bind(|state| state.open, |state, value| state.open = value);
            let time = compose_state.bind(|state| state.time, |state, value| state.time = value);
            time_picker(ui, "time")
                .open_bind(open)
                .time_bind(time)
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
    fn toast_auto_dismiss_timer_runs_when_visible() {
        let dismissed = Rc::new(Cell::new(false));
        let callback_dismissed = dismissed.clone();
        let mut runtime = Runtime::new("page");
        runtime.compose(480.0, 320.0, move |ui, _| {
            let callback_dismissed = callback_dismissed.clone();
            toast(ui, "saved")
                .visible(true)
                .screen(480.0, 320.0)
                .duration(0.1)
                .on_auto_dismiss(move || callback_dismissed.set(true))
                .build();
        });

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
        runtime.compose(200.0, 100.0, move |ui, _| {
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

        runtime.compose(320.0, 120.0, |ui, _| {
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
