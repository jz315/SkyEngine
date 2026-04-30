//! Backend-neutral render asset handles and CPU-side asset data.
//!
//! This layer is intentionally separate from the current `wgpu` runtime
//! registries.  User code can create semantic mesh/material/texture assets
//! here, and each renderer backend is free to cache its own GPU objects from
//! the same handles.

use std::borrow::Cow;
use std::sync::Arc;

use crate::asset::{Asset, AssetConfig, AssetServer, Handle, TextureAsset};
use crate::ecs::World;
use crate::render::resources::material::AlphaMode;
use crate::render::view::Color;

/// Error returned by CPU-side mesh asset construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshAssetError {
    EmptyVertices,
    EmptyIndices,
    InvalidVertexLayout {
        stride: u32,
    },
    VertexDataSizeMismatch {
        bytes: usize,
        stride: u32,
        vertex_count: u32,
    },
}

impl std::fmt::Display for MeshAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyVertices => write!(f, "MeshAsset requires at least one vertex"),
            Self::EmptyIndices => write!(f, "Indexed MeshAsset requires at least one index"),
            Self::InvalidVertexLayout { stride } => {
                write!(
                    f,
                    "MeshAsset vertex layout must use a non-zero stride (got {stride})"
                )
            }
            Self::VertexDataSizeMismatch {
                bytes,
                stride,
                vertex_count,
            } => write!(
                f,
                "MeshAsset vertex payload size mismatch: {bytes} bytes cannot describe \
                 {vertex_count} vertices with stride {stride}"
            ),
        }
    }
}

impl std::error::Error for MeshAssetError {}

/// Backend-neutral vertex attribute format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshVertexFormat {
    Float32,
    Float32x2,
    Float32x3,
    Float32x4,
    Uint32,
    Uint32x2,
    Uint32x3,
    Uint32x4,
    Sint32,
    Sint32x2,
    Sint32x3,
    Sint32x4,
    Unorm8x4,
    Snorm8x4,
    Uint16x2,
    Uint16x4,
    Unorm16x2,
    Unorm16x4,
}

impl MeshVertexFormat {
    #[inline]
    pub const fn byte_size(self) -> u32 {
        match self {
            Self::Float32 | Self::Uint32 | Self::Sint32 => 4,
            Self::Float32x2 | Self::Uint32x2 | Self::Sint32x2 => 8,
            Self::Float32x3 | Self::Uint32x3 | Self::Sint32x3 => 12,
            Self::Float32x4 | Self::Uint32x4 | Self::Sint32x4 => 16,
            Self::Unorm8x4 | Self::Snorm8x4 => 4,
            Self::Uint16x2 | Self::Unorm16x2 => 4,
            Self::Uint16x4 | Self::Unorm16x4 => 8,
        }
    }

    pub(crate) const fn to_wgpu(self) -> wgpu::VertexFormat {
        match self {
            Self::Float32 => wgpu::VertexFormat::Float32,
            Self::Float32x2 => wgpu::VertexFormat::Float32x2,
            Self::Float32x3 => wgpu::VertexFormat::Float32x3,
            Self::Float32x4 => wgpu::VertexFormat::Float32x4,
            Self::Uint32 => wgpu::VertexFormat::Uint32,
            Self::Uint32x2 => wgpu::VertexFormat::Uint32x2,
            Self::Uint32x3 => wgpu::VertexFormat::Uint32x3,
            Self::Uint32x4 => wgpu::VertexFormat::Uint32x4,
            Self::Sint32 => wgpu::VertexFormat::Sint32,
            Self::Sint32x2 => wgpu::VertexFormat::Sint32x2,
            Self::Sint32x3 => wgpu::VertexFormat::Sint32x3,
            Self::Sint32x4 => wgpu::VertexFormat::Sint32x4,
            Self::Unorm8x4 => wgpu::VertexFormat::Unorm8x4,
            Self::Snorm8x4 => wgpu::VertexFormat::Snorm8x4,
            Self::Uint16x2 => wgpu::VertexFormat::Uint16x2,
            Self::Uint16x4 => wgpu::VertexFormat::Uint16x4,
            Self::Unorm16x2 => wgpu::VertexFormat::Unorm16x2,
            Self::Unorm16x4 => wgpu::VertexFormat::Unorm16x4,
        }
    }
}

/// Semantic meaning attached to a mesh vertex attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshVertexSemantic {
    Position,
    Normal,
    Tangent,
    UV0,
    UV1,
    Color,
    Custom(u32),
}

