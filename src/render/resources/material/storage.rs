use super::{MaterialError, MaterialHandle, MaterialModel, MaterialRegistry, TypedMaterialHandle};

pub trait StorageHandle<M: MaterialModel> {
    fn typed(self) -> Option<TypedMaterialHandle<M>>;
}

impl<M: MaterialModel> StorageHandle<M> for TypedMaterialHandle<M> {
    #[inline]
    fn typed(self) -> Option<TypedMaterialHandle<M>> {
        Some(self)
    }
}

impl<M: MaterialModel> StorageHandle<M> for MaterialHandle {
    #[inline]
    fn typed(self) -> Option<TypedMaterialHandle<M>> {
        self.typed::<M>()
    }
}

/// Thin typed view over [`MaterialRegistry`] for migration of existing call
/// sites. New code should use `MaterialRegistry::{insert,set,get}` directly.
pub struct MaterialStorage<'a, M: MaterialModel> {
    registry: &'a MaterialRegistry,
    marker: std::marker::PhantomData<fn() -> M>,
}

impl<'a, M: MaterialModel> MaterialStorage<'a, M> {
    pub(crate) fn new(registry: &'a MaterialRegistry) -> Self {
        Self {
            registry,
            marker: std::marker::PhantomData,
        }
    }

    pub fn get<H: StorageHandle<M>>(&self, handle: H) -> Option<&M::Data> {
        self.registry.get(handle.typed()?).ok()
    }
}

pub struct MaterialStorageMut<'a, M: MaterialModel> {
    registry: &'a mut MaterialRegistry,
    marker: std::marker::PhantomData<fn() -> M>,
}

impl<'a, M: MaterialModel> MaterialStorageMut<'a, M> {
    pub(crate) fn new(registry: &'a mut MaterialRegistry) -> Self {
        Self {
            registry,
            marker: std::marker::PhantomData,
        }
    }

    pub fn insert(&mut self, data: M::Data) -> TypedMaterialHandle<M> {
        self.registry
            .insert::<M>(data)
            .expect("material model should be registered before storage insertion")
    }

    pub fn get<H: StorageHandle<M>>(&self, handle: H) -> Option<&M::Data> {
        self.registry.get(handle.typed()?).ok()
    }

    pub fn set<F>(&mut self, handle: TypedMaterialHandle<M>, update: F) -> Result<(), MaterialError>
    where
        F: FnOnce(&mut M::Data),
    {
        self.registry.set(handle, update)
    }

    pub fn remove(&mut self, handle: MaterialHandle) -> Option<M::Data> {
        self.registry.remove(handle.typed::<M>()?).ok()
    }

    pub fn clear(&mut self) {
        self.registry.clear_model_instances::<M>();
    }
}
