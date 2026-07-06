use std::time::Instant;

use super::{AssetEventRecordContext, AssetRecordDiagnostic, AssetReloadScanRecord, AssetStore};
use crate::asset::types::{AssetId, AssetState};

impl AssetStore {
    pub(crate) fn manifest_refresh_record_ids(&self) -> Vec<AssetId> {
        self.records
            .iter()
            .filter_map(|(id, record)| {
                (!record.runtime && record.raw_source_path.is_none()).then_some(*id)
            })
            .collect()
    }

    pub(crate) fn set_manifest_record_asset_type(
        &mut self,
        id: AssetId,
        asset_type: String,
    ) -> bool {
        let Some(record) = self.records.get_mut(&id) else {
            return false;
        };
        record.asset_type = asset_type;
        true
    }

    pub(crate) fn normal_drive_record_ids(&self) -> Vec<AssetId> {
        let mut ids = self.records.keys().copied().collect::<Vec<_>>();
        ids.sort_by(|left, right| {
            let left_record = self.records.get(left).expect("drive id should exist");
            let right_record = self.records.get(right).expect("drive id should exist");
            match (left_record.state, right_record.state) {
                (AssetState::Loading, AssetState::Loading) => right_record
                    .load_priority
                    .cmp(&left_record.load_priority)
                    .then_with(|| left.as_uuid().as_u128().cmp(&right.as_uuid().as_u128())),
                (AssetState::Loading, _) => std::cmp::Ordering::Less,
                (_, AssetState::Loading) => std::cmp::Ordering::Greater,
                _ => left.as_uuid().as_u128().cmp(&right.as_uuid().as_u128()),
            }
        });
        ids
    }

    pub(crate) fn normal_drive_iteration_limit(&self) -> usize {
        self.records.len().saturating_mul(4).max(8)
    }

    pub(crate) fn reload_scan_records(&self) -> Vec<AssetReloadScanRecord> {
        self.records
            .iter()
            .map(|(id, record)| match record.reload_tracking_fingerprint() {
                Some((loaded_entry_fingerprint, loaded_cooked_hash)) => {
                    AssetReloadScanRecord::Tracked {
                        id: *id,
                        loaded_entry_fingerprint,
                        loaded_cooked_hash,
                    }
                }
                None => AssetReloadScanRecord::Untracked { id: *id },
            })
            .collect()
    }

    pub(crate) fn diagnostic_records(&self, now: Instant) -> Vec<AssetRecordDiagnostic> {
        self.records
            .iter()
            .map(|(id, record)| AssetRecordDiagnostic::from_record(*id, record, now))
            .collect()
    }

    pub(crate) fn diagnostic_record(
        &self,
        id: AssetId,
        now: Instant,
    ) -> Option<AssetRecordDiagnostic> {
        self.records
            .get(&id)
            .map(|record| AssetRecordDiagnostic::from_record(id, record, now))
    }

    pub(crate) fn event_record_context(&self, id: AssetId) -> AssetEventRecordContext {
        self.records
            .get(&id)
            .map(AssetEventRecordContext::from_record)
            .unwrap_or_default()
    }
}
