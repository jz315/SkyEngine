use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DrawListCacheKey {
    revision: u64,
}

impl Runtime {
    fn invalidate_draw_cache(&self) {
        self.render
            .draw_cache_revision
            .set(self.render.draw_cache_revision.get().wrapping_add(1));
        self.render.draw_cache.borrow_mut().invalidate();
    }

    pub(super) fn request_render(&mut self) {
        self.render.needs_render = true;
    }

    pub(super) fn mark_render_dirty(&mut self) {
        self.render.needs_render = true;
        self.invalidate_draw_cache();
    }

    pub(super) fn mark_compose_dirty(&mut self) {
        self.render.needs_compose = true;
        self.mark_render_dirty();
    }

    pub(super) fn record_invalidation(&mut self, invalidation: Invalidation) {
        self.invalidation.push(invalidation);
    }

    pub(super) fn clear_committed_invalidations(&mut self) {
        if !self.invalidation.is_empty() {
            self.invalidation.clear();
        }
    }

    pub(super) fn mark_full_redraw_dirty(&mut self) {
        self.render.full_redraw = true;
        self.mark_render_dirty();
    }

    pub fn draw_list(&self) -> crate::draw::UiDrawList {
        let mut cache = self.render.draw_cache.borrow_mut();
        cache.get_or_rebuild(
            DrawListCacheKey {
                revision: self.render.draw_cache_revision.get(),
            },
            |draw_list| {
                *draw_list = crate::draw::build_draw_list(self);
            },
        );
        cache.value().clone()
    }

    pub fn needs_render(&self) -> bool {
        self.render.needs_render
    }

    pub fn full_redraw(&self) -> bool {
        self.render.full_redraw
    }

    pub fn needs_compose(&self) -> bool {
        self.render.needs_compose
    }

    pub fn clear_needs_compose(&mut self) {
        self.render.needs_compose = false;
    }

    pub fn mark_rendered(&mut self) {
        self.render.needs_render = false;
        self.render.full_redraw = false;
    }

    pub fn mark_full_redraw(&mut self) {
        self.mark_full_redraw_dirty();
    }
}
