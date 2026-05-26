//! AI chat application mockup built with `ui-neo`.
//!
//! ```bash
//! cargo run --example ui_neo_ai_chat --features ui-neo --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::{
    CameraMarker, MainCamera, Projection, RenderPipelineAsset, RenderSettings, SpriteFeature,
    Transform, TransparentPhase,
};
use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::widgets::theme;
use sky_engine::ui::neo::{Align, Color, HorizontalAlign, NeoState, Size, Ui, VerticalAlign};

const WINDOW_W: u32 = 1280;
const WINDOW_H: u32 = 800;
const SHELL_MAX_W: f32 = 1180.0;
const SHELL_MAX_H: f32 = 720.0;
const SHELL_RADIUS: f32 = 28.0;
const TOP_BAR_H: f32 = 76.0;
const COMPOSER_H: f32 = 118.0;

struct NeoAiChatDemo {
    state: NeoState<ChatState>,
    screenshot: ScreenshotProbe,
}

impl Default for NeoAiChatDemo {
    fn default() -> Self {
        Self {
            state: NeoState::new(ChatState::default()),
            screenshot: ScreenshotProbe::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Assistant,
    User,
}

#[derive(Debug, Clone)]
struct ChatMessage {
    role: Role,
    author: String,
    time: String,
    text: String,
}

impl ChatMessage {
    fn assistant(time: &str, text: &str) -> Self {
        Self {
            role: Role::Assistant,
            author: "Sky AI".to_string(),
            time: time.to_string(),
            text: text.to_string(),
        }
    }

    fn user(time: &str, text: &str) -> Self {
        Self {
            role: Role::User,
            author: "You".to_string(),
            time: time.to_string(),
            text: text.to_string(),
        }
    }
}

#[derive(Debug)]
struct ChatState {
    selected_thread: usize,
    draft: String,
    chat_scroll: f32,
    messages: Vec<ChatMessage>,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            selected_thread: 0,
            draft: "Can you sketch a clean UI state model?".to_string(),
            chat_scroll: 0.0,
            messages: seed_messages(0),
        }
    }
}

impl ChatState {
    fn select_thread(&mut self, index: usize) {
        if self.selected_thread == index {
            return;
        }
        self.selected_thread = index;
        self.messages = seed_messages(index);
        self.draft.clear();
        self.chat_scroll = 0.0;
    }

    fn send_draft(&mut self) {
        let text = self.draft.trim().to_string();
        if text.is_empty() {
            return;
        }

        let time = next_time(self.messages.len());
        self.messages.push(ChatMessage::user(time, &text));
        let reply = assistant_reply(&text);
        self.messages
            .push(ChatMessage::assistant("now", reply.as_str()));
        self.draft.clear();
        self.chat_scroll = 99999.0;
    }
}

#[derive(Debug, Clone)]
struct ChatSnapshot {
    selected_thread: usize,
    draft: String,
    messages: Vec<ChatMessage>,
}

impl From<&ChatState> for ChatSnapshot {
    fn from(value: &ChatState) -> Self {
        Self {
            selected_thread: value.selected_thread,
            draft: value.draft.clone(),
            messages: value.messages.clone(),
        }
    }
}

impl AppState for NeoAiChatDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: c(0.035, 0.043, 0.052, 1.0).into(),
            ..Default::default()
        });
        ctx.world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::orthographic(WINDOW_H as f32),
            MainCamera,
        ));
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        let state = self.state.clone();
        let snapshot = self.state.read(|state| ChatSnapshot::from(state));
        sky_engine::ui::neo::compose(ctx, move |ui, screen| {
            draw_chat_app(ui, screen.width, screen.height, &state, &snapshot);
        });

        ctx.render();
        ctx.ui().render_overlays();
        self.screenshot.update(ctx);
        ctx.request_redraw();
    }
}

