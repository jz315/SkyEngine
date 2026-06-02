use super::*;

impl Runtime {
    pub fn text_system_mut(&mut self) -> &mut dyn TextSystem {
        self.resources.text_system.as_mut()
    }

    pub fn register_font(&mut self, font: &FontRef, bytes: &[u8]) {
        self.resources.text_system.register_font(font, bytes);
        self.request_invalidation(Invalidation::resource(
            self.runtime_invalidation_target(),
            format!("font:{font:?}"),
        ));
    }

    pub fn register_skin(&mut self, skin: NeoSkin) {
        self.resources.skins.register(skin);
        self.request_invalidation(Invalidation::resource(
            self.runtime_invalidation_target(),
            "skin",
        ));
    }

    pub fn skins(&self) -> &SkinRegistry {
        &self.resources.skins
    }

    pub fn skins_mut(&mut self) -> &mut SkinRegistry {
        self.request_invalidation(Invalidation::resource(
            self.runtime_invalidation_target(),
            "skins_mut",
        ));
        &mut self.resources.skins
    }

    /// Record host or renderer resource readiness with precise dirty intent.
    pub fn request_resource_dirty(&mut self, dirty: ResourceDirty) {
        self.request_invalidation(Invalidation::resource_with_flags(
            self.runtime_invalidation_target(),
            dirty.source_id().clone(),
            dirty.flags(),
        ));
    }

    pub(crate) fn resolve_image_ref(&self, image: &crate::ImageRef) -> crate::ImageRef {
        self.resources.skins.resolve_image(image)
    }

    pub(crate) fn resolve_font_ref(&self, font: &FontRef) -> FontRef {
        self.resources.skins.resolve_font(font)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_mutations_emit_typed_resource_invalidations() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();

        runtime.register_font(&FontRef::DefaultText, &[]);
        runtime.register_skin(NeoSkin::new("theme"));
        runtime
            .skins_mut()
            .insert_font("body", FontRef::DefaultText);

        assert!(runtime.needs_compose());
        assert!(runtime.needs_render());
        let snapshot = runtime.diagnostics().current_snapshot();
        let sources = ["font:DefaultText", "skin", "skins_mut"];
        for source in sources {
            let invalidation = snapshot
                .invalidations
                .iter()
                .find(|record| {
                    record.source == InvalidationSource::Resource(ResourceDirtySource::new(source))
                })
                .unwrap_or_else(|| panic!("missing resource invalidation for {source}"));
            assert_eq!(invalidation.target.id(), "page");
            assert_eq!(invalidation.target.kind(), "element");
            assert!(matches!(
                &invalidation.target,
                InvalidationTarget::Element(node) if node.as_str() == "page"
            ));
            assert!(invalidation.pass_flags.request_compose_ui);
            assert!(invalidation.pass_flags.request_layout);
            assert!(invalidation.pass_flags.request_hit);
            assert!(invalidation.pass_flags.request_draw);
        }
    }

    #[test]
    fn renderer_resource_redraw_uses_draw_only_resource_invalidation() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();

        let dirty = ResourceDirty::draw(RendererResourceDirty::PendingImages);
        assert_eq!(dirty.source(), "renderer:pending_images");
        assert_eq!(dirty.source_id().as_str(), "renderer:pending_images");
        assert_eq!(
            dirty.source_id().renderer_kind(),
            Some(RendererResourceDirty::PendingImages)
        );
        runtime.request_resource_dirty(dirty);

        assert!(runtime.needs_render());
        assert!(!runtime.needs_compose());
        assert!(!runtime.full_redraw());
        let snapshot = runtime.diagnostics().current_snapshot();
        let invalidation = snapshot
            .invalidations
            .iter()
            .find(|record| {
                record.source
                    == InvalidationSource::Resource(ResourceDirtySource::from(
                        RendererResourceDirty::PendingImages,
                    ))
            })
            .expect("pending renderer image should be traceable as resource dirty");
        assert_eq!(invalidation.flags, DirtyFlags::DRAW);
        assert!(invalidation.pass_flags.request_draw);
        assert!(!invalidation.pass_flags.request_compose_ui);
        assert!(!invalidation.pass_flags.request_layout);
        assert_eq!(invalidation.target.kind(), "element");
    }

    #[test]
    fn renderer_resource_layout_dirty_requests_layout_passes() {
        let mut runtime = Runtime::new("page");
        runtime.mark_rendered();

        let dirty = ResourceDirty::layout(RendererResourceDirty::ReadyFonts);
        assert_eq!(dirty.source(), "renderer:ready_fonts");
        assert_eq!(
            dirty.source_id().renderer_kind(),
            Some(RendererResourceDirty::ReadyFonts)
        );
        runtime.request_resource_dirty(dirty);

        assert!(runtime.needs_render());
        assert!(runtime.needs_compose());
        assert!(!runtime.full_redraw());
        let snapshot = runtime.diagnostics().current_snapshot();
        let invalidation = snapshot
            .invalidations
            .iter()
            .find(|record| {
                record.source
                    == InvalidationSource::Resource(ResourceDirtySource::from(
                        RendererResourceDirty::ReadyFonts,
                    ))
            })
            .expect("ready renderer font should be traceable as resource dirty");
        assert_eq!(
            invalidation.flags,
            DirtyFlags::COMPOSE | DirtyFlags::LAYOUT | DirtyFlags::DRAW
        );
        assert!(invalidation.pass_flags.request_compose_ui);
        assert!(invalidation.pass_flags.request_reconcile);
        assert!(invalidation.pass_flags.request_layout);
        assert!(invalidation.pass_flags.request_hit);
        assert!(invalidation.pass_flags.request_draw);
        assert_eq!(invalidation.target.kind(), "element");
    }
}
