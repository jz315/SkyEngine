use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::time::Duration;

use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use smallvec::SmallVec;

use super::callbacks::UiCallbacks;
use super::clock::ClockPeriodMap;
use super::event::InteractionState;
use super::fonts::FontRef;
use super::layout::{layout_element_in_frame_with_text_system, layout_roots_with_text_system};
use super::retained::{
    begin_scope_frame, normalize_dirty_scopes_with_roots, FullLayoutReason, LayoutMode,
    RetainedComposeAction, RetainedComposeEvent, RetainedComposeReason, RetainedComposeStats,
    RetainedRoot, ScopeComposeRecord, ScopeFrame, ScopeId, ScopeRoots, ScopeSet,
};
use super::skin::{NeoSkin, SkinRegistry};
use super::text_measure::{DefaultTextSystem, TextSystem};
use super::Color;
use super::{
    AnimProperty, AnimatedValue, Border, CacheCell, DragEvent, Element, ElementKind, KeyboardEvent,
    LayoutRect, Motion, PointerEvent, Response, Screen, ScrollEvent, Shadow, SmoothedValue,
    Transform, Transition, Ui, UiClip,
};
use crate::DirtyFlags;
mod animation;
mod composition;
mod debug;
mod dirty;
mod event_command;
mod frame;
mod ids;
mod interaction;
mod invalidation;
mod layers;
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
    Invalidation, InvalidationPropagation, InvalidationSource, InvalidationTarget, PassFlags,
};
pub use layers::{
    LayerAnchorSource, LayerDebugRecord, LayerDismissalRecord, LayerId, LayerIntent, LayerKind,
    LayerLifecycleAction, LayerPlacement, LayerPointerAction, LayerPointerDebugRecord, LayerSize,
    OutsideClickPolicy,
};
#[cfg(test)]
use reconcile::refresh_scope_roots_from_tree;
use timing::TimerState;

/// Compact structure snapshot used to detect tree-level and paint changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementSnapshot {
    pub id: String,
    pub kind: ElementKind,
    pub z_index: i32,
    pub clip: bool,
    pub clip_radius_bits: u32,
    pub child_count: usize,
    pub layout_signature: u64,
    pub visual_signature: u64,
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

#[derive(Debug, Clone, PartialEq)]
pub struct ElementDebugRecord {
    pub id: String,
    pub parent: Option<String>,
    pub retained_boundary: Option<String>,
    pub scroll_ancestor: Option<String>,
    pub clip_ancestor: Option<String>,
    pub target_frame: LayoutRect,
    pub draw_frame: Option<LayoutRect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDebugRecord {
    pub raw_event: &'static str,
    pub target: EventTargetId,
    pub command: &'static str,
    pub callback: bool,
    pub invalidation: Option<Invalidation>,
}

#[derive(Clone, PartialEq)]
pub struct UiDebugSnapshot {
    pub frame_index: u64,
    pub screen: Screen,
    pub dirty_ids: Vec<String>,
    pub normalized_dirty_ids: Vec<String>,
    pub live_ids: Vec<String>,
    pub clock_ids: Vec<String>,
    pub retained: Vec<RetainedDebugRecord>,
    pub elements: Vec<ElementDebugRecord>,
    pub events: Vec<EventDebugRecord>,
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
    pub focused_id: Option<String>,
    pub active_id: Option<String>,
    pub hovered_id: Option<String>,
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
            .field("retained", &self.retained)
            .field("element_count", &self.elements.len())
            .field("events", &self.events)
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
            .field("focused_id", &self.focused_id)
            .field("active_id", &self.active_id)
            .field("hovered_id", &self.hovered_id)
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
            retained: Vec::new(),
            elements: Vec::new(),
            events: Vec::new(),
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
            focused_id: None,
            active_id: None,
            hovered_id: None,
            active_animation_count: 0,
            invalidations: Vec::new(),
            pass_flags: PassFlags::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirtyInput {
    pub id: String,
    pub flags: DirtyFlags,
    pub source: Option<String>,
}

impl DirtyInput {
    pub fn new(id: impl Into<String>, flags: DirtyFlags) -> Self {
        Self {
            id: id.into(),
            flags,
            source: None,
        }
    }

    pub fn signal(id: impl Into<String>, source: impl Into<String>, flags: DirtyFlags) -> Self {
        Self {
            id: id.into(),
            flags,
            source: Some(source.into()),
        }
    }
}

impl From<(String, DirtyFlags)> for DirtyInput {
    fn from((id, flags): (String, DirtyFlags)) -> Self {
        Self::new(id, flags)
    }
}

/// Complete host-provided input snapshot for one UI frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameInput {
    pub screen: Screen,
    pub delta_seconds: f32,
    pub pointer: PointerEvent,
    pub pointer_events: Vec<PointerEvent>,
    pub scroll: ScrollEvent,
    pub keyboard: KeyboardEvent,
    pub dirty: Option<Vec<DirtyInput>>,
    pub force_full_compose: bool,
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

    pub fn dirty(mut self, value: impl IntoIterator<Item = DirtyInput>) -> Self {
        self.dirty = Some(value.into_iter().collect());
        self
    }

    pub fn force_full_compose(mut self, value: bool) -> Self {
        self.force_full_compose = value;
        self
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
    render: RenderRuntimeState,
    invalidation: InvalidationStore,
    debug: DebugRuntimeState,
}

struct TreeRuntimeState {
    page_id: String,
    roots: Vec<Element>,
    scope_roots: ScopeRoots,
    live_ids: ScopeSet,
    clock_ids: ScopeSet,
    clock_periods: Option<ClockPeriodMap>,
    clock_period_ticks: Option<FxHashMap<ScopeId, u64>>,
    structure: Vec<ElementSnapshot>,
    screen: Screen,
    retained_stats: RetainedComposeStats,
    frame_index: u64,
}

#[derive(Debug, Default)]
struct LayerRuntimeState {
    intents: Vec<LayerIntent>,
    debug_records: Vec<LayerDebugRecord>,
    dismissal_records: Vec<LayerDismissalRecord>,
    pointer_records: Vec<LayerPointerDebugRecord>,
    focus_restore: Option<String>,
}

struct InputRuntimeState {
    interactions: FxHashMap<String, InteractionState>,
    responses: FxHashMap<String, Response>,
    callbacks: UiCallbacks,
    owners: InputOwners,
    drag_origin: Option<[f32; 2]>,
    pointer_position: Option<[f32; 2]>,
}

struct TimingRuntimeState {
    timers: FxHashMap<String, TimerState>,
    clock_seconds: f64,
}

struct AnimationRuntimeState {
    animations: FxHashMap<String, ElementAnimation>,
    frame_targets: FxHashMap<String, FrameTargetState>,
}

struct ResourceRuntimeState {
    skins: SkinRegistry,
    text_system: Box<dyn TextSystem>,
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
                page_id: page_id.into(),
                roots: Vec::new(),
                scope_roots: FxHashMap::default(),
                live_ids: FxHashSet::default(),
                clock_ids: FxHashSet::default(),
                clock_periods: None,
                clock_period_ticks: None,
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
            },
        }
    }
}
#[cfg(test)]
mod tests;
