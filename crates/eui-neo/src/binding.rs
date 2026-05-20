//! Small state binding helpers for neo widget callbacks.
//!
//! This keeps EUI-NEO-style callback authoring ergonomic without requiring
//! gallery-specific snapshot/action plumbing.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

#[macro_export]
macro_rules! neo_bind {
    ($state:expr, $field:ident) => {
        $state.bind(|state| state.$field, |state, value| state.$field = value)
    };
}

#[macro_export]
macro_rules! neo_bind_clone {
    ($state:expr, $field:ident) => {
        $state.bind_clone(
            |state| state.$field.clone(),
            |state, value| state.$field = value,
        )
    };
}

#[macro_export]
macro_rules! neo_bind_clamped {
    ($state:expr, $field:ident, $min:expr, $max:expr) => {
        $state.bind(
            |state| state.$field,
            |state, value| state.$field = value.clamp($min, $max),
        )
    };
}

#[macro_export]
macro_rules! neo_bind_max {
    ($state:expr, $field:ident, $min:expr) => {
        $state.bind(
            |state| state.$field,
            |state, value| state.$field = value.max($min),
        )
    };
}

#[macro_export]
macro_rules! neo_bind_eq {
    ($state:expr, $field:ident, $value:expr) => {
        $state.bind(
            move |state| state.$field == $value,
            move |state, selected| {
                if selected {
                    state.$field = $value;
                }
            },
        )
    };
}

#[macro_export]
macro_rules! neo_bind_array {
    ($state:expr, [$($field:ident),+ $(,)?]) => {
        $state.bind(
            |state| [$(state.$field),+],
            |state, value| {
                let [$($field),+] = value;
                $(state.$field = $field;)+
            },
        )
    };
}

pub struct NeoState<T> {
    inner: Rc<RefCell<T>>,
}

impl<T> NeoState<T> {
    pub fn new(value: T) -> Self {
        Self {
            inner: Rc::new(RefCell::new(value)),
        }
    }

    pub fn read<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.borrow())
    }

    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        f(&mut self.inner.borrow_mut())
    }
}

impl<T: 'static> NeoState<T> {
    pub fn bind<V: 'static>(
        &self,
        get: impl Fn(&T) -> V + 'static,
        set: impl Fn(&mut T, V) + 'static,
    ) -> Binding<T, V> {
        Binding {
            state: self.clone(),
            get: Rc::new(get),
            set: Rc::new(set),
        }
    }

    pub fn bind_clone<V: 'static>(
        &self,
        get: impl Fn(&T) -> V + 'static,
        set: impl Fn(&mut T, V) + 'static,
    ) -> Binding<T, V> {
        self.bind(get, set)
    }
}

impl<T> Clone for NeoState<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for NeoState<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NeoState")
            .field(&self.inner.borrow())
            .finish()
    }
}

pub struct Binding<T, V> {
    state: NeoState<T>,
    get: Rc<dyn Fn(&T) -> V>,
    set: Rc<dyn Fn(&mut T, V)>,
}

impl<T, V> Binding<T, V> {
    pub fn get(&self) -> V {
        self.state.read(|state| (self.get)(state))
    }

    pub fn set(&self, value: V) {
        self.state.update(|state| (self.set)(state, value));
    }
}

impl<T, V: 'static> Binding<T, V> {
    pub fn update(&self, f: impl FnOnce(V) -> V) {
        self.set(f(self.get()));
    }
}

impl<T, V> Clone for Binding<T, V> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            get: self.get.clone(),
            set: self.set.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NeoState;

    #[derive(Default)]
    struct State {
        enabled: bool,
        name: String,
        amount: f32,
        selected: i32,
        page: i32,
    }

    #[test]
    fn binding_reads_and_writes_copy_value() {
        let state = NeoState::new(State::default());
        let enabled = state.bind(|state| state.enabled, |state, value| state.enabled = value);

        assert!(!enabled.get());
        enabled.set(true);
        assert!(state.read(|state| state.enabled));
    }

    #[test]
    fn binding_reads_and_writes_cloned_value() {
        let state = NeoState::new(State {
            name: "Sky".to_string(),
            ..State::default()
        });
        let name = state.bind_clone(
            |state| state.name.clone(),
            |state, value| state.name = value,
        );

        assert_eq!(name.get(), "Sky");
        name.set("Neo".to_string());
        assert_eq!(state.read(|state| state.name.clone()), "Neo");
    }

    #[test]
    fn field_binding_macros_create_bindings() {
        let state = NeoState::new(State {
            name: "Sky".to_string(),
            amount: 0.25,
            selected: 1,
            ..State::default()
        });

        let enabled = crate::neo_bind!(state, enabled);
        enabled.set(true);
        assert!(state.read(|state| state.enabled));

        let name = crate::neo_bind_clone!(state, name);
        name.set("Neo".to_string());
        assert_eq!(state.read(|state| state.name.clone()), "Neo");

        let amount = crate::neo_bind_clamped!(state, amount, 0.0, 1.0);
        amount.set(2.0);
        assert_eq!(state.read(|state| state.amount), 1.0);

        let selected_two = crate::neo_bind_eq!(state, selected, 2);
        assert!(!selected_two.get());
        selected_two.set(true);
        assert_eq!(state.read(|state| state.selected), 2);

        let pair = crate::neo_bind_array!(state, [selected, page]);
        pair.set([3, 5]);
        assert_eq!(state.read(|state| state.selected), 3);
        assert_eq!(state.read(|state| state.page), 5);
    }
}