fn draw_chat_app(
    ui: &mut Ui,
    screen_width: f32,
    screen_height: f32,
    state: &NeoState<ChatState>,
    snapshot: &ChatSnapshot,
) {
    let shell_w = (screen_width - 56.0).clamp(760.0, SHELL_MAX_W);
    let shell_h = (screen_height - 56.0).clamp(560.0, SHELL_MAX_H);
    let sidebar_w = if shell_w < 930.0 { 238.0 } else { 286.0 };
    let main_w = shell_w - sidebar_w;
    let bubble_w = (main_w - 150.0).clamp(300.0, 610.0);

    ui.rect("background")
        .size(screen_width, screen_height)
        .gradient(c(0.038, 0.046, 0.055, 1.0), c(0.060, 0.070, 0.078, 1.0))
        .build();

    draw_background_grid(ui, screen_width, screen_height);

    ui.stack("stage")
        .size(screen_width, screen_height)
        .padding(28.0)
        .align(Align::Center, Align::Center)
        .content(|ui| {
            ui.stack("chat.shell")
                .size(shell_w, shell_h)
                .rounded_clip(SHELL_RADIUS)
                .content(|ui| {
                    ui.rect("chat.shell.bg")
                        .fill()
                        .radius(SHELL_RADIUS)
                        .gradient(c(0.100, 0.116, 0.128, 0.98), c(0.060, 0.070, 0.080, 0.99))
                        .border(1.0, c(0.520, 0.650, 0.700, 0.22))
                        .shadow(44.0, 0.0, 18.0, c(0.0, 0.0, 0.0, 0.36))
                        .build();

                    ui.row("chat.layout").fill().content(|ui| {
                        draw_sidebar(ui, sidebar_w, shell_h, state, snapshot);
                        draw_main_panel(ui, main_w, shell_h, bubble_w, state, snapshot);
                    });
                });
        });
}

fn draw_background_grid(ui: &mut Ui, screen_width: f32, screen_height: f32) {
    ui.stack("background.grid")
        .size(screen_width, screen_height)
        .opacity(0.32)
        .content(|ui| {
            let mut x = 0.0;
            let mut index = 0;
            while x < screen_width {
                ui.rect(format!("grid.v.{index}"))
                    .position(x, 0.0)
                    .size(1.0, screen_height)
                    .color(c(0.180, 0.260, 0.300, 0.12))
                    .build();
                x += 64.0;
                index += 1;
            }

            let mut y = 0.0;
            let mut row = 0;
            while y < screen_height {
                ui.rect(format!("grid.h.{row}"))
                    .position(0.0, y)
                    .size(screen_width, 1.0)
                    .color(c(0.180, 0.260, 0.300, 0.10))
                    .build();
                y += 64.0;
                row += 1;
            }
        });
}

fn draw_sidebar(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &NeoState<ChatState>,
    snapshot: &ChatSnapshot,
) {
    ui.stack("sidebar").size(width, Size::fill()).content(|ui| {
        ui.rect("sidebar.bg")
            .fill()
            .gradient(c(0.075, 0.092, 0.102, 0.98), c(0.048, 0.058, 0.066, 0.98))
            .build();

        ui.column("sidebar.content")
            .fill()
            .padding(18.0)
            .gap(14.0)
            .content(|ui| {
                draw_brand(ui);
                draw_new_chat_button(ui, state);

                ui.text("sidebar.section")
                    .size(Size::fill(), 20.0)
                    .text("Recent chats")
                    .font_size(13.0)
                    .font_weight(650)
                    .line_height(20.0)
                    .color(c(0.560, 0.670, 0.710, 1.0))
                    .build();

                ui.column("thread.list")
                    .size(Size::fill(), (height - 306.0).max(300.0))
                    .gap(10.0)
                    .content(|ui| {
                        for index in 0..4 {
                            draw_thread_item(ui, state, index, snapshot.selected_thread == index);
                        }
                    });

                draw_usage_card(ui);
            });
    });
}

fn draw_brand(ui: &mut Ui) {
    ui.row("brand")
        .size(Size::fill(), 52.0)
        .gap(12.0)
        .align_items(Align::Center)
        .content(|ui| {
            ui.stack("brand.mark").size(42.0, 42.0).content(|ui| {
                ui.rect("brand.mark.bg")
                    .fill()
                    .radius(14.0)
                    .gradient(c(0.230, 0.600, 0.500, 1.0), c(0.180, 0.360, 0.660, 1.0))
                    .build();
                ui.text("brand.mark.text")
                    .fill()
                    .text("AI")
                    .font_size(17.0)
                    .font_weight(760)
                    .line_height(42.0)
                    .horizontal_align(HorizontalAlign::Center)
                    .vertical_align(VerticalAlign::Center)
                    .color(c(0.970, 1.000, 0.980, 1.0))
                    .build();
            });

            ui.column("brand.copy")
                .size(0.0, 46.0)
                .grow(1.0)
                .justify_content(Align::Center)
                .content(|ui| {
                    ui.text("brand.title")
                        .size(Size::fill(), 24.0)
                        .text("Sky Chat")
                        .font_size(20.0)
                        .font_weight(720)
                        .line_height(24.0)
                        .color(c(0.930, 0.965, 0.970, 1.0))
                        .build();
                    ui.text("brand.subtitle")
                        .size(Size::fill(), 18.0)
                        .text("local UI mockup")
                        .font_size(12.0)
                        .line_height(18.0)
                        .color(c(0.540, 0.640, 0.690, 1.0))
                        .build();
                });
        });
}

