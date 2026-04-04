//! Safe wrapper around the Cubism SDK Core.
//!
//! `Live2DModel` owns the moc and model memory, provides safe access to
//! drawables (vertices, UVs, indices, masks, render order, opacity, blend mode),
//! and handles the per-frame update cycle.

use std::alloc::{self, Layout};
use std::ffi::CStr;
use std::os::raw::{c_float, c_void};
use std::ptr;

use cubism_sys::*;

/// Blend mode for a drawable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Additive,
    Multiplicative,
}

/// Per-drawable rendering data extracted from the Cubism Core.
#[derive(Debug, Clone)]
pub struct DrawableInfo {
    pub index: usize,
    pub texture_index: i32,
    pub render_order: i32,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub is_double_sided: bool,
    pub is_inverted_mask: bool,
    pub mask_indices: Vec<usize>,
    pub multiply_color: [f32; 4],
    pub screen_color: [f32; 4],
}

/// Canvas info read from the model.
#[derive(Debug, Clone, Copy)]
pub struct CanvasInfo {
    pub size_pixels: [f32; 2],
    pub origin_pixels: [f32; 2],
    pub pixels_per_unit: f32,
}

/// Safe wrapper around a Live2D Cubism model.
///
/// Owns the aligned moc and model memory. Call [`update()`](Live2DModel::update)
/// each frame after setting parameters, then read drawable data for rendering.
pub struct Live2DModel {
    /// Aligned buffer holding moc data (must live as long as model).
    moc_buf: *mut u8,
    moc_layout: Layout,
    /// Retained to keep the moc memory alive (model references it).
    #[allow(dead_code)]
    moc: *mut csmMoc,

    /// Aligned buffer holding model instance.
    model_buf: *mut u8,
    model_layout: Layout,
    model: *mut csmModel,

    /// Number of drawables in this model.
    drawable_count: usize,

    /// Cached sorted drawable indices (by render order).
    sorted_drawables: Vec<usize>,
}

// Cubism Core is thread-unsafe — model must not be shared across threads.
// We explicitly do NOT implement Send/Sync.

impl Live2DModel {
    /// Create a model from `.moc3` file bytes.
    ///
    /// The bytes are copied into an aligned internal buffer.
    pub fn from_moc3_bytes(moc_bytes: &[u8]) -> Result<Self, String> {
        unsafe {
            // Allocate aligned moc buffer
            let moc_layout = Layout::from_size_align(moc_bytes.len(), ALIGN_OF_MOC)
                .map_err(|e| format!("moc layout error: {e}"))?;
            let moc_buf = alloc::alloc(moc_layout);
            if moc_buf.is_null() {
                return Err("failed to allocate moc buffer".into());
            }
            ptr::copy_nonoverlapping(moc_bytes.as_ptr(), moc_buf, moc_bytes.len());

            // Revive moc
            let moc = csmReviveMocInPlace(moc_buf as *mut c_void, moc_bytes.len() as u32);
            if moc.is_null() {
                alloc::dealloc(moc_buf, moc_layout);
                return Err("csmReviveMocInPlace failed — invalid .moc3 data".into());
            }

            // Allocate model instance
            let model_size = csmGetSizeofModel(moc) as usize;
            if model_size == 0 {
                alloc::dealloc(moc_buf, moc_layout);
                return Err("csmGetSizeofModel returned 0".into());
            }

            let model_layout = Layout::from_size_align(model_size, ALIGN_OF_MODEL)
                .map_err(|e| format!("model layout error: {e}"))?;
            let model_buf = alloc::alloc_zeroed(model_layout);
            if model_buf.is_null() {
                alloc::dealloc(moc_buf, moc_layout);
                return Err("failed to allocate model buffer".into());
            }

            let model = csmInitializeModelInPlace(moc, model_buf as *mut c_void, model_size as u32);
            if model.is_null() {
                alloc::dealloc(model_buf, model_layout);
                alloc::dealloc(moc_buf, moc_layout);
                return Err("csmInitializeModelInPlace failed".into());
            }

            let drawable_count = csmGetDrawableCount(model) as usize;

            // Initial update to populate drawable data
            csmUpdateModel(model);

            let mut this = Self {
                moc_buf,
                moc_layout,
                moc,
                model_buf,
                model_layout,
                model,
                drawable_count,
                sorted_drawables: Vec::with_capacity(drawable_count),
            };

            this.refresh_sorted_drawables();

            Ok(this)
        }
    }

