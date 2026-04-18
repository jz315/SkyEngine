use crate::render::resources::material::MaterialHandle;
use crate::render::resources::mesh::MeshHandle;

/// High-level mesh renderer component for handle-based mesh/material rendering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeshRenderer {
    pub mesh: MeshHandle,
    pub materials: Vec<MaterialHandle>,
    pub visible: bool,
    pub layer_mask: u32,
}

impl MeshRenderer {
    #[inline]
    pub fn new(mesh: MeshHandle, material: MaterialHandle) -> Self {
        Self {
            mesh,
            materials: vec![material],
            visible: true,
            layer_mask: u32::MAX,
        }
    }

    #[inline]
    pub fn materials(mut self, materials: impl Into<Vec<MaterialHandle>>) -> Self {
        self.materials = materials.into();
        self
    }

    #[inline]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    #[inline]
    pub fn layer_mask(mut self, layer_mask: u32) -> Self {
        self.layer_mask = layer_mask;
        self
    }
}
