use std::any::TypeId;
use std::marker::PhantomData;

use super::MaterialModel;

/// Generational id for a registered material model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct MaterialModelId {
    index: u32,
    generation: u32,
}

impl MaterialModelId {
    #[inline]
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    #[inline]
    pub const fn index(self) -> u32 {
        self.index
    }

    #[inline]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

/// Generational id for one material instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct MaterialInstanceId {
    index: u32,
    generation: u32,
}

impl MaterialInstanceId {
    #[inline]
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    #[inline]
    pub const fn index(self) -> u32 {
        self.index
    }

    #[inline]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

/// Typed handle returned by user-facing insertion APIs.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct TypedMaterialHandle<M: MaterialModel> {
    id: MaterialInstanceId,
    model: MaterialModelId,
    marker: PhantomData<fn() -> M>,
}

impl<M: MaterialModel> Clone for TypedMaterialHandle<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: MaterialModel> Copy for TypedMaterialHandle<M> {}

impl<M: MaterialModel> TypedMaterialHandle<M> {
    #[inline]
    pub const fn new(model: MaterialModelId, id: MaterialInstanceId) -> Self {
        Self {
            id,
            model,
            marker: PhantomData,
        }
    }

    /// Compatibility constructor for existing tests that only need a fake
    /// handle. Real handles should come from [`super::MaterialRegistry`].
    #[inline]
    pub fn from_raw(index: u32, generation: u32) -> Self {
        Self::new(
            MaterialModelId::new(u32::MAX, 0),
            MaterialInstanceId::new(index, generation),
        )
    }

    #[inline]
    pub const fn id(self) -> MaterialInstanceId {
        self.id
    }

    #[inline]
    pub const fn model_id(self) -> MaterialModelId {
        self.model
    }

    #[inline]
    pub const fn index(self) -> u32 {
        self.id.index()
    }

    #[inline]
    pub const fn generation(self) -> u32 {
        self.id.generation()
    }

    #[inline]
    pub fn erased(self) -> ErasedMaterialHandle {
        ErasedMaterialHandle {
            model: self.model,
            instance: self.id,
            model_type: TypeId::of::<M>(),
        }
    }
}

impl<M: MaterialModel> From<TypedMaterialHandle<M>> for ErasedMaterialHandle {
    #[inline]
    fn from(handle: TypedMaterialHandle<M>) -> Self {
        handle.erased()
    }
}

/// Type-erased handle used by ECS-facing renderer components and draw payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ErasedMaterialHandle {
    model: MaterialModelId,
    instance: MaterialInstanceId,
    model_type: TypeId,
}

impl ErasedMaterialHandle {
    #[inline]
    pub fn from_ids<M: MaterialModel>(model: MaterialModelId, id: MaterialInstanceId) -> Self {
        TypedMaterialHandle::<M>::new(model, id).erased()
    }

    #[inline]
    pub fn new<M: MaterialModel>(index: u32, generation: u32) -> Self {
        Self::from_raw::<M>(index, generation)
    }

    /// Compatibility constructor for existing tests that only need a fake
    /// erased handle. Real handles should come from [`super::MaterialRegistry`].
    #[inline]
    pub fn from_raw<M: MaterialModel>(index: u32, generation: u32) -> Self {
        TypedMaterialHandle::<M>::from_raw(index, generation).erased()
    }

    #[inline]
    pub const fn model_id(self) -> MaterialModelId {
        self.model
    }

    #[inline]
    pub const fn id(self) -> MaterialInstanceId {
        self.instance
    }

    #[inline]
    pub const fn index(self) -> u32 {
        self.instance.index()
    }

    #[inline]
    pub const fn generation(self) -> u32 {
        self.instance.generation()
    }

    #[inline]
    pub fn model_type(self) -> TypeId {
        self.model_type
    }

    #[inline]
    pub fn is<M: MaterialModel>(self) -> bool {
        self.model_type == TypeId::of::<M>()
    }

    #[inline]
    pub fn typed<M: MaterialModel>(self) -> Option<TypedMaterialHandle<M>> {
        (self.is::<M>() || self.model.index() == u32::MAX)
            .then(|| TypedMaterialHandle::new(self.model, self.instance))
    }
}

/// ECS-facing erased material handle.
pub type MaterialHandle = ErasedMaterialHandle;
