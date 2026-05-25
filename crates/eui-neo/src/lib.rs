//! Rust-native EUI-NEO port and extension.
//!
//! `eui-neo` is the host-agnostic core of the EUI-NEO-style Rust UI:
//! declarative builders, retained interaction state, layout, animation,
//! widgets, and draw-list generation. Platform input, native windows, and GPU
//! rendering live in adapter crates or engine integration layers.

mod animation;
mod binding;
mod builder;
mod color;
mod draw;
mod dsl;
mod element;
mod event;
mod fonts;
mod layout;
mod runtime;
mod skin;
mod text_measure;
pub mod widgets;

pub use crate::{
    neo_bind as bind, neo_bind_array as bind_array, neo_bind_clamped as bind_clamped,
    neo_bind_clone as bind_clone, neo_bind_eq as bind_eq, neo_bind_max as bind_max,
};
pub use animation::{
    apply_ease, has_anim_property, AnimProperty, AnimatedValue, Ease, Lerp, Motion, MotionPreset,
    SmoothedValue, SpringMotion, Transition,
};
pub use binding::{Binding, NeoState};
pub use builder::{ElementBuilder, Response};
pub use color::Color;
pub use dsl::{Screen, Ui};
pub use element::{
    Align, Border, CenterMode, CursorShape, EdgeInsets, EdgeMode, Element, ElementKind, Gradient,
    GradientDirection, HorizontalAlign, ImageFit, ImageRef, ImageRefKind, Insets,
    IntoPolygonPoints, LayoutRect, Shadow, Size, Slice, Transform, Vec2, VerticalAlign,
};
pub use event::{DragEvent, KeyboardEvent, PointerEvent, ScrollEvent};
pub use fonts::FontRef;
pub use runtime::{Frame, FrameInput, FrameResult, Runtime};
pub use skin::{ButtonSkin, CheckboxSkin, NeoSkin, PanelSkin, SkinRegistry, SliderSkin};
pub use text_measure::{DefaultTextSystem, TextMeasure, TextMeasureRequest, TextSystem};

/// Common imports for application code using `eui-neo`.
pub mod prelude {
    pub use crate::{
        bind, bind_array, bind_clamped, bind_clone, bind_eq, bind_max, widgets, Align,
        AnimProperty, Binding, Border, ButtonSkin, CenterMode, CheckboxSkin, Color, CursorShape,
        DefaultTextSystem, DragEvent, Ease, EdgeInsets, EdgeMode, FontRef, Frame, FrameInput,
        FrameResult, Gradient, GradientDirection, HorizontalAlign, ImageFit, ImageRef,
        ImageRefKind, Insets, IntoPolygonPoints, KeyboardEvent, LayoutRect, Lerp, Motion,
        MotionPreset, NeoSkin, NeoState, PanelSkin, PointerEvent, Response, Runtime, Screen,
        ScrollEvent, Shadow, Size, SkinRegistry, Slice, SliderSkin, SmoothedValue, SpringMotion,
        TextMeasure, TextMeasureRequest, TextSystem, Transform, Transition, Ui, Vec2,
        VerticalAlign,
    };
}

/// Lower-level surface for renderers, tooling, and diagnostics.
pub mod expert {
    pub use crate::draw::{
        UiDrawCommand, UiDrawList, UiImageDraw, UiNineSliceDraw, UiPolygonDraw, UiRectDraw,
        UiTextDraw,
    };
    pub use crate::event::InteractionState;
    pub use crate::layout::{layout_roots, measure_element};
    pub use crate::runtime::ElementSnapshot;
}
