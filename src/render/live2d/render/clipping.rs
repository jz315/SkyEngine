//! Clipping / masking system for Live2D models.
//!
//! Ported from SakuraEngine's `live2d_clipping.cpp`, which itself is a
//! GPU-independent reimplementation of the Cubism SDK's masking system.
//!
//! Key concepts:
//! - Each clipping context groups drawables that share the same mask set.
//! - Masks are rendered into a single RGBA texture, with each channel
//!   holding one (or more) mask regions.
//! - The manager calculates layout bounds and matrix transforms for
//!   mapping drawable clip-space coordinates to mask texture UVs.

use crate::render::live2d::model::Live2DModel;

/// Mask texture resolution (same as SakuraEngine/Cubism default).
pub const MASK_RESOLUTION: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClippingObjectKind {
    Drawable,
    Offscreen,
}

/// A single clipping context — represents a group of drawables
/// that share the same set of mask drawables.
#[derive(Debug)]
pub struct ClippingContext {
    /// Indices of drawables used as masks.
    pub mask_drawable_indices: Vec<usize>,
    /// Indices of clipped objects.
    ///
    /// For a drawable clipping manager these are drawable indices; for an
    /// offscreen clipping manager these are offscreen indices.
    pub clipped_object_indices: Vec<usize>,
    /// Which RGBA channel this context uses (0=R, 1=G, 2=B, 3=A).
    pub channel_index: usize,
    /// Layout sub-region within the mask texture: `[x, y, w, h]` in 0..1 range.
    ///
    /// **Convention note**: `y` here is in NDC-aligned (Y-up) coordinates, not
    /// standard UV (Y-down). This matches how `mask_matrix` and `draw_matrix`
    /// are constructed — both consume `ly` with the same convention, so the
    /// mask write/read positions are consistent. The base_color NDC conversion
    /// in `renderer.rs` also uses this same convention.
    pub layout_bounds: [f32; 4],
    /// 4×4 matrix mapping model coords → NDC [-1,1] for mask-pass rasterization.
    pub mask_matrix: [f32; 16],
    /// 4×4 matrix mapping model coords → UV [0,1] (Y-flipped) for model-pass mask sampling.
    pub draw_matrix: [f32; 16],
}

impl ClippingContext {
    fn new(mask_indices: Vec<usize>) -> Self {
        Self {
            mask_drawable_indices: mask_indices,
            clipped_object_indices: Vec::new(),
            channel_index: 0,
            layout_bounds: [0.0, 0.0, 1.0, 1.0],
            mask_matrix: identity_matrix(),
            draw_matrix: identity_matrix(),
        }
    }
}

/// Manages all clipping contexts for a Live2D model.
pub struct ClippingManager {
    kind: ClippingObjectKind,
    /// All distinct clipping contexts.
    pub contexts: Vec<ClippingContext>,
    /// Mapping: drawable index → clipping context index (None if no mask).
    pub drawable_to_context: Vec<Option<usize>>,
    /// Mapping: offscreen index → clipping context index (None if no mask).
    pub offscreen_to_context: Vec<Option<usize>>,
    /// Channel flags for each of the 4 RGBA channels.
    pub channel_flags: [[f32; 4]; 4],
}

impl ClippingManager {
    /// Initialize clipping contexts from model mask data.
    ///
    /// Follows the same deduplication logic as SakuraEngine:
    /// drawables that share the exact same mask set are grouped together.
    pub fn new(model: &Live2DModel) -> Self {
        Self::new_with_kind(model, ClippingObjectKind::Drawable)
    }

    pub fn new_for_offscreens(model: &Live2DModel) -> Self {
        Self::new_with_kind(model, ClippingObjectKind::Offscreen)
    }

    fn new_with_kind(model: &Live2DModel, kind: ClippingObjectKind) -> Self {
        let drawable_count = model.drawable_count();
        let offscreen_count = model.offscreen_count();
        let mut contexts: Vec<ClippingContext> = Vec::new();
        let mut drawable_to_context = vec![None; drawable_count];
        let mut offscreen_to_context = vec![None; offscreen_count];

        let object_count = match kind {
            ClippingObjectKind::Drawable => drawable_count,
            ClippingObjectKind::Offscreen => offscreen_count,
        };

        for object_index in 0..object_count {
            let mask_indices = match kind {
                ClippingObjectKind::Drawable => model.drawable_mask_indices(object_index),
                ClippingObjectKind::Offscreen => model.offscreen_mask_indices(object_index),
            };
            if mask_indices.is_empty() {
                continue;
            }

            // Check if an existing context has the same mask set
            let existing = contexts
                .iter()
                .position(|ctx| same_mask_set(&ctx.mask_drawable_indices, &mask_indices));

            let ctx_index = match existing {
                Some(idx) => idx,
                None => {
                    let idx = contexts.len();
                    contexts.push(ClippingContext::new(mask_indices.clone()));
                    idx
                }
            };

            contexts[ctx_index]
                .clipped_object_indices
                .push(object_index);
            match kind {
                ClippingObjectKind::Drawable => drawable_to_context[object_index] = Some(ctx_index),
                ClippingObjectKind::Offscreen => {
                    offscreen_to_context[object_index] = Some(ctx_index)
                }
            }
        }

        let channel_flags = [
            [1.0, 0.0, 0.0, 0.0], // R
            [0.0, 1.0, 0.0, 0.0], // G
            [0.0, 0.0, 1.0, 0.0], // B
            [0.0, 0.0, 0.0, 1.0], // A
        ];

        let mut mgr = Self {
            kind,
            contexts,
            drawable_to_context,
            offscreen_to_context,
            channel_flags,
        };

        mgr.setup_layout();
        mgr
    }

