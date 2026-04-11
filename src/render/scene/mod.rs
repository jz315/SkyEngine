#![allow(unused_imports)]

mod math;
mod projection;
mod transform_resolver;
mod types;
mod view;

pub use projection::Projection;
pub use transform_resolver::ResolvedSceneTransforms;
pub use types::{
    RenderInjectionPoint, RenderOutputFormat, RenderQueueDesc, RenderQueueSort, RenderStageKey,
    RenderStats,
};
pub use view::SceneView;

pub(crate) use math::column_major_mul;
pub(crate) use projection::orthographic_cull_camera;
pub(crate) use transform_resolver::SceneTransformResolver;
pub(crate) use types::SCENE_HDR_FORMAT;
pub(crate) use view::{build_scene_view, default_scene_view_from_prepared, fallback_scene_view};

#[cfg(feature = "live2d")]
pub(crate) use math::scene_transform_matrix;
