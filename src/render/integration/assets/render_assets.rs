use std::sync::Arc;

use crate::asset::{Assets, Handle, TextureAsset};
use crate::ecs::World;

use super::{MeshAsset, StandardMaterialAsset};

/// Per-frame access point for backend-neutral render assets.
pub struct RenderAssets<'a> {
    world: &'a mut World,
}

impl<'a> RenderAssets<'a> {
    #[inline]
    pub(crate) fn new(world: &'a mut World) -> Self {
        Self { world }
    }

    /// Return the shared asset facade used by render assets.
    ///
    /// App installs this before setup/update. Tests and manual worlds should
    /// insert `Assets` explicitly so asset ownership stays visible.
    pub fn assets(&mut self) -> Assets {
        if let Some(assets) = self.world.get_resource::<Assets>() {
            return assets.clone();
        }
        panic!("RenderAssets requires an Assets resource")
    }

    /// Insert a runtime texture asset and return a stable backend-neutral handle.
    pub fn insert_texture(&mut self, texture: TextureAsset) -> Handle<TextureAsset> {
        self.assets().insert_runtime(texture)
    }

    /// Insert a runtime CPU mesh asset and return a stable backend-neutral handle.
    pub fn insert_mesh(&mut self, mesh: MeshAsset) -> Handle<MeshAsset> {
        self.assets().insert_runtime(mesh)
    }

    /// Insert a runtime standard material asset and return a stable handle.
    pub fn insert_standard_material(
        &mut self,
        material: StandardMaterialAsset,
    ) -> Handle<StandardMaterialAsset> {
        self.assets().insert_runtime(material)
    }

    /// Resolve an installed runtime mesh asset.
    pub fn mesh(&mut self, handle: Handle<MeshAsset>) -> Option<Arc<MeshAsset>> {
        self.assets().try_get(&handle)
    }

    /// Resolve an installed runtime texture asset.
    pub fn texture(&mut self, handle: Handle<TextureAsset>) -> Option<Arc<TextureAsset>> {
        self.assets().try_get(&handle)
    }

    /// Resolve an installed standard material asset.
    pub fn standard_material(
        &mut self,
        handle: Handle<StandardMaterialAsset>,
    ) -> Option<Arc<StandardMaterialAsset>> {
        self.assets().try_get(&handle)
    }
}
