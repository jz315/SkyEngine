use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::time::Duration;

use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use smallvec::SmallVec;

use super::cache::CacheCell;
use super::callbacks::UiCallbacks;
use super::clock::ClockPeriodMap;
use super::event::InteractionState;
use super::fonts::FontRef;
use super::retained::{
    FullLayoutReason, LayoutMode, RetainedComposeAction, RetainedComposeEvent,
    RetainedComposeReason, RetainedComposeStats, RetainedRoot, ScopeComposeRecord, ScopeId,
    ScopeRoots, ScopeSet,
};
use super::skin::{NeoSkin, SkinRegistry};
use super::text_measure::{DefaultTextSystem, TextSystem};
use super::Color;
use super::{
    AnimProperty, AnimatedValue, Border, CursorShape, DragEvent, Element, ElementKind,
    KeyboardEvent, LayoutRect, Motion, PointerEvent, Response, Screen, ScrollEvent, Shadow,
    SmoothedValue, Transform, Transition, Ui, UiClip,
};
use crate::{DirtyFlags, SignalKey};
mod animation;
mod composition;
mod debug;
mod dirty;
mod event_command;
mod focus;
mod frame;
mod ids;
mod interaction;
mod invalidation;
mod layers;
mod layout;
mod platform;
pub(crate) mod reconcile;
mod resources;
mod timing;
mod tree;

use animation::{ElementAnimation, FrameTargetState};
use dirty::DrawListCacheKey;
use ids::InputOwners;
pub use ids::{EventTargetId, NodeId};
use invalidation::InvalidationStore;
pub use invalidation::{
    EventSource, Invalidation, InvalidationPropagation, InvalidationSource, InvalidationTarget,
    PassFlags, TimerSource,
};
use layers::LayerRuntimeIntent;
pub use layers::{
    LayerAnchorSource, LayerCollision, LayerDebugRecord, LayerDismissalRecord, LayerId,
    LayerIntent, LayerKind, LayerLifecycleAction, LayerPlacement, LayerPointerAction,
    LayerPointerDebugRecord, LayerSize, OutsideClickPolicy,
};
pub(crate) use layers::{ScopeLayerIntents, ScopeLayerRoots};
#[cfg(test)]
use reconcile::refresh_scope_roots_from_tree;
use timing::TimerState;

/// Compact structure snapshot used to detect tree-level and paint changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementSnapshot {
    pub id: NodeId,
    pub kind: ElementKind,
    pub z_index: i32,
    pub clip: bool,
    pub clip_radius_bits: u32,
    pub child_count: usize,
    pub layout_signature: u64,
    pub visual_signature: u64,
}

