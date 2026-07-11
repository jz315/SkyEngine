use super::instance::MaterialInstanceRecord;
use super::{ErasedMaterialHandle, MaterialError, MaterialInstanceId, MaterialModelId};

#[derive(Default)]
pub(super) struct MaterialInstanceStore {
    records: Vec<Option<MaterialInstanceRecord>>,
    generations: Vec<u32>,
    free: Vec<u32>,
}

impl MaterialInstanceStore {
    #[inline]
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            generations: Vec::new(),
            free: Vec::new(),
        }
    }

    pub fn insert_with(
        &mut self,
        create: impl FnOnce(MaterialInstanceId) -> MaterialInstanceRecord,
    ) -> MaterialInstanceId {
        let index = if let Some(index) = self.free.pop() {
            index
        } else {
            let index = self.records.len() as u32;
            self.records.push(None);
            self.generations.push(0);
            index
        };
        let id = MaterialInstanceId::new(index, self.generations[index as usize]);
        self.records[index as usize] = Some(create(id));
        id
    }

    pub fn remove(
        &mut self,
        handle: ErasedMaterialHandle,
    ) -> Result<MaterialInstanceRecord, MaterialError> {
        let slot = self.slot_for_handle(handle)?;
        let record = self.records[slot]
            .take()
            .ok_or(MaterialError::StaleMaterialHandle { id: handle.id() })?;
        self.generations[slot] = self.generations[slot].wrapping_add(1);
        self.free.push(handle.id().index());
        Ok(record)
    }

    pub fn clear_model(&mut self, model: MaterialModelId) {
        for index in 0..self.records.len() {
            let remove = self.records[index]
                .as_ref()
                .is_some_and(|record| record.model == model);
            if remove {
                self.records[index] = None;
                self.generations[index] = self.generations[index].wrapping_add(1);
                self.free.push(index as u32);
            }
        }
    }

    #[inline]
    pub fn records(&self) -> &[Option<MaterialInstanceRecord>] {
        &self.records
    }

    #[inline]
    pub fn contains_live(&self, id: MaterialInstanceId) -> bool {
        self.record_by_id(id).is_some()
    }

    pub fn record(
        &self,
        handle: ErasedMaterialHandle,
    ) -> Result<&MaterialInstanceRecord, MaterialError> {
        let slot = self.slot_for_handle(handle)?;
        self.records[slot]
            .as_ref()
            .ok_or(MaterialError::StaleMaterialHandle { id: handle.id() })
    }

    pub fn record_mut(
        &mut self,
        handle: ErasedMaterialHandle,
    ) -> Result<&mut MaterialInstanceRecord, MaterialError> {
        let slot = self.slot_for_handle(handle)?;
        self.records[slot]
            .as_mut()
            .ok_or(MaterialError::StaleMaterialHandle { id: handle.id() })
    }

    pub fn record_by_id(&self, id: MaterialInstanceId) -> Option<&MaterialInstanceRecord> {
        let slot = self.slot_for_id(id)?;
        self.records[slot].as_ref()
    }

    pub fn record_mut_by_id(
        &mut self,
        id: MaterialInstanceId,
    ) -> Result<&mut MaterialInstanceRecord, MaterialError> {
        let Some(slot) = self.slot_for_id(id) else {
            return Err(MaterialError::StaleMaterialHandle { id });
        };
        self.records[slot]
            .as_mut()
            .ok_or(MaterialError::StaleMaterialHandle { id })
    }

    fn slot_for_handle(&self, handle: ErasedMaterialHandle) -> Result<usize, MaterialError> {
        let slot = handle.id().index() as usize;
        let Some(generation) = self.generations.get(slot) else {
            return Err(MaterialError::StaleMaterialHandle { id: handle.id() });
        };
        if *generation != handle.id().generation() {
            return Err(MaterialError::StaleMaterialHandle { id: handle.id() });
        }
        let record = self
            .records
            .get(slot)
            .and_then(Option::as_ref)
            .ok_or(MaterialError::StaleMaterialHandle { id: handle.id() })?;
        if handle.model_id().index() != u32::MAX && record.model != handle.model_id() {
            return Err(MaterialError::WrongMaterialModel {
                expected: record.model,
                actual: handle.model_id(),
            });
        }
        if record.model_type != handle.model_type() {
            return Err(MaterialError::WrongMaterialModel {
                expected: record.model,
                actual: handle.model_id(),
            });
        }
        Ok(slot)
    }

    fn slot_for_id(&self, id: MaterialInstanceId) -> Option<usize> {
        let slot = id.index() as usize;
        let generation = self.generations.get(slot).copied()?;
        (generation == id.generation()
            && self
                .records
                .get(slot)
                .and_then(Option::as_ref)
                .is_some_and(|record| record.id == id))
        .then_some(slot)
    }
}
