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
    fn union(&mut self, other: Self) {
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InvalidationTarget {
    Scope(String),
    Node(String),
    Element(String),
    Layer(String),
}

impl InvalidationTarget {
    pub fn id(&self) -> &str {
        match self {
            Self::Scope(id) | Self::Node(id) | Self::Element(id) | Self::Layer(id) => id,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Scope(_) => "scope",
            Self::Node(_) => "node",
            Self::Element(_) => "element",
            Self::Layer(_) => "layer",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum InvalidationSource {
    Signal(String),
    Event(String),
    Timer(String),
    Resource(String),
    Runtime(&'static str),
}

impl InvalidationSource {
    pub fn label(&self) -> &str {
        match self {
            Self::Signal(value)
            | Self::Event(value)
            | Self::Timer(value)
            | Self::Resource(value) => value,
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

    fn category(&self) -> InvalidationSourceCategory {
        match self {
            Self::Signal(_) => InvalidationSourceCategory::Signal,
            Self::Event(_) => InvalidationSourceCategory::Event,
            Self::Timer(_) => InvalidationSourceCategory::Timer,
            Self::Resource(_) => InvalidationSourceCategory::Resource,
            Self::Runtime(_) => InvalidationSourceCategory::Runtime,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
enum InvalidationSourceCategory {
    Signal,
    Event,
    Timer,
    Resource,
    Runtime,
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
    pub(super) fn event(
        target: impl Into<String>,
        source: impl Into<String>,
        flags: DirtyFlags,
    ) -> Self {
        Self {
            target: InvalidationTarget::Node(target.into()),
            source: InvalidationSource::Event(source.into()),
            flags,
            pass_flags: pass_flags_for_dirty_flags(flags),
            propagation: InvalidationPropagation::SelfOnly,
        }
    }

    pub(super) fn signal(
        target: impl Into<String>,
        source: impl Into<String>,
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

    pub(super) fn timer(target: impl Into<String>) -> Self {
        Self {
            target: InvalidationTarget::Node(target.into()),
            source: InvalidationSource::Timer("timer".to_string()),
            flags: DirtyFlags::COMPOSE | DirtyFlags::DRAW,
            pass_flags: pass_flags_for_dirty_flags(DirtyFlags::COMPOSE | DirtyFlags::DRAW),
            propagation: InvalidationPropagation::SelfOnly,
        }
    }

    pub(super) fn resource(target: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            target: InvalidationTarget::Element(target.into()),
            source: InvalidationSource::Resource(source.into()),
            flags: DirtyFlags::COMPOSE | DirtyFlags::DRAW,
            pass_flags: pass_flags_for_dirty_flags(DirtyFlags::COMPOSE | DirtyFlags::DRAW),
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
            existing.target == invalidation.target
                && existing.source.category() == invalidation.source.category()
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
        request_layer: false,
        request_focus: false,
        request_hit: flags.contains(DirtyFlags::LAYOUT),
        request_draw: flags.intersects(DirtyFlags::LAYOUT | DirtyFlags::VISUAL | DirtyFlags::DRAW),
        request_platform_effects: false,
    }
}
