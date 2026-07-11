use crate::render::resources::mesh::VertexSemantic;

use super::{MaterialInstanceId, MaterialModelId, SceneResourceKind};

/// Errors returned by fallible material APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterialError {
    DuplicateBinding {
        model: &'static str,
        binding: u32,
    },
    InvalidBindingVisibility {
        model: &'static str,
        binding: u32,
    },
    ResourceCountMismatch {
        expected: usize,
        actual: usize,
    },
    MissingMaterialLayout,
    OccupiedBindGroupSlot {
        slot: u32,
    },
    MissingVertexAttribute {
        semantic: VertexSemantic,
    },
    VertexAttributeFormatMismatch {
        semantic: VertexSemantic,
        expected: wgpu::VertexFormat,
        actual: wgpu::VertexFormat,
    },
    DuplicateVariantDimension {
        model: &'static str,
        name: &'static str,
    },
    InvalidInterface {
        model: &'static str,
        reason: String,
    },
    UnregisteredMaterialType {
        type_name: &'static str,
    },
    UnregisteredMaterialModelId {
        id: MaterialModelId,
    },
    StaleMaterialHandle {
        id: MaterialInstanceId,
    },
    WrongMaterialModel {
        expected: MaterialModelId,
        actual: MaterialModelId,
    },
    MissingPreparedMaterial {
        id: MaterialInstanceId,
    },
    MissingRequiredSceneResource {
        model: &'static str,
        kind: SceneResourceKind,
    },
    UnsupportedPassCombination {
        model: &'static str,
        reason: String,
    },
    TransparentSubmittedToOpaqueOnlyPhase {
        model: &'static str,
    },
    DowncastMaterialData {
        model: &'static str,
    },
}

impl std::fmt::Display for MaterialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateBinding { model, binding } => {
                write!(
                    f,
                    "Material model `{model}` declares duplicate binding {binding}"
                )
            }
            Self::InvalidBindingVisibility { model, binding } => {
                write!(
                    f,
                    "Material model `{model}` declares binding {binding} with empty visibility"
                )
            }
            Self::ResourceCountMismatch { expected, actual } => {
                write!(
                    f,
                    "Material binding resource count mismatch: expected {expected}, got {actual}"
                )
            }
            Self::MissingMaterialLayout => write!(
                f,
                "Material bind group slot was requested without a material layout"
            ),
            Self::OccupiedBindGroupSlot { slot } => {
                write!(f, "Bind group slot {slot} is already reserved")
            }
            Self::MissingVertexAttribute { semantic } => write!(
                f,
                "Mesh vertex layout is missing required attribute {semantic:?}"
            ),
            Self::VertexAttributeFormatMismatch {
                semantic,
                expected,
                actual,
            } => write!(
                f,
                "Mesh vertex attribute {semantic:?} uses format {actual:?}, expected {expected:?}"
            ),
            Self::DuplicateVariantDimension { model, name } => write!(
                f,
                "Material model `{model}` declares duplicate variant dimension `{name}`"
            ),
            Self::InvalidInterface { model, reason } => {
                write!(
                    f,
                    "Material model `{model}` has an invalid interface: {reason}"
                )
            }
            Self::UnregisteredMaterialType { type_name } => {
                write!(f, "Material model `{type_name}` has not been registered")
            }
            Self::UnregisteredMaterialModelId { id } => {
                write!(f, "Material model id {:?} has not been registered", id)
            }
            Self::StaleMaterialHandle { id } => {
                write!(f, "Material handle {:?} is stale or has been removed", id)
            }
            Self::WrongMaterialModel { expected, actual } => write!(
                f,
                "Material handle belongs to model {:?}, expected {:?}",
                actual, expected
            ),
            Self::MissingPreparedMaterial { id } => {
                write!(f, "Material instance {:?} has no prepared GPU state", id)
            }
            Self::MissingRequiredSceneResource { model, kind } => write!(
                f,
                "Material model `{model}` requires missing scene resource {kind:?}"
            ),
            Self::UnsupportedPassCombination { model, reason } => write!(
                f,
                "Material model `{model}` declares an unsupported pass combination: {reason}"
            ),
            Self::TransparentSubmittedToOpaqueOnlyPhase { model } => write!(
                f,
                "Transparent material model `{model}` was submitted to an opaque-only phase"
            ),
            Self::DowncastMaterialData { model } => {
                write!(f, "Failed to downcast material data for model `{model}`")
            }
        }
    }
}

impl std::error::Error for MaterialError {}
