//! Port of `EUI-NEO/components/linechart.h`.

use crate::render::Color;

use super::super::{
    AnimProperty, Ease, HorizontalAlign, Response, Shadow, Transition, Ui, VerticalAlign,
};
use super::theme::{self, ThemeColorTokens};

#[derive(Debug, Clone, Copy)]
pub struct LineChartStyle {
    pub background: Color,
    pub title: Color,
    pub label: Color,
    pub grid: Color,
    pub line: Color,
    pub point: Color,
    pub point_hover: Color,
    pub point_pressed: Color,
    pub tooltip_background: Color,
    pub tooltip_text: Color,
    pub border: Color,
    pub shadow: Shadow,
    pub radius: f32,
}

impl LineChartStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            background: tokens.surface,
            title: tokens.text,
            label: theme::with_opacity(tokens.text, 0.56),
            grid: theme::with_opacity(tokens.border, if tokens.dark { 0.38 } else { 0.36 }),
            line: tokens.primary,
            point: tokens.primary,
            point_hover: theme::mix_color(
                tokens.primary,
                theme::color(1.0, 1.0, 1.0, 1.0),
                if tokens.dark { 0.18 } else { 0.10 },
            ),
            point_pressed: theme::mix_color(tokens.primary, theme::color(0.0, 0.0, 0.0, 1.0), 0.12),
            tooltip_background: if tokens.dark {
                theme::mix_color(tokens.surface, theme::color(0.0, 0.0, 0.0, 1.0), 0.18)
            } else {
                theme::color(1.0, 1.0, 1.0, 0.96)
            },
            tooltip_text: tokens.text,
            border: theme::with_opacity(tokens.border, 0.76),
            shadow: theme::shadow(tokens, 18.0, 4.0, 0.20, 0.10),
            radius: 18.0,
        }
    }
}

impl Default for LineChartStyle {
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

pub struct LineChartBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    title: String,
    values: Vec<f32>,
    labels: Vec<String>,
    style: LineChartStyle,
    transition: Transition,
    width: f32,
    height: f32,
}

impl<'ui> LineChartBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            title: "LineChart".to_string(),
            values: Vec::new(),
            labels: ["Jan", "Feb", "Mar", "Apr", "May", "Jun"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            style: LineChartStyle::default(),
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

    pub fn style(mut self, value: LineChartStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = LineChartStyle::new(tokens);
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
            self.values = vec![0.22, 0.30, 0.20, 0.55, 0.42, 0.86];
        }

        let id = self.id.clone();
        let title_x = 20.0;
        let title_y = 18.0;
        let plot_x = 28.0;
        let plot_y = 70.0;
        let plot_width = (self.width - 56.0).max(1.0);
        let plot_height = (self.height - 112.0).max(1.0);
        let bottom_y = plot_y + plot_height;
        let count = self.values.len();
        let step_x = if count > 1 {
            plot_width / (count - 1) as f32
        } else {
            0.0
        };
        let line_height = 4.0;
        let point_size = 13.0;
        let values = self.values.clone();
        let labels = self.labels.clone();

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
                    .y(title_y)
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

                for index in 0..count.saturating_sub(1) {
                    let from = point_at(&values, index, plot_x, bottom_y, plot_height, step_x);
                    let to = point_at(&values, index + 1, plot_x, bottom_y, plot_height, step_x);
                    let dx = to[0] - from[0];
                    let dy = to[1] - from[1];
                    let length = (dx * dx + dy * dy).sqrt();
                    let angle = dy.atan2(dx);
                    let mid_x = (from[0] + to[0]) * 0.5;
                    let mid_y = (from[1] + to[1]) * 0.5;

                    ui.rect(format!("{id}.segment.{index}"))
                        .x(mid_x - length * 0.5)
                        .y(mid_y - line_height * 0.5)
                        .size(length, line_height)
                        .color(self.style.line)
                        .radius(line_height * 0.5)
                        .rotate(angle)
                        .transform_origin(0.5, 0.5)
                        .transition(self.transition)
                        .animate(AnimProperty::FRAME | AnimProperty::TRANSFORM)
                        .build();
                }

                let mut tooltips = Vec::with_capacity(count);
                for index in 0..count {
                    let point = point_at(&values, index, plot_x, bottom_y, plot_height, step_x);
                    let point_id = format!("{id}.point.{index}");
                    ui.rect(point_id.clone())
                        .x(point[0] - point_size * 0.5)
                        .y(point[1] - point_size * 0.5)
                        .size(point_size, point_size)
                        .states(
                            self.style.point,
                            self.style.point_hover,
                            self.style.point_pressed,
                        )
                        .radius(point_size * 0.5)
                        .instant_states()
                        .transition(self.transition)
                        .animate(AnimProperty::FRAME | AnimProperty::COLOR)
                        .on_click(|| {})
                        .build();

                    tooltips.push(TooltipItem {
                        source_id: point_id,
                        text: format!(
                            "{}  {}",
                            data_label(&labels, index, "P"),
                            percent(values[index])
                        ),
                        x: point[0],
                        y: point[1],
                    });
                }

                let label_width =
                    28.0_f32.max(42.0_f32.min(if count > 1 { step_x } else { plot_width }));
                for index in 0..count {
                    let point = point_at(&values, index, plot_x, bottom_y, plot_height, step_x);
                    let label_x = (point[0] - label_width * 0.5)
                        .clamp(0.0, (self.width - label_width).max(0.0));
                    label(
                        ui,
                        &format!("{id}.label.{index}"),
                        &data_label(&labels, index, "P"),
                        label_x,
                        self.height - 34.0,
                        label_width,
                        self.style.label,
                    );
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

pub fn linechart(ui: &mut Ui, id: impl Into<String>) -> LineChartBuilder<'_> {
    LineChartBuilder::new(ui, id)
}

pub fn lineChart(ui: &mut Ui, id: impl Into<String>) -> LineChartBuilder<'_> {
    linechart(ui, id)
}

