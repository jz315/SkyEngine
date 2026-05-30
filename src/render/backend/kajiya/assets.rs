use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    fs::File,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

use rustc_hash::{FxHashMap, FxHashSet};
use turbosloth::*;

use crate::asset::{
    Asset, AssetEvent, AssetEventCursor, AssetEventKind, AssetId, AssetState, Assets,
};
use crate::asset::{TextureAsset, TextureColorSpace};
use crate::ecs::EntityId;
use crate::render::asset::{
    MeshAsset, MeshIndexData, MeshVertexAttribute, MeshVertexFormat, MeshVertexSemantic,
    StandardMaterialAsset,
};

use super::super::{SceneMeshInstance, SceneSnapshot};
use super::error::KajiyaBackendError;

const KAJIYA_MESH_BAKE_VERSION: u64 = 2;

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

    fn cache_name_for_signature(&self, signature: KajiyaMeshContentSignature) -> String {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        signature.hash(&mut hasher);
        format!("sky-{:016x}.mesh", hasher.finish())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
struct KajiyaMeshContentSignature(u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct KajiyaAssetSyncStats {
    pub(crate) resident_meshes: usize,
    pub(crate) resident_instances: usize,
    pub(crate) uploaded_meshes: usize,
    pub(crate) removed_instances: usize,
    pub(crate) topology_changed: bool,
}

struct CachedMesh {
    sources: KajiyaMeshSources,
    handle: ::kajiya::world_renderer::MeshHandle,
}

struct CachedInstance {
    key: KajiyaMeshKey,
    mesh: ::kajiya::world_renderer::MeshHandle,
    handle: ::kajiya::world_renderer::InstanceHandle,
}

#[derive(Clone)]
struct KajiyaMeshSources {
    mesh: Arc<MeshAsset>,
    materials: Vec<KajiyaMaterialSources>,
}

#[derive(Clone)]
struct KajiyaMaterialSources {
    id: Option<AssetId>,
    material: Option<Arc<StandardMaterialAsset>>,
    albedo_texture_id: Option<AssetId>,
    albedo_texture: Option<Arc<TextureAsset>>,
    normal_texture_id: Option<AssetId>,
    normal_texture: Option<Arc<TextureAsset>>,
    emissive_texture_id: Option<AssetId>,
    emissive_texture: Option<Arc<TextureAsset>>,
}

impl KajiyaMeshSources {
    fn matches(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.mesh, &other.mesh)
            && self.materials.len() == other.materials.len()
            && self
                .materials
                .iter()
                .zip(&other.materials)
                .all(|(a, b)| a.matches(b))
    }

    fn signature(&self) -> KajiyaMeshContentSignature {
        let mut hasher = DefaultHasher::new();
        KAJIYA_MESH_BAKE_VERSION.hash(&mut hasher);
        hash_mesh_asset(&self.mesh, &mut hasher);
        for material in &self.materials {
            material.hash_content(&mut hasher);
        }
        KajiyaMeshContentSignature(hasher.finish())
    }
}

impl KajiyaMaterialSources {
    fn from_assets(
        assets: &Assets,
        id: AssetId,
        material: Option<Arc<StandardMaterialAsset>>,
    ) -> Self {
        let albedo_texture_id = material
            .as_ref()
            .and_then(|material| material.albedo_texture.as_ref())
            .map(|texture| texture.id());
        let albedo_texture = material
            .as_ref()
            .and_then(|material| material.albedo_texture.clone())
            .and_then(|texture| assets.try_get(&texture));
        let normal_texture_id = material
            .as_ref()
            .and_then(|material| material.normal_texture.as_ref())
            .map(|texture| texture.id());
        let normal_texture = material
            .as_ref()
            .and_then(|material| material.normal_texture.clone())
            .and_then(|texture| assets.try_get(&texture));
        let emissive_texture_id = material
            .as_ref()
            .and_then(|material| material.emissive_texture.as_ref())
            .map(|texture| texture.id());
        let emissive_texture = material
            .as_ref()
            .and_then(|material| material.emissive_texture.clone())
            .and_then(|texture| assets.try_get(&texture));

        Self {
            id: Some(id),
            material,
            albedo_texture_id,
            albedo_texture,
            normal_texture_id,
            normal_texture,
            emissive_texture_id,
            emissive_texture,
        }
    }

    fn default_placeholder() -> Self {
        Self {
            id: None,
            material: None,
            albedo_texture_id: None,
            albedo_texture: None,
            normal_texture_id: None,
            normal_texture: None,
            emissive_texture_id: None,
            emissive_texture: None,
        }
    }

    fn matches(&self, other: &Self) -> bool {
        self.id == other.id
            && option_arc_ptr_eq(&self.material, &other.material)
            && self.albedo_texture_id == other.albedo_texture_id
            && option_arc_ptr_eq(&self.albedo_texture, &other.albedo_texture)
            && self.normal_texture_id == other.normal_texture_id
            && option_arc_ptr_eq(&self.normal_texture, &other.normal_texture)
            && self.emissive_texture_id == other.emissive_texture_id
            && option_arc_ptr_eq(&self.emissive_texture, &other.emissive_texture)
    }

    fn references_texture(&self, id: AssetId) -> bool {
        self.albedo_texture_id == Some(id)
            || self.normal_texture_id == Some(id)
            || self.emissive_texture_id == Some(id)
    }

    fn hash_content(&self, hasher: &mut impl Hasher) {
        self.id.hash(hasher);
        match self.material.as_deref() {
            Some(material) => {
                true.hash(hasher);
                hash_f32_array(material.albedo.to_array(), hasher);
                material
                    .albedo_texture
                    .as_ref()
                    .map(|handle| handle.id())
                    .hash(hasher);
                material.albedo_sampler.hash(hasher);
                material.metallic.to_bits().hash(hasher);
                material.roughness.to_bits().hash(hasher);
                material
                    .normal_texture
                    .as_ref()
                    .map(|handle| handle.id())
                    .hash(hasher);
                material.normal_sampler.hash(hasher);
                hash_f32_array(material.emissive.to_array(), hasher);
                material
                    .emissive_texture
                    .as_ref()
                    .map(|handle| handle.id())
                    .hash(hasher);
                material.emissive_sampler.hash(hasher);
                material.alpha_mode.hash(hasher);
                material.alpha_cutoff.to_bits().hash(hasher);
            }
            None => false.hash(hasher),
        }
        hash_optional_texture(self.albedo_texture.as_deref(), hasher);
        hash_optional_texture(self.normal_texture.as_deref(), hasher);
        hash_optional_texture(self.emissive_texture.as_deref(), hasher);
    }
}

pub(crate) struct KajiyaRenderAssetCache {
    mesh_cache: FxHashMap<KajiyaMeshKey, CachedMesh>,
    instance_cache: FxHashMap<EntityId, CachedInstance>,
    reported_failures: FxHashSet<KajiyaMeshKey>,
    asset_event_cursor: Option<AssetEventCursor>,
    lazy_cache: Arc<turbosloth::LazyCache>,
    cache_dir: PathBuf,
}

impl KajiyaRenderAssetCache {
    pub(crate) fn new(cache_dir: PathBuf) -> Self {
        Self {
            mesh_cache: FxHashMap::default(),
            instance_cache: FxHashMap::default(),
            reported_failures: FxHashSet::default(),
            asset_event_cursor: None,
            lazy_cache: LazyCache::create(),
            cache_dir,
        }
    }

    pub(crate) fn sync_snapshot(
        &mut self,
        world_renderer: &mut ::kajiya::world_renderer::WorldRenderer,
        assets: Option<&Assets>,
        snapshot: &SceneSnapshot,
    ) -> KajiyaAssetSyncStats {
        let Some(assets) = assets else {
            if snapshot.stats().mesh_instances != 0 {
                eprintln!(
                    "[SkyEngine] Kajiya renderer cannot sync MeshAsset handles without an Assets resource"
                );
            }
            return KajiyaAssetSyncStats {
                resident_meshes: self.mesh_cache.len(),
                resident_instances: self.instance_cache.len(),
                ..KajiyaAssetSyncStats::default()
            };
        };

        let mut stats = KajiyaAssetSyncStats::default();
        self.consume_asset_events(world_renderer, assets, &mut stats);
        let mut live_entities = FxHashSet::default();

        for (entity, instance) in snapshot.mesh_instances() {
            live_entities.insert(entity);
            let key = KajiyaMeshKey::from_instance(instance);
            let (mesh_handle, uploaded_mesh) = match self.sync_mesh(world_renderer, assets, &key) {
                Ok(sync) => sync,
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
            if uploaded_mesh {
                stats.uploaded_meshes += 1;
            }

            self.reported_failures.remove(&key);
            let transform = kajiya_affine3(instance.transform);
            match self.instance_cache.get_mut(&entity) {
                Some(cached) if cached.key == key && cached.mesh == mesh_handle => {
                    world_renderer.set_instance_transform(cached.handle, transform);
                }
                Some(cached) => {
                    world_renderer.remove_instance(cached.handle);
                    let handle = world_renderer.add_instance(mesh_handle, transform);
                    *cached = CachedInstance {
                        key,
                        mesh: mesh_handle,
                        handle,
                    };
                    stats.removed_instances += 1;
                    stats.topology_changed = true;
                }
                None => {
                    let handle = world_renderer.add_instance(mesh_handle, transform);
                    self.instance_cache.insert(
                        entity,
                        CachedInstance {
                            key,
                            mesh: mesh_handle,
                            handle,
                        },
                    );
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

    fn consume_asset_events(
        &mut self,
        world_renderer: &mut ::kajiya::world_renderer::WorldRenderer,
        assets: &Assets,
        stats: &mut KajiyaAssetSyncStats,
    ) {
        if self.asset_event_cursor.is_none() {
            self.asset_event_cursor = Some(assets.event_cursor());
        }
        let events = assets.events_since(
            self.asset_event_cursor
                .as_mut()
                .expect("asset event cursor should be initialized"),
        );
        for event in &events {
            self.handle_asset_event(world_renderer, assets, event, stats);
        }
    }

    fn handle_asset_event(
        &mut self,
        world_renderer: &mut ::kajiya::world_renderer::WorldRenderer,
        assets: &Assets,
        event: &AssetEvent,
        stats: &mut KajiyaAssetSyncStats,
    ) {
        if !event_can_affect_kajiya_mesh(event) {
            return;
        }

        let keys = match event.kind {
            AssetEventKind::Loaded => Vec::new(),
            AssetEventKind::Installed | AssetEventKind::Reloaded => {
                self.installed_event_invalidated_keys(assets, event)
            }
            AssetEventKind::ReloadQueued | AssetEventKind::Unloaded => {
                self.referenced_keys_for_event(event)
            }
            AssetEventKind::Failed if event.state == AssetState::Installed => Vec::new(),
            AssetEventKind::Failed => self.referenced_keys_for_event(event),
        };
        self.invalidate_cached_keys(world_renderer, keys, stats);
    }

    fn installed_event_invalidated_keys(
        &self,
        assets: &Assets,
        event: &AssetEvent,
    ) -> Vec<KajiyaMeshKey> {
        self.mesh_cache
            .iter()
            .filter_map(|(key, cached)| {
                if !cached_key_references_event(key, cached, event) {
                    return None;
                }
                match collect_mesh_sources(assets, key) {
                    Ok(current) if cached.sources.matches(&current) => None,
                    _ => Some(key.clone()),
                }
            })
            .collect()
    }

    fn referenced_keys_for_event(&self, event: &AssetEvent) -> Vec<KajiyaMeshKey> {
        self.mesh_cache
            .iter()
            .filter_map(|(key, cached)| {
                cached_key_references_event(key, cached, event).then_some(key.clone())
            })
            .collect()
    }

    fn invalidate_cached_keys(
        &mut self,
        world_renderer: &mut ::kajiya::world_renderer::WorldRenderer,
        keys: Vec<KajiyaMeshKey>,
        stats: &mut KajiyaAssetSyncStats,
    ) {
        if keys.is_empty() {
            return;
        }

        let invalidated = keys.into_iter().collect::<FxHashSet<_>>();
        for key in &invalidated {
            self.mesh_cache.remove(key);
            self.reported_failures.remove(key);
        }

        let stale_instances = self
            .instance_cache
            .iter()
            .filter_map(|(entity, cached)| invalidated.contains(&cached.key).then_some(*entity))
            .collect::<Vec<_>>();
        for entity in stale_instances {
            if let Some(cached) = self.instance_cache.remove(&entity) {
                world_renderer.remove_instance(cached.handle);
                stats.removed_instances += 1;
                stats.topology_changed = true;
            }
        }
    }

    fn sync_mesh(
        &mut self,
        world_renderer: &mut ::kajiya::world_renderer::WorldRenderer,
        assets: &Assets,
        key: &KajiyaMeshKey,
    ) -> Result<(::kajiya::world_renderer::MeshHandle, bool), KajiyaBackendError> {
        let sources = collect_mesh_sources(assets, key)?;
        if let Some(cached) = self.mesh_cache.get(key) {
            if cached.sources.matches(&sources) {
                return Ok((cached.handle, false));
            }
        }

        let signature = sources.signature();
        let triangle_mesh = build_kajiya_triangle_mesh(sources.mesh.as_ref(), &sources)?;
        let packed = ::kajiya::asset::mesh::pack_triangle_mesh(&triangle_mesh);
        let mesh_path = self.bake_packed_mesh(key, signature, &packed)?;
        let handle = world_renderer
            .add_baked_mesh(mesh_path, ::kajiya::world_renderer::AddMeshOptions::new())
            .map_err(|error| KajiyaBackendError::mesh_upload(key.mesh.to_string(), error))?;

        self.mesh_cache
            .insert(key.clone(), CachedMesh { sources, handle });
        Ok((handle, true))
    }

    fn bake_packed_mesh(
        &self,
        key: &KajiyaMeshKey,
        signature: KajiyaMeshContentSignature,
        packed: &::kajiya::asset::mesh::PackedTriMesh::Proto,
    ) -> Result<PathBuf, KajiyaBackendError> {
        std::fs::create_dir_all(&self.cache_dir).map_err(|error| {
            KajiyaBackendError::cache_io("create Kajiya cache dir", &self.cache_dir, error)
        })?;

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
                .map_err(KajiyaBackendError::image_build)?;
            let mut file = match File::create(&image_path) {
                Ok(file) => file,
                Err(_error) if image_path.exists() => continue,
                Err(error) => {
                    return Err(KajiyaBackendError::cache_io(
                        "write Kajiya image cache file",
                        image_path,
                        error,
                    ));
                }
            };
            loaded.flatten_into(&mut file);
        }

        let mesh_path = self.cache_dir.join(key.cache_name_for_signature(signature));
        if mesh_path.exists() {
            return Ok(mesh_path);
        }

        let mut file = File::create(&mesh_path).map_err(|error| {
            KajiyaBackendError::cache_io("write Kajiya mesh cache file", &mesh_path, error)
        })?;
        packed.flatten_into(&mut file);
        Ok(mesh_path)
    }

    fn report_sync_failure(&mut self, key: &KajiyaMeshKey, error: KajiyaBackendError) {
        if self.reported_failures.insert(key.clone()) {
            eprintln!("[SkyEngine] Kajiya asset sync skipped a mesh: {error}");
        }
    }
}

fn event_can_affect_kajiya_mesh(event: &AssetEvent) -> bool {
    event.asset_type.is_empty()
        || event.asset_type == MeshAsset::TYPE
        || event.asset_type == StandardMaterialAsset::TYPE
        || event.asset_type == TextureAsset::TYPE
}

fn cached_key_references_event(
    key: &KajiyaMeshKey,
    cached: &CachedMesh,
    event: &AssetEvent,
) -> bool {
    if (event.asset_type.is_empty() || event.asset_type == MeshAsset::TYPE) && key.mesh == event.id
    {
        return true;
    }

    if event.asset_type.is_empty() || event.asset_type == StandardMaterialAsset::TYPE {
        if key.materials.contains(&event.id)
            || cached
                .sources
                .materials
                .iter()
                .any(|material| material.id == Some(event.id))
        {
            return true;
        }
    }

    if event.asset_type.is_empty() || event.asset_type == TextureAsset::TYPE {
        if cached
            .sources
            .materials
            .iter()
            .any(|material| material.references_texture(event.id))
        {
            return true;
        }
    }

    false
}

fn build_kajiya_triangle_mesh(
    mesh: &MeshAsset,
    sources: &KajiyaMeshSources,
) -> Result<::kajiya::asset::mesh::TriangleMesh, KajiyaBackendError> {
    let position = required_attribute(mesh, MeshVertexSemantic::Position)?;
    let normal = optional_attribute(mesh, MeshVertexSemantic::Normal);
    let tangent = optional_attribute(mesh, MeshVertexSemantic::Tangent);
    let uv = optional_attribute(mesh, MeshVertexSemantic::UV0);
    let color = optional_attribute(mesh, MeshVertexSemantic::Color);
    let indices = mesh_indices(mesh)?;
    if indices.len() % 3 != 0 {
        return Err(KajiyaBackendError::asset(format!(
            "MeshAsset `{}` has {} indices; Kajiya v1 only accepts triangle lists",
            mesh.label(),
            indices.len()
        )));
    }

    let vertex_count = mesh.vertex_count() as usize;
    let mut material_slots = sources.materials.clone();
    if material_slots.is_empty() {
        material_slots.push(KajiyaMaterialSources::default_placeholder());
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
                KajiyaBackendError::asset(format!(
                    "MeshAsset `{}` has an invalid Position attribute payload",
                    mesh.label()
                ))
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
    for material in &material_slots {
        push_material(&mut out, material)?;
    }

    Ok(out)
}

fn collect_mesh_sources(
    assets: &Assets,
    key: &KajiyaMeshKey,
) -> Result<KajiyaMeshSources, KajiyaBackendError> {
    let mesh = assets.try_get_id::<MeshAsset>(key.mesh).ok_or_else(|| {
        KajiyaBackendError::asset(format!("MeshAsset `{}` is not installed", key.mesh))
    })?;

    let materials = key
        .materials
        .iter()
        .map(|id| {
            let material = assets.try_get_id::<StandardMaterialAsset>(*id);
            KajiyaMaterialSources::from_assets(assets, *id, material)
        })
        .collect();

    Ok(KajiyaMeshSources { mesh, materials })
}

fn required_attribute(
    mesh: &MeshAsset,
    semantic: MeshVertexSemantic,
) -> Result<MeshVertexAttribute, KajiyaBackendError> {
    optional_attribute(mesh, semantic).ok_or_else(|| {
        KajiyaBackendError::asset(format!(
            "MeshAsset `{}` is missing required {:?} attribute for Kajiya",
            mesh.label(),
            semantic
        ))
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

fn mesh_indices(mesh: &MeshAsset) -> Result<Vec<u32>, KajiyaBackendError> {
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
    material_sources: &KajiyaMaterialSources,
) -> Result<(), KajiyaBackendError> {
    use kajiya::asset::mesh::{MeshMaterialMap, TexCompressionMode, TexGamma, TexParams};

    const DEFAULT_MAP_TRANSFORM: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let base = mesh.maps.len() as u32;

    // Kajiya's material shader expects maps in the same order produced by its
    // glTF importer: normal, specular/roughness-metalness, albedo, emissive.
    mesh.maps.push(texture_map_or_placeholder(
        material_sources.normal_texture.as_deref(),
        [127, 127, 255, 255],
        TexParams {
            gamma: TexGamma::Linear,
            use_mips: true,
            compression: TexCompressionMode::Rg,
            channel_swizzle: None,
        },
    )?);
    mesh.maps
        .push(MeshMaterialMap::Placeholder([255, 255, 127, 255]));
    mesh.maps.push(texture_map_or_placeholder(
        material_sources.albedo_texture.as_deref(),
        [255, 255, 255, 255],
        TexParams {
            gamma: TexGamma::Srgb,
            use_mips: true,
            compression: TexCompressionMode::Rgba,
            channel_swizzle: None,
        },
    )?);
    mesh.maps.push(texture_map_or_placeholder(
        material_sources.emissive_texture.as_deref(),
        [255, 255, 255, 255],
        TexParams {
            gamma: TexGamma::Srgb,
            use_mips: true,
            compression: TexCompressionMode::Rgba,
            channel_swizzle: None,
        },
    )?);

    let material = material_sources.material.as_deref();
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
    Ok(())
}

fn texture_map_or_placeholder(
    texture: Option<&TextureAsset>,
    placeholder: [u8; 4],
    params: ::kajiya::asset::mesh::TexParams,
) -> Result<::kajiya::asset::mesh::MeshMaterialMap, KajiyaBackendError> {
    let Some(texture) = texture else {
        return Ok(::kajiya::asset::mesh::MeshMaterialMap::Placeholder(
            placeholder,
        ));
    };

    Ok(::kajiya::asset::mesh::MeshMaterialMap::Image {
        source: ::kajiya::asset::image::ImageSource::Memory(bytes::Bytes::from(
            encode_texture_png(texture)?,
        )),
        params,
    })
}

fn encode_texture_png(texture: &TextureAsset) -> Result<Vec<u8>, KajiyaBackendError> {
    use image::ImageEncoder;

    let expected_len = texture
        .width()
        .checked_mul(texture.height())
        .and_then(|pixels| pixels.checked_mul(4))
        .map(|bytes| bytes as usize)
        .ok_or_else(|| KajiyaBackendError::asset("TextureAsset dimensions overflow"))?;
    if texture.pixels().len() != expected_len {
        return Err(KajiyaBackendError::asset(format!(
            "TextureAsset has {} bytes, expected {} for {}x{} RGBA8",
            texture.pixels().len(),
            expected_len,
            texture.width(),
            texture.height()
        )));
    }

    let mut encoded = Vec::new();
    image::codecs::png::PngEncoder::new(&mut encoded)
        .write_image(
            texture.pixels(),
            texture.width(),
            texture.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|error| {
            KajiyaBackendError::asset(format!("failed to encode texture PNG: {error}"))
        })?;
    Ok(encoded)
}

fn option_arc_ptr_eq<T>(a: &Option<Arc<T>>, b: &Option<Arc<T>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn hash_mesh_asset(mesh: &MeshAsset, hasher: &mut impl Hasher) {
    mesh.label().hash(hasher);
    mesh.vertex_bytes().hash(hasher);
    mesh.vertex_count().hash(hasher);
    mesh.vertex_layout().hash(hasher);
    match mesh.indices() {
        Some(MeshIndexData::U16(indices)) => {
            16u8.hash(hasher);
            indices.hash(hasher);
        }
        Some(MeshIndexData::U32(indices)) => {
            32u8.hash(hasher);
            indices.hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
    for sub_mesh in mesh.sub_meshes() {
        sub_mesh.index_offset.hash(hasher);
        sub_mesh.index_count.hash(hasher);
        sub_mesh.vertex_offset.hash(hasher);
        sub_mesh.material_index.hash(hasher);
        hash_f32_array(sub_mesh.bounding_sphere.center, hasher);
        sub_mesh.bounding_sphere.radius.to_bits().hash(hasher);
    }
    hash_f32_array(mesh.bounding_sphere().center, hasher);
    mesh.bounding_sphere().radius.to_bits().hash(hasher);
}

fn hash_optional_texture(texture: Option<&TextureAsset>, hasher: &mut impl Hasher) {
    match texture {
        Some(texture) => {
            true.hash(hasher);
            texture.width().hash(hasher);
            texture.height().hash(hasher);
            match texture.color_space() {
                TextureColorSpace::Linear => 0u8.hash(hasher),
                TextureColorSpace::Srgb => 1u8.hash(hasher),
            }
            texture.pixels().hash(hasher);
        }
        None => false.hash(hasher),
    }
}

fn hash_f32_array<const N: usize>(values: [f32; N], hasher: &mut impl Hasher) {
    for value in values {
        value.to_bits().hash(hasher);
    }
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
    use crate::render::asset::{MeshAssetDescriptor, MeshVertexLayout};
    use crate::render::Color;

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        normal: [f32; 3],
        uv: [f32; 2],
    }

    fn triangle_mesh(label: &'static str) -> MeshAsset {
        MeshAsset::from_raw(MeshAssetDescriptor::new(
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
            label,
        ))
    }

    fn installed_event(id: AssetId, asset_type: &'static str) -> AssetEvent {
        AssetEvent {
            sequence: 0,
            id,
            kind: AssetEventKind::Installed,
            state: AssetState::Installed,
            generation: 0,
            asset_type: asset_type.to_string(),
            failure_phase: None,
            manifest_fingerprint: None,
            content_hash: None,
            dependencies: Vec::new(),
            reload_pending: false,
        }
    }

    fn assert_single_key(keys: Vec<KajiyaMeshKey>, expected: &KajiyaMeshKey) {
        assert_eq!(keys.len(), 1);
        assert_eq!(&keys[0], expected);
    }

    #[test]
    fn converts_sky_mesh_and_material_to_kajiya_triangle_mesh() {
        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let texture = assets.insert_runtime(TextureAsset::white_pixel());
        let material = assets.insert_runtime(
            StandardMaterialAsset::new()
                .albedo(Color::rgb(0.25, 0.5, 0.75))
                .albedo_texture(texture)
                .roughness(0.35)
                .metallic(0.1),
        );
        let mesh = triangle_mesh("kajiya-conversion");

        let sources = KajiyaMeshSources {
            mesh: Arc::new(mesh.clone()),
            materials: vec![KajiyaMaterialSources::from_assets(
                &assets,
                material.id(),
                assets.try_get(&material),
            )],
        };
        let converted = build_kajiya_triangle_mesh(&mesh, &sources).expect("convert mesh");

        assert_eq!(converted.positions.len(), 3);
        assert_eq!(converted.indices, vec![0, 1, 2]);
        assert_eq!(converted.materials.len(), 1);
        assert_eq!(converted.maps.len(), 4);
        assert!(matches!(
            converted.maps[0],
            ::kajiya::asset::mesh::MeshMaterialMap::Placeholder([127, 127, 255, 255])
        ));
        assert!(matches!(
            converted.maps[1],
            ::kajiya::asset::mesh::MeshMaterialMap::Placeholder([255, 255, 127, 255])
        ));
        assert!(matches!(
            converted.maps[2],
            ::kajiya::asset::mesh::MeshMaterialMap::Image { .. }
        ));
        assert_eq!(converted.materials[0].maps, [0, 1, 2, 3]);
        assert_eq!(
            converted.materials[0].base_color_mult,
            [0.25, 0.5, 0.75, 1.0]
        );
    }

    #[test]
    fn installed_event_only_invalidates_changed_kajiya_sources() {
        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let texture = assets.insert_runtime(TextureAsset::white_pixel());
        let material =
            assets.insert_runtime(StandardMaterialAsset::new().albedo_texture(texture.clone()));
        let mesh = assets.insert_runtime(triangle_mesh("kajiya-event-source"));
        let key = KajiyaMeshKey {
            mesh: mesh.id(),
            materials: vec![material.id()],
        };
        let sources = collect_mesh_sources(&assets, &key).unwrap();
        let mut cache = KajiyaRenderAssetCache::new(PathBuf::new());
        cache.mesh_cache.insert(
            key.clone(),
            CachedMesh {
                sources,
                handle: ::kajiya::world_renderer::MeshHandle(7),
            },
        );

        assert!(cache
            .installed_event_invalidated_keys(
                &assets,
                &installed_event(material.id(), StandardMaterialAsset::TYPE)
            )
            .is_empty());

        assets
            .replace_runtime(
                &texture,
                TextureAsset::checkerboard(2, 1, [255, 0, 0, 255], [0, 0, 255, 255]),
            )
            .unwrap();
        assert_single_key(
            cache.installed_event_invalidated_keys(
                &assets,
                &installed_event(texture.id(), TextureAsset::TYPE),
            ),
            &key,
        );
    }

    #[test]
    fn kajiya_event_references_mesh_material_and_texture_sources() {
        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let albedo = assets.insert_runtime(TextureAsset::white_pixel());
        let material =
            assets.insert_runtime(StandardMaterialAsset::new().albedo_texture(albedo.clone()));
        let mesh = assets.insert_runtime(triangle_mesh("kajiya-event-reference"));
        let key = KajiyaMeshKey {
            mesh: mesh.id(),
            materials: vec![material.id()],
        };
        let sources = collect_mesh_sources(&assets, &key).unwrap();
        let mut cache = KajiyaRenderAssetCache::new(PathBuf::new());
        cache.mesh_cache.insert(
            key.clone(),
            CachedMesh {
                sources,
                handle: ::kajiya::world_renderer::MeshHandle(11),
            },
        );

        assert_single_key(
            cache.referenced_keys_for_event(&installed_event(mesh.id(), MeshAsset::TYPE)),
            &key,
        );
        assert_single_key(
            cache.referenced_keys_for_event(&installed_event(
                material.id(),
                StandardMaterialAsset::TYPE,
            )),
            &key,
        );
        assert_single_key(
            cache.referenced_keys_for_event(&installed_event(albedo.id(), TextureAsset::TYPE)),
            &key,
        );
    }
}
