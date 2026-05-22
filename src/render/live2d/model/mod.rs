//! Safe wrapper around the Cubism SDK Core.
//!
//! `Live2DModel` owns the moc and model memory, provides safe access to
//! drawables (vertices, UVs, indices, masks, render order, opacity, blend mode),
//! and handles the per-frame update cycle.

mod drawables;
mod layout;
mod parameters;
#[cfg(test)]
mod tests;

use std::alloc::{self, Layout};
use std::os::raw::{c_float, c_void};
use std::ptr;

use cubism_sys::*;
use rustc_hash::FxHashMap;

use crate::math::{Mat4, Vec3};

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

/// Optional placement/layout values read from `.model3.json`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Live2DLayout {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub center_x: Option<f32>,
    pub center_y: Option<f32>,
    pub top: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
    pub right: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
struct RenderTransform {
    scale_x: f32,
    scale_y: f32,
    translate_x: f32,
    translate_y: f32,
}

impl RenderTransform {
    fn to_matrix(self) -> [f32; 16] {
        (Mat4::from_translation(Vec3::new(self.translate_x, self.translate_y, 0.0))
            * Mat4::from_scale(Vec3::new(self.scale_x, self.scale_y, 1.0)))
        .to_cols_array()
    }
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
    /// Number of offscreen objects in this model.
    offscreen_count: usize,

    /// Cached sorted drawable indices (by render order).
    sorted_drawables: Vec<usize>,
    /// Cached sorted objects (drawables + offscreens) by render order.
    sorted_objects: Vec<Live2DRenderObject>,
    /// Canvas width in Live2D logical units.
    canvas_width_units: f32,
    /// Canvas height in Live2D logical units.
    canvas_height_units: f32,
    /// Layout/model transform applied before projection.
    render_transform: RenderTransform,
    /// Whether `.model3.json` provided an explicit width/height override.
    has_layout_size_override: bool,
    /// Static parent part index for each part.
    part_parent_indices: Vec<i32>,
    /// Static offscreen attachment index for each part.
    part_offscreen_indices: Vec<i32>,
    /// Static parent part index for each drawable.
    drawable_parent_part_indices: Vec<i32>,
    /// Static owner part index for each offscreen.
    offscreen_owner_indices: Vec<i32>,
    /// Cached recursive descendant drawables for each part.
    part_descendant_drawables: Vec<Vec<usize>>,
    /// Saved parameter snapshot used by higher-level motion/runtime layering.
    saved_parameter_values: Vec<f32>,
    /// Motion-driven model opacity from `Model/Opacity` curves.
    model_opacity: f32,
    /// Renderer-style model tint color.
    model_color: [f32; 4],
    /// Number of virtual parameter slots reserved for part IDs.
    part_virtual_parameter_count: usize,
    /// Synthetic parameter slots used for part-ID driven pose / opacity logic.
    virtual_parameter_indices: FxHashMap<String, usize>,
    /// Stable insertion order for synthetic parameter IDs.
    virtual_parameter_ids: Vec<String>,
    /// Backing values for synthetic parameter slots.
    virtual_parameter_values: Vec<f32>,
}

