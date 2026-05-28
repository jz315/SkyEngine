use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use smallvec::SmallVec;

use super::dsl::{ClockPeriodMap, UiCallbacks};
use super::event::InteractionState;
use super::fonts::FontRef;
use super::layout::{layout_element_in_frame_with_text_system, layout_roots_with_text_system};
use super::retained::{
    begin_scope_frame, normalize_dirty_scopes_with_roots, structural_incompatibility_reports,
    structurally_incompatible_dirty_scopes, FullLayoutReason, LayoutMode, RetainedComposeAction,
    RetainedComposeEvent, RetainedComposeStats, RetainedRoot, ScopeComposeRecord, ScopeRoots,
    ScopeSet,
};
use super::skin::{NeoSkin, SkinRegistry};
use super::text_measure::{DefaultTextSystem, TextSystem};
use super::Color;
use super::{
    AnimProperty, AnimatedValue, Border, CacheCell, DragEvent, Element, ElementKind, KeyboardEvent,
    LayoutRect, Motion, PointerEvent, Response, Screen, ScrollEvent, Shadow, SmoothedValue,
    Transform, Transition, Ui, UiClip,
};
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
        }
    }
}

/// Complete host-provided input snapshot for one UI frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameInput {
    pub screen: Screen,
    pub delta_seconds: f32,
    pub pointer: PointerEvent,
    pub scroll: ScrollEvent,
    pub keyboard: KeyboardEvent,
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

    pub fn scroll(mut self, value: ScrollEvent) -> Self {
        self.scroll = value;
        self
    }

    pub fn keyboard(mut self, value: KeyboardEvent) -> Self {
        self.keyboard = value;
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
    page_id: String,
    roots: Vec<Element>,
    scope_roots: ScopeRoots,
    live_ids: ScopeSet,
    clock_ids: ScopeSet,
    clock_periods: Option<ClockPeriodMap>,
    clock_period_ticks: Option<FxHashMap<String, u64>>,
    structure: Vec<ElementSnapshot>,
    screen: Screen,
    interactions: FxHashMap<String, InteractionState>,
    responses: FxHashMap<String, Response>,
    callbacks: UiCallbacks,
    active_id: Option<String>,
    focused_id: Option<String>,
    drag_origin: Option<[f32; 2]>,
    pointer_position: Option<[f32; 2]>,
    timers: FxHashMap<String, TimerState>,
    animations: FxHashMap<String, ElementAnimation>,
    frame_targets: FxHashMap<String, FrameTargetState>,
    skins: SkinRegistry,
    needs_render: bool,
    needs_compose: bool,
    full_redraw: bool,
    text_system: Box<dyn TextSystem>,
    draw_cache_revision: Cell<u64>,
    draw_cache: RefCell<CacheCell<DrawListCacheKey, super::draw::UiDrawList>>,
    retained_stats: RetainedComposeStats,
    frame_index: u64,
    clock_seconds: f64,
    debug_snapshot: UiDebugSnapshot,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("page_id", &self.page_id)
            .field("roots", &self.roots)
            .field("structure", &self.structure)
            .field("screen", &self.screen)
            .field("needs_render", &self.needs_render)
            .field("needs_compose", &self.needs_compose)
            .field("full_redraw", &self.full_redraw)
            .finish_non_exhaustive()
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new("")
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct TimerState {
    seconds: f32,
    elapsed: f32,
    seen: bool,
    active: bool,
}

#[derive(Debug, Clone, Copy)]
struct FrameTargetState {
    frame: LayoutRect,
    seen: bool,
}

#[derive(Debug, Default)]
struct ElementAnimation {
    seen: bool,
    hover_blend: SmoothedValue,
    press_blend: SmoothedValue,
    frame: Option<AnimatedValue<LayoutRect>>,
    color: Option<AnimatedValue<Color>>,
    text_color: Option<AnimatedValue<Color>>,
    radius: Option<AnimatedValue<f32>>,
    blur: Option<AnimatedValue<f32>>,
    opacity: Option<AnimatedValue<f32>>,
    border: Option<AnimatedValue<Border>>,
    shadow: Option<AnimatedValue<Shadow>>,
    transform: Option<AnimatedValue<Transform>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DrawListCacheKey {
    revision: u64,
}

impl ElementAnimation {
    fn is_active(&self) -> bool {
        self.hover_blend.is_moving()
            || self.press_blend.is_moving()
            || self.frame.as_ref().is_some_and(AnimatedValue::is_animating)
            || self.color.as_ref().is_some_and(AnimatedValue::is_animating)
            || self
                .text_color
                .as_ref()
                .is_some_and(AnimatedValue::is_animating)
            || self
                .radius
                .as_ref()
                .is_some_and(AnimatedValue::is_animating)
            || self.blur.as_ref().is_some_and(AnimatedValue::is_animating)
            || self
                .opacity
                .as_ref()
                .is_some_and(AnimatedValue::is_animating)
            || self
                .border
                .as_ref()
                .is_some_and(AnimatedValue::is_animating)
            || self
                .shadow
                .as_ref()
                .is_some_and(AnimatedValue::is_animating)
            || self
                .transform
                .as_ref()
                .is_some_and(AnimatedValue::is_animating)
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
            page_id: page_id.into(),
            roots: Vec::new(),
            scope_roots: FxHashMap::default(),
            live_ids: FxHashSet::default(),
            clock_ids: FxHashSet::default(),
            clock_periods: None,
            clock_period_ticks: None,
            structure: Vec::new(),
            screen: Screen::default(),
            interactions: FxHashMap::default(),
            responses: FxHashMap::default(),
            callbacks: UiCallbacks::default(),
            active_id: None,
            focused_id: None,
            drag_origin: None,
            pointer_position: None,
            timers: FxHashMap::default(),
            animations: FxHashMap::default(),
            frame_targets: FxHashMap::default(),
            skins: SkinRegistry::default(),
            needs_render: true,
            needs_compose: false,
            full_redraw: true,
            text_system: Box::new(text_system),
            draw_cache_revision: Cell::new(0),
            draw_cache: RefCell::new(CacheCell::default()),
            retained_stats: RetainedComposeStats::default(),
            frame_index: 0,
            clock_seconds: 0.0,
            debug_snapshot: UiDebugSnapshot::default(),
        }
    }

    pub fn text_system_mut(&mut self) -> &mut dyn TextSystem {
        self.text_system.as_mut()
    }

    fn invalidate_draw_cache(&self) {
        self.draw_cache_revision
            .set(self.draw_cache_revision.get().wrapping_add(1));
        self.draw_cache.borrow_mut().invalidate();
    }

    fn request_render(&mut self) {
        self.needs_render = true;
    }

    fn mark_render_dirty(&mut self) {
        self.needs_render = true;
        self.invalidate_draw_cache();
    }

    fn mark_compose_dirty(&mut self) {
        self.needs_compose = true;
        self.mark_render_dirty();
    }

    fn mark_full_redraw_dirty(&mut self) {
        self.full_redraw = true;
        self.mark_render_dirty();
    }

    pub fn register_font(&mut self, font: &FontRef, bytes: &[u8]) {
        self.text_system.register_font(font, bytes);
        self.mark_compose_dirty();
    }

    pub fn register_skin(&mut self, skin: NeoSkin) {
        self.skins.register(skin);
        self.mark_compose_dirty();
    }

    pub fn skins(&self) -> &SkinRegistry {
        &self.skins
    }

    pub fn skins_mut(&mut self) -> &mut SkinRegistry {
        self.mark_compose_dirty();
        &mut self.skins
    }

    pub(crate) fn resolve_image_ref(&self, image: &super::ImageRef) -> super::ImageRef {
        self.skins.resolve_image(image)
    }

    pub(crate) fn resolve_font_ref(&self, font: &FontRef) -> FontRef {
        self.skins.resolve_font(font)
    }

    pub fn page_id(&self) -> &str {
        &self.page_id
    }

    pub fn screen(&self) -> Screen {
        self.screen
    }

    pub fn roots(&self) -> &[Element] {
        &self.roots
    }

    pub(crate) fn element_count_hint(&self) -> usize {
        self.structure.len().max(self.roots.len())
    }

    pub fn draw_list(&self) -> super::draw::UiDrawList {
        let mut cache = self.draw_cache.borrow_mut();
        cache.get_or_rebuild(
            DrawListCacheKey {
                revision: self.draw_cache_revision.get(),
            },
            |draw_list| {
                *draw_list = super::draw::build_draw_list(self);
            },
        );
        cache.value().clone()
    }

    pub fn draw_debug_trace(&self) -> super::diagnostics::UiDrawDebugTrace {
        super::diagnostics::UiDrawDebugTrace::from_runtime(self)
    }

    pub fn needs_render(&self) -> bool {
        self.needs_render
    }

    pub fn full_redraw(&self) -> bool {
        self.full_redraw
    }

    pub fn needs_compose(&self) -> bool {
        self.needs_compose
    }

    pub fn clear_needs_compose(&mut self) {
        self.needs_compose = false;
    }

    pub fn retained_compose_stats(&self) -> RetainedComposeStats {
        self.retained_stats
    }

    pub fn debug_snapshot(&self) -> &UiDebugSnapshot {
        &self.debug_snapshot
    }

    pub fn debug_snapshot_current(&self) -> UiDebugSnapshot {
        let mut snapshot = self.debug_snapshot.clone();
        snapshot.needs_render = self.needs_render;
        snapshot.needs_compose = self.needs_compose;
        snapshot.full_redraw = self.full_redraw;
        snapshot.focused_id = self.focused_id.clone();
        snapshot.active_id = self.active_id.clone();
        snapshot.hovered_id = hovered_id(&self.interactions);
        snapshot.active_animation_count = self
            .animations
            .values()
            .filter(|animation| animation.is_active())
            .count();
        snapshot
    }

    pub fn focused_id(&self) -> Option<&str> {
        self.focused_id.as_deref()
    }

    pub fn focused_ime_rect(&self) -> Option<LayoutRect> {
        let element = self.find(self.focused_id.as_deref()?)?;
        if !element.has_ime_rect {
            return None;
        }
        Some(LayoutRect::new(
            element.frame.x + element.ime_rect.x,
            element.frame.y + element.ime_rect.y,
            element.ime_rect.width,
            element.ime_rect.height,
        ))
    }

    pub fn mark_rendered(&mut self) {
        self.needs_render = false;
        self.full_redraw = false;
    }

    pub fn mark_full_redraw(&mut self) {
        self.mark_full_redraw_dirty();
    }

    pub fn compose(&mut self, width: f32, height: f32, compose: impl FnOnce(&mut Ui, Screen)) {
        self.compose_internal(width, height, None, compose);
    }

    pub fn compose_incremental(
        &mut self,
        width: f32,
        height: f32,
        dirty_ids: impl IntoIterator<Item = String>,
        compose: impl FnOnce(&mut Ui, Screen),
    ) {
        self.compose_internal(
            width,
            height,
            Some(dirty_ids.into_iter().collect()),
            compose,
        );
    }

    fn compose_internal(
        &mut self,
        width: f32,
        height: f32,
        dirty_ids: Option<FxHashSet<String>>,
        compose: impl FnOnce(&mut Ui, Screen),
    ) {
        let profile = neo_profile_enabled();
        let debug_trace = neo_debug_trace_enabled();
        let scope_profile = neo_scope_profile_enabled();
        let diagnostics_enabled = neo_diagnostics_enabled() || debug_trace || scope_profile;
        let timed = profile || scope_profile;
        let total_start = timed.then(Instant::now);
        self.needs_compose = false;
        let screen = Screen { width, height };
        let can_reuse_scopes = dirty_ids.is_some() && self.screen == screen;
        if can_reuse_scopes {
            self.mark_due_clock_periods();
        }
        let previous_clock_periods_for_reuse = can_reuse_scopes
            .then(|| self.clock_periods.clone())
            .flatten();
        let previous_clock_ids = if diagnostics_enabled {
            self.clock_ids.clone()
        } else {
            ScopeSet::default()
        };
        let scope_frame_start = timed.then(Instant::now);
        let scope_frame = begin_scope_frame(
            can_reuse_scopes,
            dirty_ids,
            &mut self.scope_roots,
            &mut self.live_ids,
        );
        let scope_frame_ms = elapsed_ms(scope_frame_start);
        let can_reuse_scopes = scope_frame.can_reuse_scopes;
        let input_dirty_scopes = scope_frame.input_dirty_scopes;
        let live_dirty_scopes = scope_frame.live_dirty_scopes;
        let dirty_scopes = scope_frame.dirty_scopes;
        let previous_scope_roots = scope_frame.previous_scope_roots;
        let ui_setup_start = timed.then(Instant::now);
        let mut ui = Ui::new(self.page_id.clone());
        ui.set_skins(self.skins.clone());
        ui.set_focused_id(self.focused_id.clone());
        ui.set_clock(self.clock_seconds, self.frame_index);
        ui.set_profile_timing(timed);
        ui.set_diagnostics_enabled(diagnostics_enabled);
        let ui_setup_ms = elapsed_ms(ui_setup_start);
        let previous_roots_start = timed.then(Instant::now);
        let previous_roots = std::mem::take(&mut self.roots);
        ui.set_previous_roots(previous_roots);
        if can_reuse_scopes {
            ui.set_scope_reuse(
                previous_scope_roots,
                dirty_scopes.clone(),
                std::mem::take(&mut self.callbacks),
                previous_clock_periods_for_reuse,
            );
        }
        let previous_roots_ms = elapsed_ms(previous_roots_start);
        let responses_start = timed.then(Instant::now);
        for (id, response) in &self.responses {
            ui.set_response(id.clone(), *response);
        }
        let responses_ms = elapsed_ms(responses_start);
        let build_start = timed.then(Instant::now);
        compose(&mut ui, screen);
        let build_ms = elapsed_ms(build_start);
        let into_parts_start = timed.then(Instant::now);
        let (
            mut roots,
            callbacks,
            mut scope_roots,
            live_ids,
            clock_ids,
            clock_periods,
            mut retained_stats,
            retained_events,
            scope_compose_records,
            previous_scope_roots,
            previous_roots_for_layout,
            retained_ui_profile,
        ) = ui.into_parts();
        let into_parts_ms = elapsed_ms(into_parts_start);
        let dirty_normalize_start = timed.then(Instant::now);
        let normalized_dirty_ids =
            normalize_dirty_scopes_with_roots(&dirty_scopes, &previous_scope_roots);
        let dirty_normalize_ms = elapsed_ms(dirty_normalize_start);
        let layout_plan_start = timed.then(Instant::now);
        let partial_layout_blocker = partial_layout_blocker(
            can_reuse_scopes,
            &normalized_dirty_ids,
            &previous_scope_roots,
        )
        .or_else(|| {
            let scopes = structurally_incompatible_dirty_scopes(
                &normalized_dirty_ids,
                &previous_scope_roots,
                &scope_roots,
            );
            if !scopes.is_empty() && neo_structure_trace_enabled() {
                for report in structural_incompatibility_reports(
                    &normalized_dirty_ids,
                    &previous_scope_roots,
                    &scope_roots,
                ) {
                    eprintln!("[eui-neo structure] {report}");
                }
            }
            (!scopes.is_empty()).then_some(FullLayoutReason::StructureChanged { ids: scopes })
        });
        let partial_layout = partial_layout_blocker.is_none();
        let layout_plan_ms = elapsed_ms(layout_plan_start);
        let layout_execute_start = timed.then(Instant::now);
        let mut used_partial_layout = false;
        if partial_layout {
            if can_reuse_scopes {
                copy_previous_frames(&mut roots, &previous_roots_for_layout);
                used_partial_layout = layout_dirty_ids_with_text_system(
                    &mut roots,
                    &normalized_dirty_ids,
                    &previous_scope_roots,
                    self.text_system.as_mut(),
                );
            }
        }
        let layout_mode = if used_partial_layout {
            LayoutMode::Partial
        } else if partial_layout {
            LayoutMode::Full(FullLayoutReason::DirtyRetainedLayoutFailed)
        } else {
            LayoutMode::Full(
                partial_layout_blocker.expect("full layout blocker should be known here"),
            )
        };
        if !used_partial_layout {
            layout_roots_with_text_system(&mut roots, width, height, self.text_system.as_mut());
        }
        retained_stats.partial_layout = used_partial_layout;
        retained_stats.full_layout = !used_partial_layout;
        let layout_execute_ms = elapsed_ms(layout_execute_start);
        let refresh_scope_roots_start = timed.then(Instant::now);
        refresh_scope_roots_from_tree(&mut scope_roots, &roots);
        let refresh_scope_roots_ms = elapsed_ms(refresh_scope_roots_start);
        let layout_ms =
            dirty_normalize_ms + layout_plan_ms + layout_execute_ms + refresh_scope_roots_ms;
        let structure_start = timed.then(Instant::now);
        let next_structure = collect_structure(&roots, self.structure.len());
        let layout_structure_changed =
            self.screen != screen || !layout_structures_match(&next_structure, &self.structure);
        let visual_structure_changed = !visual_structures_match(&next_structure, &self.structure);
        if layout_structure_changed {
            self.mark_full_redraw_dirty();
        } else if visual_structure_changed {
            self.mark_render_dirty();
        }
        let structure_ms = elapsed_ms(structure_start);
        let commit_state_start = timed.then(Instant::now);
        self.structure = next_structure;
        self.screen = screen;
        self.roots = roots;
        self.scope_roots = scope_roots;
        self.live_ids = live_ids;
        self.clock_ids = clock_ids;
        self.clock_periods = clock_periods;
        sync_clock_period_ticks(
            self.clock_seconds,
            self.clock_periods.as_ref(),
            &mut self.clock_period_ticks,
        );
        self.callbacks = callbacks;
        self.retained_stats = retained_stats;
        self.frame_index = self.frame_index.saturating_add(1);
        let commit_state_ms = elapsed_ms(commit_state_start);
        let element_debug_start = (timed && diagnostics_enabled).then(Instant::now);
        let element_debug_records = if diagnostics_enabled {
            collect_element_debug_records(
                &self.roots,
                &self.scope_roots,
                &self.callbacks,
                &self.animations,
            )
        } else {
            Vec::new()
        };
        let element_debug_ms = elapsed_ms(element_debug_start);
        let scope_debug_start = (timed && diagnostics_enabled).then(Instant::now);
        let scope_debug_records = if diagnostics_enabled {
            collect_scope_debug_records(
                &self.scope_roots,
                &previous_scope_roots,
                &input_dirty_scopes,
                &live_dirty_scopes,
                &previous_clock_ids,
                &dirty_scopes,
                &normalized_dirty_ids,
                &element_debug_records,
                &retained_events,
            )
        } else {
            Vec::new()
        };
        let scope_debug_ms = elapsed_ms(scope_debug_start);
        let snapshot_start = (timed && diagnostics_enabled).then(Instant::now);
        if diagnostics_enabled {
            self.debug_snapshot = UiDebugSnapshot {
                frame_index: self.frame_index,
                screen: self.screen,
                dirty_ids: sorted_scope_set(&dirty_scopes),
                normalized_dirty_ids: sorted_scope_set(&normalized_dirty_ids),
                live_ids: sorted_scope_set(&self.live_ids),
                clock_ids: sorted_scope_set(&self.clock_ids),
                retained: scope_debug_records,
                elements: element_debug_records,
                retained_events,
                scope_compose: scope_compose_records,
                retained_stats: self.retained_stats,
                layout_mode,
                needs_render: self.needs_render,
                needs_compose: self.needs_compose,
                full_redraw: self.full_redraw,
                focused_id: self.focused_id.clone(),
                active_id: self.active_id.clone(),
                hovered_id: hovered_id(&self.interactions),
                active_animation_count: self
                    .animations
                    .values()
                    .filter(|animation| animation.is_active())
                    .count(),
            };
        } else {
            self.debug_snapshot = UiDebugSnapshot {
                frame_index: self.frame_index,
                screen: self.screen,
                retained_stats: self.retained_stats,
                layout_mode,
                needs_render: self.needs_render,
                needs_compose: self.needs_compose,
                full_redraw: self.full_redraw,
                ..UiDebugSnapshot::default()
            };
        };
        let snapshot_ms = elapsed_ms(snapshot_start);
        if debug_trace {
            trace_debug_snapshot(&self.debug_snapshot);
        }
        if scope_profile {
            trace_scope_profile(
                &self.debug_snapshot,
                elapsed_ms(total_start),
                build_ms,
                layout_ms,
                structure_ms,
            );
        }
        if profile {
            let accounted_ms = scope_frame_ms
                + ui_setup_ms
                + previous_roots_ms
                + responses_ms
                + build_ms
                + into_parts_ms
                + layout_ms
                + structure_ms
                + commit_state_ms
                + element_debug_ms
                + scope_debug_ms
                + snapshot_ms;
            let total_ms = elapsed_ms(total_start);
            eprintln!(
                "[eui-neo] compose total={:.3}ms scope_frame={:.3}ms ui_setup={:.3}ms previous_roots={:.3}ms responses={:.3}ms build={:.3}ms retained_lookup={:.3}ms retained_metadata={:.3}ms into_parts={:.3}ms layout={:.3}ms layout_normalize={:.3}ms layout_plan={:.3}ms layout_execute={:.3}ms refresh_scopes={:.3}ms structure={:.3}ms commit={:.3}ms element_debug={:.3}ms scope_debug={:.3}ms snapshot={:.3}ms unaccounted={:.3}ms elements={} retained_built={} retained_reused={}",
                total_ms,
                scope_frame_ms,
                ui_setup_ms,
                previous_roots_ms,
                responses_ms,
                build_ms,
                retained_ui_profile.lookup_ms,
                retained_ui_profile.metadata_ms,
                into_parts_ms,
                layout_ms,
                dirty_normalize_ms,
                layout_plan_ms,
                layout_execute_ms,
                refresh_scope_roots_ms,
                structure_ms,
                commit_state_ms,
                element_debug_ms,
                scope_debug_ms,
                snapshot_ms,
                (total_ms - accounted_ms).max(0.0),
                self.structure.len(),
                self.retained_stats.built,
                self.retained_stats.reused
            );
        }
    }

    pub fn current_frame(&self) -> Frame {
        Frame {
            screen: self.screen,
            draw_list: self.draw_list(),
            needs_render: self.needs_render,
            needs_compose: self.needs_compose,
            full_redraw: self.full_redraw,
            focused_ime_rect: self.focused_ime_rect(),
        }
    }

    pub fn frame<R>(
        &mut self,
        input: FrameInput,
        compose: impl FnOnce(&mut Ui, Screen) -> R,
    ) -> FrameResult<R> {
        self.update_events_and_timers(
            input.pointer,
            input.scroll,
            input.keyboard,
            input.delta_seconds,
        );
        let mut value = None;
        self.compose(input.screen.width, input.screen.height, |ui, screen| {
            value = Some(compose(ui, screen));
        });
        self.tick_animations(input.delta_seconds);
        FrameResult {
            value: value.expect("frame compose closure did not run"),
            frame: self.current_frame(),
        }
    }

    pub fn find(&self, id: &str) -> Option<&Element> {
        let id = self.resolve_id_ref(id);
        self.find_resolved(id.as_ref())
    }

    pub fn response(&self, id: &str) -> Response {
        self.responses
            .get(self.resolve_id_ref(id).as_ref())
            .copied()
            .unwrap_or_default()
    }

    pub fn interaction(&self, id: &str) -> InteractionState {
        self.interactions
            .get(self.resolve_id_ref(id).as_ref())
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn animated_frame(&self, element: &Element) -> LayoutRect {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.frame.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.frame)
    }

    pub(crate) fn animated_color(&self, element: &Element) -> Color {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.color.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or_else(|| self.state_color_target(element))
    }

    pub(crate) fn animated_text_color(&self, element: &Element) -> Color {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.text_color.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.text_color)
    }

    pub(crate) fn animated_radius(&self, element: &Element) -> f32 {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.radius.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.radius)
    }

    pub(crate) fn animated_blur(&self, element: &Element) -> f32 {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.blur.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.blur)
    }

    pub(crate) fn animated_opacity(&self, element: &Element) -> f32 {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.opacity.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.opacity)
    }

    pub(crate) fn animated_border(&self, element: &Element) -> Border {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.border.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.border)
    }

    pub(crate) fn animated_shadow(&self, element: &Element) -> Shadow {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.shadow.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.shadow)
    }

    pub(crate) fn animated_transform(&self, element: &Element) -> Transform {
        self.animations
            .get(&element.id)
            .and_then(|animation| animation.transform.as_ref())
            .map(AnimatedValue::current)
            .unwrap_or(element.transform)
    }

    pub(crate) fn hover_blend_for_source(&self, id: &str) -> Option<f32> {
        let id = self.resolve_id_ref(id);
        let element = self.find_resolved(id.as_ref())?;
        if !matches!(
            element.kind,
            ElementKind::Rect | ElementKind::Polygon | ElementKind::Image | ElementKind::NineSlice
        ) {
            return None;
        }
        self.animations
            .get(id.as_ref())
            .map(|animation| animation.hover_blend.current())
    }

    pub(crate) fn press_blend_for_source(&self, id: &str) -> Option<(f32, LayoutRect)> {
        let id = self.resolve_id_ref(id);
        let element = self.find_resolved(id.as_ref())?;
        if !matches!(
            element.kind,
            ElementKind::Rect | ElementKind::Polygon | ElementKind::Image | ElementKind::NineSlice
        ) {
            return None;
        }
        let animation = self.animations.get(id.as_ref())?;
        let frame = animation
            .frame
            .as_ref()
            .map(AnimatedValue::current)
            .unwrap_or(element.frame);
        Some((animation.press_blend.current(), frame))
    }

    pub fn update_pointer(&mut self, event: PointerEvent) -> bool {
        let position = event.position();
        let delta = event.delta();
        if position.is_some() {
            self.pointer_position = position;
        }
        let hit_id = hit_test_interactive(&self.roots, position);
        if event.pressed_this_frame {
            self.set_focused_id(hit_test_focusable(&self.roots, position));
        }
        if event.right_pressed_this_frame {
            if let Some(target_id) = hit_id.as_deref() {
                let frame = self
                    .find(target_id)
                    .map(|element| element.frame)
                    .unwrap_or_default();
                if let Some(callback) = self.callbacks.on_context_menu.get_mut(target_id) {
                    callback(event, frame);
                    self.mark_compose_dirty();
                }
            }
        }
        if event.pressed_this_frame {
            self.active_id = hit_id.clone();
            self.drag_origin = position;
        }

        let captured_id = self.active_id.clone();
        let hover_id = captured_id.clone().or(hit_id.clone());
        let mut ids = FxHashSet::default();
        ids.extend(self.interactions.keys().cloned());
        if let Some(id) = captured_id.as_ref() {
            ids.insert(id.clone());
        }
        if let Some(id) = hit_id.as_ref() {
            ids.insert(id.clone());
        }

        let mut changed = false;
        let mut next = FxHashMap::default();
        let mut responses = FxHashMap::default();
        for id in ids {
            let previous = self.interactions.get(&id).copied().unwrap_or_default();
            let active = captured_id.as_deref() == Some(id.as_ref());
            let hovered = hover_id.as_deref() == Some(id.as_ref());
            let pressed = active && event.down;
            let press_started = active && event.pressed_this_frame;
            let released = active && event.released_this_frame;
            let clicked = released && hit_id.as_deref() == Some(id.as_ref());
            let drag_start = if press_started {
                position.unwrap_or(previous.drag_start)
            } else {
                previous.drag_start
            };
            let drag_total = if active {
                match (position, self.drag_origin) {
                    (Some(position), Some(origin)) => {
                        [position[0] - origin[0], position[1] - origin[1]]
                    }
                    _ => previous.drag_total,
                }
            } else {
                [0.0, 0.0]
            };
            let dragging =
                active && (drag_total[0] * drag_total[0] + drag_total[1] * drag_total[1]) > 4.0;
            let mut state = InteractionState {
                hovered,
                pressed,
                clicked,
                press_started,
                released,
                dragging,
                active,
                changed: false,
                drag_start,
                drag_delta: delta,
                drag_total,
            };
            state.changed = state_without_changed(state) != state_without_changed(previous);
            changed |= state.changed;
            if press_started {
                let frame = self
                    .find(&id)
                    .map(|element| element.frame)
                    .unwrap_or_default();
                if let Some(callback) = self.callbacks.on_press.get_mut(&id) {
                    callback(event, frame);
                    self.mark_compose_dirty();
                }
            }
            if clicked {
                if let Some(callback) = self.callbacks.on_click.get_mut(&id) {
                    callback();
                    self.mark_compose_dirty();
                }
            }
            if pressed
                && (delta != [0.0, 0.0] || dragging)
                && self.callbacks.on_drag.contains_key(&id)
            {
                if let Some(callback) = self.callbacks.on_drag.get_mut(&id) {
                    let [x, y] = position.unwrap_or_default();
                    callback(DragEvent {
                        x,
                        y,
                        delta_x: delta[0],
                        delta_y: delta[1],
                        total_x: drag_total[0],
                        total_y: drag_total[1],
                    });
                    self.mark_compose_dirty();
                }
            }
            if state != InteractionState::default() || state.changed {
                responses.insert(
                    id.clone(),
                    Response {
                        hovered,
                        pressed,
                        clicked,
                        focused: false,
                        changed: state.changed,
                    },
                );
            }
            if state != InteractionState::default() {
                next.insert(id, state);
            }
        }

        if event.released_this_frame {
            self.active_id = None;
            self.drag_origin = None;
        }
        if changed {
            self.mark_render_dirty();
        }
        self.interactions = next;
        self.responses = responses;
        changed
    }

    pub fn update_scroll(&mut self, event: ScrollEvent) -> bool {
        if !event.active() {
            return false;
        }
        let target = hit_test(&self.roots, self.pointer_position, |element| {
            self.callbacks.on_scroll.contains_key(&element.id) && !element.disabled
        });
        let Some(target) = target else {
            return false;
        };
        let Some(callback) = self.callbacks.on_scroll.get_mut(&target) else {
            return false;
        };
        callback(event);
        self.mark_compose_dirty();
        true
    }

    pub fn update_keyboard(&mut self, event: KeyboardEvent) -> bool {
        if !event.has_input() {
            return false;
        }
        let Some(focused_id) = self.focused_id.clone() else {
            return false;
        };
        let Some(callback) = self.callbacks.on_text_input.get_mut(&focused_id) else {
            return false;
        };
        callback(event);
        self.mark_compose_dirty();
        true
    }

    pub fn tick_timers(&mut self, delta_seconds: f32) -> bool {
        for state in self.timers.values_mut() {
            state.seen = false;
        }
        let mut fired = Vec::new();
        collect_timer_ids(&self.roots, &mut fired);
        let mut changed = false;
        for (id, seconds) in fired {
            let state = self.timers.entry(id.clone()).or_default();
            state.seen = true;
            if !state.active || (state.seconds - seconds).abs() > 0.001 {
                state.seconds = seconds;
                state.elapsed = 0.0;
                state.active = true;
            }
            state.elapsed += delta_seconds.max(0.0);
            if state.active && state.elapsed >= state.seconds {
                state.active = false;
                if let Some(callback) = self.callbacks.on_timer.get_mut(&id) {
                    callback();
                    self.mark_compose_dirty();
                    changed = true;
                }
            } else if state.active {
                self.request_render();
            }
        }
        self.timers.retain(|_, state| state.seen);
        changed
    }

    pub fn tick_animations(&mut self, delta_seconds: f32) -> bool {
        for animation in self.animations.values_mut() {
            animation.seen = false;
        }
        for target in self.frame_targets.values_mut() {
            target.seen = false;
        }

        let mut changed = false;
        let roots = std::mem::take(&mut self.roots);
        if z_order_is_stable(&roots) {
            for root in &roots {
                changed |= self.tick_element_animation_tree(root, delta_seconds, false);
            }
        } else {
            for index in sorted_z_indices(&roots) {
                changed |= self.tick_element_animation_tree(&roots[index], delta_seconds, false);
            }
        }
        self.roots = roots;
        self.animations.retain(|_, animation| animation.seen);
        self.frame_targets.retain(|_, target| target.seen);

        let active = self.animations.values().any(ElementAnimation::is_active);
        if changed {
            self.mark_render_dirty();
        } else if active {
            self.request_render();
        }
        changed
    }

    pub fn update_input_snapshot(
        &mut self,
        pointer: PointerEvent,
        scroll: ScrollEvent,
        keyboard: KeyboardEvent,
        delta_seconds: f32,
    ) -> bool {
        let mut changed = self.update_events_and_timers(pointer, scroll, keyboard, delta_seconds);
        changed |= self.tick_animations(delta_seconds);
        changed
    }

    pub fn update_events_and_timers(
        &mut self,
        pointer: PointerEvent,
        scroll: ScrollEvent,
        keyboard: KeyboardEvent,
        delta_seconds: f32,
    ) -> bool {
        self.clock_seconds += f64::from(delta_seconds.max(0.0));
        let mut changed = self.update_pointer(pointer);
        changed |= self.update_scroll(scroll);
        changed |= self.update_keyboard(keyboard);
        changed |= self.tick_timers(delta_seconds);
        changed
    }

    fn tick_element_animation_tree(
        &mut self,
        element: &Element,
        delta_seconds: f32,
        ancestor_frame_changed: bool,
    ) -> bool {
        let frame_target_changed = self.update_frame_target(element);
        let mut changed =
            self.tick_element_animation(element, delta_seconds, ancestor_frame_changed);
        let child_ancestor_frame_changed = ancestor_frame_changed || frame_target_changed;
        if z_order_is_stable(&element.children) {
            for child in &element.children {
                changed |= self.tick_element_animation_tree(
                    child,
                    delta_seconds,
                    child_ancestor_frame_changed,
                );
            }
        } else {
            for index in sorted_z_indices(&element.children) {
                changed |= self.tick_element_animation_tree(
                    &element.children[index],
                    delta_seconds,
                    child_ancestor_frame_changed,
                );
            }
        }
        changed
    }

    fn update_frame_target(&mut self, element: &Element) -> bool {
        let entry = self
            .frame_targets
            .entry(element.id.clone())
            .or_insert(FrameTargetState {
                frame: element.frame,
                seen: true,
            });
        let moved = (entry.frame.x - element.frame.x).abs() > 0.001
            || (entry.frame.y - element.frame.y).abs() > 0.001;
        entry.frame = element.frame;
        entry.seen = true;
        moved
    }

    fn tick_element_animation(
        &mut self,
        element: &Element,
        delta_seconds: f32,
        snap_frame: bool,
    ) -> bool {
        let interaction = self.interaction(&element.id);
        let animation = self.animations.entry(element.id.clone()).or_default();
        animation.seen = true;

        let mut changed = false;
        let transition = element.transition;
        let animate_frame = !snap_frame && should_animate_frame(element);
        let animate_color = should_animate(element, AnimProperty::COLOR);
        let animate_text_color = should_animate(element, AnimProperty::TEXT_COLOR);
        let animate_opacity = should_animate(element, AnimProperty::OPACITY);
        let animate_radius = should_animate(element, AnimProperty::RADIUS);
        let animate_border = should_animate(element, AnimProperty::BORDER);
        let animate_shadow = should_animate(element, AnimProperty::SHADOW);
        let animate_blur = should_animate(element, AnimProperty::BLUR);
        let animate_transform = should_animate(element, AnimProperty::TRANSFORM);

        match element.kind {
            ElementKind::Row | ElementKind::Column | ElementKind::Stack => {
                changed |= sync_animated(
                    &mut animation.opacity,
                    element.opacity,
                    transition,
                    animate_opacity,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.transform,
                    element.transform,
                    transition,
                    animate_transform,
                    delta_seconds,
                );
            }
            ElementKind::Rect => {
                changed |= update_state_blends(animation, element, interaction, delta_seconds);
                let target_color = state_color_target(
                    element,
                    interaction,
                    Some((
                        animation.hover_blend.current(),
                        animation.press_blend.current(),
                    )),
                );
                changed |= sync_animated(
                    &mut animation.frame,
                    element.frame,
                    transition,
                    animate_frame,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.color,
                    target_color,
                    transition,
                    animate_color,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.radius,
                    element.radius,
                    transition,
                    animate_radius,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.blur,
                    element.blur,
                    transition,
                    animate_blur,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.opacity,
                    element.opacity,
                    transition,
                    animate_opacity,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.border,
                    element.border,
                    transition,
                    animate_border,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.shadow,
                    element.shadow,
                    transition,
                    animate_shadow,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.transform,
                    element.transform,
                    transition,
                    animate_transform,
                    delta_seconds,
                );
            }
            ElementKind::Polygon => {
                changed |= update_state_blends(animation, element, interaction, delta_seconds);
                let target_color = state_color_target(
                    element,
                    interaction,
                    Some((
                        animation.hover_blend.current(),
                        animation.press_blend.current(),
                    )),
                );
                changed |= sync_animated(
                    &mut animation.frame,
                    element.frame,
                    transition,
                    animate_frame,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.color,
                    target_color,
                    transition,
                    animate_color,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.opacity,
                    element.opacity,
                    transition,
                    animate_opacity,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.transform,
                    element.transform,
                    transition,
                    animate_transform,
                    delta_seconds,
                );
            }
            ElementKind::Text => {
                changed |= sync_animated(
                    &mut animation.frame,
                    element.frame,
                    transition,
                    animate_frame,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.text_color,
                    element.text_color,
                    transition,
                    animate_text_color,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.opacity,
                    element.opacity,
                    transition,
                    animate_opacity,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.transform,
                    element.transform,
                    transition,
                    animate_transform,
                    delta_seconds,
                );
            }
            ElementKind::Image | ElementKind::NineSlice => {
                changed |= update_state_blends(animation, element, interaction, delta_seconds);
                let target_color = state_color_target(
                    element,
                    interaction,
                    Some((
                        animation.hover_blend.current(),
                        animation.press_blend.current(),
                    )),
                );
                changed |= sync_animated(
                    &mut animation.frame,
                    element.frame,
                    transition,
                    animate_frame,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.color,
                    target_color,
                    transition,
                    animate_color,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.radius,
                    element.radius,
                    transition,
                    animate_radius,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.opacity,
                    element.opacity,
                    transition,
                    animate_opacity,
                    delta_seconds,
                );
                changed |= sync_animated(
                    &mut animation.transform,
                    element.transform,
                    transition,
                    animate_transform,
                    delta_seconds,
                );
            }
        }

        changed
    }

    fn state_color_target(&self, element: &Element) -> Color {
        let animation = self.animations.get(&element.id);
        let blends = animation.map(|animation| {
            (
                animation.hover_blend.current(),
                animation.press_blend.current(),
            )
        });
        state_color_target(element, self.interaction(&element.id), blends)
    }

    fn set_focused_id(&mut self, focused: Option<String>) {
        if self.focused_id == focused {
            return;
        }
        let old = self.focused_id.clone();
        self.focused_id = focused.clone();
        if let Some(old) = old {
            if let Some(callback) = self.callbacks.on_focus_changed.get_mut(&old) {
                callback(false);
            }
        }
        if let Some(new) = focused {
            if let Some(callback) = self.callbacks.on_focus_changed.get_mut(&new) {
                callback(true);
            }
        }
        self.mark_compose_dirty();
    }

    fn mark_due_clock_periods(&mut self) {
        let Some(clock_periods) = self.clock_periods.as_ref() else {
            return;
        };
        let ticks = self
            .clock_period_ticks
            .get_or_insert_with(FxHashMap::default);
        for (scope, period) in clock_periods {
            let next_tick = clock_period_tick(self.clock_seconds, *period);
            let previous_tick = ticks.entry(scope.clone()).or_insert(next_tick);
            if *previous_tick != next_tick {
                *previous_tick = next_tick;
                self.live_ids.insert(scope.clone());
            }
        }
    }

    fn resolve_id_ref<'a>(&self, id: &'a str) -> Cow<'a, str> {
        if id.is_empty() || self.page_id.is_empty() {
            return Cow::Borrowed(id);
        }
        if is_resolved_id(id, &self.page_id) {
            Cow::Borrowed(id)
        } else {
            let mut resolved = String::with_capacity(self.page_id.len() + 1 + id.len());
            resolved.push_str(&self.page_id);
            resolved.push('.');
            resolved.push_str(id);
            Cow::Owned(resolved)
        }
    }

    fn find_resolved(&self, id: &str) -> Option<&Element> {
        self.roots.iter().find_map(|root| find_element(root, id))
    }
}

