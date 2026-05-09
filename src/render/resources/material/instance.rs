use std::any::Any;
use std::borrow::Cow;

use super::{MaterialInstanceId, MaterialModelId, PreparedMaterial, ShaderVariantKey};

/// Monotonic version for one material instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MaterialInstanceVersion(u64);

impl MaterialInstanceVersion {
    #[inline]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[inline]
    pub fn bump(&mut self) {
        self.0 = self.0.wrapping_add(1).max(1);
    }
}

pub(crate) struct MaterialInstanceRecord {
    pub id: MaterialInstanceId,
    pub model: MaterialModelId,
    pub model_type: std::any::TypeId,
    pub version: MaterialInstanceVersion,
    pub data: Box<dyn Any + Send + Sync>,
    pub prepared: Option<PreparedMaterial>,
    pub last_prepared_version: Option<MaterialInstanceVersion>,
    pub last_variant: Option<ShaderVariantKey>,
    pub debug_label: Option<Cow<'static, str>>,
}

impl MaterialInstanceRecord {
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.last_prepared_version != Some(self.version)
    }
}

/// Public debug/introspection data for a material instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialInstanceInfo {
    pub id: MaterialInstanceId,
    pub model: MaterialModelId,
    pub version: MaterialInstanceVersion,
    pub prepared_version: Option<MaterialInstanceVersion>,
    pub selected_variant: Option<ShaderVariantKey>,
    pub debug_label: Option<Cow<'static, str>>,
}
