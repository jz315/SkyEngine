//! Rust-native EUI-NEO port and extension.
//!
//! `eui-neo` is the host-agnostic core of the EUI-NEO-style Rust UI:
//! declarative builders, retained interaction state, layout, animation,
//! widgets, and draw-list generation. Platform input, native windows, and GPU
//! rendering live in adapter crates or engine integration layers.

pub mod agent_debug;
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
#[cfg(feature = "taffy-layout")]
mod taffy_layout;
pub mod testing;
mod text_measure;
pub mod widgets;

#[cfg(test)]
mod test_support {
    use crate::expert::DirtyInput;
    use crate::{Runtime, Screen, Ui};

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
pub use clock::{ClockTick, UiClock};
pub use color::Color;
pub use dsl::{Screen, Ui};
pub use element::{
    Align, Border, CenterMode, CursorShape, EdgeInsets, EdgeMode, Element, ElementKind, Gradient,
    GradientDirection, HorizontalAlign, ImageFit, ImageRef, ImageRefKind, Insets,
    IntoPolygonPoints, LayoutRect, Shadow, Size, Slice, Transform, UiClip, Vec2, VerticalAlign,
};
pub use event::{DragEvent, KeyboardEvent, PointerEvent, ScrollEvent};
pub use fonts::FontRef;
pub use runtime::{
    Frame, FrameInput, FrameResult, LayerCollision, OutsideClickPolicy, PlatformCaptureState,
    PlatformEffect, RendererResourceDirty, ResourceDirty, ResourceDirtySource, Runtime,
};
pub use signal::{DirtyFlags, Signal, SignalKey, State};
pub use skin::{ButtonSkin, CheckboxSkin, NeoSkin, PanelSkin, SkinRegistry, SliderSkin};
pub use text_measure::{DefaultTextSystem, TextMeasure, TextMeasureRequest, TextSystem};

/// Common imports for application code using `eui-neo`.
///
/// The prelude intentionally stays focused on authoring and host-frame basics.
/// Diagnostics, retained trace records, renderer internals, and test-driver
/// helpers are available from [`testing`] or [`expert`] instead of being pulled
/// into every application module.
pub mod prelude {
    pub use crate::widgets::PopoverPlacement;
    pub use crate::{
        widgets, Align, AnimProperty, Border, ButtonSkin, CenterMode, CheckboxSkin, ClockTick,
        Color, CursorShape, DefaultTextSystem, DirtyFlags, DragEvent, Ease, EdgeInsets, EdgeMode,
        ElementBuilder, FontRef, Frame, FrameInput, FrameResult, Gradient, GradientDirection,
        HorizontalAlign, ImageFit, ImageRef, ImageRefKind, Insets, IntoPolygonPoints,
        KeyboardEvent, LayoutRect, Lerp, Motion, MotionPreset, NeoSkin, OutsideClickPolicy,
        PanelSkin, PointerEvent, Response, Runtime, Screen, ScrollEvent, Shadow, Signal, SignalKey,
        Size, SkinRegistry, Slice, SliderSkin, SmoothedValue, SpringMotion, State, TextMeasure,
        TextMeasureRequest, TextSystem, Transform, Transition, Ui, UiClip, UiClock, Vec2,
        VerticalAlign,
    };
}

/// Lower-level surface for renderers, tooling, and diagnostics.
pub mod expert {
    pub use crate::agent_debug::{
        AgentDebugContext, AgentDebugEffect, AgentDebugEndpoint, AgentDebugService,
        AgentDropdownState,
    };
    pub use crate::cache::{CacheAccess, CacheCell, CacheStats};
    pub use crate::diagnostics::{UiDrawDebugCommand, UiDrawDebugTrace};
    pub use crate::draw::{
        UiDrawCommand, UiDrawList, UiImageDraw, UiNineSliceDraw, UiPolygonDraw, UiRectDraw,
        UiTextDraw,
    };
    pub use crate::element::{Element, ElementKind};
    pub use crate::event::InteractionState;
    pub use crate::layout::{layout_roots, measure_element};
    pub use crate::retained::{
        CallbackTransferStats, FullLayoutReason, LayoutMode, RetainedComposeAction,
        RetainedComposeEvent, RetainedComposeReason, RetainedComposeStats, ScopeId,
    };
    pub use crate::runtime::{
        DirtyInput, DirtyReason, ElementDebugRecord, ElementSnapshot, EventDebugRecord,
        EventDebugSource, EventSource, EventTargetId, InputOwnerDebugSnapshot, Invalidation,
        InvalidationPropagation, InvalidationSource, InvalidationTarget, LayerAnchorSource,
        LayerCollision, LayerDebugRecord, LayerDismissalRecord, LayerId, LayerIntent, LayerKind,
        LayerLifecycleAction, LayerPlacement, LayerPointerAction, LayerPointerDebugRecord,
        LayerSize, NodeId, PassFlags, PlatformCaptureState, PlatformEffect, RendererResourceDirty,
        ResourceDirtySource, RetainedDebugRecord, TimerSource, UiDebugSnapshot,
    };

    #[derive(Clone, Copy)]
    pub struct RuntimeDiagnostics<'a> {
        runtime: &'a crate::Runtime,
    }

    impl<'a> RuntimeDiagnostics<'a> {
        pub(crate) fn new(runtime: &'a crate::Runtime) -> Self {
            Self { runtime }
        }

        pub fn roots(&self) -> &'a [crate::Element] {
            self.runtime.diagnostic_roots()
        }

        pub fn find(&self, id: &str) -> Option<&'a crate::Element> {
            self.runtime.diagnostic_find(id)
        }

        pub fn response(&self, id: &str) -> crate::Response {
            self.runtime.diagnostic_response(id)
        }

        pub fn interaction(&self, id: &str) -> crate::event::InteractionState {
            self.runtime.diagnostic_interaction(id)
        }

        pub fn focused_id(&self) -> Option<&'a str> {
            self.runtime.diagnostic_focused_id()
        }

        pub fn text_focused_id(&self) -> Option<&'a str> {
            self.runtime.diagnostic_text_focused_id()
        }

        pub fn committed_snapshot(&self) -> &'a UiDebugSnapshot {
            self.runtime.diagnostic_committed_snapshot()
        }

        pub fn current_snapshot(&self) -> UiDebugSnapshot {
            self.runtime.diagnostic_current_snapshot()
        }

        pub fn draw_trace(&self) -> UiDrawDebugTrace {
            self.runtime.diagnostic_draw_trace()
        }
    }
}
