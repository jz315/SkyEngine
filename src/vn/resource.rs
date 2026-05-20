use std::error::Error;
use std::fmt;
use std::path::Path;

use crate::action_queue::ActionQueue;
use crate::vn::action::{VnAction, VnInputState, VnPlaybackState};
#[cfg(feature = "vn-audio")]
use crate::vn::audio_binding::VnAudioBindings;
use crate::vn::extension::VnCommandRegistry;
use crate::vn::loader::{VnLoadRequest, VnLoader, VnLoaderStatus};
use crate::vn::preferences::VnPreferences;
#[cfg(feature = "app")]
use crate::vn::presentation::{
    VnSpritePresentationConfig, VnSpriteSceneEntities, VnSpriteTextureMap,
};
use crate::vn::rollback::VnRollbackStack;
use crate::vn::runtime::{VnRuntime, VnRuntimeError, VnRuntimeEvent, VnRuntimeResult, VnStatus};
use crate::vn::save::{VnSaveData, VnSaveStore};
use crate::vn::script::{YarnProject, YarnProjectLoadError, YarnScript};
use crate::vn::systems::VnSystemConfig;
use crate::vn::ui::VnUiState;
#[cfg(feature = "vn-ui")]
use crate::vn::ui_binding::VnUiPresentationConfig;

#[derive(Clone, Debug)]
pub struct VnResource {
    pub(crate) loader: VnLoader,
    pub(crate) project: Option<YarnProject>,
    pub(crate) runtime: Option<VnRuntime>,
    pub(crate) actions: ActionQueue<VnAction>,
    pub(crate) input: VnInputState,
    pub(crate) playback: VnPlaybackState,
    pub(crate) ui: VnUiState,
    pub(crate) preferences: VnPreferences,
    pub(crate) rollback: VnRollbackStack,
    pub(crate) saves: VnSaveStore,
    pub(crate) registry: VnCommandRegistry,
    pub(crate) system_config: VnSystemConfig,
    #[cfg(feature = "app")]
    pub(crate) sprite_presentation_config: VnSpritePresentationConfig,
    #[cfg(feature = "app")]
    pub(crate) sprite_scene_entities: VnSpriteSceneEntities,
    #[cfg(feature = "app")]
    pub(crate) sprite_textures: VnSpriteTextureMap,
    #[cfg(feature = "vn-audio")]
    pub(crate) audio_bindings: VnAudioBindings,
    #[cfg(feature = "vn-ui")]
    pub(crate) ui_presentation_config: Option<VnUiPresentationConfig>,
}

impl Default for VnResource {
    fn default() -> Self {
        Self::new(
            VnPreferences::default(),
            VnRollbackStack::default(),
            VnSystemConfig::default(),
        )
    }
}

impl VnResource {
    pub fn new(
        preferences: VnPreferences,
        rollback: VnRollbackStack,
        system_config: VnSystemConfig,
    ) -> Self {
        Self {
            loader: VnLoader::default(),
            project: None,
            runtime: None,
            actions: ActionQueue::default(),
            input: VnInputState::default(),
            playback: VnPlaybackState::default(),
            ui: VnUiState::default(),
            preferences,
            rollback,
            saves: VnSaveStore::default(),
            registry: VnCommandRegistry::default(),
            system_config,
            #[cfg(feature = "app")]
            sprite_presentation_config: VnSpritePresentationConfig::default(),
            #[cfg(feature = "app")]
            sprite_scene_entities: VnSpriteSceneEntities::default(),
            #[cfg(feature = "app")]
            sprite_textures: VnSpriteTextureMap::default(),
            #[cfg(feature = "vn-audio")]
            audio_bindings: VnAudioBindings::default(),
            #[cfg(feature = "vn-ui")]
            ui_presentation_config: None,
        }
    }

    pub fn load_project_path(&mut self, path: impl AsRef<Path>) -> Result<(), VnLoadError> {
        self.load_project(YarnProject::load(path).map_err(VnLoadError::Project)?)
    }

    pub fn load_project(&mut self, project: YarnProject) -> Result<(), VnLoadError> {
        self.loader.load_project(project);
        Ok(())
    }

    pub fn load_script(
        &mut self,
        script: YarnScript,
        start_node: impl Into<String>,
    ) -> Result<(), VnLoadError> {
        let start_node = start_node.into();
        let project = YarnProject::new(start_node, script).map_err(VnLoadError::Project)?;
        self.load_project(project)
    }

    pub fn set_asset_root(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.loader.set_asset_root(path);
        self
    }

    pub fn clear_asset_root(&mut self) -> &mut Self {
        self.loader.clear_asset_root();
        self
    }

    pub fn status(&self) -> VnResourceStatus {
        VnResourceStatus {
            load: self.loader.status().clone(),
            runtime: self
                .runtime
                .as_ref()
                .map(|runtime| runtime.status().clone()),
        }
    }

    pub fn load_status(&self) -> &VnLoaderStatus {
        self.loader.status()
    }

    pub fn push_action(&mut self, action: VnAction) {
        self.actions.push(action);
    }

    pub fn advance(&mut self) -> VnRuntimeResult<Option<VnRuntimeEvent>> {
        self.runtime_mut_required()?.apply_action(VnAction::Advance)
    }

