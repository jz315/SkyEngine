//! Post-processing effects.

pub mod bloom;
pub mod contact_shadows;
pub mod debug_view;
pub mod sharpen;
pub mod taa;
pub mod tonemap;
pub mod vignette;

mod contact_shadows_pass;
mod debug_pass;
mod passes;

pub use contact_shadows_pass::ContactShadows;
pub use debug_pass::DebugView;
pub use passes::{Bloom, Sharpen, TemporalAntiAliasing, ToneMap, Vignette};

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
