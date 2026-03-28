use super::*;
use std::collections::HashMap;

// Archetype归谁管理，1.Rc 2.Manager 生命周期是多少 如果没有World使用的Archetype是否应该Drop Archetype存什么，存包含的Component信息 在什么时候会创建用Archetype创建东西，比如有一个市民，我需要创建，不确定什么时候需要，Archetpye需要作为动态结构体一样，因为不确定什么时候会用，Archetype是共享的，生命周期大于World，如果使用引用
pub struct World {
    pub data: Vec<Data>,
    archetype_to_data_index: HashMap<Archetype, usize>,
    archetype_epoch: usize,
}

impl World {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            archetype_to_data_index: HashMap::new(),
            archetype_epoch: 0,
        }
    }

    pub fn add_entity(&mut self, archetype: Archetype) {
        if let Some(index) = self.archetype_to_data_index.get(&archetype).copied() {
            self.data[index].add_entity();
        } else {
            let mut data = Data::new(archetype);
            data.add_entity();
            let index = self.data.len();
            self.data.push(data);
            self.archetype_to_data_index.insert(archetype, index);
            self.archetype_epoch += 1;
        }
    }

    pub fn archetype_epoch(&self) -> usize {
        self.archetype_epoch
    }

    pub fn query<Q>(&self) -> PreparedQuery<Q>
    where
        Q: QuerySpec,
    {
        PreparedQuery::new()
    }
}