fn draw_new_chat_button(ui: &mut Ui, state: &NeoState<ChatState>) {
    let click_state = state.clone();
    widgets::button(ui, "new.chat")
        .size(Size::fill(), 44.0)
        .text("New chat")
        .font_size(15.0)
        .style(secondary_button_style())
        .on_click(move || {
            click_state.update(|state| {
                state.messages = seed_messages(0);
                state.selected_thread = 0;
                state.draft.clear();
                state.chat_scroll = 0.0;
            });
        })
        .build();
}

fn draw_thread_item(ui: &mut Ui, state: &NeoState<ChatState>, index: usize, active: bool) {
    let (title, subtitle, tone) = thread_info(index);
    let click_state = state.clone();
    let normal = if active {
        c(0.110, 0.185, 0.185, 0.98)
    } else {
        c(0.090, 0.104, 0.112, 0.58)
    };
    let hover = if active {
        c(0.130, 0.220, 0.210, 1.0)
    } else {
        c(0.120, 0.140, 0.150, 0.86)
    };

    ui.stack(format!("thread.{index}"))
        .size(Size::fill(), 70.0)
        .content(|ui| {
            ui.rect(format!("thread.{index}.bg"))
                .fill()
                .states(normal, hover, c(0.070, 0.100, 0.110, 1.0))
                .radius(16.0)
                .border(
                    1.0,
                    if active {
                        c(0.300, 0.740, 0.650, 0.46)
                    } else {
                        c(0.420, 0.500, 0.540, 0.16)
                    },
                )
                .on_click(move || click_state.update(|state| state.select_thread(index)))
                .build();

            ui.row(format!("thread.{index}.content"))
                .fill()
                .padding(12.0)
                .gap(10.0)
                .align_items(Align::Center)
                .content(|ui| {
                    ui.rect(format!("thread.{index}.dot"))
                        .size(10.0, 10.0)
                        .radius(5.0)
                        .color(tone)
                        .build();
                    ui.column(format!("thread.{index}.copy"))
                        .size(0.0, 46.0)
                        .grow(1.0)
                        .justify_content(Align::Center)
                        .content(|ui| {
                            ui.text(format!("thread.{index}.title"))
                                .size(Size::fill(), 22.0)
                                .text(title)
                                .font_size(14.0)
                                .font_weight(650)
                                .line_height(22.0)
                                .color(c(0.900, 0.940, 0.950, 1.0))
                                .build();
                            ui.text(format!("thread.{index}.subtitle"))
                                .size(Size::fill(), 18.0)
                                .text(subtitle)
                                .font_size(12.0)
                                .line_height(18.0)
                                .color(c(0.540, 0.635, 0.690, 1.0))
                                .build();
                        });
                });
        });
}

fn draw_usage_card(ui: &mut Ui) {
    ui.stack("usage").size(Size::fill(), 78.0).content(|ui| {
        ui.rect("usage.bg")
            .fill()
            .radius(18.0)
            .color(c(0.055, 0.068, 0.076, 0.92))
            .border(1.0, c(0.360, 0.500, 0.500, 0.20))
            .build();
        ui.column("usage.content")
            .fill()
            .padding(13.0)
            .gap(8.0)
            .content(|ui| {
                ui.row("usage.top")
                    .size(Size::fill(), 20.0)
                    .align_items(Align::Center)
                    .content(|ui| {
                        ui.text("usage.label")
                            .size(0.0, 20.0)
                            .grow(1.0)
                            .text("Context")
                            .font_size(13.0)
                            .line_height(20.0)
                            .color(c(0.720, 0.800, 0.810, 1.0))
                            .build();
                        ui.text("usage.value")
                            .size(54.0, 20.0)
                            .text("64%")
                            .font_size(13.0)
                            .font_weight(700)
                            .line_height(20.0)
                            .horizontal_align(HorizontalAlign::Right)
                            .color(c(0.480, 0.830, 0.700, 1.0))
                            .build();
                    });

                ui.stack("usage.bar").size(Size::fill(), 8.0).content(|ui| {
                    ui.rect("usage.track")
                        .fill()
                        .radius(4.0)
                        .color(c(0.110, 0.135, 0.145, 1.0))
                        .build();
                    ui.rect("usage.fill")
                        .size(132.0, 8.0)
                        .radius(4.0)
                        .color(c(0.300, 0.780, 0.640, 1.0))
                        .build();
                });
            });
    });
}

