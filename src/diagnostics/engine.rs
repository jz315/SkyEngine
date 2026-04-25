use crate::ecs::EntityId;

use super::{DiagnosticEvent, DiagnosticSeverity, DiagnosticSubsystem};

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EngineDiagnosticKind {
    CameraMissingProjection { entity: EntityId },
}

impl EngineDiagnosticKind {
    pub const CAMERA_MISSING_PROJECTION: &'static str = "render.camera.missing_projection";

    #[inline]
    pub const fn severity(self) -> DiagnosticSeverity {
        match self {
            Self::CameraMissingProjection { .. } => DiagnosticSeverity::Warning,
        }
    }

    pub fn message(self) -> String {
        match self {
            Self::CameraMissingProjection { entity } => format!(
                "Camera {:?} has no Projection; using an implicit orthographic view whose visible \
                 height matches the current viewport. Add Projection::orthographic(height) for \
                 stable world-unit sizing, or Projection::orthographic_fixed(width, height) for a \
                 fixed logical view.",
                entity
            ),
        }
    }

    pub fn event(self) -> DiagnosticEvent {
        match self {
            Self::CameraMissingProjection { entity } => DiagnosticEvent::new(
                Self::CAMERA_MISSING_PROJECTION,
                DiagnosticSubsystem::render(),
                self.severity(),
                format!(
                    "Camera {:?} has no Projection; using an implicit orthographic view whose \
                     visible height matches the current viewport.",
                    entity
                ),
            )
            .with_title("Camera is missing a Projection")
            .with_help(
                "Add Projection::orthographic(height) for stable world-unit sizing, or \
                 Projection::orthographic_fixed(width, height) for a fixed logical view.",
            )
            .with_entity(entity),
        }
    }
}

impl From<EngineDiagnosticKind> for DiagnosticEvent {
    fn from(kind: EngineDiagnosticKind) -> Self {
        kind.event()
    }
}
