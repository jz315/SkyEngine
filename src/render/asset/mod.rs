//! Backend-neutral render asset handles and CPU-side asset data.
//!
//! This layer is intentionally separate from the current `wgpu` runtime
//! registries. User code can create semantic mesh/material/texture assets
//! here, and each renderer backend is free to cache its own GPU objects from
//! the same handles.

mod material_asset;
mod mesh_asset;
mod render_assets;
mod runtime_factory;
mod vertex_layout;

#[cfg(test)]
mod tests;

pub use material_asset::{
    StandardMaterialAsset, TextureAddressMode, TextureFilter, TextureSamplerDesc,
};
pub use mesh_asset::{
    MeshAsset, MeshAssetDescriptor, MeshAssetError, MeshBoundingSphere, MeshIndexData, MeshSubMesh,
};
pub use render_assets::RenderAssets;
pub use runtime_factory::{
    register_render_asset_factories, register_render_cookers, render_cook_registry,
};
pub use vertex_layout::{
    MeshVertexAttribute, MeshVertexFormat, MeshVertexLayout, MeshVertexSemantic,
};
