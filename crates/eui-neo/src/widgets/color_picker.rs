//! Port of `EUI-NEO/components/colorpicker.h`.

use std::cell::RefCell;
use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::Color;

use super::super::{
    AnimProperty, HorizontalAlign, LayoutRect, OutsideClickPolicy, Response, Shadow, Signal,
    Transition, Ui, VerticalAlign,
};
use super::popover::{popover, PopoverPlacement};
use super::slider::{slider, SliderStyle};
use super::theme::{self, ThemeColorTokens};

type ColorChangeCallback = Rc<RefCell<Box<dyn FnMut(Color)>>>;
type OpenChangeCallback = Rc<RefCell<Box<dyn FnMut(bool)>>>;

thread_local! {
    static COLOR_DRAFTS: RefCell<FxHashMap<String, ColorDraft>> = RefCell::new(FxHashMap::default());
}

#[derive(Debug, Clone, Copy)]
pub struct ColorPickerStyle {
    pub backdrop: Color,
    pub surface: Color,
    pub track: Color,
    pub text: Color,
    pub muted_text: Color,
    pub accent: Color,
    pub border: Color,
    pub knob: Color,
    pub shadow: Shadow,
    pub radius: f32,
}

impl ColorPickerStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            backdrop: theme::color(0.0, 0.0, 0.0, if tokens.dark { 0.42 } else { 0.26 }),
            surface: if tokens.dark {
                theme::mix_color(tokens.surface, theme::color(0.0, 0.0, 0.0, 1.0), 0.14)
            } else {
                tokens.surface
            },
            track: theme::with_alpha(tokens.text, if tokens.dark { 0.12 } else { 0.10 }),
            text: tokens.text,
            muted_text: theme::with_opacity(tokens.text, 0.62),
            accent: tokens.primary,
            border: theme::with_opacity(tokens.border, 0.80),
            knob: if tokens.dark {
                theme::color(0.96, 0.98, 1.0, 1.0)
            } else {
                theme::color(1.0, 1.0, 1.0, 1.0)
            },
            shadow: theme::popup_shadow(tokens),
            radius: 16.0,
        }
    }
}

impl Default for ColorPickerStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

#[derive(Debug, Clone, Copy)]
struct ColorDraft {
    active: bool,
    value: Color,
}

impl Default for ColorDraft {
    fn default() -> Self {
        Self {
            active: false,
            value: theme::color(0.22, 0.50, 0.88, 1.0),
        }
    }
}

pub struct ColorPickerBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: ColorPickerStyle,
    transition: Transition,
    on_change: Option<ColorChangeCallback>,
    on_open_change: Option<OpenChangeCallback>,
    colors: Vec<Color>,
    value: Color,
    screen_width: f32,
    screen_height: f32,
    width: f32,
    height: f32,
    open: bool,
    z_index: i32,
}