fn draw_main_panel(
    ui: &mut Ui,
    main_w: f32,
    shell_h: f32,
    bubble_w: f32,
    state: &NeoState<ChatState>,
    snapshot: &ChatSnapshot,
) {
    let messages_h = (shell_h - TOP_BAR_H - COMPOSER_H).max(260.0);
    ui.column("main")
        .size(Size::fill(), Size::fill())
        .grow(1.0)
        .content(|ui| {
            draw_top_bar(ui, main_w, snapshot);
            draw_message_stream(ui, main_w, messages_h, bubble_w, state, snapshot);
            draw_composer(ui, main_w, state, snapshot);
        });
}

fn draw_top_bar(ui: &mut Ui, main_w: f32, snapshot: &ChatSnapshot) {
    let (title, subtitle, tone) = thread_info(snapshot.selected_thread);
    let copy_w = (main_w - 266.0).max(220.0);
    ui.stack("topbar")
        .size(Size::fill(), TOP_BAR_H)
        .content(|ui| {
            ui.rect("topbar.bg")
                .fill()
                .color(c(0.070, 0.084, 0.092, 0.66))
                .border(1.0, c(0.420, 0.500, 0.530, 0.12))
                .build();
            ui.row("topbar.content")
                .fill()
                .padding_xy(24.0, 14.0)
                .gap(14.0)
                .align_items(Align::Center)
                .content(|ui| {
                    ui.rect("topbar.status")
                        .size(12.0, 12.0)
                        .radius(6.0)
                        .color(tone)
                        .shadow(18.0, 0.0, 0.0, tone)
                        .build();
                    ui.column("topbar.copy")
                        .size(copy_w, 48.0)
                        .justify_content(Align::Center)
                        .content(|ui| {
                            ui.text("topbar.title")
                                .size(Size::fill(), 26.0)
                                .text(title)
                                .font_size(20.0)
                                .font_weight(720)
                                .line_height(26.0)
                                .color(c(0.940, 0.970, 0.975, 1.0))
                                .build();
                            ui.text("topbar.subtitle")
                                .size(Size::fill(), 18.0)
                                .text(subtitle)
                                .font_size(13.0)
                                .line_height(18.0)
                                .color(c(0.580, 0.680, 0.720, 1.0))
                                .build();
                        });
                    draw_model_pill(ui, "topbar.model", "GPT-5", c(0.280, 0.580, 0.940, 1.0));
                    draw_model_pill(ui, "topbar.mode", "fast", c(0.860, 0.620, 0.330, 1.0));
                });
        });
}

fn draw_model_pill(ui: &mut Ui, id: &str, label: &str, color: Color) {
    ui.stack(id.to_string()).size(82.0, 34.0).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .fill()
            .radius(17.0)
            .color(c(0.095, 0.112, 0.120, 0.92))
            .border(1.0, color.with_alpha(0.38))
            .build();
        ui.text(format!("{id}.text"))
            .fill()
            .text(label)
            .font_size(13.0)
            .font_weight(680)
            .line_height(34.0)
            .horizontal_align(HorizontalAlign::Center)
            .vertical_align(VerticalAlign::Center)
            .color(color.with_alpha(0.96))
            .build();
    });
}

fn draw_message_stream(
    ui: &mut Ui,
    main_w: f32,
    height: f32,
    bubble_w: f32,
    state: &NeoState<ChatState>,
    snapshot: &ChatSnapshot,
) {
    let scroll = state.bind(
        |state| state.chat_scroll,
        |state, value| state.chat_scroll = value.max(0.0),
    );

    ui.stack("messages")
        .size(Size::fill(), height)
        .content(|ui| {
            ui.rect("messages.bg")
                .fill()
                .color(c(0.054, 0.064, 0.070, 0.48))
                .build();

            ui.scroll_y("messages.scroll")
                .fill()
                .inset_xy(18.0, 14.0)
                .padding_xy(6.0, 8.0)
                .gap(14.0)
                .scrollbar_gap(10.0)
                .offset_bind(scroll)
                .content(|ui| {
                    draw_day_divider(ui);
                    for (index, message) in snapshot.messages.iter().enumerate() {
                        draw_message_row(ui, index, message, main_w, bubble_w);
                    }
                    draw_typing_preview(ui, bubble_w);
                });
        });
}

