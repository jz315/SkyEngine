#![allow(unused_imports)]
use super::backdrop::*;
use super::buffers::*;
use super::collect::*;
use super::images::*;
use super::primitives::*;
use super::*;
use eui_neo::expert::NodeId;

#[derive(Clone)]
pub(super) struct TextItem {
    pub(super) id: TextBufferIdentityKey,
    pub(super) text: String,
    pub(super) font: FontRef,
    pub(super) frame: LayoutRect,
    pub(super) clip: LayoutRect,
    pub(super) color: Color,
    pub(super) font_size: f32,
    pub(super) font_weight: i32,
    pub(super) max_width: f32,
    pub(super) wrap: bool,
    pub(super) horizontal_align: HorizontalAlign,
    pub(super) vertical_align: VerticalAlign,
    pub(super) line_height: f32,
}

pub(super) struct TextLayer {
    pub(super) renderer: TextRenderer,
    pub(super) buffers: Vec<Buffer>,
    pub(super) buffer_keys: Vec<TextBufferKey>,
    pub(super) area_keys: Vec<TextAreaKey>,
}

impl TextLayer {
    pub(super) fn new(atlas: &mut TextAtlas, device: &wgpu::Device) -> Self {
        Self {
            renderer: TextRenderer::new(atlas, device, wgpu::MultisampleState::default(), None),
            buffers: Vec::new(),
            buffer_keys: Vec::new(),
            area_keys: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TextBufferIdentityKey(NodeId);

impl TextBufferIdentityKey {
    #[cfg(test)]
    pub(super) fn new(id: impl Into<String>) -> Self {
        Self(NodeId::new(id))
    }

    pub(super) fn from_node(id: NodeId) -> Self {
        Self(id)
    }

    #[cfg(test)]
    pub(super) fn node_id(&self) -> &NodeId {
        &self.0
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct TextBufferKey {
    pub(super) text: String,
    pub(super) font: FontRef,
    pub(super) font_size: u32,
    pub(super) font_weight: i32,
    pub(super) line_height: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) wrap: bool,
    pub(super) horizontal_align: HorizontalAlign,
}

pub(super) struct CachedTextBuffer {
    pub(super) buffer: Buffer,
    pub(super) last_used_frame: u64,
    pub(super) byte_cost: usize,
}

pub(super) struct TextKeyHistory {
    pub(super) seen_count: u8,
    pub(super) last_seen_frame: u64,
}

#[derive(Default)]
pub(super) struct TextIdentityHistory {
    pub(super) last_key: Option<TextBufferKey>,
    pub(super) changed_streak: u8,
    pub(super) stable_streak: u8,
    pub(super) last_seen_frame: u64,
}

pub(super) struct TextBufferMetrics {
    pub(super) font_size: f32,
    pub(super) line_height: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) align: TextAlign,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct TextAreaKey {
    pub(super) left_bits: u32,
    pub(super) top_bits: u32,
    pub(super) bounds: [i32; 4],
    pub(super) color_rgba: u32,
}

impl WgpuRenderer {
    pub fn register_font(&mut self, font: &FontRef, bytes: &[u8], revision: u64) {
        if self
            .font_revisions
            .get(font)
            .is_some_and(|current| *current == revision)
        {
            return;
        }
        if register_font_bytes(
            &mut self.font_system,
            &mut self.registered_fonts,
            &mut self.default_text_family,
            &mut self.default_icon_family,
            font,
            bytes,
        ) {
            self.font_revisions.insert(font.clone(), revision);
            self.invalidate_text_buffers();
        }
    }

    pub fn clear_font(&mut self, font: &FontRef) {
        self.font_revisions.remove(font);
        clear_registered_font(
            &mut self.registered_fonts,
            &mut self.default_text_family,
            &mut self.default_icon_family,
            font,
        );
        self.invalidate_text_buffers();
    }

    fn invalidate_text_buffers(&mut self) {
        self.text_buffer_cache.clear();
        self.text_buffer_cache_bytes = 0;
        self.text_buffer_key_history.clear();
        self.text_buffer_identity_history.clear();
        for layer in &mut self.text_layers {
            layer.buffers.clear();
            layer.buffer_keys.clear();
            layer.area_keys.clear();
        }
    }

    pub(super) fn prepare_text_layers(
        &mut self,
        ctx: &Target<'_>,
        render_ops: &[RenderOp],
        text_items: &[TextItem],
        logical_size: [f32; 2],
    ) {
        let physical_size = ctx.physical_size;
        self.viewport.update(
            ctx.queue,
            Resolution {
                width: physical_size[0].max(1),
                height: physical_size[1].max(1),
            },
        );

        let mut layer_index = 0usize;
        for op in render_ops {
            let RenderOp::Text { start, count } = *op else {
                continue;
            };
            while self.text_layers.len() <= layer_index {
                self.text_layers
                    .push(TextLayer::new(&mut self.atlas, ctx.device));
            }
            let end = start.saturating_add(count).min(text_items.len());
            let layer_items = if start < end {
                &text_items[start..end]
            } else {
                &[]
            };
            self.prepare_text_layer(ctx, layer_index, layer_items, logical_size, physical_size);
            layer_index += 1;
        }
    }

    fn prepare_text_layer(
        &mut self,
        ctx: &Target<'_>,
        layer_index: usize,
        text_items: &[TextItem],
        logical_size: [f32; 2],
        physical_size: [u32; 2],
    ) {
        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo_wgpu.prepare_text_layer");
        let scale_x = physical_size[0] as f32 / logical_size[0].max(1.0);
        let scale_y = physical_size[1] as f32 / logical_size[1].max(1.0);
        let text_scale = scale_x.min(scale_y).max(0.01);
        let default_text_family = self.default_text_family.clone();
        let default_icon_family = self.default_icon_family.clone();
        let previous_keys = self.text_layers[layer_index].buffer_keys.clone();
        let mut buffers_changed = previous_keys.len() != text_items.len();
        let mut replacements = Vec::new();

        for (index, item) in text_items.iter().enumerate() {
            let metrics = text_buffer_metrics(item, logical_size, text_scale, scale_x, scale_y);
            let key = text_buffer_key(item, &metrics);
            if previous_keys.get(index) == Some(&key) {
                continue;
            }
            buffers_changed = true;
            let cache_admitted = text_buffer_cache_admitted(
                &mut self.text_buffer_key_history,
                &mut self.text_buffer_identity_history,
                self.text_buffer_cache_frame,
                &item.id,
                &key,
            );

            let buffer = self.cached_text_buffer(
                &key,
                item,
                &metrics,
                default_text_family.as_deref(),
                default_icon_family.as_deref(),
                cache_admitted,
            );
            replacements.push((index, key, buffer));
        }
        let replacements_len = replacements.len();

        let layer = &mut self.text_layers[layer_index];
        for (index, key, buffer) in replacements {
            if index < layer.buffers.len() {
                layer.buffers[index] = buffer;
                layer.buffer_keys[index] = key;
            } else {
                layer.buffers.push(buffer);
                layer.buffer_keys.push(key);
            }
        }
        layer.buffers.truncate(text_items.len());
        layer.buffer_keys.truncate(text_items.len());

        let mut area_keys = Vec::with_capacity(text_items.len());
        let mut areas = Vec::with_capacity(text_items.len());
        for (buffer, item) in layer.buffers.iter().zip(text_items.iter()) {
            let text_height =
                laid_out_text_height(buffer).unwrap_or(item.font_size * text_scale * 1.25);
            let rect_height = item.frame.height * scale_y;
            let y_offset = match item.vertical_align {
                VerticalAlign::Top => 0.0,
                VerticalAlign::Center => ((rect_height - text_height) * 0.5).max(0.0),
                VerticalAlign::Bottom => (rect_height - text_height).max(0.0),
            };
            let left = item.frame.x * scale_x;
            let top = item.frame.y * scale_y + y_offset;
            let bounds = TextBounds {
                left: (item.clip.x * scale_x).round() as i32,
                top: (item.clip.y * scale_y).round() as i32,
                right: (item.clip.right() * scale_x).round() as i32,
                bottom: (item.clip.bottom() * scale_y).round() as i32,
            };
            let color = multiply_alpha(item.color, 1.0);
            area_keys.push(TextAreaKey {
                left_bits: left.to_bits(),
                top_bits: top.to_bits(),
                bounds: [bounds.left, bounds.top, bounds.right, bounds.bottom],
                color_rgba: packed_color(color),
            });
            areas.push(TextArea {
                buffer,
                left,
                top,
                scale: 1.0,
                bounds,
                default_color: glyph_color(color),
                custom_glyphs: &[],
            });
        }

        if !buffers_changed && layer.area_keys == area_keys {
            return;
        }

        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo_wgpu.text_renderer_prepare");
        {
            match layer.renderer.prepare(
                ctx.device,
                ctx.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                areas,
                &mut self.swash_cache,
            ) {
                Ok(()) => {
                    layer.area_keys = area_keys;
                }
                Err(error) => {
                    layer.area_keys.clear();
                    eprintln!("[eui-neo-wgpu] text prepare failed: {error}");
                }
            }
        }
        let _ = (layer_index, replacements_len, buffers_changed);
    }

    fn cached_text_buffer(
        &mut self,
        key: &TextBufferKey,
        item: &TextItem,
        metrics: &TextBufferMetrics,
        default_text_family: Option<&str>,
        default_icon_family: Option<&str>,
        cache_admitted: bool,
    ) -> Buffer {
        if cache_admitted {
            if let Some(cached) = self.text_buffer_cache.get_mut(key) {
                cached.last_used_frame = self.text_buffer_cache_frame;
                return cached.buffer.clone();
            }
        } else {
            #[cfg(feature = "profile")]
            ::profiling::scope!("eui_neo_wgpu.create_text_buffer");
            return create_text_buffer(
                &mut self.font_system,
                &self.registered_fonts,
                default_text_family,
                default_icon_family,
                item,
                metrics,
            );
        }

        #[cfg(feature = "profile")]
        ::profiling::scope!("eui_neo_wgpu.create_text_buffer");
        let buffer = create_text_buffer(
            &mut self.font_system,
            &self.registered_fonts,
            default_text_family,
            default_icon_family,
            item,
            metrics,
        );
        let cached = CachedTextBuffer {
            buffer: buffer.clone(),
            last_used_frame: self.text_buffer_cache_frame,
            byte_cost: text_buffer_cost(key),
        };
        self.text_buffer_cache_bytes = self
            .text_buffer_cache_bytes
            .saturating_add(cached.byte_cost);
        self.text_buffer_cache.insert(key.clone(), cached);
        buffer
    }

    pub(super) fn evict_text_buffer_cache(&mut self) {
        self.prune_text_buffer_histories();
        if self.text_buffer_cache_bytes <= TEXT_BUFFER_CACHE_BUDGET_BYTES
            && self.text_buffer_cache.len() <= TEXT_BUFFER_CACHE_MAX_ENTRIES
        {
            return;
        }

        let mut entries: Vec<_> = self
            .text_buffer_cache
            .iter()
            .map(|(key, cached)| (key.clone(), cached.last_used_frame))
            .collect();
        entries.sort_by_key(|(_, frame)| *frame);

        for (key, _) in entries {
            if self.text_buffer_cache_bytes <= TEXT_BUFFER_CACHE_BUDGET_BYTES
                && self.text_buffer_cache.len() <= TEXT_BUFFER_CACHE_MAX_ENTRIES
            {
                break;
            }
            if let Some(cached) = self.text_buffer_cache.remove(&key) {
                self.text_buffer_cache_bytes = self
                    .text_buffer_cache_bytes
                    .saturating_sub(cached.byte_cost);
            }
        }
    }

    fn prune_text_buffer_histories(&mut self) {
        let frame = self.text_buffer_cache_frame;
        self.text_buffer_key_history.retain(|_, history| {
            frame.saturating_sub(history.last_seen_frame)
                <= TEXT_BUFFER_CACHE_HISTORY_MAX_AGE_FRAMES
        });
        self.text_buffer_identity_history.retain(|_, history| {
            frame.saturating_sub(history.last_seen_frame)
                <= TEXT_BUFFER_CACHE_HISTORY_MAX_AGE_FRAMES
        });
    }
}

pub(super) fn text_buffer_metrics(
    item: &TextItem,
    logical_size: [f32; 2],
    text_scale: f32,
    scale_x: f32,
    scale_y: f32,
) -> TextBufferMetrics {
    let icon_font = is_icon_font(&item.font);
    let authored_font_size = (item.font_size * text_scale).max(1.0);
    let font_size = if icon_font {
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
    let line_height = if item.line_height > 0.0 {
        item.line_height * text_scale * metrics_scale
    } else {
        authored_font_size * 1.2 * metrics_scale
    };
    let width = if item.frame.width > 0.0 {
        if item.wrap && item.max_width > 0.0 {
            item.max_width.min(item.frame.width)
        } else {
            item.frame.width
        }
    } else {
        logical_size[0]
    };

    TextBufferMetrics {
        font_size,
        line_height,
        width: (width * scale_x).max(1.0),
        height: (item.frame.height * scale_y).max(1.0),
        align: text_align(item.horizontal_align),
    }
}

pub(super) fn text_buffer_key(item: &TextItem, metrics: &TextBufferMetrics) -> TextBufferKey {
    TextBufferKey {
        text: item.text.clone(),
        font: item.font.clone(),
        font_size: metrics.font_size.to_bits(),
        font_weight: item.font_weight,
        line_height: metrics.line_height.to_bits(),
        width: metrics.width.to_bits(),
        height: metrics.height.to_bits(),
        wrap: item.wrap,
        horizontal_align: item.horizontal_align,
    }
}

pub(super) fn text_buffer_cache_admitted(
    key_history: &mut FxHashMap<TextBufferKey, TextKeyHistory>,
    identity_history: &mut FxHashMap<TextBufferIdentityKey, TextIdentityHistory>,
    frame: u64,
    id: &TextBufferIdentityKey,
    key: &TextBufferKey,
) -> bool {
    let identity = identity_history
        .entry(id.clone())
        .or_insert_with(TextIdentityHistory::default);
    let changed = identity.last_key.as_ref() != Some(key);
    if changed {
        identity.changed_streak = identity.changed_streak.saturating_add(1);
        identity.stable_streak = 0;
        identity.last_key = Some(key.clone());
    } else {
        identity.changed_streak = 0;
        identity.stable_streak = identity.stable_streak.saturating_add(1);
    }
    identity.last_seen_frame = frame;

    let key_history = key_history.entry(key.clone()).or_insert(TextKeyHistory {
        seen_count: 0,
        last_seen_frame: frame,
    });
    key_history.seen_count = key_history.seen_count.saturating_add(1);
    key_history.last_seen_frame = frame;

    let volatile = identity.changed_streak >= TEXT_BUFFER_CACHE_VOLATILE_STREAK;
    let repeated_key = key_history.seen_count >= TEXT_BUFFER_CACHE_ADMIT_AFTER_SEEN;
    let stable_identity = !changed && identity.stable_streak > 0;
    repeated_key || (!volatile && stable_identity)
}

pub(super) fn text_buffer_cost(key: &TextBufferKey) -> usize {
    let text_bytes = key.text.len();
    let font_bytes = std::mem::size_of_val(&key.font);
    256usize
        .saturating_add(text_bytes.saturating_mul(4))
        .saturating_add(font_bytes)
}

pub(super) fn create_text_buffer(
    font_system: &mut FontSystem,
    registered_fonts: &FxHashMap<FontRef, RegisteredFont>,
    default_text_family: Option<&str>,
    default_icon_family: Option<&str>,
    item: &TextItem,
    metrics: &TextBufferMetrics,
) -> Buffer {
    let mut buffer = Buffer::new(
        font_system,
        Metrics::new(metrics.font_size, metrics.line_height),
    );
    buffer.set_size(font_system, Some(metrics.width), Some(metrics.height));
    buffer.set_wrap(
        font_system,
        if item.wrap {
            Wrap::WordOrGlyph
        } else {
            Wrap::None
        },
    );
    let attrs = Attrs::new()
        .family(resolve_family(
            &item.font,
            registered_fonts,
            default_text_family,
            default_icon_family,
        ))
        .weight(Weight(resolved_font_weight(&item.font, item.font_weight)));
    buffer.set_text(
        font_system,
        &item.text,
        &attrs,
        Shaping::Advanced,
        Some(metrics.align),
    );
    for line in &mut buffer.lines {
        line.set_align(Some(metrics.align));
    }
    buffer.shape_until_scroll(font_system, false);
    buffer
}

pub(super) fn glyph_color(color: Color) -> glyphon::Color {
    glyphon::Color::rgba(
        channel(color.r),
        channel(color.g),
        channel(color.b),
        channel(color.a),
    )
}

pub(super) fn text_align(align: HorizontalAlign) -> TextAlign {
    match align {
        HorizontalAlign::Left => TextAlign::Left,
        HorizontalAlign::Center => TextAlign::Center,
        HorizontalAlign::Right => TextAlign::End,
    }
}

pub(super) fn laid_out_text_height(buffer: &Buffer) -> Option<f32> {
    buffer
        .layout_runs()
        .map(|run| run.line_top + run.line_height)
        .reduce(f32::max)
}
