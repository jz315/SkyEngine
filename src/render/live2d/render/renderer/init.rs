use super::state::Live2DRenderer;
use super::*;

impl Live2DRenderer {
    pub fn new(ctx: &GpuContext) -> Self {
        Self::new_internal(ctx)
    }
}