    // ── Canvas ──────────────────────────────────────────────────────────

    /// Read canvas info from the model.
    pub fn canvas_info(&self) -> CanvasInfo {
        unsafe {
            let mut size = csmVector2::default();
            let mut origin = csmVector2::default();
            let mut ppu: c_float = 0.0;
            csmReadCanvasInfo(self.model, &mut size, &mut origin, &mut ppu);
            CanvasInfo {
                size_pixels: [size.x, size.y],
                origin_pixels: [origin.x, origin.y],
                pixels_per_unit: ppu,
            }
        }
    }

    // ── Parameters ──────────────────────────────────────────────────────

    /// Number of parameters.
    pub fn parameter_count(&self) -> usize {
        unsafe { csmGetParameterCount(self.model) as usize }
    }

    /// Get parameter ID string by index.
    pub fn parameter_id(&self, index: usize) -> &str {
        unsafe {
            let ids = csmGetParameterIds(self.model);
            let c_str = CStr::from_ptr(*ids.add(index));
            c_str.to_str().unwrap_or("???")
        }
    }

    /// Get a mutable slice of all parameter values.
    ///
    /// Write to this slice to set parameters before calling [`update()`].
    pub fn parameter_values_mut(&mut self) -> &mut [f32] {
        unsafe {
            let ptr = csmGetParameterValues(self.model);
            let count = csmGetParameterCount(self.model) as usize;
            std::slice::from_raw_parts_mut(ptr, count)
        }
    }

    /// Get the default parameter values.
    pub fn parameter_defaults(&self) -> &[f32] {
        unsafe {
            let ptr = csmGetParameterDefaultValues(self.model);
            let count = csmGetParameterCount(self.model) as usize;
            std::slice::from_raw_parts(ptr, count)
        }
    }

    /// Read a parameter minimum by index.
    pub fn parameter_minimum(&self, index: usize) -> f32 {
        assert!(index < self.parameter_count());
        unsafe { *csmGetParameterMinimumValues(self.model).add(index) }
    }

    /// Read a parameter maximum by index.
    pub fn parameter_maximum(&self, index: usize) -> f32 {
        assert!(index < self.parameter_count());
        unsafe { *csmGetParameterMaximumValues(self.model).add(index) }
    }

    /// Read a parameter value by index.
    pub fn parameter_value(&self, index: usize) -> f32 {
        assert!(index < self.parameter_count());
        unsafe { *csmGetParameterValues(self.model).add(index) }
    }

    /// Find parameter index by ID string.
    pub fn find_parameter(&self, id: &str) -> Option<usize> {
        let count = self.parameter_count();
        for i in 0..count {
            if self.parameter_id(i) == id {
                return Some(i);
            }
        }
        None
    }

    /// Set a parameter value by ID string.
    pub fn set_parameter(&mut self, id: &str, value: f32) -> bool {
        if let Some(idx) = self.find_parameter(id) {
            self.set_parameter_by_index(idx, value);
            true
        } else {
            false
        }
    }

    /// Set a parameter value by index.
    pub fn set_parameter_by_index(&mut self, index: usize, value: f32) {
        assert!(index < self.parameter_count());
        unsafe {
            *csmGetParameterValues(self.model).add(index) = value;
        }
    }

    /// Blend a parameter toward a value by weight, clamped to the model range.
    pub fn set_parameter_weighted_by_index(&mut self, index: usize, value: f32, weight: f32) {
        assert!(index < self.parameter_count());
        let weight = weight.clamp(0.0, 1.0);
        let minimum = self.parameter_minimum(index);
        let maximum = self.parameter_maximum(index);

        unsafe {
            let ptr = csmGetParameterValues(self.model).add(index);
            let current = *ptr;
            let blended = if weight >= 1.0 - f32::EPSILON {
                value
            } else {
                current + (value - current) * weight
            };
            *ptr = blended.clamp(minimum, maximum);
        }
    }

    /// Add to a parameter value by index.
    pub fn add_parameter_by_index(&mut self, index: usize, delta: f32) {
        assert!(index < self.parameter_count());
        unsafe {
            let ptr = csmGetParameterValues(self.model).add(index);
            *ptr += delta;
        }
    }