/// Ordered render object returned by `sorted_render_objects()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Live2DRenderObject {
    Drawable(usize),
    Offscreen(usize),
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

            if csmHasMocConsistency(moc_buf as *mut c_void, moc_bytes.len() as u32) == 0 {
                alloc::dealloc(moc_buf, moc_layout);
                return Err("csmHasMocConsistency failed — invalid .moc3 data".into());
            }

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
            let offscreen_count = csmGetOffscreenCount(model) as usize;

            // Initial update to populate drawable data
            csmUpdateModel(model);

            let (canvas_width_units, canvas_height_units) = read_canvas_size_units(model);
            let render_transform =
                default_render_transform(canvas_width_units, canvas_height_units);
            let static_relations = read_static_relations(model, drawable_count, offscreen_count);
            let part_descendant_drawables = build_part_descendant_drawables(
                &static_relations.part_parent_indices,
                &static_relations.drawable_parent_part_indices,
            );

            let mut this = Self {
                moc_buf,
                moc_layout,
                moc,
                model_buf,
                model_layout,
                model,
                drawable_count,
                offscreen_count,
                sorted_drawables: Vec::with_capacity(drawable_count),
                sorted_objects: Vec::with_capacity(drawable_count + offscreen_count),
                canvas_width_units,
                canvas_height_units,
                render_transform,
                has_layout_size_override: false,
                part_parent_indices: static_relations.part_parent_indices,
                part_offscreen_indices: static_relations.part_offscreen_indices,
                drawable_parent_part_indices: static_relations.drawable_parent_part_indices,
                offscreen_owner_indices: static_relations.offscreen_owner_indices,
                part_descendant_drawables,
                saved_parameter_values: Vec::new(),
                model_opacity: 1.0,
                model_color: [1.0, 1.0, 1.0, 1.0],
                part_virtual_parameter_count: 0,
                virtual_parameter_indices: FxHashMap::default(),
                virtual_parameter_ids: Vec::new(),
                virtual_parameter_values: Vec::new(),
            };

            this.initialize_virtual_part_parameters();
            this.refresh_sorted_drawables();
            this.save_parameters();

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
}

struct StaticRelations {
    part_parent_indices: Vec<i32>,
    part_offscreen_indices: Vec<i32>,
    drawable_parent_part_indices: Vec<i32>,
    offscreen_owner_indices: Vec<i32>,
}

fn read_static_relations(
    model: *mut csmModel,
    drawable_count: usize,
    offscreen_count: usize,
) -> StaticRelations {
    unsafe {
        let part_count = csmGetPartCount(model) as usize;

        let part_parent_indices =
            std::slice::from_raw_parts(csmGetPartParentPartIndices(model), part_count).to_vec();
        let part_offscreen_indices =
            std::slice::from_raw_parts(csmGetPartOffscreenIndices(model), part_count).to_vec();
        let drawable_parent_part_indices =
            std::slice::from_raw_parts(csmGetDrawableParentPartIndices(model), drawable_count)
                .to_vec();
        let offscreen_owner_indices = if offscreen_count == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(csmGetOffscreenOwnerIndices(model), offscreen_count).to_vec()
        };

        StaticRelations {
            part_parent_indices,
            part_offscreen_indices,
            drawable_parent_part_indices,
            offscreen_owner_indices,
        }
    }
}

fn build_part_descendant_drawables(
    part_parent_indices: &[i32],
    drawable_parent_part_indices: &[i32],
) -> Vec<Vec<usize>> {
    let part_count = part_parent_indices.len();
    let mut child_parts = vec![Vec::new(); part_count];
    let mut direct_drawables = vec![Vec::new(); part_count];

    for (part_index, parent_index) in part_parent_indices.iter().copied().enumerate() {
        if parent_index >= 0 {
            child_parts[parent_index as usize].push(part_index);
        }
    }
    for (drawable_index, parent_index) in drawable_parent_part_indices.iter().copied().enumerate() {
        if parent_index >= 0 {
            direct_drawables[parent_index as usize].push(drawable_index);
        }
    }

    let mut descendant_drawables = vec![Vec::new(); part_count];
    let mut visited = vec![false; part_count];
    for part_index in 0..part_count {
        collect_part_descendant_drawables(
            part_index,
            &child_parts,
            &direct_drawables,
            &mut descendant_drawables,
            &mut visited,
        );
    }

    descendant_drawables
}

fn collect_part_descendant_drawables(
    part_index: usize,
    child_parts: &[Vec<usize>],
    direct_drawables: &[Vec<usize>],
    descendant_drawables: &mut [Vec<usize>],
    visited: &mut [bool],
) {
    if visited[part_index] {
        return;
    }
    visited[part_index] = true;

    let mut collected = direct_drawables[part_index].clone();
    for &child_part in &child_parts[part_index] {
        collect_part_descendant_drawables(
            child_part,
            child_parts,
            direct_drawables,
            descendant_drawables,
            visited,
        );
        collected.extend_from_slice(&descendant_drawables[child_part]);
    }
    descendant_drawables[part_index] = collected;
}

