use super::{
    cook_audio_registered, cook_font_registered, cook_texture_registered,
    cook_video_clip_registered, default_audio_import_settings, default_texture_import_settings,
    normalize_audio_import_settings, normalize_texture_import_settings,
    update_video_clip_dependencies, CookerDescriptor,
};

const TEXTURE_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: "texture",
    importer: "texture.image",
    cooker: "texture.rgba8",
    version: 1,
    dependency_schema: None,
    source_extensions: &["png", "jpg", "jpeg"],
    cooked_dir: "texture",
    cooked_extension: "skytx",
    cook: cook_texture_registered,
    update_dependencies: None,
    default_import_settings: Some(default_texture_import_settings),
    normalize_import_settings: Some(normalize_texture_import_settings),
};

const FONT_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: "font",
    importer: "font.raw",
    cooker: "font.raw_bytes",
    version: 1,
    dependency_schema: None,
    source_extensions: &["ttf", "otf"],
    cooked_dir: "misc",
    cooked_extension: "skyasset",
    cook: cook_font_registered,
    update_dependencies: None,
    default_import_settings: None,
    normalize_import_settings: None,
};

const SOUND_CLIP_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: "sound_clip",
    importer: "audio.symphonia",
    cooker: "audio.copy",
    version: 1,
    dependency_schema: None,
    source_extensions: &["wav", "ogg", "mp3"],
    cooked_dir: "audio",
    cooked_extension: "skyaudio",
    cook: cook_audio_registered,
    update_dependencies: None,
    default_import_settings: Some(default_audio_import_settings),
    normalize_import_settings: Some(normalize_audio_import_settings),
};

const MUSIC_TRACK_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: "music_track",
    importer: "audio.symphonia",
    cooker: "audio.copy",
    version: 1,
    dependency_schema: None,
    source_extensions: &["wav", "ogg", "mp3"],
    cooked_dir: "audio",
    cooked_extension: "skyaudio",
    cook: cook_audio_registered,
    update_dependencies: None,
    default_import_settings: Some(default_audio_import_settings),
    normalize_import_settings: Some(normalize_audio_import_settings),
};

const VIDEO_CLIP_COOKER: CookerDescriptor = CookerDescriptor {
    asset_type: "video_clip",
    importer: "video.frame_sequence",
    cooker: "video.clip_json",
    version: 1,
    dependency_schema: Some("video.frames.texture"),
    source_extensions: &["skyvideo"],
    cooked_dir: "video",
    cooked_extension: "skyvideo",
    cook: cook_video_clip_registered,
    update_dependencies: Some(update_video_clip_dependencies),
    default_import_settings: None,
    normalize_import_settings: None,
};

pub(super) const BUILTIN_COOKERS: &[&CookerDescriptor] = &[
    &TEXTURE_COOKER,
    &FONT_COOKER,
    &SOUND_CLIP_COOKER,
    &MUSIC_TRACK_COOKER,
    &VIDEO_CLIP_COOKER,
];
