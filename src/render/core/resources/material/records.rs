use std::any::Any;

use super::{
    MaterialError, MaterialInterface, MaterialModel, MaterialModelId, MaterialPrepareContext,
    MaterialVariantContext, PreparedMaterial, ShaderVariantKey,
};

pub(super) type PrepareFn =
    fn(&dyn Any, &mut MaterialPrepareContext<'_>) -> Result<PreparedMaterial, MaterialError>;
pub(super) type VariantFn =
    fn(&dyn Any, &MaterialVariantContext<'_>) -> Result<ShaderVariantKey, MaterialError>;

pub(super) struct ModelRecord {
    pub id: MaterialModelId,
    pub interface: MaterialInterface,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub prepare: PrepareFn,
    pub variant: VariantFn,
}

pub(super) fn prepare_erased<M: MaterialModel>(
    data: &dyn Any,
    ctx: &mut MaterialPrepareContext<'_>,
) -> Result<PreparedMaterial, MaterialError> {
    let data = data
        .downcast_ref::<M::Data>()
        .ok_or(MaterialError::DowncastMaterialData {
            model: std::any::type_name::<M>(),
        })?;
    M::prepare(data, ctx)
}

pub(super) fn variant_erased<M: MaterialModel>(
    data: &dyn Any,
    ctx: &MaterialVariantContext<'_>,
) -> Result<ShaderVariantKey, MaterialError> {
    let data = data
        .downcast_ref::<M::Data>()
        .ok_or(MaterialError::DowncastMaterialData {
            model: std::any::type_name::<M>(),
        })?;
    Ok(M::variant(data, ctx))
}