fn draw_day_divider(ui: &mut Ui) {
    ui.row("messages.day")
        .size(Size::fill(), 30.0)
        .align_items(Align::Center)
        .gap(12.0)
        .content(|ui| {
            ui.rect("messages.day.left")
                .size(0.0, 1.0)
                .grow(1.0)
                .color(c(0.360, 0.440, 0.460, 0.18))
                .build();
            ui.text("messages.day.text")
                .size(92.0, 22.0)
                .text("Today")
                .font_size(12.0)
                .line_height(22.0)
                .horizontal_align(HorizontalAlign::Center)
                .color(c(0.560, 0.640, 0.680, 0.92))
                .build();
            ui.rect("messages.day.right")
                .size(0.0, 1.0)
                .grow(1.0)
                .color(c(0.360, 0.440, 0.460, 0.18))
                .build();
        });
}

fn draw_message_row(ui: &mut Ui, index: usize, message: &ChatMessage, main_w: f32, bubble_w: f32) {
    let user_bubble_w = bubble_w.min(460.0);
    let row_bubble_w = if message.role == Role::User {
        user_bubble_w
    } else {
        bubble_w
    };
    let bubble_h = bubble_height(&message.text, row_bubble_w);
    let row_h = bubble_h + 6.0;
    let id = format!("message.{index}");

    ui.stack(id.clone())
        .size(Size::fill(), row_h)
        .content(|ui| match message.role {
            Role::Assistant => {
                draw_avatar(
                    ui,
                    format!("{id}.avatar"),
                    "AI",
                    c(0.240, 0.660, 0.560, 1.0),
                );
                ui.stack(format!("{id}.bubble.slot"))
                    .position(44.0, 0.0)
                    .size(row_bubble_w, bubble_h)
                    .content(|ui| {
                        draw_bubble(ui, format!("{id}.bubble"), message, row_bubble_w, bubble_h);
                    });
            }
            Role::User => {
                let x = (main_w - row_bubble_w - 88.0).max(44.0);
                ui.stack(format!("{id}.bubble.slot"))
                    .position(x, 0.0)
                    .size(row_bubble_w, bubble_h)
                    .content(|ui| {
                        draw_bubble(ui, format!("{id}.bubble"), message, row_bubble_w, bubble_h);
                    });
            }
        });
}

fn draw_avatar(ui: &mut Ui, id: String, label: &str, color: Color) {
    ui.stack(id.clone()).size(34.0, 34.0).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .fill()
            .radius(17.0)
            .color(color)
            .shadow(12.0, 0.0, 4.0, color.with_alpha(0.26))
            .build();
        ui.text(format!("{id}.text"))
            .fill()
            .text(label)
            .font_size(12.0)
            .font_weight(760)
            .line_height(34.0)
            .horizontal_align(HorizontalAlign::Center)
            .vertical_align(VerticalAlign::Center)
            .color(c(0.970, 0.990, 1.0, 1.0))
            .build();
    });
}

fn draw_bubble(ui: &mut Ui, id: String, message: &ChatMessage, width: f32, height: f32) {
    let is_user = message.role == Role::User;
    let bg = if is_user {
        c(0.180, 0.350, 0.610, 0.98)
    } else {
        c(0.096, 0.118, 0.126, 0.98)
    };
    let border = if is_user {
        c(0.470, 0.680, 0.950, 0.30)
    } else {
        c(0.370, 0.520, 0.500, 0.24)
    };

    ui.stack(id.clone())
        .size(width, height)
        .rounded_clip(20.0)
        .content(|ui| {
            ui.rect(format!("{id}.bg"))
                .fill()
                .radius(20.0)
                .color(bg)
                .border(1.0, border)
                .build();
            ui.column(format!("{id}.content"))
                .fill()
                .padding_xy(18.0, 13.0)
                .gap(6.0)
                .content(|ui| {
                    ui.row(format!("{id}.meta"))
                        .size(Size::fill(), 20.0)
                        .align_items(Align::Center)
                        .content(|ui| {
                            ui.text(format!("{id}.author"))
                                .size(Size::fill(), 20.0)
                                .text(message.author.clone())
                                .font_size(12.0)
                                .font_weight(760)
                                .line_height(20.0)
                                .color(if is_user {
                                    c(0.880, 0.940, 1.0, 1.0)
                                } else {
                                    c(0.700, 0.860, 0.800, 1.0)
                                })
                                .build();
                            ui.text(format!("{id}.time"))
                                .size(64.0, 20.0)
                                .text(message.time.clone())
                                .font_size(11.0)
                                .line_height(20.0)
                                .horizontal_align(HorizontalAlign::Right)
                                .color(c(0.680, 0.760, 0.790, 0.72))
                                .build();
                        });

                    ui.text(format!("{id}.text"))
                        .size(Size::fill(), height - 45.0)
                        .text(message.text.clone())
                        .font_size(14.0)
                        .line_height(20.0)
                        .wrap(true)
                        .max_width(width - 36.0)
                        .color(c(0.930, 0.960, 0.965, 1.0))
                        .build();
                });
        });
}

