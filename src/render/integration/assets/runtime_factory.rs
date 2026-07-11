use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::asset::cook::{import_path_with_registry, CookRegistry, CookerDescriptor};
use crate::asset::{
    Asset, AssetConfig, AssetCookedSchema, AssetError, AssetId, AssetInstallContext,
    AssetInstallResult, AssetLoadContext, AssetMeta, AssetRuntimeFactory, Assets, LoadedAsset,
    TextureAsset,
};
use crate::render::resources::material::AlphaMode;
use crate::render::Color;

use super::{
    MeshAsset, MeshAssetDescriptor as RuntimeMeshAssetDescriptor, MeshBoundingSphere,
    MeshIndexData, MeshSubMesh, MeshVertexAttribute, MeshVertexFormat, MeshVertexLayout,
    MeshVertexSemantic, StandardMaterialAsset,
};

const MESH_COOKED_VERSION: u32 = 1;
const STANDARD_MATERIAL_COOKED_VERSION: u32 = 1;

static MESH_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: MeshAsset::TYPE,
    importer: "render.mesh",
    cooker: "render.mesh_json",
    version: MESH_COOKED_VERSION,
    dependency_schema: None,
    source_extensions: &["skymesh", "gltf", "glb"],
    cooked_dir: "mesh",
    cooked_extension: "skymesh",
    cook: cook_mesh,
    update_dependencies: None,
    default_import_settings: None,
    normalize_import_settings: None,
};

static STANDARD_MATERIAL_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: StandardMaterialAsset::TYPE,
    importer: "render.standard_material_json",
    cooker: "render.standard_material_json",
    version: STANDARD_MATERIAL_COOKED_VERSION,
    dependency_schema: Some("render.standard_material.textures"),
    source_extensions: &["skymaterial", "gltf", "glb"],
    cooked_dir: "material",
    cooked_extension: "skymaterial",
    cook: cook_standard_material,
    update_dependencies: Some(update_standard_material_dependencies),
    default_import_settings: None,
    normalize_import_settings: None,
};

pub fn register_render_asset_factories(assets: &Assets) {
    assets.register_factory(MeshAssetFactory);
    assets.register_factory(StandardMaterialAssetFactory);
}

pub fn register_render_cookers(registry: &mut CookRegistry) {
    registry.register(&MESH_COOKER);
    registry.register(&STANDARD_MATERIAL_COOKER);
}

#[must_use]
pub fn render_cook_registry() -> CookRegistry {
    let mut registry = CookRegistry::with_builtins();
    register_render_cookers(&mut registry);
    registry
}

pub(crate) struct MeshAssetFactory;

impl AssetRuntimeFactory for MeshAssetFactory {
    type Asset = MeshAsset;
    type Loaded = MeshAsset;

    fn cooked_schema(&self) -> Option<AssetCookedSchema> {
        Some(MESH_COOKER.cooked_schema())
    }

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let descriptor = MeshDescriptor::from_bytes(ctx.asset_id, ctx.bytes)?;
        let mesh = descriptor.to_asset(Some(ctx.asset_id))?;
        Ok(LoadedAsset::new(mesh).with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        Ok(AssetInstallResult::Ready(loaded.clone()))
    }
}

pub(crate) struct StandardMaterialAssetFactory;

impl AssetRuntimeFactory for StandardMaterialAssetFactory {
    type Asset = StandardMaterialAsset;
    type Loaded = LoadedStandardMaterial;

    fn cooked_schema(&self) -> Option<AssetCookedSchema> {
        Some(STANDARD_MATERIAL_COOKER.cooked_schema())
    }

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let descriptor = StandardMaterialDescriptor::from_bytes(ctx.asset_id, ctx.bytes)?;
        let material = descriptor.to_loaded(ctx.asset_id)?;
        Ok(LoadedAsset::new(material).with_dependencies(ctx.entry.dependencies.clone()))
    }

    fn begin_install(
        &self,
        loaded: &Self::Loaded,
        mut ctx: AssetInstallContext<'_>,
    ) -> Result<AssetInstallResult<Self::Asset>, AssetError> {
        let mut material = loaded.material.clone();
        if let Some(texture) = loaded.albedo_texture {
            material.albedo_texture = Some(ctx.dependency_handle::<TextureAsset>(texture)?);
        }
        if let Some(texture) = loaded.normal_texture {
            material.normal_texture = Some(ctx.dependency_handle::<TextureAsset>(texture)?);
        }
        if let Some(texture) = loaded.emissive_texture {
            material.emissive_texture = Some(ctx.dependency_handle::<TextureAsset>(texture)?);
        }
        Ok(AssetInstallResult::Ready(material))
    }
}

