use std::hash::{Hash, Hasher};

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

    fn pipeline_key(data: &Self::Data) -> u64 {
        let interface = Self::interface();
        let mut hasher = rustc_hash::FxHasher::default();
        Self::shader_source(data).hash(&mut hasher);
        Self::vertex_layout(data).hash(&mut hasher);
        Self::render_state(data).hash(&mut hasher);
        Self::vertex_entry(data).hash(&mut hasher);
        Self::fragment_entry(data).hash(&mut hasher);
        Self::variant(
            data,
            &MaterialVariantContext {
                interface: &interface,
            },
        )
        .hash(&mut hasher);
        hasher.finish()
    }

    #[inline]
    fn is_transparent(data: &Self::Data) -> bool {
        Self::passes(data).is_transparent()
    }

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

    fn scene_prepass_pipeline_key(data: &Self::Data) -> Option<u64> {
        let shader = Self::scene_prepass_shader_source(data)?;
        let mut hasher = rustc_hash::FxHasher::default();
        shader.hash(&mut hasher);
        Self::scene_prepass_vertex_layout(data).hash(&mut hasher);
        Self::scene_prepass_vertex_entry(data).hash(&mut hasher);
        Self::scene_prepass_fragment_entry(data).hash(&mut hasher);
        Some(hasher.finish())
    }
}
