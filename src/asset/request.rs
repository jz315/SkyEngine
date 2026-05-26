use std::any::TypeId;

use super::types::{AssetId, AssetState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssetRequestPhase {
    Queued,
    Loading,
    WaitingDependencies,
    ReadyToInstall,
    Installing,
    Installed,
    Unloading,
    Unloaded,
    Failed,
}

impl AssetRequestPhase {
    pub(crate) fn from_state(state: AssetState) -> Self {
        match state {
            AssetState::Unloaded => Self::Unloaded,
            AssetState::Loading => Self::Loading,
            AssetState::Loaded => Self::ReadyToInstall,
            AssetState::WaitingDependencies => Self::WaitingDependencies,
            AssetState::Installing => Self::Installing,
            AssetState::Installed => Self::Installed,
            AssetState::Uninstalling | AssetState::Unloading => Self::Unloading,
            AssetState::Failed => Self::Failed,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AssetRequest {
    pub(crate) id: AssetId,
    pub(crate) requested_type: Option<TypeId>,
    pub(crate) phase: AssetRequestPhase,
}

impl AssetRequest {
    pub(crate) fn new(id: AssetId, requested_type: Option<TypeId>) -> Self {
        Self {
            id,
            requested_type,
            phase: AssetRequestPhase::Queued,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_phase_maps_record_states() {
        assert_eq!(
            AssetRequestPhase::from_state(AssetState::Loaded),
            AssetRequestPhase::ReadyToInstall
        );
        assert_eq!(
            AssetRequestPhase::from_state(AssetState::WaitingDependencies),
            AssetRequestPhase::WaitingDependencies
        );
        assert_eq!(
            AssetRequestPhase::from_state(AssetState::Uninstalling),
            AssetRequestPhase::Unloading
        );
    }
}
