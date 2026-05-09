use crate::ecs::World;
use crate::vn::preferences::VnPreferences;
use crate::vn::resource::VnResource;
use crate::vn::rollback::VnRollbackStack;
use crate::vn::systems::{
    vn_input_system, vn_load_system, vn_script_system, vn_ui_system, VnSystemConfig,
};
use crate::vn::VnRuntimeResult;

#[derive(Clone, Debug)]
pub struct VnPlugin {
    pub preferences: VnPreferences,
    pub rollback_limit: usize,
    pub system_config: VnSystemConfig,
    pub install_systems: bool,
    pub builtin_ui: bool,
    pub sprite_presentation: bool,
}

impl Default for VnPlugin {
    fn default() -> Self {
        Self {
            preferences: VnPreferences::default(),
            rollback_limit: 64,
            system_config: VnSystemConfig::default(),
            install_systems: true,
            builtin_ui: cfg!(feature = "vn-ui"),
            sprite_presentation: cfg!(feature = "app"),
        }
    }
}

impl VnPlugin {
    pub fn with_preferences(mut self, preferences: VnPreferences) -> Self {
        self.preferences = preferences;
        self
    }

    pub fn with_rollback_limit(mut self, rollback_limit: usize) -> Self {
        self.rollback_limit = rollback_limit.max(1);
        self
    }

    pub fn with_system_config(mut self, system_config: VnSystemConfig) -> Self {
        self.system_config = system_config;
        self
    }

    pub fn without_systems(mut self) -> Self {
        self.install_systems = false;
        self
    }

    pub fn with_builtin_ui(mut self, enabled: bool) -> Self {
        self.builtin_ui = enabled;
        self
    }

    pub fn with_sprite_presentation(mut self, enabled: bool) -> Self {
        self.sprite_presentation = enabled;
        self
    }

    pub fn install(self, world: &mut World) -> VnRuntimeResult<()> {
        let _builtin_ui = self.builtin_ui;
        let _sprite_presentation = self.sprite_presentation;
        world.insert_resource(VnResource::new(
            self.preferences,
            VnRollbackStack::new(self.rollback_limit),
            self.system_config,
        ));
        if self.install_systems {
            world.group("vn/load").add(vn_load_system);
            world.group("vn/input").add(vn_input_system);
            world.group("vn/script").add(vn_script_system);
            world.group("vn/ui").add(vn_ui_system);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vn::{VnInputState, VnLoader, VnRuntime, VnSaveStore, VnUiState};

    #[test]
    fn default_install_adds_one_vn_resource_without_loading_runtime() {
        let mut world = World::new();

        VnPlugin::default().install(&mut world).unwrap();

        assert!(world.contains_resource::<VnResource>());
        assert!(!world.contains_resource::<VnRuntime>());
        assert!(!world.contains_resource::<VnLoader>());
        assert!(!world.contains_resource::<VnInputState>());
        assert!(!world.contains_resource::<VnSaveStore>());
        assert!(!world.contains_resource::<VnUiState>());
    }
}
