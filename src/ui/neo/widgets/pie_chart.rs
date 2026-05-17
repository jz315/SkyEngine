//! Port of `EUI-NEO/components/piechart.h`.

use std::cell::RefCell;
use std::f32::consts::{FRAC_PI_2, TAU};

use rustc_hash::FxHashMap;

use crate::render::Color;

use super::super::{Ease, Response, Shadow, Transition, Ui};
use super::line_chart::{chart_tooltip, data_label, percent};
use super::theme::{self, ThemeColorTokens};

thread_local! {
    static PIE_ANIM_STATES: RefCell<FxHashMap<String, AnimState>> = RefCell::new(FxHashMap::default());
}

#[derive(Debug, Clone)]
pub struct PieChartStyle {
    pub background: Color,
    pub title: Color,
    pub tooltip_background: Color,
    pub tooltip_text: Color,
    pub border: Color,
    pub shadow: Shadow,
    pub palette: Vec<Color>,
    pub radius: f32,
}

impl PieChartStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            background: tokens.surface,
            title: tokens.text,
            tooltip_background: if tokens.dark {
                theme::mix_color(tokens.surface, theme::color(0.0, 0.0, 0.0, 1.0), 0.18)
            } else {
                theme::color(1.0, 1.0, 1.0, 0.96)
            },
            tooltip_text: tokens.text,
            border: theme::with_opacity(tokens.border, 0.76),
            shadow: theme::shadow(tokens, 18.0, 4.0, 0.20, 0.10),
            palette: vec![
                theme::color(0.22, 0.50, 0.88, 1.0),
                theme::color(0.20, 0.76, 0.58, 1.0),
                theme::color(0.98, 0.62, 0.15, 1.0),
                theme::color(0.86, 0.28, 0.44, 1.0),
            ],
            radius: 18.0,
        }
    }
}

impl Default for PieChartStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

#[derive(Debug, Clone, Default)]
struct AnimState {
    display: Vec<f32>,
    target: Vec<f32>,
    animating: bool,
}

struct TooltipItem {
    source_id: String,
    text: String,
    x: f32,
    y: f32,
}

pub struct PieChartBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    title: String,
    values: Vec<f32>,
    labels: Vec<String>,
    style: PieChartStyle,
    transition: Transition,
    width: f32,
    height: f32,
}

impl<'ui> PieChartBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            title: "PieChart".to_string(),
            values: Vec::new(),
            labels: Vec::new(),
            style: PieChartStyle::default(),
            transition: Transition::make(0.16, Ease::OutCubic),
            width: 206.0,
            height: 236.0,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn title(mut self, value: impl Into<String>) -> Self {
        self.title = value.into();
        self
    }

    pub fn values<I>(mut self, value: I) -> Self
    where
        I: IntoIterator<Item = f32>,
    {
        self.values = value.into_iter().collect();
        self
    }

    pub fn labels<I, S>(mut self, value: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.labels = value.into_iter().map(Into::into).collect();
        self
    }

    pub fn colors<I>(mut self, value: I) -> Self
    where
        I: IntoIterator<Item = Color>,
    {
        self.style.palette = value.into_iter().collect();
        self
    }

    pub fn style(mut self, value: PieChartStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = PieChartStyle::new(tokens);
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
        self
    }

    pub fn transition_seconds(mut self, duration: f32, ease: Ease) -> Self {
        self.transition = Transition::make(duration, ease);
        self
    }

    pub fn transitionSeconds(self, duration: f32, ease: Ease) -> Self {
        self.transition_seconds(duration, ease)
    }

    pub fn build(mut self) -> Response {
        if self.values.is_empty() {
            self.values = vec![0.42, 0.24, 0.18, 0.16];
        }
        if self.labels.is_empty() {
            self.labels = ["Blue", "Green", "Orange", "Pink"]
                .into_iter()
                .map(str::to_string)
                .collect();
        }

        let id = self.id.clone();
        let title_x = 20.0;
        let pie_size = 96.0_f32.max((self.width - 48.0).min(self.height - 82.0));
        let pie_x = (self.width - pie_size) * 0.5;
        let pie_y = 70.0;
        let labels = self.labels.clone();
        let values = self.values.clone();
        let palette = self.style.palette.clone();
        let (display_values, animating) = sync_animation(&id, &values);
        let total = value_total(&display_values);

        self.ui
            .stack(id.clone())
            .size(self.width, self.height)
            .content(|ui| {
                ui.rect(format!("{id}.bg"))
                    .size(self.width, self.height)
                    .color(self.style.background)
                    .radius(self.style.radius)
                    .border(1.0, self.style.border)
                    .shadow_style(self.style.shadow)
                    .build();

                ui.text(format!("{id}.title"))
                    .x(title_x)
                    .y(18.0)
                    .size((self.width - title_x * 2.0).max(0.0), 28.0)
                    .text(self.title.clone())
                    .font_size(22.0)
                    .line_height(26.0)
                    .color(self.style.title)
                    .build();

                let mut tooltips = Vec::with_capacity(display_values.len());
                let mut start_angle = -FRAC_PI_2;
                for (index, raw_value) in display_values.iter().copied().enumerate() {
                    let amount = if total > 0.0 {
                        raw_value.max(0.0) / total
                    } else {
                        0.0
                    };
                    let sweep = amount * TAU;
                    let end_angle = start_angle + sweep;
                    let color = slice_color(&palette, index);
                    let slice_id = format!("{id}.slice.{index}");

                    ui.polygon(slice_id.clone())
                        .x(pie_x)
                        .y(pie_y)
                        .size(pie_size, pie_size)
                        .points(slice_points(pie_size, start_angle, end_angle))
                        .states(
                            color,
                            theme::mix_color(color, theme::color(1.0, 1.0, 1.0, 1.0), 0.18),
                            theme::mix_color(color, theme::color(0.0, 0.0, 0.0, 1.0), 0.12),
                        )
                        .instant_states()
                        .transition(self.transition)
                        .on_click(|| {})
                        .build();

                    let anchor =
                        slice_anchor(pie_x, pie_y, pie_size, (start_angle + end_angle) * 0.5);
                    tooltips.push(TooltipItem {
                        source_id: slice_id,
                        text: format!("{}  {}", data_label(&labels, index, "S"), percent(amount)),
                        x: anchor[0],
                        y: anchor[1],
                    });
                    start_angle = end_angle;
                }

                for item in tooltips {
                    chart_tooltip(
                        ui,
                        &item.source_id,
                        &item.text,
                        item.x,
                        item.y,
                        self.width,
                        self.height,
                        self.style.tooltip_background,
                        self.style.tooltip_text,
                        self.style.border,
                    );
                }

                if animating {
                    ui.stack(format!("{id}.animator"))
                        .size(0.0, 0.0)
                        .on_timer(0.016, || {})
                        .build();
                }
            });

        self.ui.response(&id)
    }
}

