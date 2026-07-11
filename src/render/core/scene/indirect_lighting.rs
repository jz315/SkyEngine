//! Backend-neutral configuration used to select an indirect-lighting provider.

use std::any::Any;
use std::fmt;
use std::sync::Arc;

pub trait IndirectLightingSettings: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

impl<T> IndirectLightingSettings for T
where
    T: Any + Send + Sync,
{
    #[inline]
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone)]
pub struct IndirectLightingProviderConfig {
    pub id: &'static str,
    pub settings: Arc<dyn IndirectLightingSettings>,
}

impl IndirectLightingProviderConfig {
    #[inline]
    pub fn new<T>(id: &'static str, settings: T) -> Self
    where
        T: IndirectLightingSettings + 'static,
    {
        Self {
            id,
            settings: Arc::new(settings),
        }
    }
}

impl fmt::Debug for IndirectLightingProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IndirectLightingProviderConfig")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