pub fn line_chart(ui: &mut Ui, id: impl Into<String>) -> LineChartBuilder<'_> {
    linechart(ui, id)
}

fn point_at(
    values: &[f32],
    index: usize,
    plot_x: f32,
    bottom_y: f32,
    plot_height: f32,
    step_x: f32,
) -> [f32; 2] {
    let value = values[index].clamp(0.0, 1.0);
    [
        plot_x + index as f32 * step_x,
        bottom_y - value * plot_height,
    ]
}

pub(super) fn label(ui: &mut Ui, id: &str, value: &str, x: f32, y: f32, width: f32, color: Color) {
    ui.text(id)
        .x(x)
        .y(y)
        .size(width, 22.0)
        .text(value)
        .font_size(14.0)
        .line_height(18.0)
        .color(color)
        .horizontal_align(HorizontalAlign::Center)
        .build();
}

pub(super) fn data_label(labels: &[String], index: usize, prefix: &str) -> String {
    labels
        .get(index)
        .cloned()
        .unwrap_or_else(|| format!("{prefix}{}", index + 1))
}

pub(super) fn percent(value: f32) -> String {
    format!("{:.0}%", value.clamp(0.0, 1.0) * 100.0)
}

pub(super) fn chart_tooltip(
    ui: &mut Ui,
    source_id: &str,
    value: &str,
    anchor_x: f32,
    anchor_y: f32,
    width: f32,
    height: f32,
    background: Color,
    text_color: Color,
    border: Color,
) {
    let tooltip_width = 112.0_f32.min((width - 42.0).max(86.0));
    let tooltip_height = 32.0;
    let pointer_height = 8.0;
    let pointer_half_width = 7.0;
    let stack_height = tooltip_height + pointer_height;
    let tooltip_gap = 0.0;
    let tooltip_x =
        (anchor_x - tooltip_width * 0.5).clamp(12.0, 12.0_f32.max(width - tooltip_width - 12.0));
    let below_anchor = anchor_y - stack_height - tooltip_gap < 46.0;
    let wanted_y = if below_anchor {
        anchor_y + tooltip_gap
    } else {
        anchor_y - stack_height - tooltip_gap
    };
    let tooltip_y = wanted_y.clamp(44.0, 44.0_f32.max(height - stack_height - 14.0));
    let panel_y = if below_anchor { pointer_height } else { 0.0 };
    let pointer_y = if below_anchor { 0.0 } else { tooltip_height };
    let pointer_x = (anchor_x - tooltip_x).clamp(
        pointer_half_width + 4.0,
        tooltip_width - pointer_half_width - 4.0,
    );
    let tooltip_id = format!("{source_id}.tooltip");

    ui.stack(tooltip_id.clone())
        .x(tooltip_x)
        .y(tooltip_y)
        .size(tooltip_width, stack_height)
        .hover_opacity_from(source_id, 0.0, 1.0)
        .content(|ui| {
            ui.rect(format!("{tooltip_id}.bg"))
                .y(panel_y)
                .size(tooltip_width, tooltip_height)
                .color(background)
                .radius(9.0)
                .border(1.0, border)
                .shadow(12.0, 0.0, 4.0, theme::color(0.0, 0.0, 0.0, 0.16))
                .build();

            let pointer_points = if below_anchor {
                vec![
                    [pointer_x, 0.0],
                    [pointer_x + pointer_half_width, pointer_height],
                    [pointer_x - pointer_half_width, pointer_height],
                ]
            } else {
                vec![
                    [pointer_x - pointer_half_width, 0.0],
                    [pointer_x + pointer_half_width, 0.0],
                    [pointer_x, pointer_height],
                ]
            };
            ui.polygon(format!("{tooltip_id}.pointer"))
                .x(0.0)
                .y(pointer_y)
                .size(tooltip_width, pointer_height)
                .points(pointer_points)
                .color(background)
                .build();

            ui.text(format!("{tooltip_id}.text"))
                .x(10.0)
                .y(panel_y)
                .size((tooltip_width - 20.0).max(0.0), tooltip_height)
                .text(value)
                .font_size(13.0)
                .line_height(16.0)
                .color(text_color)
                .horizontal_align(HorizontalAlign::Center)
                .vertical_align(VerticalAlign::Center)
                .build();
        });
}
