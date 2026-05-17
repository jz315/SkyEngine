use crate::model::{FocusMode, Locale, Page, Priority, QualityPreset, RunState, ThemeMode};

pub fn window_title(locale: Locale, project_name: &str, page: Page) -> String {
    format!("{project_name} · {}", page_label(locale, page))
}

pub fn app_kicker(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "NEO APP",
        Locale::ZhCn => "NEO 应用",
    }
}

pub fn app_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Control Center",
        Locale::ZhCn => "控制中心",
    }
}

pub fn page_label(locale: Locale, page: Page) -> &'static str {
    match (locale, page) {
        (Locale::EnUs, Page::Overview) => "Overview",
        (Locale::EnUs, Page::Tasks) => "Tasks",
        (Locale::EnUs, Page::Settings) => "Settings",
        (Locale::ZhCn, Page::Overview) => "总览",
        (Locale::ZhCn, Page::Tasks) => "任务",
        (Locale::ZhCn, Page::Settings) => "设置",
    }
}

pub fn page_subtitle(locale: Locale, page: Page) -> &'static str {
    match (locale, page) {
        (Locale::EnUs, Page::Overview) => "A calm command surface for the work already in motion.",
        (Locale::EnUs, Page::Tasks) => "Short-lived work, triage, and progress in one queue.",
        (Locale::EnUs, Page::Settings) => "Preferences that shape how the workspace feels.",
        (Locale::ZhCn, Page::Overview) => "把正在推进的工作放进一个安静清晰的控制台里。",
        (Locale::ZhCn, Page::Tasks) => "短周期任务、分流和进度都放在同一条队列里。",
        (Locale::ZhCn, Page::Settings) => "调整偏好，让整个工作台更贴近你的节奏。",
    }
}

pub fn run_state_label(locale: Locale, state: RunState) -> &'static str {
    match (locale, state) {
        (Locale::EnUs, RunState::Ready) => "Ready",
        (Locale::EnUs, RunState::Running) => "Running",
        (Locale::EnUs, RunState::Paused) => "Paused",
        (Locale::EnUs, RunState::Shipping) => "Shipping",
        (Locale::ZhCn, RunState::Ready) => "就绪",
        (Locale::ZhCn, RunState::Running) => "运行中",
        (Locale::ZhCn, RunState::Paused) => "已暂停",
        (Locale::ZhCn, RunState::Shipping) => "发版中",
    }
}

pub fn run_state_summary(locale: Locale, state: RunState) -> &'static str {
    match (locale, state) {
        (Locale::EnUs, RunState::Ready) => "Systems are standing by for the next pass.",
        (Locale::EnUs, RunState::Running) => "The queue is moving and feedback is live.",
        (Locale::EnUs, RunState::Paused) => "Flow is paused, state stays warm.",
        (Locale::EnUs, RunState::Shipping) => "Build output is staged for release review.",
        (Locale::ZhCn, RunState::Ready) => "系统已待命，准备进入下一轮推进。",
        (Locale::ZhCn, RunState::Running) => "队列正在流动，反馈保持在线。",
        (Locale::ZhCn, RunState::Paused) => "流程已暂停，但上下文仍然保持温热。",
        (Locale::ZhCn, RunState::Shipping) => "构建产物已进入发版审核阶段。",
    }
}

pub fn priority_label(locale: Locale, priority: Priority) -> &'static str {
    match (locale, priority) {
        (Locale::EnUs, Priority::Low) => "Low",
        (Locale::EnUs, Priority::Medium) => "Medium",
        (Locale::EnUs, Priority::High) => "High",
        (Locale::ZhCn, Priority::Low) => "低",
        (Locale::ZhCn, Priority::Medium) => "中",
        (Locale::ZhCn, Priority::High) => "高",
    }
}

pub fn focus_mode_label(locale: Locale, mode: FocusMode) -> &'static str {
    match (locale, mode) {
        (Locale::EnUs, FocusMode::Balance) => "Balance",
        (Locale::EnUs, FocusMode::Sprint) => "Sprint",
        (Locale::EnUs, FocusMode::Quiet) => "Quiet",
        (Locale::ZhCn, FocusMode::Balance) => "平衡",
        (Locale::ZhCn, FocusMode::Sprint) => "冲刺",
        (Locale::ZhCn, FocusMode::Quiet) => "安静",
    }
}