impl MeshVertexSemantic {
    pub(crate) const fn to_wgpu(self) -> crate::render::expert::VertexSemantic {
        match self {
            Self::Position => crate::render::expert::VertexSemantic::Position,
            Self::Normal => crate::render::expert::VertexSemantic::Normal,
            Self::Tangent => crate::render::expert::VertexSemantic::Tangent,
            Self::UV0 => crate::render::expert::VertexSemantic::UV0,
            Self::UV1 => crate::render::expert::VertexSemantic::UV1,
            Self::Color => crate::render::expert::VertexSemantic::Color,
            Self::Custom(value) => crate::render::expert::VertexSemantic::Custom(value),
        }
    }
}

/// One semantic vertex attribute in a [`MeshVertexLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshVertexAttribute {
    pub semantic: MeshVertexSemantic,
    pub format: MeshVertexFormat,
    pub offset: u32,
}

impl MeshVertexAttribute {
    #[inline]
    pub const fn new(semantic: MeshVertexSemantic, format: MeshVertexFormat, offset: u32) -> Self {
        Self {
            semantic,
            format,
            offset,
        }
    }
}

/// Backend-neutral vertex buffer layout metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeshVertexLayout {
    stride: u32,
    attributes: Vec<MeshVertexAttribute>,
}

impl MeshVertexLayout {
    #[inline]
    pub fn new(stride: u32, attributes: impl Into<Vec<MeshVertexAttribute>>) -> Self {
        Self {
            stride,
            attributes: attributes.into(),
        }
    }

    #[inline]
    pub fn empty(stride: u32) -> Self {
        Self::new(stride, Vec::new())
    }

    #[inline]
    pub fn position_uv() -> Self {
        Self::new(
            20,
            [
                MeshVertexAttribute::new(
                    MeshVertexSemantic::Position,
                    MeshVertexFormat::Float32x3,
                    0,
                ),
                MeshVertexAttribute::new(MeshVertexSemantic::UV0, MeshVertexFormat::Float32x2, 12),
            ],
        )
    }

    #[inline]
    pub fn position_normal_uv() -> Self {
        Self::new(
            32,
            [
                MeshVertexAttribute::new(
                    MeshVertexSemantic::Position,
                    MeshVertexFormat::Float32x3,
                    0,
                ),
                MeshVertexAttribute::new(
                    MeshVertexSemantic::Normal,
                    MeshVertexFormat::Float32x3,
                    12,
                ),
                MeshVertexAttribute::new(MeshVertexSemantic::UV0, MeshVertexFormat::Float32x2, 24),
            ],
        )
    }

    #[inline]
    pub fn position_normal_tangent_uv() -> Self {
        Self::new(
            48,
            [
                MeshVertexAttribute::new(
                    MeshVertexSemantic::Position,
                    MeshVertexFormat::Float32x3,
                    0,
                ),
                MeshVertexAttribute::new(
                    MeshVertexSemantic::Normal,
                    MeshVertexFormat::Float32x3,
                    12,
                ),
                MeshVertexAttribute::new(
                    MeshVertexSemantic::Tangent,
                    MeshVertexFormat::Float32x4,
                    24,
                ),
                MeshVertexAttribute::new(MeshVertexSemantic::UV0, MeshVertexFormat::Float32x2, 40),
            ],
        )
    }

    #[inline]
    pub fn stride(&self) -> u32 {
        self.stride
    }

    #[inline]
    pub fn attributes(&self) -> &[MeshVertexAttribute] {
        &self.attributes
    }

    pub(crate) fn to_wgpu(&self) -> crate::render::expert::VertexLayout {
        crate::render::expert::VertexLayout::new(
            self.stride,
            self.attributes
                .iter()
                .map(|attribute| {
                    crate::render::expert::VertexAttribute::new(
                        attribute.semantic.to_wgpu(),
                        attribute.format.to_wgpu(),
                        attribute.offset,
                    )
                })
                .collect::<Vec<_>>(),
        )
    }
}

/// Conservative culling volume stored with mesh assets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshBoundingSphere {
    pub center: [f32; 3],
    pub radius: f32,
}

impl MeshBoundingSphere {
    pub const UNBOUNDED: Self = Self {
        center: [0.0, 0.0, 0.0],
        radius: f32::INFINITY,
    };

    #[inline]
    pub const fn new(center: [f32; 3], radius: f32) -> Self {
        Self { center, radius }
    }

