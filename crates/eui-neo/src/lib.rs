//! Rust-native EUI-NEO port and extension.
//!
//! `eui-neo` is the host-agnostic core of the EUI-NEO-style Rust UI:
//! declarative builders, retained interaction state, layout, animation,
//! widgets, and draw-list generation. Platform input, native windows, and GPU
//! rendering live in adapter crates or engine integration layers.

mod animation;
mod builder;
mod cache;
mod callbacks;
mod clock;
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

#[cfg(test)]
mod test_support {
    use crate::{DirtyInput, Runtime, Screen, Ui};

    pub(crate) fn compose<R>(
        runtime: &mut Runtime,
        width: f32,
        height: f32,
        build: impl FnOnce(&mut Ui, Screen) -> R,
    ) -> R {
        let mut value = None;
        runtime.compose_tree_with_dirty(width, height, None, |ui, screen| {
            value = Some(build(ui, screen));
        });
        value.expect("test compose closure did not run")
    }

    pub(crate) fn compose_incremental_dirty<R>(
        runtime: &mut Runtime,
        width: f32,
        height: f32,
        dirty: impl IntoIterator<Item = DirtyInput>,
        build: impl FnOnce(&mut Ui, Screen) -> R,
    ) -> R {
        let dirty = dirty.into_iter().collect();
        let mut value = None;
        runtime.compose_tree_with_dirty(width, height, Some(dirty), |ui, screen| {
            value = Some(build(ui, screen));
        });
        value.expect("test compose closure did not run")
    }
}

pub use animation::{
    apply_ease, has_anim_property, AnimProperty, AnimatedValue, Ease, Lerp, Motion, MotionPreset,
    SmoothedValue, SpringMotion, Transition,
};
pub use builder::{ElementBuilder, Response};
pub use cache::{CacheAccess, CacheCell, CacheStats};
pub use clock::{ClockTick, UiClock};
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
    FullLayoutReason, LayoutMode, RetainedComposeAction, RetainedComposeEvent,
    RetainedComposeReason, RetainedComposeStats,
};
pub use runtime::{
    DirtyInput, DirtyReason, ElementDebugRecord, Frame, FrameInput, FrameResult, Invalidation,
    InvalidationPropagation, InvalidationSource, InvalidationTarget, PassFlags,
    RetainedDebugRecord, Runtime, UiDebugSnapshot,
};
pub use signal::{DirtyFlags, Signal, SignalKey, State};
pub use skin::{ButtonSkin, CheckboxSkin, NeoSkin, PanelSkin, SkinRegistry, SliderSkin};
pub use testing::{TargetPoint, UiActionTrace, UiTestDriver, UiTestError};
pub use text_measure::{DefaultTextSystem, TextMeasure, TextMeasureRequest, TextSystem};

/// Common imports for application code using `eui-neo`.
pub mod prelude {
    pub use crate::widgets::PopoverPlacement;
    pub use crate::{
        widgets, Align, AnimProperty, Border, ButtonSkin, CenterMode, CheckboxSkin, ClockTick,
        Color, CursorShape, DefaultTextSystem, DirtyInput, DirtyReason, DragEvent, Ease,
        EdgeInsets, EdgeMode, ElementDebugRecord, FontRef, Frame, FrameInput, FrameResult,
        FullLayoutReason, Gradient, GradientDirection, HorizontalAlign, ImageFit, ImageRef,
        ImageRefKind, Insets, IntoPolygonPoints, Invalidation, InvalidationPropagation,
        InvalidationSource, InvalidationTarget, KeyboardEvent, LayoutMode, LayoutRect, Lerp,
        Motion, MotionPreset, NeoSkin, PanelSkin, PassFlags, PointerEvent, Response,
        RetainedComposeAction, RetainedComposeEvent, RetainedComposeReason, RetainedComposeStats,
        RetainedDebugRecord, Runtime, Screen, ScrollEvent, Shadow, Signal, SignalKey, Size,
        SkinRegistry, Slice, SliderSkin, SmoothedValue, SpringMotion, State, TargetPoint,
        TextMeasure, TextMeasureRequest, TextSystem, Transform, Transition, Ui, UiActionTrace,
        UiClip, UiClock, UiDebugSnapshot, UiDrawDebugCommand, UiDrawDebugTrace, UiTestDriver,
        UiTestError, Vec2, VerticalAlign,
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
