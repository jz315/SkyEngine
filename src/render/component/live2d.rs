#[cfg(feature = "live2d")]
use std::path::PathBuf;

#[cfg(feature = "live2d")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Live2DModelInstance {
    pub model_path: PathBuf,
    pub visible: bool,
}

#[cfg(feature = "live2d")]
impl Live2DModelInstance {
    #[inline]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: path.into(),
            visible: true,
        }
    }

    #[inline]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
}
