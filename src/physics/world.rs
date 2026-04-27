use super::backend::RapierBackend;
use super::config::PhysicsConfig2D;
use super::handles::PhysicsHandles;

/// Rapier-backed physics world resource.
pub struct PhysicsWorld2D {
    pub(crate) config: PhysicsConfig2D,
    pub(crate) backend: RapierBackend,
    pub(crate) handles: PhysicsHandles,
}

impl PhysicsWorld2D {
    pub fn new(config: PhysicsConfig2D) -> Self {
        Self {
            config,
            backend: RapierBackend::new(config.fixed_dt),
            handles: PhysicsHandles::default(),
        }
    }

    #[inline]
    pub fn config(&self) -> PhysicsConfig2D {
        self.config
    }

    #[inline]
    pub fn set_config(&mut self, config: PhysicsConfig2D) {
        self.config = config;
        self.backend.set_timestep(config.fixed_dt);
    }

    #[inline]
    pub fn body_count(&self) -> usize {
        self.backend.body_count()
    }

    #[inline]
    pub fn collider_count(&self) -> usize {
        self.backend.collider_count()
    }
}

impl Default for PhysicsWorld2D {
    fn default() -> Self {
        Self::new(PhysicsConfig2D::default())
    }
}
