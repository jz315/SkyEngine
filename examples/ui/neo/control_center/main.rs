mod actions;
mod app;
mod locale;
mod model;
mod theme;
mod view;

use sky_engine::app::{App, AssetPlugin, InputPlugin, RenderPlugin, RunnerPlugin, WindowPlugin};
use sky_engine::ecs::World;

fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("Neo Control Center", 1440, 920)
                .with_vsync(true)
                .with_resizable(true),
        )
        .unwrap();
    world
        .install(RunnerPlugin::game().with_frame_rate_limit(60.0))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world.install(RenderPlugin::forward_2d()).unwrap();

    App::new(world).run(app::NeoControlCenter::default());
}

#[cfg(test)]
mod tests {
    use sky_engine::ui::neo::expert::UiDrawCommand;
    use sky_engine::ui::neo::{Color, Frame, FrameInput, PointerEvent, Runtime, Screen};

    use super::model::{AppModel, Page};
    use super::theme;
    use super::view::{self, RuntimeInfo};

    fn compose_control_center(runtime: &mut Runtime, state: &sky_engine::ui::neo::State<AppModel>) {
        let snapshot = state.read(Clone::clone);
        let runtime_info = RuntimeInfo {
            uptime_seconds: 0.0,
            frame_count: 0,
        };
        runtime.compose(1440.0, 920.0, |ui, screen| {
            view::render(ui, screen, state, &snapshot, runtime_info);
        });
    }

    fn compose_control_center_incremental(
        runtime: &mut Runtime,
        state: &sky_engine::ui::neo::State<AppModel>,
    ) {
        let snapshot = state.read(Clone::clone);
        let force_full_compose = runtime.needs_compose();
        let dirty_ids = state.take_dirty_ids();
        let runtime_info = RuntimeInfo {
            uptime_seconds: 0.0,
            frame_count: 0,
        };
        if force_full_compose && dirty_ids.is_empty() {
            runtime.compose(1440.0, 920.0, |ui, screen| {
                view::render(ui, screen, state, &snapshot, runtime_info);
            });
        } else {
            runtime.compose_incremental(1440.0, 920.0, dirty_ids, |ui, screen| {
                view::render(ui, screen, state, &snapshot, runtime_info);
            });
        }
    }

    fn click(runtime: &mut Runtime, x: f32, y: f32) {
        runtime.update_pointer(PointerEvent::pressed_at(x, y));
        runtime.update_pointer(PointerEvent::released_at(x, y));
    }

    fn frame_control_center(
        runtime: &mut Runtime,
        state: &sky_engine::ui::neo::State<AppModel>,
        pointer: PointerEvent,
    ) -> Frame {
        runtime
            .frame(
                FrameInput::new(Screen::new(1440.0, 920.0), 1.0 / 60.0).pointer(pointer),
                |ui, screen| {
                    let snapshot = state.read(Clone::clone);
                    view::render(
                        ui,
                        screen,
                        state,
                        &snapshot,
                        RuntimeInfo {
                            uptime_seconds: 0.0,
                            frame_count: 1,
                        },
                    );
                },
            )
            .frame
    }

