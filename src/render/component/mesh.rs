use crate::asset::Handle;
use crate::render::asset::{MeshAsset, StandardMaterialAsset};
use crate::render::resources::material::MaterialHandle;
use crate::render::resources::mesh::MeshHandle;

use super::light::MAX_DIRECTIONAL_SHADOW_CASCADES;

pub const ALL_SHADOW_CASCADE_MASK: u8 = cascade_mask(MAX_DIRECTIONAL_SHADOW_CASCADES as u32);

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
    pub casts_shadows: bool,
    pub shadow_cascade_mask: u8,
}

impl MeshRenderer {
    #[inline]
    pub fn new(mesh: Handle<MeshAsset>, material: Handle<StandardMaterialAsset>) -> Self {
        Self {
            mesh,
            materials: vec![material],
            visible: true,
            layer_mask: u32::MAX,
            casts_shadows: true,
            shadow_cascade_mask: ALL_SHADOW_CASCADE_MASK,
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

    #[inline]
    pub fn casts_shadows(mut self, casts_shadows: bool) -> Self {
        self.casts_shadows = casts_shadows;
        self
    }

    #[inline]
    pub fn shadow_cascade_mask(mut self, shadow_cascade_mask: u8) -> Self {
        self.shadow_cascade_mask = shadow_cascade_mask;
        self
    }

    #[inline]
    pub fn shadow_lod_cascades(mut self, cascade_count: u32) -> Self {
        self.shadow_cascade_mask = cascade_mask(cascade_count);
        self
    }

    #[inline]
    pub const fn casts_shadows_in_cascade(&self, cascade_index: u32) -> bool {
        self.casts_shadows && (self.shadow_cascade_mask & cascade_bit(cascade_index)) != 0
    }
}

/// wgpu-internal mesh renderer for direct GPU handles.
///
/// Prefer [`MeshRenderer`] for new scene code.  This component exists for the
/// current wgpu `RenderRuntime` path and expert examples that build meshes and
/// materials directly on the wgpu backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WgpuMeshRenderer {
    pub mesh: MeshHandle,
    pub materials: Vec<MaterialHandle>,
    pub visible: bool,
    pub layer_mask: u32,
    pub casts_shadows: bool,
    pub shadow_cascade_mask: u8,
}

impl WgpuMeshRenderer {
    #[inline]
    pub fn new(mesh: MeshHandle, material: impl Into<MaterialHandle>) -> Self {
        Self {
            mesh,
            materials: vec![material.into()],
            visible: true,
            layer_mask: u32::MAX,
            casts_shadows: true,
            shadow_cascade_mask: ALL_SHADOW_CASCADE_MASK,
        }
    }

    #[inline]
    pub fn materials<H>(mut self, materials: impl IntoIterator<Item = H>) -> Self
    where
        H: Into<MaterialHandle>,
    {
        self.materials = materials.into_iter().map(Into::into).collect();
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

    #[inline]
    pub fn casts_shadows(mut self, casts_shadows: bool) -> Self {
        self.casts_shadows = casts_shadows;
        self
    }

    #[inline]
    pub fn shadow_cascade_mask(mut self, shadow_cascade_mask: u8) -> Self {
        self.shadow_cascade_mask = shadow_cascade_mask;
        self
    }

    #[inline]
    pub fn shadow_lod_cascades(mut self, cascade_count: u32) -> Self {
        self.shadow_cascade_mask = cascade_mask(cascade_count);
        self
    }

    #[inline]
    pub const fn casts_shadows_in_cascade(&self, cascade_index: u32) -> bool {
        self.casts_shadows && (self.shadow_cascade_mask & cascade_bit(cascade_index)) != 0
    }
}

#[inline]
const fn cascade_mask(cascade_count: u32) -> u8 {
    if cascade_count == 0 {
        0
    } else if cascade_count >= MAX_DIRECTIONAL_SHADOW_CASCADES as u32 {
        (1u8 << MAX_DIRECTIONAL_SHADOW_CASCADES) - 1
    } else {
        ((1u16 << cascade_count) - 1) as u8
    }
}

#[inline]
const fn cascade_bit(cascade_index: u32) -> u8 {
    if cascade_index >= 8 {
        0
    } else {
        1u8 << cascade_index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh_renderer() -> MeshRenderer {
        MeshRenderer::new(
            Handle::<MeshAsset>::new(crate::asset::AssetId::new()),
            Handle::<StandardMaterialAsset>::new(crate::asset::AssetId::new()),
        )
    }

    fn wgpu_mesh_renderer() -> WgpuMeshRenderer {
        WgpuMeshRenderer::new(
            MeshHandle::dynamic(1),
            MaterialHandle::new::<crate::render::StandardMaterial>(2, 0),
        )
    }

    #[test]
    fn shadow_lod_cascades_builds_near_cascade_mask() {
        assert_eq!(
            mesh_renderer().shadow_lod_cascades(0).shadow_cascade_mask,
            0
        );
        assert_eq!(
            mesh_renderer().shadow_lod_cascades(2).shadow_cascade_mask,
            0b0011
        );
        assert_eq!(
            mesh_renderer().shadow_lod_cascades(99).shadow_cascade_mask,
            ALL_SHADOW_CASCADE_MASK
        );
    }

    #[test]
    fn shadow_cascade_mask_controls_per_cascade_casting() {
        let mesh = wgpu_mesh_renderer().shadow_lod_cascades(2);

        assert!(mesh.casts_shadows_in_cascade(0));
        assert!(mesh.casts_shadows_in_cascade(1));
        assert!(!mesh.casts_shadows_in_cascade(2));
        assert!(!mesh.casts_shadows(false).casts_shadows_in_cascade(0));
    }
}
