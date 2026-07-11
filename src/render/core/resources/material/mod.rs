//! Declarative material models, generational material instances, and prepared
//! GPU-facing material state.

mod binding;
pub mod builtins;
mod debug;
mod dirty_queue;
mod error;
mod id;
mod instance;
mod instance_store;
mod interface;
mod model;
mod pass;
mod pipeline;
mod prepare;
mod prepared;
mod records;
mod registry;
mod scene;
mod scene_binding;
mod shader;

pub use binding::{MaterialBinding, MaterialBindingLayout};
pub use builtins::{AlphaMode, SpriteMaterial, StandardMaterial, UnlitMaterial};
#[allow(unused_imports)]
pub use debug::{MaterialDebugSummary, MaterialInstanceDebugInfo, MaterialModelDebugInfo};
pub use error::MaterialError;
pub use id::{
    ErasedMaterialHandle, MaterialHandle, MaterialInstanceId, MaterialModelId, TypedMaterialHandle,
};
pub use instance::{MaterialInstanceInfo, MaterialInstanceVersion};
pub use interface::{MaterialInterface, MaterialInterfaceBuilder, MaterialRenderState};
pub use model::MaterialModel as Material;
pub use model::{MaterialModel, MaterialVariantContext};
pub use pass::{MainPassMode, MaterialPassSet, MaterialPrepassMode, ShadowPassMode};
pub use pipeline::{
    MaterialPipelineCache, MaterialPipelineDesc, MaterialPipelineKey, PipelineCache,
};
pub use prepare::MaterialPrepareContext;
pub use prepared::{PreparedMaterial, PreparedMaterialBinding};
pub use registry::MaterialRegistry;
pub use scene::{SceneResourceKind, SceneResourceRequirements};
pub use scene_binding::{SceneBindingDesc, SceneBindingKind};
pub use shader::{MaterialShaderSet, ShaderSource, ShaderVariantKey, ShaderVariantPolicy};