fn is_resolved_id(id: &str, page_id: &str) -> bool {
    id.len() > page_id.len()
        && id.as_bytes().get(page_id.len()) == Some(&b'.')
        && id.as_bytes().starts_with(page_id.as_bytes())
}

fn neo_profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SKY_NEO_PROFILE").is_some())
}

fn neo_debug_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SKY_NEO_DEBUG_TRACE").is_some())
}

fn neo_diagnostics_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| cfg!(debug_assertions) || std::env::var_os("SKY_NEO_DIAGNOSTICS").is_some())
}

fn neo_scope_profile_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SKY_NEO_SCOPE_PROFILE").is_some())
}

fn neo_structure_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SKY_NEO_STRUCTURE_TRACE").is_some())
}

fn trace_debug_snapshot(snapshot: &UiDebugSnapshot) {
    eprintln!(
        "[eui-neo debug] frame={} layout={:?} dirty={:?} normalized_dirty={:?} live={:?} clock={:?} built={} reused={} animations={} focused={:?} hovered={:?} active={:?}",
        snapshot.frame_index,
        snapshot.layout_mode,
        snapshot.dirty_ids,
        snapshot.normalized_dirty_ids,
        snapshot.live_ids,
        snapshot.clock_ids,
        snapshot.retained_stats.built,
        snapshot.retained_stats.reused,
        snapshot.active_animation_count,
        snapshot.focused_id,
        snapshot.hovered_id,
        snapshot.active_id,
    );
    for event in &snapshot.retained_events {
        eprintln!("[eui-neo retained] {:?} {}", event.action, event.id);
    }
    trace_debug_filters(snapshot);
}

