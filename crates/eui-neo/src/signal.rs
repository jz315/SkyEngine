//! Explicit state signals for scoped reactive UI authoring.

use std::cell::RefCell;
use std::fmt;
use std::rc::{Rc, Weak};
use std::sync::{Arc, OnceLock};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::retained::ScopeId;
use crate::runtime::DirtyInput;
use crate::Ui;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
    pub struct DirtyFlags: u8 {
        const COMPOSE = 0b0000_0001;
        const LAYOUT = 0b0000_0010;
        const VISUAL = 0b0000_0100;
        const DRAW = 0b0000_1000;
        const LAYER = 0b0001_0000;
        const FOCUS = 0b0010_0000;
        const HIT = 0b0100_0000;
        const PLATFORM = 0b1000_0000;
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SignalKey(Arc<str>);

impl SignalKey {
    pub fn new(value: impl Into<Arc<str>>) -> Self {
        Self(value.into())
    }

    pub fn static_str(value: &'static str) -> Self {
        Self(Arc::from(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SignalKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&'static str> for SignalKey {
    fn from(value: &'static str) -> Self {
        Self::static_str(value)
    }
}

impl From<String> for SignalKey {
    fn from(value: String) -> Self {
        Self::new(Arc::<str>::from(value))
    }
}

#[derive(Debug, Default)]
struct SignalGraph {
    signal_to_scopes: FxHashMap<SignalKey, FxHashSet<ScopeId>>,
    scope_to_signals: FxHashMap<ScopeId, FxHashSet<SignalKey>>,
    dirty_scopes: FxHashSet<ScopeId>,
    dirty_reasons: FxHashMap<ScopeId, FxHashSet<SignalKey>>,
    dirty_flags: FxHashMap<ScopeId, DirtyFlags>,
    dirty_source_flags: FxHashMap<SignalDirtyKey, DirtyFlags>,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
struct SignalDirtyKey {
    scope: ScopeId,
    source: SignalKey,
}

impl SignalDirtyKey {
    fn new(scope: ScopeId, source: SignalKey) -> Self {
        Self { scope, source }
    }
}

type SignalGraphCell = RefCell<SignalGraph>;

thread_local! {
    static SCOPE_SIGNAL_GRAPHS: RefCell<FxHashMap<ScopeId, Vec<Weak<SignalGraphCell>>>> =
        RefCell::new(FxHashMap::default());
    static PENDING_SCOPE_RESETS: RefCell<FxHashSet<ScopeId>> = RefCell::new(FxHashSet::default());
}

impl SignalGraph {
    fn record_watch(&mut self, key: SignalKey, scope: ScopeId) {
        if signal_trace_enabled() {
            eprintln!("[eui-neo signal] watch {key} -> {scope}");
        }
        self.signal_to_scopes
            .entry(key.clone())
            .or_default()
            .insert(scope.clone());
        self.scope_to_signals.entry(scope).or_default().insert(key);
    }

    fn mark_dirty(&mut self, key: &SignalKey) {
        if let Some(scopes) = self.signal_to_scopes.get(key) {
            for scope in scopes {
                if dirty_trace_enabled() {
                    eprintln!("[eui-neo dirty] {scope} <- {key}");
                }
                self.dirty_scopes.insert(scope.clone());
                self.dirty_reasons
                    .entry(scope.clone())
                    .or_default()
                    .insert(key.clone());
                self.dirty_flags
                    .entry(scope.clone())
                    .and_modify(|flags| *flags |= DirtyFlags::COMPOSE | DirtyFlags::DRAW)
                    .or_insert(DirtyFlags::COMPOSE | DirtyFlags::DRAW);
                self.dirty_source_flags
                    .entry(SignalDirtyKey::new(scope.clone(), key.clone()))
                    .and_modify(|flags| *flags |= DirtyFlags::COMPOSE | DirtyFlags::DRAW)
                    .or_insert(DirtyFlags::COMPOSE | DirtyFlags::DRAW);
            }
        }
    }

    fn clear_scope_dependencies(&mut self, scope: &ScopeId) {
        if let Some(keys) = self.scope_to_signals.remove(scope) {
            for key in keys {
                let should_remove = if let Some(scopes) = self.signal_to_scopes.get_mut(&key) {
                    scopes.remove(scope);
                    scopes.is_empty()
                } else {
                    false
                };
                if should_remove {
                    self.signal_to_scopes.remove(&key);
                }
            }
        } else {
            let mut empty_keys = Vec::new();
            for (key, scopes) in &mut self.signal_to_scopes {
                scopes.remove(scope);
                if scopes.is_empty() {
                    empty_keys.push(key.clone());
                }
            }
            for key in empty_keys {
                self.signal_to_scopes.remove(&key);
            }
        }
        self.dirty_scopes.remove(scope);
        self.dirty_reasons.remove(scope);
        self.dirty_flags.remove(scope);
        self.dirty_source_flags.retain(|key, _| &key.scope != scope);
    }
}

pub struct State<T> {
    inner: Rc<RefCell<T>>,
    graph: Rc<SignalGraphCell>,
}

impl<T> State<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: Rc::new(RefCell::new(value)),
            graph: Rc::new(RefCell::new(SignalGraph::default())),
        }
    }

    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.borrow())
    }

    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut self.inner.borrow_mut())
    }

    #[cfg(test)]
    pub(crate) fn dirty(&self) -> Vec<DirtyInput> {
        let graph = self.graph.borrow();
        dirty_inputs_from_graph(&graph)
    }

    pub fn signal_dependencies(&self) -> Vec<(SignalKey, Vec<String>)> {
        self.graph
            .borrow()
            .signal_to_scopes
            .iter()
            .map(|(key, scopes)| {
                let mut scopes: Vec<_> = scopes
                    .iter()
                    .map(|scope| scope.as_str().to_string())
                    .collect();
                scopes.sort();
                (key.clone(), scopes)
            })
            .collect()
    }

    pub fn dirty_reasons(&self) -> Vec<(String, SignalKey)> {
        let mut reasons = self
            .graph
            .borrow()
            .dirty_reasons
            .iter()
            .flat_map(|(scope, keys)| {
                keys.iter()
                    .map(|key| (scope.as_str().to_string(), key.clone()))
            })
            .collect::<Vec<_>>();
        reasons.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.as_str().cmp(right.1.as_str()))
        });
        reasons
    }

    pub fn dirty_flags(&self) -> Vec<(String, DirtyFlags)> {
        let mut flags = self
            .graph
            .borrow()
            .dirty_flags
            .iter()
            .map(|(scope, flags)| (scope.as_str().to_string(), *flags))
            .collect::<Vec<_>>();
        flags.sort_by(|left, right| left.0.cmp(&right.0));
        flags
    }

    pub(crate) fn take_dirty(&self) -> Vec<DirtyInput> {
        let mut graph = self.graph.borrow_mut();
        let dirty = dirty_inputs_from_graph(&graph);
        graph.dirty_scopes.clear();
        graph.dirty_reasons.clear();
        graph.dirty_flags.clear();
        graph.dirty_source_flags.clear();
        dirty
    }
}

