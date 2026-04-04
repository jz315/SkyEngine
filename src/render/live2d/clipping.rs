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

/// A single clipping context — represents a group of drawables
/// that share the same set of mask drawables.
#[derive(Debug)]
pub struct ClippingContext {
    /// Indices of drawables used as masks.
    pub mask_drawable_indices: Vec<usize>,
    /// Indices of drawables that are clipped by these masks.
    pub clipped_drawable_indices: Vec<usize>,
    /// Which RGBA channel this context uses (0=R, 1=G, 2=B, 3=A).
    pub channel_index: usize,
    /// Layout bounds within the mask texture [x, y, w, h] in 0..1 range.
    pub layout_bounds: [f32; 4],
    /// 4x4 matrix for transforming drawable positions to mask UV space.
    pub mask_matrix: [f32; 16],
}

impl ClippingContext {
    fn new(mask_indices: Vec<usize>) -> Self {
        Self {
            mask_drawable_indices: mask_indices,
            clipped_drawable_indices: Vec::new(),
            channel_index: 0,
            layout_bounds: [0.0, 0.0, 1.0, 1.0],
            mask_matrix: identity_matrix(),
        }
    }
}

/// Manages all clipping contexts for a Live2D model.
pub struct ClippingManager {
    /// All distinct clipping contexts.
    pub contexts: Vec<ClippingContext>,
    /// Mapping: drawable index → clipping context index (None if no mask).
    pub drawable_to_context: Vec<Option<usize>>,
    /// Channel flags for each of the 4 RGBA channels.
    pub channel_flags: [[f32; 4]; 4],
}

impl ClippingManager {
    /// Initialize clipping contexts from model mask data.
    ///
    /// Follows the same deduplication logic as SakuraEngine:
    /// drawables that share the exact same mask set are grouped together.
    pub fn new(model: &Live2DModel) -> Self {
        let drawable_count = model.drawable_count();
        let mut contexts: Vec<ClippingContext> = Vec::new();
        let mut drawable_to_context = vec![None; drawable_count];

        for d in 0..drawable_count {
            let info = model.drawable_info(d);
            if info.mask_indices.is_empty() {
                continue;
            }

            // Check if an existing context has the same mask set
            let existing = contexts
                .iter()
                .position(|ctx| ctx.mask_drawable_indices == info.mask_indices);

            let ctx_index = match existing {
                Some(idx) => idx,
                None => {
                    let idx = contexts.len();
                    contexts.push(ClippingContext::new(info.mask_indices.clone()));
                    idx
                }
            };

            contexts[ctx_index].clipped_drawable_indices.push(d);
            drawable_to_context[d] = Some(ctx_index);
        }

        let channel_flags = [
            [1.0, 0.0, 0.0, 0.0], // R
            [0.0, 1.0, 0.0, 0.0], // G
            [0.0, 0.0, 1.0, 0.0], // B
            [0.0, 0.0, 0.0, 1.0], // A
        ];

        let mut mgr = Self {
            contexts,
            drawable_to_context,
            channel_flags,
        };

        mgr.setup_layout();
        mgr
    }

    /// Returns true if there are any clipping contexts (i.e., the model uses masks).
    pub fn has_masks(&self) -> bool {
        !self.contexts.is_empty()
    }

    /// Set up the channel and layout bounds for all contexts.
    ///
    /// Follows SakuraEngine's layout packing strategy:
    /// - ≤4 contexts → one per RGBA channel, full texture
    /// - 5–8 → 2 sub-rects per channel
    /// - 9–16 → 4 sub-rects per channel (2×2 grid)
    /// - etc.
    fn setup_layout(&mut self) {
        let count = self.contexts.len();
        if count == 0 {
            return;
        }

        // Determine subdivision level
        let (div_x, div_y) = if count <= 4 {
            (1, 1)
        } else if count <= 8 {
            (2, 1)
        } else if count <= 16 {
            (2, 2)
        } else {
            (3, 3) // up to 36 masks
        };

        let rects_per_channel = div_x * div_y;
        let cell_w = 1.0 / div_x as f32;
        let cell_h = 1.0 / div_y as f32;

        for (i, ctx) in self.contexts.iter_mut().enumerate() {
            let channel = i % 4;
            let rect_in_channel = i / 4;

            ctx.channel_index = channel;

            if rect_in_channel < rects_per_channel {
                let col = rect_in_channel % div_x;
                let row = rect_in_channel / div_x;
                ctx.layout_bounds = [col as f32 * cell_w, row as f32 * cell_h, cell_w, cell_h];
            }
        }
    }

    /// Update mask matrices for the current frame.
    ///
    /// Called each frame before rendering masks. Computes the model-to-mask
    /// UV transformation matrix for each clipping context based on the
    /// bounding box of its mask drawables.
    pub fn update_matrices(&mut self, model: &Live2DModel) {
        for ctx in &mut self.contexts {
            // Compute bounding box of all mask drawable vertices
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;

            for &mask_idx in &ctx.mask_drawable_indices {
                let positions = model.drawable_vertex_positions(mask_idx);
                for pos in positions {
                    min_x = min_x.min(pos[0]);
                    min_y = min_y.min(pos[1]);
                    max_x = max_x.max(pos[0]);
                    max_y = max_y.max(pos[1]);
                }
            }

            if min_x >= max_x || min_y >= max_y {
                ctx.mask_matrix = identity_matrix();
                continue;
            }

            // Expand bounds slightly to avoid clipping edge pixels
            let margin = 0.05;
            let w = max_x - min_x;
            let h = max_y - min_y;
            min_x -= w * margin;
            min_y -= h * margin;
            max_x += w * margin;
            max_y += h * margin;

            let scale_x = 1.0 / (max_x - min_x);
            let scale_y = 1.0 / (max_y - min_y);

            // Map model coords → [0,1] UV within the mask texture layout region
            let lx = ctx.layout_bounds[0];
            let ly = ctx.layout_bounds[1];
            let lw = ctx.layout_bounds[2];
            let lh = ctx.layout_bounds[3];

            // Column-major 4×4 matrix:
            // Combines: translate(-min) → scale(1/(max-min)) → scale(layout_size) → translate(layout_offset)
            ctx.mask_matrix = [
                scale_x * lw,
                0.0,
                0.0,
                0.0,
                0.0,
                scale_y * lh,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
                -min_x * scale_x * lw + lx,
                -min_y * scale_y * lh + ly,
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
