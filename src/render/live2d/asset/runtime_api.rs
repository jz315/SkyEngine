use std::path::Path;

use crate::render::gpu::Texture;
use crate::render::live2d::runtime::Live2DUserModel;

use super::*;

impl Live2DModelResource {
    /// Create one mutable runtime instance from this immutable asset bundle.
    pub fn instantiate(&self) -> Result<Live2DUserModel, Live2DLoadError> {
        Live2DUserModel::from_resource(self)
    }

    /// GPU textures referenced by this model definition.
    pub fn textures(&self) -> &[Texture] {
        &self.textures
    }

    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Parsed hit areas declared in `.model3.json`.
    pub fn hit_areas(&self) -> &[Live2DHitArea] {
        &self.hit_areas
    }

    /// Parsed `.userdata3.json` entries.
    pub fn user_data(&self) -> &[Live2DUserDataEntry] {
        &self.user_data
    }

    /// Parsed `.cdi3.json` display metadata, if present.
    pub fn display_info(&self) -> Option<&Live2DDisplayInfo> {
        self.display_info.as_ref()
    }

    /// Base directory of the model on disk.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}
