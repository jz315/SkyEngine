//! Port of `EUI-NEO/components/slider.h`.

use std::cell::RefCell;
use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::Color;

use super::super::{
    AnimProperty, DragEvent, LayoutRect, PointerEvent, Response, Signal, Transition, Ui,
};
use super::theme::{self, ThemeColorTokens};

type ChangeCallback = Rc<RefCell<Box<dyn FnMut(f32)>>>;

thread_local! {
    static SLIDER_BOUNDS: RefCell<FxHashMap<String, LayoutRect>> = RefCell::new(FxHashMap::default());
}

#[derive(Debug, Clone, Copy)]
pub struct SliderStyle {
    pub track: Color,
    pub fill: Color,
    pub knob: Color,
}

impl SliderStyle {
    pub fn new(tokens: ThemeColorTokens) -> Self {
        Self {
            track: theme::mix_color(
                tokens.surface_hover,
                tokens.surface_active,
                if tokens.dark { 0.24 } else { 0.18 },
            ),
            fill: tokens.primary,
            knob: tokens.text,
        }
    }
}

impl Default for SliderStyle {
    fn default() -> Self {
        Self::new(theme::dark_theme_colors())
    }
}

pub struct SliderBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    style: SliderStyle,
    transition: Transition,
    on_change: Option<ChangeCallback>,
    width: f32,
    height: f32,
    value: f32,
}

impl<'ui> SliderBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            style: SliderStyle::default(),
            transition: Transition::snappy(),
            on_change: None,
            width: 300.0,
            height: 28.0,
            value: 0.0,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = value.clamp(0.0, 1.0);
        self
    }

    pub fn signal<T: 'static>(self, signal: Signal<T, f32>) -> Self {
        let value = signal.watch(self.ui);
        self.value(value).on_change(move |next| signal.set(next))
    }

    pub fn style(mut self, value: SliderStyle) -> Self {
        self.style = value;
        self
    }

    pub fn theme(mut self, tokens: ThemeColorTokens) -> Self {
        self.style = SliderStyle::new(tokens);
        self
    }

    pub fn transition(mut self, value: Transition) -> Self {
        self.transition = value;
        self
    }

    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: FnMut(f32) + 'static,
    {
        let next: ChangeCallback = Rc::new(RefCell::new(Box::new(callback)));
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

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let hit_id = format!("{id}.hit");
        let track_height = 3.0_f32.max(self.height * 0.18);
        let track_y = (self.height - track_height) * 0.5;
        let knob_size = 14.0_f32.max(self.height * 0.72);
        let knob_x = (self.width * self.value - knob_size * 0.5)
            .clamp(0.0, (self.width - knob_size).max(0.0));
        let on_change_press = self.on_change.clone();
        let on_change_drag = self.on_change.clone();
        let press_id = id.clone();
        let drag_id = id.clone();
        let width = self.width;

        self.ui
            .stack(id.clone())
            .size(self.width, self.height)
            .content(|ui| {
                ui.rect(format!("{id}.track"))
                    .y(track_y)
                    .size(self.width, track_height)
                    .color(self.style.track)
                    .radius(track_height * 0.5)
                    .build();

                ui.rect(format!("{id}.fill"))
                    .y(track_y)
                    .size(self.width * self.value, track_height)
                    .color(self.style.fill)
                    .radius(track_height * 0.5)
                    .transition(self.transition)
                    .animate(AnimProperty::COLOR)
                    .build();

                ui.rect(format!("{id}.knob"))
                    .x(knob_x)
                    .y((self.height - knob_size) * 0.5)
                    .size(knob_size, knob_size)
                    .color(self.style.knob)
                    .radius(knob_size * 0.5)
                    .shadow(12.0, 0.0, 4.0, theme::with_alpha(self.style.fill, 0.20))
                    .transition(self.transition)
                    .animate(AnimProperty::COLOR | AnimProperty::SHADOW)
                    .build();

                ui.rect(hit_id)
                    .size(self.width, self.height)
                    .states(
                        theme::color(0.0, 0.0, 0.0, 0.0),
                        theme::color(0.0, 0.0, 0.0, 0.0),
                        theme::color(0.0, 0.0, 0.0, 0.0),
                    )
                    .z(10)
                    .interactive(true)
                    .on_press(move |event, bounds| {
                        SLIDER_BOUNDS.with(|states| {
                            states.borrow_mut().insert(press_id.clone(), bounds);
                        });
                        let next = value_from_pointer(pointer_x(event), bounds, width);
                        if let Some(callback) = &on_change_press {
                            (callback.borrow_mut())(next);
                        }
                    })
                    .on_drag(move |event| {
                        let bounds = SLIDER_BOUNDS.with(|states| {
                            states
                                .borrow()
                                .get(&drag_id)
                                .copied()
                                .unwrap_or(LayoutRect::new(0.0, 0.0, width, 1.0))
                        });
                        let next = value_from_drag(event, bounds, width);
                        if let Some(callback) = &on_change_drag {
                            (callback.borrow_mut())(next);
                        }
                    })
                    .build();
            });

        self.ui.response(&format!("{id}.hit"))
    }
}

pub fn slider(ui: &mut Ui, id: impl Into<String>) -> SliderBuilder<'_> {
    SliderBuilder::new(ui, id)
}

fn pointer_x(event: PointerEvent) -> f32 {
    event
        .position()
        .map(|position| position[0])
        .unwrap_or(event.x)
}

fn value_from_drag(event: DragEvent, bounds: LayoutRect, width: f32) -> f32 {
    value_from_pointer(event.x, bounds, width)
}

fn value_from_pointer(pointer_x: f32, bounds: LayoutRect, width: f32) -> f32 {
    let scale = if width > 0.0 {
        bounds.width / width
    } else {
        1.0
    };
    let local_x = (pointer_x - bounds.x) / scale.max(0.001);
    (local_x / width.max(1.0)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::value_from_pointer;
    use crate::LayoutRect;

    #[test]
    fn slider_value_from_pointer_clamps_to_unit_range() {
        let bounds = LayoutRect::new(10.0, 0.0, 100.0, 20.0);
        assert_eq!(value_from_pointer(0.0, bounds, 100.0), 0.0);
        assert_eq!(value_from_pointer(110.0, bounds, 100.0), 1.0);
        assert_eq!(value_from_pointer(210.0, bounds, 100.0), 1.0);
    }
}
