use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::asset::{AssetId, AssetServer, Handle, TextureAsset};
use crate::gpu::GpuContext;
use crate::render::assets::{MeshAsset, StandardMaterialAsset};
use crate::render::gpu::Texture;
use crate::render::resources::material::{MaterialHandle, StandardMaterial};
use crate::render::resources::mesh::MeshHandle;
use crate::render::runtime::RenderComposer;

#[derive(Default)]
pub(crate) struct WgpuRenderAssetCache {
    meshes: FxHashMap<AssetId, CachedWgpuMesh>,
    standard_materials: FxHashMap<AssetId, CachedWgpuStandardMaterial>,
    textures: FxHashMap<AssetId, CachedWgpuTexture>,
}

struct CachedWgpuMesh {
    source: Arc<MeshAsset>,
    handle: MeshHandle,
}

struct CachedWgpuStandardMaterial {
    source: Arc<StandardMaterialAsset>,
    handle: MaterialHandle,
}

struct CachedWgpuTexture {
    source: Arc<TextureAsset>,
    texture: Texture,
}

impl WgpuRenderAssetCache {
    pub(crate) fn sync_mesh(
        &mut self,
        gpu: &GpuContext,
        composer: &mut RenderComposer,
        assets: &AssetServer,
        handle: Handle<MeshAsset>,
    ) -> Option<MeshHandle> {
        let source = assets.try_get(&handle)?;
        if let Some(cached) = self.meshes.get(&handle.id()) {
            if Arc::ptr_eq(&cached.source, &source) {
                return Some(cached.handle);
            }
        }

        if let Some(stale) = self.meshes.remove(&handle.id()) {
            let _ = composer.remove_mesh(stale.handle);
        }

        let mesh = match source.to_wgpu_mesh(gpu) {
            Ok(mesh) => mesh,
            Err(error) => {
                eprintln!(
                    "[SkyEngine] Failed to upload MeshAsset `{}` to wgpu: {error}",
                    source.label()
                );
                return None;
            }
        };
        let gpu_handle = composer.insert_mesh(mesh);
        self.meshes.insert(
            handle.id(),
            CachedWgpuMesh {
                source,
                handle: gpu_handle,
            },
        );
        Some(gpu_handle)
    }

    pub(crate) fn sync_standard_material(
        &mut self,
        gpu: &GpuContext,
        composer: &mut RenderComposer,
        assets: &AssetServer,
        handle: Handle<StandardMaterialAsset>,
    ) -> Option<MaterialHandle> {
        let source = assets.try_get(&handle)?;
        composer.register_material::<StandardMaterial>(gpu);
        if let Some(cached) = self.standard_materials.get(&handle.id()) {
            if Arc::ptr_eq(&cached.source, &source) {
                return Some(cached.handle);
            }
        }

        if let Some(stale) = self.standard_materials.remove(&handle.id()) {
            let _ = composer
                .materials_mut::<StandardMaterial>()
                .remove(stale.handle);
        }

        let material = StandardMaterial {
            albedo: source.albedo,
            albedo_texture: source
                .albedo_texture
                .and_then(|texture| self.sync_texture(gpu, assets, texture)),
            metallic: source.metallic,
            roughness: source.roughness,
            normal_texture: source
                .normal_texture
                .and_then(|texture| self.sync_texture(gpu, assets, texture)),
            emissive: source.emissive,
            emissive_texture: source
                .emissive_texture
                .and_then(|texture| self.sync_texture(gpu, assets, texture)),
            alpha_mode: source.alpha_mode,
            alpha_cutoff: source.alpha_cutoff,
            receive_shadows: source.receive_shadows,
        };
        let material_handle = composer
            .materials_mut::<StandardMaterial>()
            .insert(material);
        self.standard_materials.insert(
            handle.id(),
            CachedWgpuStandardMaterial {
                source,
                handle: material_handle,
            },
        );
        Some(material_handle)
    }

    fn sync_texture(
        &mut self,
        gpu: &GpuContext,
        assets: &AssetServer,
        handle: Handle<TextureAsset>,
    ) -> Option<Texture> {
        let source = assets.try_get(&handle)?;
        if let Some(cached) = self.textures.get(&handle.id()) {
            if Arc::ptr_eq(&cached.source, &source) {
                return Some(cached.texture.clone());
            }
        }

        let format = match source.color_space() {
            crate::asset::TextureColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
            crate::asset::TextureColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        };
        let texture = Texture::from_rgba8_with_format(
            gpu,
            source.width(),
            source.height(),
            source.pixels(),
            format,
            "asset_standard_material_texture",
        );
        self.textures.insert(
            handle.id(),
            CachedWgpuTexture {
                source,
                texture: texture.clone(),
            },
        );
        Some(texture)
    }
}
