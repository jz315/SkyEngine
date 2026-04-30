use std::path::{Path, PathBuf};

use crate::vn::script::{YarnProject, YarnScript};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VnLoaderStatus {
    Idle,
    Pending,
    Loaded { image_count: usize },
    Failed { message: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct VnLoader {
    asset_root: Option<PathBuf>,
    pending: Option<VnLoadRequest>,
    status: VnLoaderStatus,
    last_error: Option<String>,
}

impl Default for VnLoader {
    fn default() -> Self {
        Self {
            asset_root: None,
            pending: None,
            status: VnLoaderStatus::Idle,
            last_error: None,
        }
    }
}

impl VnLoader {
    pub fn set_asset_root(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.asset_root = Some(path.as_ref().to_path_buf());
        self
    }

    pub fn clear_asset_root(&mut self) -> &mut Self {
        self.asset_root = None;
        self
    }

    pub fn asset_root(&self) -> Option<&Path> {
        self.asset_root.as_deref()
    }

    pub fn load_project(&mut self, project: YarnProject) -> &mut Self {
        self.pending = Some(VnLoadRequest::Project(project));
        self.status = VnLoaderStatus::Pending;
        self.last_error = None;
        self
    }

    pub fn load_script(&mut self, script: YarnScript, start_node: impl Into<String>) -> &mut Self {
        self.pending = Some(VnLoadRequest::Script {
            script,
            start_node: start_node.into(),
        });
        self.status = VnLoaderStatus::Pending;
        self.last_error = None;
        self
    }

    pub fn status(&self) -> &VnLoaderStatus {
        &self.status
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub(crate) fn take_pending(&mut self) -> Option<VnLoadRequest> {
        self.pending.take()
    }

    pub(crate) fn mark_loaded(&mut self, image_count: usize) {
        self.status = VnLoaderStatus::Loaded { image_count };
        self.last_error = None;
    }

    pub(crate) fn mark_failed(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.status = VnLoaderStatus::Failed {
            message: message.clone(),
        };
        self.last_error = Some(message);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VnLoadRequest {
    Project(YarnProject),
    Script {
        script: YarnScript,
        start_node: String,
    },
}
