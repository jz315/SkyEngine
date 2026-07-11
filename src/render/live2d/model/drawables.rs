use std::ffi::CStr;

use cubism_sys::*;

use super::*;

impl Live2DModel {
    pub fn drawable_count(&self) -> usize {
        self.drawable_count
    }

    pub fn offscreen_count(&self) -> usize {
        self.offscreen_count
    }

    pub fn sorted_drawable_indices(&self) -> &[usize] {
        &self.sorted_drawables
    }

    pub fn sorted_render_objects(&self) -> &[Live2DRenderObject] {
        &self.sorted_objects
    }

    pub fn drawable_id(&self, index: usize) -> &str {
        assert!(index < self.drawable_count);
        unsafe {
            let ids = csmGetDrawableIds(self.model);
            let c_str = CStr::from_ptr(*ids.add(index));
            c_str.to_str().unwrap_or("???")
        }
    }

    pub fn find_drawable(&self, id: &str) -> Option<usize> {
        (0..self.drawable_count).find(|&index| self.drawable_id(index) == id)
    }

    pub fn drawable_bounds(&self, index: usize) -> Option<([f32; 2], [f32; 2])> {
        let positions = self.drawable_vertex_positions(index);
        let first = positions.first()?;

        let mut min = *first;
        let mut max = *first;
        for position in positions.iter().copied().skip(1) {
            min[0] = min[0].min(position[0]);
            min[1] = min[1].min(position[1]);
            max[0] = max[0].max(position[0]);
            max[1] = max[1].max(position[1]);
        }

        Some((min, max))
    }

    pub fn hit_test_drawable(&self, index: usize, point: [f32; 2]) -> bool {
        if self.model_opacity < 1.0 {
            return false;
        }
        if !self.drawable_is_visible(index) || self.drawable_opacity(index) <= f32::EPSILON {
            return false;
        }

        let Some((min, max)) = self.drawable_bounds(index) else {
            return false;
        };
        point[0] >= min[0] && point[0] <= max[0] && point[1] >= min[1] && point[1] <= max[1]
    }

    pub fn drawable_info(&self, index: usize) -> DrawableInfo {
        assert!(index < self.drawable_count);
        DrawableInfo {
            index,
            texture_index: self.drawable_texture_index(index),
            render_order: self.drawable_render_order(index),
            opacity: self.drawable_opacity(index),
            blend_mode: self.drawable_blend_mode(index),
            is_double_sided: self.drawable_is_double_sided(index),
            is_inverted_mask: self.drawable_is_inverted_mask(index),
            mask_indices: self.drawable_mask_indices(index),
            multiply_color: self.drawable_multiply_color(index),
            screen_color: self.drawable_screen_color(index),
        }
    }

    pub fn drawable_is_visible(&self, index: usize) -> bool {
        unsafe {
            let flags = *csmGetDrawableDynamicFlags(self.model).add(index);
            flags & IS_VISIBLE != 0
        }
    }

    pub fn drawable_opacity(&self, index: usize) -> f32 {
        unsafe { *csmGetDrawableOpacities(self.model).add(index) }
    }

    pub fn drawable_render_order(&self, index: usize) -> i32 {
        unsafe { *csmGetRenderOrders(self.model).add(index) }
    }

    pub fn drawable_blend_mode_raw(&self, index: usize) -> i32 {
        unsafe {
            let ptr = csmGetDrawableBlendModes(self.model);
            if ptr.is_null() {
                match self.drawable_blend_mode(index) {
                    BlendMode::Normal => 0,
                    BlendMode::Additive => 1,
                    BlendMode::Multiplicative => 2,
                }
            } else {
                *ptr.add(index)
            }
        }
    }

    pub fn drawable_blend_mode(&self, index: usize) -> BlendMode {
        unsafe {
            let const_flags = *csmGetDrawableConstantFlags(self.model).add(index);
            if const_flags & BLEND_ADDITIVE != 0 {
                BlendMode::Additive
            } else if const_flags & BLEND_MULTIPLICATIVE != 0 {
                BlendMode::Multiplicative
            } else {
                BlendMode::Normal
            }
        }
    }

    pub fn drawable_is_double_sided(&self, index: usize) -> bool {
        unsafe { *csmGetDrawableConstantFlags(self.model).add(index) & IS_DOUBLE_SIDED != 0 }
    }

    pub fn drawable_is_inverted_mask(&self, index: usize) -> bool {
        unsafe { *csmGetDrawableConstantFlags(self.model).add(index) & IS_INVERTED_MASK != 0 }
    }

    pub fn drawable_mask_indices(&self, index: usize) -> Vec<usize> {
        unsafe {
            let mask_count = *csmGetDrawableMaskCounts(self.model).add(index) as usize;
            let mask_ptr = *csmGetDrawableMasks(self.model).add(index);
            if mask_count > 0 && !mask_ptr.is_null() {
                (0..mask_count).map(|i| *mask_ptr.add(i) as usize).collect()
            } else {
                Vec::new()
            }
        }
    }

    pub fn drawable_multiply_color(&self, index: usize) -> [f32; 4] {
        unsafe {
            let color = &*csmGetDrawableMultiplyColors(self.model).add(index);
            [color.x, color.y, color.z, color.w]
        }
    }