fn draw_typing_preview(ui: &mut Ui, bubble_w: f32) {
    ui.row("typing")
        .size(Size::fill(), 48.0)
        .gap(10.0)
        .align_items(Align::Center)
        .content(|ui| {
            draw_avatar(
                ui,
                "typing.avatar".to_string(),
                "AI",
                c(0.240, 0.660, 0.560, 1.0),
            );
            ui.stack("typing.bubble")
                .size(bubble_w.min(260.0), 42.0)
                .rounded_clip(18.0)
                .content(|ui| {
                    ui.rect("typing.bg")
                        .fill()
                        .radius(18.0)
                        .color(c(0.075, 0.092, 0.100, 0.74))
                        .border(1.0, c(0.310, 0.480, 0.470, 0.18))
                        .build();
                    ui.row("typing.dots")
                        .fill()
                        .padding_xy(18.0, 0.0)
                        .gap(8.0)
                        .align_items(Align::Center)
                        .content(|ui| {
                            for index in 0..3 {
                                ui.rect(format!("typing.dot.{index}"))
                                    .size(7.0, 7.0)
                                    .radius(4.0)
                                    .color(c(0.460, 0.760, 0.680, 0.84 - index as f32 * 0.18))
                                    .build();
                            }
                        });
                });
            ui.stack("typing.push").size(0.0, 48.0).grow(1.0).build();
        });
}

fn draw_composer(ui: &mut Ui, main_w: f32, state: &NeoState<ChatState>, snapshot: &ChatSnapshot) {
    let draft = state.bind(
        |state| state.draft.clone(),
        |state, value| state.draft = value,
    );
    let send_state = state.clone();
    let enter_state = state.clone();
    let input_w = (main_w - 168.0).max(260.0);

    ui.stack("composer")
        .size(Size::fill(), COMPOSER_H)
        .content(|ui| {
            ui.rect("composer.bg")
                .fill()
                .color(c(0.065, 0.076, 0.084, 0.88))
                .border(1.0, c(0.420, 0.500, 0.530, 0.14))
                .build();

            ui.column("composer.content")
                .fill()
                .padding_each(22.0, 14.0, 22.0, 18.0)
                .gap(10.0)
                .content(|ui| {
                    draw_prompt_chips(ui, state);

                    ui.row("composer.row")
                        .size(Size::fill(), 54.0)
                        .gap(12.0)
                        .align_items(Align::Center)
                        .content(|ui| {
                            widgets::input(ui, "composer.input")
                                .size(input_w, 54.0)
                                .placeholder("Message Sky AI")
                                .font_size(16.0)
                                .inset(16.0)
                                .style(input_style())
                                .text_bind(draft)
                                .on_enter(move || enter_state.update(ChatState::send_draft))
                                .build();

                            widgets::button(ui, "composer.send")
                                .size(112.0, 54.0)
                                .text("Send")
                                .font_size(16.0)
                                .style(primary_button_style())
                                .disabled(snapshot.draft.trim().is_empty())
                                .on_click(move || send_state.update(ChatState::send_draft))
                                .build();
                        });
                });
        });
}

fn draw_prompt_chips(ui: &mut Ui, state: &NeoState<ChatState>) {
    ui.row("prompt.chips")
        .size(Size::fill(), 22.0)
        .gap(8.0)
        .content(|ui| {
            prompt_chip(ui, state, 0, "Refine layout");
            prompt_chip(ui, state, 1, "Summarize state");
            prompt_chip(ui, state, 2, "Find edge cases");
            ui.stack("prompt.push").size(0.0, 22.0).grow(1.0).build();
        });
}

