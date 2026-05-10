use std::any::TypeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneBindingKind {
    GpuTable(TypeId),
    ShadowView,
    GlobalIllumination,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SceneBindingDesc {
    pub slot: u32,
    pub kind: SceneBindingKind,
}

impl SceneBindingDesc {
    pub fn gpu_table<T>(slot: u32) -> Self
    where
        T: crate::render::GpuTable + 'static,
    {
        Self {
            slot,
            kind: SceneBindingKind::GpuTable(TypeId::of::<T>()),
        }
    }

    #[inline]
    pub const fn shadow_view(slot: u32) -> Self {
        Self {
            slot,
            kind: SceneBindingKind::ShadowView,
        }
    }

    #[inline]
    pub const fn global_illumination(slot: u32) -> Self {
        Self {
            slot,
            kind: SceneBindingKind::GlobalIllumination,
        }
    }
}
