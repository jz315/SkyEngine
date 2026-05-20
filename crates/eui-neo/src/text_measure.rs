use std::cell::RefCell;

use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, Weight, Wrap};
use rustc_hash::FxHashMap;

use super::fonts::{
    is_icon_font, load_font_bytes, resolve_family, resolved_font_weight, FontRef, RegisteredFont,
    DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE,
};

/// Text measurement request expressed in logical pixels.
#[derive(Debug, Clone, Copy)]
pub struct TextMeasureRequest<'a> {
    pub text: &'a str,
    pub font: &'a FontRef,
    pub font_size: f32,
    pub font_weight: i32,
    pub line_height: f32,
    pub max_width: f32,
    pub wrap: bool,
}

/// Logical text bounds returned by a [`TextSystem`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextMeasure {
    pub width: f32,
    pub height: f32,
}

/// Host-neutral text service used by layout.
pub trait TextSystem {
    fn register_font(&mut self, font: &FontRef, bytes: &[u8]);

    fn measure(&mut self, request: TextMeasureRequest<'_>) -> TextMeasure;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MeasureKey {
    value: String,
    font: FontRef,
    font_size_bits: u32,
    font_weight: i32,
    line_height_bits: u32,
    max_width_bits: u32,
    wrap: bool,
}

/// Default text system backed by `cosmic-text` and platform font discovery.
#[derive(Debug)]
pub struct DefaultTextSystem {
    font_system: FontSystem,
    registered: FxHashMap<FontRef, RegisteredFont>,
    default_text_family: Option<String>,
    default_icon_family: Option<String>,
    cache: FxHashMap<MeasureKey, TextMeasure>,
}

impl Default for DefaultTextSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultTextSystem {
    pub fn new() -> Self {
        Self {
            font_system: FontSystem::new(),
            registered: FxHashMap::default(),
            default_text_family: None,
            default_icon_family: None,
            cache: FxHashMap::default(),
        }
    }

}

impl TextSystem for DefaultTextSystem {
    fn register_font(&mut self, font: &FontRef, bytes: &[u8]) {
        let Some(loaded) = load_font_bytes(&mut self.font_system, bytes) else {
            return;
        };
        match font {
            FontRef::DefaultText => {
                self.font_system
                    .db_mut()
                    .set_sans_serif_family(loaded.family.clone());
                self.font_system
                    .db_mut()
                    .set_serif_family(loaded.family.clone());
                self.default_text_family = Some(loaded.family.clone());
            }
            FontRef::DefaultIcon => {
                self.default_icon_family = Some(loaded.family.clone());
            }
            FontRef::Family(_) | FontRef::Source(_) => {}
        }
        self.registered.insert(font.clone(), loaded);
        self.cache.clear();
    }

