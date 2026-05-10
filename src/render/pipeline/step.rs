use super::{ComputePass, GraphPass, PostFxPass, RenderPass, RenderPhase};

pub enum PipelineStep {
    Phase(Box<dyn RenderPhase>),
    Compute(Box<dyn ComputePass>),
    Graph(Box<dyn GraphPass>),
    Pass(Box<dyn RenderPass>),
    PostFx(Box<dyn PostFxPass>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineStepDescriptor {
    Phase(&'static str),
    Compute(&'static str),
    Graph(&'static str),
    Pass(&'static str),
    PostFx(&'static str),
}