fn cook_mesh(
    _config: &AssetConfig,
    source: &Path,
    _meta: &AssetMeta,
    cooked_path: &Path,
) -> Result<(), AssetError> {
    let descriptor = if is_gltf_source(source) {
        MeshDescriptor::from_gltf_path(source)?
    } else {
        let bytes = std::fs::read(source).map_err(|error| AssetError::Io {
            path: source.to_path_buf(),
            message: error.to_string(),
        })?;
        MeshDescriptor::from_bytes(None, &bytes)?
    };
    let _ = descriptor.to_asset(None)?;
    let bytes = serde_json::to_vec_pretty(&descriptor).map_err(|error| AssetError::Json {
        path: cooked_path.to_path_buf(),
        message: error.to_string(),
    })?;
    std::fs::write(cooked_path, bytes).map_err(|error| AssetError::Io {
        path: cooked_path.to_path_buf(),
        message: error.to_string(),
    })
}

fn cook_standard_material(
    config: &AssetConfig,
    source: &Path,
    meta: &AssetMeta,
    cooked_path: &Path,
) -> Result<(), AssetError> {
    let mut descriptor = if is_gltf_source(source) {
        StandardMaterialDescriptor::from_gltf_path(source, gltf_material_index(meta)?)?
    } else {
        let bytes = std::fs::read(source).map_err(|error| AssetError::Io {
            path: source.to_path_buf(),
            message: error.to_string(),
        })?;
        StandardMaterialDescriptor::from_bytes(None, &bytes)?
    };
    let _ = descriptor.resolve_texture_dependencies(config, source)?;
    let bytes = serde_json::to_vec_pretty(&descriptor).map_err(|error| AssetError::Json {
        path: cooked_path.to_path_buf(),
        message: error.to_string(),
    })?;
    std::fs::write(cooked_path, bytes).map_err(|error| AssetError::Io {
        path: cooked_path.to_path_buf(),
        message: error.to_string(),
    })
}

