use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};

#[derive(Default)]
pub(crate) struct Resources {
    values: FxHashMap<TypeId, Box<dyn Any>>,
}

impl Resources {
    pub(crate) fn insert<R: 'static>(&mut self, resource: R) -> Option<R> {
        self.values
            .insert(TypeId::of::<R>(), Box::new(resource))
            .and_then(|old| old.downcast::<R>().ok().map(|value| *value))
    }

    pub(crate) fn get<R: 'static>(&self) -> Option<&R> {
        self.values
            .get(&TypeId::of::<R>())
            .and_then(|value| value.downcast_ref::<R>())
    }

    pub(crate) fn get_mut<R: 'static>(&mut self) -> Option<&mut R> {
        self.values
            .get_mut(&TypeId::of::<R>())
            .and_then(|value| value.downcast_mut::<R>())
    }

    pub(crate) fn contains<R: 'static>(&self) -> bool {
        self.values.contains_key(&TypeId::of::<R>())
    }

    pub(crate) fn contains_id(&self, id: TypeId) -> bool {
        self.values.contains_key(&id)
    }

    pub(crate) fn remove<R: 'static>(&mut self) -> Option<R> {
        self.values
            .remove(&TypeId::of::<R>())
            .and_then(|value| value.downcast::<R>().ok().map(|value| *value))
    }
}
