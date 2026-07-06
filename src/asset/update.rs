use std::time::{Duration, Instant};

use super::driver::{self, AssetDriveMode, AssetRecordDriver};
use super::types::{AssetError, AssetId};

pub(crate) trait AssetUpdateContext: AssetRecordDriver {
    fn drain_handle_releases(&mut self);
    fn maybe_auto_reload(&mut self) -> Result<(), AssetError>;
    fn activate_queued_requests(&mut self) -> Result<(), AssetError>;
    fn refresh_active_requests_after_drive(&mut self);
    fn install_budget_per_update(&self) -> Option<usize>;
    fn install_time_budget(&self) -> Option<Duration>;
}

pub(crate) fn apply_update(ctx: &mut impl AssetUpdateContext) -> Result<(), AssetError> {
    ctx.drain_handle_releases();
    ctx.maybe_auto_reload()?;
    ctx.activate_queued_requests()?;
    let _ = AssetRecordDriver::drain_load_completions(ctx)?;
    driver::drive_records(
        ctx,
        AssetDriveMode::Normal {
            install_budget_per_update: ctx.install_budget_per_update(),
            install_time_budget: ctx.install_time_budget(),
            started_at: Instant::now(),
        },
    )?;

    ctx.refresh_active_requests_after_drive();
    Ok(())
}

pub(crate) fn apply_blocking_update(
    ctx: &mut impl AssetUpdateContext,
    target_id: AssetId,
    force_synchronous_load: bool,
) -> Result<(), AssetError> {
    ctx.drain_handle_releases();
    ctx.activate_queued_requests()?;
    let _ = AssetRecordDriver::drain_load_completions(ctx)?;

    driver::drive_records(
        ctx,
        AssetDriveMode::Blocking {
            target_id,
            force_synchronous_load,
            started_at: Instant::now(),
        },
    )?;

    ctx.refresh_active_requests_after_drive();
    Ok(())
}