    /// Returns true if there are any clipping contexts (i.e., the model uses masks).
    pub fn has_masks(&self) -> bool {
        !self.contexts.is_empty()
    }

    #[inline]
    pub fn kind(&self) -> ClippingObjectKind {
        self.kind
    }

    /// Set up the channel and layout bounds for all contexts.
    ///
    /// Follows Cubism Framework's one-mask-texture packing strategy:
    /// contexts are assigned per RGBA channel, and each channel gets only as
    /// many sub-rects as it needs. For example, 5 contexts become
    /// `[2, 1, 1, 1]`, not four channels all using half-width rects.
    fn setup_layout(&mut self) {
        let count = self.contexts.len();
        if count == 0 {
            return;
        }

        const COLOR_CHANNEL_COUNT: usize = 4;
        const MAX_RECTS_PER_CHANNEL: usize = 9;

        debug_assert!(
            count <= COLOR_CHANNEL_COUNT * MAX_RECTS_PER_CHANNEL,
            "Live2D supports up to 36 clipping contexts per mask texture"
        );

        let base_count_per_channel = count / COLOR_CHANNEL_COUNT;
        let extra_channel_count = count % COLOR_CHANNEL_COUNT;
        let mut context_index = 0;

        for channel_index in 0..COLOR_CHANNEL_COUNT {
            let layout_count =
                base_count_per_channel + usize::from(channel_index < extra_channel_count);

            for rect_index in 0..layout_count.min(MAX_RECTS_PER_CHANNEL) {
                let Some(ctx) = self.contexts.get_mut(context_index) else {
                    return;
                };
                ctx.channel_index = channel_index;
                ctx.layout_bounds = layout_bounds_for_channel_rect(layout_count, rect_index);
                context_index += 1;
            }
        }
    }

    /// Update mask matrices for the current frame.
    ///
    /// Called each frame before rendering masks. Generates two matrices per
    /// clipping context (matching SakuraEngine's approach):
    ///
    /// - `mask_matrix`: maps model coords → NDC [-1,1] for mask-pass rasterization.
    /// - `draw_matrix`: maps model coords → UV [0,1] with Y-flip for model-pass
    ///   mask texture sampling (compensates for NDC→framebuffer Y inversion).
    ///
    /// The bounding box is computed from **clipped drawables** (the things being
    /// masked, e.g. pupils), not from mask drawables (e.g. eye whites).
    pub fn update_matrices(&mut self, model: &Live2DModel) {
        for ctx in &mut self.contexts {
            // Compute bounding box of all CLIPPED drawable vertices
            // (matching SakuraEngine's CalcClippedDrawTotalBounds)
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;

            for &clipped_idx in &ctx.clipped_object_indices {
                match self.kind {
                    ClippingObjectKind::Drawable => {
                        let positions = model.drawable_vertex_positions(clipped_idx);
                        for pos in positions {
                            min_x = min_x.min(pos[0]);
                            min_y = min_y.min(pos[1]);
                            max_x = max_x.max(pos[0]);
                            max_y = max_y.max(pos[1]);
                        }
                    }
                    ClippingObjectKind::Offscreen => {
                        for &drawable_idx in model.offscreen_child_drawables(clipped_idx) {
                            let positions = model.drawable_vertex_positions(drawable_idx);
                            for pos in positions {
                                min_x = min_x.min(pos[0]);
                                min_y = min_y.min(pos[1]);
                                max_x = max_x.max(pos[0]);
                                max_y = max_y.max(pos[1]);
                            }
                        }
                    }
                }
            }

            if min_x >= max_x || min_y >= max_y {
                ctx.mask_matrix = identity_matrix();
                ctx.draw_matrix = identity_matrix();
                continue;
            }

            // Expand bounds slightly (5% margin, same as SakuraEngine)
            let margin = 0.05;
            let w = max_x - min_x;
            let h = max_y - min_y;
            min_x -= w * margin;
            min_y -= h * margin;
            max_x += w * margin;
            max_y += h * margin;

            let lx = ctx.layout_bounds[0];
            let ly = ctx.layout_bounds[1];
            let lw = ctx.layout_bounds[2];
            let lh = ctx.layout_bounds[3];

            let sx = lw / (max_x - min_x);
            let sy = lh / (max_y - min_y);

            // ── mask_matrix: model coords → NDC [-1,1] ─────────────────
            // Transform chain (column-major, mat * vec):
            //   Translate(-1,-1) * Scale(2,2) * Translate(lx,ly) * Scale(sx,sy) * Translate(-min_x,-min_y)
            // Result: positions map to [-1,1] for correct rasterization into the mask texture.
            ctx.mask_matrix = [
                2.0 * sx,
                0.0,
                0.0,
                0.0,
                0.0,
                2.0 * sy,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
                -2.0 * min_x * sx + 2.0 * lx - 1.0,
                -2.0 * min_y * sy + 2.0 * ly - 1.0,
                0.0,
                1.0,
            ];

            // ── draw_matrix: model coords → UV [0,1] with Y-flip ───────
            // Compensates for NDC→framebuffer Y inversion:
            //   NDC Y=+1 → framebuffer row 0 → UV v=0
            //   NDC Y=-1 → framebuffer row H → UV v=1
            // So: u = (ndc_x+1)/2,  v = (1-ndc_y)/2
            // Equivalent to: Translate(0.5,0.5) * Scale(0.5,-0.5) * mask_matrix
            ctx.draw_matrix = [
                sx,
                0.0,
                0.0,
                0.0,
                0.0,
                -sy,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
                -min_x * sx + lx,
                min_y * sy + 1.0 - ly,
                0.0,
                1.0,
            ];
        }
    }
}