pub(crate) fn clear_scope_signal_dependencies(scope: &ScopeId) {
    take_scheduled_scope_dependency_reset(scope);
    SCOPE_SIGNAL_GRAPHS.with(|registry| {
        let mut registry = registry.borrow_mut();
        let Some(graphs) = registry.get_mut(scope) else {
            return;
        };
        graphs.retain(|graph| {
            if let Some(graph) = graph.upgrade() {
                graph.borrow_mut().clear_scope_dependencies(scope);
                true
            } else {
                false
            }
        });
        if graphs.is_empty() {
            registry.remove(scope);
        }
    });
}

fn take_scheduled_scope_dependency_reset(scope: &ScopeId) -> bool {
    PENDING_SCOPE_RESETS.with(|pending| pending.borrow_mut().remove(scope))
}

pub(crate) fn schedule_scope_dependency_reset(scope: &ScopeId) {
    PENDING_SCOPE_RESETS.with(|pending| {
        pending.borrow_mut().insert(scope.clone());
    });
}

fn clear_scope_signal_dependencies_if_scheduled(scope: &ScopeId) {
    if take_scheduled_scope_dependency_reset(scope) {
        clear_scope_signal_dependencies(scope);
    }
}

fn register_scope_signal_graph(scope: &ScopeId, graph: &Rc<SignalGraphCell>) {
    SCOPE_SIGNAL_GRAPHS.with(|registry| {
        let mut registry = registry.borrow_mut();
        let graphs = registry.entry(scope.clone()).or_default();
        graphs.retain(|existing| existing.upgrade().is_some());
        let already_registered = graphs
            .iter()
            .filter_map(Weak::upgrade)
            .any(|existing| Rc::ptr_eq(&existing, graph));
        if !already_registered {
            graphs.push(Rc::downgrade(graph));
        }
    });
}

fn dirty_inputs_from_graph(graph: &SignalGraph) -> Vec<DirtyInput> {
    let mut dirty = graph
        .dirty_source_flags
        .iter()
        .map(|(key, flags)| DirtyInput::signal_scope(key.scope.clone(), key.source.clone(), *flags))
        .collect::<Vec<_>>();
    dirty.sort_by(|left, right| {
        left.scope_id()
            .cmp(right.scope_id())
            .then_with(|| left.source().cmp(&right.source()))
    });
    dirty
}

