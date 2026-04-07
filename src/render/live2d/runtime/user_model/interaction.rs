use super::*;

impl Live2DUserModel {
    pub fn set_drag(&mut self, x: f32, y: f32) -> bool {
        self.look.as_mut().is_some_and(|look| {
            look.set_target(x, y);
            true
        })
    }

    pub fn clear_drag(&mut self) -> bool {
        self.set_drag(0.0, 0.0)
    }

    pub fn set_lip_sync(&mut self, value: f32) -> bool {
        self.lip_sync.as_mut().is_some_and(|lip_sync| {
            lip_sync.set_value(value);
            true
        })
    }

    pub fn clear_lip_sync(&mut self) -> bool {
        self.set_lip_sync(0.0)
    }

    pub fn screen_to_model(
        &self,
        screen_position: [f32; 2],
        view_size: [u32; 2],
    ) -> Option<[f32; 2]> {
        let width = view_size[0].max(1) as f32;
        let height = view_size[1].max(1) as f32;
        let matrix = self.model.render_matrix_for_view(width, height);
        let scale_x = matrix[0];
        let scale_y = matrix[5];
        if scale_x.abs() <= f32::EPSILON || scale_y.abs() <= f32::EPSILON {
            return None;
        }

        let ndc_x = screen_position[0] / width * 2.0 - 1.0;
        let ndc_y = 1.0 - screen_position[1] / height * 2.0;
        Some([
            (ndc_x - matrix[12]) / scale_x,
            (ndc_y - matrix[13]) / scale_y,
        ])
    }

    pub fn hit_area_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.hit_areas.iter().map(|area| area.name.as_str())
    }

    pub fn hit_test(&self, area_name: &str, point: [f32; 2]) -> bool {
        let Some(hit_area) = self.hit_areas.iter().find(|area| area.name == area_name) else {
            return false;
        };
        self.model.hit_test_drawable(hit_area.drawable_index, point)
    }

    pub fn hit_test_screen(
        &self,
        area_name: &str,
        screen_position: [f32; 2],
        view_size: [u32; 2],
    ) -> bool {
        self.screen_to_model(screen_position, view_size)
            .is_some_and(|point| self.hit_test(area_name, point))
    }

    pub fn handle_tap_model_space(&mut self, point: [f32; 2]) -> bool {
        if self.hit_test_any_name(&["Head", "HitAreaHead"], point) {
            return self.set_random_expression();
        }
        if self.hit_test_any_name(&["Body", "HitAreaBody"], point) {
            return self.start_random_motion("TapBody", MotionPriority::Normal);
        }
        false
    }

    pub fn handle_tap_screen(&mut self, screen_position: [f32; 2], view_size: [u32; 2]) -> bool {
        self.screen_to_model(screen_position, view_size)
            .is_some_and(|point| self.handle_tap_model_space(point))
    }

    fn hit_test_any_name(&self, names: &[&str], point: [f32; 2]) -> bool {
        names.iter().any(|name| {
            self.hit_areas.iter().any(|area| {
                area.name.eq_ignore_ascii_case(name)
                    && self.model.hit_test_drawable(area.drawable_index, point)
            })
        })
    }
}
