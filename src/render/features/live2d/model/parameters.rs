use std::ffi::CStr;

use cubism_sys::*;

use super::*;

impl Live2DModel {
    fn real_parameter_count(&self) -> usize {
        unsafe { csmGetParameterCount(self.model) as usize }
    }

    fn virtual_parameter_index(&self, index: usize) -> Option<usize> {
        let real_count = self.real_parameter_count();
        (index >= real_count).then(|| index - real_count)
    }

    pub(crate) fn initialize_virtual_part_parameters(&mut self) {
        let real_count = self.real_parameter_count();
        for part_index in 0..self.part_count() {
            let id = self.part_id(part_index).to_string();
            if self.virtual_parameter_indices.contains_key(&id) {
                continue;
            }
            let slot_index = real_count + self.virtual_parameter_values.len();
            self.virtual_parameter_indices
                .insert(id.clone(), slot_index);
            self.virtual_parameter_ids.push(id);
            self.virtual_parameter_values.push(0.0);
        }
        self.part_virtual_parameter_count = self.virtual_parameter_values.len();
    }

    pub(crate) fn ensure_parameter_slot(&mut self, id: &str) -> usize {
        if let Some(index) = self.find_parameter(id) {
            return index;
        }

        let slot_index = self.real_parameter_count() + self.virtual_parameter_values.len();
        self.virtual_parameter_indices
            .insert(id.to_string(), slot_index);
        self.virtual_parameter_ids.push(id.to_string());
        self.virtual_parameter_values.push(0.0);
        slot_index
    }

    pub(crate) fn register_virtual_parameter_ids(&mut self, ids: &[String]) {
        for id in ids {
            let _ = self.ensure_parameter_slot(id);
        }
    }

    pub(crate) fn extra_virtual_parameter_ids(&self) -> &[String] {
        &self.virtual_parameter_ids[self.part_virtual_parameter_count..]
    }

    pub fn parameter_count(&self) -> usize {
        self.real_parameter_count()
    }

    pub fn parameter_id(&self, index: usize) -> &str {
        unsafe {
            let ids = csmGetParameterIds(self.model);
            let c_str = CStr::from_ptr(*ids.add(index));
            c_str.to_str().unwrap_or("???")
        }
    }

    pub fn parameter_values_mut(&mut self) -> &mut [f32] {
        unsafe {
            let ptr = csmGetParameterValues(self.model);
            let count = csmGetParameterCount(self.model) as usize;
            std::slice::from_raw_parts_mut(ptr, count)
        }
    }

    pub fn parameter_defaults(&self) -> &[f32] {
        unsafe {
            let ptr = csmGetParameterDefaultValues(self.model);
            let count = csmGetParameterCount(self.model) as usize;
            std::slice::from_raw_parts(ptr, count)
        }
    }

    pub fn model_opacity(&self) -> f32 {
        self.model_opacity
    }

    pub fn set_model_opacity(&mut self, value: f32) {
        self.model_opacity = value.clamp(0.0, 1.0);
    }

    pub fn model_color(&self) -> [f32; 4] {
        self.model_color
    }

    pub fn set_model_color(&mut self, color: [f32; 4]) {
        self.model_color = color.map(|channel| channel.clamp(0.0, 1.0));
    }

    pub(crate) fn premultiplied_model_color_with_opacity(&self, opacity: f32) -> [f32; 4] {
        let alpha = (self.model_color[3] * opacity).clamp(0.0, 1.0);
        [
            self.model_color[0] * alpha,
            self.model_color[1] * alpha,
            self.model_color[2] * alpha,
            alpha,
        ]
    }

    pub fn load_parameters(&mut self) {
        let count = self.parameter_count();
        if self.saved_parameter_values.len() != count {
            self.save_parameters();
            return;
        }

        unsafe {
            let values = std::slice::from_raw_parts_mut(csmGetParameterValues(self.model), count);
            values.copy_from_slice(&self.saved_parameter_values);
        }
    }