pub fn theme_mode_label(locale: Locale, mode: ThemeMode) -> &'static str {
    match (locale, mode) {
        (Locale::EnUs, ThemeMode::Nocturne) => "Nocturne",
        (Locale::EnUs, ThemeMode::Studio) => "Studio",
        (Locale::EnUs, ThemeMode::Paper) => "Paper",
        (Locale::ZhCn, ThemeMode::Nocturne) => "夜曲",
        (Locale::ZhCn, ThemeMode::Studio) => "工作室",
        (Locale::ZhCn, ThemeMode::Paper) => "纸感",
    }
}

pub fn quality_preset_label(locale: Locale, preset: QualityPreset) -> &'static str {
    match (locale, preset) {
        (Locale::EnUs, QualityPreset::Draft) => "Draft",
        (Locale::EnUs, QualityPreset::Balanced) => "Balanced",
        (Locale::EnUs, QualityPreset::Crisp) => "Crisp",
        (Locale::ZhCn, QualityPreset::Draft) => "草稿",
        (Locale::ZhCn, QualityPreset::Balanced) => "平衡",
        (Locale::ZhCn, QualityPreset::Crisp) => "精细",
    }
}

pub fn priority_items(locale: Locale) -> [&'static str; 3] {
    match locale {
        Locale::EnUs => ["Low", "Medium", "High"],
        Locale::ZhCn => ["低", "中", "高"],
    }
}

pub fn quality_preset_items(locale: Locale) -> [&'static str; 3] {
    match locale {
        Locale::EnUs => ["Draft", "Balanced", "Crisp"],
        Locale::ZhCn => ["草稿", "平衡", "精细"],
    }
}

pub fn theme_mode_items(locale: Locale) -> [&'static str; 3] {
    match locale {
        Locale::EnUs => ["Nocturne", "Studio", "Paper"],
        Locale::ZhCn => ["夜曲", "工作室", "纸感"],
    }
}

pub fn focus_mode_items(locale: Locale) -> [&'static str; 3] {
    match locale {
        Locale::EnUs => ["Balance", "Sprint", "Quiet"],
        Locale::ZhCn => ["平衡", "冲刺", "安静"],
    }
}

pub fn active_tasks_short(locale: Locale, count: usize) -> String {
    match locale {
        Locale::EnUs => format!("{count} active"),
        Locale::ZhCn => format!("{count} 个进行中"),
    }
}

pub fn urgent_tasks_short(locale: Locale, count: usize) -> String {
    match locale {
        Locale::EnUs => format!("{count} urgent"),
        Locale::ZhCn => format!("{count} 个紧急"),
    }
}

pub fn header_uptime(locale: Locale, uptime: &str) -> String {
    match locale {
        Locale::EnUs => format!("{uptime} uptime"),
        Locale::ZhCn => format!("运行 {uptime}"),
    }
}

pub fn header_frames(locale: Locale, frames: u64) -> String {
    match locale {
        Locale::EnUs => format!("{frames} frames"),
        Locale::ZhCn => format!("{frames} 帧"),
    }
}

pub fn frames_rendered(locale: Locale, frames: u64) -> String {
    match locale {
        Locale::EnUs => format!("{frames} frames rendered"),
        Locale::ZhCn => format!("已渲染 {frames} 帧"),
    }
}

pub fn tasks_active_badge(locale: Locale, count: usize) -> String {
    match locale {
        Locale::EnUs => format!("{count} active"),
        Locale::ZhCn => format!("{count} 个进行中"),
    }
}

pub fn tasks_done_badge(locale: Locale, count: usize) -> String {
    match locale {
        Locale::EnUs => format!("{count} wrapped"),
        Locale::ZhCn => format!("{count} 个已完成"),
    }
}

pub fn task_status_label(locale: Locale, done: bool) -> &'static str {
    match (locale, done) {
        (Locale::EnUs, true) => "Done",
        (Locale::EnUs, false) => "Open",
        (Locale::ZhCn, true) => "完成",
        (Locale::ZhCn, false) => "待办",
    }
}