    pub(crate) const fn to_wgpu(self) -> crate::render::expert::BoundingSphere {
        crate::render::expert::BoundingSphere {
            center: self.center,
            radius: self.radius,
        }
    }
}

impl Default for MeshBoundingSphere {
    fn default() -> Self {
        Self::UNBOUNDED
    }
}

/// Continuous geometry slice inside a [`MeshAsset`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshSubMesh {
    pub index_offset: u32,
    pub index_count: u32,
    pub vertex_offset: i32,
    pub material_index: u32,
    pub bounding_sphere: MeshBoundingSphere,
}

impl MeshSubMesh {
    #[inline]
    pub const fn new(
        index_offset: u32,
        index_count: u32,
        vertex_offset: i32,
        material_index: u32,
        bounding_sphere: MeshBoundingSphere,
    ) -> Self {
        Self {
            index_offset,
            index_count,
            vertex_offset,
            material_index,
            bounding_sphere,
        }
    }

    pub(crate) const fn to_wgpu(self) -> crate::render::expert::SubMesh {
        crate::render::expert::SubMesh {
            index_offset: self.index_offset,
            index_count: self.index_count,
            vertex_offset: self.vertex_offset,
            material_index: self.material_index,
            bounding_sphere: self.bounding_sphere.to_wgpu(),
        }
    }
}

/// CPU-side index buffer data for a [`MeshAsset`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeshIndexData {
    U16(Arc<[u16]>),
    U32(Arc<[u32]>),
}

impl MeshIndexData {
    #[inline]
    pub fn u16(indices: impl Into<Arc<[u16]>>) -> Self {
        Self::U16(indices.into())
    }

    #[inline]
    pub fn u32(indices: impl Into<Arc<[u32]>>) -> Self {
        Self::U32(indices.into())
    }

    #[inline]
    pub fn count(&self) -> u32 {
        match self {
            Self::U16(indices) => indices.len() as u32,
            Self::U32(indices) => indices.len() as u32,
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    pub(crate) fn as_wgpu(&self) -> crate::render::expert::MeshIndexData<'_> {
        match self {
            Self::U16(indices) => crate::render::expert::MeshIndexData::U16(indices),
            Self::U32(indices) => crate::render::expert::MeshIndexData::U32(indices),
        }
    }
}

/// Raw CPU mesh descriptor used by [`MeshAsset::try_from_raw`].
#[derive(Debug, Clone)]
pub struct MeshAssetDescriptor<'a> {
    pub label: Cow<'static, str>,
    pub vertex_bytes: &'a [u8],
    pub vertex_count: u32,
    pub vertex_layout: MeshVertexLayout,
    pub indices: Option<MeshIndexData>,
    pub sub_meshes: Vec<MeshSubMesh>,
    pub bounding_sphere: MeshBoundingSphere,
}

impl<'a> MeshAssetDescriptor<'a> {
    #[inline]
    pub fn new(
        vertex_bytes: &'a [u8],
        vertex_count: u32,
        vertex_layout: MeshVertexLayout,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            label: label.into(),
            vertex_bytes,
            vertex_count,
            vertex_layout,
            indices: None,
            sub_meshes: Vec::new(),
            bounding_sphere: MeshBoundingSphere::UNBOUNDED,
        }
    }

    #[inline]
    pub fn with_indices(mut self, indices: MeshIndexData) -> Self {
        self.indices = Some(indices);
        self
    }

    #[inline]
    pub fn with_sub_meshes(mut self, sub_meshes: impl Into<Vec<MeshSubMesh>>) -> Self {
        self.sub_meshes = sub_meshes.into();
        self
    }

    #[inline]
    pub fn with_bounding_sphere(mut self, bounding_sphere: MeshBoundingSphere) -> Self {
        self.bounding_sphere = bounding_sphere;
        self
    }
}

/// Backend-neutral CPU mesh asset.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshAsset {
    label: Cow<'static, str>,
    vertex_bytes: Arc<[u8]>,
    vertex_count: u32,
    vertex_layout: MeshVertexLayout,
    indices: Option<MeshIndexData>,
    sub_meshes: Vec<MeshSubMesh>,
    bounding_sphere: MeshBoundingSphere,
}

impl Asset for MeshAsset {
    const TYPE: &'static str = "mesh";
}

impl MeshAsset {
    pub fn from_raw(desc: MeshAssetDescriptor<'_>) -> Self {
        Self::try_from_raw(desc).expect("MeshAsset::from_raw failed")
    }

