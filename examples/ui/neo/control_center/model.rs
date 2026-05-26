#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Tasks,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    EnUs,
    ZhCn,
}

impl Locale {
    pub fn index(self) -> i32 {
        match self {
            Self::EnUs => 0,
            Self::ZhCn => 1,
        }
    }

    pub fn from_index(value: i32) -> Self {
        match value {
            1 => Self::ZhCn,
            _ => Self::EnUs,
        }
    }
}

impl Page {
    pub const ALL: [Self; 3] = [Self::Overview, Self::Tasks, Self::Settings];

    pub fn index(self) -> i32 {
        match self {
            Self::Overview => 0,
            Self::Tasks => 1,
            Self::Settings => 2,
        }
    }

    pub fn icon(self) -> u32 {
        match self {
            Self::Overview => 0xF201,
            Self::Tasks => 0xF0AE,
            Self::Settings => 0xF013,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Ready,
    Running,
    Paused,
    Shipping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Low,
    Medium,
    High,
}

impl Priority {
    pub fn index(self) -> i32 {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
        }
    }

    pub fn from_index(value: i32) -> Self {
        match value {
            0 => Self::Low,
            2 => Self::High,
            _ => Self::Medium,
        }
    }

    pub fn score(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMode {
    Balance,
    Sprint,
    Quiet,
}

impl FocusMode {
    pub fn index(self) -> i32 {
        match self {
            Self::Balance => 0,
            Self::Sprint => 1,
            Self::Quiet => 2,
        }
    }

    pub fn from_index(value: i32) -> Self {
        match value {
            1 => Self::Sprint,
            2 => Self::Quiet,
            _ => Self::Balance,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Nocturne,
    Studio,
    Paper,
}

impl ThemeMode {
    pub fn index(self) -> i32 {
        match self {
            Self::Nocturne => 0,
            Self::Studio => 1,
            Self::Paper => 2,
        }
    }

    pub fn from_index(value: i32) -> Self {
        match value {
            1 => Self::Studio,
            2 => Self::Paper,
            _ => Self::Nocturne,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityPreset {
    Draft,
    Balanced,
    Crisp,
}

impl QualityPreset {
    pub fn index(self) -> i32 {
        match self {
            Self::Draft => 0,
            Self::Balanced => 1,
            Self::Crisp => 2,
        }
    }

    pub fn from_index(value: i32) -> Self {
        match value {
            0 => Self::Draft,
            2 => Self::Crisp,
            _ => Self::Balanced,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskItem {
    pub id: u32,
    pub title: String,
    pub priority: Priority,
    pub done: bool,
    pub urgent: bool,
}

#[derive(Debug, Clone)]
pub struct DraftTask {
    pub title: String,
    pub priority: Priority,
    pub urgent: bool,
}

#[derive(Debug, Clone)]
pub struct ToastModel {
    pub visible: bool,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct AppModel {
    pub locale: Locale,
    pub page: Page,
    pub run_state: RunState,
    pub project_name: String,
    pub tasks: Vec<TaskItem>,
    pub draft: DraftTask,
    pub new_task_sheet_open: bool,
    pub ship_dialog_open: bool,
    pub toast: ToastModel,
    pub notifications_enabled: bool,
    pub auto_save: bool,
    pub ui_scale: f32,
    pub volume: f32,
    pub focus_mode: FocusMode,
    pub theme_mode: ThemeMode,
    pub quality_preset: QualityPreset,
    pub quality_preset_open: bool,
    pub overview_scroll: f32,
    pub tasks_scroll: f32,
    pub settings_scroll: f32,
    pub next_task_id: u32,
}

impl Default for AppModel {
    fn default() -> Self {
        let locale = Locale::EnUs;
        Self {
            locale,
            page: Page::Overview,
            run_state: RunState::Ready,
            project_name: "Skyline Shipping".to_string(),
            tasks: vec![
                TaskItem {
                    id: 1,
                    title: "Tune the launch copy".to_string(),
                    priority: Priority::High,
                    done: false,
                    urgent: true,
                },
                TaskItem {
                    id: 2,
                    title: "Polish animation pacing".to_string(),
                    priority: Priority::Medium,
                    done: false,
                    urgent: false,
                },
                TaskItem {
                    id: 3,
                    title: "Verify release notes".to_string(),
                    priority: Priority::High,
                    done: true,
                    urgent: false,
                },
                TaskItem {
                    id: 4,
                    title: "Archive stale mockups".to_string(),
                    priority: Priority::Low,
                    done: false,
                    urgent: false,
                },
            ],
            draft: DraftTask {
                title: String::new(),
                priority: Priority::Medium,
                urgent: false,
            },
            new_task_sheet_open: false,
            ship_dialog_open: false,
            toast: ToastModel {
                visible: false,
                title: crate::locale::run_state_label(locale, RunState::Ready).to_string(),
                message: crate::locale::run_state_summary(locale, RunState::Ready).to_string(),
            },
            notifications_enabled: true,
            auto_save: true,
            ui_scale: 0.72,
            volume: 0.58,
            focus_mode: FocusMode::Balance,
            theme_mode: ThemeMode::Nocturne,
            quality_preset: QualityPreset::Balanced,
            quality_preset_open: false,
            overview_scroll: 0.0,
            tasks_scroll: 0.0,
            settings_scroll: 0.0,
            next_task_id: 5,
        }
    }
}

impl AppModel {
    pub fn active_tasks(&self) -> usize {
        self.tasks.iter().filter(|task| !task.done).count()
    }

    pub fn completed_tasks(&self) -> usize {
        self.tasks.iter().filter(|task| task.done).count()
    }

    pub fn urgent_tasks(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| !task.done && task.urgent)
            .count()
    }

    pub fn completion_ratio(&self) -> f32 {
        if self.tasks.is_empty() {
            0.0
        } else {
            self.completed_tasks() as f32 / self.tasks.len() as f32
        }
    }

    pub fn completion_percent(&self) -> u32 {
        (self.completion_ratio() * 100.0).round() as u32
    }

    pub fn next_task(&self) -> Option<&TaskItem> {
        self.tasks
            .iter()
            .filter(|task| !task.done)
            .max_by_key(|task| (task.urgent, task.priority.score(), u8::from(!task.done)))
    }
}
