//! Post-processing effects.

pub mod bloom;
pub mod tonemap;
pub mod vignette;

use crate::gpu::GpuContext;
use crate::render::gpu::RenderTarget;

/// Shared trait for post-processing passes.
pub trait PostFx {
    fn apply_to_target(
        &mut self,
        ctx: &mut GpuContext,
        input: &RenderTarget,
        output: &RenderTarget,
    );
}
