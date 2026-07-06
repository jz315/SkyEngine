use std::path::Path;

use crate::asset::types::normalize_source_key;

pub(super) fn default_texture_import_settings(_source_key: &str) -> serde_json::Value {
    serde_json::json!({ "srgb": true })
}

pub(super) fn normalize_texture_import_settings(
    _source_key: &str,
    import_settings: &mut serde_json::Value,
) {
    let srgb = import_settings
        .get("srgb")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    set_import_setting_bool(import_settings, "srgb", srgb);
}

pub(super) fn default_audio_import_settings(source_key: &str) -> serde_json::Value {
    let stream = infer_default_audio_stream(source_key);
    serde_json::json!({
        "asset_type": audio_asset_type_for_stream(stream),
        "stream": stream,
    })
}

pub(super) fn requested_asset_type(import_settings: Option<&serde_json::Value>) -> Option<&str> {
    import_settings
        .and_then(|settings| settings.get("asset_type"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn normalize_audio_import_settings(
    source_key: &str,
    import_settings: &mut serde_json::Value,
) {
    let stream = requested_asset_type(Some(import_settings))
        .and_then(stream_for_audio_asset_type)
        .or_else(|| {
            import_settings
                .get("stream")
                .and_then(|value| value.as_bool())
        })
        .unwrap_or_else(|| infer_default_audio_stream(source_key));
    set_import_setting_string(
        import_settings,
        "asset_type",
        audio_asset_type_for_stream(stream),
    );
    set_import_setting_bool(import_settings, "stream", stream);
}

fn import_settings_object_mut(
    import_settings: &mut serde_json::Value,
) -> &mut serde_json::Map<String, serde_json::Value> {
    if !import_settings.is_object() {
        *import_settings = serde_json::Value::Object(serde_json::Map::new());
    }
    import_settings
        .as_object_mut()
        .expect("import_settings should be an object after normalization")
}

fn set_import_setting_bool(import_settings: &mut serde_json::Value, key: &str, value: bool) {
    import_settings_object_mut(import_settings)
        .insert(key.to_string(), serde_json::Value::Bool(value));
}

fn set_import_setting_string(import_settings: &mut serde_json::Value, key: &str, value: &str) {
    import_settings_object_mut(import_settings).insert(
        key.to_string(),
        serde_json::Value::String(value.to_string()),
    );
}

fn infer_default_audio_stream(source_key: &str) -> bool {
    let lower = normalize_source_key(source_key);
    Path::new(&lower).components().any(|component| {
        let std::path::Component::Normal(part) = component else {
            return false;
        };
        let Some(part) = part.to_str() else {
            return false;
        };
        part.split(|ch: char| !ch.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .any(|token| matches!(token, "music" | "bgm" | "stream" | "streaming"))
    })
}

fn audio_asset_type_for_stream(stream: bool) -> &'static str {
    if stream {
        "music_track"
    } else {
        "sound_clip"
    }
}

fn stream_for_audio_asset_type(asset_type: &str) -> Option<bool> {
    match asset_type {
        "music_track" => Some(true),
        "sound_clip" => Some(false),
        _ => None,
    }
}
