use std::any::Any;

use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::input::Input;
use crate::render::SharedRenderAssetCache;
use winit::event::WindowEvent;
use winit::window::Window;

/// Stable identifier for a UI backend installed in [`UiHost`](super::UiHost).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UiBackendId(&'static str);

impl UiBackendId {
    pub const fn new(value: &'static str) -> Self {
        Self(value)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// Aggregated input-capture state reported by UI backends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiCaptureState {
    pub wants_pointer: bool,
    pub wants_keyboard: bool,
}

impl UiCaptureState {
    pub const fn new(wants_pointer: bool, wants_keyboard: bool) -> Self {
        Self {
            wants_pointer,
            wants_keyboard,
        }
    }

    pub fn merge(&mut self, other: Self) {
        self.wants_pointer |= other.wants_pointer;
        self.wants_keyboard |= other.wants_keyboard;
    }
}

/// Context passed to UI backends at the start of a frame.
pub struct UiBeginFrameContext<'a> {
    pub world: &'a mut World,
    pub input: &'a Input,
    pub window: Option<&'a Window>,
    pub logical_surface_size: [f32; 2],
    pub physical_surface_size: [f32; 2],
    pub scale_factor: f32,
}

/// Context passed to UI backends for raw window events.
pub struct UiEventContext<'a> {
    pub world: &'a mut World,
    pub window: Option<&'a Window>,
    pub event: &'a WindowEvent,
    pub scale_factor: f32,
}

/// Input handling result from a UI backend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UiEventResponse {
    pub consumed: bool,
}

impl UiEventResponse {
    pub const fn ignored() -> Self {
        Self { consumed: false }
    }

    pub const fn consumed() -> Self {
        Self { consumed: true }
    }

    pub fn merge(&mut self, other: Self) {
        self.consumed |= other.consumed;
    }
}

/// Context passed to UI backends when overlay rendering is requested.
pub struct UiRenderContext<'a> {
    pub world: &'a mut World,
    pub gpu: &'a mut GpuContext,
    pub render_assets: Option<&'a SharedRenderAssetCache>,
}

/// Error returned by pluggable UI backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiError {
    message: String,
}

impl UiError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for UiError {}

/// Minimal lifecycle contract for a pluggable game UI backend.
pub trait UiBackend: 'static {
    fn id(&self) -> UiBackendId;

    fn name(&self) -> &'static str {
        self.id().as_str()
    }

    fn handle_event(&mut self, _ctx: UiEventContext<'_>) -> UiEventResponse {
        UiEventResponse::ignored()
    }

    fn begin_frame(&mut self, ctx: UiBeginFrameContext<'_>);

    fn render_overlay(&mut self, ctx: UiRenderContext<'_>) -> Result<(), UiError>;

    fn capture(&self) -> UiCaptureState;

    fn as_any_mut(&mut self) -> &mut dyn Any;
}
