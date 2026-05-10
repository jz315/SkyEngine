//! GPU mesh resources split by responsibility.

mod error;
mod gpu_mesh;
mod math;
mod ray_geometry;
mod registry;
mod shape;
mod vertex_layout;

#[cfg(test)]
mod tests;

pub use error::MeshError;
pub use gpu_mesh::{Mesh, MeshDescriptor, MeshIndexData};
pub use ray_geometry::{Ray, RayAabb, RayBlasNode, RayHit, RayMesh, RayTriangle};
pub use registry::{MeshHandle, MeshRegistry};
pub use shape::{BoundingSphere, SubMesh};
pub use vertex_layout::{VertexAttribute, VertexLayout, VertexSemantic};