fn prompt_chip(ui: &mut Ui, state: &NeoState<ChatState>, index: usize, label: &'static str) {
    let click_state = state.clone();
    ui.stack(format!("prompt.{index}"))
        .size(124.0, 22.0)
        .content(|ui| {
            ui.rect(format!("prompt.{index}.bg"))
                .fill()
                .states(
                    c(0.092, 0.110, 0.118, 0.88),
                    c(0.120, 0.150, 0.155, 0.96),
                    c(0.065, 0.084, 0.090, 1.0),
                )
                .radius(11.0)
                .border(1.0, c(0.360, 0.480, 0.480, 0.22))
                .on_click(move || {
                    click_state.update(|state| {
                        state.draft = match index {
                            0 => "Refine this layout into reusable components.".to_string(),
                            1 => "Summarize the state model and event flow.".to_string(),
                            _ => "Find edge cases in scrolling and popovers.".to_string(),
                        };
                    });
                })
                .build();
            ui.text(format!("prompt.{index}.text"))
                .fill()
                .text(label)
                .font_size(11.0)
                .line_height(22.0)
                .horizontal_align(HorizontalAlign::Center)
                .vertical_align(VerticalAlign::Center)
                .color(c(0.660, 0.760, 0.770, 1.0))
                .build();
        });
}

fn thread_info(index: usize) -> (&'static str, &'static str, Color) {
    match index {
        1 => (
            "Code review",
            "layout and tests",
            c(0.350, 0.620, 0.950, 1.0),
        ),
        2 => ("Design notes", "API shape", c(0.900, 0.600, 0.360, 1.0)),
        3 => (
            "Release draft",
            "docs and polish",
            c(0.760, 0.460, 0.860, 1.0),
        ),
        _ => (
            "UI infrastructure",
            "scroll, clip, popover",
            c(0.320, 0.820, 0.660, 1.0),
        ),
    }
}

fn seed_messages(index: usize) -> Vec<ChatMessage> {
    match index {
        1 => vec![
            ChatMessage::assistant(
                "10:12",
                "I checked the renderer path. The main risk is when layout data and visual clipping drift apart.",
            ),
            ChatMessage::user(
                "10:13",
                "What should the review focus on?",
            ),
            ChatMessage::assistant(
                "10:13",
                "Start with hit testing, draw-list ordering, screenshot coverage, and any example that manually recreates scroll math.",
            ),
        ],
        2 => vec![
            ChatMessage::assistant(
                "09:44",
                "A good API should make the common safe path shorter than the risky path.",
            ),
            ChatMessage::user("09:45", "So scroll_y should own viewport and scrollbar?"),
            ChatMessage::assistant(
                "09:45",
                "Yes. The caller should describe intent: fill this panel, inset from the shell, bind offset, compose content.",
            ),
        ],
        3 => vec![
            ChatMessage::assistant(
                "Yesterday",
                "The release note can stay small: new scroll_y, popover, and rounded_clip helpers for Neo UI.",
            ),
            ChatMessage::user("Yesterday", "Mention the screenshot path too."),
            ChatMessage::assistant(
                "Yesterday",
                "Done. I would include the exact cargo commands and the saved screenshot path in the verification section.",
            ),
        ],
        _ => vec![
            ChatMessage::assistant(
                "09:30",
                "I can help turn the UI shell into a stable component set. The key is to make layout intent explicit.",
            ),
            ChatMessage::user(
                "09:31",
                "The scroll bar keeps touching the rounded edge.",
            ),
            ChatMessage::assistant(
                "09:31",
                "That means the scroll area needs a panel-safe inset. The viewport and scrollbar should live inside the shell, not on the shell edge.",
            ),
            ChatMessage::user(
                "09:32",
                "Can dropdowns avoid changing the parent layout?",
            ),
            ChatMessage::assistant(
                "09:32",
                "Yes. A popover should be root-layer content anchored to the previous frame of the field, so it floats without resizing the row.",
            ),
        ],
    }
}

fn assistant_reply(prompt: &str) -> String {
    let lower = prompt.to_ascii_lowercase();
    if lower.contains("layout") {
        "I would split the layout into three stable zones: sidebar, message stream, and composer. Each zone owns its constraints, while scroll_y handles overflow without manual viewport math.".to_string()
    } else if lower.contains("state") {
        "Keep state boring: selected_thread, draft, chat_scroll, and messages. Bind input and scroll offset directly, then make send_draft the only mutation that appends messages.".to_string()
    } else if lower.contains("edge") || lower.contains("popover") {
        "The edge cases are first-frame anchoring, rounded clipping, scroll offset clamping, and long text. The example uses fallback anchors, rounded_clip, and auto content measurement to keep those boring.".to_string()
    } else {
        "I would keep the UI calm: clear hierarchy, visible state, no hidden layout math, and a short path from user input to rendered feedback.".to_string()
    }
}