fn blend_mode_from_raw(raw: i32) -> BlendMode {
    match raw & 0xFF {
        1 | 3 | 4 => BlendMode::Additive,
        2 | 6 => BlendMode::Multiplicative,
        _ => BlendMode::Normal,
    }
}

fn read_canvas_size_units(model: *mut csmModel) -> (f32, f32) {
    unsafe {
        let mut size = csmVector2::default();
        let mut origin = csmVector2::default();
        let mut pixels_per_unit: c_float = 0.0;
        csmReadCanvasInfo(model, &mut size, &mut origin, &mut pixels_per_unit);
        let ppu = pixels_per_unit.max(f32::EPSILON);
        (size.x / ppu, size.y / ppu)
    }
}

fn default_render_transform(canvas_width_units: f32, canvas_height_units: f32) -> RenderTransform {
    let _ = canvas_width_units;
    let scale = 2.0 / canvas_height_units.max(f32::EPSILON);
    RenderTransform {
        scale_x: scale,
        scale_y: scale,
        translate_x: 0.0,
        translate_y: 0.0,
    }
}

fn build_layout_transform(
    canvas_width_units: f32,
    canvas_height_units: f32,
    layout: Live2DLayout,
) -> (RenderTransform, bool) {
    let mut transform = default_render_transform(canvas_width_units, canvas_height_units);
    let mut has_layout_size_override = false;

    if let Some(width) = layout.width {
        let scale = width / canvas_width_units.max(f32::EPSILON);
        transform.scale_x = scale;
        transform.scale_y = scale;
        has_layout_size_override = true;
    }
    if let Some(height) = layout.height {
        let scale = height / canvas_height_units.max(f32::EPSILON);
        transform.scale_x = scale;
        transform.scale_y = scale;
        has_layout_size_override = true;
    }

    if let Some(x) = layout.x {
        transform.translate_x = x;
    }
    if let Some(y) = layout.y {
        transform.translate_y = y;
    }
    if let Some(center_x) = layout.center_x {
        transform.translate_x = center_x - canvas_width_units * transform.scale_x * 0.5;
    }
    if let Some(center_y) = layout.center_y {
        transform.translate_y = center_y - canvas_height_units * transform.scale_y * 0.5;
    }
    if let Some(top) = layout.top {
        transform.translate_y = top;
    }
    if let Some(bottom) = layout.bottom {
        transform.translate_y = bottom - canvas_height_units * transform.scale_y;
    }
    if let Some(left) = layout.left {
        transform.translate_x = left;
    }
    if let Some(right) = layout.right {
        transform.translate_x = right - canvas_width_units * transform.scale_x;
    }

    (transform, has_layout_size_override)
}

fn fit_render_transform_for_view(
    mut transform: RenderTransform,
    canvas_width_units: f32,
    has_layout_size_override: bool,
    screen_w: f32,
    screen_h: f32,
) -> RenderTransform {
    if !has_layout_size_override && screen_w < screen_h && canvas_width_units > 1.0 + f32::EPSILON {
        let scale = 2.0 / canvas_width_units.max(f32::EPSILON);
        transform.scale_x = scale;
        transform.scale_y = scale;
    }
    transform
}

fn make_aspect_projection(screen_w: f32, screen_h: f32, canvas_width_units: f32) -> [f32; 16] {
    let aspect = if screen_w > screen_h {
        (screen_h / screen_w.max(f32::EPSILON), 1.0)
    } else if canvas_width_units > 1.0 + f32::EPSILON {
        (1.0, screen_w / screen_h.max(f32::EPSILON))
    } else {
        (1.0, screen_w / screen_h.max(f32::EPSILON))
    };

    Mat4::from_scale(Vec3::new(aspect.0, aspect.1, 1.0)).to_cols_array()
}

fn multiply_matrices(lhs: [f32; 16], rhs: [f32; 16]) -> [f32; 16] {
    (Mat4::from_cols_array(lhs) * Mat4::from_cols_array(rhs)).to_cols_array()
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
