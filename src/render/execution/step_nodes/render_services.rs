use super::*;

pub(crate) trait RuntimeRenderServices {
    fn draw_functions(&self) -> &DrawFunctionRegistry;
    fn split_phase_services(
        &mut self,
    ) -> (
        &mut DrawFunctionRegistry,
        &mut MaterialRegistry,
        &MeshRegistry,
        &Texture,
    );
}

pub(crate) struct RenderServices<'a> {
    draw_functions: &'a mut DrawFunctionRegistry,
    materials: &'a mut MaterialRegistry,
    mesh_registry: &'a MeshRegistry,
    fallback_texture: &'a Texture,
}

impl<'a> RenderServices<'a> {
    pub(crate) fn new(
        draw_functions: &'a mut DrawFunctionRegistry,
        materials: &'a mut MaterialRegistry,
        mesh_registry: &'a MeshRegistry,
        fallback_texture: &'a Texture,
    ) -> Self {
        Self {
            draw_functions,
            materials,
            mesh_registry,
            fallback_texture,
        }
    }
}

impl RuntimeRenderServices for RenderServices<'_> {
    #[inline]
    fn draw_functions(&self) -> &DrawFunctionRegistry {
        self.draw_functions
    }

    #[inline]
    fn split_phase_services(
        &mut self,
    ) -> (
        &mut DrawFunctionRegistry,
        &mut MaterialRegistry,
        &MeshRegistry,
        &Texture,
    ) {
        (
            self.draw_functions,
            self.materials,
            self.mesh_registry,
            self.fallback_texture,
        )
    }
}