pub fn piechart(ui: &mut Ui, id: impl Into<String>) -> PieChartBuilder<'_> {
    PieChartBuilder::new(ui, id)
}

pub fn pieChart(ui: &mut Ui, id: impl Into<String>) -> PieChartBuilder<'_> {
    piechart(ui, id)
}

pub fn pie_chart(ui: &mut Ui, id: impl Into<String>) -> PieChartBuilder<'_> {
    piechart(ui, id)
}

fn sync_animation(id: &str, values: &[f32]) -> (Vec<f32>, bool) {
    PIE_ANIM_STATES.with(|states| {
        let mut states = states.borrow_mut();
        let anim = states.entry(id.to_string()).or_default();
        if anim.display.len() != values.len() {
            anim.display = values.to_vec();
            anim.target = values.to_vec();
            anim.animating = false;
            return (anim.display.clone(), anim.animating);
        }

        if !close_values(&anim.target, values) {
            anim.target = values.to_vec();
            anim.animating = true;
        }

        if anim.animating {
            let mut moving = false;
            for index in 0..anim.display.len() {
                let next = anim.display[index] + (anim.target[index] - anim.display[index]) * 0.20;
                if (next - anim.target[index]).abs() > 0.002 {
                    anim.display[index] = next;
                    moving = true;
                } else {
                    anim.display[index] = anim.target[index];
                }
            }
            anim.animating = moving;
        }

        (anim.display.clone(), anim.animating)
    })
}

fn close_values(left: &[f32], right: &[f32]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(left, right)| (*left - *right).abs() <= 0.001)
}

fn value_total(values: &[f32]) -> f32 {
    values.iter().map(|value| value.max(0.0)).sum()
}

fn slice_color(palette: &[Color], index: usize) -> Color {
    if palette.is_empty() {
        theme::color(0.22, 0.50, 0.88, 1.0)
    } else {
        palette[index % palette.len()]
    }
}

fn slice_points(size: f32, start_angle: f32, end_angle: f32) -> Vec<[f32; 2]> {
    let radius = size * 0.5;
    let center = [radius, radius];
    let sweep = (end_angle - start_angle).max(0.0);
    let steps = 2.max((sweep / TAU * 56.0).ceil() as usize);
    let mut points = Vec::with_capacity(steps + 2);
    points.push(center);
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let angle = start_angle + sweep * t;
        points.push([
            center[0] + angle.cos() * radius,
            center[1] + angle.sin() * radius,
        ]);
    }
    points
}

fn slice_anchor(pie_x: f32, pie_y: f32, pie_size: f32, angle: f32) -> [f32; 2] {
    let radius = pie_size * 0.33;
    [
        pie_x + pie_size * 0.5 + angle.cos() * radius,
        pie_y + pie_size * 0.5 + angle.sin() * radius,
    ]
}