    pub fn drawable_screen_color(&self, index: usize) -> [f32; 4] {
        unsafe {
            let color = &*csmGetDrawableScreenColors(self.model).add(index);
            [color.x, color.y, color.z, color.w]
        }
    }

    pub fn drawable_vertex_positions(&self, index: usize) -> &[[f32; 2]] {
        unsafe {
            let count = *csmGetDrawableVertexCounts(self.model).add(index) as usize;
            let ptr = *csmGetDrawableVertexPositions(self.model).add(index);
            if ptr.is_null() || count == 0 {
                return &[];
            }
            std::slice::from_raw_parts(ptr as *const [f32; 2], count)
        }
    }

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

    pub fn drawable_indices(&self, index: usize) -> &[u16] {
        unsafe {
            let count = *csmGetDrawableIndexCounts(self.model).add(index) as usize;
            let ptr = *csmGetDrawableIndices(self.model).add(index);
            if ptr.is_null() || count == 0 {
                return &[];
            }
            std::slice::from_raw_parts(ptr, count)
        }
    }

    pub fn drawable_texture_index(&self, index: usize) -> i32 {
        unsafe { *csmGetDrawableTextureIndices(self.model).add(index) }
    }

    pub fn drawable_parent_part_index(&self, index: usize) -> i32 {
        self.drawable_parent_part_indices[index]
    }

    pub fn part_parent_part_index(&self, index: usize) -> i32 {
        self.part_parent_indices[index]
    }

    pub fn part_offscreen_index(&self, index: usize) -> i32 {
        self.part_offscreen_indices[index]
    }

    pub fn offscreen_owner_index(&self, index: usize) -> i32 {
        self.offscreen_owner_indices[index]
    }

    pub fn offscreen_child_drawables(&self, index: usize) -> &[usize] {
        let owner = self.offscreen_owner_indices[index];
        if owner < 0 {
            &[]
        } else {
            &self.part_descendant_drawables[owner as usize]
        }
    }

    pub fn offscreen_blend_mode_raw(&self, index: usize) -> i32 {
        unsafe {
            let ptr = csmGetOffscreenBlendModes(self.model);
            if ptr.is_null() {
                0
            } else {
                *ptr.add(index)
            }
        }
    }

    pub fn offscreen_blend_mode(&self, index: usize) -> BlendMode {
        blend_mode_from_raw(self.offscreen_blend_mode_raw(index))
    }

    pub fn offscreen_opacity(&self, index: usize) -> f32 {
        unsafe { *csmGetOffscreenOpacities(self.model).add(index) }
    }

    pub fn offscreen_is_double_sided(&self, index: usize) -> bool {
        unsafe { *csmGetOffscreenConstantFlags(self.model).add(index) & IS_DOUBLE_SIDED != 0 }
    }

    pub fn offscreen_is_inverted_mask(&self, index: usize) -> bool {
        unsafe { *csmGetOffscreenConstantFlags(self.model).add(index) & IS_INVERTED_MASK != 0 }
    }

    pub fn offscreen_mask_indices(&self, index: usize) -> Vec<usize> {
        unsafe {
            let mask_count = *csmGetOffscreenMaskCounts(self.model).add(index) as usize;
            let mask_ptr = *csmGetOffscreenMasks(self.model).add(index);
            if mask_count > 0 && !mask_ptr.is_null() {
                (0..mask_count).map(|i| *mask_ptr.add(i) as usize).collect()
            } else {
                Vec::new()
            }
        }
    }

    pub fn offscreen_multiply_color(&self, index: usize) -> [f32; 4] {
        unsafe {
            let color = &*csmGetOffscreenMultiplyColors(self.model).add(index);
            [color.x, color.y, color.z, color.w]
        }
    }

    pub fn offscreen_screen_color(&self, index: usize) -> [f32; 4] {
        unsafe {
            let color = &*csmGetOffscreenScreenColors(self.model).add(index);
            [color.x, color.y, color.z, color.w]
        }
    }

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

    pub(crate) fn refresh_sorted_drawables(&mut self) {
        self.sorted_drawables.clear();
        self.sorted_drawables.resize(self.drawable_count, 0);
        self.sorted_objects.clear();
        self.sorted_objects.resize(
            self.drawable_count + self.offscreen_count,
            Live2DRenderObject::Drawable(0),
        );

        unsafe {
            let render_orders = csmGetRenderOrders(self.model);
            for i in 0..(self.drawable_count + self.offscreen_count) {
                let order = *render_orders.add(i) as usize;
                if i < self.drawable_count {
                    if order < self.drawable_count {
                        self.sorted_drawables[order] = i;
                    }
                    if order < self.sorted_objects.len() {
                        self.sorted_objects[order] = Live2DRenderObject::Drawable(i);
                    }
                } else {
                    let offscreen_index = i - self.drawable_count;
                    if order < self.sorted_objects.len() {
                        self.sorted_objects[order] = Live2DRenderObject::Offscreen(offscreen_index);
                    }
                }
            }
        }
    }
}
