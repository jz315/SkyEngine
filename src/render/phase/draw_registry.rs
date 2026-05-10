use std::any::TypeId;

use rustc_hash::FxHashMap;

use crate::ecs::EntityId;
use crate::render::view::ResolvedSceneTransforms;

use super::{
    DrawContext, DrawError, DrawFunctionId, PhaseItem, SceneMaterialPrepassContext,
    StandaloneDrawContext,
};

#[allow(private_interfaces)]
pub trait DrawFunction: Send {
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn draw(
        &mut self,
        ctx: &mut DrawContext<'_, '_, '_>,
        item: &PhaseItem,
    ) -> Result<(), DrawError>;

    #[inline]
    fn draw_batch(
        &mut self,
        ctx: &mut DrawContext<'_, '_, '_>,
        items: &[PhaseItem],
    ) -> Result<(), DrawError> {
        for item in items {
            self.draw(ctx, item)?;
        }
        Ok(())
    }

    #[inline]
    fn assign_model_matrix(
        &mut self,
        _item: &mut PhaseItem,
        _transforms: &ResolvedSceneTransforms,
        _entity_to_slot: &mut FxHashMap<EntityId, u32>,
        _model_matrices: &mut Vec<[f32; 16]>,
    ) {
    }

    #[inline]
    fn is_standalone(&self) -> bool {
        false
    }

    #[inline]
    fn draw_standalone(
        &mut self,
        _ctx: &mut StandaloneDrawContext<'_, '_, '_>,
        _item: &PhaseItem,
    ) -> Result<(), DrawError> {
        unreachable!("draw_standalone called for non-standalone draw function")
    }

    #[inline]
    fn draw_call_count(&self, items: &[PhaseItem]) -> usize {
        items.len()
    }

    #[inline]
    fn material_type_id(&self) -> Option<TypeId> {
        None
    }

    #[inline]
    fn supports_scene_material_prepass(&self) -> bool {
        false
    }

    #[inline]
    fn draw_scene_material_prepass_batch(
        &mut self,
        _ctx: &mut SceneMaterialPrepassContext<'_, '_>,
        _items: &[PhaseItem],
    ) -> Result<(), DrawError> {
        Ok(())
    }
}

#[derive(Default)]
pub struct DrawFunctionRegistry {
    functions: Vec<Box<dyn DrawFunction>>,
}

impl DrawFunctionRegistry {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<D: DrawFunction + 'static>(&mut self, draw_function: D) -> DrawFunctionId {
        let id = DrawFunctionId::from_raw(self.functions.len());
        self.functions.push(Box::new(draw_function));
        id
    }

    pub fn register_boxed(&mut self, draw_function: Box<dyn DrawFunction>) -> DrawFunctionId {
        let id = DrawFunctionId::from_raw(self.functions.len());
        self.functions.push(draw_function);
        id
    }

    pub fn draw(
        &mut self,
        id: DrawFunctionId,
        ctx: &mut DrawContext<'_, '_, '_>,
        item: &PhaseItem,
    ) -> Result<(), DrawError> {
        let Some(function) = self.functions.get_mut(id.index()) else {
            return Err(DrawError::MissingDrawFunction { id });
        };
        function.draw(ctx, item)
    }

    pub fn draw_batch(
        &mut self,
        id: DrawFunctionId,
        ctx: &mut DrawContext<'_, '_, '_>,
        items: &[PhaseItem],
    ) -> Result<(), DrawError> {
        let Some(function) = self.functions.get_mut(id.index()) else {
            return Err(DrawError::MissingDrawFunction { id });
        };
        function.draw_batch(ctx, items)
    }

    pub fn draw_standalone(
        &mut self,
        id: DrawFunctionId,
        ctx: &mut StandaloneDrawContext<'_, '_, '_>,
        item: &PhaseItem,
    ) -> Result<(), DrawError> {
        let Some(function) = self.functions.get_mut(id.index()) else {
            return Err(DrawError::MissingDrawFunction { id });
        };
        function.draw_standalone(ctx, item)
    }

    pub fn is_standalone(&self, id: DrawFunctionId) -> bool {
        self.functions
            .get(id.index())
            .is_some_and(|function| function.is_standalone())
    }

    pub fn draw_call_count(&self, id: DrawFunctionId, items: &[PhaseItem]) -> usize {
        self.functions
            .get(id.index())
            .map_or(items.len(), |function| function.draw_call_count(items))
    }

    pub fn material_type_id(&self, id: DrawFunctionId) -> Option<TypeId> {
        self.functions
            .get(id.index())
            .and_then(|function| function.material_type_id())
    }

    pub fn supports_scene_material_prepass(&self, id: DrawFunctionId) -> bool {
        self.functions
            .get(id.index())
            .is_some_and(|function| function.supports_scene_material_prepass())
    }

    #[allow(private_interfaces)]
    pub fn draw_scene_material_prepass_batch(
        &mut self,
        id: DrawFunctionId,
        ctx: &mut SceneMaterialPrepassContext<'_, '_>,
        items: &[PhaseItem],
    ) -> Result<(), DrawError> {
        let Some(function) = self.functions.get_mut(id.index()) else {
            return Err(DrawError::MissingDrawFunction { id });
        };
        function.draw_scene_material_prepass_batch(ctx, items)
    }

    pub fn assign_model_matrices(
        &mut self,
        items: &mut [PhaseItem],
        transforms: &ResolvedSceneTransforms,
        entity_to_slot: &mut FxHashMap<EntityId, u32>,
        model_matrices: &mut Vec<[f32; 16]>,
    ) {
        for item in items {
            let Some(function) = self.functions.get_mut(item.draw_function_id.index()) else {
                continue;
            };
            function.assign_model_matrix(item, transforms, entity_to_slot, model_matrices);
        }
    }
}