    pub fn save_parameters(&mut self) {
        let count = self.parameter_count();
        unsafe {
            let values = std::slice::from_raw_parts(csmGetParameterValues(self.model), count);
            self.saved_parameter_values.clear();
            self.saved_parameter_values.extend_from_slice(values);
        }
    }

    pub fn parameter_minimum(&self, index: usize) -> f32 {
        assert!(index < self.parameter_count());
        unsafe { *csmGetParameterMinimumValues(self.model).add(index) }
    }

    pub fn parameter_maximum(&self, index: usize) -> f32 {
        assert!(index < self.parameter_count());
        unsafe { *csmGetParameterMaximumValues(self.model).add(index) }
    }

    pub fn parameter_value(&self, index: usize) -> f32 {
        if let Some(virtual_index) = self.virtual_parameter_index(index) {
            return self.virtual_parameter_values[virtual_index];
        }
        assert!(index < self.parameter_count());
        unsafe { *csmGetParameterValues(self.model).add(index) }
    }

    pub fn parameter_repeats(&self, index: usize) -> bool {
        if self.virtual_parameter_index(index).is_some() {
            return false;
        }
        assert!(index < self.parameter_count());
        unsafe { *csmGetParameterRepeats(self.model).add(index) != 0 }
    }

    pub fn find_parameter(&self, id: &str) -> Option<usize> {
        let count = self.parameter_count();
        for i in 0..count {
            if self.parameter_id(i) == id {
                return Some(i);
            }
        }
        self.virtual_parameter_indices.get(id).copied()
    }

    pub fn set_parameter(&mut self, id: &str, value: f32) -> bool {
        if let Some(idx) = self.find_parameter(id) {
            self.set_parameter_by_index(idx, value);
            true
        } else {
            false
        }
    }

    pub fn set_parameter_by_index(&mut self, index: usize, value: f32) {
        if let Some(virtual_index) = self.virtual_parameter_index(index) {
            self.virtual_parameter_values[virtual_index] = value;
            return;
        }
        assert!(index < self.parameter_count());
        let value = self.normalize_real_parameter_value(index, value);
        unsafe {
            *csmGetParameterValues(self.model).add(index) = value;
        }
    }

    pub(crate) fn set_parameter_clamped_no_repeat_by_index(&mut self, index: usize, value: f32) {
        if let Some(virtual_index) = self.virtual_parameter_index(index) {
            self.virtual_parameter_values[virtual_index] = value;
            return;
        }
        assert!(index < self.parameter_count());
        let value = value.clamp(self.parameter_minimum(index), self.parameter_maximum(index));
        unsafe {
            *csmGetParameterValues(self.model).add(index) = value;
        }
    }

    pub fn set_parameter_weighted_by_index(&mut self, index: usize, value: f32, weight: f32) {
        let weight = weight.clamp(0.0, 1.0);
        if let Some(virtual_index) = self.virtual_parameter_index(index) {
            let current = self.virtual_parameter_values[virtual_index];
            self.virtual_parameter_values[virtual_index] = if weight >= 1.0 - f32::EPSILON {
                value
            } else {
                current + (value - current) * weight
            };
            return;
        }
        assert!(index < self.parameter_count());
        let value = self.normalize_real_parameter_value(index, value);

        unsafe {
            let ptr = csmGetParameterValues(self.model).add(index);
            let current = *ptr;
            *ptr = if weight >= 1.0 - f32::EPSILON {
                value
            } else {
                current + (value - current) * weight
            };
        }
    }

    pub fn add_parameter_by_index(&mut self, index: usize, delta: f32) {
        if let Some(virtual_index) = self.virtual_parameter_index(index) {
            self.virtual_parameter_values[virtual_index] += delta;
            return;
        }
        let next = self.parameter_value(index) + delta;
        self.set_parameter_by_index(index, next);
    }

    pub fn add_parameter_weighted_by_index(&mut self, index: usize, delta: f32, weight: f32) {
        let weight = weight.clamp(0.0, 1.0);
        if let Some(virtual_index) = self.virtual_parameter_index(index) {
            self.virtual_parameter_values[virtual_index] += delta * weight;
            return;
        }
        let next = self.parameter_value(index) + delta * weight;
        self.set_parameter_by_index(index, next);
    }

