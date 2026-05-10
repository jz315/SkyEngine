use super::MaterialInstanceId;

#[derive(Default)]
pub(super) struct DirtyMaterialQueue {
    ids: Vec<MaterialInstanceId>,
}

impl DirtyMaterialQueue {
    #[inline]
    pub fn new() -> Self {
        Self { ids: Vec::new() }
    }

    pub fn mark(&mut self, id: MaterialInstanceId) {
        if !self.ids.contains(&id) {
            self.ids.push(id);
        }
    }

    pub fn remove(&mut self, id: MaterialInstanceId) {
        self.ids.retain(|dirty| *dirty != id);
    }

    pub fn retain(&mut self, keep: impl FnMut(&MaterialInstanceId) -> bool) {
        self.ids.retain(keep);
    }

    pub fn drain(&mut self) -> Vec<MaterialInstanceId> {
        std::mem::take(&mut self.ids)
    }
}
