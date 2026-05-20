//! Visual novel / Galgame script and runtime support.
//!
//! The core `vn` feature is intentionally renderer-agnostic. It can load and
//! validate a small Yarn-compatible script subset, then execute story flow into
//! dialogue, choice, command, and wait events. Presentation systems can consume
//! those events later without coupling the script runtime to a specific UI or
//! renderer.

pub mod action;
pub mod asset;
pub mod audio;
#[cfg(feature = "vn-audio")]
pub mod audio_binding;
pub mod components;
pub mod debug;
pub mod dialogue;
pub mod extension;
pub mod loader;
pub mod localization;
pub mod plugin;
pub mod preferences;
#[cfg(feature = "app")]
pub mod presentation;
pub mod progress;
pub mod resource;
pub mod rollback;
pub mod runtime;
pub mod save;
pub mod scene;
pub mod script;
pub mod systems;
pub mod ui;
#[cfg(feature = "vn-ui")]
pub mod ui_binding;
pub mod video;

pub use action::{VnAction, VnInputState, VnPlaybackState};
pub use asset::{VnAssetIntent, VnAssetIntentKind, VnAssetState, VnAssetUsePolicy};
pub use audio::{
    VnAudioIntent, VnAudioState, VnAudioVolumes, VnBgmState, VnSfxEvent, VnVoiceState,
};
#[cfg(feature = "vn-audio")]
pub use audio_binding::{
    sync_audio_intents_to_world, sync_runtime_audio_to_world, VnAudioBindings, VnAudioSyncReport,
    VnAudioSystemError, VnBoundAudioInstance,
};
pub use components::{
    VnActorSprite, VnBackground, VnChoiceUi, VnDialogueUi, VnLive2DActor, VnSceneLayer,
};
pub use debug::VnDebugState;
pub use dialogue::{VnBacklogLine, VnDialogueChoice, VnDialogueState};
pub use extension::{VnCommandDescriptor, VnCommandFamily, VnCommandRegistry};
pub use loader::{VnLoader, VnLoaderStatus};
pub use localization::{VnLocalizationTable, VnLocalizedLine};
pub use plugin::VnPlugin;
pub use preferences::VnPreferences;
#[cfg(feature = "app")]
pub use presentation::{
    sync_runtime_scene_to_world, sync_scene_to_world, VnSpritePresentationConfig,
    VnSpriteSceneEntities, VnSpriteTextureMap, VnTextureLoadError,
};
pub use progress::{VnProgressChange, VnProgressState};
pub use resource::{VnLoadError, VnResource, VnResourceStatus};
pub use rollback::{VnRollbackReason, VnRollbackSnapshot, VnRollbackStack};
pub use runtime::{
    VnActiveChoice, VnConditionalSnapshot, VnRuntime, VnRuntimeConfig, VnRuntimeError,
    VnRuntimeEvent, VnRuntimeResult, VnRuntimeSnapshot, VnStackFrame, VnStatus,
};
pub use save::{VnSaveData, VnSaveSlot, VnSaveStore, VnSaveStoreError, VnSaveThumbnail};
pub use scene::{
    VnActor, VnCameraState, VnImageLayer, VnSceneChange, VnSceneState, VnTransition,
    VnTransitionKind,
};
pub use script::{
    VnCharacterManifest, VnCommandArg, VnCompileError, VnDiagnostic, VnDiagnosticSeverity,
    VnProjectManifest, VnSpan, VnValue, YarnChoice, YarnCommand, YarnInstruction, YarnLine,
    YarnNode, YarnProject, YarnScript,
};
pub use systems::{
    drain_runtime, vn_input_system, vn_load_system, vn_script_system, vn_ui_system, VnSystemConfig,
};
pub use ui::{VnConfirmKind, VnUiMode, VnUiState};
#[cfg(feature = "vn-ui")]
pub use ui_binding::{
    compose_vn_ui, compose_vn_ui_with, drain_vn_ui_actions_to_resource, VnUiActionSink,
    VnUiComposeContext, VnUiLayoutPreset, VnUiPresentationConfig,
};
pub use video::{VnVideoIntent, VnVideoPlayback, VnVideoState};
