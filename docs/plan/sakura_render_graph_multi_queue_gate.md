# Sakura RenderGraph Multi-Queue Gate

Date: 2026-05-27

This gate supports `docs/plan/sakura_render_graph_adaptation_plan.md`
Milestone 8.  It records why real multi-queue execution is not implemented in
the current `wgpu` render graph path.

## Current Evidence

- `RenderGraph::queue_schedule_diagnostic()` now reports dry-run pass classes,
  async compute candidates, copy queue candidates, blockers, dependency levels,
  and resources shared with graphics passes.
- `GpuContext` exposes one `wgpu::Queue` through `queue()` and owns one active
  frame encoder between `begin_frame()` and `end_frame()`.
- `GpuContext::flush(next_encoder_label)` submits the active frame encoder and
  replaces it with a new encoder.  Render graph copy passes already use submit
  boundaries for copy/upload ordering.
- Surface presentation remains tied to `GpuContext::end_frame()`.
- No real Sky workload profile in this audit proves enough compute/copy work to
  justify cross-queue synchronization complexity.

## Gate Decision

Real multi-queue execution remains deferred.

The current implementation intentionally stops at dry-run diagnostics.  It does
not create async command encoders, does not submit to alternate queues, does not
represent cross-queue dependencies, and does not add explicit barrier
generation.  This keeps SkyEngine aligned with its current `wgpu` execution
model while still exposing the facts needed for future design work.

## Required Evidence Before Reopening

Reopen this gate only after all of these are available:

- A real Sky workload report from `queue_schedule_diagnostic()` showing
  meaningful compute or copy candidates.
- Frame timing or allocation/profiling evidence that those candidates are a
  bottleneck on a target backend.
- A concrete `wgpu` backend design for queue access, submission ordering, and
  synchronization.
- A review of `GpuContext::begin_frame()`, `flush()`, and `end_frame()` showing
  how surface presentation order remains correct.
- Tests that prove single-queue behavior remains unchanged when multi-queue is
  disabled.

Until those facts exist, SakuraEngine's queue scheduling, cross-queue sync, and
barrier phases remain reference material rather than implementation targets.