impl ElementSnapshot {
    pub fn id(&self) -> &str {
        self.id.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirtyReason {
    External,
    Live,
    Clock,
    Descendant,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetainedDebugRecord {
    scope_id: ScopeId,
    parent_scope_id: Option<ScopeId>,
    scroll_ancestor_id: Option<NodeId>,
    clip_ancestor_id: Option<NodeId>,
    pub id: String,
    pub parent_id: Option<String>,
    pub dirty: bool,
    pub raw_dirty: bool,
    pub normalized_dirty_root: bool,
    pub dirty_reasons: Vec<DirtyReason>,
    pub action: Option<RetainedComposeAction>,
    pub compose_reason: Option<RetainedComposeReason>,
    pub previous_roots: usize,
    pub current_roots: usize,
    pub layout_anchor: Option<LayoutRect>,
    pub scroll_ancestor: Option<String>,
    pub clip_ancestor: Option<String>,
}

impl RetainedDebugRecord {
    pub fn scope_id(&self) -> &ScopeId {
        &self.scope_id
    }

    pub fn parent_scope_id(&self) -> Option<&ScopeId> {
        self.parent_scope_id.as_ref()
    }

    pub fn scroll_ancestor_id(&self) -> Option<&NodeId> {
        self.scroll_ancestor_id.as_ref()
    }

    pub fn clip_ancestor_id(&self) -> Option<&NodeId> {
        self.clip_ancestor_id.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ElementDebugRecord {
    node_id: NodeId,
    parent_id: Option<NodeId>,
    retained_boundary_id: Option<ScopeId>,
    scroll_ancestor_id: Option<NodeId>,
    clip_ancestor_id: Option<NodeId>,
    pub id: String,
    pub parent: Option<String>,
    pub retained_boundary: Option<String>,
    pub scroll_ancestor: Option<String>,
    pub clip_ancestor: Option<String>,
    pub target_frame: LayoutRect,
    pub draw_frame: Option<LayoutRect>,
    pub transformed_draw_frame: Option<LayoutRect>,
    pub draw_transform: Transform,
    pub active_clip: Option<UiClip>,
    pub draw_visible: bool,
}

impl ElementDebugRecord {
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    pub fn parent_id(&self) -> Option<&NodeId> {
        self.parent_id.as_ref()
    }

    pub fn retained_boundary_id(&self) -> Option<&ScopeId> {
        self.retained_boundary_id.as_ref()
    }

    pub fn scroll_ancestor_id(&self) -> Option<&NodeId> {
        self.scroll_ancestor_id.as_ref()
    }

    pub fn clip_ancestor_id(&self) -> Option<&NodeId> {
        self.clip_ancestor_id.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDebugRecord {
    pub source: EventDebugSource,
    pub raw_event: &'static str,
    pub target: EventTargetId,
    pub command: &'static str,
    pub callback: bool,
    pub invalidation: Option<Invalidation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventDebugSource {
    Event(EventSource),
    Timer(TimerSource),
    Runtime {
        raw_event: &'static str,
        command: &'static str,
    },
}

impl EventDebugSource {
    pub fn raw_event(self) -> &'static str {
        match self {
            Self::Event(source) => source.raw_event(),
            Self::Timer(source) => source.raw_event(),
            Self::Runtime { raw_event, .. } => raw_event,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Event(source) => source.label(),
            Self::Timer(source) => source.label(),
            Self::Runtime { command, .. } => command,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlatformEffect {
    ImeStart { rect: LayoutRect },
    ImeMove { rect: LayoutRect },
    ImeEnd,
    CursorShape { shape: CursorShape },
    Capture { state: PlatformCaptureState },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlatformCaptureState {
    pub wants_pointer: bool,
    pub wants_keyboard: bool,
}

impl PlatformCaptureState {
    pub const fn new(wants_pointer: bool, wants_keyboard: bool) -> Self {
        Self {
            wants_pointer,
            wants_keyboard,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameInputDebugRecord {
    pub input_state_changed: bool,
    pub timer_render_requested: bool,
    pub command_count: usize,
    pub callback_count: usize,
    pub invalidation_count: usize,
    pub pass_flags: PassFlags,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputOwnerDebugSnapshot {
    pub pointer_hover: Option<NodeId>,
    pub pointer_active: Option<NodeId>,
    pub pointer_capture: Option<NodeId>,
    pub keyboard_focus: Option<NodeId>,
    pub text_focus: Option<NodeId>,
    pub ime_owner: Option<NodeId>,
    pub scroll_owner: Option<NodeId>,
    pub drag_owner: Option<NodeId>,
}

#[derive(Clone, PartialEq)]
pub struct UiDebugSnapshot {
    pub frame_index: u64,
    pub screen: Screen,
    pub dirty_ids: Vec<String>,
    pub normalized_dirty_ids: Vec<String>,
    pub live_ids: Vec<String>,
    pub clock_ids: Vec<String>,
    pub dirty_scope_ids: Vec<ScopeId>,
    pub normalized_dirty_scope_ids: Vec<ScopeId>,
    pub live_scope_ids: Vec<ScopeId>,
    pub clock_scope_ids: Vec<ScopeId>,
    pub retained: Vec<RetainedDebugRecord>,
    pub elements: Vec<ElementDebugRecord>,
    pub events: Vec<EventDebugRecord>,
    pub input_pass: Option<FrameInputDebugRecord>,
    pub platform_effects: Vec<PlatformEffect>,
    pub layers: Vec<LayerDebugRecord>,
    pub layer_dismissals: Vec<LayerDismissalRecord>,
    pub layer_pointer: Vec<LayerPointerDebugRecord>,
    pub retained_events: Vec<RetainedComposeEvent>,
    pub scope_compose: Vec<ScopeComposeRecord>,
    pub retained_stats: RetainedComposeStats,
    pub layout_mode: LayoutMode,
    pub needs_render: bool,
    pub needs_compose: bool,
    pub full_redraw: bool,
    pub input_owners: InputOwnerDebugSnapshot,
    pub hovered_node_id: Option<NodeId>,
    pub focused_id: Option<String>,
    pub active_id: Option<String>,
    pub hovered_id: Option<String>,
    pub pointer_hover_id: Option<String>,
    pub pointer_active_id: Option<String>,
    pub pointer_capture_id: Option<String>,
    pub keyboard_focus_id: Option<String>,
    pub text_focus_id: Option<String>,
    pub ime_owner_id: Option<String>,
    pub scroll_owner_id: Option<String>,
    pub drag_owner_id: Option<String>,
    pub active_animation_count: usize,
    pub invalidations: Vec<Invalidation>,
    pub pass_flags: PassFlags,
}

impl std::fmt::Debug for UiDebugSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiDebugSnapshot")
            .field("frame_index", &self.frame_index)
            .field("screen", &self.screen)
            .field("dirty_ids", &self.dirty_ids)
            .field("normalized_dirty_ids", &self.normalized_dirty_ids)
            .field("live_ids", &self.live_ids)
            .field("clock_ids", &self.clock_ids)
            .field("dirty_scope_ids", &self.dirty_scope_ids)
            .field(
                "normalized_dirty_scope_ids",
                &self.normalized_dirty_scope_ids,
            )
            .field("live_scope_ids", &self.live_scope_ids)
            .field("clock_scope_ids", &self.clock_scope_ids)
            .field("retained", &self.retained)
            .field("element_count", &self.elements.len())
            .field("events", &self.events)
            .field("input_pass", &self.input_pass)
            .field("platform_effects", &self.platform_effects)
            .field("layers", &self.layers)
            .field("layer_dismissals", &self.layer_dismissals)
            .field("layer_pointer", &self.layer_pointer)
            .field("retained_events", &self.retained_events)
            .field("scope_compose", &self.scope_compose)
            .field("retained_stats", &self.retained_stats)
            .field("layout_mode", &self.layout_mode)
            .field("needs_render", &self.needs_render)
            .field("needs_compose", &self.needs_compose)
            .field("full_redraw", &self.full_redraw)
            .field("input_owners", &self.input_owners)
            .field("hovered_node_id", &self.hovered_node_id)
            .field("focused_id", &self.focused_id)
            .field("active_id", &self.active_id)
            .field("hovered_id", &self.hovered_id)
            .field("pointer_hover_id", &self.pointer_hover_id)
            .field("pointer_active_id", &self.pointer_active_id)
            .field("pointer_capture_id", &self.pointer_capture_id)
            .field("keyboard_focus_id", &self.keyboard_focus_id)
            .field("text_focus_id", &self.text_focus_id)
            .field("ime_owner_id", &self.ime_owner_id)
            .field("scroll_owner_id", &self.scroll_owner_id)
            .field("drag_owner_id", &self.drag_owner_id)
            .field("active_animation_count", &self.active_animation_count)
            .field("invalidations", &self.invalidations)
            .field("pass_flags", &self.pass_flags)
            .finish()
    }
}

impl Default for UiDebugSnapshot {
    fn default() -> Self {
        Self {
            frame_index: 0,
            screen: Screen::default(),
            dirty_ids: Vec::new(),
            normalized_dirty_ids: Vec::new(),
            live_ids: Vec::new(),
            clock_ids: Vec::new(),
            dirty_scope_ids: Vec::new(),
            normalized_dirty_scope_ids: Vec::new(),
            live_scope_ids: Vec::new(),
            clock_scope_ids: Vec::new(),
            retained: Vec::new(),
            elements: Vec::new(),
            events: Vec::new(),
            input_pass: None,
            platform_effects: Vec::new(),
            layers: Vec::new(),
            layer_dismissals: Vec::new(),
            layer_pointer: Vec::new(),
            retained_events: Vec::new(),
            scope_compose: Vec::new(),
            retained_stats: RetainedComposeStats::default(),
            layout_mode: LayoutMode::Full(FullLayoutReason::RetainedReuseUnavailable),
            needs_render: false,
            needs_compose: false,
            full_redraw: false,
            input_owners: InputOwnerDebugSnapshot::default(),
            hovered_node_id: None,
            focused_id: None,
            active_id: None,
            hovered_id: None,
            pointer_hover_id: None,
            pointer_active_id: None,
            pointer_capture_id: None,
            keyboard_focus_id: None,
            text_focus_id: None,
            ime_owner_id: None,
            scroll_owner_id: None,
            drag_owner_id: None,
            active_animation_count: 0,
            invalidations: Vec::new(),
            pass_flags: PassFlags::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyInput {
    id: ScopeId,
    flags: DirtyFlags,
    source: Option<SignalKey>,
}

impl DirtyInput {
    /// Create an external dirty record for a retained scope.
    pub fn new(id: impl Into<String>, flags: DirtyFlags) -> Self {
        Self {
            id: ScopeId::new(id),
            flags,
            source: None,
        }
    }

    /// Create a signal-owned dirty record for a retained scope.
    pub fn signal(id: impl Into<String>, source: impl Into<SignalKey>, flags: DirtyFlags) -> Self {
        Self {
            id: ScopeId::new(id),
            flags,
            source: Some(source.into()),
        }
    }

    pub(crate) fn signal_scope(
        id: ScopeId,
        source: impl Into<SignalKey>,
        flags: DirtyFlags,
    ) -> Self {
        Self {
            id,
            flags,
            source: Some(source.into()),
        }
    }

    /// Readable retained scope id targeted by this dirty record.
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    /// Typed retained scope id targeted by this dirty record.
    pub fn scope_id(&self) -> &ScopeId {
        &self.id
    }

    /// Dirty flags requested by this record.
    pub fn flags(&self) -> DirtyFlags {
        self.flags
    }

    /// Optional signal source label for signal-owned dirty records.
    pub fn source(&self) -> Option<&str> {
        self.source.as_ref().map(SignalKey::as_str)
    }

    /// Optional typed signal source for signal-owned dirty records.
    pub fn source_key(&self) -> Option<&SignalKey> {
        self.source.as_ref()
    }
}

/// Host or renderer resource readiness request for the runtime invalidation spine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceDirty {
    source: ResourceDirtySource,
    flags: DirtyFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RendererResourceDirty {
    PendingImages,
    PendingFonts,
    ReadyImages,
    ReadyFonts,
}

impl RendererResourceDirty {
    pub fn label(self) -> &'static str {
        match self {
            Self::PendingImages => "renderer:pending_images",
            Self::PendingFonts => "renderer:pending_fonts",
            Self::ReadyImages => "renderer:ready_images",
            Self::ReadyFonts => "renderer:ready_fonts",
        }
    }

    pub fn from_label(value: &str) -> Option<Self> {
        match value {
            "renderer:pending_images" => Some(Self::PendingImages),
            "renderer:pending_fonts" => Some(Self::PendingFonts),
            "renderer:ready_images" => Some(Self::ReadyImages),
            "renderer:ready_fonts" => Some(Self::ReadyFonts),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceDirtySource(String);

impl ResourceDirtySource {
    pub fn new(source: impl Into<String>) -> Self {
        Self(source.into())
    }

    pub fn renderer(source: RendererResourceDirty) -> Self {
        Self::new(source.label())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn renderer_kind(&self) -> Option<RendererResourceDirty> {
        RendererResourceDirty::from_label(self.as_str())
    }
}

impl std::fmt::Display for ResourceDirtySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for ResourceDirtySource {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ResourceDirtySource {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<RendererResourceDirty> for ResourceDirtySource {
    fn from(value: RendererResourceDirty) -> Self {
        Self::renderer(value)
    }
}

impl ResourceDirty {
    /// Create a resource dirty request with explicit invalidation flags.
    pub fn new(source: impl Into<ResourceDirtySource>, flags: DirtyFlags) -> Self {
        Self {
            source: source.into(),
            flags,
        }
    }

    /// Create a draw-only resource dirty request.
    pub fn draw(source: impl Into<ResourceDirtySource>) -> Self {
        Self::new(source, DirtyFlags::DRAW)
    }

    /// Create a layout-affecting resource dirty request.
    pub fn layout(source: impl Into<ResourceDirtySource>) -> Self {
        Self::new(
            source,
            DirtyFlags::COMPOSE | DirtyFlags::LAYOUT | DirtyFlags::DRAW,
        )
    }

    /// Resource source label recorded in debug invalidation traces.
    pub fn source(&self) -> &str {
        self.source.as_str()
    }

    /// Typed resource dirty source carried by invalidation traces.
    pub fn source_id(&self) -> &ResourceDirtySource {
        &self.source
    }

    /// Dirty flags requested by this resource transition.
    pub fn flags(&self) -> DirtyFlags {
        self.flags
    }
}

/// Complete host-provided input snapshot for one UI frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameInput {
    screen: Screen,
    delta_seconds: f32,
    pointer: PointerEvent,
    pointer_events: Vec<PointerEvent>,
    scroll: ScrollEvent,
    keyboard: KeyboardEvent,
    dirty: Option<Vec<DirtyInput>>,
    force_full_compose: bool,
}

impl FrameInput {
    pub fn new(screen: Screen, delta_seconds: f32) -> Self {
        Self {
            screen,
            delta_seconds,
            ..Self::default()
        }
    }

    pub fn pointer(mut self, value: PointerEvent) -> Self {
        self.pointer = value;
        self
    }

    pub fn pointer_events(mut self, value: impl IntoIterator<Item = PointerEvent>) -> Self {
        self.pointer_events = value.into_iter().collect();
        self
    }

    pub fn scroll(mut self, value: ScrollEvent) -> Self {
        self.scroll = value;
        self
    }

    pub fn keyboard(mut self, value: KeyboardEvent) -> Self {
        self.keyboard = value;
        self
    }

    #[cfg(test)]
    pub(crate) fn dirty(mut self, value: impl IntoIterator<Item = DirtyInput>) -> Self {
        self.dirty = Some(value.into_iter().collect());
        self
    }

    pub fn force_full_compose(mut self, value: bool) -> Self {
        self.force_full_compose = value;
        self
    }

    /// Screen metrics used by this frame.
    pub fn screen(&self) -> Screen {
        self.screen
    }

    /// Delta time, in seconds, used for timers and animation sampling.
    pub fn delta_seconds(&self) -> f32 {
        self.delta_seconds
    }

    /// Coalesced pointer snapshot used when no explicit pointer event queue exists.
    pub fn pointer_snapshot(&self) -> PointerEvent {
        self.pointer
    }

    /// Ordered pointer events collected for this frame.
    pub fn queued_pointer_events(&self) -> &[PointerEvent] {
        &self.pointer_events
    }

    /// Scroll input collected for this frame.
    pub fn scroll_event(&self) -> ScrollEvent {
        self.scroll
    }

    /// Keyboard/text input collected for this frame.
    pub fn keyboard_event(&self) -> &KeyboardEvent {
        &self.keyboard
    }

    /// Whether this frame requests the diagnostic full-compose path.
    pub fn force_full_compose_enabled(&self) -> bool {
        self.force_full_compose
    }
}

/// Host-facing result of a completed UI frame.
#[derive(Debug, Clone)]
pub struct Frame {
    pub screen: Screen,
    pub draw_list: super::draw::UiDrawList,
    pub needs_render: bool,
    pub needs_compose: bool,
    pub full_redraw: bool,
    pub focused_ime_rect: Option<LayoutRect>,
}

impl Frame {
    pub fn draw_list(&self) -> &super::draw::UiDrawList {
        &self.draw_list
    }
}

/// Return value from [`Runtime::frame`].
#[derive(Debug, Clone)]
pub struct FrameResult<R = ()> {
    pub value: R,
    pub frame: Frame,
}

impl<R> FrameResult<R> {
    pub fn into_parts(self) -> (R, Frame) {
        (self.value, self.frame)
    }
}

/// EUI-NEO-style runtime shell.
///
/// This first slice owns composition, layout, and structure tracking. Event,
/// renderer, and backend integration are layered on top in later milestones.
pub struct Runtime {
    tree: TreeRuntimeState,
    input: InputRuntimeState,
    timing: TimingRuntimeState,
    animation: AnimationRuntimeState,
    resources: ResourceRuntimeState,
    layers: LayerRuntimeState,
    platform: PlatformRuntimeState,
    render: RenderRuntimeState,
    invalidation: InvalidationStore,
    debug: DebugRuntimeState,
}

struct TreeRuntimeState {
    page_id: NodeId,
    roots: Vec<Element>,
    scope_roots: ScopeRoots,
    live_ids: ScopeSet,
    clock_ids: ScopeSet,
    clock_periods: Option<ClockPeriodMap>,
    clock_period_ticks: Option<FxHashMap<ScopeId, u64>>,
    scope_layer_roots: ScopeLayerRoots,
    scope_layer_intents: ScopeLayerIntents,
    structure: Vec<ElementSnapshot>,
    screen: Screen,
    retained_stats: RetainedComposeStats,
    frame_index: u64,
}

#[derive(Debug, Default)]
struct LayerRuntimeState {
    intents: Vec<LayerRuntimeIntent>,
    debug_records: Vec<LayerDebugRecord>,
    dismissal_records: Vec<LayerDismissalRecord>,
    pointer_records: Vec<LayerPointerDebugRecord>,
    focus_restore: Option<NodeId>,
}

struct InputRuntimeState {
    interactions: FxHashMap<NodeId, InteractionState>,
    responses: FxHashMap<NodeId, Response>,
    callbacks: UiCallbacks,
    owners: InputOwners,
    drag_origin: Option<[f32; 2]>,
    pointer_position: Option<[f32; 2]>,
}

struct TimingRuntimeState {
    timers: FxHashMap<NodeId, TimerState>,
    clock_seconds: f64,
}

struct AnimationRuntimeState {
    animations: FxHashMap<NodeId, ElementAnimation>,
    frame_targets: FxHashMap<NodeId, FrameTargetState>,
}

struct ResourceRuntimeState {
    skins: SkinRegistry,
    text_system: Box<dyn TextSystem>,
}

#[derive(Debug, Default)]
struct PlatformRuntimeState {
    ime_rect: Option<LayoutRect>,
    cursor_shape: CursorShape,
    capture: PlatformCaptureState,
    effects: Vec<PlatformEffect>,
}

struct RenderRuntimeState {
    needs_render: bool,
    needs_compose: bool,
    full_redraw: bool,
    draw_cache_revision: Cell<u64>,
    draw_cache: RefCell<CacheCell<DrawListCacheKey, super::draw::UiDrawList>>,
}

struct DebugRuntimeState {
    snapshot: UiDebugSnapshot,
    events: Vec<EventDebugRecord>,
    input_pass: Option<FrameInputDebugRecord>,
    platform_effects: Vec<PlatformEffect>,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("page_id", &self.tree.page_id)
            .field("roots", &self.tree.roots)
            .field("structure", &self.tree.structure)
            .field("screen", &self.tree.screen)
            .field("needs_render", &self.render.needs_render)
            .field("needs_compose", &self.render.needs_compose)
            .field("full_redraw", &self.render.full_redraw)
            .finish_non_exhaustive()
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new("")
    }
}
impl Runtime {
    pub fn new(page_id: impl Into<String>) -> Self {
        Self::with_text_system(page_id, DefaultTextSystem::new())
    }

    pub fn with_text_system(
        page_id: impl Into<String>,
        text_system: impl TextSystem + 'static,
    ) -> Self {
        Self {
            tree: TreeRuntimeState {
                page_id: NodeId::new(page_id),
                roots: Vec::new(),
                scope_roots: FxHashMap::default(),
                live_ids: FxHashSet::default(),
                clock_ids: FxHashSet::default(),
                clock_periods: None,
                clock_period_ticks: None,
                scope_layer_roots: FxHashMap::default(),
                scope_layer_intents: FxHashMap::default(),
                structure: Vec::new(),
                screen: Screen::default(),
                retained_stats: RetainedComposeStats::default(),
                frame_index: 0,
            },
            input: InputRuntimeState {
                interactions: FxHashMap::default(),
                responses: FxHashMap::default(),
                callbacks: UiCallbacks::default(),
                owners: InputOwners::default(),
                drag_origin: None,
                pointer_position: None,
            },
            timing: TimingRuntimeState {
                timers: FxHashMap::default(),
                clock_seconds: 0.0,
            },
            animation: AnimationRuntimeState {
                animations: FxHashMap::default(),
                frame_targets: FxHashMap::default(),
            },
            resources: ResourceRuntimeState {
                skins: SkinRegistry::default(),
                text_system: Box::new(text_system),
            },
            layers: LayerRuntimeState::default(),
            platform: PlatformRuntimeState::default(),
            render: RenderRuntimeState {
                needs_render: true,
                needs_compose: false,
                full_redraw: true,
                draw_cache_revision: Cell::new(0),
                draw_cache: RefCell::new(CacheCell::default()),
            },
            invalidation: InvalidationStore::default(),
            debug: DebugRuntimeState {
                snapshot: UiDebugSnapshot::default(),
                events: Vec::new(),
                input_pass: None,
                platform_effects: Vec::new(),
            },
        }
    }
}
#[cfg(test)]
mod tests;
