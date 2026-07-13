use sky_engine::ui::serein::{Signal, State};

use crate::locale;
use crate::model::{
    AppModel, FocusMode, Page, Priority, QualityPreset, RunState, ThemeMode, ToastModel,
};

pub fn project_name_signal(state: &State<AppModel>) -> Signal<AppModel, String> {
    state.signal(
        "control-center.project-name",
        |model| model.project_name.clone(),
        |model, value| model.project_name = value,
    )
}

pub fn focus_mode_signal(state: &State<AppModel>) -> Signal<AppModel, i32> {
    state.signal(
        "control-center.focus-mode",
        |model| model.focus_mode.index(),
        |model, value| model.focus_mode = FocusMode::from_index(value),
    )
}

pub fn page_signal(state: &State<AppModel>) -> Signal<AppModel, i32> {
    state.signal(
        "control-center.page",
        |model| model.page.index(),
        |model, value| model.page = Page::from_index(value),
    )
}

pub fn locale_signal(state: &State<AppModel>) -> Signal<AppModel, i32> {
    state.signal(
        "control-center.locale",
        |model| model.locale.index(),
        |model, value| model.locale = crate::model::Locale::from_index(value),
    )
}

pub fn theme_mode_signal(state: &State<AppModel>) -> Signal<AppModel, i32> {
    state.signal(
        "control-center.theme-mode",
        |model| model.theme_mode.index(),
        |model, value| model.theme_mode = ThemeMode::from_index(value),
    )
}

pub fn quality_preset_signal(state: &State<AppModel>) -> Signal<AppModel, i32> {
    state.signal(
        "control-center.quality-preset",
        |model| model.quality_preset.index(),
        |model, value| model.quality_preset = QualityPreset::from_index(value),
    )
}

pub fn quality_preset_open_signal(state: &State<AppModel>) -> Signal<AppModel, bool> {
    state.signal(
        "control-center.quality-preset-open",
        |model| model.quality_preset_open,
        |model, value| model.quality_preset_open = value,
    )
}

pub fn notifications_signal(state: &State<AppModel>) -> Signal<AppModel, bool> {
    state.signal(
        "control-center.notifications",
        |model| model.notifications_enabled,
        |model, value| model.notifications_enabled = value,
    )
}

pub fn auto_save_signal(state: &State<AppModel>) -> Signal<AppModel, bool> {
    state.signal(
        "control-center.auto-save",
        |model| model.auto_save,
        |model, value| model.auto_save = value,
    )
}

pub fn ui_scale_signal(state: &State<AppModel>) -> Signal<AppModel, f32> {
    state.signal(
        "control-center.ui-scale",
        |model| model.ui_scale,
        |model, value| model.ui_scale = value.clamp(0.0, 1.0),
    )
}

pub fn volume_signal(state: &State<AppModel>) -> Signal<AppModel, f32> {
    state.signal(
        "control-center.volume",
        |model| model.volume,
        |model, value| model.volume = value.clamp(0.0, 1.0),
    )
}

pub fn overview_scroll_signal(state: &State<AppModel>) -> Signal<AppModel, f32> {
    state.signal(
        "control-center.overview-scroll",
        |model| model.overview_scroll,
        |model, value| model.overview_scroll = value.max(0.0),
    )
}

pub fn tasks_scroll_signal(state: &State<AppModel>) -> Signal<AppModel, f32> {
    state.signal(
        "control-center.tasks-scroll",
        |model| model.tasks_scroll,
        |model, value| model.tasks_scroll = value.max(0.0),
    )
}

pub fn settings_scroll_signal(state: &State<AppModel>) -> Signal<AppModel, f32> {
    state.signal(
        "control-center.settings-scroll",
        |model| model.settings_scroll,
        |model, value| model.settings_scroll = value.max(0.0),
    )
}

pub fn draft_title_signal(state: &State<AppModel>) -> Signal<AppModel, String> {
    state.signal(
        "control-center.draft-title",
        |model| model.draft.title.clone(),
        |model, value| model.draft.title = value,
    )
}