fn trace_scope_profile(
    snapshot: &UiDebugSnapshot,
    total_ms: f32,
    build_ms: f32,
    layout_ms: f32,
    structure_ms: f32,
) {
    let mut records = snapshot.scope_compose.clone();
    records.sort_by(|left, right| {
        right
            .self_build_ms
            .partial_cmp(&left.self_build_ms)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .build_ms
                    .partial_cmp(&left.build_ms)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.id.cmp(&right.id))
    });
    let limit = std::env::var("SKY_NEO_SCOPE_PROFILE_TOP")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(8);
    eprintln!(
        "[eui-neo scope-profile] frame={} total={:.3}ms build={:.3}ms layout={:.3}ms structure={:.3}ms mode={:?} dirty={:?} normalized={:?} live={:?} built={} reused={} scopes={}",
        snapshot.frame_index,
        total_ms,
        build_ms,
        layout_ms,
        structure_ms,
        snapshot.layout_mode,
        snapshot.dirty_ids,
        snapshot.normalized_dirty_ids,
        snapshot.live_ids,
        snapshot.retained_stats.built,
        snapshot.retained_stats.reused,
        snapshot.scope_compose.len(),
    );
    for record in records.iter().take(limit) {
        let reasons = snapshot
            .retained
            .iter()
            .find(|retained| retained.id == record.id)
            .map(|retained| retained.dirty_reasons.as_slice())
            .unwrap_or(&[]);
        eprintln!(
            "[eui-neo scope-profile]   {:?} {:<48} self={:.3}ms total={:.3}ms reasons={:?} roots={}->{} elements={}",
            record.action,
            record.id,
            record.self_build_ms,
            record.build_ms,
            reasons,
            record.previous_roots,
            record.current_roots,
            record.element_count,
        );
    }
}

fn trace_debug_filters(snapshot: &UiDebugSnapshot) {
    if std::env::var_os("SKY_NEO_DEBUG_DIRTY").is_some() {
        for record in snapshot.retained.iter().filter(|record| record.dirty) {
            eprintln!(
                "[eui-neo dirty] id={} raw={} normalized={} reasons={:?} action={:?} roots={}->{} anchor={:?}",
                record.id,
                record.raw_dirty,
                record.normalized_dirty_root,
                record.dirty_reasons,
                record.action,
                record.previous_roots,
                record.current_roots,
                record.layout_anchor,
            );
        }
    }
    if let Ok(filter) = std::env::var("SKY_NEO_DEBUG_RETAINED") {
        for record in snapshot
            .retained
            .iter()
            .filter(|record| record.id.contains(&filter))
        {
            eprintln!(
                "[eui-neo retained-debug] id={} parent={:?} dirty={} raw={} normalized={} reasons={:?} action={:?} scroll={:?} clip={:?} anchor={:?}",
                record.id,
                record.parent_id,
                record.dirty,
                record.raw_dirty,
                record.normalized_dirty_root,
                record.dirty_reasons,
                record.action,
                record.scroll_ancestor,
                record.clip_ancestor,
                record.layout_anchor,
            );
        }
    }
    if let Ok(filter) = std::env::var("SKY_NEO_DEBUG_ELEMENT") {
        for element in snapshot
            .elements
            .iter()
            .filter(|element| element.id.contains(&filter))
        {
            eprintln!(
                "[eui-neo element] id={} parent={:?} boundary={:?} scroll={:?} clip={:?} target={:?} draw={:?}",
                element.id,
                element.parent,
                element.retained_boundary,
                element.scroll_ancestor,
                element.clip_ancestor,
                element.target_frame,
                element.draw_frame,
            );
        }
    }
}