fn identity_matrix() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn layout_bounds_for_channel_rect(layout_count: usize, rect_index: usize) -> [f32; 4] {
    if layout_count <= 1 {
        return [0.0, 0.0, 1.0, 1.0];
    }
    if layout_count == 2 {
        let x = (rect_index % 2) as f32 * 0.5;
        return [x, 0.0, 0.5, 1.0];
    }
    if layout_count <= 4 {
        let x = (rect_index % 2) as f32 * 0.5;
        let y = (rect_index / 2) as f32 * 0.5;
        return [x, y, 0.5, 0.5];
    }

    let x = (rect_index % 3) as f32 / 3.0;
    let y = (rect_index / 3) as f32 / 3.0;
    [x, y, 1.0 / 3.0, 1.0 / 3.0]
}

fn same_mask_set(lhs: &[usize], rhs: &[usize]) -> bool {
    if lhs.len() != rhs.len() {
        return false;
    }
    lhs.iter().all(|mask| rhs.contains(mask))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager_with_context_count(count: usize) -> ClippingManager {
        let contexts = (0..count)
            .map(|index| ClippingContext::new(vec![index]))
            .collect();
        let mut manager = ClippingManager {
            kind: ClippingObjectKind::Drawable,
            contexts,
            drawable_to_context: Vec::new(),
            offscreen_to_context: Vec::new(),
            channel_flags: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        };
        manager.setup_layout();
        manager
    }

    fn assert_bounds(actual: [f32; 4], expected: [f32; 4]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((actual - expected).abs() < 0.0001);
        }
    }

    #[test]
    fn five_clip_contexts_match_cubism_channel_packing() {
        let manager = manager_with_context_count(5);

        assert_eq!(manager.contexts[0].channel_index, 0);
        assert_bounds(manager.contexts[0].layout_bounds, [0.0, 0.0, 0.5, 1.0]);
        assert_eq!(manager.contexts[1].channel_index, 0);
        assert_bounds(manager.contexts[1].layout_bounds, [0.5, 0.0, 0.5, 1.0]);

        assert_eq!(manager.contexts[2].channel_index, 1);
        assert_bounds(manager.contexts[2].layout_bounds, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(manager.contexts[3].channel_index, 2);
        assert_bounds(manager.contexts[3].layout_bounds, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(manager.contexts[4].channel_index, 3);
        assert_bounds(manager.contexts[4].layout_bounds, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn nine_clip_contexts_give_only_first_channel_a_two_by_two_grid() {
        let manager = manager_with_context_count(9);

        assert_eq!(manager.contexts[0].channel_index, 0);
        assert_bounds(manager.contexts[0].layout_bounds, [0.0, 0.0, 0.5, 0.5]);
        assert_eq!(manager.contexts[1].channel_index, 0);
        assert_bounds(manager.contexts[1].layout_bounds, [0.5, 0.0, 0.5, 0.5]);
        assert_eq!(manager.contexts[2].channel_index, 0);
        assert_bounds(manager.contexts[2].layout_bounds, [0.0, 0.5, 0.5, 0.5]);

        assert_eq!(manager.contexts[3].channel_index, 1);
        assert_bounds(manager.contexts[3].layout_bounds, [0.0, 0.0, 0.5, 1.0]);
        assert_eq!(manager.contexts[4].channel_index, 1);
        assert_bounds(manager.contexts[4].layout_bounds, [0.5, 0.0, 0.5, 1.0]);
    }
}
