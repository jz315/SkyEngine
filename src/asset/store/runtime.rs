use std::any::{Any, TypeId};
use std::path::PathBuf;
use std::sync::Arc;

use super::{AssetDependencyLeaseUpdate, AssetRecord, AssetStore};
use crate::asset::font::FontAsset;
use crate::asset::texture::TextureAsset;
use crate::asset::types::{Asset, AssetError, AssetFailurePhase, AssetId, AssetState};

impl AssetStore {
    pub(crate) fn ensure_raw_texture_record(&mut self, key: String, path: PathBuf) -> AssetId {
        match self.raw_textures.get(&key).copied() {
            Some(id) => id,
            None => {
                let id = AssetId::new();
                self.raw_textures.insert(key, id);
                self.records.insert(id, AssetRecord::new_raw_texture(path));
                id
            }
        }
    }

    pub(crate) fn ensure_raw_font_record(&mut self, key: String, path: PathBuf) -> AssetId {
        match self.raw_fonts.get(&key).copied() {
            Some(id) => id,
            None => {
                let id = AssetId::new();
                self.raw_fonts.insert(key, id);
                self.records.insert(id, AssetRecord::new_raw_font(path));
                id
            }
        }
    }

    pub(crate) fn retain_raw_texture_record(&mut self, key: String, path: PathBuf) -> AssetId {
        let id = self.ensure_raw_texture_record(key, path.clone());
        self.retain_direct_with(id, Some(TypeId::of::<TextureAsset>()), || {
            AssetRecord::new_raw_texture(path)
        });
        id
    }

    pub(crate) fn retain_raw_font_record(&mut self, key: String, path: PathBuf) -> AssetId {
        let id = self.ensure_raw_font_record(key, path.clone());
        self.retain_direct_with(id, Some(TypeId::of::<FontAsset>()), || {
            AssetRecord::new_raw_font(path)
        });
        id
    }

    pub(crate) fn insert_runtime_asset<T: Asset>(&mut self, id: AssetId, asset: T) {
        let installed: Arc<dyn Any + Send + Sync> = Arc::new(asset);
        self.records.insert(
            id,
            AssetRecord::new_runtime(T::TYPE.to_string(), TypeId::of::<T>(), installed),
        );
    }

    pub(crate) fn replace_runtime_asset<T: Asset>(
        &mut self,
        id: AssetId,
        asset: T,
    ) -> Result<AssetDependencyLeaseUpdate, AssetError> {
        if let Some(record) = self.records.get(&id) {
            if record
                .requested_type
                .is_some_and(|type_id| type_id != TypeId::of::<T>())
            {
                return Err(AssetError::AssetTypeMismatch {
                    id,
                    expected: T::TYPE,
                    actual: record.asset_type.clone(),
                });
            }
        }

        let dependency_update = self.replace_held_dependencies(id, Vec::new());
        let installed: Arc<dyn Any + Send + Sync> = Arc::new(asset);
        match self.records.get_mut(&id) {
            Some(record) => {
                record.replace_runtime(T::TYPE.to_string(), TypeId::of::<T>(), installed);
            }
            None => {
                self.records.insert(
                    id,
                    AssetRecord::new_runtime(T::TYPE.to_string(), TypeId::of::<T>(), installed),
                );
            }
        }

        Ok(dependency_update)
    }

    pub(crate) fn state_for_handle(&self, id: AssetId) -> AssetState {
        self.records
            .get(&id)
            .map(|record| record.state)
            .unwrap_or(AssetState::Unloaded)
    }

    pub(crate) fn error_for_handle(&self, id: AssetId) -> Option<AssetError> {
        self.records
            .get(&id)
            .and_then(|record| record.error.clone())
    }

    pub(crate) fn failure_phase(&self, id: AssetId) -> Option<AssetFailurePhase> {
        self.records
            .get(&id)
            .and_then(|record| record.failure_phase)
    }

    pub(crate) fn get_for_handle(
        &self,
        id: AssetId,
        expected_type: &'static str,
        expected_type_id: TypeId,
    ) -> Result<Arc<dyn Any + Send + Sync>, AssetError> {
        let record = self.records.get(&id).ok_or(AssetError::AssetNotInstalled {
            id,
            state: AssetState::Unloaded,
        })?;
        if record
            .requested_type
            .is_some_and(|type_id| type_id != expected_type_id)
        {
            return Err(AssetError::AssetTypeMismatch {
                id,
                expected: expected_type,
                actual: record.asset_type.clone(),
            });
        }
        record
            .installed
            .clone()
            .ok_or(AssetError::AssetNotInstalled {
                id,
                state: record.state,
            })
    }
}
