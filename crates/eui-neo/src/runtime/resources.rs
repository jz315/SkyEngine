use super::*;

impl Runtime {
    pub fn text_system_mut(&mut self) -> &mut dyn TextSystem {
        self.resources.text_system.as_mut()
    }

    pub fn register_font(&mut self, font: &FontRef, bytes: &[u8]) {
        self.resources.text_system.register_font(font, bytes);
        self.record_invalidation(Invalidation::resource(
            self.tree.page_id.clone(),
            format!("font:{font:?}"),
        ));
        self.mark_compose_dirty();
    }

    pub fn register_skin(&mut self, skin: NeoSkin) {
        self.resources.skins.register(skin);
        self.record_invalidation(Invalidation::resource(self.tree.page_id.clone(), "skin"));
        self.mark_compose_dirty();
    }

    pub fn skins(&self) -> &SkinRegistry {
        &self.resources.skins
    }

    pub fn skins_mut(&mut self) -> &mut SkinRegistry {
        self.record_invalidation(Invalidation::resource(
            self.tree.page_id.clone(),
            "skins_mut",
        ));
        self.mark_compose_dirty();
        &mut self.resources.skins
    }

    pub(crate) fn resolve_image_ref(&self, image: &crate::ImageRef) -> crate::ImageRef {
        self.resources.skins.resolve_image(image)
    }

    pub(crate) fn resolve_font_ref(&self, font: &FontRef) -> FontRef {
        self.resources.skins.resolve_font(font)
    }
}