pub fn task_action_label(locale: Locale, done: bool) -> &'static str {
    match (locale, done) {
        (Locale::EnUs, true) => "Reopen",
        (Locale::EnUs, false) => "Complete",
        (Locale::ZhCn, true) => "重新打开",
        (Locale::ZhCn, false) => "完成任务",
    }
}

pub fn task_meta(locale: Locale, urgent: bool) -> &'static str {
    match (locale, urgent) {
        (Locale::EnUs, true) => "Urgent · needs attention soon",
        (Locale::EnUs, false) => "Stable · can move with the regular lane",
        (Locale::ZhCn, true) => "紧急 · 需要尽快处理",
        (Locale::ZhCn, false) => "稳定 · 可以按常规节奏推进",
    }
}

pub fn ui_scale_text(locale: Locale, value: u32) -> String {
    match locale {
        Locale::EnUs => format!("UI scale · {value}%"),
        Locale::ZhCn => format!("界面缩放 · {value}%"),
    }
}

pub fn volume_text(locale: Locale, value: u32) -> String {
    match locale {
        Locale::EnUs => format!("Volume · {value}%"),
        Locale::ZhCn => format!("音量 · {value}%"),
    }
}

pub fn completion_text(locale: Locale, done: usize, total: usize) -> String {
    match locale {
        Locale::EnUs => format!("{done} of {total} tasks complete"),
        Locale::ZhCn => format!("共 {total} 项，已完成 {done} 项"),
    }
}

pub fn urgent_items_text(locale: Locale, count: usize) -> String {
    match locale {
        Locale::EnUs => format!("{count} urgent items"),
        Locale::ZhCn => format!("{count} 个紧急事项"),
    }
}

pub fn session_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Session",
        Locale::ZhCn => "会话",
    }
}

pub fn new_task_action(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "New Task",
        Locale::ZhCn => "新建任务",
    }
}

pub fn create_task_action(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Create Task",
        Locale::ZhCn => "创建任务",
    }
}

pub fn start_run_action(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Start Run",
        Locale::ZhCn => "开始运行",
    }
}

pub fn pause_action(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Pause",
        Locale::ZhCn => "暂停",
    }
}

pub fn ship_build_action(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Ship Build",
        Locale::ZhCn => "发布构建",
    }
}

pub fn active_tasks_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Active tasks",
        Locale::ZhCn => "进行中的任务",
    }
}

pub fn active_tasks_meta(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Work not yet wrapped",
        Locale::ZhCn => "尚未收尾的工作",
    }
}

pub fn completion_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Completion",
        Locale::ZhCn => "完成度",
    }
}

pub fn completion_meta(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Queue completion ratio",
        Locale::ZhCn => "当前队列完成比例",
    }
}

pub fn focus_mode_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Focus mode",
        Locale::ZhCn => "专注模式",
    }
}

pub fn focus_mode_meta(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Intentional workspace pacing",
        Locale::ZhCn => "为工作节奏设定明确步调",
    }
}

pub fn launchpad_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Launchpad",
        Locale::ZhCn => "启动台",
    }
}

pub fn launchpad_subtitle(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Fast actions, clean status, and a live read on throughput.",
        Locale::ZhCn => "把快捷操作、清晰状态和实时吞吐量放在同一块面板里。",
    }
}

pub fn next_up_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Next up",
        Locale::ZhCn => "接下来",
    }
}

pub fn next_up_empty(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "The queue is beautifully empty.",
        Locale::ZhCn => "队列现在很干净，没有待推进的事项。",
    }
}

pub fn atmosphere_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Atmosphere",
        Locale::ZhCn => "氛围",
    }
}

pub fn atmosphere_summary(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "A soft, low-noise shell with deliberate contrast.",
        Locale::ZhCn => "柔和、低噪点、但层次分明的界面外壳。",
    }
}

pub fn queue_texture_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Queue texture",
        Locale::ZhCn => "队列质感",
    }
}

pub fn queue_texture_subtitle(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "The shape of work matters more than the count.",
        Locale::ZhCn => "工作的形态，有时比数量更重要。",
    }
}

