use std::any::TypeId;

use crate::render::extract::{ExtractMeshes, ExtractSprites, Extractor};
use crate::render::phase::{
    DrawFunction, DrawFunctionId, DrawMesh, DrawSprite, OpaquePhase, TransparentPhase,
};
use crate::render::resources::material::{Material, SpriteMaterial};
use crate::render::GpuTable;

use super::{
    AnyRenderFeature, Bloom, ComputePass, DdgiUpdateCompute, PostFxPass, RenderFeature, RenderPass,
    RenderPhase, ToneMap,
};

/// Rendering backend requested by a [`RenderPipelineAsset`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderBackendKind {
    /// SkyEngine's native `wgpu` renderer.
    #[default]
    Wgpu,
    /// Kajiya-backed high-quality 3D renderer.
    Kajiya,
}

pub(crate) struct MaterialRegistration {
    pub(crate) type_id: TypeId,
    pub(crate) type_name: &'static str,
    pub(crate) register:
        fn(&mut crate::render::resources::material::MaterialRegistry, &wgpu::Device),
}

pub enum PipelineStep {
    Phase(Box<dyn RenderPhase>),
    Compute(Box<dyn ComputePass>),
    Pass(Box<dyn RenderPass>),
    PostFx(Box<dyn PostFxPass>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineStepDescriptor {
    Phase(&'static str),
    Compute(&'static str),
    Pass(&'static str),
    PostFx(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenderPipelineDescriptor {
    pub backend_kind: RenderBackendKind,
    pub feature_names: Vec<&'static str>,
    pub step_names: Vec<PipelineStepDescriptor>,
    pub extractor_names: Vec<&'static str>,
    pub gpu_table_names: Vec<&'static str>,
    pub draw_function_names: Vec<&'static str>,
    pub material_names: Vec<&'static str>,
}

pub struct RenderPipelineBuilder {
    backend_kind: RenderBackendKind,
    runtime_features: Vec<Box<dyn AnyRenderFeature>>,
    feature_names: Vec<&'static str>,
    steps: Vec<PipelineStep>,
    extractors: Vec<Box<dyn Extractor>>,
    gpu_tables: Vec<Box<dyn GpuTable>>,
    draw_functions: Vec<Box<dyn DrawFunction>>,
    materials: Vec<MaterialRegistration>,
}

impl RenderPipelineBuilder {
    fn register_draw_function_boxed(
        &mut self,
        draw_function: Box<dyn DrawFunction>,
    ) -> DrawFunctionId {
        let id = DrawFunctionId::from_raw(self.draw_functions.len());
        self.draw_functions.push(draw_function);
        id
    }

    pub(crate) fn register_draw_function<F>(&mut self, func: F) -> DrawFunctionId
    where
        F: DrawFunction + 'static,
    {
        self.register_draw_function_boxed(Box::new(func))
    }

    fn register_material_inner<M>(&mut self)
    where
        M: Material + 'static,
    {
        let type_id = TypeId::of::<M>();
        if self
            .materials
            .iter()
            .any(|registration| registration.type_id == type_id)
        {
            return;
        }

        self.materials.push(MaterialRegistration {
            type_id,
            type_name: std::any::type_name::<M>(),
            register: |registry, device| registry.register_material::<M>(device),
        });

        if type_id == TypeId::of::<SpriteMaterial>() {
            let draw_function_id = self.register_draw_function(DrawSprite::new());
            self.extractors
                .push(Box::new(ExtractSprites::new(draw_function_id)));
        } else {
            let draw_function_id = self.register_draw_function(DrawMesh::<M>::new());
            self.extractors
                .push(Box::new(ExtractMeshes::<M>::new(draw_function_id)));
        }
    }

    pub fn new() -> Self {
        Self {
            backend_kind: RenderBackendKind::Wgpu,
            runtime_features: Vec::new(),
            feature_names: Vec::new(),
            steps: Vec::new(),
            extractors: Vec::new(),
            gpu_tables: Vec::new(),
            draw_functions: Vec::new(),
            materials: Vec::new(),
        }
    }

    pub fn add_phase<P>(mut self, phase: P) -> Self
    where
        P: RenderPhase + 'static,
    {
        self.steps.push(PipelineStep::Phase(Box::new(phase)));
        self
    }

    pub fn add_pass<P>(mut self, pass: P) -> Self
    where
        P: RenderPass + 'static,
    {
        self.steps.push(PipelineStep::Pass(Box::new(pass)));
        self
    }

    pub fn add_postfx<F>(mut self, fx: F) -> Self
    where
        F: PostFxPass + 'static,
    {
        self.steps.push(PipelineStep::PostFx(Box::new(fx)));
        self
    }

    pub fn add_extractor<E>(mut self, extractor: E) -> Self
    where
        E: Extractor + 'static,
    {
        self.extractors.push(Box::new(extractor));
        self
    }

    pub fn add_gpu_table<T>(mut self, table: T) -> Self
    where
        T: GpuTable + 'static,
    {
        self.gpu_tables.push(Box::new(table));
        self
    }

    pub fn add_compute<C>(mut self, compute: C) -> Self
    where
        C: ComputePass + 'static,
    {
        self.steps.push(PipelineStep::Compute(Box::new(compute)));
        self
    }

    pub fn add_draw_function<F>(mut self, func: F) -> Self
    where
        F: DrawFunction + 'static,
    {
        let _ = self.register_draw_function(func);
        self
    }

    pub fn register_material<M>(mut self) -> Self
    where
        M: Material + 'static,
    {
        self.register_material_inner::<M>();
        self
    }

    pub fn add_feature<F>(mut self, mut feature: F) -> Self
    where
        F: RenderFeature + 'static,
    {
        self.feature_names.push(feature.name());
        feature.register(&mut self);
        self.runtime_features.push(Box::new(feature));
        self
    }

    pub fn build(self) -> RenderPipelineAsset {
        RenderPipelineAsset::from_builder(self)
    }
}

impl Default for RenderPipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RenderPipelineAsset {
    pub(crate) backend_kind: RenderBackendKind,
    pub(crate) runtime_features: Vec<Box<dyn AnyRenderFeature>>,
    pub(crate) feature_names: Vec<&'static str>,
    pub(crate) steps: Vec<PipelineStep>,
    pub(crate) extractors: Vec<Box<dyn Extractor>>,
    pub(crate) gpu_tables: Vec<Box<dyn GpuTable>>,
    pub(crate) draw_functions: Vec<Box<dyn DrawFunction>>,
    pub(crate) materials: Vec<MaterialRegistration>,
}

impl RenderPipelineAsset {
    #[inline]
    pub fn builder() -> RenderPipelineBuilder {
        RenderPipelineBuilder::new()
    }

    #[inline]
    pub fn backend_kind(&self) -> RenderBackendKind {
        self.backend_kind
    }

    pub fn forward_2d() -> Self {
        Self::builder()
            .add_feature(super::SpriteFeature::lit_hdr())
            .add_phase(TransparentPhase::new())
            .add_postfx(Bloom::default())
            .add_postfx(ToneMap::default())
            .build()
    }

    #[cfg(feature = "live2d")]
    pub fn live2d_2d() -> Self {
        Self::builder()
            .add_feature(super::SpriteFeature::unlit())
            .add_feature(super::Live2DFeature::new())
            .add_phase(TransparentPhase::new())
            .build()
    }

    pub fn forward_3d() -> Self {
        Self::builder()
            .add_feature(super::SpriteFeature::lit_hdr())
            .add_phase(crate::render::lighting::shadow::DirectionalShadowPhase::new())
            .add_compute(DdgiUpdateCompute::default())
            .add_phase(OpaquePhase::new())
            .add_phase(TransparentPhase::new())
            .add_postfx(Bloom::default())
            .add_postfx(ToneMap::default())
            .build()
    }

    pub fn kajiya_3d() -> Self {
        let mut asset = Self::builder().build();
        asset.backend_kind = RenderBackendKind::Kajiya;
        asset
    }

    pub fn kajiya_triangle() -> Self {
        let mut asset = Self::kajiya_3d();
        asset.feature_names.push("kajiya_triangle");
        asset
    }

    pub fn descriptor(&self) -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            backend_kind: self.backend_kind,
            feature_names: self.feature_names.clone(),
            step_names: self
                .steps
                .iter()
                .map(|step| match step {
                    PipelineStep::Phase(phase) => PipelineStepDescriptor::Phase(phase.name()),
                    PipelineStep::Compute(compute) => {
                        PipelineStepDescriptor::Compute(compute.name())
                    }
                    PipelineStep::Pass(pass) => PipelineStepDescriptor::Pass(pass.name()),
                    PipelineStep::PostFx(fx) => PipelineStepDescriptor::PostFx(fx.name()),
                })
                .collect(),
            extractor_names: self
                .extractors
                .iter()
                .map(|extractor| extractor.name())
                .collect(),
            gpu_table_names: self.gpu_tables.iter().map(|table| table.name()).collect(),
            draw_function_names: self
                .draw_functions
                .iter()
                .map(|draw_function| draw_function.name())
                .collect(),
            material_names: self
                .materials
                .iter()
                .map(|registration| registration.type_name)
                .collect(),
        }
    }

    fn from_builder(builder: RenderPipelineBuilder) -> Self {
        Self {
            backend_kind: builder.backend_kind,
            runtime_features: builder.runtime_features,
            feature_names: builder.feature_names,
            steps: builder.steps,
            extractors: builder.extractors,
            gpu_tables: builder.gpu_tables,
            draw_functions: builder.draw_functions,
            materials: builder.materials,
        }
    }
}
