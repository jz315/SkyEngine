use std::borrow::Cow;

use super::{
    MaterialInstanceId, MaterialInstanceVersion, MaterialInterface, MaterialModelId,
    ShaderVariantKey,
};

#[derive(Clone, Debug)]
pub struct MaterialModelDebugInfo {
    pub id: MaterialModelId,
    pub name: &'static str,
    pub interface: MaterialInterface,
    pub instance_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialInstanceDebugInfo {
    pub id: MaterialInstanceId,
    pub model: MaterialModelId,
    pub version: MaterialInstanceVersion,
    pub prepared_version: Option<MaterialInstanceVersion>,
    pub selected_variant: Option<ShaderVariantKey>,
    pub debug_label: Option<Cow<'static, str>>,
}

#[derive(Clone, Debug)]
pub struct MaterialDebugSummary {
    pub models: Vec<MaterialModelDebugInfo>,
    pub instances: Vec<MaterialInstanceDebugInfo>,
    pub pipeline_count: usize,
}
