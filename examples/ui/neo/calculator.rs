//! Calculator demo built with `ui-neo`.
//!
//! ```bash
//! cargo run --example ui_neo_calculator --features ui-neo --release
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
use sky_engine::ui::neo::{Align, Color, HorizontalAlign, NeoState, Size, Ui};

const WINDOW_W: u32 = 520;
const WINDOW_H: u32 = 720;
const CARD_W: f32 = 390.0;
const CARD_H: f32 = 560.0;
const KEY_W: f32 = 79.0;
const KEY_H: f32 = 66.0;
const KEY_GAP: f32 = 10.0;

struct NeoCalculatorDemo {
    state: NeoState<CalculatorState>,
    screenshot: ScreenshotProbe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl Operator {
    fn apply(self, lhs: f64, rhs: f64) -> Option<f64> {
        match self {
            Self::Add => Some(lhs + rhs),
            Self::Subtract => Some(lhs - rhs),
            Self::Multiply => Some(lhs * rhs),
            Self::Divide => (rhs != 0.0).then_some(lhs / rhs),
        }
    }
}

#[derive(Debug)]
struct CalculatorState {
    display: String,
    stored: Option<f64>,
    pending: Option<Operator>,
    reset_display: bool,
    last_expression: String,
}

impl Default for NeoCalculatorDemo {
    fn default() -> Self {
        Self {
            state: NeoState::new(CalculatorState::default()),
            screenshot: ScreenshotProbe::default(),
        }
    }
}

impl Default for CalculatorState {
    fn default() -> Self {
        Self {
            display: "0".to_string(),
            stored: None,
            pending: None,
            reset_display: false,
            last_expression: String::new(),
        }
    }
}

impl CalculatorState {
    fn input_digit(&mut self, digit: char) {
        if self.display == "Error" || self.reset_display {
            self.display.clear();
            self.reset_display = false;
        }
        if self.display == "0" {
            self.display.clear();
        }
        if self.display.len() < 14 {
            self.display.push(digit);
        }
        if self.display.is_empty() {
            self.display.push('0');
        }
    }

    fn input_decimal(&mut self) {
        if self.display == "Error" || self.reset_display {
            self.display = "0".to_string();
            self.reset_display = false;
        }
        if !self.display.contains('.') && self.display.len() < 14 {
            self.display.push('.');
        }
    }

    fn choose_operator(&mut self, operator: Operator) {
        if self.display == "Error" {
            self.clear();
            return;
        }
        let current = self.current_value();
        if let (Some(lhs), Some(pending)) = (self.stored, self.pending) {
            if !self.reset_display {
                match pending.apply(lhs, current) {
                    Some(value) => {
                        self.stored = Some(value);
                        self.display = format_value(value);
                    }
                    None => {
                        self.error();
                        return;
                    }
                }
            }
        } else {
            self.stored = Some(current);
        }
        self.pending = Some(operator);
        self.reset_display = true;
        self.last_expression = format!(
            "{} {}",
            format_value(self.stored.unwrap_or(0.0)),
            op_text(operator)
        );
    }

    fn equals(&mut self) {
        if self.display == "Error" {
            self.clear();
            return;
        }
        let Some(operator) = self.pending else {
            return;
        };
        let lhs = self.stored.unwrap_or(0.0);
        let rhs = self.current_value();
        match operator.apply(lhs, rhs) {
            Some(value) => {
                self.display = format_value(value);
                self.last_expression = format!(
                    "{} {} {}",
                    format_value(lhs),
                    op_text(operator),
                    format_value(rhs)
                );
                self.stored = None;
                self.pending = None;
                self.reset_display = true;
            }
            None => self.error(),
        }
    }

    fn toggle_sign(&mut self) {
        if self.display == "0" || self.display == "Error" {
            return;
        }
        if self.display.starts_with('-') {
            self.display.remove(0);
        } else if self.display.len() < 14 {
            self.display.insert(0, '-');
        }
    }

    fn percent(&mut self) {
        if self.display == "Error" {
            return;
        }
        self.display = format_value(self.current_value() / 100.0);
        self.reset_display = true;
    }

