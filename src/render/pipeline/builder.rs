use std::any::TypeId;

use crate::render::extract::{ExtractMeshes, ExtractSprites, Extractor};
use crate::render::phase::{DrawFunction, DrawFunctionId, DrawMesh, DrawSprite};
use crate::render::resources::material::{Material, SpriteMaterial};
use crate::render::GpuTable;

use super::{
    AnyRenderFeature, ComputePass, GraphPass, MaterialRegistration, PipelineStep, PostFxPass,
    RenderBackendKind, RenderFeature, RenderPass, RenderPhase, RenderPipelineAsset,
};
pub struct RenderPipelineBuilder {
    pub(super) backend_kind: RenderBackendKind,
    pub(super) runtime_features: Vec<Box<dyn AnyRenderFeature>>,
    pub(super) feature_names: Vec<&'static str>,
    pub(super) steps: Vec<PipelineStep>,
    pub(super) extractors: Vec<Box<dyn Extractor>>,
    pub(super) gpu_tables: Vec<Box<dyn GpuTable>>,
    pub(super) draw_functions: Vec<Box<dyn DrawFunction>>,
    pub(super) materials: Vec<MaterialRegistration>,
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
            register: |registry, device| {
                let _ = registry.register_model::<M>(device);
            },
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

    pub fn add_graph_pass<P>(mut self, pass: P) -> Self
    where
        P: GraphPass + 'static,
    {
        self.steps.push(PipelineStep::Graph(Box::new(pass)));
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
