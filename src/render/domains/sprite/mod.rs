mod access;
mod backend;
mod composite_node;
mod config;
mod domain;
mod extractor;
mod feature;
mod gpu_scene;
mod light_node;
mod prepared;
mod render_pipeline;
mod runtime;
mod scene_cache;
mod sprite_pass;

pub use backend::{SpriteBackend, SpriteBackendBuilder};
pub use composite_node::SpriteCompositeNode;
pub use config::SpriteBackendStats;
pub use domain::SpriteDomain;
pub(crate) use extractor::SceneExtractor;
pub use feature::{
    SpriteDomainExecuteContext, SpriteDomainFeature, SpriteDomainSetupContext, SpriteDomainStage,
};
pub use gpu_scene::GpuScene2D;
pub use light_node::SpriteLightNode;
pub(crate) use prepared::PreparedRenderWorld2D;
pub use prepared::PreparedView2D;
pub use render_pipeline::SpriteFramePipeline;
pub(crate) use scene_cache::{SceneCache2D, SceneLightItem, SceneSpriteItem};
pub use sprite_pass::SpriteSceneNode;

pub(crate) use access::{draw_spans, gpu_scene, prepared_view_2d, require_texture_slot};
