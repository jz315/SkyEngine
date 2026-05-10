use std::sync::Arc;

use crate::asset::{AssetConfig, AssetServer, Handle, TextureAsset};
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

    /// Return the shared asset server used by render assets, creating the
    /// default runtime server if the application did not install one.
    pub fn asset_server(&mut self) -> AssetServer {
        if let Some(server) = self.world.get_resource::<AssetServer>() {
            return server.clone();
        }

        let config = AssetConfig::default().with_background_loading(true);
        let server = match AssetServer::new(config.clone()) {
            Ok(server) => server,
            Err(error) => {
                eprintln!("[SkyEngine] Asset server initialization failed: {error}");
                AssetServer::with_empty_manifest(config)
            }
        };
        self.world.insert_resource(server.clone());
        server
    }

    /// Insert a runtime texture asset and return a stable backend-neutral handle.
    pub fn insert_texture(&mut self, texture: TextureAsset) -> Handle<TextureAsset> {
        self.asset_server().insert_runtime(texture)
    }

    /// Insert a runtime CPU mesh asset and return a stable backend-neutral handle.
    pub fn insert_mesh(&mut self, mesh: MeshAsset) -> Handle<MeshAsset> {
        self.asset_server().insert_runtime(mesh)
    }

    /// Insert a runtime standard material asset and return a stable handle.
    pub fn insert_standard_material(
        &mut self,
        material: StandardMaterialAsset,
    ) -> Handle<StandardMaterialAsset> {
        self.asset_server().insert_runtime(material)
    }

    /// Resolve an installed runtime mesh asset.
    pub fn mesh(&mut self, handle: Handle<MeshAsset>) -> Option<Arc<MeshAsset>> {
        self.asset_server().try_get(&handle)
    }

    /// Resolve an installed runtime texture asset.
    pub fn texture(&mut self, handle: Handle<TextureAsset>) -> Option<Arc<TextureAsset>> {
        self.asset_server().try_get(&handle)
    }

    /// Resolve an installed standard material asset.
    pub fn standard_material(
        &mut self,
        handle: Handle<StandardMaterialAsset>,
    ) -> Option<Arc<StandardMaterialAsset>> {
        self.asset_server().try_get(&handle)
    }
}
