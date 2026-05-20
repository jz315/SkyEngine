use rustc_hash::FxHashMap;
use std::cell::Cell;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::ptr;
use std::sync::RwLock;

use smallvec::SmallVec;

use crate::ecs::{component_type, ComponentType};

use super::Query;

pub const MAX_COMPONENTS: usize = 32;

lazy_static::lazy_static! {
    static ref ARCHETYPE_CACHE: RwLock<FxHashMap<Vec<usize>, Archetype>> = RwLock::new(FxHashMap::default());
}

thread_local! {
    static LAST_COMPONENT_LOOKUP: Cell<Option<(usize, usize, u8)>> = const { Cell::new(None) };
}

// 定义Archetype结构体，包括组件信息和整体对齐要求
#[derive(Debug)]
pub struct InternalArchetype {
    pub components: SmallVec<[ComponentType; MAX_COMPONENTS]>,
    pub(crate) drop_component_indices: SmallVec<[u8; MAX_COMPONENTS]>,
    pub alignment: usize,
}

impl InternalArchetype {
    // 创建一个新的Archetype
    fn new() -> Self {
        InternalArchetype {
            components: SmallVec::new(),
            drop_component_indices: SmallVec::new(),
            alignment: 1,
        }
    }

    // 添加一个Component
    fn add_component(mut self, ty: ComponentType) -> Self {
        self.alignment = self.alignment.max(ty.align);
        self.components.push(ty);

        self
    }

    fn build(mut self) -> Self {
        self.components.sort_by_key(|a| a.id());
        self.drop_component_indices.clear();
        for (index, component) in self.components.iter().enumerate() {
            if component.needs_drop() {
                debug_assert!(index < u8::MAX as usize);
                self.drop_component_indices.push(index as u8);
            }
        }
        self
    }
}

impl InternalArchetype {
    // 查询Archetype是否包含指定类型的Component
    #[inline(always)]
    pub fn has_component(&self, ty: &ComponentType) -> bool {
        self.query_component_index(ty).is_some()
    }

    #[inline(always)]
    pub fn query_component_index(&self, ty: &ComponentType) -> Option<usize> {
        let archetype_id = self as *const InternalArchetype as usize;
        let component_id = ty.id();

        if let Some((cached_archetype_id, cached_component_id, cached_index)) =
            LAST_COMPONENT_LOOKUP.with(Cell::get)
        {
            if cached_archetype_id == archetype_id && cached_component_id == component_id {
                return Some(cached_index as usize);
            }
        }

        let index = self
            .components
            .binary_search_by(|component| component.id().cmp(&ty.id()))
            .ok()?;

        debug_assert!(index < u8::MAX as usize);
        LAST_COMPONENT_LOOKUP.with(|cache| {
            cache.set(Some((archetype_id, component_id, index as u8)));
        });
        Some(index)
    }

    #[inline(always)]
    pub fn matches_query(&self, query: &Query) -> bool {
        query.types.iter().all(|ty| self.has_component(ty))
    }
}

impl fmt::Display for InternalArchetype {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Archetype[")?;
        for (i, component) in self.components.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", component.name)?;
        }
        write!(f, "]")
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

    #[inline(always)]
    pub fn id(&self) -> usize {
        self.archetype as *const InternalArchetype as usize
    }
}

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

    pub fn add_component(mut self, ty: ComponentType) -> Self {
        self.internal_archetype = self.internal_archetype.add_component(ty);
        self
    }

    pub fn add_rust_component<T: 'static>(self) -> Self {
        self.add_component(component_type::<T>())
    }

    pub fn build(self) -> Archetype {
        let built = self.internal_archetype.build();
        let key = built
            .components
            .iter()
            .map(|component| component.id())
            .collect::<Vec<_>>();

        if let Some(archetype) = ARCHETYPE_CACHE.read().unwrap().get(&key).copied() {
            return archetype;
        }

        let archetype = Archetype::new(Box::leak(Box::new(built)));
        let mut cache = ARCHETYPE_CACHE.write().unwrap();
        *cache.entry(key).or_insert(archetype)
    }
}

pub fn create_archetype() -> ArchetypeBuilder {
    ArchetypeBuilder::new()
}
