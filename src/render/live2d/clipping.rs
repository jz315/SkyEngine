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
            clipped_drawable_indices: Vec::new(),
            channel_index: 0,
            layout_bounds: [0.0, 0.0, 1.0, 1.0],
            mask_matrix: identity_matrix(),
            draw_matrix: identity_matrix(),
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

            for &clipped_idx in &ctx.clipped_drawable_indices {
                let positions = model.drawable_vertex_positions(clipped_idx);
                for pos in positions {
                    min_x = min_x.min(pos[0]);
                    min_y = min_y.min(pos[1]);
                    max_x = max_x.max(pos[0]);
                    max_y = max_y.max(pos[1]);
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
                2.0 * sx, 0.0, 0.0, 0.0,
                0.0, 2.0 * sy, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                -2.0 * min_x * sx + 2.0 * lx - 1.0,
                -2.0 * min_y * sy + 2.0 * ly - 1.0,
                0.0, 1.0,
            ];

            // ── draw_matrix: model coords → UV [0,1] with Y-flip ───────
            // Compensates for NDC→framebuffer Y inversion:
            //   NDC Y=+1 → framebuffer row 0 → UV v=0
            //   NDC Y=-1 → framebuffer row H → UV v=1
            // So: u = (ndc_x+1)/2,  v = (1-ndc_y)/2
            // Equivalent to: Translate(0.5,0.5) * Scale(0.5,-0.5) * mask_matrix
            ctx.draw_matrix = [
                sx, 0.0, 0.0, 0.0,
                0.0, -sy, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                -min_x * sx + lx,
                min_y * sy + 1.0 - ly,
                0.0, 1.0,
            ];
        }
    }
}

fn identity_matrix() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
