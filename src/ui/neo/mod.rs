//! Experimental EUI-NEO-inspired declarative game UI.
//!
//! This module ports the EUI-NEO authoring and runtime model into SkyEngine:
//! small core elements, stable IDs, declarative builders, runtime-owned layout,
//! interaction, and animation state. Rendering and platform input are adapted
//! to SkyEngine instead of copying EUI-NEO's OpenGL/GLFW backend.

#![allow(non_snake_case)]

mod animation;
mod api;
mod backend;
mod binding;
mod builder;
mod config;
mod draw;
mod dsl;
mod element;
mod event;
mod fonts;
mod input_bridge;
mod layout;
mod plugin;
mod renderer;
mod runtime;
pub mod widgets;
mod window;

pub use crate::{
    neo_bind as bind, neo_bind_array as bind_array, neo_bind_clamped as bind_clamped,
    neo_bind_clone as bind_clone, neo_bind_eq as bind_eq, neo_bind_max as bind_max,
};
pub use animation::{
    applyEase, apply_ease, hasAnimProperty, has_anim_property, AnimProperty, AnimatedValue, Ease,
    Lerp, SmoothedValue, Transition,
};
pub use api::{compose, open_window};
pub use backend::NeoUiBackend;
pub use binding::{Binding, NeoState};
pub use builder::{ElementBuilder, Response};
pub use config::{NeoUiConfig, NeoWindowConfig};
pub use draw::{UiDrawCommand, UiDrawList, UiImageDraw, UiPolygonDraw, UiRectDraw, UiTextDraw};
pub use dsl::{Screen, Ui};
pub use element::{
    Align, Border, CursorShape, EdgeInsets, Element, ElementKind, Gradient, GradientDirection,
    HorizontalAlign, ImageFit, IntoPolygonPoints, LayoutRect, Rect, Shadow, Size, Transform, Vec2,
    VerticalAlign,
};
pub use event::{DragEvent, InteractionState, KeyboardEvent, PointerEvent, ScrollEvent};
pub use layout::{layout_roots, measure_element};
pub use plugin::{install_neo_ui_backend, NeoUiPlugin};
pub use renderer::{NeoRenderStatus, NeoRenderer};
pub use runtime::{ElementSnapshot, NeoRuntime};
