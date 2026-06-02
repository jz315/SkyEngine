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

    fn mark_render_dirty(&mut self) {
        self.render.needs_render = true;
        self.invalidate_draw_cache();
    }

    fn mark_compose_dirty(&mut self) {
        self.render.needs_compose = true;
        self.mark_render_dirty();
    }

    pub(super) fn request_invalidation(&mut self, invalidation: Invalidation) {
        self.apply_invalidation_pass_flags(invalidation.pass_flags);
        self.record_committed_invalidation_trace(invalidation);
    }

    pub(super) fn runtime_invalidation_target(&self) -> NodeId {
        if self.tree.page_id.as_str().is_empty() {
            NodeId::new("runtime")
        } else {
            self.tree.page_id.clone()
        }
    }

    pub(super) fn record_committed_invalidation_trace(&mut self, invalidation: Invalidation) {
        self.invalidation.push(invalidation);
    }

    fn apply_invalidation_pass_flags(&mut self, flags: PassFlags) {
        if flags.request_compose_ui || flags.request_reconcile {
            self.mark_compose_dirty();
        } else if flags.request_layout {
            self.mark_full_redraw_dirty();
        } else if flags.request_draw
            || flags.request_layer
            || flags.request_focus
            || flags.request_hit
            || flags.request_platform_effects
        {
            self.mark_render_dirty();
        }
    }

    pub(super) fn clear_committed_invalidations(&mut self) {
        if !self.invalidation.is_empty() {
            self.invalidation.clear();
        }
    }

    pub(super) fn record_event_debug(&mut self, event: EventDebugRecord) {
        self.debug.events.push(event);
    }

    pub(super) fn clear_committed_event_debug_records(&mut self) {
        if !self.debug.events.is_empty() {
            self.debug.events.clear();
        }
    }

    fn mark_full_redraw_dirty(&mut self) {
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

    pub fn mark_rendered(&mut self) {
        self.render.needs_render = false;
        self.render.full_redraw = false;
    }

    #[cfg(test)]
    pub(crate) fn mark_full_redraw(&mut self) {
        self.request_invalidation(Invalidation::runtime(
            self.runtime_invalidation_target(),
            "mark_full_redraw",
            DirtyFlags::LAYOUT,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_invalidation_applies_pass_flags_but_committed_trace_does_not() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();

        runtime.record_committed_invalidation_trace(Invalidation::resource(
            NodeId::new("page.record"),
            "committed-trace",
        ));
        assert!(!runtime.needs_compose());
        assert!(!runtime.needs_render());

        runtime.request_invalidation(Invalidation::resource(
            NodeId::new("page.request"),
            "requested",
        ));
        assert!(runtime.needs_compose());
        assert!(runtime.needs_render());
        assert_eq!(runtime.invalidation.snapshot().len(), 2);
    }

    #[test]
    fn internal_full_redraw_request_records_runtime_invalidation() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();

        runtime.mark_full_redraw();

        assert!(runtime.needs_render());
        assert!(runtime.full_redraw());
        assert!(!runtime.needs_compose());
        let snapshot = runtime.diagnostics().current_snapshot();
        let invalidation = snapshot
            .invalidations
            .iter()
            .find(|record| record.source == InvalidationSource::Runtime("mark_full_redraw"))
            .expect("internal full-redraw request should be traceable");
        assert_eq!(invalidation.target.id(), "page");
        assert_eq!(invalidation.flags, DirtyFlags::LAYOUT);
        assert!(invalidation.pass_flags.request_layout);
        assert!(invalidation.pass_flags.request_hit);
        assert!(invalidation.pass_flags.request_draw);
        assert!(!invalidation.pass_flags.request_compose_ui);
    }

    #[test]
    fn runtime_invalidation_target_reuses_typed_page_identity() {
        let runtime = Runtime::new("page");
        assert_eq!(runtime.page_id(), "page");
        assert_eq!(runtime.runtime_invalidation_target(), NodeId::new("page"));

        let runtime = Runtime::new("");
        assert_eq!(runtime.page_id(), "");
        assert_eq!(
            runtime.runtime_invalidation_target(),
            NodeId::new("runtime")
        );
    }
}
