use std::borrow::Cow;

use super::{instance::MaterialInstanceRecord, records::ModelRecord};
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

pub(super) fn build_debug_summary(
    models: &[Option<ModelRecord>],
    instances: &[Option<MaterialInstanceRecord>],
    pipeline_count: usize,
) -> MaterialDebugSummary {
    let models = models
        .iter()
        .filter_map(|model| model.as_ref())
        .map(|model| MaterialModelDebugInfo {
            id: model.id,
            name: model.interface.name,
            interface: model.interface.clone(),
            instance_count: instances
                .iter()
                .filter_map(|record| record.as_ref())
                .filter(|record| record.model == model.id)
                .count(),
        })
        .collect();
    let instances = instances
        .iter()
        .filter_map(|record| record.as_ref())
        .map(|record| MaterialInstanceDebugInfo {
            id: record.id,
            model: record.model,
            version: record.version,
            prepared_version: record.last_prepared_version,
            selected_variant: record.last_variant.clone(),
            debug_label: record.debug_label.clone(),
        })
        .collect();
    MaterialDebugSummary {
        models,
        instances,
        pipeline_count,
    }
}
