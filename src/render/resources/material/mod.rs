//! Declarative material models, generational material instances, and prepared
//! GPU-facing material state.

mod binding;
pub mod builtins;
mod debug;
mod error;
mod id;
mod instance;
mod interface;
mod legacy;
mod model;
mod pass;
mod pipeline;
mod prepare;
mod prepared;
mod properties;
mod registry;
mod scene;
mod shader;
mod storage;

pub use binding::{MaterialBinding, MaterialBindingLayout};
pub use builtins::{
    AlphaMode, SpriteMaterial, SpriteMaterialModel, StandardMaterial, StandardMaterialModel,
    UnlitMaterial, UnlitMaterialModel,
};
pub use debug::{MaterialDebugSummary, MaterialInstanceDebugInfo, MaterialModelDebugInfo};
pub use error::MaterialError;
pub use id::{
    ErasedMaterialHandle, MaterialHandle, MaterialInstanceId, MaterialModelId, TypedMaterialHandle,
};
pub use instance::{MaterialInstanceInfo, MaterialInstanceVersion};
pub use interface::{MaterialInterface, MaterialInterfaceBuilder, MaterialRenderState};
pub use legacy::{MaterialModelExt, SceneBindingDesc, SceneBindingKind};
pub use model::MaterialModel as Material;
pub use model::{MaterialModel, MaterialVariantContext};
pub use pass::{MainPassMode, MaterialPassSet, MaterialPrepassMode, ShadowPassMode};
pub use pipeline::{
    MaterialPipelineCache, MaterialPipelineDesc, MaterialPipelineKey, PipelineCache,
};
pub use prepare::{MaterialPrepareContext, PreparedMaterialBuilder};
pub use prepared::{
    MaterialBindContext, MaterialInstance, MaterialResourceBindings, PreparedMaterial,
    PreparedMaterialBinding, PreparedMaterialVersion,
};
pub use properties::{MaterialProperties, PropertyType};
pub use registry::MaterialRegistry;
pub use scene::{SceneResourceKind, SceneResourceRequirements};
pub use shader::{MaterialShaderSet, ShaderSource, ShaderVariantKey, ShaderVariantPolicy};
pub use storage::{MaterialStorage, MaterialStorageMut};

pub type MaterialId = MaterialInstanceId;