fn elapsed_ms(start: Option<Instant>) -> f32 {
    start
        .map(|start| start.elapsed().as_secs_f32() * 1000.0)
        .unwrap_or(0.0)
}

fn sync_clock_period_ticks(
    seconds: f64,
    periods: Option<&ClockPeriodMap>,
    ticks: &mut Option<FxHashMap<String, u64>>,
) {
    let Some(periods) = periods else {
        *ticks = None;
        return;
    };
    let ticks = ticks.get_or_insert_with(FxHashMap::default);
    ticks.retain(|scope, _| periods.contains_key(scope));
    for (scope, period) in periods {
        ticks
            .entry(scope.clone())
            .or_insert_with(|| clock_period_tick(seconds, *period));
    }
}

fn clock_period_tick(seconds: f64, period: Duration) -> u64 {
    if period.is_zero() {
        return 0;
    }
    let period_seconds = period.as_secs_f64().max(f64::EPSILON);
    (seconds.max(0.0) / period_seconds).floor() as u64
}

fn sorted_z_indices(elements: &[Element]) -> SmallVec<[usize; 16]> {
    let mut order: SmallVec<[usize; 16]> = (0..elements.len()).collect();
    order.sort_by_key(|&index| (elements[index].z_index, index));
    order
}

fn sync_animated<T>(
    slot: &mut Option<AnimatedValue<T>>,
    target: T,
    transition: Transition,
    animate_property: bool,
    delta_seconds: f32,
) -> bool
where
    T: super::Lerp,
{
    let Some(value) = slot.as_mut() else {
        *slot = Some(AnimatedValue::new(target));
        return true;
    };
    let mut changed = value.set_target_if(target, transition, animate_property);
    changed |= value.update(delta_seconds);
    changed
}

fn update_state_blends(
    animation: &mut ElementAnimation,
    element: &Element,
    interaction: InteractionState,
    delta_seconds: f32,
) -> bool {
    let interactive = element.interactive && !element.disabled;
    let state_colors_visible = element.has_state_colors
        && (!color_close_enough(element.color, element.hover_color)
            || !color_close_enough(element.color, element.pressed_color));
    let hover_speed = if element.smooth_state_colors {
        9.0
    } else {
        0.0
    };
    let press_speed = if element.smooth_state_colors {
        16.0
    } else {
        0.0
    };
    let hover_target = (interactive && state_colors_visible && interaction.hovered) as u8 as f32;
    let press_target = (interactive && state_colors_visible && interaction.pressed) as u8 as f32;
    let mut changed = animation
        .hover_blend
        .update_to(hover_target, hover_speed, delta_seconds);
    changed |= animation
        .press_blend
        .update_to(press_target, press_speed, delta_seconds);
    changed
}

fn should_animate(element: &Element, property: AnimProperty) -> bool {
    element.transition.enabled && element.transition.properties.contains(property)
}

fn should_animate_frame(element: &Element) -> bool {
    should_animate(element, AnimProperty::FRAME) && element.explicit_frame_animation
}

fn state_color_target(
    element: &Element,
    interaction: InteractionState,
    blends: Option<(f32, f32)>,
) -> Color {
    let interactive = element.interactive && !element.disabled;
    let state_colors_visible = element.has_state_colors
        && (!color_close_enough(element.color, element.hover_color)
            || !color_close_enough(element.color, element.pressed_color));
    if !interactive || !state_colors_visible {
        return element.color;
    }

    let (hover, press) = blends.unwrap_or((
        interaction.hovered as u8 as f32,
        interaction.pressed as u8 as f32,
    ));
    let hover_color = mix_color(element.color, element.hover_color, hover);
    mix_color(hover_color, element.pressed_color, press)
}

fn mix_color(from: Color, to: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    let inverse = 1.0 - amount;
    Color {
        r: from.r * inverse + to.r * amount,
        g: from.g * inverse + to.g * amount,
        b: from.b * inverse + to.b * amount,
        a: from.a * inverse + to.a * amount,
    }
}

fn color_close_enough(left: Color, right: Color) -> bool {
    (left.r - right.r).abs() <= 0.001
        && (left.g - right.g).abs() <= 0.001
        && (left.b - right.b).abs() <= 0.001
        && (left.a - right.a).abs() <= 0.001
}

fn collect_structure(roots: &[Element], previous_len: usize) -> Vec<ElementSnapshot> {
    let mut snapshots = Vec::with_capacity(previous_len);
    for root in roots {
        collect_element_structure(root, &mut snapshots);
    }
    snapshots
}

fn sorted_scope_set(scopes: &ScopeSet) -> Vec<String> {
    let mut scopes: Vec<_> = scopes.iter().cloned().collect();
    scopes.sort();
    scopes
}

fn hovered_id(interactions: &FxHashMap<String, InteractionState>) -> Option<String> {
    interactions
        .iter()
        .find_map(|(id, state)| state.hovered.then(|| id.clone()))
}

fn collect_scope_debug_records(
    current_scope_roots: &ScopeRoots,
    previous_scope_roots: &ScopeRoots,
    input_dirty_ids: &ScopeSet,
    live_dirty_ids: &ScopeSet,
    previous_clock_ids: &ScopeSet,
    dirty_ids: &ScopeSet,
    normalized_dirty_ids: &ScopeSet,
    element_records: &[ElementDebugRecord],
    retained_events: &[RetainedComposeEvent],
) -> Vec<RetainedDebugRecord> {
    let mut scope_ids: ScopeSet = ScopeSet::default();
    scope_ids.extend(current_scope_roots.keys().cloned());
    scope_ids.extend(previous_scope_roots.keys().cloned());
    scope_ids.extend(dirty_ids.iter().cloned());
    scope_ids.extend(normalized_dirty_ids.iter().cloned());

    let action_by_scope: FxHashMap<_, _> = retained_events
        .iter()
        .map(|event| (event.id.clone(), event.action))
        .collect();
    let element_by_id: FxHashMap<_, _> = element_records
        .iter()
        .map(|record| (record.id.as_str(), record))
        .collect();
    let scope_ids_sorted = sorted_scope_set(&scope_ids);

    scope_ids_sorted
        .into_iter()
        .map(|scope| {
            let first_current_root = current_scope_roots
                .get(&scope)
                .and_then(|roots| roots.first())
                .map(|element| element.id.as_str());
            let first_previous_root = previous_scope_roots
                .get(&scope)
                .and_then(|roots| roots.first());
            let current_record = first_current_root.and_then(|id| element_by_id.get(id).copied());
            RetainedDebugRecord {
                parent_id: retained_parent_for_scope(&scope, current_scope_roots, &element_by_id),
                dirty: dirty_ids.contains(&scope),
                raw_dirty: input_dirty_ids.contains(&scope),
                normalized_dirty_root: normalized_dirty_ids.contains(&scope),
                dirty_reasons: dirty_reasons_for_scope(
                    &scope,
                    input_dirty_ids,
                    live_dirty_ids,
                    previous_clock_ids,
                    dirty_ids,
                    previous_scope_roots,
                ),
                action: action_by_scope.get(&scope).copied(),
                previous_roots: previous_scope_roots
                    .get(&scope)
                    .map_or(0, |roots| roots.len()),
                current_roots: current_scope_roots
                    .get(&scope)
                    .map_or(0, |roots| roots.len()),
                layout_anchor: first_previous_root
                    .map(|element| element.frame)
                    .or_else(|| current_record.map(|record| record.target_frame)),
                scroll_ancestor: current_record.and_then(|record| record.scroll_ancestor.clone()),
                clip_ancestor: current_record.and_then(|record| record.clip_ancestor.clone()),
                id: scope,
            }
        })
        .collect()
}

fn retained_parent_for_scope(
    scope: &str,
    current_scope_roots: &ScopeRoots,
    element_by_id: &FxHashMap<&str, &ElementDebugRecord>,
) -> Option<String> {
    let root_id = current_scope_roots
        .get(scope)
        .and_then(|roots| roots.first())
        .map(|element| element.id.as_str())?;
    let parent_id = element_by_id.get(root_id)?.parent.as_deref()?;
    let parent_boundary = element_by_id.get(parent_id)?.retained_boundary.as_deref()?;
    (parent_boundary != scope).then(|| parent_boundary.to_string())
}

fn dirty_reasons_for_scope(
    scope: &str,
    input_dirty_ids: &ScopeSet,
    live_dirty_ids: &ScopeSet,
    previous_clock_ids: &ScopeSet,
    dirty_ids: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
) -> Vec<DirtyReason> {
    let mut reasons = Vec::new();
    if input_dirty_ids.contains(scope) {
        reasons.push(DirtyReason::External);
    }
    if live_dirty_ids.contains(scope) {
        if previous_clock_ids.contains(scope) {
            reasons.push(DirtyReason::Clock);
        } else {
            reasons.push(DirtyReason::Live);
        }
    }
    if !dirty_ids.contains(scope)
        && scope_contains_dirty_dependency(scope, dirty_ids, previous_scope_roots)
    {
        reasons.push(DirtyReason::Descendant);
    }
    reasons
}

fn scope_contains_dirty_dependency(
    scope: &str,
    dirty_ids: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
) -> bool {
    let Some(scope_roots) = previous_scope_roots.get(scope) else {
        return false;
    };
    dirty_ids.iter().any(|dirty| {
        previous_scope_roots.get(dirty).is_some_and(|dirty_roots| {
            dirty_roots
                .iter()
                .any(|root| retained_roots_contain_id(scope_roots, &root.id))
        })
    })
}

fn retained_roots_contain_id(elements: &[RetainedRoot], id: &str) -> bool {
    elements
        .iter()
        .any(|element| element.id == id || retained_roots_contain_id(&element.children, id))
}

fn collect_element_debug_records(
    roots: &[Element],
    scope_roots: &ScopeRoots,
    callbacks: &UiCallbacks,
    animations: &FxHashMap<String, ElementAnimation>,
) -> Vec<ElementDebugRecord> {
    let mut records = Vec::new();
    let scope_by_root_id = scope_by_root_id(scope_roots);
    for root in roots {
        collect_element_debug_record(
            root,
            None,
            None,
            None,
            None,
            &scope_by_root_id,
            callbacks,
            animations,
            &mut records,
        );
    }
    records
}

#[allow(clippy::too_many_arguments)]
fn collect_element_debug_record(
    element: &Element,
    parent: Option<&str>,
    scroll_ancestor: Option<&str>,
    clip_ancestor: Option<&str>,
    retained_boundary: Option<&str>,
    scope_by_root_id: &FxHashMap<String, String>,
    callbacks: &UiCallbacks,
    animations: &FxHashMap<String, ElementAnimation>,
    records: &mut Vec<ElementDebugRecord>,
) {
    let retained_boundary = scope_by_root_id
        .get(&element.id)
        .map(String::as_str)
        .or(retained_boundary);
    let draw_frame = animations
        .get(&element.id)
        .and_then(|animation| animation.frame.as_ref())
        .map(AnimatedValue::current)
        .unwrap_or(element.frame);
    records.push(ElementDebugRecord {
        id: element.id.clone(),
        parent: parent.map(str::to_string),
        retained_boundary: retained_boundary.map(str::to_string),
        scroll_ancestor: scroll_ancestor.map(str::to_string),
        clip_ancestor: clip_ancestor.map(str::to_string),
        target_frame: element.frame,
        draw_frame: Some(draw_frame),
    });

    let next_scroll_ancestor = if callbacks.on_scroll.contains_key(&element.id) {
        Some(element.id.as_str())
    } else {
        scroll_ancestor
    };
    let next_clip_ancestor = if element.clip {
        Some(element.id.as_str())
    } else {
        clip_ancestor
    };
    for child in &element.children {
        collect_element_debug_record(
            child,
            Some(&element.id),
            next_scroll_ancestor,
            next_clip_ancestor,
            retained_boundary,
            scope_by_root_id,
            callbacks,
            animations,
            records,
        );
    }
}

fn scope_by_root_id(scope_roots: &ScopeRoots) -> FxHashMap<String, String> {
    let mut candidates: FxHashMap<String, Vec<String>> = FxHashMap::default();
    for (scope, roots) in scope_roots {
        for root in roots {
            candidates
                .entry(root.id.clone())
                .or_default()
                .push(scope.clone());
        }
    }
    let mut map = FxHashMap::default();
    for (root_id, mut scopes) in candidates {
        scopes.sort();
        let owner = scopes
            .iter()
            .find(|scope| *scope == &root_id)
            .cloned()
            .unwrap_or_else(|| {
                scopes
                    .into_iter()
                    .next()
                    .expect("scope root candidate list should not be empty")
            });
        map.insert(root_id, owner);
    }
    map
}

fn collect_element_structure(element: &Element, snapshots: &mut Vec<ElementSnapshot>) {
    snapshots.push(ElementSnapshot {
        id: element.id.clone(),
        kind: element.kind,
        z_index: element.z_index,
        clip: element.clip,
        clip_radius_bits: element.clip_radius.to_bits(),
        child_count: element.children.len(),
        layout_signature: element_layout_signature(element),
        visual_signature: element_visual_signature(element),
    });
    for child in &element.children {
        collect_element_structure(child, snapshots);
    }
}

fn layout_structures_match(next: &[ElementSnapshot], previous: &[ElementSnapshot]) -> bool {
    next.len() == previous.len()
        && next.iter().zip(previous).all(|(next, previous)| {
            next.id == previous.id
                && next.kind == previous.kind
                && next.child_count == previous.child_count
                && next.layout_signature == previous.layout_signature
        })
}

fn visual_structures_match(next: &[ElementSnapshot], previous: &[ElementSnapshot]) -> bool {
    next.len() == previous.len()
        && next.iter().zip(previous).all(|(next, previous)| {
            next.id == previous.id
                && next.kind == previous.kind
                && next.z_index == previous.z_index
                && next.clip == previous.clip
                && next.clip_radius_bits == previous.clip_radius_bits
                && next.child_count == previous.child_count
                && next.visual_signature == previous.visual_signature
        })
}

fn layout_dirty_ids_with_text_system(
    roots: &mut [Element],
    dirty_ids: &FxHashSet<String>,
    previous_scope_roots: &ScopeRoots,
    text_system: &mut dyn TextSystem,
) -> bool {
    for scope in dirty_ids {
        let Some(previous_roots) = previous_scope_roots.get(scope) else {
            return false;
        };
        for previous in previous_roots {
            let Some(current) = find_element_mut(roots, &previous.id) else {
                return false;
            };
            if !layout_element_in_frame_with_text_system(current, previous.frame, text_system) {
                return false;
            }
        }
    }
    true
}

fn partial_layout_blocker(
    can_reuse_scopes: bool,
    layout_dirty_scopes: &ScopeSet,
    previous_scope_roots: &ScopeRoots,
) -> Option<FullLayoutReason> {
    if !can_reuse_scopes {
        return Some(FullLayoutReason::RetainedReuseUnavailable);
    }
    layout_dirty_scopes.iter().find_map(|scope| {
        (!previous_scope_roots.contains_key(scope))
            .then(|| FullLayoutReason::MissingPreviousRetainedRoot { id: scope.clone() })
    })
}