pub fn draft_priority_signal(state: &State<AppModel>) -> Signal<AppModel, i32> {
    state.signal(
        "control-center.draft-priority",
        |model| model.draft.priority.index(),
        |model, value| model.draft.priority = Priority::from_index(value),
    )
}

pub fn draft_urgent_signal(state: &State<AppModel>) -> Signal<AppModel, bool> {
    state.signal(
        "control-center.draft-urgent",
        |model| model.draft.urgent,
        |model, value| model.draft.urgent = value,
    )
}

pub fn toast_visible_signal(state: &State<AppModel>) -> Signal<AppModel, bool> {
    state.signal(
        "control-center.toast-visible",
        |model| model.toast.visible,
        |model, value| model.toast.visible = value,
    )
}

pub fn advance_run(state: &State<AppModel>) {
    state.update(|model| {
        model.run_state = RunState::Running;
        let completed = if let Some(task) = model.tasks.iter_mut().find(|task| !task.done) {
            task.done = true;
            Some(task.title.clone())
        } else {
            None
        };

        if let Some(title) = completed {
            let (toast_title, toast_message) = locale::run_advanced_toast(model.locale, &title);
            push_toast(model, toast_title, toast_message);
        } else {
            let (toast_title, toast_message) = locale::queue_clear_toast(model.locale);
            push_toast(model, toast_title, toast_message);
        }
    });
}

pub fn pause_run(state: &State<AppModel>) {
    state.update(|model| {
        model.run_state = RunState::Paused;
        let (toast_title, toast_message) = locale::run_paused_toast(model.locale);
        push_toast(model, toast_title, toast_message);
    });
}

pub fn open_ship_dialog(state: &State<AppModel>) {
    state.update(|model| model.ship_dialog_open = true);
}

pub fn close_ship_dialog(state: &State<AppModel>) {
    state.update(|model| model.ship_dialog_open = false);
}

pub fn confirm_ship(state: &State<AppModel>) {
    state.update(|model| {
        model.ship_dialog_open = false;
        model.run_state = RunState::Shipping;
        let (toast_title, toast_message) = locale::build_queued_toast(model.locale);
        push_toast(model, toast_title, toast_message);
    });
}

pub fn open_new_task_sheet(state: &State<AppModel>) {
    state.update(|model| {
        model.new_task_sheet_open = true;
        model.page = Page::Tasks;
    });
}

pub fn close_new_task_sheet(state: &State<AppModel>) {
    state.update(|model| model.new_task_sheet_open = false);
}

pub fn submit_new_task(state: &State<AppModel>) {
    state.update(|model| {
        let title = model.draft.title.trim().to_string();
        if title.is_empty() {
            let (toast_title, toast_message) = locale::name_required_toast(model.locale);
            push_toast(model, toast_title, toast_message);
            return;
        }

        let next_id = model.next_task_id;
        model.next_task_id += 1;
        model.tasks.push(crate::model::TaskItem {
            id: next_id,
            title: title.clone(),
            priority: model.draft.priority,
            done: false,
            urgent: model.draft.urgent,
        });
        model.new_task_sheet_open = false;
        model.draft.title.clear();
        model.draft.priority = Priority::Medium;
        model.draft.urgent = false;
        let (toast_title, toast_message) = locale::create_task_toast(model.locale, &title);
        push_toast(model, toast_title, toast_message);
    });
}

pub fn toggle_task_done(state: &State<AppModel>, task_id: u32) {
    state.update(|model| {
        if let Some(task) = model.tasks.iter_mut().find(|task| task.id == task_id) {
            task.done = !task.done;
            let title = task.title.clone();
            if task.done {
                let (toast_title, toast_message) =
                    locale::task_completed_toast(model.locale, &title);
                push_toast(model, toast_title, toast_message);
            } else {
                let (toast_title, toast_message) =
                    locale::task_reopened_toast(model.locale, &title);
                push_toast(model, toast_title, toast_message);
            }
        }
    });
}

fn push_toast(model: &mut AppModel, title: impl Into<String>, message: impl Into<String>) {
    model.toast = ToastModel {
        visible: true,
        title: title.into(),
        message: message.into(),
    };
}