    pub fn try_from_raw(desc: MeshAssetDescriptor<'_>) -> Result<Self, MeshAssetError> {
        if desc.vertex_count == 0 || desc.vertex_bytes.is_empty() {
            return Err(MeshAssetError::EmptyVertices);
        }
        if desc.vertex_layout.stride() == 0 {
            return Err(MeshAssetError::InvalidVertexLayout {
                stride: desc.vertex_layout.stride(),
            });
        }
        let expected_bytes = desc.vertex_layout.stride() as usize * desc.vertex_count as usize;
        if desc.vertex_bytes.len() != expected_bytes {
            return Err(MeshAssetError::VertexDataSizeMismatch {
                bytes: desc.vertex_bytes.len(),
                stride: desc.vertex_layout.stride(),
                vertex_count: desc.vertex_count,
            });
        }
        if matches!(desc.indices.as_ref(), Some(indices) if indices.is_empty()) {
            return Err(MeshAssetError::EmptyIndices);
        }

        Ok(Self {
            label: desc.label,
            vertex_bytes: Arc::<[u8]>::from(desc.vertex_bytes.to_vec()),
            vertex_count: desc.vertex_count,
            vertex_layout: desc.vertex_layout,
            indices: desc.indices,
            sub_meshes: desc.sub_meshes,
            bounding_sphere: desc.bounding_sphere,
        })
    }

    pub fn from_vertices<T: bytemuck::Pod>(
        vertices: &[T],
        vertex_layout: MeshVertexLayout,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::try_from_vertices(vertices, vertex_layout, label)
            .expect("MeshAsset::from_vertices failed")
    }

    pub fn try_from_vertices<T: bytemuck::Pod>(
        vertices: &[T],
        vertex_layout: MeshVertexLayout,
        label: impl Into<Cow<'static, str>>,
    ) -> Result<Self, MeshAssetError> {
        Self::try_from_raw(MeshAssetDescriptor::new(
            bytemuck::cast_slice(vertices),
            vertices.len() as u32,
            vertex_layout,
            label,
        ))
    }

    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[inline]
    pub fn vertex_bytes(&self) -> &[u8] {
        &self.vertex_bytes
    }

    #[inline]
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    #[inline]
    pub fn vertex_layout(&self) -> &MeshVertexLayout {
        &self.vertex_layout
    }

    #[inline]
    pub fn indices(&self) -> Option<&MeshIndexData> {
        self.indices.as_ref()
    }

    #[inline]
    pub fn sub_meshes(&self) -> &[MeshSubMesh] {
        &self.sub_meshes
    }

    #[inline]
    pub fn bounding_sphere(&self) -> MeshBoundingSphere {
        self.bounding_sphere
    }

    pub(crate) fn to_wgpu_mesh(
        &self,
        gpu: &crate::gpu::GpuContext,
    ) -> Result<crate::render::expert::Mesh, crate::render::expert::MeshError> {
        let mut desc = crate::render::expert::MeshDescriptor::new(
            &self.vertex_bytes,
            self.vertex_count,
            self.vertex_layout.to_wgpu(),
            self.label.to_string(),
        )
        .with_bounding_sphere(self.bounding_sphere.to_wgpu())
        .with_sub_meshes(
            self.sub_meshes
                .iter()
                .map(|sub_mesh| sub_mesh.to_wgpu())
                .collect::<Vec<_>>(),
        );

        if let Some(indices) = self.indices.as_ref() {
            desc = desc.with_indices(indices.as_wgpu());
        }

        crate::render::expert::Mesh::try_from_raw(gpu, desc)
    }
}

/// Backend-neutral texture sampling metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureSamplerDesc {
    pub min_filter: TextureFilter,
    pub mag_filter: TextureFilter,
    pub mip_filter: TextureFilter,
    pub address_u: TextureAddressMode,
    pub address_v: TextureAddressMode,
}

impl Default for TextureSamplerDesc {
    fn default() -> Self {
        Self {
            min_filter: TextureFilter::Linear,
            mag_filter: TextureFilter::Linear,
            mip_filter: TextureFilter::Linear,
            address_u: TextureAddressMode::Repeat,
            address_v: TextureAddressMode::Repeat,
        }
    }
}

/// Backend-neutral texture filter mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureFilter {
    Nearest,
    Linear,
}

/// Backend-neutral texture address mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureAddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