impl<T: 'static> State<T> {
    pub fn signal<V: Clone + PartialEq + 'static>(
        &self,
        key: impl Into<SignalKey>,
        get: impl Fn(&T) -> V + 'static,
        set: impl Fn(&mut T, V) + 'static,
    ) -> Signal<T, V> {
        Signal {
            key: key.into(),
            state: self.clone(),
            get: Rc::new(get),
            set: Rc::new(set),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for State<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("State").field(&self.inner.borrow()).finish()
    }
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            graph: self.graph.clone(),
        }
    }
}

pub struct Signal<T, V> {
    key: SignalKey,
    state: State<T>,
    get: Rc<dyn Fn(&T) -> V>,
    set: Rc<dyn Fn(&mut T, V)>,
}

impl<T, V: Clone> Signal<T, V> {
    pub fn key(&self) -> SignalKey {
        self.key.clone()
    }

    pub fn watch(&self, ui: &mut Ui) -> V {
        if let Some(scope) = ui.dependency_owner_id() {
            clear_scope_signal_dependencies_if_scheduled(&scope);
            register_scope_signal_graph(&scope, &self.state.graph);
            self.state
                .graph
                .borrow_mut()
                .record_watch(self.key.clone(), scope);
        }
        self.peek()
    }

    pub fn peek(&self) -> V {
        self.state.read(|state| (self.get)(state))
    }
}

impl<T, V: Clone + PartialEq> Signal<T, V> {
    pub fn set(&self, value: V) {
        let old = self.peek();
        self.state.update(|state| (self.set)(state, value));
        if self.peek() != old {
            self.state.graph.borrow_mut().mark_dirty(&self.key);
        }
    }

    pub fn update(&self, f: impl FnOnce(V) -> V) {
        self.set(f(self.peek()));
    }
}

impl<T, V> Clone for Signal<T, V> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            state: self.state.clone(),
            get: self.get.clone(),
            set: self.set.clone(),
        }
    }
}

fn signal_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SKY_NEO_SIGNAL_TRACE").is_some())
}

fn dirty_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SKY_NEO_DIRTY_TRACE").is_some())
}

#[cfg(test)]
mod tests {
    use super::{
        clear_scope_signal_dependencies, schedule_scope_dependency_reset, SignalKey, State,
        PENDING_SCOPE_RESETS,
    };
    use crate::retained::ScopeId;
    use crate::Ui;

    #[derive(Default)]
    struct AppState {
        enabled: bool,
        page: i32,
        name: String,
    }

    #[test]
    fn signal_reads_and_writes_value() {
        let state = State::new(AppState::default());
        let enabled = state.signal(
            "enabled",
            |state| state.enabled,
            |state, value| state.enabled = value,
        );

        assert!(!enabled.peek());
        enabled.set(true);
        assert!(state.read(|state| state.enabled));
    }

    #[test]
    fn unchanged_signal_set_does_not_dirty_scope() {
        let state = State::new(AppState::default());
        let page = state.signal(
            "page",
            |state| state.page,
            |state, value| state.page = value,
        );
        let mut ui = Ui::new("test");

        ui.retained_scope("nav", |ui| {
            assert_eq!(page.watch(ui), 0);
        });

        page.set(0);
        assert!(state.dirty().is_empty());
    }

    #[test]
    fn clearing_scope_dependencies_discards_pending_dependency_reset() {
        let scope = ScopeId::new("test.nav");

        schedule_scope_dependency_reset(&scope);
        assert!(PENDING_SCOPE_RESETS.with(|pending| pending.borrow().contains(&scope)));

        clear_scope_signal_dependencies(&scope);
        assert!(!PENDING_SCOPE_RESETS.with(|pending| pending.borrow().contains(&scope)));
    }

    #[test]
    fn clearing_scope_dependencies_prunes_stale_dirty_records_without_reverse_edges() {
        let state = State::new(AppState::default());
        let page = state.signal(
            "page",
            |state| state.page,
            |state, value| state.page = value,
        );
        let mut ui = Ui::new("test");
        let scope = ScopeId::new("test.nav");

        ui.retained_scope("nav", |ui| {
            assert_eq!(page.watch(ui), 0);
        });
        page.set(1);
        assert!(!state.dirty().is_empty());

        state.graph.borrow_mut().scope_to_signals.remove(&scope);
        clear_scope_signal_dependencies(&scope);

        assert!(state.signal_dependencies().is_empty());
        assert!(state.dirty().is_empty());
        page.set(2);
        assert!(state.dirty().is_empty());
    }

