use std::cell::RefCell;

use glyphon::{Attrs, Buffer, FontSystem, Metrics, Shaping, Weight, Wrap};
use rustc_hash::FxHashMap;

use super::fonts::{
    is_icon_family, load_default_eui_fonts, resolve_family, resolved_font_weight,
    DEFAULT_TEXT_FONT_PIXEL_HEIGHT_SCALE,
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct MeasuredText {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MeasureKey {
    value: String,
    font_family: String,
    font_size_bits: u32,
    font_weight: i32,
}

thread_local! {
    static TEXT_MEASURE_CONTEXT: RefCell<TextMeasureContext> = RefCell::new(TextMeasureContext::new());
    static TEXT_MEASURE_CACHE: RefCell<FxHashMap<MeasureKey, f32>> = RefCell::new(FxHashMap::default());
}

struct TextMeasureContext {
    font_system: FontSystem,
    default_text_family: Option<String>,
    default_icon_family: Option<String>,
}

impl TextMeasureContext {
    fn new() -> Self {
        let mut font_system = FontSystem::new();
        let loaded = load_default_eui_fonts(&mut font_system);
        if loaded.text {
            font_system.db_mut().set_sans_serif_family("JinNanJunJunTi");
            font_system.db_mut().set_serif_family("JinNanJunJunTi");
        }
        Self {
            font_system,
            default_text_family: loaded.text.then(|| "JinNanJunJunTi".to_string()),
            default_icon_family: loaded.icon.then(|| "Font Awesome 7 Free".to_string()),
        }
    }
}

/// Measure text width with the same font family, weight resolution, shaping,
/// and default text scale used by the neo renderer.
pub fn measure_text_width(value: &str, font_family: &str, font_size: f32, font_weight: i32) -> f32 {
    if value.is_empty() {
        return 0.0;
    }
    let font_size = font_size.max(1.0);
    let key = MeasureKey {
        value: value.to_string(),
        font_family: font_family.to_string(),
        font_size_bits: font_size.to_bits(),
        font_weight,
    };
    if let Some(width) = TEXT_MEASURE_CACHE.with(|cache| cache.borrow().get(&key).copied()) {
        return width;
    }

    let width = TEXT_MEASURE_CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        measure_shaped_text(
            &mut context,
            value,
            font_family,
            font_size,
            font_weight,
            0.0,
            0.0,
            false,
        )
        .width
    });

    TEXT_MEASURE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(key, width);
    });
    width
}

pub(crate) fn measure_text_size(
    value: &str,
    font_family: &str,
    font_size: f32,
    font_weight: i32,
    line_height: f32,
    max_width: f32,
    wrap: bool,
) -> MeasuredText {
    TEXT_MEASURE_CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        measure_shaped_text(
            &mut context,
            value,
            font_family,
            font_size.max(1.0),
            font_weight,
            line_height.max(0.0),
            max_width.max(0.0),
            wrap,
        )
    })
}

fn measure_shaped_text(
    context: &mut TextMeasureContext,
    value: &str,
    font_family: &str,
    font_size: f32,
    font_weight: i32,
    line_height: f32,
    max_width: f32,
    wrap: bool,
) -> MeasuredText {
    let icon_font = is_icon_family(font_family);
    let authored_font_size = font_size.max(1.0);
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
    let resolved_line_height = if line_height > 0.0 {
        line_height * metrics_scale
    } else {
        authored_font_size * 1.2 * metrics_scale
    }
    .max(shaped_font_size);
    let width_limit = if wrap && max_width > 0.0 {
        max_width
    } else {
        measurement_width_limit(value, shaped_font_size)
    };
    let height_limit = (resolved_line_height * value.lines().count().max(1) as f32 * 4.0)
        .max(resolved_line_height)
        .max(4096.0);
    let mut buffer = Buffer::new(
        &mut context.font_system,
        Metrics::new(shaped_font_size, resolved_line_height),
    );
    buffer.set_size(
        &mut context.font_system,
        Some(width_limit.max(1.0)),
        Some(height_limit),
    );
    buffer.set_wrap(
        &mut context.font_system,
        if wrap && max_width > 0.0 {
            Wrap::WordOrGlyph
        } else {
            Wrap::None
        },
    );
    let attrs = Attrs::new()
        .family(resolve_family(
            font_family,
            context.default_text_family.as_deref(),
            context.default_icon_family.as_deref(),
        ))
        .weight(Weight(resolved_font_weight(
            font_family,
            font_weight,
            icon_font,
        )));
    buffer.set_text(
        &mut context.font_system,
        value,
        &attrs,
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(&mut context.font_system, false);
    let mut width = 0.0_f32;
    let mut height = 0.0_f32;
    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        height = height.max(run.line_top + run.line_height);
    }
    if width <= 0.0 {
        width = fallback_text_width(value, authored_font_size);
    }
    if height <= 0.0 {
        height = resolved_line_height;
    }
    MeasuredText { width, height }
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
