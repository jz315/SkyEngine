use super::{Archetype, Query};
use smallvec::smallvec;
use smallvec::SmallVec;
use std::collections::HashSet;
use std::ptr;

use std::rc::Rc;

const CHUNK_SIZE: usize = 48 * 1024;

pub struct Chunk {
    pub entity_count: usize,
    pub max_entity_count: usize,
    pub archetype: Archetype,   // Archetype describing the layout
    pub data: [u8; CHUNK_SIZE], // Raw data storage
}
//创建Archetype之后，用创建Entity,
impl Chunk {
    pub fn new(archetype: Archetype) -> Self {
        Chunk {
            data: [0; CHUNK_SIZE],
            archetype: archetype,
            entity_count: 0,
            max_entity_count: CHUNK_SIZE / archetype.total_size,
        }
    }

    pub fn is_full(&self) -> bool {
        self.entity_count == self.max_entity_count
    }

    pub fn is_empty(&self) -> bool {
        self.entity_count == 0
    }

    pub fn add_entity(&mut self) -> bool {
        if self.entity_count == self.max_entity_count {
            return false;
        }

        self.entity_count += 1;

        return true;
    }

    pub fn get_entity() {
        todo!()
    }

    pub fn get_entity_as_ptr(&self, index: usize) -> *const u8 {
        if index >= self.entity_count {
            return ptr::null(); // 索引无效时返回空指针
        }
        let entity_size = self.archetype.total_size;
        let offset = index * entity_size;
        unsafe { self.data.as_ptr().add(offset) }
    }
}

pub struct Data {
    pub archetype: Archetype,
    pub chunks: Vec<Chunk>,
}
// Chunk是什么
impl Data {
    pub fn new(archetype: Archetype) -> Self {
        Data {
            archetype,
            chunks: Vec::new(),
        }
    }

    fn add_chunk(&mut self) {
        let chunk = Chunk::new(self.archetype);
        self.chunks.push(chunk);
    }

    pub fn add_entity(&mut self) {
        loop {
            let chunk = self.chunks.last_mut();
            if !chunk.is_none() {
                if chunk.unwrap().add_entity() {
                    break;
                }
            }

            self.add_chunk();
        }
    }
}

pub struct EntityIter<'a> {
    data: &'a Data,

    current_chunk_index: usize,
    current_entity_index: usize,
}

impl<'a> EntityIter<'a> {
    pub fn new(data: &'a Data) -> Self {
        Self {
            data,
            current_chunk_index: 0,
            current_entity_index: 0,
        }
    }
}

impl<'a> Iterator for EntityIter<'a> {
    type Item = *const u8;

    fn next(&mut self) -> Option<Self::Item> {
        let mut result: Option<Self::Item> = None;
        if self.current_chunk_index < self.data.chunks.len() {
            let chunk = &self.data.chunks[self.current_chunk_index];
            if self.current_entity_index < chunk.entity_count {
                self.current_entity_index += 1;
                result = Some(chunk.get_entity_as_ptr(self.current_entity_index));
            } else {
                self.current_chunk_index += 1;
                self.current_entity_index = 0;
            }
        }
        result
    }
}
