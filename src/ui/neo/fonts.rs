use std::path::{Path, PathBuf};

use glyphon::{Family, FontSystem};

pub(crate) const DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE: f32 = 1000.0 / 1300.0;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LoadedEuiFonts {
    pub(crate) text: bool,
    pub(crate) icon: bool,
}

pub(crate) fn load_default_eui_fonts(font_system: &mut FontSystem) -> LoadedEuiFonts {
    let mut loaded = LoadedEuiFonts::default();
    for dir in eui_asset_dirs() {
        if !loaded.text {
            loaded.text = load_font_file(
                font_system,
                dir.join("JingNanJunJunTi-JinNanJunJunTi-Bold-2.ttf"),
            );
        }
        if !loaded.icon {
            loaded.icon =
                load_font_file(font_system, dir.join("Font Awesome 7 Free-Solid-900.otf"));
        }
        if loaded.text && loaded.icon {
            break;
        }
    }
    loaded
}

pub(crate) fn resolve_family<'a>(
    requested: &'a str,
    default_text_family: Option<&'a str>,
    default_icon_family: Option<&'a str>,
) -> Family<'a> {
    if requested.is_empty() {
        return default_text_family.map_or(Family::SansSerif, Family::Name);
    }
    if is_icon_family(requested) {
        return default_icon_family.map_or(Family::Name(requested), Family::Name);
    }
    Family::Name(requested)
}

pub(crate) fn is_icon_family(requested: &str) -> bool {
    matches!(
        requested,
        "Icon"
            | "FontAwesome"
            | "Font Awesome"
            | "Font Awesome 7 Free"
            | "Font Awesome 7 Free Solid"
    )
}

pub(crate) fn resolved_font_weight(
    requested_family: &str,
    requested_weight: i32,
    icon_font: bool,
) -> u16 {
    if icon_font {
        return 900;
    }
    if requested_family.is_empty() {
        return 700;
    }
    requested_weight.clamp(1, u16::MAX as i32) as u16
}

fn eui_asset_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for key in ["SKY_NEO_ASSET_DIR", "EUI_NEO_ASSET_DIR"] {
        if let Some(value) = std::env::var_os(key) {
            push_existing_dir(&mut dirs, PathBuf::from(value));
        }
    }
    if let Ok(current_dir) = std::env::current_dir() {
        push_existing_dir(&mut dirs, current_dir.join("assets"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            push_existing_dir(&mut dirs, parent.join("assets"));
            push_existing_dir(&mut dirs, parent.join("..").join("assets"));
        }
    }
    dirs
}

fn push_existing_dir(dirs: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_dir() && !dirs.iter().any(|dir| dir == &path) {
        dirs.push(path);
    }
}

fn load_font_file(font_system: &mut FontSystem, path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    let Ok(data) = std::fs::read(path) else {
        return false;
    };
    font_system.db_mut().load_font_data(data);
    true
}