pub fn queue_texture_meta(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Urgent work stays visible without flooding the rest of the board.",
        Locale::ZhCn => "让紧急任务保持可见，但不会淹没整块看板的其余内容。",
    }
}

pub fn preferences_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Preferences",
        Locale::ZhCn => "偏好",
    }
}

pub fn preferences_subtitle(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Small inputs that make the whole shell feel calmer.",
        Locale::ZhCn => "一些细小输入，会让整个工作台更安定。",
    }
}

pub fn preferences_meta(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Use Settings to tune palette, volume, and pacing without leaving the app.",
        Locale::ZhCn => "你可以在设置里调整配色、音量和节奏，而不用离开当前界面。",
    }
}

pub fn notifications_badge(locale: Locale, enabled: bool) -> &'static str {
    match (locale, enabled) {
        (Locale::EnUs, true) => "Notifications on",
        (Locale::EnUs, false) => "Notifications off",
        (Locale::ZhCn, true) => "通知已开启",
        (Locale::ZhCn, false) => "通知已关闭",
    }
}

pub fn task_queue_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Task Queue",
        Locale::ZhCn => "任务队列",
    }
}

pub fn task_queue_subtitle(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Tight cards, visible priority, and no hidden state.",
        Locale::ZhCn => "卡片紧凑、优先级明确、状态不隐藏。",
    }
}

pub fn live_queue_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Live queue",
        Locale::ZhCn => "实时队列",
    }
}

pub fn live_queue_subtitle(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Each card can be resolved in place.",
        Locale::ZhCn => "每张卡片都能在当前位置直接处理。",
    }
}

pub fn workspace_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Workspace",
        Locale::ZhCn => "工作区",
    }
}

pub fn workspace_subtitle(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Naming and fidelity are part of the tone.",
        Locale::ZhCn => "命名和呈现精度，本身就是体验调性的一部分。",
    }
}

pub fn project_name_placeholder(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Project name",
        Locale::ZhCn => "项目名称",
    }
}

pub fn quality_preset_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Quality preset",
        Locale::ZhCn => "质量预设",
    }
}

pub fn language_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Language",
        Locale::ZhCn => "语言",
    }
}

pub fn language_items(locale: Locale) -> [&'static str; 2] {
    match locale {
        Locale::EnUs => ["English", "Simplified Chinese"],
        Locale::ZhCn => ["English", "简体中文"],
    }
}

pub fn sound_scale_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Sound & Scale",
        Locale::ZhCn => "声音与缩放",
    }
}

pub fn sound_scale_subtitle(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Two soft dials for how dense the shell should feel.",
        Locale::ZhCn => "用两个柔和旋钮决定界面的密度和氛围。",
    }
}

pub fn preferences_settings_subtitle(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Toggles stay explicit, with no hidden dependencies.",
        Locale::ZhCn => "所有开关都保持显性，不制造隐藏依赖。",
    }
}

pub fn notifications_label(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Notifications",
        Locale::ZhCn => "通知",
    }
}

pub fn auto_save_label(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Auto save snapshots",
        Locale::ZhCn => "自动保存快照",
    }
}

pub fn preferences_settings_meta(
    locale: Locale,
    notifications_enabled: bool,
    auto_save: bool,
) -> &'static str {
    match (locale, notifications_enabled, auto_save) {
        (Locale::EnUs, true, true) => "The app will stay talkative and preserve intermediate work.",
        (Locale::EnUs, true, false) => {
            "Feedback remains visible, but background persistence is lighter."
        }
        (Locale::EnUs, false, _) => "The shell is intentionally quiet until you ask for something.",
        (Locale::ZhCn, true, true) => "应用会保持反馈活跃，并保留中间过程。",
        (Locale::ZhCn, true, false) => "反馈依然清晰可见，但后台持久化会更轻一些。",
        (Locale::ZhCn, false, _) => "界面会刻意保持安静，直到你主动触发它。",
    }
}

pub fn appearance_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Appearance",
        Locale::ZhCn => "外观",
    }
}

pub fn appearance_subtitle(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Palette and pacing are adjustable without breaking composition.",
        Locale::ZhCn => "你可以调整配色和节奏，而不会破坏整体构图。",
    }
}

