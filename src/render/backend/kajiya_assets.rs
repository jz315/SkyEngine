use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    fs::File,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

use rustc_hash::{FxHashMap, FxHashSet};
use turbosloth::*;

use crate::asset::{AssetId, AssetServer, Handle};
use crate::ecs::EntityId;
use crate::render::assets::{
    MeshAsset, MeshIndexData, MeshVertexAttribute, MeshVertexFormat, MeshVertexSemantic,
    StandardMaterialAsset,
};

use super::{SceneMeshInstance, SceneSnapshot};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct KajiyaMeshKey {
    mesh: AssetId,
    materials: Vec<AssetId>,
}

impl KajiyaMeshKey {
    fn from_instance(instance: &SceneMeshInstance) -> Self {
        Self {
            mesh: instance.mesh,
            materials: instance.materials.clone(),
        }
    }

    fn cache_name(&self) -> String {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        format!("sky-{:016x}.mesh", hasher.finish())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct KajiyaAssetSyncStats {
    pub(crate) resident_meshes: usize,
    pub(crate) resident_instances: usize,
    pub(crate) uploaded_meshes: usize,
    pub(crate) removed_instances: usize,
    pub(crate) topology_changed: bool,
}

struct CachedMesh {
    handle: ::kajiya::world_renderer::MeshHandle,
}

struct CachedInstance {
    key: KajiyaMeshKey,
    handle: ::kajiya::world_renderer::InstanceHandle,
}

pub(crate) struct KajiyaRenderAssetCache {
    mesh_cache: FxHashMap<KajiyaMeshKey, CachedMesh>,
    instance_cache: FxHashMap<EntityId, CachedInstance>,
    reported_failures: FxHashSet<KajiyaMeshKey>,
    lazy_cache: Arc<turbosloth::LazyCache>,
    cache_dir: PathBuf,
}

impl KajiyaRenderAssetCache {
    pub(crate) fn new(cache_dir: PathBuf) -> Self {
        Self {
            mesh_cache: FxHashMap::default(),
            instance_cache: FxHashMap::default(),
            reported_failures: FxHashSet::default(),
            lazy_cache: LazyCache::create(),
            cache_dir,
        }
    }

    pub(crate) fn sync_snapshot(
        &mut self,
        world_renderer: &mut ::kajiya::world_renderer::WorldRenderer,
        assets: Option<&AssetServer>,
        snapshot: &SceneSnapshot,
    ) -> KajiyaAssetSyncStats {
        let Some(assets) = assets else {
            if snapshot.stats().mesh_instances != 0 {
                eprintln!(
                    "[SkyEngine] Kajiya renderer cannot sync MeshAsset handles without an AssetServer resource"
                );
            }
            return KajiyaAssetSyncStats {
                resident_meshes: self.mesh_cache.len(),
                resident_instances: self.instance_cache.len(),
                ..KajiyaAssetSyncStats::default()
            };
        };

        let mut stats = KajiyaAssetSyncStats::default();
        let mut live_entities = FxHashSet::default();

        for (entity, instance) in snapshot.mesh_instances() {
            live_entities.insert(entity);
            let key = KajiyaMeshKey::from_instance(instance);
            let mesh_count_before = self.mesh_cache.len();
            let mesh_handle = match self.sync_mesh(world_renderer, assets, &key) {
                Ok(handle) => handle,
                Err(error) => {
                    self.report_sync_failure(&key, error);
                    if let Some(old) = self.instance_cache.remove(&entity) {
                        world_renderer.remove_instance(old.handle);
                        stats.removed_instances += 1;
                        stats.topology_changed = true;
                    }
                    continue;
                }
            };
            if self.mesh_cache.len() > mesh_count_before {
                stats.uploaded_meshes += 1;
            }

            self.reported_failures.remove(&key);
            let transform = kajiya_affine3(instance.transform);
            match self.instance_cache.get_mut(&entity) {
                Some(cached) if cached.key == key => {
                    world_renderer.set_instance_transform(cached.handle, transform);
                }
                Some(cached) => {
                    world_renderer.remove_instance(cached.handle);
                    let handle = world_renderer.add_instance(mesh_handle, transform);
                    *cached = CachedInstance { key, handle };
                    stats.removed_instances += 1;
                    stats.topology_changed = true;
                }
                None => {
                    let handle = world_renderer.add_instance(mesh_handle, transform);
                    self.instance_cache
                        .insert(entity, CachedInstance { key, handle });
                    stats.topology_changed = true;
                }
            }
        }

        let stale_entities = self
            .instance_cache
            .keys()
            .copied()
            .filter(|entity| !live_entities.contains(entity))
            .collect::<Vec<_>>();
        for entity in stale_entities {
            if let Some(cached) = self.instance_cache.remove(&entity) {
                world_renderer.remove_instance(cached.handle);
                stats.removed_instances += 1;
                stats.topology_changed = true;
            }
        }

        stats.resident_meshes = self.mesh_cache.len();
        stats.resident_instances = self.instance_cache.len();
        stats
    }

    fn sync_mesh(
        &mut self,
        world_renderer: &mut ::kajiya::world_renderer::WorldRenderer,
        assets: &AssetServer,
        key: &KajiyaMeshKey,
    ) -> Result<::kajiya::world_renderer::MeshHandle, String> {
        if let Some(cached) = self.mesh_cache.get(key) {
            return Ok(cached.handle);
        }

        let mesh = assets
            .try_get(&Handle::<MeshAsset>::new(key.mesh))
            .ok_or_else(|| format!("MeshAsset `{}` is not installed", key.mesh))?;
        let triangle_mesh = build_kajiya_triangle_mesh(mesh.as_ref(), assets, &key.materials)?;
        let packed = ::kajiya::asset::mesh::pack_triangle_mesh(&triangle_mesh);
        let mesh_path = self.bake_packed_mesh(key, &packed)?;
        let handle = world_renderer
            .add_baked_mesh(mesh_path, ::kajiya::world_renderer::AddMeshOptions::new())
            .map_err(|error| format!("failed to upload Kajiya mesh `{}`: {error:?}", key.mesh))?;

        self.mesh_cache.insert(key.clone(), CachedMesh { handle });
        Ok(handle)
    }

    fn bake_packed_mesh(
        &self,
        key: &KajiyaMeshKey,
        packed: &::kajiya::asset::mesh::PackedTriMesh::Proto,
    ) -> Result<PathBuf, String> {
        std::fs::create_dir_all(&self.cache_dir)
            .map_err(|error| format!("failed to create Kajiya cache dir: {error}"))?;

        let unique_images = packed
            .maps
            .iter()
            .cloned()
            .collect::<HashSet<::turbosloth::Lazy<::kajiya::asset::mesh::GpuImage::Proto>>>();
        for image in unique_images {
            let image_path = self
                .cache_dir
                .join(format!("{:8.8x}.image", image.identity()));
            if image_path.exists() {
                continue;
            }
            let loaded = pollster::block_on(image.eval(&self.lazy_cache))
                .map_err(|error| format!("failed to build Kajiya image asset: {error:?}"))?;
            let mut file = match File::create(&image_path) {
                Ok(file) => file,
                Err(_error) if image_path.exists() => continue,
                Err(error) => {
                    return Err(format!(
                        "failed to write `{}`: {error}",
                        image_path.display()
                    ));
                }
            };
            loaded.flatten_into(&mut file);
        }

        let mesh_path = self.cache_dir.join(key.cache_name());
        if mesh_path.exists() {
            return Ok(mesh_path);
        }

        let mut file = File::create(&mesh_path)
            .map_err(|error| format!("failed to write `{}`: {error}", mesh_path.display()))?;
        packed.flatten_into(&mut file);
        Ok(mesh_path)
    }

    fn report_sync_failure(&mut self, key: &KajiyaMeshKey, error: String) {
        if self.reported_failures.insert(key.clone()) {
            eprintln!("[SkyEngine] Kajiya asset sync skipped a mesh: {error}");
        }
    }
}

fn build_kajiya_triangle_mesh(
    mesh: &MeshAsset,
    assets: &AssetServer,
    material_ids: &[AssetId],
) -> Result<::kajiya::asset::mesh::TriangleMesh, String> {
    let position = required_attribute(mesh, MeshVertexSemantic::Position)?;
    let normal = optional_attribute(mesh, MeshVertexSemantic::Normal);
    let tangent = optional_attribute(mesh, MeshVertexSemantic::Tangent);
    let uv = optional_attribute(mesh, MeshVertexSemantic::UV0);
    let color = optional_attribute(mesh, MeshVertexSemantic::Color);
    let indices = mesh_indices(mesh)?;
    if indices.len() % 3 != 0 {
        return Err(format!(
            "MeshAsset `{}` has {} indices; Kajiya v1 only accepts triangle lists",
            mesh.label(),
            indices.len()
        ));
    }

    let vertex_count = mesh.vertex_count() as usize;
    let mut material_slots = material_ids
        .iter()
        .map(|id| assets.try_get(&Handle::<StandardMaterialAsset>::new(*id)))
        .collect::<Vec<_>>();
    if material_slots.is_empty() {
        material_slots.push(None);
    }

    let mut out = ::kajiya::asset::mesh::TriangleMesh {
        positions: Vec::with_capacity(vertex_count),
        normals: Vec::with_capacity(vertex_count),
        colors: Vec::with_capacity(vertex_count),
        uvs: Vec::with_capacity(vertex_count),
        tangents: Vec::with_capacity(vertex_count),
        material_ids: vec![0; vertex_count],
        indices,
        materials: Vec::with_capacity(material_slots.len()),
        maps: Vec::with_capacity(material_slots.len() * 4),
        images: Vec::new(),
    };

    for vertex in 0..vertex_count {
        out.positions
            .push(read_vec3(mesh, vertex, position).ok_or_else(|| {
                format!(
                    "MeshAsset `{}` has an invalid Position attribute payload",
                    mesh.label()
                )
            })?);
        out.normals.push(
            normal
                .and_then(|attribute| read_vec3(mesh, vertex, attribute))
                .map(normalize_or_y)
                .unwrap_or([0.0, 1.0, 0.0]),
        );
        out.tangents.push(
            tangent
                .and_then(|attribute| read_vec4(mesh, vertex, attribute))
                .map(normalize_tangent_or_x)
                .unwrap_or([1.0, 0.0, 0.0, 1.0]),
        );
        out.uvs.push(
            uv.and_then(|attribute| read_vec2(mesh, vertex, attribute))
                .unwrap_or([0.0, 0.0]),
        );
        out.colors.push(
            color
                .and_then(|attribute| read_color(mesh, vertex, attribute))
                .unwrap_or([1.0, 1.0, 1.0, 1.0]),
        );
    }

    assign_material_ids(
        mesh,
        &mut out.material_ids,
        &out.indices,
        material_slots.len(),
    );
    for material in material_slots {
        push_material(&mut out, material.as_deref());
    }

    Ok(out)
}

fn required_attribute(
    mesh: &MeshAsset,
    semantic: MeshVertexSemantic,
) -> Result<MeshVertexAttribute, String> {
    optional_attribute(mesh, semantic).ok_or_else(|| {
        format!(
            "MeshAsset `{}` is missing required {:?} attribute for Kajiya",
            mesh.label(),
            semantic
        )
    })
}

fn optional_attribute(
    mesh: &MeshAsset,
    semantic: MeshVertexSemantic,
) -> Option<MeshVertexAttribute> {
    mesh.vertex_layout()
        .attributes()
        .iter()
        .copied()
        .find(|attribute| attribute.semantic == semantic)
}

fn mesh_indices(mesh: &MeshAsset) -> Result<Vec<u32>, String> {
    match mesh.indices() {
        Some(MeshIndexData::U16(indices)) => {
            Ok(indices.iter().map(|index| *index as u32).collect())
        }
        Some(MeshIndexData::U32(indices)) => Ok(indices.iter().copied().collect()),
        None => Ok((0..mesh.vertex_count()).collect()),
    }
}

fn assign_material_ids(
    mesh: &MeshAsset,
    material_ids: &mut [u32],
    indices: &[u32],
    material_len: usize,
) {
    if mesh.sub_meshes().is_empty() {
        return;
    }

    let max_material = material_len.saturating_sub(1) as u32;
    for sub_mesh in mesh.sub_meshes() {
        let material = sub_mesh.material_index.min(max_material);
        let start = sub_mesh.index_offset as usize;
        let end = start
            .saturating_add(sub_mesh.index_count as usize)
            .min(indices.len());
        for index in &indices[start..end] {
            let vertex = *index as i32 + sub_mesh.vertex_offset;
            if vertex >= 0 {
                if let Some(slot) = material_ids.get_mut(vertex as usize) {
                    *slot = material;
                }
            }
        }
    }
}

fn push_material(
    mesh: &mut ::kajiya::asset::mesh::TriangleMesh,
    material: Option<&StandardMaterialAsset>,
) {
    const DEFAULT_MAP_TRANSFORM: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let base = mesh.maps.len() as u32;
    mesh.maps
        .push(::kajiya::asset::mesh::MeshMaterialMap::Placeholder([
            127, 127, 255, 255,
        ]));
    mesh.maps
        .push(::kajiya::asset::mesh::MeshMaterialMap::Placeholder([
            255, 255, 127, 255,
        ]));
    mesh.maps
        .push(::kajiya::asset::mesh::MeshMaterialMap::Placeholder([
            255, 255, 255, 255,
        ]));
    mesh.maps
        .push(::kajiya::asset::mesh::MeshMaterialMap::Placeholder([
            255, 255, 255, 255,
        ]));

    let albedo = material.map_or([1.0, 1.0, 1.0, 1.0], |material| material.albedo.to_array());
    let emissive = material.map_or([0.0, 0.0, 0.0, 1.0], |material| {
        material.emissive.to_array()
    });
    let roughness = material.map_or(0.8, |material| material.roughness);
    let metallic = material.map_or(0.0, |material| material.metallic);

    mesh.materials.push(::kajiya::asset::mesh::MeshMaterial {
        base_color_mult: albedo,
        maps: [base, base + 1, base + 2, base + 3],
        roughness_mult: roughness,
        metalness_factor: metallic,
        emissive: [emissive[0], emissive[1], emissive[2]],
        flags: 0,
        map_transforms: [DEFAULT_MAP_TRANSFORM; 4],
    });
}

fn read_vec2(mesh: &MeshAsset, vertex: usize, attribute: MeshVertexAttribute) -> Option<[f32; 2]> {
    (attribute.format == MeshVertexFormat::Float32x2)
        .then(|| read_f32s::<2>(mesh, vertex, attribute.offset))
        .flatten()
}

fn read_vec3(mesh: &MeshAsset, vertex: usize, attribute: MeshVertexAttribute) -> Option<[f32; 3]> {
    (attribute.format == MeshVertexFormat::Float32x3)
        .then(|| read_f32s::<3>(mesh, vertex, attribute.offset))
        .flatten()
}

fn read_vec4(mesh: &MeshAsset, vertex: usize, attribute: MeshVertexAttribute) -> Option<[f32; 4]> {
    (attribute.format == MeshVertexFormat::Float32x4)
        .then(|| read_f32s::<4>(mesh, vertex, attribute.offset))
        .flatten()
}

fn read_color(mesh: &MeshAsset, vertex: usize, attribute: MeshVertexAttribute) -> Option<[f32; 4]> {
    match attribute.format {
        MeshVertexFormat::Float32x4 => read_f32s::<4>(mesh, vertex, attribute.offset),
        MeshVertexFormat::Unorm8x4 => {
            let bytes = read_bytes(mesh, vertex, attribute.offset, 4)?;
            Some([
                bytes[0] as f32 / 255.0,
                bytes[1] as f32 / 255.0,
                bytes[2] as f32 / 255.0,
                bytes[3] as f32 / 255.0,
            ])
        }
        _ => None,
    }
}

fn read_f32s<const N: usize>(mesh: &MeshAsset, vertex: usize, offset: u32) -> Option<[f32; N]> {
    let bytes = read_bytes(mesh, vertex, offset, N * std::mem::size_of::<f32>())?;
    let mut values = [0.0; N];
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        values[index] = f32::from_ne_bytes(chunk.try_into().ok()?);
    }
    Some(values)
}

fn read_bytes(mesh: &MeshAsset, vertex: usize, offset: u32, len: usize) -> Option<&[u8]> {
    let base = vertex
        .checked_mul(mesh.vertex_layout().stride() as usize)?
        .checked_add(offset as usize)?;
    mesh.vertex_bytes().get(base..base.checked_add(len)?)
}

fn normalize_or_y(value: [f32; 3]) -> [f32; 3] {
    let len = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if len <= f32::EPSILON {
        [0.0, 1.0, 0.0]
    } else {
        [value[0] / len, value[1] / len, value[2] / len]
    }
}

fn normalize_tangent_or_x(value: [f32; 4]) -> [f32; 4] {
    let len = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if len <= f32::EPSILON {
        [1.0, 0.0, 0.0, 1.0]
    } else {
        [value[0] / len, value[1] / len, value[2] / len, value[3]]
    }
}

fn kajiya_affine3(cols: [f32; 16]) -> ::kajiya::math::Affine3A {
    ::kajiya::math::Affine3A::from_mat4(::kajiya::math::Mat4::from_cols_array(&cols))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{AssetConfig, TextureAsset};
    use crate::render::assets::{MeshAssetDescriptor, MeshVertexLayout};
    use crate::render::view::Color;

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    #[test]
    fn converts_sky_mesh_and_material_to_kajiya_triangle_mesh() {
        let assets = AssetServer::with_empty_manifest(AssetConfig::default());
        let material = assets.insert_runtime(
            StandardMaterialAsset::new()
                .albedo(Color::rgb(0.25, 0.5, 0.75))
                .roughness(0.35)
                .metallic(0.1),
        );
        let _texture = assets.insert_runtime(TextureAsset::white_pixel());
        let mesh = MeshAsset::from_raw(MeshAssetDescriptor::new(
            bytemuck::cast_slice(&[
                Vertex {
                    position: [-1.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                },
                Vertex {
                    position: [1.0, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [1.0, 0.0],
                },
                Vertex {
                    position: [0.0, 1.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.5, 1.0],
                },
            ]),
            3,
            MeshVertexLayout::position_normal_uv(),
            "kajiya-conversion",
        ));

        let converted =
            build_kajiya_triangle_mesh(&mesh, &assets, &[material.id()]).expect("convert mesh");

        assert_eq!(converted.positions.len(), 3);
        assert_eq!(converted.indices, vec![0, 1, 2]);
        assert_eq!(converted.materials.len(), 1);
        assert_eq!(converted.maps.len(), 4);
        assert_eq!(
            converted.materials[0].base_color_mult,
            [0.25, 0.5, 0.75, 1.0]
        );
    }
}
