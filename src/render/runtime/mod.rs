#[path = "runtime.rs"]
mod engine_runtime;
mod executor;
mod frame;
mod frame_coordinator;
mod history;
mod outcome;
mod pipeline_runtime;
mod presentation;
mod state;
mod stats;
mod temporal;
#[cfg(test)]
mod tests;
mod view_collection;

pub use engine_runtime::RenderRuntime;
pub use history::{HistoryTexture, HistoryTextureRequest, HistoryTextureSize};
pub use outcome::{FrameRenderOutcome, FrameSkipReason};
pub use stats::RenderTimingStats;

pub(crate) use frame::PreviousModelMatrices;
pub(crate) use history::HistoryTextureStore;
pub(crate) use presentation::ViewportBlitNode;
pub(crate) use stats::{elapsed_ms, timing_start};
pub(crate) use temporal::TemporalViewTracker;
pub(crate) use view_collection::WorldViewCollector;