    fn backspace(&mut self) {
        if self.display == "Error" || self.reset_display {
            self.display = "0".to_string();
            self.reset_display = false;
            return;
        }
        self.display.pop();
        if self.display.is_empty() || self.display == "-" {
            self.display = "0".to_string();
        }
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn error(&mut self) {
        self.display = "Error".to_string();
        self.stored = None;
        self.pending = None;
        self.reset_display = true;
        self.last_expression = "Cannot divide by zero".to_string();
    }

    fn current_value(&self) -> f64 {
        self.display.parse::<f64>().unwrap_or(0.0)
    }
}

impl AppState for NeoCalculatorDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        ctx.world.insert_resource(RenderSettings {
            clear_color: c(0.070, 0.082, 0.096, 1.0).into(),
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
        let snapshot = self.state.read(|state| CalculatorSnapshot::from(state));
        sky_engine::ui::neo::compose(ctx, move |ui, screen| {
            draw_calculator(ui, screen.width, screen.height, &state, &snapshot);
        });

        ctx.render();
        ctx.ui().render_overlays();
        self.screenshot.update(ctx);
        ctx.request_redraw();
    }
}

struct CalculatorSnapshot {
    display: String,
    expression: String,
    pending: Option<Operator>,
}

impl From<&CalculatorState> for CalculatorSnapshot {
    fn from(value: &CalculatorState) -> Self {
        Self {
            display: value.display.clone(),
            expression: value.last_expression.clone(),
            pending: value.pending,
        }
    }
}

fn draw_calculator(
    ui: &mut Ui,
    screen_width: f32,
    screen_height: f32,
    state: &NeoState<CalculatorState>,
    snapshot: &CalculatorSnapshot,
) {
    ui.rect("background")
        .size(screen_width, screen_height)
        .gradient(c(0.070, 0.082, 0.096, 1.0), c(0.125, 0.106, 0.135, 1.0))
        .build();

    ui.stack("stage")
        .size(screen_width, screen_height)
        .padding(28.0)
        .align(Align::Center, Align::Center)
        .content(|ui| {
            ui.stack("calculator").size(CARD_W, CARD_H).content(|ui| {
                widgets::panel(ui, "calculator.bg")
                    .fill()
                    .radius(26.0)
                    .gradient(c(0.135, 0.152, 0.170, 0.98), c(0.075, 0.085, 0.102, 0.99))
                    .border(1.0, c(0.520, 0.660, 0.740, 0.20))
                    .shadow(36.0, 0.0, 18.0, c(0.0, 0.0, 0.0, 0.34))
                    .build();

                ui.column("calculator.content")
                    .fill()
                    .padding(22.0)
                    .gap(14.0)
                    .content(|ui| {
                        display_panel(ui, snapshot);
                        key_grid(ui, state, snapshot.pending);
                    });
            });
        });
}

fn display_panel(ui: &mut Ui, snapshot: &CalculatorSnapshot) {
    ui.stack("display").size(Size::fill(), 118.0).content(|ui| {
        ui.rect("display.bg")
            .fill()
            .radius(18.0)
            .color(c(0.035, 0.043, 0.052, 0.92))
            .border(1.0, c(0.380, 0.470, 0.520, 0.22))
            .build();

        ui.column("display.text")
            .fill()
            .padding(18.0)
            .gap(8.0)
            .justify_content(Align::End)
            .content(|ui| {
                ui.text("display.expression")
                    .size(Size::fill(), 24.0)
                    .text(if snapshot.expression.is_empty() {
                        " ".to_string()
                    } else {
                        snapshot.expression.clone()
                    })
                    .font_size(15.0)
                    .line_height(24.0)
                    .color(c(0.640, 0.735, 0.785, 0.78))
                    .horizontal_align(HorizontalAlign::Right)
                    .build();

                ui.text("display.value")
                    .size(Size::fill(), 54.0)
                    .text(snapshot.display.clone())
                    .font_size(display_font_size(&snapshot.display))
                    .line_height(54.0)
                    .color(c(0.950, 0.975, 1.0, 1.0))
                    .horizontal_align(HorizontalAlign::Right)
                    .build();
            });
    });
}

fn key_grid(ui: &mut Ui, state: &NeoState<CalculatorState>, pending: Option<Operator>) {
    let rows: [[Key; 4]; 5] = [
        [
            Key::Action("C", CalcAction::Clear),
            Key::Action("+/-", CalcAction::Sign),
            Key::Action("%", CalcAction::Percent),
            Key::Operator("/", Operator::Divide),
        ],
        [
            Key::Digit('7'),
            Key::Digit('8'),
            Key::Digit('9'),
            Key::Operator("x", Operator::Multiply),
        ],
        [
            Key::Digit('4'),
            Key::Digit('5'),
            Key::Digit('6'),
            Key::Operator("-", Operator::Subtract),
        ],
        [
            Key::Digit('1'),
            Key::Digit('2'),
            Key::Digit('3'),
            Key::Operator("+", Operator::Add),
        ],
        [
            Key::Action("Del", CalcAction::Backspace),
            Key::Digit('0'),
            Key::Action(".", CalcAction::Decimal),
            Key::Action("=", CalcAction::Equals),
        ],
    ];

    ui.column("keys")
        .size(Size::fill(), Size::fill())
        .gap(KEY_GAP)
        .content(|ui| {
            for (row_index, row) in rows.iter().enumerate() {
                ui.row(format!("keys.row.{row_index}"))
                    .size(Size::fill(), KEY_H)
                    .gap(KEY_GAP)
                    .content(|ui| {
                        for key in row {
                            draw_key(ui, state, *key, pending);
                        }
                    });
            }
        });
}

#[derive(Clone, Copy)]
enum Key {
    Digit(char),
    Operator(&'static str, Operator),
    Action(&'static str, CalcAction),
}

#[derive(Clone, Copy)]
enum CalcAction {
    Clear,
    Sign,
    Percent,
    Decimal,
    Equals,
    Backspace,
}

fn draw_key(ui: &mut Ui, state: &NeoState<CalculatorState>, key: Key, pending: Option<Operator>) {
    let id = match key {
        Key::Digit(value) => format!("key.{value}"),
        Key::Operator(label, _) | Key::Action(label, _) => format!("key.{label}"),
    };
    let label = match key {
        Key::Digit(value) => value.to_string(),
        Key::Operator(label, _) | Key::Action(label, _) => label.to_string(),
    };
    let is_operator = matches!(key, Key::Operator(_, _));
    let is_equals = matches!(key, Key::Action("=", _));
    let selected = matches!((key, pending), (Key::Operator(_, op), Some(active)) if op == active);
    let style = key_style(is_operator, is_equals, selected);
    let click_state = state.clone();

    widgets::button(ui, id)
        .size(KEY_W, KEY_H)
        .min_width(70.0)
        .text(label)
        .font_size(24.0)
        .radius(16.0)
        .style(style)
        .on_click(move || {
            click_state.update(|state| match key {
                Key::Digit(value) => state.input_digit(value),
                Key::Operator(_, operator) => state.choose_operator(operator),
                Key::Action(_, CalcAction::Clear) => state.clear(),
                Key::Action(_, CalcAction::Sign) => state.toggle_sign(),
                Key::Action(_, CalcAction::Percent) => state.percent(),
                Key::Action(_, CalcAction::Decimal) => state.input_decimal(),
                Key::Action(_, CalcAction::Equals) => state.equals(),
                Key::Action(_, CalcAction::Backspace) => state.backspace(),
            });
        })
        .build();
}

fn key_style(operator: bool, equals: bool, selected: bool) -> widgets::ButtonStyle {
    let tokens = theme::dark_theme_colors();
    let mut style = widgets::ButtonStyle::new(tokens, operator || equals);
    if equals {
        style.normal = c(0.260, 0.600, 0.900, 1.0);
        style.hover = c(0.330, 0.690, 0.980, 1.0);
        style.pressed = c(0.180, 0.440, 0.740, 1.0);
        style.text = c(0.965, 0.985, 1.0, 1.0);
    } else if operator {
        style.normal = if selected {
            c(0.780, 0.560, 0.260, 1.0)
        } else {
            c(0.540, 0.370, 0.190, 1.0)
        };
        style.hover = c(0.710, 0.500, 0.250, 1.0);
        style.pressed = c(0.430, 0.280, 0.145, 1.0);
        style.text = c(1.0, 0.960, 0.880, 1.0);
    } else {
        style.normal = c(0.155, 0.180, 0.205, 1.0);
        style.hover = c(0.220, 0.255, 0.285, 1.0);
        style.pressed = c(0.105, 0.125, 0.145, 1.0);
        style.text = c(0.930, 0.960, 0.980, 1.0);
    }
    style.border.color = c(0.700, 0.820, 0.880, if selected { 0.40 } else { 0.16 });
    style.shadow.enabled = false;
    style.press_scale = 0.96;
    style
}

fn display_font_size(value: &str) -> f32 {
    match value.len() {
        0..=8 => 42.0,
        9..=11 => 36.0,
        _ => 30.0,
    }
}

fn format_value(value: f64) -> String {
    if !value.is_finite() {
        return "Error".to_string();
    }
    let mut text = if value.abs() >= 1_000_000_000.0 || (value != 0.0 && value.abs() < 0.000001) {
        format!("{value:.6e}")
    } else {
        format!("{value:.8}")
    };
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        text = "0".to_string();
    }
    if text.len() > 14 {
        text.truncate(14);
    }
    text
}

fn op_text(operator: Operator) -> &'static str {
    match operator {
        Operator::Add => "+",
        Operator::Subtract => "-",
        Operator::Multiply => "x",
        Operator::Divide => "/",
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
            WindowPlugin::new("Neo Calculator", WINDOW_W, WINDOW_H)
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

    App::new(world).run(NeoCalculatorDemo::default());
}