pub fn appearance_caption(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => {
            "Theme changes re-skin the shell immediately while keeping layout and motion stable."
        }
        Locale::ZhCn => "切换主题会立即改变皮肤，但保持布局和动效的稳定性。",
    }
}

pub fn task_sheet_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Create Task",
        Locale::ZhCn => "创建任务",
    }
}

pub fn task_sheet_subtitle(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Keep the input compact, focused, and obvious.",
        Locale::ZhCn => "让输入保持紧凑、聚焦，而且一眼就能理解。",
    }
}

pub fn task_title_placeholder(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Write a concise task title",
        Locale::ZhCn => "写一个简洁明确的任务标题",
    }
}

pub fn mark_urgent_label(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Mark as urgent",
        Locale::ZhCn => "标记为紧急",
    }
}

pub fn cancel_action(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Cancel",
        Locale::ZhCn => "取消",
    }
}

pub fn create_action(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Create",
        Locale::ZhCn => "创建",
    }
}

pub fn ship_dialog_title(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Ship the current build?",
        Locale::ZhCn => "要发布当前构建吗？",
    }
}

pub fn ship_dialog_message(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "This confirms the release pass and moves the console into shipping mode.",
        Locale::ZhCn => "这会确认当前发版流程，并让控制台进入发布模式。",
    }
}

pub fn ship_dialog_primary(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Ship It",
        Locale::ZhCn => "立即发布",
    }
}

pub fn ship_dialog_secondary(locale: Locale) -> &'static str {
    match locale {
        Locale::EnUs => "Not Yet",
        Locale::ZhCn => "稍后再说",
    }
}

pub fn queue_clear_toast(locale: Locale) -> (&'static str, &'static str) {
    match locale {
        Locale::EnUs => ("Queue is clear", "Nothing is waiting in the active lane."),
        Locale::ZhCn => ("队列已清空", "当前活跃通道里没有待处理事项。"),
    }
}

pub fn run_paused_toast(locale: Locale) -> (&'static str, &'static str) {
    match locale {
        Locale::EnUs => (
            "Run paused",
            "The queue is frozen without losing local state.",
        ),
        Locale::ZhCn => ("运行已暂停", "队列已冻结，但本地状态不会丢失。"),
    }
}

pub fn build_queued_toast(locale: Locale) -> (&'static str, &'static str) {
    match locale {
        Locale::EnUs => ("Build queued", "The release package is staged for handoff."),
        Locale::ZhCn => ("构建已排队", "发版包已经准备好进入交付阶段。"),
    }
}

pub fn name_required_toast(locale: Locale) -> (&'static str, &'static str) {
    match locale {
        Locale::EnUs => (
            "Name required",
            "Give the task a concise title before creating it.",
        ),
        Locale::ZhCn => ("需要名称", "创建前先给这个任务一个简洁明确的标题。"),
    }
}

pub fn create_task_toast(locale: Locale, title: &str) -> (String, String) {
    match locale {
        Locale::EnUs => (
            "Task created".to_string(),
            format!("{title} joined the active queue."),
        ),
        Locale::ZhCn => (
            "已创建任务".to_string(),
            format!("{title} 已加入当前队列。"),
        ),
    }
}

pub fn run_advanced_toast(locale: Locale, title: &str) -> (String, String) {
    match locale {
        Locale::EnUs => (
            "Run advanced".to_string(),
            format!("{title} moved across the line."),
        ),
        Locale::ZhCn => (
            "推进成功".to_string(),
            format!("{title} 已推进到下一阶段。"),
        ),
    }
}

pub fn task_completed_toast(locale: Locale, title: &str) -> (String, String) {
    match locale {
        Locale::EnUs => ("Task completed".to_string(), format!("{title} is wrapped.")),
        Locale::ZhCn => ("任务完成".to_string(), format!("{title} 已处理完成。")),
    }
}

pub fn task_reopened_toast(locale: Locale, title: &str) -> (String, String) {
    match locale {
        Locale::EnUs => (
            "Task reopened".to_string(),
            format!("{title} is back in play."),
        ),
        Locale::ZhCn => (
            "任务重新打开".to_string(),
            format!("{title} 已重新回到队列中。"),
        ),
    }
}