    /// Add to a parameter by weight, clamped to the model range.
    pub fn add_parameter_weighted_by_index(&mut self, index: usize, delta: f32, weight: f32) {
        assert!(index < self.parameter_count());
        let weight = weight.clamp(0.0, 1.0);
        let minimum = self.parameter_minimum(index);
        let maximum = self.parameter_maximum(index);

        unsafe {
            let ptr = csmGetParameterValues(self.model).add(index);
            let next = *ptr + delta * weight;
            *ptr = next.clamp(minimum, maximum);
        }
    }

    /// Multiply a parameter toward a factor by weight, clamped to the model range.
    pub fn multiply_parameter_weighted_by_index(&mut self, index: usize, value: f32, weight: f32) {
        assert!(index < self.parameter_count());
        let weight = weight.clamp(0.0, 1.0);
        let minimum = self.parameter_minimum(index);
        let maximum = self.parameter_maximum(index);

        unsafe {
            let ptr = csmGetParameterValues(self.model).add(index);
            let next = *ptr * (1.0 + (value - 1.0) * weight);
            *ptr = next.clamp(minimum, maximum);
        }
    }

    // ── Parts ───────────────────────────────────────────────────────────

    /// Number of parts.
    pub fn part_count(&self) -> usize {
        unsafe { csmGetPartCount(self.model) as usize }
    }

    /// Get part ID string by index.
    pub fn part_id(&self, index: usize) -> &str {
        unsafe {
            let ids = csmGetPartIds(self.model);
            let c_str = CStr::from_ptr(*ids.add(index));
            c_str.to_str().unwrap_or("???")
        }
    }

    /// Find part index by ID string.
    pub fn find_part(&self, id: &str) -> Option<usize> {
        let count = self.part_count();
        for i in 0..count {
            if self.part_id(i) == id {
                return Some(i);
            }
        }
        None
    }

    /// Get a mutable slice of part opacities.
    pub fn part_opacities_mut(&mut self) -> &mut [f32] {
        unsafe {
            let ptr = csmGetPartOpacities(self.model);
            let count = csmGetPartCount(self.model) as usize;
            std::slice::from_raw_parts_mut(ptr, count)
        }
    }

    /// Read a part opacity by index.
    pub fn part_opacity(&self, index: usize) -> f32 {
        assert!(index < self.part_count());
        unsafe { *csmGetPartOpacities(self.model).add(index) }
    }

    /// Set a part opacity by index.
    pub fn set_part_opacity(&mut self, index: usize, value: f32) {
        assert!(index < self.part_count());
        unsafe {
            *csmGetPartOpacities(self.model).add(index) = value;
        }
    }

    pub(crate) fn raw_model_ptr(&self) -> *mut csmModel {
        self.model
    }

    // ── Update ──────────────────────────────────────────────────────────

    /// Update the model (evaluate parameters → update drawables).
    ///
    /// Call this each frame after modifying parameter values.
    pub fn update(&mut self) {
        unsafe {
            csmUpdateModel(self.model);
            csmResetDrawableDynamicFlags(self.model);
        }
        self.refresh_sorted_drawables();
    }

    // ── Drawables ───────────────────────────────────────────────────────

    /// Number of drawables in this model.
    #[inline]
    pub fn drawable_count(&self) -> usize {
        self.drawable_count
    }

    /// Get the sorted drawable indices (sorted by render order, ascending).
    pub fn sorted_drawable_indices(&self) -> &[usize] {
        &self.sorted_drawables
    }

    /// Get info for a specific drawable.
    pub fn drawable_info(&self, index: usize) -> DrawableInfo {
        assert!(index < self.drawable_count);
        unsafe {
            let const_flags = *csmGetDrawableConstantFlags(self.model).add(index);
            let texture_index = *csmGetDrawableTextureIndices(self.model).add(index);
            let render_order = *csmGetRenderOrders(self.model).add(index);
            let opacity = *csmGetDrawableOpacities(self.model).add(index);
            let mul_colors = csmGetDrawableMultiplyColors(self.model);
            let scr_colors = csmGetDrawableScreenColors(self.model);

            let blend_mode = if const_flags & BLEND_ADDITIVE != 0 {
                BlendMode::Additive
            } else if const_flags & BLEND_MULTIPLICATIVE != 0 {
                BlendMode::Multiplicative
            } else {
                BlendMode::Normal
            };

            let mask_count = *csmGetDrawableMaskCounts(self.model).add(index) as usize;
            let mask_ptr = *csmGetDrawableMasks(self.model).add(index);
            let mask_indices: Vec<usize> = if mask_count > 0 && !mask_ptr.is_null() {
                (0..mask_count).map(|i| *mask_ptr.add(i) as usize).collect()
            } else {
                Vec::new()
            };

            let mc = &*mul_colors.add(index);
            let sc = &*scr_colors.add(index);

            DrawableInfo {
                index,
                texture_index,
                render_order,
                opacity,
                blend_mode,
                is_double_sided: const_flags & IS_DOUBLE_SIDED != 0,
                is_inverted_mask: const_flags & IS_INVERTED_MASK != 0,
                mask_indices,
                multiply_color: [mc.x, mc.y, mc.z, mc.w],
                screen_color: [sc.x, sc.y, sc.z, sc.w],
            }
        }
    }

