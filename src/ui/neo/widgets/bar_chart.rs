//! Port of `EUI-NEO/components/barchart.h`.

use crate::render::Color;

use super::super::{AnimProperty, Ease, HorizontalAlign, Response, Shadow, Transition, Ui};
use super::line_chart::{chart_tooltip, data_label, percent};
use super::theme::{self, ThemeColorTokens};

#[derive(Debug, Clone)]
pub struct BarChartStyle {
    pub background: Color,
    pub title: Color,
    pub label: Color,
    pub grid: Color,
    pub tooltip_background: Color,
    pub tooltip_text: Color,
    pub border: Color,
    pub shadow: Shadow,
    pub palette: Vec<Color>,
    pub radius: f32,
}

impl BarChartStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            background: tokens.surface,
            title: tokens.text,
            label: theme::with_opacity(tokens.text, 0.56),
            grid: theme::with_opacity(tokens.border, if tokens.dark { 0.38 } else { 0.36 }),
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

impl Default for BarChartStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

struct TooltipItem {
    source_id: String,
    text: String,
    x: f32,
    y: f32,
}

pub struct BarChartBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    title: String,
    values: Vec<f32>,
    labels: Vec<String>,
    style: BarChartStyle,
    transition: Transition,
    width: f32,
    height: f32,
}

impl<'ui> BarChartBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            title: "BarChart".to_string(),
            values: Vec::new(),
            labels: Vec::new(),
            style: BarChartStyle::default(),
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

    pub fn style(mut self, value: BarChartStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = BarChartStyle::new(tokens);
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
            self.values = vec![0.92, 0.36, 0.68, 0.52];
        }
        if self.labels.is_empty() {
            self.labels = ["D1", "D2", "D3", "D4"]
                .into_iter()
                .map(str::to_string)
                .collect();
        }

        let id = self.id.clone();
        let title_x = 20.0;
        let plot_x = 32.0;
        let plot_y = 70.0;
        let plot_width = (self.width - 64.0).max(1.0);
        let plot_height = (self.height - 112.0).max(1.0);
        let bottom_y = plot_y + plot_height;
        let count = self.values.len();
        let slot_width = plot_width / count.max(1) as f32;
        let bar_width = 32.0_f32.min(18.0_f32.max(slot_width * 0.54));
        let values = self.values.clone();
        let labels = self.labels.clone();
        let palette = self.style.palette.clone();

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

                for line in 0..4 {
                    let y = plot_y + line as f32 * plot_height / 3.0;
                    ui.rect(format!("{id}.grid.{line}"))
                        .x(plot_x)
                        .y(y)
                        .size(plot_width, 1.0)
                        .color(self.style.grid)
                        .build();
                }

                let mut tooltips = Vec::with_capacity(count);
                for (index, raw_value) in values.iter().copied().enumerate() {
                    let value = raw_value.clamp(0.0, 1.0);
                    let bar_height = 8.0_f32.max(value * plot_height);
                    let x = plot_x + index as f32 * slot_width + (slot_width - bar_width) * 0.5;
                    let y = bottom_y - bar_height;
                    let color = if palette.is_empty() {
                        theme::color(0.22, 0.50, 0.88, 1.0)
                    } else {
                        palette[index % palette.len()]
                    };
                    let bar_id = format!("{id}.bar.{index}");

                    ui.rect(bar_id.clone())
                        .x(x)
                        .y(y)
                        .size(bar_width, bar_height)
                        .states(
                            color,
                            theme::mix_color(color, theme::color(1.0, 1.0, 1.0, 1.0), 0.18),
                            theme::mix_color(color, theme::color(0.0, 0.0, 0.0, 1.0), 0.12),
                        )
                        .radius(10.0_f32.min(bar_width * 0.34))
                        .instant_states()
                        .transition(self.transition)
                        .animate(AnimProperty::FRAME | AnimProperty::COLOR)
                        .on_click(|| {})
                        .build();

                    tooltips.push(TooltipItem {
                        source_id: bar_id,
                        text: format!("{}  {}", data_label(&labels, index, "D"), percent(value)),
                        x: x + bar_width * 0.5,
                        y,
                    });

                    ui.text(format!("{id}.label.{index}"))
                        .x(x - 8.0)
                        .y(self.height - 34.0)
                        .size(bar_width + 16.0, 22.0)
                        .text(labels.get(index).cloned().unwrap_or_default())
                        .font_size(14.0)
                        .line_height(18.0)
                        .color(self.style.label)
                        .horizontal_align(HorizontalAlign::Center)
                        .build();
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
            });

        self.ui.response(&id)
    }
}

pub fn barchart(ui: &mut Ui, id: impl Into<String>) -> BarChartBuilder<'_> {
    BarChartBuilder::new(ui, id)
}

pub fn barChart(ui: &mut Ui, id: impl Into<String>) -> BarChartBuilder<'_> {
    barchart(ui, id)
}

pub fn bar_chart(ui: &mut Ui, id: impl Into<String>) -> BarChartBuilder<'_> {
    barchart(ui, id)
}
