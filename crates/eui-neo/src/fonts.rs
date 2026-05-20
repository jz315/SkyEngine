use std::sync::Arc;

use cosmic_text::{fontdb, Family, FontSystem};

pub(crate) const DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE: f32 = 1000.0 / 1300.0;

/// Host-neutral font reference used by Neo elements and draw commands.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FontRef {
    DefaultText,
    DefaultIcon,
    Family(String),
    Source(String),
}

impl Default for FontRef {
    fn default() -> Self {
        Self::DefaultText
    }
}

impl FontRef {
    pub fn default_text() -> Self {
        Self::DefaultText
    }

    pub fn default_icon() -> Self {
        Self::DefaultIcon
    }

    pub fn family(value: impl Into<String>) -> Self {
        let value = value.into();
        if is_icon_family_name(&value) {
            Self::DefaultIcon
        } else if value.is_empty() {
            Self::DefaultText
        } else {
            Self::Family(value)
        }
    }

    pub fn source(value: impl Into<String>) -> Self {
        Self::Source(value.into())
    }

    pub fn as_family(&self) -> Option<&str> {
        match self {
            Self::Family(value) => Some(value.as_str()),
            _ => None,
        }
    }

    pub fn as_source(&self) -> Option<&str> {
        match self {
            Self::Source(value) => Some(value.as_str()),
            _ => None,
        }
    }

    pub fn is_icon(&self) -> bool {
        matches!(self, Self::DefaultIcon)
            || matches!(self, Self::Family(value) if is_icon_family_name(value))
    }
}

impl From<&str> for FontRef {
    fn from(value: &str) -> Self {
        Self::family(value)
    }
}

impl From<String> for FontRef {
    fn from(value: String) -> Self {
        Self::family(value)
    }
}

/// Font face loaded into a text backend for a specific [`FontRef`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredFont {
    pub family: String,
}

impl RegisteredFont {
    pub fn new(family: impl Into<String>) -> Self {
        Self {
            family: family.into(),
        }
    }
}

pub(crate) fn load_font_bytes(font_system: &mut FontSystem, bytes: &[u8]) -> Option<RegisteredFont> {
    if bytes.is_empty() {
        return None;
    }
    let ids = font_system
        .db_mut()
        .load_font_source(fontdb::Source::Binary(Arc::new(bytes.to_vec())));
    let family = ids.into_iter().find_map(|id| {
        font_system
            .db()
            .face(id)
            .and_then(|face| face.families.first())
            .map(|(family, _)| family.clone())
    })?;
    Some(RegisteredFont::new(family))
}

pub(crate) fn resolve_family<'a>(
    font: &'a FontRef,
    registered: impl Fn(&'a FontRef) -> Option<&'a RegisteredFont>,
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
        FontRef::Source(_) => registered(font)
            .map(|font| Family::Name(font.family.as_str()))
            .unwrap_or(Family::SansSerif),
    }
}

pub(crate) fn is_icon_font(font: &FontRef) -> bool {
    font.is_icon()
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
    if matches!(font, FontRef::DefaultText) {
        return requested_weight.max(1).clamp(1, u16::MAX as i32) as u16;
    }
    requested_weight.clamp(1, u16::MAX as i32) as u16
}
