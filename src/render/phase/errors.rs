use crate::render::resources::material::{MaterialError, SceneBindingKind};
use crate::render::resources::mesh::MeshHandle;

use super::DrawFunctionId;

#[derive(Debug)]
pub enum DrawError {
    MissingDrawFunction {
        id: DrawFunctionId,
    },
    MissingMaterial {
        type_name: &'static str,
    },
    MissingMesh {
        handle: MeshHandle,
    },
    InvalidSubMeshIndex {
        mesh: String,
        sub_mesh_index: u32,
    },
    MissingIndexBuffer {
        mesh: String,
        sub_mesh_index: u32,
    },
    MissingFramePayload {
        type_name: &'static str,
    },
    MissingViewPayload {
        type_name: &'static str,
    },
    MissingPreparedFrame {
        entity: crate::ecs::EntityId,
    },
    MissingSceneBinding {
        type_name: &'static str,
        kind: SceneBindingKind,
    },
    Material(MaterialError),
}

impl std::fmt::Display for DrawError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDrawFunction { id } => {
                write!(f, "Draw function {:?} has not been registered", id)
            }
            Self::MissingMaterial { type_name } => {
                write!(f, "Material handle did not resolve to `{type_name}`")
            }
            Self::MissingMesh { handle } => {
                write!(
                    f,
                    "Mesh handle {:?} did not resolve to a registered mesh",
                    handle
                )
            }
            Self::InvalidSubMeshIndex {
                mesh,
                sub_mesh_index,
            } => write!(
                f,
                "Mesh `{mesh}` does not contain sub-mesh index {sub_mesh_index}"
            ),
            Self::MissingIndexBuffer {
                mesh,
                sub_mesh_index,
            } => write!(
                f,
                "Mesh `{mesh}` sub-mesh {sub_mesh_index} requires an index buffer"
            ),
            Self::MissingFramePayload { type_name } => {
                write!(f, "Standalone draw requires frame payload `{type_name}`")
            }
            Self::MissingViewPayload { type_name } => {
                write!(f, "Standalone draw requires view payload `{type_name}`")
            }
            Self::MissingPreparedFrame { entity } => {
                write!(
                    f,
                    "Standalone draw could not resolve prepared frame for entity {entity:?}"
                )
            }
            Self::MissingSceneBinding { type_name, kind } => {
                write!(
                    f,
                    "Material `{type_name}` requires scene binding `{kind:?}` but it is unavailable"
                )
            }
            Self::Material(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for DrawError {}

impl From<MaterialError> for DrawError {
    fn from(value: MaterialError) -> Self {
        Self::Material(value)
    }
}
