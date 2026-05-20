use std::sync::Arc;

use eui_neo::FontRef;
use glyphon::{fontdb, Family, FontSystem};
use rustc_hash::FxHashMap;

pub(crate) const DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE: f32 = 1000.0 / 1300.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegisteredFont {
    pub(crate) family: String,
}

pub(crate) fn register_font_bytes(
    font_system: &mut FontSystem,
    fonts: &mut FxHashMap<FontRef, RegisteredFont>,
    default_text_family: &mut Option<String>,
    default_icon_family: &mut Option<String>,
    font: &FontRef,
    bytes: &[u8],
) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let ids = font_system
        .db_mut()
        .load_font_source(fontdb::Source::Binary(Arc::new(bytes.to_vec())));
    let Some(family) = ids.into_iter().find_map(|id| {
        font_system
            .db()
            .face(id)
            .and_then(|face| face.families.first())
            .map(|(family, _)| family.clone())
    }) else {
        return false;
    };
    match font {
        FontRef::DefaultText => {
            font_system.db_mut().set_sans_serif_family(family.clone());
            font_system.db_mut().set_serif_family(family.clone());
            *default_text_family = Some(family.clone());
        }
        FontRef::DefaultIcon => {
            *default_icon_family = Some(family.clone());
        }
        FontRef::Family(_) | FontRef::Source(_) => {}
    }
    fonts.insert(font.clone(), RegisteredFont { family });
    true
}

pub(crate) fn clear_registered_font(
    fonts: &mut FxHashMap<FontRef, RegisteredFont>,
    default_text_family: &mut Option<String>,
    default_icon_family: &mut Option<String>,
    font: &FontRef,
) {
    fonts.remove(font);
    match font {
        FontRef::DefaultText => *default_text_family = None,
        FontRef::DefaultIcon => *default_icon_family = None,
        FontRef::Family(_) | FontRef::Source(_) => {}
    }
}

pub(crate) fn resolve_family<'a>(
    font: &'a FontRef,
    registered: &'a FxHashMap<FontRef, RegisteredFont>,
    default_text_family: Option<&'a str>,
    default_icon_family: Option<&'a str>,
) -> Family<'a> {
    match font {
        FontRef::DefaultText => default_text_family.map_or(Family::SansSerif, Family::Name),
        FontRef::DefaultIcon => default_icon_family.map_or(Family::SansSerif, Family::Name),
        FontRef::Family(value) => {
            if value.is_empty() {
                default_text_family.map_or(Family::SansSerif, Family::Name)
            } else if is_icon_family_name(value) {
                default_icon_family.map_or(Family::SansSerif, Family::Name)
            } else {
                Family::Name(value)
            }
        }
        FontRef::Source(_) => registered
            .get(font)
            .map(|font| Family::Name(font.family.as_str()))
            .unwrap_or(Family::SansSerif),
    }
}

pub(crate) fn is_icon_font(font: &FontRef) -> bool {
    matches!(font, FontRef::DefaultIcon)
        || matches!(font, FontRef::Family(value) if is_icon_family_name(value))
}

pub(crate) fn is_icon_family_name(requested: &str) -> bool {
    matches!(
        requested,
        "Icon"
            | "FontAwesome"
            | "Font Awesome"
            | "Font Awesome 7 Free"
            | "Font Awesome 7 Free Solid"
    )
}

pub(crate) fn resolved_font_weight(font: &FontRef, requested_weight: i32) -> u16 {
    if is_icon_font(font) {
        return 900;
    }
    requested_weight.clamp(1, u16::MAX as i32) as u16
}