fn refresh_scope_roots_from_tree(scope_roots: &mut ScopeRoots, roots: &[Element]) {
    let mut elements_by_id = FxHashMap::default();
    collect_elements_by_id(roots, &mut elements_by_id);
    for elements in scope_roots.values_mut() {
        for element in elements {
            if let Some(updated) = elements_by_id.get(element.id.as_str()) {
                refresh_retained_root_layout_frames(element, updated);
            }
        }
    }
}

fn collect_elements_by_id<'a>(
    elements: &'a [Element],
    index: &mut FxHashMap<&'a str, &'a Element>,
) {
    for element in elements {
        index.entry(element.id.as_str()).or_insert(element);
        collect_elements_by_id(&element.children, index);
    }
}

fn refresh_retained_root_layout_frames(element: &mut RetainedRoot, updated: &Element) {
    // Layout mutates only Element::frame. Retained scope roots keep the same
    // visual/callback data unless their structure changed, so refresh frames
    // in place and fall back to replacement only when the tree no longer matches.
    if element.kind != updated.kind
        || element.id != updated.id
        || element.children.len() != updated.children.len()
    {
        *element = RetainedRoot::from_element(updated);
        return;
    }
    element.frame = updated.frame;
    for (child, updated_child) in element.children.iter_mut().zip(&updated.children) {
        refresh_retained_root_layout_frames(child, updated_child);
    }
}

fn copy_previous_frames(elements: &mut [Element], previous_roots: &[Element]) {
    for element in elements {
        if let Some(previous) = find_element_in_slice(previous_roots, &element.id) {
            element.frame = previous.frame;
        }
        copy_previous_frames(&mut element.children, previous_roots);
    }
}

fn find_element_in_slice<'a>(elements: &'a [Element], id: &str) -> Option<&'a Element> {
    elements
        .iter()
        .find_map(|element| find_element(element, id))
}

fn find_element<'a>(element: &'a Element, id: &str) -> Option<&'a Element> {
    if element.id == id {
        return Some(element);
    }
    element
        .children
        .iter()
        .find_map(|child| find_element(child, id))
}

fn find_element_mut<'a>(elements: &'a mut [Element], id: &str) -> Option<&'a mut Element> {
    for element in elements {
        if element.id == id {
            return Some(element);
        }
        if let Some(found) = find_element_mut(&mut element.children, id) {
            return Some(found);
        }
    }
    None
}

fn hit_test_interactive(elements: &[Element], position: Option<[f32; 2]>) -> Option<String> {
    hit_test(elements, position, |element| {
        element.interactive && !element.disabled
    })
}

fn hit_test_focusable(elements: &[Element], position: Option<[f32; 2]>) -> Option<String> {
    hit_test(elements, position, |element| {
        element.focusable && !element.disabled
    })
}

fn hit_test(
    elements: &[Element],
    position: Option<[f32; 2]>,
    predicate: impl Fn(&Element) -> bool,
) -> Option<String> {
    let position = position?;
    hit_test_elements(elements, position, None, &predicate).map(|element| element.id.clone())
}

fn hit_test_elements<'a>(
    elements: &'a [Element],
    position: [f32; 2],
    clip: Option<UiClip>,
    predicate: &impl Fn(&Element) -> bool,
) -> Option<&'a Element> {
    if elements.len() <= 1 {
        for element in elements.iter().rev() {
            if let Some(target) = hit_test_element(element, position, clip, predicate) {
                return Some(target);
            }
        }
        return None;
    }

    if z_order_is_stable(elements) {
        for element in elements.iter().rev() {
            if let Some(target) = hit_test_element(element, position, clip, predicate) {
                return Some(target);
            }
        }
        return None;
    }

    let mut order: SmallVec<[usize; 16]> = (0..elements.len()).collect();
    order.sort_by_key(|&index| (elements[index].z_index, index));
    for index in order.into_iter().rev() {
        if let Some(target) = hit_test_element(&elements[index], position, clip, predicate) {
            return Some(target);
        }
    }
    None
}

fn z_order_is_stable(elements: &[Element]) -> bool {
    elements
        .windows(2)
        .all(|pair| pair[0].z_index <= pair[1].z_index)
}

fn hit_test_element<'a>(
    element: &'a Element,
    position: [f32; 2],
    clip: Option<UiClip>,
    predicate: &impl Fn(&Element) -> bool,
) -> Option<&'a Element> {
    if clip.is_some_and(|clip| !clip.contains(position)) {
        return None;
    }
    let next_clip = if element.clip {
        let radius = element.clip_radius;
        let current = UiClip::new(element.frame, radius);
        let clip = match clip {
            Some(parent) => intersect_clip(parent, current)?,
            None => current,
        };
        if !clip.contains(position) {
            return None;
        }
        Some(clip)
    } else {
        clip
    };

    let element_hit = if predicate(element)
        && hit_contains(element, position)
        && next_clip.is_none_or(|clip| clip.contains(position))
    {
        Some(element)
    } else {
        None
    };

    hit_test_elements(&element.children, position, next_clip, predicate).or(element_hit)
}

fn hit_contains(element: &Element, position: [f32; 2]) -> bool {
    if element.kind == ElementKind::Polygon {
        return polygon_contains(element, position);
    }
    element.frame.contains(position)
}

