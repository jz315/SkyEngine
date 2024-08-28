use super::*;
use std::hash::{Hash, Hasher};
use std::ptr;
use std::{collections::HashMap, rc::Rc};

// Archetype归谁管理，1.Rc 2.Manager 生命周期是多少 如果没有World使用的Archetype是否应该Drop Archetype存什么，存包含的Component信息 在什么时候会创建用Archetype创建东西，比如有一个市民，我需要创建，不确定什么时候需要，Archetpye需要作为动态结构体一样，因为不确定什么时候会用，Archetype是共享的，生命周期大于World，如果使用引用
pub struct World {
    pub data: HashMap<Archetype, Data>,
}

impl World {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn add_entity(&mut self, archetype: Archetype) {
        // Check if the archetype already exists in the data
        if let Some(data) = self.data.get_mut(&archetype) {
            // Add a new chunk or update an existing chunk
            data.add_entity();
        } else {
            // If the archetype does not exist, create a new Data entry
            let mut data = Data::new(archetype);
            data.add_entity();

            self.data.insert(archetype, data);
        }
    }

    //pub fn query(&self, query: &Query) -> QueryIter {}
}