    pub fn multiply_parameter_weighted_by_index(&mut self, index: usize, value: f32, weight: f32) {
        let weight = weight.clamp(0.0, 1.0);
        if let Some(virtual_index) = self.virtual_parameter_index(index) {
            let current = self.virtual_parameter_values[virtual_index];
            self.virtual_parameter_values[virtual_index] = current * (1.0 + (value - 1.0) * weight);
            return;
        }
        let next = self.parameter_value(index) * (1.0 + (value - 1.0) * weight);
        self.set_parameter_by_index(index, next);
    }

    pub fn part_count(&self) -> usize {
        unsafe { csmGetPartCount(self.model) as usize }
    }

    pub fn part_id(&self, index: usize) -> &str {
        unsafe {
            let ids = csmGetPartIds(self.model);
            let c_str = CStr::from_ptr(*ids.add(index));
            c_str.to_str().unwrap_or("???")
        }
    }

    pub fn find_part(&self, id: &str) -> Option<usize> {
        let count = self.part_count();
        (0..count).find(|&index| self.part_id(index) == id)
    }

    pub fn part_opacities_mut(&mut self) -> &mut [f32] {
        unsafe {
            let ptr = csmGetPartOpacities(self.model);
            let count = csmGetPartCount(self.model) as usize;
            std::slice::from_raw_parts_mut(ptr, count)
        }
    }

    pub fn part_opacity(&self, index: usize) -> f32 {
        assert!(index < self.part_count());
        unsafe { *csmGetPartOpacities(self.model).add(index) }
    }

    pub fn set_part_opacity(&mut self, index: usize, value: f32) {
        assert!(index < self.part_count());
        unsafe {
            *csmGetPartOpacities(self.model).add(index) = value;
        }
    }

    pub(crate) fn raw_model_ptr(&self) -> *mut csmModel {
        self.model
    }

    pub fn update(&mut self) {
        unsafe {
            csmUpdateModel(self.model);
            csmResetDrawableDynamicFlags(self.model);
        }
        self.refresh_sorted_drawables();
    }

    fn normalize_real_parameter_value(&self, index: usize, value: f32) -> f32 {
        let minimum = self.parameter_minimum(index);
        let maximum = self.parameter_maximum(index);
        if self.parameter_repeats(index) {
            repeat_parameter_value(value, minimum, maximum)
        } else {
            value.clamp(minimum, maximum)
        }
    }
}

fn repeat_parameter_value(value: f32, minimum: f32, maximum: f32) -> f32 {
    let value_size = maximum - minimum;
    if value_size.abs() <= f32::EPSILON {
        return value.clamp(minimum, maximum);
    }

    if value > maximum {
        let over_value = (value - maximum).rem_euclid(value_size);
        return if over_value.is_nan() {
            maximum
        } else {
            minimum + over_value
        };
    }

    if value < minimum {
        let over_value = (minimum - value).rem_euclid(value_size);
        return if over_value.is_nan() {
            minimum
        } else {
            maximum - over_value
        };
    }

    value
}

#[cfg(test)]
mod parameter_tests {
    use super::repeat_parameter_value;

    #[test]
    fn repeat_parameter_value_wraps_above_range() {
        assert!((repeat_parameter_value(2.2, -1.0, 1.0) - 0.2).abs() < 0.0001);
    }

    #[test]
    fn repeat_parameter_value_wraps_below_range() {
        assert!((repeat_parameter_value(-1.5, -1.0, 1.0) - 0.5).abs() < 0.0001);
    }

    #[test]
    fn repeat_parameter_value_preserves_cycle_boundaries() {
        assert!((repeat_parameter_value(3.0, -1.0, 1.0) - -1.0).abs() < 0.0001);
        assert!((repeat_parameter_value(-3.0, -1.0, 1.0) - 1.0).abs() < 0.0001);
    }
}