fn next_time(count: usize) -> &'static str {
    match count % 4 {
        0 => "now",
        1 => "10:24",
        2 => "10:25",
        _ => "10:26",
    }
}

fn bubble_height(text: &str, width: f32) -> f32 {
    let chars_per_line = ((width - 42.0) / 7.6).max(18.0) as usize;
    let mut lines = 0usize;
    for paragraph in text.split('\n') {
        let count = paragraph.chars().count().max(1);
        lines += count.div_ceil(chars_per_line);
    }
    (48.0 + lines as f32 * 20.0).clamp(74.0, 190.0)
}

fn primary_button_style() -> widgets::ButtonStyle {
    let mut style = widgets::ButtonStyle::new(theme::dark_theme_colors(), true);
    style.normal = c(0.230, 0.560, 0.880, 1.0);
    style.hover = c(0.300, 0.660, 0.960, 1.0);
    style.pressed = c(0.160, 0.410, 0.680, 1.0);
    style.text = c(0.965, 0.990, 1.0, 1.0);
    style.radius = 16.0;
    style.shadow.enabled = false;
    style.press_scale = 0.97;
    style
}

fn secondary_button_style() -> widgets::ButtonStyle {
    let mut style = widgets::ButtonStyle::new(theme::dark_theme_colors(), false);
    style.normal = c(0.105, 0.130, 0.138, 0.96);
    style.hover = c(0.135, 0.170, 0.175, 1.0);
    style.pressed = c(0.080, 0.102, 0.108, 1.0);
    style.text = c(0.800, 0.900, 0.890, 1.0);
    style.border.color = c(0.360, 0.520, 0.500, 0.26);
    style.radius = 14.0;
    style.shadow.enabled = false;
    style
}

fn input_style() -> widgets::InputStyle {
    let mut style = widgets::InputStyle::default();
    style.background = c(0.046, 0.056, 0.062, 0.98);
    style.hover = c(0.060, 0.074, 0.080, 0.98);
    style.focused = c(0.055, 0.070, 0.076, 1.0);
    style.pressed = c(0.050, 0.060, 0.066, 1.0);
    style.border = c(0.350, 0.460, 0.470, 0.24);
    style.focus_border = c(0.330, 0.730, 0.650, 0.86);
    style.text = c(0.920, 0.955, 0.960, 1.0);
    style.placeholder = c(0.520, 0.610, 0.640, 0.82);
    style.cursor = c(0.350, 0.850, 0.700, 1.0);
    style.radius = 16.0;
    style.shadow.enabled = false;
    style
}

trait AlphaColor {
    fn with_alpha(self, alpha: f32) -> Self;
}

impl AlphaColor for Color {
    fn with_alpha(mut self, alpha: f32) -> Self {
        self.a = alpha;
        self
    }
}

fn c(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}

#[derive(Debug)]
struct ScreenshotProbe {
    path: Option<String>,
    frame: u32,
    frame_count: u32,
    taken: bool,
    exit_after: bool,
}

impl Default for ScreenshotProbe {
    fn default() -> Self {
        Self {
            path: std::env::var("SKY_NEO_SCREENSHOT_PATH")
                .ok()
                .filter(|value| !value.trim().is_empty()),
            frame: env_u32("SKY_NEO_SCREENSHOT_FRAME").unwrap_or(30),
            frame_count: 0,
            taken: false,
            exit_after: env_flag("SKY_NEO_EXIT_AFTER_SCREENSHOT"),
        }
    }
}

impl ScreenshotProbe {
    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        if !self.taken && self.frame_count >= self.frame {
            if let Some(path) = self.path.as_ref() {
                ctx.request_screenshot(path);
                self.taken = true;
                if self.exit_after {
                    ctx.request_exit();
                }
            }
        }
        self.frame_count = self.frame_count.saturating_add(1);
    }
}

fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok()?.parse().ok()
}

fn main() {
    let mut world = World::new();
    world
        .install(
            WindowPlugin::new("Sky Chat - Neo AI", WINDOW_W, WINDOW_H)
                .with_vsync(false)
                .with_resizable(true),
        )
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world
        .install(RenderPlugin::pipeline(
            RenderPipelineAsset::builder()
                .add_feature(SpriteFeature::unlit())
                .add_phase(TransparentPhase::new())
                .build(),
        ))
        .unwrap();

    App::new(world).run(NeoAiChatDemo::default());
}
