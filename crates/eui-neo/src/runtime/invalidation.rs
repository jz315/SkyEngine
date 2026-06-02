use super::*;
use crate::DirtyFlags;

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct PassFlags {
    pub request_compose_ui: bool,
    pub request_reconcile: bool,
    pub request_layout: bool,
    pub request_layer: bool,
    pub request_focus: bool,
    pub request_hit: bool,
    pub request_draw: bool,
    pub request_platform_effects: bool,
}

impl PassFlags {
    pub(super) fn union(&mut self, other: Self) {
        self.request_compose_ui |= other.request_compose_ui;
        self.request_reconcile |= other.request_reconcile;
        self.request_layout |= other.request_layout;
        self.request_layer |= other.request_layer;
        self.request_focus |= other.request_focus;
        self.request_hit |= other.request_hit;
        self.request_draw |= other.request_draw;
        self.request_platform_effects |= other.request_platform_effects;
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct NormalizedDirtyInput {
    pub(super) invalidations: Vec<Invalidation>,
    pub(super) compose_scopes: ScopeSet,
    pub(super) layout_scopes: ScopeSet,
    pub(super) pass_flags: PassFlags,
}

impl NormalizedDirtyInput {
    pub(super) fn from_optional_dirty_inputs(records: Option<Vec<DirtyInput>>) -> Option<Self> {
        records.map(|records| Self::from_dirty_inputs(records))
    }

    pub(super) fn from_dirty_inputs(records: Vec<DirtyInput>) -> Self {
        Self::from_dirty_input_slice(&records)
    }

    pub(super) fn from_dirty_input_slice(records: &[DirtyInput]) -> Self {
        let mut normalized = Self::default();
        for record in records {
            normalized.push_invalidation(Invalidation::dirty_input(record));
        }
        normalized
    }

    fn push_invalidation(&mut self, invalidation: Invalidation) {
        self.pass_flags.union(invalidation.pass_flags);
        if let InvalidationTarget::Scope(scope) = &invalidation.target {
            if invalidation.pass_flags.request_compose_ui {
                self.compose_scopes.insert(scope.clone());
            }
            if invalidation.pass_flags.request_layout {
                self.layout_scopes.insert(scope.clone());
            }
        }
        self.invalidations.push(invalidation);
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.invalidations.extend(other.invalidations);
        self.compose_scopes.extend(other.compose_scopes);
        self.layout_scopes.extend(other.layout_scopes);
        self.pass_flags.union(other.pass_flags);
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InvalidationTarget {
    Scope(ScopeId),
    Node(NodeId),
    Focus(NodeId),
    Text(NodeId),
    Scroll(NodeId),
    Element(NodeId),
    Layer(LayerId),
}

impl InvalidationTarget {
    pub fn id(&self) -> &str {
        match self {
            Self::Scope(id) => id.as_str(),
            Self::Node(id)
            | Self::Focus(id)
            | Self::Text(id)
            | Self::Scroll(id)
            | Self::Element(id) => id.as_str(),
            Self::Layer(id) => id.as_str(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Scope(_) => "scope",
            Self::Node(_) => "node",
            Self::Focus(_) => "focus",
            Self::Text(_) => "text",
            Self::Scroll(_) => "scroll",
            Self::Element(_) => "element",
            Self::Layer(_) => "layer",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InvalidationSource {
    Signal(SignalKey),
    Event(EventSource),
    Timer(TimerSource),
    Resource(ResourceDirtySource),
    Runtime(&'static str),
}

impl InvalidationSource {
    pub fn label(&self) -> &str {
        match self {
            Self::Signal(value) => value.as_str(),
            Self::Event(value) => value.label(),
            Self::Timer(value) => value.label(),
            Self::Resource(value) => value.as_str(),
            Self::Runtime(value) => value,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Signal(_) => "signal",
            Self::Event(_) => "event",
            Self::Timer(_) => "timer",
            Self::Resource(_) => "resource",
            Self::Runtime(_) => "runtime",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum EventSource {
    Press,
    Click,
    ContextMenu,
    Drag,
    TextInput,
    Scroll,
    Focus,
    LayerDismiss,
}

impl EventSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Press => "press",
            Self::Click => "click",
            Self::ContextMenu => "context_menu",
            Self::Drag => "drag",
            Self::TextInput => "text_input",
            Self::Scroll => "scroll",
            Self::Focus => "focus",
            Self::LayerDismiss => "dismiss",
        }
    }

    pub fn raw_event(self) -> &'static str {
        match self {
            Self::Press | Self::Click | Self::ContextMenu | Self::Drag => "pointer",
            Self::TextInput => "keyboard",
            Self::Scroll => "scroll",
            Self::Focus => "focus",
            Self::LayerDismiss => "layer",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum TimerSource {
    Timer,
}

impl TimerSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Timer => "timer",
        }
    }

    pub fn raw_event(self) -> &'static str {
        match self {
            Self::Timer => "timer",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InvalidationPropagation {
    SelfOnly,
    Children,
    Subtree,
    Ancestors,
    LayerOwner,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Invalidation {
    pub target: InvalidationTarget,
    pub source: InvalidationSource,
    pub flags: DirtyFlags,
    pub pass_flags: PassFlags,
    pub propagation: InvalidationPropagation,
}

impl Invalidation {
    pub(super) fn event(target: EventTargetId, source: EventSource, flags: DirtyFlags) -> Self {
        let target = match target {
            EventTargetId::Node(id) => InvalidationTarget::Node(id),
            EventTargetId::Focus(id) => InvalidationTarget::Focus(id),
            EventTargetId::Scroll(id) => InvalidationTarget::Scroll(id),
            EventTargetId::Text(id) => InvalidationTarget::Text(id),
            EventTargetId::Layer(id) => InvalidationTarget::Layer(id),
        };
        Self {
            target,
            source: InvalidationSource::Event(source),
            flags,
            pass_flags: pass_flags_for_dirty_flags(flags),
            propagation: InvalidationPropagation::SelfOnly,
        }
    }

    pub(super) fn signal(
        target: impl Into<ScopeId>,
        source: impl Into<SignalKey>,
        flags: DirtyFlags,
    ) -> Self {
        Self {
            target: InvalidationTarget::Scope(target.into()),
            source: InvalidationSource::Signal(source.into()),
            flags,
            pass_flags: pass_flags_for_dirty_flags(flags),
            propagation: InvalidationPropagation::SelfOnly,
        }
    }

    pub(super) fn dirty_input(record: &DirtyInput) -> Self {
        if let Some(source) = record.source_key() {
            return Self::signal(record.scope_id().clone(), source.clone(), record.flags());
        }
        Self {
            target: InvalidationTarget::Scope(record.scope_id().clone()),
            source: InvalidationSource::Runtime("dirty_input"),
            flags: record.flags(),
            pass_flags: pass_flags_for_dirty_flags(record.flags()),
            propagation: InvalidationPropagation::SelfOnly,
        }
    }

    pub(super) fn runtime(target: NodeId, source: &'static str, flags: DirtyFlags) -> Self {
        Self {
            target: InvalidationTarget::Element(target),
            source: InvalidationSource::Runtime(source),
            flags,
            pass_flags: pass_flags_for_dirty_flags(flags),
            propagation: InvalidationPropagation::SelfOnly,
        }
    }

    pub(super) fn timer(target: NodeId) -> Self {
        Self {
            target: InvalidationTarget::Node(target),
            source: InvalidationSource::Timer(TimerSource::Timer),
            flags: DirtyFlags::COMPOSE | DirtyFlags::DRAW,
            pass_flags: pass_flags_for_dirty_flags(DirtyFlags::COMPOSE | DirtyFlags::DRAW),
            propagation: InvalidationPropagation::SelfOnly,
        }
    }

    pub(super) fn resource(target: NodeId, source: impl Into<ResourceDirtySource>) -> Self {
        Self::resource_with_flags(
            target,
            source,
            DirtyFlags::COMPOSE | DirtyFlags::LAYOUT | DirtyFlags::DRAW,
        )
    }

    pub(super) fn resource_with_flags(
        target: NodeId,
        source: impl Into<ResourceDirtySource>,
        flags: DirtyFlags,
    ) -> Self {
        Self {
            target: InvalidationTarget::Element(target),
            source: InvalidationSource::Resource(source.into()),
            flags,
            pass_flags: pass_flags_for_dirty_flags(flags),
            propagation: InvalidationPropagation::SelfOnly,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct InvalidationStore {
    records: Vec<Invalidation>,
    pass_flags: PassFlags,
}

impl InvalidationStore {
    pub(super) fn push(&mut self, invalidation: Invalidation) {
        if let Some(existing) = self.records.iter_mut().find(|existing| {
            existing.target == invalidation.target && existing.source == invalidation.source
        }) {
            existing.flags |= invalidation.flags;
            existing.pass_flags.union(invalidation.pass_flags);
            self.pass_flags.union(existing.pass_flags);
            return;
        }
        self.pass_flags.union(invalidation.pass_flags);
        self.records.push(invalidation);
    }

    pub(super) fn snapshot(&self) -> Vec<Invalidation> {
        let mut records = self.records.clone();
        records.sort_by(|left, right| {
            left.target
                .id()
                .cmp(right.target.id())
                .then_with(|| format!("{:?}", left.source).cmp(&format!("{:?}", right.source)))
        });
        records
    }

    pub(super) fn pass_flags(&self) -> PassFlags {
        self.pass_flags
    }

    pub(super) fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub(super) fn clear(&mut self) {
        self.records.clear();
        self.pass_flags = PassFlags::default();
    }
}

pub(super) fn pass_flags_for_dirty_flags(flags: DirtyFlags) -> PassFlags {
    PassFlags {
        request_compose_ui: flags.contains(DirtyFlags::COMPOSE),
        request_reconcile: flags.contains(DirtyFlags::COMPOSE),
        request_layout: flags.contains(DirtyFlags::LAYOUT),
        request_layer: flags.contains(DirtyFlags::LAYER),
        request_focus: flags.contains(DirtyFlags::FOCUS),
        request_hit: flags.intersects(DirtyFlags::LAYOUT | DirtyFlags::LAYER | DirtyFlags::HIT),
        request_draw: flags.intersects(
            DirtyFlags::LAYOUT | DirtyFlags::VISUAL | DirtyFlags::DRAW | DirtyFlags::LAYER,
        ),
        request_platform_effects: flags.intersects(DirtyFlags::FOCUS | DirtyFlags::PLATFORM),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_dirty_scope_sets_come_from_typed_invalidation_targets() {
        let mut normalized = NormalizedDirtyInput::default();

        normalized.push_invalidation(Invalidation {
            target: InvalidationTarget::Node(NodeId::new("page.node")),
            source: InvalidationSource::Runtime("test"),
            flags: DirtyFlags::COMPOSE | DirtyFlags::LAYOUT,
            pass_flags: pass_flags_for_dirty_flags(DirtyFlags::COMPOSE | DirtyFlags::LAYOUT),
            propagation: InvalidationPropagation::SelfOnly,
        });
        normalized.push_invalidation(Invalidation::signal(
            ScopeId::new("page.scope"),
            "signal",
            DirtyFlags::COMPOSE | DirtyFlags::LAYOUT,
        ));

        assert_eq!(normalized.invalidations.len(), 2);
        assert!(!normalized.compose_scopes.contains("page.node"));
        assert!(!normalized.layout_scopes.contains("page.node"));
        assert!(normalized.compose_scopes.contains("page.scope"));
        assert!(normalized.layout_scopes.contains("page.scope"));
        assert!(normalized.pass_flags.request_compose_ui);
        assert!(normalized.pass_flags.request_reconcile);
        assert!(normalized.pass_flags.request_layout);
    }

    #[test]
    fn dirty_flags_map_to_precise_pass_flags() {
        let compose = pass_flags_for_dirty_flags(DirtyFlags::COMPOSE);
        assert!(compose.request_compose_ui);
        assert!(compose.request_reconcile);
        assert!(!compose.request_layout);
        assert!(!compose.request_draw);

        let layout = pass_flags_for_dirty_flags(DirtyFlags::LAYOUT);
        assert!(layout.request_layout);
        assert!(layout.request_hit);
        assert!(layout.request_draw);
        assert!(!layout.request_compose_ui);

        let visual = pass_flags_for_dirty_flags(DirtyFlags::VISUAL);
        assert!(visual.request_draw);
        assert!(!visual.request_layout);
        assert!(!visual.request_hit);

        let layer = pass_flags_for_dirty_flags(DirtyFlags::LAYER);
        assert!(layer.request_layer);
        assert!(layer.request_hit);
        assert!(layer.request_draw);

        let focus = pass_flags_for_dirty_flags(DirtyFlags::FOCUS);
        assert!(focus.request_focus);
        assert!(focus.request_platform_effects);
        assert!(!focus.request_draw);

        let hit = pass_flags_for_dirty_flags(DirtyFlags::HIT);
        assert!(hit.request_hit);
        assert!(!hit.request_draw);

        let platform = pass_flags_for_dirty_flags(DirtyFlags::PLATFORM);
        assert!(platform.request_platform_effects);
        assert!(!platform.request_focus);
    }

    #[test]
    fn dirty_input_invalidation_reuses_typed_scope_payload() {
        let record = DirtyInput::new("page.scope", DirtyFlags::COMPOSE | DirtyFlags::DRAW);
        assert_eq!(record.scope_id().as_str(), "page.scope");

        let invalidation = Invalidation::dirty_input(&record);

        assert!(matches!(
            &invalidation.target,
            InvalidationTarget::Scope(scope) if scope == record.scope_id()
        ));
        assert_eq!(invalidation.target.id(), record.id());
        assert_eq!(
            invalidation.source,
            InvalidationSource::Runtime("dirty_input")
        );

        let signal_record = DirtyInput::signal(
            "page.scope",
            SignalKey::static_str("page"),
            DirtyFlags::COMPOSE,
        );
        let signal_invalidation = Invalidation::dirty_input(&signal_record);

        assert!(matches!(
            &signal_invalidation.target,
            InvalidationTarget::Scope(scope) if scope == signal_record.scope_id()
        ));
        assert_eq!(
            signal_invalidation.source,
            InvalidationSource::Signal(SignalKey::static_str("page"))
        );
    }

    #[test]
    fn normalized_dirty_input_preserves_distinct_signal_sources_for_same_scope() {
        let normalized = NormalizedDirtyInput::from_dirty_inputs(vec![
            DirtyInput::signal(
                "page.scope",
                SignalKey::static_str("first"),
                DirtyFlags::COMPOSE | DirtyFlags::DRAW,
            ),
            DirtyInput::signal(
                "page.scope",
                SignalKey::static_str("second"),
                DirtyFlags::COMPOSE | DirtyFlags::DRAW,
            ),
        ]);

        assert_eq!(normalized.invalidations.len(), 2);
        assert_eq!(normalized.compose_scopes.len(), 1);
        assert!(normalized.compose_scopes.contains("page.scope"));
        assert!(normalized.pass_flags.request_compose_ui);
        assert!(normalized.pass_flags.request_draw);
        assert!(normalized.invalidations.iter().any(|invalidation| {
            invalidation.source == InvalidationSource::Signal(SignalKey::static_str("first"))
        }));
        assert!(normalized.invalidations.iter().any(|invalidation| {
            invalidation.source == InvalidationSource::Signal(SignalKey::static_str("second"))
        }));
    }

    #[test]
    fn event_and_timer_sources_keep_typed_payloads_with_readable_labels() {
        let event = Invalidation::event(
            EventTargetId::Node(NodeId::new("page.button")),
            EventSource::ContextMenu,
            DirtyFlags::DRAW,
        );
        assert_eq!(
            event.source,
            InvalidationSource::Event(EventSource::ContextMenu)
        );
        assert_eq!(event.source.kind(), "event");
        assert_eq!(event.source.label(), "context_menu");

        let timer = Invalidation::timer(NodeId::new("page.timer"));
        assert_eq!(timer.source, InvalidationSource::Timer(TimerSource::Timer));
        assert_eq!(timer.source.kind(), "timer");
        assert_eq!(timer.source.label(), "timer");
    }

    #[test]
    fn invalidation_store_preserves_distinct_sources_for_one_target() {
        let mut store = InvalidationStore::default();

        store.push(Invalidation::runtime(
            NodeId::new("page"),
            "force_full_compose",
            DirtyFlags::COMPOSE | DirtyFlags::DRAW,
        ));
        store.push(Invalidation::runtime(
            NodeId::new("page"),
            "layout_structure",
            DirtyFlags::LAYOUT,
        ));
        store.push(Invalidation::runtime(
            NodeId::new("page"),
            "force_full_compose",
            DirtyFlags::VISUAL,
        ));

        let records = store.snapshot();
        assert_eq!(records.len(), 2);
        let forced = records
            .iter()
            .find(|record| record.source == InvalidationSource::Runtime("force_full_compose"))
            .expect("force source should stay distinct");
        let layout = records
            .iter()
            .find(|record| record.source == InvalidationSource::Runtime("layout_structure"))
            .expect("layout source should stay distinct");
        assert_eq!(
            forced.flags,
            DirtyFlags::COMPOSE | DirtyFlags::VISUAL | DirtyFlags::DRAW
        );
        assert_eq!(layout.flags, DirtyFlags::LAYOUT);
    }
}