fn polygon_contains(element: &Element, position: [f32; 2]) -> bool {
    if element.polygon_points.len() < 3 || !element.frame.contains(position) {
        return false;
    }
    let local_x = position[0] - element.frame.x;
    let local_y = position[1] - element.frame.y;
    let mut inside = false;
    let mut previous = element.polygon_points.len() - 1;
    for current in 0..element.polygon_points.len() {
        let a = element.polygon_points[current];
        let b = element.polygon_points[previous];
        let denominator = b[1] - a[1];
        let crosses = (a[1] > local_y) != (b[1] > local_y)
            && local_x < (b[0] - a[0]) * (local_y - a[1]) / denominator + a[0];
        if crosses {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

fn intersect_rect(left: LayoutRect, right: LayoutRect) -> Option<LayoutRect> {
    let x0 = left.x.max(right.x);
    let y0 = left.y.max(right.y);
    let x1 = left.right().min(right.right());
    let y1 = left.bottom().min(right.bottom());
    (x1 > x0 && y1 > y0).then(|| LayoutRect::new(x0, y0, x1 - x0, y1 - y0))
}

fn intersect_clip(left: UiClip, right: UiClip) -> Option<UiClip> {
    let rect = intersect_rect(left.rect, right.rect)?;
    let radius = if same_rect(rect, right.rect) {
        right.radius
    } else if same_rect(rect, left.rect) {
        left.radius
    } else {
        left.radius.min(right.radius)
    };
    Some(UiClip::new(rect, radius))
}

fn same_rect(left: LayoutRect, right: LayoutRect) -> bool {
    (left.x - right.x).abs() <= 0.001
        && (left.y - right.y).abs() <= 0.001
        && (left.width - right.width).abs() <= 0.001
        && (left.height - right.height).abs() <= 0.001
}

fn state_without_changed(mut state: InteractionState) -> InteractionState {
    state.changed = false;
    state
}

fn collect_timer_ids(elements: &[Element], timers: &mut Vec<(String, f32)>) {
    for element in elements {
        if element.timer_seconds > 0.0 {
            timers.push((element.id.clone(), element.timer_seconds));
        }
        collect_timer_ids(&element.children, timers);
    }
}

fn element_layout_signature(element: &Element) -> u64 {
    let mut hasher = FxHasher::default();
    hash_rect(element.frame, &mut hasher);
    element.has_x.hash(&mut hasher);
    element.has_y.hash(&mut hasher);
    hash_f32(element.x, &mut hasher);
    hash_f32(element.y, &mut hasher);
    hash_size(element.width, &mut hasher);
    hash_size(element.height, &mut hasher);
    hash_edge_insets(element.margin, &mut hasher);
    hash_edge_insets(element.padding, &mut hasher);
    hash_f32(element.min_width, &mut hasher);
    hash_f32(element.max_layout_width, &mut hasher);
    hash_f32(element.min_height, &mut hasher);
    hash_f32(element.max_height, &mut hasher);
    hash_f32(element.grow, &mut hasher);
    hash_f32(element.spacing, &mut hasher);
    element.main_align.hash(&mut hasher);
    element.cross_align.hash(&mut hasher);

    match element.kind {
        ElementKind::Row | ElementKind::Column | ElementKind::Stack | ElementKind::Rect => {}
        ElementKind::Polygon => {}
        ElementKind::Text => {
            if text_measure_affects_layout(element) {
                element.text.hash(&mut hasher);
                element.font.hash(&mut hasher);
                hash_f32(element.font_size, &mut hasher);
                element.font_weight.hash(&mut hasher);
                hash_f32(element.text_max_width, &mut hasher);
                element.wrap.hash(&mut hasher);
                element.horizontal_align.hash(&mut hasher);
                element.vertical_align.hash(&mut hasher);
                hash_f32(element.line_height, &mut hasher);
            }
        }
        ElementKind::Image | ElementKind::NineSlice => {}
    }

    hash_f32(element.timer_seconds, &mut hasher);
    hasher.finish()
}

fn element_visual_signature(element: &Element) -> u64 {
    let mut hasher = FxHasher::default();
    element.z_index.hash(&mut hasher);
    element.clip.hash(&mut hasher);
    hash_f32(element.clip_radius, &mut hasher);

    match element.kind {
        ElementKind::Row | ElementKind::Column | ElementKind::Stack => {
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
        ElementKind::Rect => {
            hash_color(element.color, &mut hasher);
            hash_gradient(element.gradient, &mut hasher);
            hash_border(element.border, &mut hasher);
            hash_shadow(element.shadow, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.radius, &mut hasher);
            hash_f32(element.blur, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
        ElementKind::Polygon => {
            hash_color(element.color, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
            for point in &element.polygon_points {
                hash_f32(point[0], &mut hasher);
                hash_f32(point[1], &mut hasher);
            }
        }
        ElementKind::Text => {
            element.text.hash(&mut hasher);
            element.font.hash(&mut hasher);
            hash_f32(element.font_size, &mut hasher);
            element.font_weight.hash(&mut hasher);
            hash_f32(element.text_max_width, &mut hasher);
            element.wrap.hash(&mut hasher);
            element.horizontal_align.hash(&mut hasher);
            element.vertical_align.hash(&mut hasher);
            hash_f32(element.line_height, &mut hasher);
            hash_color(element.text_color, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
        ElementKind::Image => {
            element.image.hash(&mut hasher);
            element.image_fit.hash(&mut hasher);
            hash_color(element.tint, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.radius, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
        ElementKind::NineSlice => {
            element.image.hash(&mut hasher);
            hash_slice(element.slice, &mut hasher);
            hash_edge_insets(element.content_inset, &mut hasher);
            element.center_mode.hash(&mut hasher);
            element.edge_mode.hash(&mut hasher);
            hash_color(element.tint, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
    }

    element.interactive.hash(&mut hasher);
    element.focusable.hash(&mut hasher);
    element.disabled.hash(&mut hasher);
    element.cursor.hash(&mut hasher);
    element.has_ime_rect.hash(&mut hasher);
    hash_rect(element.ime_rect, &mut hasher);
    hash_color(element.hover_color, &mut hasher);
    hash_color(element.pressed_color, &mut hasher);
    element.has_state_colors.hash(&mut hasher);
    element.smooth_state_colors.hash(&mut hasher);
    element.visual_state_source_id.hash(&mut hasher);
    element.hover_opacity_source_id.hash(&mut hasher);
    hash_f32(element.pressed_scale, &mut hasher);
    hash_f32(element.hover_hidden_opacity, &mut hasher);
    hash_f32(element.hover_visible_opacity, &mut hasher);
    element.transition.enabled.hash(&mut hasher);
    hash_f32(element.transition.duration_seconds, &mut hasher);
    hash_f32(element.transition.delay_seconds, &mut hasher);
    element.transition.ease.hash(&mut hasher);
    element.transition.properties.hash(&mut hasher);
    hash_motion(element.transition.motion, &mut hasher);
    hash_f32(element.transition.damping_ratio, &mut hasher);
    element.explicit_frame_animation.hash(&mut hasher);
    hasher.finish()
}

fn text_measure_affects_layout(element: &Element) -> bool {
    matches!(element.width, super::Size::WrapContent)
        || matches!(element.height, super::Size::WrapContent)
}

fn hash_rect(rect: LayoutRect, hasher: &mut impl Hasher) {
    hash_f32(rect.x, hasher);
    hash_f32(rect.y, hasher);
    hash_f32(rect.width, hasher);
    hash_f32(rect.height, hasher);
}

fn hash_size(size: super::Size, hasher: &mut impl Hasher) {
    match size {
        super::Size::Fixed(value) => {
            0_u8.hash(hasher);
            hash_f32(value, hasher);
        }
        super::Size::WrapContent => 1_u8.hash(hasher),
        super::Size::Fill => 2_u8.hash(hasher),
    }
}

fn hash_edge_insets(insets: super::EdgeInsets, hasher: &mut impl Hasher) {
    hash_f32(insets.left, hasher);
    hash_f32(insets.top, hasher);
    hash_f32(insets.right, hasher);
    hash_f32(insets.bottom, hasher);
}

fn hash_gradient(gradient: super::Gradient, hasher: &mut impl Hasher) {
    gradient.enabled.hash(hasher);
    hash_color(gradient.start, hasher);
    hash_color(gradient.end, hasher);
    gradient.direction.hash(hasher);
}

fn hash_border(border: Border, hasher: &mut impl Hasher) {
    hash_f32(border.width, hasher);
    hash_color(border.color, hasher);
}

fn hash_shadow(shadow: Shadow, hasher: &mut impl Hasher) {
    shadow.enabled.hash(hasher);
    hash_f32(shadow.offset[0], hasher);
    hash_f32(shadow.offset[1], hasher);
    hash_f32(shadow.blur, hasher);
    hash_f32(shadow.spread, hasher);
    hash_color(shadow.color, hasher);
}

fn hash_transform(transform: Transform, hasher: &mut impl Hasher) {
    hash_f32(transform.translate[0], hasher);
    hash_f32(transform.translate[1], hasher);
    hash_f32(transform.scale[0], hasher);
    hash_f32(transform.scale[1], hasher);
    hash_f32(transform.rotation, hasher);
    hash_f32(transform.origin[0], hasher);
    hash_f32(transform.origin[1], hasher);
}

fn hash_slice(slice: super::Slice, hasher: &mut impl Hasher) {
    hash_f32(slice.left, hasher);
    hash_f32(slice.top, hasher);
    hash_f32(slice.right, hasher);
    hash_f32(slice.bottom, hasher);
}

fn hash_motion(motion: Motion, hasher: &mut impl Hasher) {
    match motion {
        Motion::Ease => 0_u8.hash(hasher),
        Motion::Spring => 1_u8.hash(hasher),
    }
}

fn hash_color(color: Color, hasher: &mut impl Hasher) {
    hash_f32(color.r, hasher);
    hash_f32(color.g, hasher);
    hash_f32(color.b, hasher);
    hash_f32(color.a, hasher);
}

fn hash_f32(value: f32, hasher: &mut impl Hasher) {
    value.to_bits().hash(hasher);
}

#[cfg(test)]
mod tests {
    use super::Color;
    use super::RetainedRoot;
    use super::Runtime;
    use crate::expert::{UiDrawCommand, UiRectDraw};
    use crate::widgets::{button, panel, text};
    use crate::DirtyReason;
    use crate::{
        Align, AnimProperty, Ease, Element, ElementKind, FontRef, FrameInput, FullLayoutReason,
        HorizontalAlign, KeyboardEvent, LayoutMode, LayoutRect, PointerEvent,
        RetainedComposeAction, Screen, ScrollEvent, Size, State, TextMeasure, TextMeasureRequest,
        TextSystem, Transition,
    };
    use rustc_hash::FxHashMap;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::time::Duration;

    #[test]
    fn runtime_composes_and_lays_out_tree() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(800.0, 600.0, |ui, screen| {
            ui.stack("root")
                .size(screen.width, screen.height)
                .content(|ui| {
                    ui.text("title").text("Hello").font_size(20.0).build();
                });
        });

        let root = runtime.find("root").unwrap();
        assert_eq!(root.frame.width, 800.0);
        assert_eq!(root.frame.height, 600.0);
        assert!(runtime.find("title").is_some());
        assert!(runtime.needs_render());
        assert!(runtime.full_redraw());
    }

    #[test]
    fn refresh_scope_roots_updates_from_current_tree_by_id() {
        let mut root = Element::new(ElementKind::Stack, "page.root");
        let mut child = Element::new(ElementKind::Stack, "page.child");
        child.frame = LayoutRect::new(10.0, 20.0, 30.0, 40.0);
        let mut grandchild = Element::new(ElementKind::Rect, "page.grandchild");
        grandchild.frame = LayoutRect::new(11.0, 22.0, 33.0, 44.0);
        child.children.push(grandchild);
        root.children.push(child);

        let mut stale_child = Element::new(ElementKind::Stack, "page.child");
        stale_child.frame = LayoutRect::new(0.0, 0.0, 1.0, 1.0);
        stale_child
            .children
            .push(Element::new(ElementKind::Rect, "page.grandchild"));
        let mut scope_roots = FxHashMap::default();
        scope_roots.insert(
            "page.child".to_string(),
            RetainedRoot::from_elements(&[stale_child]),
        );

        super::refresh_scope_roots_from_tree(&mut scope_roots, &[root]);

        assert_eq!(
            scope_roots["page.child"][0].frame,
            LayoutRect::new(10.0, 20.0, 30.0, 40.0)
        );
        assert_eq!(
            scope_roots["page.child"][0].children[0].frame,
            LayoutRect::new(11.0, 22.0, 33.0, 44.0)
        );
    }

    #[test]
    fn font_source_reaches_draw_list() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(200.0, 80.0, |ui, _| {
            ui.text("title")
                .text("Hello")
                .font_source("ui/fonts/title.ttf")
                .build();
        });

        let draw = runtime.draw_list();
        let text = draw
            .commands()
            .iter()
            .find_map(|command| match command {
                UiDrawCommand::Text(text) => Some(text),
                _ => None,
            })
            .expect("text draw should exist");
        assert_eq!(text.font, FontRef::source("ui/fonts/title.ttf"));
    }

    #[test]
    fn injected_text_system_controls_layout_measurement() {
        #[derive(Default)]
        struct FixedTextSystem;

        impl TextSystem for FixedTextSystem {
            fn register_font(&mut self, _font: &FontRef, _bytes: &[u8]) {}

            fn measure(&mut self, _request: TextMeasureRequest<'_>) -> TextMeasure {
                TextMeasure {
                    width: 77.0,
                    height: 19.0,
                }
            }
        }

        let mut runtime = Runtime::with_text_system("demo", FixedTextSystem);
        runtime.compose(200.0, 80.0, |ui, _| {
            ui.text("title")
                .text("Hello")
                .size(Size::wrap_content(), Size::wrap_content())
                .build();
        });

        let title = runtime.find("title").expect("title should exist");
        assert_eq!(title.frame.width, 77.0);
        assert_eq!(title.frame.height, 19.0);
    }

    #[test]
    fn frame_updates_input_composes_and_returns_draw_list() {
        let mut runtime = Runtime::new("demo");
        let result = runtime.frame(
            FrameInput::new(Screen::new(320.0, 180.0), 1.0 / 60.0),
            |ui, _| {
                ui.rect("root").size(64.0, 32.0).build();
                42
            },
        );

        assert_eq!(result.value, 42);
        assert_eq!(result.frame.screen.width, 320.0);
        assert!(!result.frame.draw_list().is_empty());
        assert!(result.frame.needs_render);
    }

    #[test]
    fn scoped_compose_rebuilds_dirty_scope_and_reuses_clean_sibling_with_callbacks() {
        #[derive(Default)]
        struct AppModel {
            selected: i32,
        }

        let state = State::new(AppModel::default());
        let left_builds = Rc::new(Cell::new(0));
        let right_builds = Rc::new(Cell::new(0));
        let right_clicks = Rc::new(Cell::new(0));
        let mut runtime = Runtime::new("page");

        let compose = |runtime: &mut Runtime, dirty_ids: Vec<String>| {
            let state = state.clone();
            let left_builds = left_builds.clone();
            let right_builds = right_builds.clone();
            let right_clicks = right_clicks.clone();
            runtime.compose_incremental(240.0, 80.0, dirty_ids, move |ui, _| {
                ui.row("root").size(240.0, 40.0).content(|ui| {
                    ui.retained_scope("left", |ui| {
                        left_builds.set(left_builds.get() + 1);
                        let selected = state
                            .signal(
                                "selected",
                                |state| state.selected,
                                |state, value| state.selected = value,
                            )
                            .watch(ui);
                        ui.text("left.label")
                            .size(100.0, 40.0)
                            .text(format!("left {selected}"))
                            .build();
                    });
                    ui.retained_scope("right", |ui| {
                        right_builds.set(right_builds.get() + 1);
                        let right_clicks = right_clicks.clone();
                        button(ui, "right.button")
                            .size(100.0, 40.0)
                            .text("right")
                            .on_click(move || right_clicks.set(right_clicks.get() + 1))
                            .build();
                    });
                });
            });
        };

        compose(&mut runtime, Vec::new());
        assert_eq!(left_builds.get(), 1);
        assert_eq!(right_builds.get(), 1);

        let selected = state.signal(
            "selected",
            |state| state.selected,
            |state, value| state.selected = value,
        );
        selected.set(1);
        compose(&mut runtime, state.take_dirty_ids());

        assert_eq!(left_builds.get(), 2);
        assert_eq!(right_builds.get(), 1);
        assert_eq!(runtime.find("left.label").unwrap().text, "left 1");
        assert!(runtime.find("right.button.bg").is_some());
        assert!(runtime.retained_compose_stats().built >= 1);
        assert!(runtime.retained_compose_stats().reused >= 1);
        assert!(runtime.retained_compose_stats().partial_layout);
        assert!(!runtime.retained_compose_stats().full_layout);

        let frame = runtime.find("right.button.bg").unwrap().frame;
        runtime.update_pointer(PointerEvent::pressed_at(frame.x + 1.0, frame.y + 1.0));
        runtime.update_pointer(PointerEvent::released_at(frame.x + 1.0, frame.y + 1.0));
        assert_eq!(right_clicks.get(), 1);
    }

    #[test]
    fn live_scope_rebuilds_on_next_scoped_compose_without_state_dirty() {
        let mut runtime = Runtime::new("page");
        let live_builds = Rc::new(Cell::new(0));
        let static_builds = Rc::new(Cell::new(0));

        let compose = |runtime: &mut Runtime, dirty_ids: Vec<String>| {
            let live_builds = live_builds.clone();
            let static_builds = static_builds.clone();
            runtime.compose_incremental(240.0, 80.0, dirty_ids, move |ui, _| {
                ui.row("root").size(240.0, 40.0).content(|ui| {
                    ui.retained_live_scope("live", |ui| {
                        live_builds.set(live_builds.get() + 1);
                        ui.text("live.label")
                            .size(100.0, 40.0)
                            .text(format!("live {}", live_builds.get()))
                            .build();
                    });
                    ui.retained_scope("static", |ui| {
                        static_builds.set(static_builds.get() + 1);
                        ui.text("static.label")
                            .size(100.0, 40.0)
                            .text("static")
                            .build();
                    });
                });
            });
        };

        compose(&mut runtime, Vec::new());
        compose(&mut runtime, Vec::new());

        assert_eq!(live_builds.get(), 2);
        assert_eq!(static_builds.get(), 1);
        assert_eq!(runtime.find("live.label").unwrap().text, "live 2");
        assert!(runtime.retained_compose_stats().built >= 1);
        assert!(runtime.retained_compose_stats().reused >= 1);
        assert!(runtime.retained_compose_stats().partial_layout);
        assert!(!runtime.retained_compose_stats().full_layout);
    }

    #[test]
    fn clock_read_marks_active_scope_live_and_rebuilds_next_scoped_compose() {
        let mut runtime = Runtime::new("page");
        let builds = Rc::new(Cell::new(0));
        let mut sampled_seconds = 0.0;

        let compose = |runtime: &mut Runtime, dirty_ids: Vec<String>, sampled_seconds: &mut f32| {
            let builds = builds.clone();
            runtime.compose_incremental(240.0, 80.0, dirty_ids, move |ui, _| {
                ui.retained_scope("clocked", |ui| {
                    builds.set(builds.get() + 1);
                    let seconds = ui.clock().seconds();
                    let tick = ui.clock().every(Duration::from_millis(250));
                    assert_eq!(tick.period, Duration::from_millis(250));
                    *sampled_seconds = seconds;
                    ui.text("label")
                        .size(100.0, 40.0)
                        .text(format!("clock {seconds:.1}"))
                        .build();
                });
                ui.retained_scope("static", |ui| {
                    ui.text("static.label")
                        .size(100.0, 40.0)
                        .text("static")
                        .build();
                });
            });
        };

        compose(&mut runtime, Vec::new(), &mut sampled_seconds);
        assert_eq!(builds.get(), 1);
        assert_eq!(sampled_seconds, 0.0);
        assert_eq!(
            runtime.debug_snapshot().clock_ids,
            vec!["page.clocked".to_string()]
        );

        runtime.update_events_and_timers(
            PointerEvent::default(),
            ScrollEvent::default(),
            KeyboardEvent::default(),
            0.5,
        );
        compose(&mut runtime, Vec::new(), &mut sampled_seconds);

        assert_eq!(builds.get(), 2);
        assert_eq!(sampled_seconds, 0.5);
        assert_eq!(runtime.find("label").unwrap().text, "clock 0.5");
        assert_eq!(
            runtime.debug_snapshot().dirty_ids,
            vec!["page.clocked".to_string()]
        );
        assert_eq!(
            runtime.debug_snapshot().clock_ids,
            vec!["page.clocked".to_string()]
        );
        let clocked_record = runtime
            .debug_snapshot()
            .retained
            .iter()
            .find(|scope| scope.id == "page.clocked")
            .expect("clocked scope should be reported");
        assert_eq!(clocked_record.dirty_reasons, vec![DirtyReason::Clock]);
        assert!(runtime.retained_compose_stats().partial_layout);
    }

    #[test]
    fn clock_every_rebuilds_only_when_period_bucket_changes() {
        let mut runtime = Runtime::new("page");
        let builds = Rc::new(Cell::new(0));

        let compose = |runtime: &mut Runtime, dirty_ids: Vec<String>| {
            let builds = builds.clone();
            runtime.compose_incremental(240.0, 80.0, dirty_ids, move |ui, _| {
                ui.retained_scope("clocked", |ui| {
                    builds.set(builds.get() + 1);
                    let tick = ui.clock().every(Duration::from_millis(250));
                    ui.text("label")
                        .size(100.0, 40.0)
                        .text(format!("tick {}", tick.frame_index))
                        .build();
                });
            });
        };

        compose(&mut runtime, Vec::new());
        assert_eq!(builds.get(), 1);
        assert_eq!(
            runtime.debug_snapshot().clock_ids,
            vec!["page.clocked".to_string()]
        );

        runtime.update_events_and_timers(
            PointerEvent::default(),
            ScrollEvent::default(),
            KeyboardEvent::default(),
            0.1,
        );
        compose(&mut runtime, Vec::new());
        assert_eq!(builds.get(), 1);
        assert!(runtime.retained_compose_stats().partial_layout);

        runtime.update_events_and_timers(
            PointerEvent::default(),
            ScrollEvent::default(),
            KeyboardEvent::default(),
            0.2,
        );
        compose(&mut runtime, Vec::new());
        assert_eq!(builds.get(), 2);
        assert_eq!(
            runtime.debug_snapshot().dirty_ids,
            vec!["page.clocked".to_string()]
        );
    }

    #[test]
    fn clock_read_without_scope_uses_current_element_owner() {
        let mut runtime = Runtime::new("page");

        runtime.compose_incremental(240.0, 80.0, Vec::<String>::new(), |ui, _| {
            ui.stack("panel").size(160.0, 40.0).content(|ui| {
                let seconds = ui.clock().seconds();
                ui.text("panel.label")
                    .size(120.0, 24.0)
                    .text(format!("{seconds:.1}"))
                    .build();
            });
        });

        assert_eq!(
            runtime.debug_snapshot().clock_ids,
            vec!["page.panel".to_string()]
        );

        runtime.update_events_and_timers(
            PointerEvent::default(),
            ScrollEvent::default(),
            KeyboardEvent::default(),
            0.25,
        );
        runtime.compose_incremental(240.0, 80.0, Vec::<String>::new(), |ui, _| {
            ui.stack("panel").size(160.0, 40.0).content(|ui| {
                let seconds = ui.clock().seconds();
                ui.text("panel.label")
                    .size(120.0, 24.0)
                    .text(format!("{seconds:.2}"))
                    .build();
            });
        });

        assert_eq!(
            runtime.debug_snapshot().dirty_ids,
            vec!["page.panel".to_string()]
        );
        assert_eq!(runtime.find("panel.label").unwrap().text, "0.25");
        let panel_record = runtime
            .debug_snapshot()
            .retained
            .iter()
            .find(|scope| scope.id == "page.panel")
            .expect("automatic clock owner should be reported");
        assert_eq!(panel_record.dirty_reasons, vec![DirtyReason::Clock]);
    }

    #[test]
    fn debug_snapshot_records_scope_and_element_ancestry() {
        let mut runtime = Runtime::new("page");

        runtime.compose(240.0, 120.0, |ui, _| {
            ui.retained_scope("panel", |ui| {
                ui.stack("panel.scroll")
                    .size(200.0, 100.0)
                    .clip()
                    .on_scroll(|_| {})
                    .content(|ui| {
                        ui.retained_scope("child", |ui| {
                            ui.rect("panel.child.leaf").size(40.0, 20.0).build();
                        });
                    });
            });
        });

        let snapshot = runtime.debug_snapshot();
        let child_record = snapshot
            .retained
            .iter()
            .find(|scope| scope.id == "page.panel.child")
            .expect("child scope should be reported");
        assert_eq!(child_record.parent_id.as_deref(), Some("page.panel.scroll"));
        assert_eq!(child_record.current_roots, 1);
        assert_eq!(child_record.action, Some(RetainedComposeAction::Built));

        let leaf = snapshot
            .elements
            .iter()
            .find(|element| element.id == "page.panel.child.leaf")
            .expect("leaf element should be reported");
        assert_eq!(leaf.parent.as_deref(), Some("page.panel.scroll"));
        assert_eq!(leaf.retained_boundary.as_deref(), Some("page.panel.child"));
        assert_eq!(leaf.scroll_ancestor.as_deref(), Some("page.panel.scroll"));
        assert_eq!(leaf.clip_ancestor.as_deref(), Some("page.panel.scroll"));
        assert_eq!(leaf.draw_frame, Some(leaf.target_frame));
    }

    #[test]
    fn debug_snapshot_uses_tree_ancestry_for_non_prefixed_scope_ids() {
        let mut runtime = Runtime::new("page");

        runtime.compose(240.0, 120.0, |ui, _| {
            ui.retained_scope("panel", |ui| {
                ui.stack("host").size(200.0, 100.0).content(|ui| {
                    ui.retained_scope("page.live", |ui| {
                        ui.rect("live.leaf").size(40.0, 20.0).build();
                    });
                });
            });
        });

        let snapshot = runtime.debug_snapshot();
        let live_record = snapshot
            .retained
            .iter()
            .find(|scope| scope.id == "page.live")
            .expect("non-prefixed child scope should be reported");
        assert_eq!(live_record.parent_id.as_deref(), Some("page.host"));

        let leaf = snapshot
            .elements
            .iter()
            .find(|element| element.id == "page.live.leaf")
            .expect("non-prefixed child leaf should be reported");
        assert_eq!(leaf.parent.as_deref(), Some("page.host"));
        assert_eq!(leaf.retained_boundary.as_deref(), Some("page.live"));
    }

    #[test]
    fn dirty_non_prefixed_child_scope_prevents_parent_reuse_by_tree() {
        let mut runtime = Runtime::new("page");
        let parent_builds = Rc::new(Cell::new(0));
        let child_builds = Rc::new(Cell::new(0));

        let compose = |runtime: &mut Runtime, dirty_ids: Vec<String>| {
            let parent_builds = parent_builds.clone();
            let child_builds = child_builds.clone();
            runtime.compose_incremental(240.0, 120.0, dirty_ids, move |ui, _| {
                ui.retained_scope("panel", |ui| {
                    parent_builds.set(parent_builds.get() + 1);
                    ui.stack("host").size(200.0, 100.0).content(|ui| {
                        ui.retained_scope("page.live", |ui| {
                            child_builds.set(child_builds.get() + 1);
                            ui.text("live.label")
                                .size(120.0, 24.0)
                                .text(format!("child {}", child_builds.get()))
                                .build();
                        });
                    });
                });
            });
        };

        compose(&mut runtime, Vec::new());
        compose(&mut runtime, vec!["page.live".to_string()]);

        assert_eq!(parent_builds.get(), 2);
        assert_eq!(child_builds.get(), 2);
        assert_eq!(runtime.find("live.label").unwrap().text, "child 2");
        assert_eq!(
            runtime.debug_snapshot().normalized_dirty_ids,
            vec!["page.live".to_string()]
        );
    }

    #[test]
    fn dirty_parent_normalizes_live_child_for_layout_but_still_rebuilds_child() {
        let mut runtime = Runtime::new("page");
        let parent_builds = Rc::new(Cell::new(0));
        let child_builds = Rc::new(Cell::new(0));

        let compose = |runtime: &mut Runtime, dirty_ids: Vec<String>| {
            let parent_builds = parent_builds.clone();
            let child_builds = child_builds.clone();
            runtime.compose_incremental(240.0, 80.0, dirty_ids, move |ui, _| {
                ui.retained_scope("parent", |ui| {
                    parent_builds.set(parent_builds.get() + 1);
                    ui.row("row").size(240.0, 40.0).content(|ui| {
                        ui.retained_live_scope("child", |ui| {
                            child_builds.set(child_builds.get() + 1);
                            ui.text("label")
                                .size(100.0, 40.0)
                                .text(format!("child {}", child_builds.get()))
                                .build();
                        });
                    });
                });
            });
        };

        compose(&mut runtime, Vec::new());
        compose(&mut runtime, vec!["page.parent".to_string()]);

        assert_eq!(parent_builds.get(), 2);
        assert_eq!(child_builds.get(), 2);
        assert_eq!(runtime.find("label").unwrap().text, "child 2");
        assert_eq!(
            runtime.debug_snapshot().dirty_ids,
            vec!["page.parent".to_string(), "page.parent.child".to_string()]
        );
        assert_eq!(
            runtime.debug_snapshot().normalized_dirty_ids,
            vec!["page.parent".to_string()]
        );
        assert!(runtime.retained_compose_stats().partial_layout);
    }

    #[test]
    fn live_child_inside_dirty_scroll_parent_tracks_scroll_offset() {
        let mut runtime = Runtime::new("page");
        let live_builds = Rc::new(Cell::new(0));

        let compose = |runtime: &mut Runtime, dirty_ids: Vec<String>, offset: f32| {
            let live_builds = live_builds.clone();
            runtime.compose_incremental(240.0, 120.0, dirty_ids, move |ui, _| {
                ui.retained_scope("panel", |ui| {
                    ui.scroll_y("scroll")
                        .size(200.0, 80.0)
                        .content_height(180.0)
                        .offset(offset)
                        .content(|ui| {
                            ui.stack("top").size(Size::fill(), 60.0).build();
                            ui.retained_live_scope("secret.live", |ui| {
                                live_builds.set(live_builds.get() + 1);
                                ui.stack("secret").size(Size::fill(), 40.0).build();
                            });
                        });
                });
            });
        };

        compose(&mut runtime, Vec::new(), 0.0);
        let first = runtime.find("secret").unwrap().frame;

        compose(&mut runtime, vec!["page.panel".to_string()], 30.0);
        let second = runtime.find("secret").unwrap().frame;

        assert_eq!(live_builds.get(), 2);
        assert!(
            (second.y - (first.y - 30.0)).abs() < 0.001,
            "live child should remain attached to scroll content: first={first:?} second={second:?}"
        );
        assert_eq!(
            runtime.debug_snapshot().dirty_ids,
            vec![
                "page.panel".to_string(),
                "page.panel.secret.live".to_string()
            ]
        );
        assert_eq!(
            runtime.debug_snapshot().normalized_dirty_ids,
            vec!["page.panel".to_string()]
        );
        assert!(runtime.retained_compose_stats().partial_layout);
    }

    #[test]
    fn partial_layout_preserves_parent_assigned_grow_frame_for_live_scope_root() {
        let mut runtime = Runtime::new("page");
        let live_builds = Rc::new(Cell::new(0));

        let compose = |runtime: &mut Runtime, dirty_ids: Vec<String>| {
            let live_builds = live_builds.clone();
            runtime.compose_incremental(500.0, 100.0, dirty_ids, move |ui, _| {
                ui.row("root").size(500.0, 80.0).gap(20.0).content(|ui| {
                    ui.stack("left").size(100.0, 80.0).build();
                    ui.retained_live_scope("live", |ui| {
                        live_builds.set(live_builds.get() + 1);
                        ui.stack("center")
                            .size(120.0, 80.0)
                            .grow(1.0)
                            .min_width(120.0)
                            .content(|ui| {
                                ui.stack("viewport")
                                    .size(Size::fill(), Size::fill())
                                    .clip()
                                    .content(|ui| {
                                        ui.stack("child").size(Size::fill(), 40.0).build();
                                    });
                            });
                    });
                    ui.stack("right").size(100.0, 80.0).build();
                });
            });
        };

        compose(&mut runtime, Vec::new());
        let first_center = runtime.find("center").unwrap().frame;
        let first_viewport = runtime.find("viewport").unwrap().frame;
        assert_frame(first_center, 120.0, 0.0, 260.0, 80.0);
        assert_frame(first_viewport, 120.0, 0.0, 260.0, 80.0);

        compose(&mut runtime, Vec::new());
        let second_center = runtime.find("center").unwrap().frame;
        let second_viewport = runtime.find("viewport").unwrap().frame;
        assert_frame(second_center, 120.0, 0.0, 260.0, 80.0);
        assert_frame(second_viewport, 120.0, 0.0, 260.0, 80.0);
        assert_eq!(live_builds.get(), 2);
        assert!(runtime.retained_compose_stats().partial_layout);
    }

    #[test]
    fn dirty_scope_with_same_structure_uses_partial_layout() {
        let mut runtime = Runtime::new("page");
        let mut value = 0;

        runtime.compose_incremental(240.0, 80.0, Vec::<String>::new(), |ui, _| {
            ui.retained_scope("body", |ui| {
                ui.text("label")
                    .size(100.0, 40.0)
                    .text(format!("value {value}"))
                    .build();
            });
        });

        value = 1;
        runtime.compose_incremental(240.0, 80.0, ["page.body".to_string()], |ui, _| {
            ui.retained_scope("body", |ui| {
                ui.text("label")
                    .size(100.0, 40.0)
                    .text(format!("value {value}"))
                    .build();
            });
        });

        assert_eq!(runtime.find("label").unwrap().text, "value 1");
        assert!(runtime.retained_compose_stats().partial_layout);
        assert!(!runtime.retained_compose_stats().full_layout);
        assert_eq!(runtime.debug_snapshot().layout_mode, LayoutMode::Partial);
        assert_eq!(
            runtime.debug_snapshot().dirty_ids,
            vec!["page.body".to_string()]
        );
    }

    #[test]
    fn dirty_scope_with_changed_structure_uses_full_layout_fallback() {
        let mut runtime = Runtime::new("page");

        runtime.compose_incremental(240.0, 80.0, Vec::<String>::new(), |ui, _| {
            ui.retained_scope("body", |ui| {
                ui.text("motion").size(100.0, 40.0).text("motion").build();
            });
        });

        runtime.compose_incremental(240.0, 80.0, ["page.body".to_string()], |ui, _| {
            ui.retained_scope("body", |ui| {
                ui.row("chart").size(120.0, 40.0).content(|ui| {
                    ui.text("bar").size(60.0, 40.0).text("bar").build();
                    ui.text("pie").size(60.0, 40.0).text("pie").build();
                });
            });
        });

        assert!(runtime.find("chart").is_some());
        assert!(!runtime.retained_compose_stats().partial_layout);
        assert!(runtime.retained_compose_stats().full_layout);
        assert_eq!(
            runtime.debug_snapshot().layout_mode,
            LayoutMode::Full(FullLayoutReason::StructureChanged {
                ids: vec!["page.body".to_string()]
            })
        );
    }

    #[test]
    fn dirty_scope_with_changed_fixed_size_uses_full_layout_fallback() {
        let mut runtime = Runtime::new("page");

        runtime.compose(240.0, 80.0, |ui, _| {
            ui.row("root").size(240.0, 40.0).content(|ui| {
                ui.retained_scope("left", |ui| {
                    ui.rect("left.box").size(40.0, 40.0).build();
                });
                ui.rect("right").size(40.0, 40.0).build();
            });
        });
        let right_before = runtime.find("right").unwrap().frame;

        runtime.compose_incremental(240.0, 80.0, vec!["page.left".to_string()], |ui, _| {
            ui.row("root").size(240.0, 40.0).content(|ui| {
                ui.retained_scope("left", |ui| {
                    ui.rect("left.box").size(80.0, 40.0).build();
                });
                ui.rect("right").size(40.0, 40.0).build();
            });
        });

        let right_after = runtime.find("right").unwrap().frame;
        assert_eq!(right_before.x, 40.0);
        assert_eq!(right_after.x, 80.0);
        assert_eq!(
            runtime.debug_snapshot().layout_mode,
            LayoutMode::Full(FullLayoutReason::StructureChanged {
                ids: vec!["page.left".to_string()]
            })
        );
    }

    #[test]
    fn root_dirty_scope_without_retained_scope_roots_uses_full_layout_fallback() {
        let mut runtime = Runtime::new("page");
        runtime.compose(240.0, 80.0, |ui, _| {
            ui.row("root").size(240.0, 40.0).content(|ui| {
                for index in 0..8 {
                    button(ui, format!("button.{index}"))
                        .size(24.0, 24.0)
                        .text(index.to_string())
                        .build();
                }
            });
        });

        runtime.compose_incremental(240.0, 80.0, ["page".to_string()], |ui, _| {
            ui.row("root").size(240.0, 40.0).content(|ui| {
                for index in 0..8 {
                    button(ui, format!("button.{index}"))
                        .size(24.0, 24.0)
                        .text(index.to_string())
                        .build();
                }
            });
        });

        assert!(!runtime.retained_compose_stats().partial_layout);
        assert!(runtime.retained_compose_stats().full_layout);
    }

    #[test]
    fn eui_demo_layout_matches_original_frames() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(800.0, 600.0, |ui, screen| {
            ui.stack("root")
                .size(screen.width, screen.height)
                .align(Align::Center, Align::Center)
                .content(|ui| {
                    panel(ui, "card")
                        .size(360.0, 260.0)
                        .radius(18.0)
                        .gradient(
                            Color::new(0.10, 0.12, 0.16, 1.0),
                            Color::new(0.05, 0.07, 0.10, 1.0),
                        )
                        .border(1.0, Color::new(0.23, 0.29, 0.38, 1.0))
                        .shadow(26.0, 0.0, 8.0, Color::new(0.0, 0.0, 0.0, 0.26))
                        .build();

                    ui.column("content")
                        .size(360.0, 260.0)
                        .gap(8.0)
                        .justify_content(Align::Center)
                        .align_items(Align::Center)
                        .content(|ui| {
                            text(ui, "title")
                                .size(300.0, 38.0)
                                .text("Hello EUI")
                                .font_size(30.0)
                                .line_height(38.0)
                                .color(Color::new(0.94, 0.97, 1.0, 1.0))
                                .horizontal_align(HorizontalAlign::Center)
                                .build();

                            text(ui, "subtitle")
                                .size(300.0, 30.0)
                                .margin_each(0.0, 0.0, 0.0, 16.0)
                                .text("Text Button Component")
                                .font_size(24.0)
                                .line_height(30.0)
                                .color(Color::new(0.62, 0.70, 0.82, 1.0))
                                .horizontal_align(HorizontalAlign::Center)
                                .build();

                            button(ui, "primary")
                                .size(240.0, 70.0)
                                .text("Click Me")
                                .build();
                        });
                });
        });

        assert_frame(runtime.find("root").unwrap().frame, 0.0, 0.0, 800.0, 600.0);
        assert_frame(
            runtime.find("card").unwrap().frame,
            220.0,
            170.0,
            360.0,
            260.0,
        );
        assert_frame(
            runtime.find("content").unwrap().frame,
            220.0,
            170.0,
            360.0,
            260.0,
        );
        assert_frame(
            runtime.find("title").unwrap().frame,
            250.0,
            215.0,
            300.0,
            38.0,
        );
        assert_frame(
            runtime.find("subtitle").unwrap().frame,
            250.0,
            261.0,
            300.0,
            30.0,
        );
        assert_frame(
            runtime.find("primary").unwrap().frame,
            280.0,
            315.0,
            240.0,
            70.0,
        );
        assert_frame(
            runtime.find("primary.bg").unwrap().frame,
            280.0,
            315.0,
            240.0,
            70.0,
        );
        assert_frame(
            runtime.find("primary.content").unwrap().frame,
            280.0,
            315.0,
            240.0,
            70.0,
        );
        assert_frame(
            runtime.find("primary.text").unwrap().frame,
            280.0,
            315.0,
            240.0,
            70.0,
        );
    }

    #[test]
    fn runtime_marks_no_redraw_for_same_structure_and_size_after_rendered() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("a").size(10.0, 10.0).build();
        });
        runtime.mark_rendered();

        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("a").size(10.0, 10.0).build();
        });

        assert!(!runtime.needs_render());
        assert!(!runtime.full_redraw());
    }

    #[test]
    fn runtime_detects_structure_changes() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("a").size(10.0, 10.0).build();
        });
        runtime.mark_rendered();

        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("a").size(10.0, 10.0).build();
            ui.rect("b").size(10.0, 10.0).build();
        });

        assert!(runtime.needs_render());
        assert!(runtime.full_redraw());
    }

    #[test]
    fn runtime_detects_visual_changes_without_full_layout_redraw() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("panel").size(40.0, 20.0).color(Color::RED).build();
        });
        runtime.mark_rendered();

        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("panel").size(40.0, 20.0).color(Color::BLUE).build();
        });

        assert!(runtime.needs_render());
        assert!(!runtime.full_redraw());
    }

    #[test]
    fn runtime_treats_fixed_text_color_as_visual_only() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.text("title")
                .size(80.0, 20.0)
                .text("A")
                .color(Color::RED)
                .build();
        });
        runtime.mark_rendered();

        runtime.compose(100.0, 100.0, |ui, _| {
            ui.text("title")
                .size(80.0, 20.0)
                .text("A")
                .color(Color::BLUE)
                .build();
        });

        assert!(runtime.needs_render());
        assert!(!runtime.full_redraw());
    }

    #[test]
    fn runtime_treats_fixed_text_content_as_visual_only() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.text("title").size(80.0, 20.0).text("A").build();
        });
        runtime.mark_rendered();

        runtime.compose(100.0, 100.0, |ui, _| {
            ui.text("title").size(80.0, 20.0).text("B").build();
        });

        assert!(runtime.needs_render());
        assert!(!runtime.full_redraw());
    }

    #[test]
    fn runtime_detects_wrap_content_text_content_as_layout_affecting() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.text("title").wrap_content().text("A").build();
        });
        runtime.mark_rendered();

        runtime.compose(100.0, 100.0, |ui, _| {
            ui.text("title").wrap_content().text("Wider").build();
        });

        assert!(runtime.needs_render());
        assert!(runtime.full_redraw());
    }

    #[test]
    fn cached_draw_list_invalidates_when_visuals_change() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("panel").size(40.0, 20.0).color(Color::RED).build();
        });
        let first = runtime.draw_list();
        let first_cached = runtime.draw_list();
        let first_color = first
            .commands()
            .iter()
            .filter_map(rect_draw)
            .find(|draw| draw.id == "demo.panel")
            .map(|draw| draw.color)
            .expect("panel rect should draw");
        assert_eq!(first.revision(), first_cached.revision());
        assert_eq!(first_color, Color::RED);

        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("panel").size(40.0, 20.0).color(Color::BLUE).build();
        });
        let second = runtime.draw_list();
        let second_color = second
            .commands()
            .iter()
            .filter_map(rect_draw)
            .find(|draw| draw.id == "demo.panel")
            .map(|draw| draw.color)
            .expect("panel rect should draw");

        assert_ne!(first.revision(), second.revision());
        assert_eq!(second_color, Color::BLUE);
    }

    #[test]
    fn frame_transition_interpolates_draw_list_frame() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(200.0, 100.0, |ui, _| {
            ui.rect("bar")
                .size(10.0, 10.0)
                .transition(Transition::ease(1.0, Ease::Linear))
                .animate(AnimProperty::FRAME)
                .build();
        });
        runtime.tick_animations(0.0);
        runtime.mark_rendered();

        runtime.compose(200.0, 100.0, |ui, _| {
            ui.rect("bar")
                .size(110.0, 10.0)
                .transition(Transition::ease(1.0, Ease::Linear))
                .animate(AnimProperty::FRAME)
                .build();
        });
        assert!(runtime.tick_animations(0.5));

        let draw = runtime.draw_list();
        let bar = draw
            .commands()
            .iter()
            .filter_map(rect_draw)
            .find(|draw| draw.id == "demo.bar")
            .unwrap();

        assert!((bar.frame.width - 60.0).abs() < 0.001);
        assert!(runtime.needs_render());
    }

    #[test]
    fn active_animation_without_value_change_keeps_draw_list_cache() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(200.0, 100.0, |ui, _| {
            ui.rect("bar")
                .size(10.0, 10.0)
                .transition(Transition::ease(1.0, Ease::Linear))
                .animate(AnimProperty::FRAME)
                .build();
        });
        runtime.tick_animations(0.0);
        runtime.mark_rendered();

        runtime.compose(200.0, 100.0, |ui, _| {
            ui.rect("bar")
                .size(110.0, 10.0)
                .transition(Transition::ease(1.0, Ease::Linear))
                .animate(AnimProperty::FRAME)
                .build();
        });
        assert!(runtime.tick_animations(0.5));
        let first = runtime.draw_list();
        runtime.mark_rendered();

        assert!(!runtime.tick_animations(0.0));
        assert!(runtime.needs_render());
        let second = runtime.draw_list();

        assert_eq!(first.revision(), second.revision());
    }

    #[test]
    fn ancestor_frame_change_snaps_descendant_frame_animation() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(200.0, 100.0, |ui, _| {
            ui.stack("viewport").size(100.0, 80.0).clip().content(|ui| {
                ui.stack("content").size(100.0, 160.0).content(|ui| {
                    ui.rect("indicator")
                        .size(20.0, 10.0)
                        .transition(Transition::ease(1.0, Ease::Linear))
                        .animate(AnimProperty::FRAME)
                        .build();
                });
            });
        });
        runtime.tick_animations(0.0);
        runtime.mark_rendered();

        runtime.compose(200.0, 100.0, |ui, _| {
            ui.stack("viewport").size(100.0, 80.0).clip().content(|ui| {
                ui.stack("content")
                    .y(-4.0)
                    .size(100.0, 160.0)
                    .content(|ui| {
                        ui.rect("indicator")
                            .size(20.0, 10.0)
                            .transition(Transition::ease(1.0, Ease::Linear))
                            .animate(AnimProperty::FRAME)
                            .build();
                    });
            });
        });

        assert!(runtime.tick_animations(0.016));

        let draw = runtime.draw_list();
        let indicator = draw
            .commands()
            .iter()
            .filter_map(rect_draw)
            .find(|draw| draw.id == "demo.indicator")
            .unwrap();

        assert_frame(indicator.frame, 0.0, -4.0, 20.0, 10.0);
    }

    #[test]
    fn state_color_uses_smoothed_hover_blend() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("button")
                .size(40.0, 40.0)
                .states(Color::BLACK, Color::RED, Color::GREEN)
                .build();
        });
        runtime.tick_animations(0.0);

        runtime.update_pointer(PointerEvent::at(5.0, 5.0));
        assert!(runtime.tick_animations(0.016));

        let draw = runtime.draw_list();
        let button = draw
            .commands()
            .iter()
            .filter_map(rect_draw)
            .find(|draw| draw.id == "demo.button")
            .unwrap();

        assert!(button.color.r > 0.0);
        assert!(button.color.r < 1.0);
    }

    #[test]
    fn pointer_press_and_release_produces_click_response() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("button").size(40.0, 30.0).interactive(true).build();
        });
        runtime.mark_rendered();

        runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));

        assert!(runtime.response("button").clicked());
        assert!(runtime.needs_render());
    }

    #[test]
    fn pointer_hover_leave_reports_changed_response() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.rect("button").size(40.0, 30.0).interactive(true).build();
        });

        runtime.update_pointer(PointerEvent::at(10.0, 10.0));
        assert!(runtime.response("button").hovered());

        runtime.update_pointer(PointerEvent::at(90.0, 90.0));
        let response = runtime.response("button");
        assert!(!response.hovered());
        assert!(response.changed());
    }

    #[test]
    fn press_capture_prevents_other_element_click() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.stack("root").size(100.0, 100.0).content(|ui| {
                ui.rect("a")
                    .position(0.0, 0.0)
                    .size(40.0, 40.0)
                    .interactive(true)
                    .build();
                ui.rect("b")
                    .position(50.0, 0.0)
                    .size(40.0, 40.0)
                    .interactive(true)
                    .build();
            });
        });

        runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
        runtime.update_pointer(PointerEvent::dragged_to(60.0, 10.0, 50.0, 0.0));
        runtime.update_pointer(PointerEvent::released_at(60.0, 10.0));

        assert!(!runtime.response("a").clicked());
        assert!(!runtime.response("b").clicked());
        assert!(runtime.interaction("a").released);
    }

    #[test]
    fn z_index_controls_topmost_hit_test() {
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, |ui, _| {
            ui.stack("root").size(100.0, 100.0).content(|ui| {
                ui.rect("low")
                    .size(50.0, 50.0)
                    .interactive(true)
                    .z_index(0)
                    .build();
                ui.rect("high")
                    .size(50.0, 50.0)
                    .interactive(true)
                    .z_index(10)
                    .build();
            });
        });

        runtime.update_pointer(PointerEvent::at(10.0, 10.0));

        assert!(!runtime.response("low").hovered());
        assert!(runtime.response("high").hovered());
    }

    #[test]
    fn click_callback_runs_from_runtime_dispatch() {
        let clicks = Rc::new(Cell::new(0));
        let callback_clicks = clicks.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, move |ui, _| {
            let callback_clicks = callback_clicks.clone();
            ui.rect("button")
                .size(40.0, 30.0)
                .on_click(move || {
                    callback_clicks.set(callback_clicks.get() + 1);
                })
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(10.0, 10.0));
        runtime.update_pointer(PointerEvent::released_at(10.0, 10.0));

        assert_eq!(clicks.get(), 1);
        assert!(runtime.needs_compose());
    }

    #[test]
    fn right_click_dispatches_context_menu_callback() {
        let opened = Rc::new(Cell::new(false));
        let callback_opened = opened.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, move |ui, _| {
            let callback_opened = callback_opened.clone();
            ui.rect("target")
                .size(40.0, 30.0)
                .on_context_menu(move |_, _| {
                    callback_opened.set(true);
                })
                .build();
        });

        runtime.update_pointer(PointerEvent::right_pressed_at(10.0, 10.0));

        assert!(opened.get());
    }

    #[test]
    fn focused_element_receives_keyboard_input() {
        let text = Rc::new(RefCell::new(String::new()));
        let callback_text = text.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, move |ui, _| {
            let callback_text = callback_text.clone();
            ui.rect("input")
                .size(80.0, 24.0)
                .on_text_input(move |event| {
                    callback_text.borrow_mut().push_str(&event.text);
                })
                .build();
        });

        runtime.update_pointer(PointerEvent::pressed_at(5.0, 5.0));
        runtime.update_keyboard(KeyboardEvent {
            text: "A".to_string(),
            ..KeyboardEvent::default()
        });

        assert_eq!(text.borrow().as_str(), "A");
        assert_eq!(runtime.focused_id(), Some("demo.input"));
    }

    #[test]
    fn scroll_dispatches_to_topmost_scrollable_element() {
        let amount = Rc::new(Cell::new(0.0));
        let callback_amount = amount.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, move |ui, _| {
            let callback_amount = callback_amount.clone();
            ui.rect("scroll")
                .size(80.0, 80.0)
                .on_scroll(move |event| {
                    callback_amount.set(callback_amount.get() + event.y);
                })
                .build();
        });

        runtime.update_pointer(PointerEvent::at(5.0, 5.0));
        runtime.update_scroll(ScrollEvent { x: 0.0, y: 3.0 });

        assert_eq!(amount.get(), 3.0);
    }

    #[test]
    fn rounded_clip_excludes_corner_hits() {
        let clicks = Rc::new(Cell::new(0));
        let callback_clicks = clicks.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(120.0, 120.0, move |ui, _| {
            let callback_clicks = callback_clicks.clone();
            ui.stack("viewport")
                .size(100.0, 100.0)
                .rounded_clip(20.0)
                .content(|ui| {
                    ui.rect("child")
                        .size(100.0, 100.0)
                        .on_click(move || callback_clicks.set(callback_clicks.get() + 1))
                        .build();
                });
        });

        runtime.update_pointer(PointerEvent::pressed_at(1.0, 1.0));
        runtime.update_pointer(PointerEvent::released_at(1.0, 1.0));
        assert_eq!(clicks.get(), 0);

        runtime.update_pointer(PointerEvent::pressed_at(20.0, 20.0));
        runtime.update_pointer(PointerEvent::released_at(20.0, 20.0));
        assert_eq!(clicks.get(), 1);
    }

    #[test]
    fn fullscreen_modal_layer_blocks_underlying_hits() {
        let under_clicks = Rc::new(Cell::new(0));
        let modal_clicks = Rc::new(Cell::new(0));
        let under = under_clicks.clone();
        let modal = modal_clicks.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(200.0, 120.0, move |ui, _| {
            let under = under.clone();
            let modal = modal.clone();
            ui.rect("under")
                .size(200.0, 120.0)
                .z_index(0)
                .on_click(move || under.set(under.get() + 1))
                .build();
            ui.stack("modal")
                .size(200.0, 120.0)
                .z_index(1000)
                .content(|ui| {
                    ui.rect("modal.backdrop")
                        .size(200.0, 120.0)
                        .on_click(move || modal.set(modal.get() + 1))
                        .build();
                });
        });

        runtime.update_pointer(PointerEvent::pressed_at(20.0, 20.0));
        runtime.update_pointer(PointerEvent::released_at(20.0, 20.0));

        assert_eq!(under_clicks.get(), 0);
        assert_eq!(modal_clicks.get(), 1);
        assert!(!runtime.response("under").hovered());
        assert!(runtime.response("modal.backdrop").clicked());
    }

    #[test]
    fn timer_callback_runs_after_elapsed_duration() {
        let fired = Rc::new(Cell::new(false));
        let callback_fired = fired.clone();
        let mut runtime = Runtime::new("demo");
        runtime.compose(100.0, 100.0, move |ui, _| {
            let callback_fired = callback_fired.clone();
            ui.rect("timer")
                .size(1.0, 1.0)
                .on_timer(0.10, move || {
                    callback_fired.set(true);
                })
                .build();
        });

        assert!(!runtime.tick_timers(0.05));
        assert!(runtime.tick_timers(0.05));
        assert!(fired.get());
    }

    fn assert_frame(frame: LayoutRect, x: f32, y: f32, width: f32, height: f32) {
        const EPSILON: f32 = 0.001;
        assert!(
            (frame.x - x).abs() < EPSILON
                && (frame.y - y).abs() < EPSILON
                && (frame.width - width).abs() < EPSILON
                && (frame.height - height).abs() < EPSILON,
            "frame mismatch: got {:?}, expected ({x}, {y}, {width}, {height})",
            frame
        );
    }

    fn rect_draw(command: &UiDrawCommand) -> Option<&UiRectDraw> {
        match command {
            UiDrawCommand::Rect(draw) => Some(draw),
            _ => None,
        }
    }
}
