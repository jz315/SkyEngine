use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::{ptr, rc::Rc};

use smallvec::SmallVec;

use crate::reflect::*;

use super::{Chunk, Query};

pub const MAX_COMPONENTS: usize = 32;

// 定义Archetype结构体，包括内存布局和对齐信息，以及组件信息
#[derive(Debug)]
pub struct InternalArchetype {
    pub layout: SmallVec<[usize; MAX_COMPONENTS]>,
    pub components: SmallVec<[Type; MAX_COMPONENTS]>,

    pub alignment: usize,
    pub total_size: usize,
}

impl InternalArchetype {
    // 创建一个新的Archetype
    fn new() -> Self {
        InternalArchetype {
            components: SmallVec::new(),
            layout: SmallVec::new(),

            alignment: 1,
            total_size: 0,
        }
    }

    // 添加一个Component
    fn add_component(mut self, ty: Type) -> Self {
        self.alignment = self.alignment.max(ty.align);
        self.total_size += ty.size;

        self.components.push(ty.clone());

        self
    }

    fn build(mut self) -> Self {
        self.components.sort_by(|a, b| a.id().cmp(&b.id()));

        let mut offset = 0;
        for component in &self.components {
            // 对齐偏移量
            offset = (offset + component.align - 1) & !(component.align - 1);
            self.layout.push(offset);
            offset += component.size;
        }

        // 对齐total_size
        self.total_size = (offset + self.alignment - 1) & !(self.alignment - 1);

        self
    }
}

impl InternalArchetype {
    // 查询Archetype是否包含指定类型的Component
    pub fn has_component(&self, ty: &Type) -> bool {
        self.components
            .binary_search_by(|component| component.id().cmp(&ty.id()))
            .is_ok()
    }

    // 查询Archetype是否包含指定类型的Component
    pub fn query_component_offset(&self, ty: &Type) -> Option<usize> {
        match self
            .components
            .binary_search_by(|component| component.id().cmp(&ty.id()))
        {
            Ok(index) => Some(self.layout[index]),
            Err(_) => None,
        }
    }

    pub fn matches_query(&self, query: &Query) -> bool {
        query.types.iter().all(|ty| self.has_component(ty))
    }

    // 打印Archetype的内存布局信息
    pub fn print_layout(&self) {
        println!("Archetype Alignment: {}", self.alignment);
        println!("Archetype Total Size: {}", self.total_size);
        println!("Components:");
        for (i, component) in self.components.iter().enumerate() {
            println!(
                "  - : Size = {}, Align = {}, Offset = {}",
                component.size, component.align, self.layout[i]
            );
        }
        println!("Layout Offsets: {:?}", self.layout);
    }
}
impl PartialEq for Archetype {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.archetype, other.archetype)
    }
}

impl Eq for Archetype {}

impl Hash for Archetype {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.id());
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Archetype {
    archetype: &'static InternalArchetype,
}

impl Archetype {
    fn new(archetype: &'static InternalArchetype) -> Self {
        Archetype { archetype }
    }

    pub fn id(&self) -> usize {
        self.archetype as *const InternalArchetype as usize
    }
}

// Implement Deref to allow `Type` to be treated like `&TypeInfo`
impl Deref for Archetype {
    type Target = InternalArchetype;

    fn deref(&self) -> &Self::Target {
        self.archetype
    }
}

// 定义ArchetypeBuilder结构体
pub struct ArchetypeBuilder {
    internal_archetype: InternalArchetype,
}

impl ArchetypeBuilder {
    fn new() -> Self {
        ArchetypeBuilder {
            internal_archetype: InternalArchetype::new(),
        }
    }

    pub fn add_component(mut self, ty: Type) -> Self {
        self.internal_archetype = self.internal_archetype.add_component(ty);
        self
    }

    pub fn build(self) -> Archetype {
        let internal_archetype = Box::leak(Box::new(self.internal_archetype.build()));
        Archetype::new(internal_archetype)
    }
}

pub fn create_archetype() -> ArchetypeBuilder {
    ArchetypeBuilder::new()
}