impl<'ui> ColorPickerBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: ColorPickerStyle::default(),
            transition: Transition::smooth(),
            on_change: None,
            on_open_change: None,
            colors: Vec::new(),
            value: theme::color(0.22, 0.50, 0.88, 1.0),
            screen_width: 800.0,
            screen_height: 600.0,
            width: 420.0,
            height: 320.0,
            open: false,
            z_index: 1000,
        }
    }

    pub fn open(mut self, value: bool) -> Self {
        self.open = value;
        self
    }

    pub fn open_signal<T: 'static>(self, signal: Signal<T, bool>) -> Self {
        let owner = self.id.clone();
        let value = self.ui.with_dependency_owner(owner, |ui| signal.watch(ui));
        self.open(value)
            .on_open_change(move |next| signal.set(next))
    }

    pub fn screen(mut self, width: f32, height: f32) -> Self {
        self.screen_width = width;
        self.screen_height = height;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn value(mut self, value: Color) -> Self {
        self.value = clamp_color(value);
        self
    }

    pub fn value_signal<T: 'static>(self, signal: Signal<T, Color>) -> Self {
        let owner = self.id.clone();
        let value = self.ui.with_dependency_owner(owner, |ui| signal.watch(ui));
        self.value(value).on_change(move |next| signal.set(next))
    }

    pub fn colors<I>(mut self, value: I) -> Self
    where
        I: IntoIterator<Item = Color>,
    {
        self.colors = value.into_iter().collect();
        self
    }

    pub fn style(mut self, value: ColorPickerStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = ColorPickerStyle::new(tokens);
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
        self
    }

    pub fn z_index(mut self, value: i32) -> Self {
        self.z_index = value;
        self
    }

    pub fn z(self, value: i32) -> Self {
        self.z_index(value)
    }

    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: FnMut(Color) + 'static,
    {
        let next: ColorChangeCallback = Rc::new(RefCell::new(Box::new(callback)));
        self.on_change = Some(if let Some(existing) = self.on_change.take() {
            Rc::new(RefCell::new(Box::new(move |value| {
                (existing.borrow_mut())(value);
                (next.borrow_mut())(value);
            })))
        } else {
            next
        });
        self
    }

    pub fn on_open_change<F>(mut self, callback: F) -> Self
    where
        F: FnMut(bool) + 'static,
    {
        let next: OpenChangeCallback = Rc::new(RefCell::new(Box::new(callback)));
        self.on_open_change = Some(if let Some(existing) = self.on_open_change.take() {
            Rc::new(RefCell::new(Box::new(move |value| {
                (existing.borrow_mut())(value);
                (next.borrow_mut())(value);
            })))
        } else {
            next
        });
        self
    }

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let panel_width = self.width.min((self.screen_width - 48.0).max(0.0));
        let panel_height = self.height.min((self.screen_height - 48.0).max(0.0));
        let panel_x = 24.0_f32.max((self.screen_width - panel_width) * 0.5);
        let panel_y = 24.0_f32.max((self.screen_height - panel_height) * 0.5);
        let visible = if self.open { 1.0 } else { 0.0 };
        let panel_scale = if self.open { 1.0 } else { 0.965 };
        let panel_offset_y = if self.open { 0.0 } else { 14.0 };
        let draft = sync_color_draft(&id, self.open, self.value);
        let open_change = self.on_open_change.clone();

        if self.open {
            self.ui.with_root_layer(|ui| {
                ui.rect(format!("{id}.backdrop"))
                    .position(0.0, 0.0)
                    .size(self.screen_width, self.screen_height)
                    .color(self.style.backdrop)
                    .opacity(visible)
                    .z_index(self.z_index)
                    .transition(self.transition)
                    .animate(AnimProperty::OPACITY)
                    .build();
            });
        }

        popover(self.ui, id.clone())
            .open(self.open)
            .fallback_anchor(LayoutRect::new(panel_x, panel_y, 0.0, 0.0))
            .placement(PopoverPlacement::BottomStart)
            .gap(0.0)
            .size(panel_width, panel_height)
            .z_index(self.z_index + 1)
            .outside_click(OutsideClickPolicy::Close)
            .on_dismiss(move || call_open_change(&open_change, false))
            .content(|ui| {
                ui.stack(format!("{id}.panel"))
                    .size(panel_width, panel_height)
                    .opacity(visible)
                    .translate_y(panel_offset_y)
                    .scale(panel_scale)
                    .transform_origin(0.5, 0.5)
                    .transition(self.transition)
                    .animate(AnimProperty::OPACITY | AnimProperty::TRANSFORM)
                    .content(|ui| {
                        color_panel(
                            ui,
                            &id,
                            panel_width,
                            panel_height,
                            self.open,
                            self.style,
                            self.transition,
                            draft.value,
                            self.value,
                            self.colors.clone(),
                            self.on_change.clone(),
                            self.on_open_change.clone(),
                        );
                    });
            });

        self.ui.response(&id)
    }
}

pub fn color_picker(ui: &mut Ui, id: impl Into<String>) -> ColorPickerBuilder<'_> {
    ColorPickerBuilder::new(ui, id)
}

