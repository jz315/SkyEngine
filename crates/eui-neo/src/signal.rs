//! Explicit state signals for scoped reactive UI authoring.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::Ui;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
    pub struct DirtyFlags: u8 {
        const COMPOSE = 0b0000_0001;
        const LAYOUT = 0b0000_0010;
        const VISUAL = 0b0000_0100;
        const DRAW = 0b0000_1000;
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
    signal_to_scopes: FxHashMap<SignalKey, FxHashSet<String>>,
    scope_to_signals: FxHashMap<String, FxHashSet<SignalKey>>,
    dirty_scopes: FxHashSet<String>,
    dirty_reasons: FxHashMap<String, SignalKey>,
    dirty_flags: FxHashMap<String, DirtyFlags>,
}

impl SignalGraph {
    fn record_watch(&mut self, key: SignalKey, scope: String) {
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
                self.dirty_reasons.insert(scope.clone(), key.clone());
                self.dirty_flags
                    .entry(scope.clone())
                    .and_modify(|flags| *flags |= DirtyFlags::COMPOSE | DirtyFlags::DRAW)
                    .or_insert(DirtyFlags::COMPOSE | DirtyFlags::DRAW);
            }
        }
    }
}

pub struct State<T> {
    inner: Rc<RefCell<T>>,
    graph: Rc<RefCell<SignalGraph>>,
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

    pub fn dirty_scopes(&self) -> Vec<String> {
        self.graph.borrow().dirty_scopes.iter().cloned().collect()
    }

    pub fn clear_dirty_scopes(&self) {
        let mut graph = self.graph.borrow_mut();
        graph.dirty_scopes.clear();
        graph.dirty_reasons.clear();
        graph.dirty_flags.clear();
    }

    pub fn take_dirty_scopes(&self) -> Vec<String> {
        let mut graph = self.graph.borrow_mut();
        let scopes = graph.dirty_scopes.iter().cloned().collect();
        graph.dirty_scopes.clear();
        graph.dirty_reasons.clear();
        graph.dirty_flags.clear();
        scopes
    }

    pub fn signal_dependencies(&self) -> Vec<(SignalKey, Vec<String>)> {
        self.graph
            .borrow()
            .signal_to_scopes
            .iter()
            .map(|(key, scopes)| (key.clone(), scopes.iter().cloned().collect()))
            .collect()
    }

    pub fn dirty_scope_reasons(&self) -> Vec<(String, SignalKey)> {
        self.graph
            .borrow()
            .dirty_reasons
            .iter()
            .map(|(scope, key)| (scope.clone(), key.clone()))
            .collect()
    }

    pub fn dirty_scope_flags(&self) -> Vec<(String, DirtyFlags)> {
        self.graph
            .borrow()
            .dirty_flags
            .iter()
            .map(|(scope, flags)| (scope.clone(), *flags))
            .collect()
    }
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
    use super::State;
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

        ui.scope("nav", |ui| {
            assert_eq!(page.watch(ui), 0);
        });

        page.set(0);
        assert!(state.dirty_scopes().is_empty());
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

        ui.scope("nav", |ui| {
            assert_eq!(page.watch(ui), 0);
        });

        let dependencies = state.signal_dependencies();
        assert_eq!(dependencies.len(), 1);
        assert_eq!(dependencies[0].0.as_str(), "page");
        assert_eq!(dependencies[0].1, vec!["test.nav".to_string()]);

        page.set(2);
        assert_eq!(state.dirty_scopes(), vec!["test.nav".to_string()]);
        assert_eq!(state.dirty_scope_reasons()[0].0, "test.nav");
        assert_eq!(state.dirty_scope_reasons()[0].1.as_str(), "page");
        assert_eq!(
            state.dirty_scope_flags()[0],
            (
                "test.nav".to_string(),
                super::DirtyFlags::COMPOSE | super::DirtyFlags::DRAW
            )
        );
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
        assert_eq!(state.dirty_scopes(), vec!["test.panel".to_string()]);
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

        ui.scope("name", |_ui| {
            assert_eq!(name.peek(), "Sky");
        });

        assert!(state.signal_dependencies().is_empty());
    }
}
