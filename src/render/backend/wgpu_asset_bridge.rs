use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::asset::{AssetId, Assets, Handle, TextureAsset};
use crate::gpu::GpuContext;
use crate::render::asset::{MeshAsset, StandardMaterialAsset};
use crate::render::gpu::Texture;
use crate::render::resources::material::{MaterialHandle, StandardMaterial};
use crate::render::resources::mesh::MeshHandle;
use crate::render::resources::texture_cache::SharedRenderAssetCache;
use crate::render::runtime::RenderRuntime;

#[derive(Default)]
pub(crate) struct WgpuRenderAssetCache {
    meshes: FxHashMap<AssetId, CachedWgpuMesh>,
    standard_materials: FxHashMap<AssetId, CachedWgpuStandardMaterial>,
}

struct CachedWgpuMesh {
    source: Arc<MeshAsset>,
    handle: MeshHandle,
}

struct CachedWgpuStandardMaterial {
    source: Arc<StandardMaterialAsset>,
    texture_keys: StandardMaterialTextureKeys,
    handle: MaterialHandle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct StandardMaterialTextureKeys {
    albedo: Option<usize>,
    normal: Option<usize>,
    emissive: Option<usize>,
}

impl WgpuRenderAssetCache {
    pub(crate) fn sync_mesh(
        &mut self,
        gpu: &GpuContext,
        render_runtime: &mut RenderRuntime,
        assets: &Assets,
        handle: Handle<MeshAsset>,
    ) -> Option<MeshHandle> {
        let source = assets.try_get(&handle)?;
        if let Some(cached) = self.meshes.get(&handle.id()) {
            if Arc::ptr_eq(&cached.source, &source) {
                return Some(cached.handle);
            }
        }

        if let Some(stale) = self.meshes.remove(&handle.id()) {
            let _ = render_runtime.remove_mesh(stale.handle);
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
        let gpu_handle = render_runtime.insert_mesh(mesh);
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
        render_runtime: &mut RenderRuntime,
        assets: &Assets,
        render_assets: &SharedRenderAssetCache,
        handle: Handle<StandardMaterialAsset>,
    ) -> Option<MaterialHandle> {
        let source = assets.try_get(&handle)?;
        render_runtime.register_material::<StandardMaterial>(gpu);
        let albedo_texture = source
            .albedo_texture
            .as_ref()
            .and_then(|texture| sync_texture(gpu, assets, render_assets, texture));
        let normal_texture = source
            .normal_texture
            .as_ref()
            .and_then(|texture| sync_texture(gpu, assets, render_assets, texture));
        let emissive_texture = source
            .emissive_texture
            .as_ref()
            .and_then(|texture| sync_texture(gpu, assets, render_assets, texture));
        let texture_keys =
            StandardMaterialTextureKeys::new(&albedo_texture, &normal_texture, &emissive_texture);

        if let Some(cached) = self.standard_materials.get(&handle.id()) {
            if Arc::ptr_eq(&cached.source, &source) && cached.texture_keys == texture_keys {
                return Some(cached.handle);
            }
        }

        if let Some(stale) = self.standard_materials.remove(&handle.id()) {
            let _ = render_runtime.remove_material_erased::<StandardMaterial>(stale.handle);
        }

        let material = StandardMaterial {
            albedo: source.albedo,
            albedo_texture,
            metallic: source.metallic,
            roughness: source.roughness,
            normal_texture,
            emissive: source.emissive,
            emissive_texture,
            alpha_mode: source.alpha_mode,
            alpha_cutoff: source.alpha_cutoff,
            receive_shadows: source.receive_shadows,
        };
        let material_handle = render_runtime
            .insert_material::<StandardMaterial>(material)
            .erased();
        self.standard_materials.insert(
            handle.id(),
            CachedWgpuStandardMaterial {
                source,
                texture_keys,
                handle: material_handle,
            },
        );
        Some(material_handle)
    }
}

impl StandardMaterialTextureKeys {
    fn new(albedo: &Option<Texture>, normal: &Option<Texture>, emissive: &Option<Texture>) -> Self {
        Self {
            albedo: texture_key(albedo),
            normal: texture_key(normal),
            emissive: texture_key(emissive),
        }
    }
}

fn texture_key(texture: &Option<Texture>) -> Option<usize> {
    texture
        .as_ref()
        .map(|texture| std::ptr::from_ref(texture.texture()) as usize)
}

fn sync_texture(
    gpu: &GpuContext,
    assets: &Assets,
    render_assets: &SharedRenderAssetCache,
    handle: &Handle<TextureAsset>,
) -> Option<crate::render::Texture> {
    render_assets.borrow_mut().texture(gpu, assets, handle)
}
