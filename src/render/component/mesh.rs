use crate::asset::Handle;
use crate::render::assets::{MeshAsset, StandardMaterialAsset};
use crate::render::resources::material::MaterialHandle;
use crate::render::resources::mesh::MeshHandle;

/// Backend-neutral mesh renderer component.
///
/// This is the scene-facing component future render backends consume.  It
/// references CPU-side semantic assets, leaving each backend to upload and cache
/// its own GPU resources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeshRenderer {
    pub mesh: Handle<MeshAsset>,
    pub materials: Vec<Handle<StandardMaterialAsset>>,
    pub visible: bool,
    pub layer_mask: u32,
}

impl MeshRenderer {
    #[inline]
    pub fn new(mesh: Handle<MeshAsset>, material: Handle<StandardMaterialAsset>) -> Self {
        Self {
            mesh,
            materials: vec![material],
            visible: true,
            layer_mask: u32::MAX,
        }
    }

    #[inline]
    pub fn materials(mut self, materials: impl Into<Vec<Handle<StandardMaterialAsset>>>) -> Self {
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

/// wgpu-internal mesh renderer for direct GPU handles.
///
/// Prefer [`MeshRenderer`] for new scene code.  This component exists for the
/// current wgpu `RenderComposer` path and expert examples that build meshes and
/// materials directly on the wgpu backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WgpuMeshRenderer {
    pub mesh: MeshHandle,
    pub materials: Vec<MaterialHandle>,
    pub visible: bool,
    pub layer_mask: u32,
}

impl WgpuMeshRenderer {
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
