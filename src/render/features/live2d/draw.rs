use crate::render::live2d::{Live2DPhaseRenderer, PreparedLive2DPhaseView};
use crate::render::phase::{
    DrawError, DrawFunction, Live2DDrawData, PhaseItem, PhasePayloadKind, StandaloneDrawContext,
};

#[derive(Default)]
pub(crate) struct DrawLive2D;

impl DrawLive2D {
    #[inline]
    pub(crate) fn new() -> Self {
        Self
    }
}

impl DrawFunction for DrawLive2D {
    #[inline]
    fn payload_kind(&self) -> PhasePayloadKind {
        PhasePayloadKind::of::<Live2DDrawData>()
    }

    fn draw(
        &mut self,
        _ctx: &mut crate::render::phase::DrawContext<'_, '_, '_>,
        _item: &PhaseItem,
    ) -> Result<(), DrawError> {
        unreachable!("DrawLive2D is a standalone draw function")
    }

    fn is_standalone(&self) -> bool {
        true
    }

    fn draw_standalone(
        &mut self,
        ctx: &mut StandaloneDrawContext<'_, '_, '_>,
        item: &PhaseItem,
    ) -> Result<(), DrawError> {
        let renderer =
            ctx.frame_payload::<Live2DPhaseRenderer>()
                .ok_or(DrawError::MissingFramePayload {
                    type_name: std::any::type_name::<Live2DPhaseRenderer>(),
                })?;
        let phase_view =
            ctx.view_payload::<PreparedLive2DPhaseView>()
                .ok_or(DrawError::MissingViewPayload {
                    type_name: std::any::type_name::<PreparedLive2DPhaseView>(),
                })?;
        let prepared = phase_view
            .frame(item.data::<Live2DDrawData>().frame_index())
            .ok_or(DrawError::MissingPreparedFrame {
                entity: item.entity,
            })?;

        let mut renderer = renderer
            .renderer
            .lock()
            .expect("Live2D renderer lock should not be poisoned");
        let (gpu, target) = ctx.gpu_and_target();
        renderer.execute_prepared_model_to_target(gpu, target, prepared);
        Ok(())
    }
}
