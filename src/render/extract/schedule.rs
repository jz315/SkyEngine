use crate::asset::Assets;
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::phase::{OpaquePhase, TransparentPhase};
use crate::render::resources::texture_cache::SharedRenderAssetCache;
use crate::render::resources::{
    material::{MaterialError, MaterialRegistry},
    mesh::{MeshHandle, MeshRegistry},
};
use crate::render::view::{ResolvedSceneTransforms, SceneView, SceneViewKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractorViewKinds(u8);

impl ExtractorViewKinds {
    pub const MAIN: Self = Self(0b0000_0001);
    pub const DIRECTIONAL_SHADOW: Self = Self(0b0000_0010);
    pub const ALL: Self = Self(Self::MAIN.0 | Self::DIRECTIONAL_SHADOW.0);

    #[inline]
    pub fn contains(self, kind: SceneViewKind) -> bool {
        let flag = match kind {
            SceneViewKind::Main => Self::MAIN,
            SceneViewKind::DirectionalShadow => Self::DIRECTIONAL_SHADOW,
        };
        self.0 & flag.0 != 0
    }
}

pub trait Extractor: Send {
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn supported_view_kinds(&self) -> ExtractorViewKinds {
        ExtractorViewKinds::ALL
    }

    fn begin_frame(&mut self) {}

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
    pub asset_server: Option<&'a Assets>,
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

    pub fn begin_frame(&mut self) {
        for extractor in &mut self.extractors {
            extractor.begin_frame();
        }
    }

    pub fn extract(
        &mut self,
        world: &World,
        transforms: &ResolvedSceneTransforms,
        view: &SceneView,
        ctx: &mut ExtractContext<'_>,
    ) -> Result<(), ExtractError> {
        for extractor in &mut self.extractors {
            if !extractor.supported_view_kinds().contains(view.kind) {
                continue;
            }
            extractor.extract(world, transforms, view, ctx)?;
        }
        Ok(())
    }
}
