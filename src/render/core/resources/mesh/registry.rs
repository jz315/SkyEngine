use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;

use super::Mesh;

/// Stable handle used by future ECS-facing mesh components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshHandle {
    kind: MeshHandleKind,
    slot: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MeshHandleKind {
    Builtin,
    Dynamic,
}

impl MeshHandle {
    pub const BUILTIN_QUAD: Self = Self::builtin(0);

    #[inline]
    pub const fn builtin(slot: u32) -> Self {
        Self {
            kind: MeshHandleKind::Builtin,
            slot,
        }
    }

    #[inline]
    pub const fn dynamic(slot: u32) -> Self {
        Self {
            kind: MeshHandleKind::Dynamic,
            slot,
        }
    }

    #[inline]
    pub const fn is_builtin(self) -> bool {
        matches!(self.kind, MeshHandleKind::Builtin)
    }

    #[inline]
    pub const fn slot(self) -> u32 {
        self.slot
    }
}

/// Type-erased registry backing [`MeshHandle`] lookups.
#[derive(Clone)]
pub struct MeshRegistry {
    builtins: FxHashMap<u32, Mesh>,
    meshes: Vec<Option<Mesh>>,
    generations: Vec<u32>,
    free_list: Vec<u32>,
    len: usize,
}

impl MeshRegistry {
    #[inline]
    pub fn new() -> Self {
        Self {
            builtins: FxHashMap::default(),
            meshes: Vec::new(),
            generations: Vec::new(),
            free_list: Vec::new(),
            len: 0,
        }
    }

    pub fn ensure_builtin_quad(&mut self, ctx: &GpuContext) -> MeshHandle {
        self.builtins
            .entry(MeshHandle::BUILTIN_QUAD.slot())
            .or_insert_with(|| Mesh::builtin_quad(ctx));
        MeshHandle::BUILTIN_QUAD
    }

    pub fn insert(&mut self, mesh: Mesh) -> MeshHandle {
        let slot = if let Some(slot) = self.free_list.pop() {
            self.meshes[slot as usize] = Some(mesh);
            slot
        } else {
            let slot = self.meshes.len() as u32;
            self.meshes.push(Some(mesh));
            self.generations.push(0);
            slot
        };
        self.len += 1;
        MeshHandle::dynamic(slot)
    }

    pub fn get(&self, handle: MeshHandle) -> Option<&Mesh> {
        match handle.kind {
            MeshHandleKind::Builtin => self.builtins.get(&handle.slot),
            MeshHandleKind::Dynamic => self.meshes.get(handle.slot as usize)?.as_ref(),
        }
    }

    pub fn get_mut(&mut self, handle: MeshHandle) -> Option<&mut Mesh> {
        match handle.kind {
            MeshHandleKind::Builtin => self.builtins.get_mut(&handle.slot),
            MeshHandleKind::Dynamic => self.meshes.get_mut(handle.slot as usize)?.as_mut(),
        }
    }

    pub fn remove(&mut self, handle: MeshHandle) -> Option<Mesh> {
        if handle.is_builtin() {
            return None;
        }
        let mesh = self.meshes.get_mut(handle.slot as usize)?.take()?;
        self.generations[handle.slot as usize] =
            self.generations[handle.slot as usize].wrapping_add(1);
        self.free_list.push(handle.slot);
        self.len -= 1;
        Some(mesh)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len + self.builtins.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0 && self.builtins.is_empty()
    }
}

impl Default for MeshRegistry {
    fn default() -> Self {
        Self::new()
    }
}
