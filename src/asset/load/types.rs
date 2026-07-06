use std::any::Any;
use std::sync::atomic::{AtomicU8, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::Duration;

use crate::asset::types::{
    AssetError, AssetFailurePhase, AssetId, AssetManifestEntry, LoadedAsset,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssetSourceLoadPhase {
    Queued,
    Reading,
    Decoding,
}
#[derive(Clone, Debug)]
pub(crate) struct AssetLoadPhaseTracker {
    phase: Arc<AtomicU8>,
}
impl AssetLoadPhaseTracker {
    const QUEUED: u8 = 0;
    const READING: u8 = 1;
    const DECODING: u8 = 2;
    pub(crate) fn new() -> Self {
        Self {
            phase: Arc::new(AtomicU8::new(Self::QUEUED)),
        }
    }
    pub(crate) fn set(&self, phase: AssetSourceLoadPhase) {
        self.phase.store(phase.as_u8(), AtomicOrdering::Release);
    }
    pub(crate) fn phase(&self) -> AssetSourceLoadPhase {
        AssetSourceLoadPhase::from_u8(self.phase.load(AtomicOrdering::Acquire))
    }
}
impl AssetSourceLoadPhase {
    fn as_u8(self) -> u8 {
        match self {
            Self::Queued => AssetLoadPhaseTracker::QUEUED,
            Self::Reading => AssetLoadPhaseTracker::READING,
            Self::Decoding => AssetLoadPhaseTracker::DECODING,
        }
    }
    fn from_u8(value: u8) -> Self {
        match value {
            AssetLoadPhaseTracker::READING => Self::Reading,
            AssetLoadPhaseTracker::DECODING => Self::Decoding,
            _ => Self::Queued,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct AssetLoadTimingSample {
    pub(crate) sampled: bool,
    pub(crate) read_time: Duration,
    pub(crate) decode_time: Duration,
    pub(crate) total_time: Duration,
}
#[derive(Debug)]
pub(crate) struct TimedAssetLoadError {
    pub(crate) error: AssetError,
    pub(crate) timings: AssetLoadTimingSample,
}
impl From<AssetError> for TimedAssetLoadError {
    fn from(error: AssetError) -> Self {
        Self {
            error,
            timings: AssetLoadTimingSample::default(),
        }
    }
}
pub(crate) struct LoadedSourceAsset {
    pub(crate) loaded: LoadedAsset<Arc<dyn Any + Send + Sync>>,
    pub(crate) content_hash: String,
    pub(crate) timings: AssetLoadTimingSample,
}
pub(crate) struct CompletedLoad {
    pub(crate) id: AssetId,
    pub(crate) generation: u64,
    pub(crate) entry: AssetManifestEntry,
    pub(crate) cooked_hash: Option<String>,
    pub(crate) timings: AssetLoadTimingSample,
    pub(crate) result: Result<LoadedAsset<Arc<dyn Any + Send + Sync>>, AssetError>,
}
#[derive(Debug)]
pub(crate) struct CompletedLoadFailure {
    pub(crate) id: AssetId,
    pub(crate) error: AssetError,
    pub(crate) phase: AssetFailurePhase,
}
