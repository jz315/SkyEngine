//! Post-processing effects.

pub mod bloom;
pub mod tonemap;
pub mod vignette;

use crate::gpu::Gpu;
use crate::render::target::RenderTarget;

/// Shared trait for post-processing passes.
pub trait PostFx {
    fn apply_to_target<G: Gpu>(&mut self, gpu: &mut G, input: &RenderTarget, output: &RenderTarget);
}