#[allow(clippy::too_many_arguments)]
fn color_panel(
    ui: &mut Ui,
    id: &str,
    width: f32,
    height: f32,
    open: bool,
    style: ColorPickerStyle,
    transition: Transition,
    current: Color,
    committed: Color,
    colors: Vec<Color>,
    on_change: Option<ColorChangeCallback>,
    on_open_change: Option<OpenChangeCallback>,
) {
    let pad = 24.0;
    let title_height = 60.0;
    let preview_y = title_height;
    let preview_height = 58.0;
    let sliders_y = preview_y + preview_height + 18.0;
    let slider_row_height = 34.0;
    let swatches_y = height - 48.0;
    let slider_width = 90.0_f32.max(width - pad * 2.0 - 90.0);

    ui.rect(format!("{id}.panel.bg"))
        .size(width, height)
        .color(style.surface)
        .radius(style.radius)
        .border(1.0, style.border)
        .shadow_style(style.shadow)
        .build();

    ui.rect(format!("{id}.panel.hit"))
        .size(width, height)
        .states(
            theme::color(0.0, 0.0, 0.0, 0.0),
            theme::color(0.0, 0.0, 0.0, 0.0),
            theme::color(0.0, 0.0, 0.0, 0.0),
        )
        .disabled(!open)
        .on_click(|| {})
        .on_scroll(|_| {})
        .build();

    ui.text(format!("{id}.title"))
        .x(24.0)
        .y(18.0)
        .size((width - 124.0).max(0.0), 30.0)
        .text("Color")
        .font_size(24.0)
        .line_height(29.0)
        .color(style.text)
        .build();

    let done_id = id.to_string();
    let done_on_change = on_change.clone();
    let done_on_open_change = on_open_change.clone();
    ui.rect(format!("{id}.done.bg"))
        .x((width - 86.0).max(0.0))
        .y(18.0)
        .size(62.0, 30.0)
        .states(
            style.accent,
            theme::mix_color(style.accent, theme::color(1.0, 1.0, 1.0, 1.0), 0.12),
            theme::mix_color(style.accent, theme::color(0.0, 0.0, 0.0, 1.0), 0.14),
        )
        .radius(15.0)
        .disabled(!open)
        .on_click(move || {
            let draft = color_draft(&done_id);
            emit_color(committed, draft.value, &done_on_change);
            call_open_change(&done_on_open_change, false);
        })
        .build();

    ui.text(format!("{id}.done.text"))
        .x((width - 86.0).max(0.0))
        .y(18.0)
        .size(62.0, 30.0)
        .text("Done")
        .font_size(13.0)
        .line_height(16.0)
        .color(theme::color(1.0, 1.0, 1.0, 1.0))
        .horizontal_align(HorizontalAlign::Center)
        .vertical_align(VerticalAlign::Center)
        .build();

    ui.rect(format!("{id}.preview"))
        .x(pad)
        .y(preview_y)
        .size((width - pad * 2.0).max(0.0), preview_height)
        .color(current)
        .radius(16.0)
        .shadow(18.0, 0.0, 6.0, theme::with_alpha(current, 0.24))
        .transition(transition)
        .animate(AnimProperty::COLOR | AnimProperty::SHADOW)
        .build();

    ui.text(format!("{id}.preview.hex"))
        .x(pad)
        .y(preview_y)
        .size((width - pad * 2.0 - 16.0).max(0.0), preview_height)
        .text(format_hex(current))
        .font_size(17.0)
        .line_height(22.0)
        .color(theme::color(1.0, 1.0, 1.0, 0.94))
        .horizontal_align(HorizontalAlign::Right)
        .vertical_align(VerticalAlign::Center)
        .build();

    if open {
        channel_slider(
            ui,
            id,
            0,
            "R",
            theme::color(0.92, 0.20, 0.22, 1.0),
            pad,
            sliders_y,
            slider_width,
            slider_row_height,
            current,
            style,
            transition,
        );
        channel_slider(
            ui,
            id,
            1,
            "G",
            theme::color(0.15, 0.74, 0.40, 1.0),
            pad,
            sliders_y + slider_row_height,
            slider_width,
            slider_row_height,
            current,
            style,
            transition,
        );
        channel_slider(
            ui,
            id,
            2,
            "B",
            theme::color(0.20, 0.46, 0.92, 1.0),
            pad,
            sliders_y + slider_row_height * 2.0,
            slider_width,
            slider_row_height,
            current,
            style,
            transition,
        );

        let swatches = palette(&colors, style);
        let swatch_size = 24.0;
        let swatch_gap = 8.0;
        for (index, swatch) in swatches.into_iter().enumerate() {
            let swatch_x = pad + index as f32 * (swatch_size + swatch_gap);
            if swatch_x + swatch_size > width - pad {
                break;
            }
            ui.rect(format!("{id}.swatch.border.{index}"))
                .x(swatch_x - 3.0)
                .y(swatches_y - 3.0)
                .size(swatch_size + 6.0, swatch_size + 6.0)
                .color(if same_color(swatch, current) {
                    style.accent
                } else {
                    theme::with_alpha(style.text, 0.08)
                })
                .radius(10.0)
                .transition(transition)
                .animate(AnimProperty::COLOR)
                .build();

            let draft_id = id.to_string();
            ui.rect(format!("{id}.swatch.{index}"))
                .x(swatch_x)
                .y(swatches_y)
                .size(swatch_size, swatch_size)
                .states(
                    swatch,
                    theme::mix_color(swatch, theme::color(1.0, 1.0, 1.0, 1.0), 0.16),
                    theme::mix_color(swatch, theme::color(0.0, 0.0, 0.0, 1.0), 0.14),
                )
                .radius(8.0)
                .on_click(move || set_color_draft_value(&draft_id, swatch))
                .build();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn channel_slider(
    ui: &mut Ui,
    id: &str,
    channel: i32,
    label: &str,
    fill: Color,
    x: f32,
    y: f32,
    slider_width: f32,
    row_height: f32,
    current: Color,
    style: ColorPickerStyle,
    transition: Transition,
) {
    ui.text(format!("{id}.slider.label.{channel}"))
        .x(x)
        .y(y)
        .size(24.0, row_height)
        .text(label)
        .font_size(14.0)
        .line_height(18.0)
        .color(style.text)
        .vertical_align(VerticalAlign::Center)
        .build();

    let mut slider_style = SliderStyle::default();
    slider_style.track = style.track;
    slider_style.fill = fill;
    slider_style.knob = style.knob;
    let draft_id = id.to_string();
    ui.stack(format!("{id}.slider.wrap.{channel}"))
        .x(x + 32.0)
        .y(y + 5.0)
        .size(slider_width, 22.0)
        .content(|ui| {
            slider(ui, format!("{id}.slider.{channel}"))
                .size(slider_width, 22.0)
                .value(channel_value(current, channel))
                .style(slider_style)
                .transition(transition)
                .on_change(move |value| {
                    mutate_color_draft(&draft_id, |draft| {
                        draft.value = with_channel(draft.value, channel, value);
                    });
                })
                .build();
        });

    ui.text(format!("{id}.slider.value.{channel}"))
        .x(x + 42.0 + slider_width)
        .y(y)
        .size(40.0, row_height)
        .text(channel_to_int(channel_value(current, channel)).to_string())
        .font_size(12.0)
        .line_height(16.0)
        .color(style.muted_text)
        .vertical_align(VerticalAlign::Center)
        .build();
}

fn sync_color_draft(id: &str, open: bool, value: Color) -> ColorDraft {
    COLOR_DRAFTS.with(|drafts| {
        let mut drafts = drafts.borrow_mut();
        let draft = drafts.entry(id.to_string()).or_default();
        if !open || !draft.active {
            draft.value = clamp_color(value);
            draft.active = open;
        }
        *draft
    })
}

fn color_draft(id: &str) -> ColorDraft {
    COLOR_DRAFTS.with(|drafts| drafts.borrow().get(id).copied().unwrap_or_default())
}

fn mutate_color_draft(id: &str, f: impl FnOnce(&mut ColorDraft)) {
    COLOR_DRAFTS.with(|drafts| {
        let mut drafts = drafts.borrow_mut();
        let draft = drafts.entry(id.to_string()).or_default();
        f(draft);
    });
}

fn set_color_draft_value(id: &str, value: Color) {
    mutate_color_draft(id, |draft| {
        draft.value = clamp_color(value);
    });
}

fn clamp_color(mut value: Color) -> Color {
    value.r = value.r.clamp(0.0, 1.0);
    value.g = value.g.clamp(0.0, 1.0);
    value.b = value.b.clamp(0.0, 1.0);
    value.a = 1.0;
    value
}

fn same_color(a: Color, b: Color) -> bool {
    (a.r - b.r).abs() < 0.001 && (a.g - b.g).abs() < 0.001 && (a.b - b.b).abs() < 0.001
}

fn channel_to_int(value: f32) -> i32 {
    (value.clamp(0.0, 1.0) * 255.0).round().clamp(0.0, 255.0) as i32
}

fn format_hex(color: Color) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        channel_to_int(color.r),
        channel_to_int(color.g),
        channel_to_int(color.b)
    )
}

fn channel_value(color: Color, channel: i32) -> f32 {
    match channel {
        0 => color.r,
        1 => color.g,
        _ => color.b,
    }
}

fn with_channel(mut color: Color, channel: i32, next_value: f32) -> Color {
    let next_value = next_value.clamp(0.0, 1.0);
    if channel == 0 {
        color.r = next_value;
    } else if channel == 1 {
        color.g = next_value;
    } else {
        color.b = next_value;
    }
    clamp_color(color)
}

fn emit_color(current: Color, next: Color, on_change: &Option<ColorChangeCallback>) {
    let next = clamp_color(next);
    if !same_color(current, next) {
        if let Some(callback) = on_change {
            (callback.borrow_mut())(next);
        }
    }
}

fn palette(colors: &[Color], style: ColorPickerStyle) -> Vec<Color> {
    if !colors.is_empty() {
        colors.iter().copied().map(clamp_color).collect()
    } else {
        vec![
            style.accent,
            theme::color(0.20, 0.50, 0.90, 1.0),
            theme::color(0.12, 0.72, 0.78, 1.0),
            theme::color(0.15, 0.78, 0.48, 1.0),
            theme::color(0.96, 0.68, 0.18, 1.0),
            theme::color(0.92, 0.28, 0.46, 1.0),
            theme::color(0.56, 0.36, 0.96, 1.0),
            theme::color(0.88, 0.18, 0.24, 1.0),
        ]
    }
}

fn call_open_change(callback: &Option<OpenChangeCallback>, open: bool) {
    if let Some(callback) = callback {
        (callback.borrow_mut())(open);
    }
}
