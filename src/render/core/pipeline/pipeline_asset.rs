use crate::render::extract::Extractor;
use crate::render::phase::DrawFunction;
use crate::render::runtime::FrameExtension;
use crate::render::GpuTable;

use super::{
    AnyRenderFeature, KajiyaRendererSettings, MaterialRegistration, PipelineStep,
    PipelineStepDescriptor, RenderBackendKind, RenderPipelineBuilder, RenderPipelineDescriptor,
};
pub struct RenderPipelineAsset {
    pub(crate) backend_kind: RenderBackendKind,
    pub(crate) kajiya_settings: KajiyaRendererSettings,
    pub(crate) runtime_features: Vec<Box<dyn AnyRenderFeature>>,
    pub(crate) frame_extensions: Vec<Box<dyn FrameExtension>>,
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

    #[inline]
    pub fn kajiya_settings(&self) -> KajiyaRendererSettings {
        self.kajiya_settings
    }

    #[inline]
    pub fn with_kajiya_settings(mut self, settings: KajiyaRendererSettings) -> Self {
        self.kajiya_settings = settings;
        self
    }

    pub fn kajiya_3d() -> Self {
        let mut asset = Self::builder().build();
        asset.backend_kind = RenderBackendKind::Kajiya;
        asset
    }

    pub fn renderling_3d() -> Self {
        let mut asset = Self::builder().build();
        asset.backend_kind = RenderBackendKind::Renderling;
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
                    PipelineStep::Graph(pass) => PipelineStepDescriptor::Graph(pass.name()),
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

    pub(super) fn from_builder(builder: RenderPipelineBuilder) -> Self {
        Self {
            backend_kind: builder.backend_kind,
            kajiya_settings: KajiyaRendererSettings::default(),
            runtime_features: builder.runtime_features,
            frame_extensions: builder.frame_extensions,
            feature_names: builder.feature_names,
            steps: builder.steps,
            extractors: builder.extractors,
            gpu_tables: builder.gpu_tables,
            draw_functions: builder.draw_functions,
            materials: builder.materials,
        }
    }
}
