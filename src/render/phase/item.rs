use std::any::TypeId;

use crate::ecs::EntityId;
use crate::render::resources::{
    material::{Material, MaterialHandle},
    mesh::MeshHandle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PhasePayloadKind {
    type_id: TypeId,
    type_name: &'static str,
}

impl PhasePayloadKind {
    #[inline]
    pub fn of<T: PhasePayload>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            type_name: std::any::type_name::<T>(),
        }
    }

    #[inline]
    pub fn type_name(self) -> &'static str {
        self.type_name
    }
}

pub trait PhasePayload: Copy + 'static {}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct MeshDrawData {
    mesh_slot: u32,
    mesh_kind: u32,
    material_index: u32,
    material_generation: u32,
    sub_mesh_index: u32,
    model_slot: u32,
}

impl MeshDrawData {
    const BUILTIN_KIND: u32 = 0;
    const DYNAMIC_KIND: u32 = 1;

    #[inline]
    pub fn new(
        mesh_handle: MeshHandle,
        material_handle: impl Into<MaterialHandle>,
        sub_mesh_index: u32,
    ) -> Self {
        let material_handle = material_handle.into();
        Self {
            mesh_slot: mesh_handle.slot(),
            mesh_kind: if mesh_handle.is_builtin() {
                Self::BUILTIN_KIND
            } else {
                Self::DYNAMIC_KIND
            },
            material_index: material_handle.index(),
            material_generation: material_handle.generation(),
            sub_mesh_index,
            model_slot: 0,
        }
    }

    #[inline]
    pub fn mesh_handle(self) -> MeshHandle {
        if self.mesh_kind == Self::BUILTIN_KIND {
            MeshHandle::builtin(self.mesh_slot)
        } else {
            MeshHandle::dynamic(self.mesh_slot)
        }
    }

    #[inline]
    pub fn material_handle<M: Material>(self) -> MaterialHandle {
        MaterialHandle::new::<M>(self.material_index, self.material_generation)
    }

    #[inline]
    pub fn sub_mesh_index(self) -> u32 {
        self.sub_mesh_index
    }

    #[inline]
    pub fn model_slot(self) -> u32 {
        self.model_slot
    }

    #[inline]
    pub fn set_model_slot(&mut self, model_slot: u32) {
        self.model_slot = model_slot;
    }
}

impl PhasePayload for MeshDrawData {}

#[cfg(feature = "live2d")]
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Live2DDrawData {
    frame_index: u32,
}

#[cfg(feature = "live2d")]
impl Live2DDrawData {
    #[inline]
    pub const fn new(frame_index: u32) -> Self {
        Self { frame_index }
    }

    #[inline]
    pub const fn frame_index(self) -> usize {
        self.frame_index as usize
    }
}

#[cfg(feature = "live2d")]
impl PhasePayload for Live2DDrawData {}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SpriteDrawData {
    material_index: u32,
    material_generation: u32,
    color: [f32; 4],
    size: [f32; 2],
    uv_rect: [f32; 4],
}

impl SpriteDrawData {
    #[inline]
    pub fn new(
        material_handle: MaterialHandle,
        size: [f32; 2],
        color: [f32; 4],
        uv_rect: [f32; 4],
    ) -> Self {
        Self {
            material_index: material_handle.index(),
            material_generation: material_handle.generation(),
            color,
            size,
            uv_rect,
        }
    }

    #[inline]
    pub fn material_handle(self) -> MaterialHandle {
        MaterialHandle::new::<crate::render::SpriteMaterial>(
            self.material_index,
            self.material_generation,
        )
    }

    #[inline]
    pub fn color(self) -> [f32; 4] {
        self.color
    }

    #[inline]
    pub fn size(self) -> [f32; 2] {
        self.size
    }

    #[inline]
    pub fn uv_rect(self) -> [f32; 4] {
        self.uv_rect
    }
}

impl PhasePayload for SpriteDrawData {}

#[derive(Clone)]
pub struct PhaseItem {
    pub sort_key: u64,
    pub draw_function_id: crate::render::phase::DrawFunctionId,
    pub entity: EntityId,
    pub batch_key: u64,
    payload_kind: PhasePayloadKind,
    data: [u64; 6],
}

impl PhaseItem {
    #[inline]
    pub fn new<T: PhasePayload>(
        sort_key: u64,
        draw_function_id: crate::render::phase::DrawFunctionId,
        entity: EntityId,
        batch_key: u64,
        payload: T,
    ) -> Self {
        let mut item = Self {
            sort_key,
            draw_function_id,
            entity,
            batch_key,
            payload_kind: PhasePayloadKind::of::<T>(),
            data: [0; 6],
        };
        item.set_data(payload);
        item
    }

    #[inline]
    pub fn set_data<T: PhasePayload>(&mut self, value: T) {
        const { assert!(std::mem::size_of::<T>() <= 48) };
        const { assert!(std::mem::align_of::<T>() <= std::mem::align_of::<u64>()) };
        self.payload_kind = PhasePayloadKind::of::<T>();
        unsafe {
            std::ptr::write(self.data.as_mut_ptr().cast::<T>(), value);
        }
    }

    #[inline]
    pub fn data<T: PhasePayload>(&self) -> &T {
        const { assert!(std::mem::size_of::<T>() <= 48) };
        const { assert!(std::mem::align_of::<T>() <= std::mem::align_of::<u64>()) };
        assert!(
            self.has_payload::<T>(),
            "phase payload mismatch: item stores `{}`, requested `{}`",
            self.payload_kind.type_name(),
            std::any::type_name::<T>()
        );
        unsafe { &*self.data.as_ptr().cast::<T>() }
    }

    #[inline]
    pub fn data_mut<T: PhasePayload>(&mut self) -> &mut T {
        const { assert!(std::mem::size_of::<T>() <= 48) };
        const { assert!(std::mem::align_of::<T>() <= std::mem::align_of::<u64>()) };
        assert!(
            self.has_payload::<T>(),
            "phase payload mismatch: item stores `{}`, requested `{}`",
            self.payload_kind.type_name(),
            std::any::type_name::<T>()
        );
        unsafe { &mut *self.data.as_mut_ptr().cast::<T>() }
    }

    #[inline]
    pub fn payload_kind(&self) -> PhasePayloadKind {
        self.payload_kind
    }

    #[inline]
    pub fn has_payload<T: PhasePayload>(&self) -> bool {
        self.payload_kind == PhasePayloadKind::of::<T>()
    }
}
