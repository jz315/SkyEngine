use crate::asset::AssetServer;
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::phase::{OpaquePhase, TransparentPhase};
use crate::render::resources::assets::SharedRenderAssetCache;
use crate::render::resources::{
    material::{MaterialError, MaterialRegistry},
    mesh::{MeshHandle, MeshRegistry},
};
use crate::render::view::{ResolvedSceneTransforms, SceneView};

pub trait Extractor: Send {
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        view: &SceneView,
        ctx: &mut ExtractContext<'_>,
    ) -> Result<(), ExtractError>;
}

pub struct ExtractContext<'a> {
    pub gpu: &'a GpuContext,
    pub asset_server: Option<&'a AssetServer>,
    pub render_assets: Option<&'a SharedRenderAssetCache>,
    pub material_registry: &'a mut MaterialRegistry,
    pub mesh_registry: &'a MeshRegistry,
    pub opaque_phase: &'a mut OpaquePhase,
    pub transparent_phase: &'a mut TransparentPhase,
    pub quad_mesh_handle: MeshHandle,
}

#[derive(Debug)]
pub enum ExtractError {
    Material(MaterialError),
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Material(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ExtractError {}

impl From<MaterialError> for ExtractError {
    fn from(value: MaterialError) -> Self {
        Self::Material(value)
    }
}

#[derive(Default)]
pub struct ExtractSchedule {
    extractors: Vec<Box<dyn Extractor>>,
}

impl ExtractSchedule {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add<E>(&mut self, extractor: E)
    where
        E: Extractor + 'static,
    {
        self.extractors.push(Box::new(extractor));
    }

    pub fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        view: &SceneView,
        ctx: &mut ExtractContext<'_>,
    ) -> Result<(), ExtractError> {
        for extractor in &mut self.extractors {
            extractor.extract(world, transforms, view, ctx)?;
        }
        Ok(())
    }
}