    fn nav_color(frame: &Frame, index: usize) -> Color {
        let id = format!("neo.control-center.nav.{index}.bg");
        frame
            .draw_list()
            .commands()
            .iter()
            .find_map(|command| match command {
                UiDrawCommand::Rect(draw) if draw.id == id => Some(draw.color),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing nav draw command {id}"))
    }

    fn color_distance(left: Color, right: Color) -> f32 {
        let dr = left.r - right.r;
        let dg = left.g - right.g;
        let db = left.b - right.b;
        dr * dr + dg * dg + db * db
    }

    #[test]
    fn sidebar_nav_buttons_switch_pages_from_their_visual_bounds() {
        let state = sky_engine::ui::neo::State::new(AppModel::default());
        let mut runtime = Runtime::new("neo");

        compose_control_center(&mut runtime, &state);
        let tasks_frame = runtime.find("control-center.nav.1.bg").unwrap().frame;
        click(
            &mut runtime,
            tasks_frame.x + tasks_frame.width * 0.5,
            tasks_frame.y + tasks_frame.height * 0.5,
        );
        assert_eq!(state.read(|model| model.page), Page::Tasks);

        compose_control_center(&mut runtime, &state);
        let overview_frame = runtime.find("control-center.nav.0.bg").unwrap().frame;
        click(
            &mut runtime,
            overview_frame.x + overview_frame.width * 0.5,
            overview_frame.y + overview_frame.height * 0.5,
        );
        assert_eq!(state.read(|model| model.page), Page::Overview);
    }

    #[test]
    fn sidebar_nav_buttons_switch_pages_through_incremental_compose() {
        let state = sky_engine::ui::neo::State::new(AppModel::default());
        let mut runtime = Runtime::new("neo");

        compose_control_center_incremental(&mut runtime, &state);
        let tasks_frame = runtime.find("control-center.nav.1.bg").unwrap().frame;
        click(
            &mut runtime,
            tasks_frame.x + tasks_frame.width * 0.5,
            tasks_frame.y + tasks_frame.height * 0.5,
        );
        assert_eq!(state.read(|model| model.page), Page::Tasks);
        assert!(
            runtime.needs_compose(),
            "nav click should mark the runtime dirty before incremental compose"
        );

        compose_control_center_incremental(&mut runtime, &state);
        assert_eq!(
            runtime.debug_snapshot().layout_mode,
            sky_engine::ui::neo::LayoutMode::Full(
                sky_engine::ui::neo::FullLayoutReason::StructureChanged {
                    ids: vec!["neo.control-center.workspace".to_string()]
                }
            )
        );
        assert_eq!(
            runtime.debug_snapshot().dirty_ids,
            vec![
                "neo.control-center.nav".to_string(),
                "neo.control-center.workspace".to_string(),
            ]
        );
        let theme = theme::resolve(state.read(|model| model.theme_mode));
        let frame = runtime.current_frame();
        let overview_to_primary = color_distance(nav_color(&frame, 0), theme.tokens.primary);
        let tasks_to_primary = color_distance(nav_color(&frame, 1), theme.tokens.primary);
        assert!(
            tasks_to_primary < overview_to_primary,
            "incremental compose should repaint selected Tasks nav; \
             overview_distance={overview_to_primary}, tasks_distance={tasks_to_primary}"
        );
        assert_eq!(
            runtime.find("control-center.header.title").unwrap().text,
            "Tasks"
        );
    }

    #[test]
    fn compose_frame_reads_page_after_nav_click_callback() {
        let state = sky_engine::ui::neo::State::new(AppModel::default());
        let mut runtime = Runtime::new("neo");

        compose_control_center(&mut runtime, &state);
        let tasks_frame = runtime.find("control-center.nav.1.bg").unwrap().frame;
        let click_x = tasks_frame.x + tasks_frame.width * 0.5;
        let click_y = tasks_frame.y + tasks_frame.height * 0.5;

        runtime.update_pointer(PointerEvent::pressed_at(click_x, click_y));
        let result = runtime.frame(
            FrameInput::new(Screen::new(1440.0, 920.0), 1.0 / 60.0)
                .pointer(PointerEvent::released_at(click_x, click_y)),
            |ui, screen| {
                let snapshot = state.read(Clone::clone);
                view::render(
                    ui,
                    screen,
                    &state,
                    &snapshot,
                    RuntimeInfo {
                        uptime_seconds: 0.0,
                        frame_count: 1,
                    },
                );
                snapshot.page
            },
        );

        assert_eq!(result.value, Page::Tasks);
    }

    #[test]
    fn clicked_nav_buttons_animate_old_out_and_new_in() {
        let state = sky_engine::ui::neo::State::new(AppModel::default());
        let mut runtime = Runtime::new("neo");

        frame_control_center(&mut runtime, &state, PointerEvent::default());
        let tasks_frame = runtime.find("control-center.nav.1.bg").unwrap().frame;
        let click_x = tasks_frame.x + tasks_frame.width * 0.5;
        let click_y = tasks_frame.y + tasks_frame.height * 0.5;

        frame_control_center(
            &mut runtime,
            &state,
            PointerEvent::pressed_at(click_x, click_y),
        );
        let frame = frame_control_center(
            &mut runtime,
            &state,
            PointerEvent::released_at(click_x, click_y),
        );

        let theme = theme::resolve(state.read(|model| model.theme_mode));
        let overview_start = color_distance(nav_color(&frame, 0), theme.tokens.primary);
        let tasks_start = color_distance(nav_color(&frame, 1), theme.tokens.primary);
        let mut later = frame;
        for _ in 0..12 {
            later = frame_control_center(&mut runtime, &state, PointerEvent::at(click_x, click_y));
        }
        let overview_later = color_distance(nav_color(&later, 0), theme.tokens.primary);
        let tasks_later = color_distance(nav_color(&later, 1), theme.tokens.primary);
        assert!(
            overview_later > overview_start,
            "old Overview nav should animate away from primary; \
             start_distance={overview_start}, later_distance={overview_later}"
        );
        assert!(
            tasks_later < tasks_start,
            "new Tasks nav should animate toward primary; \
             start_distance={tasks_start}, later_distance={tasks_later}"
        );
    }

    #[test]
    fn previous_nav_button_does_not_remain_selected_after_later_frames() {
        let state = sky_engine::ui::neo::State::new(AppModel::default());
        let mut runtime = Runtime::new("neo");

        frame_control_center(&mut runtime, &state, PointerEvent::default());
        let tasks_frame = runtime.find("control-center.nav.1.bg").unwrap().frame;
        let click_x = tasks_frame.x + tasks_frame.width * 0.5;
        let click_y = tasks_frame.y + tasks_frame.height * 0.5;

        frame_control_center(
            &mut runtime,
            &state,
            PointerEvent::pressed_at(click_x, click_y),
        );
        let mut frame = frame_control_center(
            &mut runtime,
            &state,
            PointerEvent::released_at(click_x, click_y),
        );
        for _ in 0..60 {
            frame = frame_control_center(&mut runtime, &state, PointerEvent::at(click_x, click_y));
        }

        let theme = theme::resolve(state.read(|model| model.theme_mode));
        let overview_to_primary = color_distance(nav_color(&frame, 0), theme.tokens.primary);
        let tasks_to_primary = color_distance(nav_color(&frame, 1), theme.tokens.primary);
        assert!(
            tasks_to_primary < overview_to_primary,
            "after later frames Tasks should remain selected, not Overview; \
             overview_distance={overview_to_primary}, tasks_distance={tasks_to_primary}"
        );
    }

    #[test]
    fn nav_selection_returns_to_overview_after_clicking_back() {
        let state = sky_engine::ui::neo::State::new(AppModel::default());
        let mut runtime = Runtime::new("neo");

        frame_control_center(&mut runtime, &state, PointerEvent::default());
        let tasks_frame = runtime.find("control-center.nav.1.bg").unwrap().frame;
        let tasks_x = tasks_frame.x + tasks_frame.width * 0.5;
        let tasks_y = tasks_frame.y + tasks_frame.height * 0.5;
        frame_control_center(
            &mut runtime,
            &state,
            PointerEvent::pressed_at(tasks_x, tasks_y),
        );
        frame_control_center(
            &mut runtime,
            &state,
            PointerEvent::released_at(tasks_x, tasks_y),
        );

        let overview_frame = runtime.find("control-center.nav.0.bg").unwrap().frame;
        let overview_x = overview_frame.x + overview_frame.width * 0.5;
        let overview_y = overview_frame.y + overview_frame.height * 0.5;
        frame_control_center(
            &mut runtime,
            &state,
            PointerEvent::pressed_at(overview_x, overview_y),
        );
        let mut frame = frame_control_center(
            &mut runtime,
            &state,
            PointerEvent::released_at(overview_x, overview_y),
        );
        for _ in 0..60 {
            frame = frame_control_center(
                &mut runtime,
                &state,
                PointerEvent::at(overview_x, overview_y),
            );
        }

        let theme = theme::resolve(state.read(|model| model.theme_mode));
        let overview_to_primary = color_distance(nav_color(&frame, 0), theme.tokens.primary);
        let tasks_to_primary = color_distance(nav_color(&frame, 1), theme.tokens.primary);
        assert!(
            overview_to_primary < tasks_to_primary,
            "after clicking back Overview should be selected, not Tasks; \
             overview_distance={overview_to_primary}, tasks_distance={tasks_to_primary}"
        );
    }
}
