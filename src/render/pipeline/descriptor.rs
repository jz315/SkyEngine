use super::{PipelineStepDescriptor, RenderBackendKind};

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
