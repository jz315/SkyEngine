//! Rust-native EUI-NEO port and extension.
//!
//! `eui-neo` is the host-agnostic core of the EUI-NEO-style Rust UI:
//! declarative builders, retained interaction state, layout, animation,
//! widgets, and draw-list generation. Platform input, native windows, and GPU
//! rendering live in adapter crates or engine integration layers.

mod animation;
mod builder;
mod cache;
mod color;
mod diagnostics;
mod draw;
mod dsl;
mod element;
mod event;
mod fonts;
mod layout;
mod retained;
mod runtime;
mod signal;
mod skin;
pub mod testing;
mod text_measure;
pub mod widgets;

pub use animation::{
    apply_ease, has_anim_property, AnimProperty, AnimatedValue, Ease, Lerp, Motion, MotionPreset,
    SmoothedValue, SpringMotion, Transition,
};
pub use builder::{ElementBuilder, Response};
pub use cache::{CacheAccess, CacheCell, CacheStats};
pub use color::Color;
pub use diagnostics::{UiDrawDebugCommand, UiDrawDebugTrace};
pub use dsl::{Screen, Ui};
pub use element::{
    Align, Border, CenterMode, CursorShape, EdgeInsets, EdgeMode, Element, ElementKind, Gradient,
    GradientDirection, HorizontalAlign, ImageFit, ImageRef, ImageRefKind, Insets,
    IntoPolygonPoints, LayoutRect, Shadow, Size, Slice, Transform, UiClip, Vec2, VerticalAlign,
};
pub use event::{DragEvent, KeyboardEvent, PointerEvent, ScrollEvent};
pub use fonts::FontRef;
pub use retained::{
    FullLayoutReason, LayoutMode, ScopeComposeAction, ScopeComposeEvent, ScopeComposeStats,
};
pub use runtime::{Frame, FrameInput, FrameResult, Runtime, UiDebugSnapshot};
pub use signal::{DirtyFlags, Signal, SignalKey, State};
pub use skin::{ButtonSkin, CheckboxSkin, NeoSkin, PanelSkin, SkinRegistry, SliderSkin};
pub use testing::{TargetPoint, UiActionTrace, UiTestDriver, UiTestError};
pub use text_measure::{DefaultTextSystem, TextMeasure, TextMeasureRequest, TextSystem};

/// Common imports for application code using `eui-neo`.
pub mod prelude {
    pub use crate::widgets::PopoverPlacement;
    pub use crate::{
        widgets, Align, AnimProperty, Border, ButtonSkin, CenterMode, CheckboxSkin, Color,
        CursorShape, DefaultTextSystem, DragEvent, Ease, EdgeInsets, EdgeMode, FontRef, Frame,
        FrameInput, FrameResult, FullLayoutReason, Gradient, GradientDirection, HorizontalAlign,
        ImageFit, ImageRef, ImageRefKind, Insets, IntoPolygonPoints, KeyboardEvent, LayoutMode,
        LayoutRect, Lerp, Motion, MotionPreset, NeoSkin, PanelSkin, PointerEvent, Response,
        Runtime, ScopeComposeAction, ScopeComposeEvent, ScopeComposeStats, Screen, ScrollEvent,
        Shadow, Signal, SignalKey, Size, SkinRegistry, Slice, SliderSkin, SmoothedValue,
        SpringMotion, State, TargetPoint, TextMeasure, TextMeasureRequest, TextSystem, Transform,
        Transition, Ui, UiActionTrace, UiClip, UiDebugSnapshot, UiDrawDebugCommand,
        UiDrawDebugTrace, UiTestDriver, UiTestError, Vec2, VerticalAlign,
    };
}

/// Lower-level surface for renderers, tooling, and diagnostics.
pub mod expert {
    pub use crate::cache::{CacheAccess, CacheCell, CacheStats};
    pub use crate::diagnostics::{UiDrawDebugCommand, UiDrawDebugTrace};
    pub use crate::draw::{
        UiDrawCommand, UiDrawList, UiImageDraw, UiNineSliceDraw, UiPolygonDraw, UiRectDraw,
        UiTextDraw,
    };
    pub use crate::event::InteractionState;
    pub use crate::layout::{layout_roots, measure_element};
    pub use crate::runtime::ElementSnapshot;
}
