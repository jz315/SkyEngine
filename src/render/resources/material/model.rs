use crate::render::resources::mesh::VertexLayout;

use super::{
    MaterialError, MaterialInterface, MaterialPassSet, MaterialPrepareContext, MaterialRenderState,
    PreparedMaterial, SceneBindingDesc, SceneResourceKind, ShaderSource, ShaderVariantKey,
};

/// Context used when selecting a shader variant.
#[derive(Debug, Clone, Copy)]
pub struct MaterialVariantContext<'a> {
    pub interface: &'a MaterialInterface,
}

/// Static type-level definition of a material family.
pub trait MaterialModel: Send + Sync + 'static {
    type Data: Clone + Send + Sync + 'static;

    fn interface() -> MaterialInterface;

    fn variant(_data: &Self::Data, _ctx: &MaterialVariantContext<'_>) -> ShaderVariantKey {
        ShaderVariantKey::default()
    }

    fn prepare(
        data: &Self::Data,
        ctx: &mut MaterialPrepareContext<'_>,
    ) -> Result<PreparedMaterial, MaterialError>;

    #[inline]
    fn shader_source(data: &Self::Data) -> ShaderSource {
        let _ = data;
        Self::interface().shader.source.clone()
    }

    #[inline]
    fn vertex_layout(data: &Self::Data) -> VertexLayout {
        let _ = data;
        Self::interface().vertex.clone()
    }

    #[inline]
    fn render_state(data: &Self::Data) -> MaterialRenderState {
        let _ = data;
        Self::interface().render_state
    }

    #[inline]
    fn passes(data: &Self::Data) -> MaterialPassSet {
        let _ = data;
        Self::interface().passes
    }

    #[inline]
    fn vertex_entry(_data: &Self::Data) -> &'static str {
        Self::interface().shader.vertex_entry
    }

    #[inline]
    fn fragment_entry(_data: &Self::Data) -> &'static str {
        Self::interface().shader.fragment_entry
    }

    #[inline]
    fn scene_bindings(data: &Self::Data) -> Vec<SceneBindingDesc> {
        let interface = Self::interface();
        let mut bindings = Vec::new();
        if interface.scene.contains(SceneResourceKind::Shadows) {
            bindings.push(SceneBindingDesc::shadow_view(3));
        }
        if interface.scene.contains(SceneResourceKind::Gi) {
            bindings.push(SceneBindingDesc::global_illumination(2));
        }
        let _ = data;
        bindings
    }

    #[inline]
    fn scene_prepass_shader_source(data: &Self::Data) -> Option<ShaderSource> {
        let _ = data;
        None
    }

    #[inline]
    fn scene_prepass_vertex_layout(data: &Self::Data) -> VertexLayout {
        Self::vertex_layout(data)
    }

    #[inline]
    fn scene_prepass_vertex_entry(_data: &Self::Data) -> &'static str {
        "vs_main"
    }

    #[inline]
    fn scene_prepass_fragment_entry(_data: &Self::Data) -> &'static str {
        "fs_main"
    }
}
