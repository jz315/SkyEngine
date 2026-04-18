//! Type-erased material handle and per-type generational storage.

use std::any::TypeId;

use super::traits::Material;

/// Type-erased material handle for ECS-facing renderer components.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialHandle {
    type_id: TypeId,
    index: u32,
    generation: u32,
}

impl MaterialHandle {
    #[inline]
    pub fn new<M: Material>(index: u32, generation: u32) -> Self {
        Self {
            type_id: TypeId::of::<M>(),
            index,
            generation,
        }
    }

    #[inline]
    pub fn index(self) -> u32 {
        self.index
    }

    #[inline]
    pub fn generation(self) -> u32 {
        self.generation
    }

    #[inline]
    pub fn type_id(self) -> TypeId {
        self.type_id
    }

    #[inline]
    pub fn is<M: Material>(self) -> bool {
        self.type_id == TypeId::of::<M>()
    }
}

/// Per-material-type typed storage.
pub struct MaterialStorage<M: Material> {
    materials: Vec<Option<M>>,
    generations: Vec<u32>,
    free_list: Vec<u32>,
    len: usize,
}

impl<M: Material> MaterialStorage<M> {
    #[inline]
    pub fn new() -> Self {
        Self {
            materials: Vec::new(),
            generations: Vec::new(),
            free_list: Vec::new(),
            len: 0,
        }
    }

    pub fn insert(&mut self, material: M) -> MaterialHandle {
        let index = if let Some(index) = self.free_list.pop() {
            self.materials[index as usize] = Some(material);
            index
        } else {
            let index = self.materials.len() as u32;
            self.materials.push(Some(material));
            self.generations.push(0);
            index
        };
        self.len += 1;
        MaterialHandle::new::<M>(index, self.generations[index as usize])
    }

    pub fn get(&self, handle: MaterialHandle) -> Option<&M> {
        if !self.matches(handle) {
            return None;
        }
        self.materials.get(handle.index as usize)?.as_ref()
    }

    pub fn get_mut(&mut self, handle: MaterialHandle) -> Option<&mut M> {
        if !self.matches(handle) {
            return None;
        }
        self.materials.get_mut(handle.index as usize)?.as_mut()
    }

    pub fn remove(&mut self, handle: MaterialHandle) -> Option<M> {
        if !self.matches(handle) {
            return None;
        }

        let slot = handle.index as usize;
        let material = self.materials[slot].take()?;
        self.generations[slot] = self.generations[slot].wrapping_add(1);
        self.free_list.push(handle.index);
        self.len -= 1;
        Some(material)
    }

    pub fn clear(&mut self) {
        self.free_list.clear();
        for (index, slot) in self.materials.iter_mut().enumerate() {
            if slot.take().is_some() {
                self.generations[index] = self.generations[index].wrapping_add(1);
                self.free_list.push(index as u32);
            }
        }
        self.len = 0;
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn matches(&self, handle: MaterialHandle) -> bool {
        if handle.type_id != TypeId::of::<M>() {
            return false;
        }
        self.generations
            .get(handle.index as usize)
            .is_some_and(|generation| *generation == handle.generation)
    }
}

impl<M> MaterialStorage<M>
where
    M: Material + Clone,
{
    pub fn clone_from_storage(&mut self, other: &Self) {
        self.materials = other.materials.clone();
        self.generations = other.generations.clone();
        self.free_list = other.free_list.clone();
        self.len = other.len;
    }
}

impl<M: Material> Default for MaterialStorage<M> {
    fn default() -> Self {
        Self::new()
    }
}
