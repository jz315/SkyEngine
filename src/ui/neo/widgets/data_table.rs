//! Port of `EUI-NEO/components/datatable.h`.

use crate::render::Color;

use super::super::{Ease, Response, Transition, Ui, VerticalAlign};
use super::theme::{self, ThemeColorTokens};

#[derive(Debug, Clone, Copy)]
pub struct DataTableStyle {
    pub background: Color,
    pub header: Color,
    pub row: Color,
    pub row_alt: Color,
    pub row_hover: Color,
    pub text: Color,
    pub muted_text: Color,
    pub accent: Color,
    pub border: Color,
    pub divider: Color,
    pub radius: f32,
}

impl DataTableStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            background: tokens.surface,
            header: if tokens.dark {
                theme::mix_color(tokens.surface_hover, tokens.surface, 0.32)
            } else {
                tokens.surface_hover
            },
            row: tokens.surface,
            row_alt: theme::mix_color(
                tokens.surface,
                tokens.surface_hover,
                if tokens.dark { 0.30 } else { 0.36 },
            ),
            row_hover: tokens.surface_hover,
            text: tokens.text,
            muted_text: theme::with_opacity(tokens.text, 0.62),
            accent: tokens.primary,
            border: theme::with_opacity(tokens.border, 0.72),
            divider: theme::with_opacity(tokens.border, if tokens.dark { 0.42 } else { 0.46 }),
            radius: 12.0,
        }
    }
}

impl Default for DataTableStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct DataTableBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    style: DataTableStyle,
    transition: Transition,
    width: f32,
    height: f32,
}

impl<'ui> DataTableBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            columns: Vec::new(),
            rows: Vec::new(),
            style: DataTableStyle::default(),
            transition: Transition::make(0.12, Ease::OutCubic),
            width: 420.0,
            height: 174.0,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn columns<I, S>(mut self, value: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.columns = value.into_iter().map(Into::into).collect();
        self
    }

    pub fn rows<I, R, S>(mut self, value: I) -> Self
    where
        I: IntoIterator<Item = R>,
        R: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.rows = value
            .into_iter()
            .map(|row| row.into_iter().map(Into::into).collect())
            .collect();
        self
    }

    pub fn style(mut self, value: DataTableStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = DataTableStyle::new(tokens);
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

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let column_count = self.columns.len().max(1);
        let row_count = self.rows.len();
        let border_width = 1.0;
        let content_x = border_width;
        let content_y = border_width;
        let content_width = (self.width - border_width * 2.0).max(0.0);
        let content_height = (self.height - border_width * 2.0).max(0.0);
        let content_radius = (self.style.radius - border_width).max(0.0);
        let header_height = 38.0_f32.min(content_height);
        let body_height = (content_height - header_height).max(0.0);
        let row_height = if row_count > 0 {
            body_height / row_count as f32
        } else {
            body_height
        };
        let column_width = content_width / column_count as f32;
        let header_patch_y = (header_height - content_radius).max(0.0);
        let header_patch_height = content_radius.min(header_height);
        let text_inset = 16.0;

        self.ui
            .stack(id.clone())
            .size(self.width, self.height)
            .content(|ui| {
                ui.rect(format!("{id}.bg"))
                    .size(self.width, self.height)
                    .color(self.style.background)
                    .radius(self.style.radius)
                    .border(border_width, self.style.border)
                    .build();

                ui.stack(format!("{id}.content"))
                    .x(content_x)
                    .y(content_y)
                    .size(content_width, content_height)
                    .clip()
                    .content(|ui| {
                        ui.rect(format!("{id}.header.cap"))
                            .size(content_width, header_height)
                            .color(self.style.header)
                            .radius(content_radius)
                            .build();

                        if header_patch_height > 0.0 {
                            ui.rect(format!("{id}.header.patch"))
                                .y(header_patch_y)
                                .size(content_width, header_patch_height)
                                .color(self.style.header)
                                .build();
                        }

                        ui.rect(format!("{id}.header.divider"))
                            .y((header_height - 1.0).max(0.0))
                            .size(content_width, 1.0)
                            .color(self.style.divider)
                            .build();

                        for column in 0..column_count {
                            let x = column as f32 * column_width;
                            let label = self.columns.get(column).cloned().unwrap_or_default();
                            ui.text(format!("{id}.header.{column}"))
                                .x(x + text_inset)
                                .size((column_width - text_inset * 2.0).max(0.0), header_height)
                                .text(label)
                                .font_size(14.0)
                                .line_height(18.0)
                                .color(if column == 0 {
                                    self.style.accent
                                } else {
                                    self.style.muted_text
                                })
                                .vertical_align(VerticalAlign::Center)
                                .build();
                        }

                        for (row_index, row) in self.rows.iter().enumerate() {
                            let last_row = row_index + 1 == row_count;
                            let y = header_height + row_index as f32 * row_height;
                            let next_y = if last_row {
                                content_height
                            } else {
                                header_height + (row_index + 1) as f32 * row_height
                            };
                            let height = (next_y - y).max(0.0);
                            let top_patch_height = content_radius.min(height);
                            let row_color = if row_index % 2 == 0 {
                                self.style.row
                            } else {
                                self.style.row_alt
                            };

                            ui.rect(format!("{id}.row.{row_index}"))
                                .y(y)
                                .size(content_width, height)
                                .color(row_color)
                                .radius(if last_row { content_radius } else { 0.0 })
                                .build();

                            if last_row && top_patch_height > 0.0 {
                                ui.rect(format!("{id}.row.{row_index}.top.patch"))
                                    .y(y)
                                    .size(content_width, top_patch_height)
                                    .color(row_color)
                                    .build();
                            }

                            if !last_row {
                                ui.rect(format!("{id}.row.{row_index}.divider"))
                                    .y(header_height.max(next_y - 1.0))
                                    .size(content_width, 1.0)
                                    .color(self.style.divider)
                                    .build();
                            }

                            for column in 0..column_count {
                                let x = column as f32 * column_width;
                                let value = row.get(column).cloned().unwrap_or_default();
                                ui.text(format!("{id}.cell.{row_index}.{column}"))
                                    .x(x + text_inset)
                                    .y(y)
                                    .size((column_width - text_inset * 2.0).max(0.0), height)
                                    .text(value)
                                    .font_size(14.0)
                                    .line_height(18.0)
                                    .color(if column == 0 {
                                        self.style.text
                                    } else {
                                        self.style.muted_text
                                    })
                                    .vertical_align(VerticalAlign::Center)
                                    .transition(self.transition)
                                    .animate(super::super::AnimProperty::TEXT_COLOR)
                                    .build();
                            }
                        }
                    });
            });

        self.ui.response(&id)
    }
}

pub fn data_table(ui: &mut Ui, id: impl Into<String>) -> DataTableBuilder<'_> {
    DataTableBuilder::new(ui, id)
}

pub fn dataTable(ui: &mut Ui, id: impl Into<String>) -> DataTableBuilder<'_> {
    data_table(ui, id)
}

pub fn datatable(ui: &mut Ui, id: impl Into<String>) -> DataTableBuilder<'_> {
    data_table(ui, id)
}