fn update_standard_material_dependencies(
    config: &AssetConfig,
    source: &Path,
    meta: &mut AssetMeta,
) -> Result<(), AssetError> {
    let mut descriptor = if is_gltf_source(source) {
        StandardMaterialDescriptor::from_gltf_path(source, gltf_material_index(meta)?)?
    } else {
        let bytes = std::fs::read(source).map_err(|error| AssetError::Io {
            path: source.to_path_buf(),
            message: error.to_string(),
        })?;
        StandardMaterialDescriptor::from_bytes(None, &bytes)?
    };
    meta.dependencies = descriptor.resolve_texture_dependencies(config, source)?;
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MeshDescriptor {
    #[serde(default = "default_mesh_label")]
    label: String,
    vertex_bytes: Vec<u8>,
    vertex_count: u32,
    vertex_layout: MeshVertexLayoutDescriptor,
    #[serde(default)]
    indices: Option<MeshIndexDescriptor>,
    #[serde(default)]
    sub_meshes: Vec<MeshSubMeshDescriptor>,
    #[serde(default)]
    bounding_sphere: Option<MeshBoundingSphereDescriptor>,
}

impl MeshDescriptor {
    fn from_bytes(id: impl Into<Option<AssetId>>, bytes: &[u8]) -> Result<Self, AssetError> {
        serde_json::from_slice(bytes).map_err(|error| AssetError::InvalidCookedAsset {
            id: id.into(),
            message: format!("invalid mesh json: {error}"),
        })
    }

    fn from_gltf_path(path: &Path) -> Result<Self, AssetError> {
        const STRIDE: u32 = 32;

        let path_label = path.display().to_string();
        let (document, buffers, _) =
            gltf::import(path).map_err(|error| AssetError::InvalidCookedAsset {
                id: None,
                message: format!("failed to import glTF mesh `{path_label}`: {error}"),
            })?;

        let default_label = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("gltf_mesh");
        let mut label = default_label.to_string();
        let mut vertex_bytes = Vec::<u8>::new();
        let mut indices = Vec::<u32>::new();
        let mut sub_meshes = Vec::<MeshSubMeshDescriptor>::new();
        let mut all_positions = Vec::<[f32; 3]>::new();
        let mut saw_primitive = false;

        for mesh in document.meshes() {
            let mesh_name = mesh.name().unwrap_or(default_label);
            if !saw_primitive {
                label = mesh_name.to_string();
            }
            for (primitive_index, primitive) in mesh.primitives().enumerate() {
                saw_primitive = true;
                if primitive.mode() != gltf::mesh::Mode::Triangles {
                    return Err(invalid_gltf_mesh(format!(
                        "unsupported primitive mode {:?} in mesh `{mesh_name}` primitive {primitive_index}; only triangles are supported",
                        primitive.mode()
                    )));
                }

                let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()].0));
                let positions = reader
                    .read_positions()
                    .ok_or_else(|| {
                        invalid_gltf_mesh(format!(
                            "missing POSITION accessor in mesh `{mesh_name}` primitive {primitive_index}"
                        ))
                    })?
                    .collect::<Vec<_>>();
                if positions.is_empty() {
                    return Err(invalid_gltf_mesh(format!(
                        "empty POSITION accessor in mesh `{mesh_name}` primitive {primitive_index}"
                    )));
                }

                let normals = reader
                    .read_normals()
                    .map(Iterator::collect::<Vec<_>>)
                    .unwrap_or_else(|| vec![[0.0, 0.0, 1.0]; positions.len()]);
                let uvs = reader
                    .read_tex_coords(0)
                    .map(|coords| coords.into_f32().collect::<Vec<_>>())
                    .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);
                let primitive_indices = reader
                    .read_indices()
                    .map(|indices| indices.into_u32().collect::<Vec<_>>())
                    .unwrap_or_else(|| (0..positions.len() as u32).collect::<Vec<_>>());

                let base_vertex = checked_u32(vertex_bytes.len() / STRIDE as usize, "vertices")?;
                for (index, position) in positions.iter().enumerate() {
                    push_f32x3(&mut vertex_bytes, *position);
                    push_f32x3(
                        &mut vertex_bytes,
                        normals.get(index).copied().unwrap_or([0.0, 0.0, 1.0]),
                    );
                    push_f32x2(
                        &mut vertex_bytes,
                        uvs.get(index).copied().unwrap_or([0.0, 0.0]),
                    );
                }

                let index_offset = checked_u32(indices.len(), "indices")?;
                let primitive_vertex_count = checked_u32(positions.len(), "vertices")?;
                for index in primitive_indices {
                    if index >= primitive_vertex_count {
                        return Err(invalid_gltf_mesh(format!(
                            "index {index} is out of bounds for mesh `{mesh_name}` primitive {primitive_index} with {primitive_vertex_count} vertices"
                        )));
                    }
                    indices.push(base_vertex.checked_add(index).ok_or_else(|| {
                        invalid_gltf_mesh(format!(
                            "glTF mesh index overflow in mesh `{mesh_name}` primitive {primitive_index}"
                        ))
                    })?);
                }
                let index_count = checked_u32(indices.len(), "indices")? - index_offset;
                sub_meshes.push(MeshSubMeshDescriptor {
                    index_offset,
                    index_count,
                    vertex_offset: 0,
                    material_index: primitive.material().index().unwrap_or(0) as u32,
                    bounding_sphere: Some(bounding_sphere_descriptor_from_points(&positions)),
                });
                all_positions.extend(positions);
            }
        }

        if !saw_primitive || vertex_bytes.is_empty() {
            return Err(invalid_gltf_mesh(format!(
                "glTF file `{path_label}` does not contain a usable mesh"
            )));
        }

        let indices = if indices.iter().all(|index| *index <= u16::MAX as u32) {
            MeshIndexDescriptor::U16(indices.iter().map(|index| *index as u16).collect())
        } else {
            MeshIndexDescriptor::U32(indices)
        };

        Ok(Self {
            label,
            vertex_bytes,
            vertex_count: checked_u32(all_positions.len(), "vertices")?,
            vertex_layout: MeshVertexLayoutDescriptor {
                stride: STRIDE,
                attributes: vec![
                    MeshVertexAttributeDescriptor {
                        semantic: "position".to_string(),
                        format: "float32x3".to_string(),
                        offset: 0,
                    },
                    MeshVertexAttributeDescriptor {
                        semantic: "normal".to_string(),
                        format: "float32x3".to_string(),
                        offset: 12,
                    },
                    MeshVertexAttributeDescriptor {
                        semantic: "uv0".to_string(),
                        format: "float32x2".to_string(),
                        offset: 24,
                    },
                ],
            },
            indices: Some(indices),
            sub_meshes,
            bounding_sphere: Some(bounding_sphere_descriptor_from_points(&all_positions)),
        })
    }

    fn to_asset(&self, id: Option<AssetId>) -> Result<MeshAsset, AssetError> {
        let mut descriptor = RuntimeMeshAssetDescriptor::new(
            &self.vertex_bytes,
            self.vertex_count,
            self.vertex_layout.to_runtime(id)?,
            self.label.clone(),
        )
        .with_bounding_sphere(self.bounding_sphere.as_ref().map_or(
            MeshBoundingSphere::UNBOUNDED,
            MeshBoundingSphereDescriptor::to_runtime,
        ))
        .with_sub_meshes(
            self.sub_meshes
                .iter()
                .map(|sub_mesh| sub_mesh.to_runtime())
                .collect::<Vec<_>>(),
        );
        if let Some(indices) = &self.indices {
            descriptor = descriptor.with_indices(indices.to_runtime());
        }
        MeshAsset::try_from_raw(descriptor).map_err(|error| AssetError::InvalidCookedAsset {
            id,
            message: error.to_string(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MeshVertexLayoutDescriptor {
    stride: u32,
    #[serde(default)]
    attributes: Vec<MeshVertexAttributeDescriptor>,
}

impl MeshVertexLayoutDescriptor {
    fn to_runtime(&self, id: Option<AssetId>) -> Result<MeshVertexLayout, AssetError> {
        let attributes = self
            .attributes
            .iter()
            .map(|attribute| attribute.to_runtime(id))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(MeshVertexLayout::new(self.stride, attributes))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MeshVertexAttributeDescriptor {
    semantic: String,
    format: String,
    offset: u32,
}

impl MeshVertexAttributeDescriptor {
    fn to_runtime(&self, id: Option<AssetId>) -> Result<MeshVertexAttribute, AssetError> {
        Ok(MeshVertexAttribute::new(
            parse_vertex_semantic(id, &self.semantic)?,
            parse_vertex_format(id, &self.format)?,
            self.offset,
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "format", content = "data", rename_all = "snake_case")]
enum MeshIndexDescriptor {
    U16(Vec<u16>),
    U32(Vec<u32>),
}

impl MeshIndexDescriptor {
    fn to_runtime(&self) -> MeshIndexData {
        match self {
            Self::U16(indices) => MeshIndexData::u16(indices.clone()),
            Self::U32(indices) => MeshIndexData::u32(indices.clone()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MeshSubMeshDescriptor {
    index_offset: u32,
    index_count: u32,
    #[serde(default)]
    vertex_offset: i32,
    #[serde(default)]
    material_index: u32,
    #[serde(default)]
    bounding_sphere: Option<MeshBoundingSphereDescriptor>,
}

impl MeshSubMeshDescriptor {
    fn to_runtime(&self) -> MeshSubMesh {
        MeshSubMesh::new(
            self.index_offset,
            self.index_count,
            self.vertex_offset,
            self.material_index,
            self.bounding_sphere.as_ref().map_or(
                MeshBoundingSphere::UNBOUNDED,
                MeshBoundingSphereDescriptor::to_runtime,
            ),
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MeshBoundingSphereDescriptor {
    center: [f32; 3],
    radius: f32,
}

impl MeshBoundingSphereDescriptor {
    fn to_runtime(&self) -> MeshBoundingSphere {
        MeshBoundingSphere::new(self.center, self.radius)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StandardMaterialDescriptor {
    #[serde(default = "default_albedo")]
    albedo: [f32; 4],
    #[serde(default)]
    albedo_texture: Option<String>,
    #[serde(default)]
    metallic: f32,
    #[serde(default = "default_roughness")]
    roughness: f32,
    #[serde(default)]
    normal_texture: Option<String>,
    #[serde(default = "default_emissive")]
    emissive: [f32; 4],
    #[serde(default)]
    emissive_texture: Option<String>,
    #[serde(default)]
    alpha_mode: MaterialAlphaMode,
    #[serde(default = "default_alpha_cutoff")]
    alpha_cutoff: f32,
    #[serde(default = "default_receive_shadows")]
    receive_shadows: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct LoadedStandardMaterial {
    material: StandardMaterialAsset,
    albedo_texture: Option<AssetId>,
    normal_texture: Option<AssetId>,
    emissive_texture: Option<AssetId>,
}

impl StandardMaterialDescriptor {
    fn from_bytes(id: impl Into<Option<AssetId>>, bytes: &[u8]) -> Result<Self, AssetError> {
        serde_json::from_slice(bytes).map_err(|error| AssetError::InvalidCookedAsset {
            id: id.into(),
            message: format!("invalid standard material json: {error}"),
        })
    }

    fn from_gltf_path(path: &Path, material_index: usize) -> Result<Self, AssetError> {
        let path_label = path.display().to_string();
        let gltf = gltf::Gltf::open(path).map_err(|error| AssetError::InvalidCookedAsset {
            id: None,
            message: format!("failed to import glTF material `{path_label}`: {error}"),
        })?;
        let material = gltf.materials().nth(material_index).ok_or_else(|| {
            invalid_gltf_material(format!(
                "missing material index {material_index} in glTF `{path_label}`"
            ))
        })?;
        let pbr = material.pbr_metallic_roughness();
        let albedo = pbr.base_color_factor();
        let emissive = material.emissive_factor();
        Ok(Self {
            albedo,
            albedo_texture: pbr
                .base_color_texture()
                .map(|info| gltf_texture_uri(info.texture(), "baseColorTexture"))
                .transpose()?,
            metallic: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
            normal_texture: material
                .normal_texture()
                .map(|info| gltf_texture_uri(info.texture(), "normalTexture"))
                .transpose()?,
            emissive: [emissive[0], emissive[1], emissive[2], 1.0],
            emissive_texture: material
                .emissive_texture()
                .map(|info| gltf_texture_uri(info.texture(), "emissiveTexture"))
                .transpose()?,
            alpha_mode: material.alpha_mode().into(),
            alpha_cutoff: material.alpha_cutoff().unwrap_or_else(default_alpha_cutoff),
            receive_shadows: true,
        })
    }

    fn to_loaded(&self, id: AssetId) -> Result<LoadedStandardMaterial, AssetError> {
        Ok(LoadedStandardMaterial {
            material: self.to_asset(),
            albedo_texture: self.texture_id(
                id,
                "albedo_texture",
                self.albedo_texture.as_deref(),
            )?,
            normal_texture: self.texture_id(
                id,
                "normal_texture",
                self.normal_texture.as_deref(),
            )?,
            emissive_texture: self.texture_id(
                id,
                "emissive_texture",
                self.emissive_texture.as_deref(),
            )?,
        })
    }

    fn to_asset(&self) -> StandardMaterialAsset {
        StandardMaterialAsset::new()
            .albedo(color(self.albedo))
            .metallic(self.metallic)
            .roughness(self.roughness)
            .emissive(color(self.emissive))
            .alpha_mode(self.alpha_mode.into())
            .alpha_cutoff(self.alpha_cutoff)
            .receive_shadows(self.receive_shadows)
    }

    fn resolve_texture_dependencies(
        &mut self,
        config: &AssetConfig,
        source: &Path,
    ) -> Result<Vec<AssetId>, AssetError> {
        let mut dependencies = Vec::new();
        if let Some(dependency) =
            resolve_optional_texture_dependency(config, source, &mut self.albedo_texture)?
        {
            push_unique_dependency(&mut dependencies, dependency);
        }
        if let Some(dependency) =
            resolve_optional_texture_dependency(config, source, &mut self.normal_texture)?
        {
            push_unique_dependency(&mut dependencies, dependency);
        }
        if let Some(dependency) =
            resolve_optional_texture_dependency(config, source, &mut self.emissive_texture)?
        {
            push_unique_dependency(&mut dependencies, dependency);
        }
        Ok(dependencies)
    }

    fn texture_id(
        &self,
        id: AssetId,
        field: &str,
        value: Option<&str>,
    ) -> Result<Option<AssetId>, AssetError> {
        let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
            return Ok(None);
        };
        AssetId::parse_str(value).map(Some).map_err(|error| {
            AssetError::InvalidCookedAsset {
                id: Some(id),
                message: format!(
                    "standard material field `{field}` must contain a cooked texture asset id: {error}"
                ),
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MaterialAlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
    Additive,
}

impl From<MaterialAlphaMode> for AlphaMode {
    fn from(value: MaterialAlphaMode) -> Self {
        match value {
            MaterialAlphaMode::Opaque => Self::Opaque,
            MaterialAlphaMode::Mask => Self::Mask,
            MaterialAlphaMode::Blend => Self::Blend,
            MaterialAlphaMode::Additive => Self::Additive,
        }
    }
}

impl From<gltf::material::AlphaMode> for MaterialAlphaMode {
    fn from(value: gltf::material::AlphaMode) -> Self {
        match value {
            gltf::material::AlphaMode::Opaque => Self::Opaque,
            gltf::material::AlphaMode::Mask => Self::Mask,
            gltf::material::AlphaMode::Blend => Self::Blend,
        }
    }
}

fn color([r, g, b, a]: [f32; 4]) -> Color {
    Color::new(r, g, b, a)
}

fn default_albedo() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}

fn default_roughness() -> f32 {
    0.8
}

fn default_emissive() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn default_alpha_cutoff() -> f32 {
    0.5
}

fn default_receive_shadows() -> bool {
    true
}

fn default_mesh_label() -> String {
    "mesh".to_string()
}

fn is_gltf_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extension.eq_ignore_ascii_case("gltf") || extension.eq_ignore_ascii_case("glb")
        })
        .unwrap_or(false)
}

fn invalid_gltf_mesh(message: String) -> AssetError {
    AssetError::InvalidCookedAsset { id: None, message }
}

fn invalid_gltf_material(message: String) -> AssetError {
    AssetError::InvalidCookedAsset { id: None, message }
}

fn gltf_material_index(meta: &AssetMeta) -> Result<usize, AssetError> {
    match meta.import_settings.get("material_index") {
        Some(value) => value
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                invalid_gltf_material(
                    "standard material glTF import setting `material_index` must be a non-negative integer"
                        .to_string(),
                )
            }),
        None => Ok(0),
    }
}

fn gltf_texture_uri(texture: gltf::Texture<'_>, label: &str) -> Result<String, AssetError> {
    match texture.source().source() {
        gltf::image::Source::Uri { uri, .. } => Ok(uri.to_string()),
        gltf::image::Source::View { .. } => Err(AssetError::Unsupported {
            message: format!(
                "standard material glTF `{label}` uses an embedded image; only external image URI dependencies are supported"
            ),
        }),
    }
}

fn checked_u32(value: usize, label: &str) -> Result<u32, AssetError> {
    u32::try_from(value).map_err(|_| {
        invalid_gltf_mesh(format!(
            "glTF mesh expands to {value} {label}, exceeding u32 limits"
        ))
    })
}

fn push_f32x2(bytes: &mut Vec<u8>, value: [f32; 2]) {
    for component in value {
        bytes.extend_from_slice(&component.to_le_bytes());
    }
}

fn push_f32x3(bytes: &mut Vec<u8>, value: [f32; 3]) {
    for component in value {
        bytes.extend_from_slice(&component.to_le_bytes());
    }
}

fn bounding_sphere_descriptor_from_points(points: &[[f32; 3]]) -> MeshBoundingSphereDescriptor {
    if points.is_empty() {
        return MeshBoundingSphereDescriptor {
            center: MeshBoundingSphere::UNBOUNDED.center,
            radius: MeshBoundingSphere::UNBOUNDED.radius,
        };
    }

    let mut min = points[0];
    let mut max = points[0];
    for point in &points[1..] {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    let center = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let radius = points
        .iter()
        .map(|point| {
            let dx = point[0] - center[0];
            let dy = point[1] - center[1];
            let dz = point[2] - center[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        })
        .fold(0.0, f32::max);
    MeshBoundingSphereDescriptor { center, radius }
}

fn resolve_optional_texture_dependency(
    config: &AssetConfig,
    owner_source: &Path,
    texture_ref: &mut Option<String>,
) -> Result<Option<AssetId>, AssetError> {
    let Some(value) = texture_ref
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
    else {
        *texture_ref = None;
        return Ok(None);
    };

    let dependency = resolve_texture_dependency(config, owner_source, &value)?;
    *texture_ref = Some(dependency.to_string());
    Ok(Some(dependency))
}

fn push_unique_dependency(dependencies: &mut Vec<AssetId>, dependency: AssetId) {
    if !dependencies.contains(&dependency) {
        dependencies.push(dependency);
    }
}

fn resolve_texture_dependency(
    config: &AssetConfig,
    owner_source: &Path,
    texture_ref: &str,
) -> Result<AssetId, AssetError> {
    if let Ok(asset_id) = AssetId::parse_str(texture_ref) {
        return Ok(asset_id);
    }

    let texture_path = Path::new(texture_ref);
    let relative_candidate = if texture_path.is_absolute() {
        texture_path.to_path_buf()
    } else {
        owner_source
            .parent()
            .unwrap_or(&config.asset_root)
            .join(texture_path)
    };
    let source = if relative_candidate.exists() {
        relative_candidate
    } else {
        config.asset_root.join(texture_path)
    };

    let registry = CookRegistry::with_builtins();
    let meta = import_path_with_registry(&config.asset_root, &source, &registry)?;
    if meta.asset_type != TextureAsset::TYPE {
        return Err(AssetError::Unsupported {
            message: format!(
                "standard material texture reference `{texture_ref}` resolved to `{}` instead of `{}`",
                meta.asset_type,
                TextureAsset::TYPE
            ),
        });
    }
    Ok(meta.asset_id)
}

fn parse_vertex_semantic(
    id: Option<AssetId>,
    value: &str,
) -> Result<MeshVertexSemantic, AssetError> {
    let normalized = normalize_token(value);
    if let Some(custom) = normalized
        .strip_prefix("custom")
        .and_then(|suffix| suffix.parse::<u32>().ok())
    {
        return Ok(MeshVertexSemantic::Custom(custom));
    }

    match normalized.as_str() {
        "position" => Ok(MeshVertexSemantic::Position),
        "normal" => Ok(MeshVertexSemantic::Normal),
        "tangent" => Ok(MeshVertexSemantic::Tangent),
        "uv0" => Ok(MeshVertexSemantic::UV0),
        "uv1" => Ok(MeshVertexSemantic::UV1),
        "color" => Ok(MeshVertexSemantic::Color),
        _ => Err(AssetError::InvalidCookedAsset {
            id,
            message: format!("unknown mesh vertex semantic `{value}`"),
        }),
    }
}

fn parse_vertex_format(id: Option<AssetId>, value: &str) -> Result<MeshVertexFormat, AssetError> {
    match normalize_token(value).as_str() {
        "float32" => Ok(MeshVertexFormat::Float32),
        "float32x2" => Ok(MeshVertexFormat::Float32x2),
        "float32x3" => Ok(MeshVertexFormat::Float32x3),
        "float32x4" => Ok(MeshVertexFormat::Float32x4),
        "uint32" => Ok(MeshVertexFormat::Uint32),
        "uint32x2" => Ok(MeshVertexFormat::Uint32x2),
        "uint32x3" => Ok(MeshVertexFormat::Uint32x3),
        "uint32x4" => Ok(MeshVertexFormat::Uint32x4),
        "sint32" => Ok(MeshVertexFormat::Sint32),
        "sint32x2" => Ok(MeshVertexFormat::Sint32x2),
        "sint32x3" => Ok(MeshVertexFormat::Sint32x3),
        "sint32x4" => Ok(MeshVertexFormat::Sint32x4),
        "unorm8x4" => Ok(MeshVertexFormat::Unorm8x4),
        "snorm8x4" => Ok(MeshVertexFormat::Snorm8x4),
        "uint16x2" => Ok(MeshVertexFormat::Uint16x2),
        "uint16x4" => Ok(MeshVertexFormat::Uint16x4),
        "unorm16x2" => Ok(MeshVertexFormat::Unorm16x2),
        "unorm16x4" => Ok(MeshVertexFormat::Unorm16x4),
        _ => Err(AssetError::InvalidCookedAsset {
            id,
            message: format!("unknown mesh vertex format `{value}`"),
        }),
    }
}

fn normalize_token(value: &str) -> String {
    value
        .chars()
        .filter(|ch| *ch != '_' && *ch != '-' && *ch != ' ')
        .flat_map(char::to_lowercase)
        .collect()
}
