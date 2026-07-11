pub use crate::math::Projection;

mod camera;
mod frustum;
mod logging;
mod math;
mod projection;
mod scene_view;
mod transform;
mod types;
mod viewport;

pub use camera::{Camera, RenderView, ViewUniform};
pub use frustum::Frustum;
pub use scene_view::{SceneView, SceneViewKind, TemporalViewState};
pub use types::{RenderQueueSort, RenderStats};
pub use viewport::ViewportRect;

pub(crate) use logging::should_log_scene_view;
#[cfg(feature = "live2d")]
pub(crate) use math::column_major_mul;
#[cfg(feature = "live2d")]
pub(crate) use math::scene_transform_matrix;
pub(crate) use projection::ProjectionViewUniformExt;
pub(crate) use scene_view::{build_scene_view, fallback_scene_view};
pub(crate) use transform::{ResolvedSceneTransforms, SceneTransformResolver};
pub(crate) use types::SCENE_HDR_FORMAT;