    pub fn choose(&mut self, index: usize) -> VnRuntimeResult<Option<VnRuntimeEvent>> {
        self.runtime_mut_required()?
            .apply_action(VnAction::Choice(index))
    }

    pub fn save_slot(&mut self, slot: impl Into<String>) -> VnRuntimeResult<()> {
        let slot = slot.into();
        let script_id = self.script_id();
        let preferences = self.preferences.clone();
        let save = {
            let runtime = self.runtime_required()?;
            VnSaveData::from_runtime(slot, script_id, runtime, preferences)
        };
        self.saves.save(save);
        Ok(())
    }

    pub fn load_slot(&mut self, slot: &str) -> VnRuntimeResult<()> {
        let save = self
            .saves
            .get(slot)
            .cloned()
            .ok_or_else(|| VnRuntimeError::MissingSaveSlot(slot.to_owned()))?;
        self.runtime_mut_required()?.restore_snapshot(save.runtime)
    }

    pub fn runtime(&self) -> Option<&VnRuntime> {
        self.runtime.as_ref()
    }

    pub fn runtime_mut(&mut self) -> Option<&mut VnRuntime> {
        self.runtime.as_mut()
    }

    pub fn project(&self) -> Option<&YarnProject> {
        self.project.as_ref()
    }

    pub fn preferences(&self) -> &VnPreferences {
        &self.preferences
    }

    pub fn preferences_mut(&mut self) -> &mut VnPreferences {
        &mut self.preferences
    }

    pub fn ui(&self) -> &VnUiState {
        &self.ui
    }

    pub fn ui_mut(&mut self) -> &mut VnUiState {
        &mut self.ui
    }

    pub fn playback(&self) -> &VnPlaybackState {
        &self.playback
    }

    pub fn playback_mut(&mut self) -> &mut VnPlaybackState {
        &mut self.playback
    }

    pub fn saves(&self) -> &VnSaveStore {
        &self.saves
    }

    pub fn saves_mut(&mut self) -> &mut VnSaveStore {
        &mut self.saves
    }

    pub fn rollback(&self) -> &VnRollbackStack {
        &self.rollback
    }

    pub fn rollback_mut(&mut self) -> &mut VnRollbackStack {
        &mut self.rollback
    }

    pub fn command_registry(&self) -> &VnCommandRegistry {
        &self.registry
    }

    pub fn command_registry_mut(&mut self) -> &mut VnCommandRegistry {
        &mut self.registry
    }

    #[cfg(feature = "app")]
    pub fn sprite_textures(&self) -> &VnSpriteTextureMap {
        &self.sprite_textures
    }

    #[cfg(feature = "app")]
    pub fn sprite_textures_mut(&mut self) -> &mut VnSpriteTextureMap {
        &mut self.sprite_textures
    }

    #[cfg(feature = "app")]
    pub fn sprite_presentation_config(&self) -> &VnSpritePresentationConfig {
        &self.sprite_presentation_config
    }

    #[cfg(feature = "app")]
    pub fn sprite_presentation_config_mut(&mut self) -> &mut VnSpritePresentationConfig {
        &mut self.sprite_presentation_config
    }

    #[cfg(feature = "vn-ui")]
    pub fn ui_presentation_config(&self) -> Option<&VnUiPresentationConfig> {
        self.ui_presentation_config.as_ref()
    }

    #[cfg(feature = "vn-ui")]
    pub fn set_ui_presentation_config(&mut self, config: VnUiPresentationConfig) {
        self.ui_presentation_config = Some(config);
    }

    pub(crate) fn take_pending_load(&mut self) -> Option<VnLoadRequest> {
        self.loader.take_pending()
    }

    pub(crate) fn mark_loaded(
        &mut self,
        project: YarnProject,
        runtime: VnRuntime,
        image_count: usize,
        #[cfg(feature = "app")] textures: VnSpriteTextureMap,
    ) {
        self.project = Some(project);
        self.runtime = Some(runtime);
        #[cfg(feature = "app")]
        {
            self.sprite_textures = textures;
        }
        self.loader.mark_loaded(image_count);
    }

    pub(crate) fn mark_failed(&mut self, message: impl Into<String>) {
        self.loader.mark_failed(message);
    }

    fn runtime_required(&self) -> VnRuntimeResult<&VnRuntime> {
        self.runtime
            .as_ref()
            .ok_or(VnRuntimeError::RuntimeUnavailable)
    }

    fn runtime_mut_required(&mut self) -> VnRuntimeResult<&mut VnRuntime> {
        self.runtime
            .as_mut()
            .ok_or(VnRuntimeError::RuntimeUnavailable)
    }

    fn script_id(&self) -> String {
        self.project
            .as_ref()
            .and_then(|project| {
                let title = project.manifest.title.trim();
                (!title.is_empty()).then(|| title.to_owned())
            })
            .unwrap_or_else(|| "memory".to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VnResourceStatus {
    pub load: VnLoaderStatus,
    pub runtime: Option<VnStatus>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VnLoadError {
    Project(YarnProjectLoadError),
}

impl fmt::Display for VnLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(source) => write!(f, "{source}"),
        }
    }
}

impl Error for VnLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Project(source) => Some(source),
        }
    }
}