    fn measure(&mut self, request: TextMeasureRequest<'_>) -> TextMeasure {
        if request.text.is_empty() {
            return TextMeasure::default();
        }
        let font_size = request.font_size.max(1.0);
        let key = MeasureKey {
            value: request.text.to_string(),
            font: request.font.clone(),
            font_size_bits: font_size.to_bits(),
            font_weight: request.font_weight,
            line_height_bits: request.line_height.max(0.0).to_bits(),
            max_width_bits: request.max_width.max(0.0).to_bits(),
            wrap: request.wrap,
        };
        if let Some(measure) = self.cache.get(&key).copied() {
            return measure;
        }
        let registered = &self.registered;
        let measure = measure_shaped_text(
            &mut self.font_system,
            |font| registered.get(font),
            self.default_text_family.as_deref(),
            self.default_icon_family.as_deref(),
            TextMeasureRequest {
                font_size,
                ..request
            },
        );
        if self.cache.len() > 4096 {
            self.cache.clear();
        }
        self.cache.insert(key, measure);
        measure
    }
}

thread_local! {
    static DEFAULT_TEXT_SYSTEM: RefCell<DefaultTextSystem> = RefCell::new(DefaultTextSystem::new());
}

/// Measure text width using the default text system and a font family name.
pub fn measure_text_width(value: &str, font_family: &str, font_size: f32, font_weight: i32) -> f32 {
    measure_text_width_with_font(
        value,
        &FontRef::family(font_family),
        font_size,
        font_weight,
    )
}

/// Measure text width using the default text system and an explicit font ref.
pub fn measure_text_width_with_font(
    value: &str,
    font: &FontRef,
    font_size: f32,
    font_weight: i32,
) -> f32 {
    DEFAULT_TEXT_SYSTEM.with(|system| {
        system
            .borrow_mut()
            .measure(TextMeasureRequest {
                text: value,
                font,
                font_size,
                font_weight,
                line_height: 0.0,
                max_width: 0.0,
                wrap: false,
            })
            .width
    })
}

pub(crate) fn measure_text_size_with_system(
    system: &mut dyn TextSystem,
    request: TextMeasureRequest<'_>,
) -> TextMeasure {
    system.measure(request)
}

pub(crate) fn with_default_text_system<R>(f: impl FnOnce(&mut DefaultTextSystem) -> R) -> R {
    DEFAULT_TEXT_SYSTEM.with(|system| f(&mut system.borrow_mut()))
}

fn measure_shaped_text<'a>(
    font_system: &mut FontSystem,
    registered: impl Fn(&'a FontRef) -> Option<&'a RegisteredFont>,
    default_text_family: Option<&'a str>,
    default_icon_family: Option<&'a str>,
    request: TextMeasureRequest<'a>,
) -> TextMeasure {
    let icon_font = is_icon_font(request.font);
    let authored_font_size = request.font_size.max(1.0);
    let shaped_font_size = if icon_font {
        authored_font_size
    } else {
        authored_font_size * DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE
    }
    .max(1.0);
    let metrics_scale = if icon_font {
        1.0
    } else {
        DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE
    };
    let resolved_line_height = if request.line_height > 0.0 {
        request.line_height * metrics_scale
    } else {
        authored_font_size * 1.2 * metrics_scale
    }
    .max(shaped_font_size);
    let width_limit = if request.wrap && request.max_width > 0.0 {
        request.max_width
    } else {
        measurement_width_limit(request.text, shaped_font_size)
    };
    let height_limit = (resolved_line_height * request.text.lines().count().max(1) as f32 * 4.0)
        .max(resolved_line_height)
        .max(4096.0);
    let mut buffer = Buffer::new(
        font_system,
        Metrics::new(shaped_font_size, resolved_line_height),
    );
    buffer.set_size(
        font_system,
        Some(width_limit.max(1.0)),
        Some(height_limit),
    );
    buffer.set_wrap(
        font_system,
        if request.wrap && request.max_width > 0.0 {
            Wrap::WordOrGlyph
        } else {
            Wrap::None
        },
    );
    let attrs = Attrs::new()
        .family(resolve_family(
            request.font,
            registered,
            default_text_family,
            default_icon_family,
        ))
        .weight(Weight(resolved_font_weight(
            request.font,
            request.font_weight,
        )));
    buffer.set_text(font_system, request.text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    let mut width = 0.0_f32;
    let mut height = 0.0_f32;
    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        height = height.max(run.line_top + run.line_height);
    }
    if width <= 0.0 {
        width = fallback_text_width(request.text, authored_font_size);
    }
    if height <= 0.0 {
        height = resolved_line_height;
    }
    TextMeasure { width, height }
}

fn measurement_width_limit(value: &str, font_size: f32) -> f32 {
    let count = value.chars().count().max(1) as f32;
    (count * font_size * 4.0)
        .max(font_size * 8.0)
        .min(1_000_000.0)
}

fn fallback_text_width(value: &str, font_size: f32) -> f32 {
    value.chars().count() as f32 * font_size * 0.5
}