    #[test]
    fn signal_watch_registers_scope_and_set_marks_it_dirty() {
        let state = State::new(AppState::default());
        let page = state.signal(
            "page",
            |state| state.page,
            |state, value| state.page = value,
        );
        let mut ui = Ui::new("test");

        ui.retained_scope("nav", |ui| {
            assert_eq!(page.watch(ui), 0);
        });

        let dependencies = state.signal_dependencies();
        assert_eq!(dependencies.len(), 1);
        assert_eq!(dependencies[0].0.as_str(), "page");
        assert_eq!(dependencies[0].1, vec!["test.nav".to_string()]);
        assert!(state
            .graph
            .borrow()
            .scope_to_signals
            .contains_key(&ScopeId::new("test.nav")));

        page.set(2);
        assert_eq!(state.dirty()[0].id(), "test.nav");
        assert_eq!(state.dirty()[0].scope_id().as_str(), "test.nav");
        assert_eq!(state.dirty()[0].source(), Some("page"));
        assert_eq!(
            state.dirty()[0].source_key().map(SignalKey::as_str),
            Some("page")
        );
        assert!(state
            .graph
            .borrow()
            .dirty_flags
            .contains_key(&ScopeId::new("test.nav")));
        assert_eq!(state.dirty_reasons()[0].0, "test.nav");
        assert_eq!(state.dirty_reasons()[0].1.as_str(), "page");
        assert_eq!(
            state.dirty_flags()[0],
            (
                "test.nav".to_string(),
                super::DirtyFlags::COMPOSE | super::DirtyFlags::DRAW
            )
        );
    }

    #[test]
    fn multiple_signal_sources_for_one_scope_emit_distinct_dirty_inputs() {
        let state = State::new(AppState::default());
        let page = state.signal(
            "page",
            |state| state.page,
            |state, value| state.page = value,
        );
        let enabled = state.signal(
            "enabled",
            |state| state.enabled,
            |state, value| state.enabled = value,
        );
        let mut ui = Ui::new("test");

        ui.retained_scope("nav", |ui| {
            assert_eq!(page.watch(ui), 0);
            assert!(!enabled.watch(ui));
        });

        page.set(1);
        enabled.set(true);

        let dirty = state.dirty();
        assert_eq!(dirty.len(), 2);
        assert!(dirty
            .iter()
            .all(|record| record.scope_id().as_str() == "test.nav"));
        assert_eq!(
            dirty
                .iter()
                .filter_map(|record| record.source().map(str::to_string))
                .collect::<Vec<_>>(),
            vec!["enabled".to_string(), "page".to_string()]
        );
        assert_eq!(
            state.dirty_reasons(),
            vec![
                ("test.nav".to_string(), SignalKey::static_str("enabled")),
                ("test.nav".to_string(), SignalKey::static_str("page")),
            ]
        );
        assert_eq!(
            state.dirty_flags(),
            vec![(
                "test.nav".to_string(),
                super::DirtyFlags::COMPOSE | super::DirtyFlags::DRAW
            )]
        );

        let taken = state.take_dirty();
        assert_eq!(taken.len(), 2);
        assert!(state.dirty().is_empty());
        assert!(state.dirty_reasons().is_empty());
        assert!(state.dirty_flags().is_empty());
    }

    #[test]
    fn signal_watch_without_scope_registers_current_element_owner() {
        let state = State::new(AppState::default());
        let page = state.signal(
            "page",
            |state| state.page,
            |state, value| state.page = value,
        );
        let mut ui = Ui::new("test");

        ui.column("panel").content(|ui| {
            assert_eq!(page.watch(ui), 0);
            ui.text("panel.label").text("Panel").build();
        });

        let dependencies = state.signal_dependencies();
        assert_eq!(dependencies.len(), 1);
        assert_eq!(dependencies[0].0.as_str(), "page");
        assert_eq!(dependencies[0].1, vec!["test.panel".to_string()]);

        page.set(1);
        assert_eq!(state.dirty()[0].id(), "test.panel");
    }

    #[test]
    fn peek_does_not_register_scope_dependency() {
        let state = State::new(AppState {
            name: "Sky".to_string(),
            ..AppState::default()
        });
        let name = state.signal(
            "name",
            |state| state.name.clone(),
            |state, value| state.name = value,
        );
        let mut ui = Ui::new("test");

        ui.retained_scope("name", |_ui| {
            assert_eq!(name.peek(), "Sky");
        });

        assert!(state.signal_dependencies().is_empty());
    }
}