    /// Check if a drawable is visible.
    pub fn drawable_is_visible(&self, index: usize) -> bool {
        unsafe {
            let flags = *csmGetDrawableDynamicFlags(self.model).add(index);
            flags & IS_VISIBLE != 0
        }
    }

    /// Get drawable opacity.
    pub fn drawable_opacity(&self, index: usize) -> f32 {
        unsafe { *csmGetDrawableOpacities(self.model).add(index) }
    }

    /// Get vertex positions for a drawable as `[x, y]` pairs.
    pub fn drawable_vertex_positions(&self, index: usize) -> &[[f32; 2]] {
        unsafe {
            let count = *csmGetDrawableVertexCounts(self.model).add(index) as usize;
            let ptr = *csmGetDrawableVertexPositions(self.model).add(index);
            if ptr.is_null() || count == 0 {
                return &[];
            }
            // csmVector2 is { f32, f32 } which is layout-compatible with [f32; 2]
            std::slice::from_raw_parts(ptr as *const [f32; 2], count)
        }
    }

    /// Get texture UVs for a drawable as `[u, v]` pairs.
    pub fn drawable_vertex_uvs(&self, index: usize) -> &[[f32; 2]] {
        unsafe {
            let count = *csmGetDrawableVertexCounts(self.model).add(index) as usize;
            let ptr = *csmGetDrawableVertexUvs(self.model).add(index);
            if ptr.is_null() || count == 0 {
                return &[];
            }
            std::slice::from_raw_parts(ptr as *const [f32; 2], count)
        }
    }

    /// Get triangle indices for a drawable.
    pub fn drawable_indices(&self, index: usize) -> &[u16] {
        unsafe {
            let count = *csmGetDrawableIndexCounts(self.model).add(index) as usize;
            let ptr = *csmGetDrawableIndices(self.model).add(index);
            if ptr.is_null() || count == 0 {
                return &[];
            }
            std::slice::from_raw_parts(ptr as *const u16, count)
        }
    }

    /// Get the texture index for a drawable.
    pub fn drawable_texture_index(&self, index: usize) -> i32 {
        unsafe { *csmGetDrawableTextureIndices(self.model).add(index) }
    }

    /// Get the number of textures referenced by drawables.
    pub fn texture_count(&self) -> usize {
        let mut max_idx = -1i32;
        for i in 0..self.drawable_count {
            let idx = self.drawable_texture_index(i);
            if idx > max_idx {
                max_idx = idx;
            }
        }
        if max_idx < 0 {
            0
        } else {
            (max_idx + 1) as usize
        }
    }

    // ── Internal ────────────────────────────────────────────────────────

    fn refresh_sorted_drawables(&mut self) {
        // Same algorithm as SakuraEngine: invert render_order → index mapping.
        self.sorted_drawables.clear();
        self.sorted_drawables.resize(self.drawable_count, 0);

        unsafe {
            let render_orders = csmGetRenderOrders(self.model);
            for i in 0..self.drawable_count {
                let order = *render_orders.add(i) as usize;
                // render_order is 0..N-1 unique per drawable
                if order < self.drawable_count {
                    self.sorted_drawables[order] = i;
                }
            }
        }
    }
}

impl Drop for Live2DModel {
    fn drop(&mut self) {
        unsafe {
            // Model must be freed before moc (model references moc memory).
            if !self.model_buf.is_null() {
                alloc::dealloc(self.model_buf, self.model_layout);
            }
            if !self.moc_buf.is_null() {
                alloc::dealloc(self.moc_buf, self.moc_layout);
            }
        }
    }
}
