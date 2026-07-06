use std::ffi::OsStr;
use std::path::Path;

use super::{builtins::BUILTIN_COOKERS, requested_asset_type};
use crate::asset::types::{AssetConfig, AssetCookedSchema, AssetError, AssetId, AssetMeta};
pub type CookFn = fn(&AssetConfig, &Path, &AssetMeta, &Path) -> Result<(), AssetError>;
pub type DependencyFn = fn(&AssetConfig, &Path, &mut AssetMeta) -> Result<(), AssetError>;
pub type ImportSettingsFn = fn(&str) -> serde_json::Value;
pub type NormalizeImportSettingsFn = fn(&str, &mut serde_json::Value);

#[derive(Clone, Copy, Debug)]
pub struct CookerDescriptor {
    pub asset_type: &'static str,
    pub importer: &'static str,
    pub cooker: &'static str,
    pub version: u32,
    pub dependency_schema: Option<&'static str>,
    pub source_extensions: &'static [&'static str],
    pub cooked_dir: &'static str,
    pub cooked_extension: &'static str,
    pub cook: CookFn,
    pub update_dependencies: Option<DependencyFn>,
    pub default_import_settings: Option<ImportSettingsFn>,
    pub normalize_import_settings: Option<NormalizeImportSettingsFn>,
}

impl CookerDescriptor {
    pub(super) fn cooked_relative_path(&self, asset_id: AssetId) -> String {
        format!("{}/{}.{}", self.cooked_dir, asset_id, self.cooked_extension)
    }

    #[must_use]
    pub fn cooked_schema(&self) -> AssetCookedSchema {
        let schema = AssetCookedSchema::new(self.cooker, self.version);
        match self.dependency_schema {
            Some(dependency_schema) => schema.with_dependency_schema(dependency_schema),
            None => schema,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CookRegistry {
    cookers: Vec<&'static CookerDescriptor>,
}

impl CookRegistry {
    #[must_use]
    pub fn with_builtins() -> Self {
        Self {
            cookers: BUILTIN_COOKERS.to_vec(),
        }
    }

    #[must_use]
    pub fn empty() -> Self {
        Self {
            cookers: Vec::new(),
        }
    }

    pub fn register(&mut self, descriptor: &'static CookerDescriptor) {
        self.cookers.push(descriptor);
    }

    #[must_use]
    pub fn with_registered(mut self, descriptor: &'static CookerDescriptor) -> Self {
        self.register(descriptor);
        self
    }

    pub(super) fn cooker_for_asset_type(
        &self,
        asset_type: &str,
    ) -> Option<&'static CookerDescriptor> {
        self.cookers
            .iter()
            .copied()
            .rev()
            .find(|descriptor| descriptor.asset_type == asset_type)
    }

    pub(super) fn supports_source(&self, path: &Path) -> bool {
        let Some(extension) = path.extension().and_then(OsStr::to_str) else {
            return false;
        };
        self.cookers.iter().copied().any(|descriptor| {
            descriptor
                .source_extensions
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
    }

    pub(super) fn default_cooker_for_source(
        &self,
        source_key: &str,
        import_settings: Option<&serde_json::Value>,
    ) -> Option<&'static CookerDescriptor> {
        let extension = Path::new(source_key).extension()?.to_str()?;
        let candidates = self
            .cookers
            .iter()
            .copied()
            .filter(|descriptor| {
                descriptor
                    .source_extensions
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(extension))
            })
            .collect::<Vec<_>>();

        if let Some(asset_type) = requested_asset_type(import_settings) {
            return candidates
                .iter()
                .copied()
                .rev()
                .find(|descriptor| descriptor.asset_type == asset_type);
        }

        if let Some(import_settings) = import_settings {
            let mut normalized_settings = import_settings.clone();
            for descriptor in &candidates {
                if let Some(normalize_import_settings) = descriptor.normalize_import_settings {
                    normalize_import_settings(source_key, &mut normalized_settings);
                    if let Some(asset_type) = requested_asset_type(Some(&normalized_settings)) {
                        return candidates
                            .iter()
                            .copied()
                            .rev()
                            .find(|descriptor| descriptor.asset_type == asset_type);
                    }
                }
            }
        } else {
            for descriptor in &candidates {
                if let Some(default_import_settings) = descriptor.default_import_settings {
                    let settings = default_import_settings(source_key);
                    if let Some(asset_type) = requested_asset_type(Some(&settings)) {
                        return candidates
                            .iter()
                            .copied()
                            .rev()
                            .find(|descriptor| descriptor.asset_type == asset_type);
                    }
                }
            }
        }

        candidates.into_iter().next()
    }

    pub(super) fn default_import_settings_for_source(&self, source_key: &str) -> serde_json::Value {
        self.default_cooker_for_source(source_key, None)
            .and_then(|descriptor| descriptor.default_import_settings)
            .map(|default_import_settings| default_import_settings(source_key))
            .unwrap_or_else(|| serde_json::json!({}))
    }
}

impl Default for CookRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}