/// Backend-neutral physically based material asset.
#[derive(Debug, Clone)]
pub struct StandardMaterialAsset {
    pub albedo: Color,
    pub albedo_texture: Option<Handle<TextureAsset>>,
    pub albedo_sampler: TextureSamplerDesc,
    pub metallic: f32,
    pub roughness: f32,
    pub normal_texture: Option<Handle<TextureAsset>>,
    pub normal_sampler: TextureSamplerDesc,
    pub emissive: Color,
    pub emissive_texture: Option<Handle<TextureAsset>>,
    pub emissive_sampler: TextureSamplerDesc,
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
    pub receive_shadows: bool,
}

impl Asset for StandardMaterialAsset {
    const TYPE: &'static str = "standard_material";
}

impl StandardMaterialAsset {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn albedo(mut self, color: Color) -> Self {
        self.albedo = color;
        self
    }

    #[inline]
    pub fn albedo_texture(mut self, texture: Handle<TextureAsset>) -> Self {
        self.albedo_texture = Some(texture);
        self
    }

    #[inline]
    pub fn normal_texture(mut self, texture: Handle<TextureAsset>) -> Self {
        self.normal_texture = Some(texture);
        self
    }

    #[inline]
    pub fn emissive(mut self, color: Color) -> Self {
        self.emissive = color;
        self
    }

    #[inline]
    pub fn emissive_texture(mut self, texture: Handle<TextureAsset>) -> Self {
        self.emissive_texture = Some(texture);
        self
    }

    #[inline]
    pub fn metallic(mut self, metallic: f32) -> Self {
        self.metallic = metallic;
        self
    }

    #[inline]
    pub fn roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness;
        self
    }

    #[inline]
    pub fn alpha_mode(mut self, alpha_mode: AlphaMode) -> Self {
        self.alpha_mode = alpha_mode;
        self
    }

    #[inline]
    pub fn alpha_cutoff(mut self, alpha_cutoff: f32) -> Self {
        self.alpha_cutoff = alpha_cutoff;
        self
    }

    #[inline]
    pub fn alpha_mask(mut self, alpha_cutoff: f32) -> Self {
        self.alpha_mode = AlphaMode::Mask;
        self.alpha_cutoff = alpha_cutoff;
        self
    }

    #[inline]
    pub fn receive_shadows(mut self, receive_shadows: bool) -> Self {
        self.receive_shadows = receive_shadows;
        self
    }
}

impl Default for StandardMaterialAsset {
    fn default() -> Self {
        Self {
            albedo: Color::WHITE,
            albedo_texture: None,
            albedo_sampler: TextureSamplerDesc::default(),
            metallic: 0.0,
            roughness: 0.8,
            normal_texture: None,
            normal_sampler: TextureSamplerDesc::default(),
            emissive: Color::BLACK,
            emissive_texture: None,
            emissive_sampler: TextureSamplerDesc::default(),
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            receive_shadows: true,
        }
    }
}

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

        let config = AssetConfig::default();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        uv: [f32; 2],
    }

    #[test]
    fn mesh_asset_validates_vertex_payload_size() {
        let error = MeshAsset::try_from_raw(MeshAssetDescriptor::new(
            &[0, 1, 2, 3],
            1,
            MeshVertexLayout::position_uv(),
            "bad",
        ))
        .expect_err("payload is too small for the declared layout");

        assert_eq!(
            error,
            MeshAssetError::VertexDataSizeMismatch {
                bytes: 4,
                stride: 20,
                vertex_count: 1,
            }
        );
    }

    #[test]
    fn render_assets_insert_and_resolve_runtime_assets() {
        let mut world = World::new();
        let mut assets = RenderAssets::new(&mut world);

        let texture = assets.insert_texture(TextureAsset::white_pixel());
        let material =
            assets.insert_standard_material(StandardMaterialAsset::new().albedo_texture(texture));
        let mesh = assets.insert_mesh(MeshAsset::from_vertices(
            &[
                Vertex {
                    position: [0.0, 0.0, 0.0],
                    uv: [0.0, 0.0],
                },
                Vertex {
                    position: [1.0, 0.0, 0.0],
                    uv: [1.0, 0.0],
                },
                Vertex {
                    position: [0.0, 1.0, 0.0],
                    uv: [0.0, 1.0],
                },
            ],
            MeshVertexLayout::position_uv(),
            "triangle",
        ));

        assert!(assets.texture(texture).is_some());
        assert!(assets.standard_material(material).is_some());
        assert_eq!(
            assets
                .mesh(mesh)
                .expect("mesh should resolve")
                .vertex_count(),
            3
        );
    }
}
