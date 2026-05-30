use std::sync::Arc;
use std::time::{Duration, Instant};

use super::types::{AssetError, AssetId, AssetState};

const POLL_INTERVAL: Duration = Duration::from_millis(1);

pub(crate) enum BlockingLoadStatus<T> {
    Ready(Result<Arc<T>, AssetError>),
    Failed(Option<AssetError>),
    Pending(AssetState),
}

pub(crate) fn deadline_from_timeout(
    id: AssetId,
    timeout: Duration,
    current_state: impl FnOnce() -> AssetState,
) -> Result<Instant, AssetError> {
    Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| AssetError::InvalidState {
            id,
            state: current_state(),
            message: "blocking load timeout is too large".to_string(),
        })
}

pub(crate) fn drive_until_ready<T>(
    id: AssetId,
    deadline: Option<Instant>,
    mut drive: impl FnMut(bool) -> Result<(), AssetError>,
    mut observe: impl FnMut() -> BlockingLoadStatus<T>,
) -> Result<Arc<T>, AssetError> {
    loop {
        drive(deadline.is_none())?;

        match observe() {
            BlockingLoadStatus::Ready(result) => return result,
            BlockingLoadStatus::Failed(Some(error)) => return Err(error),
            BlockingLoadStatus::Failed(None) => {
                return Err(AssetError::AssetNotInstalled {
                    id,
                    state: AssetState::Failed,
                });
            }
            BlockingLoadStatus::Pending(state) => wait_or_timeout(id, state, deadline)?,
        }
    }
}

fn wait_or_timeout(
    id: AssetId,
    state: AssetState,
    deadline: Option<Instant>,
) -> Result<(), AssetError> {
    let Some(deadline) = deadline else {
        std::thread::sleep(POLL_INTERVAL);
        return Ok(());
    };

    let now = Instant::now();
    if now >= deadline {
        return Err(AssetError::InvalidState {
            id,
            state,
            message: "asset did not reach a terminal state before blocking load timeout"
                .to_string(),
        });
    }

    let remaining = deadline.saturating_duration_since(now);
    std::thread::sleep(remaining.min(POLL_INTERVAL));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocking_driver_returns_ready_payload_after_drive() {
        use std::cell::Cell;

        let id = AssetId::new();
        let drove = Cell::new(false);

        let value = drive_until_ready(
            id,
            None,
            |force_synchronous_load| {
                assert!(force_synchronous_load);
                drove.set(true);
                Ok(())
            },
            || {
                assert!(drove.get());
                BlockingLoadStatus::Ready(Ok(Arc::new(7_u32)))
            },
        )
        .expect("ready payload should return");

        assert_eq!(*value, 7);
    }

    #[test]
    fn blocking_driver_maps_failed_without_error_to_not_installed() {
        let id = AssetId::new();

        let error =
            drive_until_ready::<u32>(id, None, |_| Ok(()), || BlockingLoadStatus::Failed(None))
                .expect_err("failed state without stored error should map to not installed");

        assert!(matches!(
            error,
            AssetError::AssetNotInstalled {
                id: failed,
                state: AssetState::Failed,
            } if failed == id
        ));
    }

    #[test]
    fn blocking_driver_times_out_pending_state_after_one_drive() {
        let id = AssetId::new();
        let mut drives = 0;

        let error = drive_until_ready::<u32>(
            id,
            Some(Instant::now()),
            |_| {
                drives += 1;
                Ok(())
            },
            || BlockingLoadStatus::Pending(AssetState::Loading),
        )
        .expect_err("expired deadline should report timeout");

        assert_eq!(drives, 1);
        assert!(matches!(
            error,
            AssetError::InvalidState {
                id: failed,
                state: AssetState::Loading,
                ..
            } if failed == id
        ));
    }
}
