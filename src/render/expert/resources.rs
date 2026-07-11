//! Low-level materials, meshes, atlases, and scene binding resources.

pub use crate::render::resources::{
    atlas::{AtlasError, AtlasPacker, TextureAtlas, UvRect},
    blackboard::Blackboard,
    material::{
        AlphaMode, Material, MaterialBindingLayout, MaterialError, MaterialHandle,
        MaterialPipelineCache, MaterialPipelineDesc, MaterialRegistry, MaterialRenderState,
        PipelineCache, SceneBindingDesc, SceneBindingKind, ShaderSource, SpriteMaterial,
        StandardMaterial, UnlitMaterial,
    },
    mesh::{
        BoundingSphere, Mesh, MeshDescriptor, MeshError, MeshHandle, MeshIndexData, MeshRegistry,
        Ray, RayAabb, RayBlasNode, RayHit, RayMesh, RayTriangle, SubMesh, VertexAttribute,
        VertexLayout, VertexSemantic,
    },
};
