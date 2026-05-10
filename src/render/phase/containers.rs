use super::item::PhaseItem;
use super::sort_key::entity_sort_key;
use super::{DrawContext, DrawError, DrawFunctionRegistry};

#[derive(Default)]
pub struct TransparentPhase {
    items: Vec<PhaseItem>,
}

impl TransparentPhase {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn add_item(&mut self, item: PhaseItem) {
        self.items.push(item);
    }

    pub fn sort(&mut self) {
        self.items.sort_by(|lhs, rhs| {
            lhs.sort_key
                .cmp(&rhs.sort_key)
                .then_with(|| lhs.batch_key.cmp(&rhs.batch_key))
                .then_with(|| entity_sort_key(lhs.entity).cmp(&entity_sort_key(rhs.entity)))
        });
    }

    pub fn render(
        &self,
        draw_functions: &mut DrawFunctionRegistry,
        ctx: &mut DrawContext<'_, '_, '_>,
    ) -> Result<(), DrawError> {
        let mut cursor = 0usize;
        while cursor < self.items.len() {
            let draw_function_id = self.items[cursor].draw_function_id;
            let batch_key = self.items[cursor].batch_key;
            let mut batch_end = cursor + 1;
            while batch_end < self.items.len()
                && self.items[batch_end].draw_function_id == draw_function_id
                && self.items[batch_end].batch_key == batch_key
            {
                batch_end += 1;
            }
            draw_functions.draw_batch(draw_function_id, ctx, &self.items[cursor..batch_end])?;
            cursor = batch_end;
        }
        Ok(())
    }

    #[inline]
    pub fn items(&self) -> &[PhaseItem] {
        &self.items
    }

    #[inline]
    pub fn items_mut(&mut self) -> &mut [PhaseItem] {
        &mut self.items
    }

    #[inline]
    pub fn clear(&mut self) {
        self.items.clear();
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[derive(Default)]
pub struct OpaquePhase {
    items: Vec<PhaseItem>,
}

impl OpaquePhase {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn add_item(&mut self, item: PhaseItem) {
        self.items.push(item);
    }

    pub fn sort(&mut self) {
        self.items.sort_by(|lhs, rhs| {
            lhs.sort_key
                .cmp(&rhs.sort_key)
                .then_with(|| lhs.batch_key.cmp(&rhs.batch_key))
                .then_with(|| entity_sort_key(lhs.entity).cmp(&entity_sort_key(rhs.entity)))
        });
    }

    pub fn render(
        &self,
        draw_functions: &mut DrawFunctionRegistry,
        ctx: &mut DrawContext<'_, '_, '_>,
    ) -> Result<(), DrawError> {
        let mut cursor = 0usize;
        while cursor < self.items.len() {
            let draw_function_id = self.items[cursor].draw_function_id;
            let batch_key = self.items[cursor].batch_key;
            let mut batch_end = cursor + 1;
            while batch_end < self.items.len()
                && self.items[batch_end].draw_function_id == draw_function_id
                && self.items[batch_end].batch_key == batch_key
            {
                batch_end += 1;
            }
            draw_functions.draw_batch(draw_function_id, ctx, &self.items[cursor..batch_end])?;
            cursor = batch_end;
        }
        Ok(())
    }

    #[inline]
    pub fn items(&self) -> &[PhaseItem] {
        &self.items
    }

    #[inline]
    pub fn items_mut(&mut self) -> &mut [PhaseItem] {
        &mut self.items
    }

    #[inline]
    pub fn clear(&mut self) {
        self.items.clear();
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
