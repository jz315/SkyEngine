use crate::ecs::World;
use crate::vn::action::VnInputState;
use crate::vn::extension::VnCommandRegistry;
use crate::vn::loader::VnLoader;
use crate::vn::preferences::VnPreferences;
use crate::vn::rollback::VnRollbackStack;
use crate::vn::save::VnSaveStore;
use crate::vn::systems::{
    vn_input_system, vn_load_system, vn_script_system, vn_ui_system, VnSystemConfig,
};
use crate::vn::ui::VnUiState;
use crate::vn::VnRuntimeResult;

#[derive(Clone, Debug)]
pub struct VnPlugin {
    pub preferences: VnPreferences,
    pub rollback_limit: usize,
    pub system_config: VnSystemConfig,
    pub install_systems: bool,
}

impl Default for VnPlugin {
    fn default() -> Self {
        Self {
            preferences: VnPreferences::default(),
            rollback_limit: 64,
            system_config: VnSystemConfig::default(),
            install_systems: true,
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

    pub fn install(self, world: &mut World) -> VnRuntimeResult<()> {
        if !world.contains_resource::<VnLoader>() {
            world.insert_resource(VnLoader::default());
        }
        world.insert_resource(self.preferences);
        world.insert_resource(VnInputState::default());
        world.insert_resource(crate::vn::VnPlaybackState::default());
        world.insert_resource(VnUiState::default());
        world.insert_resource(VnCommandRegistry::default());
        world.insert_resource(self.system_config);
        #[cfg(feature = "app")]
        {
            if !world.contains_resource::<crate::vn::presentation::VnSpritePresentationConfig>() {
                world.insert_resource(
                    crate::vn::presentation::VnSpritePresentationConfig::default(),
                );
            }
            if !world.contains_resource::<crate::vn::presentation::VnSpriteSceneEntities>() {
                world.insert_resource(crate::vn::presentation::VnSpriteSceneEntities::default());
            }
            if !world.contains_resource::<crate::vn::presentation::VnSpriteTextureMap>() {
                world.insert_resource(crate::vn::presentation::VnSpriteTextureMap::default());
            }
        }
        #[cfg(feature = "vn-audio")]
        if !world.contains_resource::<crate::vn::audio_binding::VnAudioBindings>() {
            world.insert_resource(crate::vn::audio_binding::VnAudioBindings::default());
        }
        #[cfg(feature = "vn-ui")]
        {
            if !world.contains_resource::<crate::vn::ui_binding::VnUiEntities>() {
                world.insert_resource(crate::vn::ui_binding::VnUiEntities::default());
            }
        }
        world.insert_resource(VnRollbackStack::new(self.rollback_limit));
        world.insert_resource(VnSaveStore::default());
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
    use crate::vn::VnRuntime;

    #[test]
    fn default_install_adds_loader_without_loading_runtime() {
        let mut world = World::new();

        VnPlugin::default().install(&mut world).unwrap();

        assert!(world.contains_resource::<VnLoader>());
        assert!(!world.contains_resource::<VnRuntime>());
    }
}
