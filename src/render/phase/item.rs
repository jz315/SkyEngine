use crate::ecs::EntityId;
use crate::render::resources::{
    material::{Material, MaterialHandle},
    mesh::MeshHandle,
};

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
        material_handle: MaterialHandle,
        sub_mesh_index: u32,
    ) -> Self {
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

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SpriteDrawData {
    material_index: u32,
    material_generation: u32,
    color_rgba8: u32,
    size_half: [u16; 2],
    uv_rect_half: [u16; 4],
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
            color_rgba8: pack_rgba8(color),
            size_half: size.map(pack_size_half),
            uv_rect_half: uv_rect.map(pack_half),
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
        unpack_rgba8(self.color_rgba8)
    }

    #[inline]
    pub fn size(self) -> [f32; 2] {
        self.size_half.map(unpack_size_half)
    }

    #[inline]
    pub fn uv_rect(self) -> [f32; 4] {
        self.uv_rect_half.map(unpack_half)
    }
}

#[derive(Clone)]
pub struct PhaseItem {
    pub sort_key: u64,
    pub draw_function_id: crate::render::phase::DrawFunctionId,
    pub entity: EntityId,
    pub batch_key: u64,
    data: [u64; 3],
}

impl PhaseItem {
    #[inline]
    pub fn new<T: Copy>(
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
            data: [0; 3],
        };
        item.set_data(payload);
        item
    }

    #[inline]
    pub fn set_data<T: Copy>(&mut self, value: T) {
        const { assert!(std::mem::size_of::<T>() <= 24) };
        const { assert!(std::mem::align_of::<T>() <= std::mem::align_of::<u64>()) };
        unsafe {
            std::ptr::write(self.data.as_mut_ptr().cast::<T>(), value);
        }
    }

    #[inline]
    pub fn data<T: Copy>(&self) -> &T {
        const { assert!(std::mem::size_of::<T>() <= 24) };
        const { assert!(std::mem::align_of::<T>() <= std::mem::align_of::<u64>()) };
        unsafe { &*self.data.as_ptr().cast::<T>() }
    }

    #[inline]
    pub fn data_mut<T: Copy>(&mut self) -> &mut T {
        const { assert!(std::mem::size_of::<T>() <= 24) };
        const { assert!(std::mem::align_of::<T>() <= std::mem::align_of::<u64>()) };
        unsafe { &mut *self.data.as_mut_ptr().cast::<T>() }
    }
}

#[inline]
fn pack_rgba8(color: [f32; 4]) -> u32 {
    let [r, g, b, a] = color.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u32);
    r | (g << 8) | (b << 16) | (a << 24)
}

#[inline]
fn unpack_rgba8(color: u32) -> [f32; 4] {
    [
        (color & 0xff) as f32 / 255.0,
        ((color >> 8) & 0xff) as f32 / 255.0,
        ((color >> 16) & 0xff) as f32 / 255.0,
        ((color >> 24) & 0xff) as f32 / 255.0,
    ]
}

#[inline]
fn pack_half(value: f32) -> u16 {
    (value.clamp(0.0, 1.0) * 65535.0).round() as u16
}

#[inline]
fn unpack_half(value: u16) -> f32 {
    value as f32 / 65535.0
}

#[inline]
fn pack_size_half(value: f32) -> u16 {
    value.clamp(0.0, 4096.0).round() as u16
}

#[inline]
fn unpack_size_half(value: u16) -> f32 {
    value as f32
}
