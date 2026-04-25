use super::*;

impl Live2DModel {
    pub fn canvas_size_units(&self) -> [f32; 2] {
        [self.canvas_width_units, self.canvas_height_units]
    }

    pub fn apply_layout(&mut self, layout: &Live2DLayout) {
        let (render_transform, has_layout_size_override) =
            build_layout_transform(self.canvas_width_units, self.canvas_height_units, *layout);
        self.render_transform = render_transform;
        self.has_layout_size_override = has_layout_size_override;
    }

    pub fn render_matrix(&self) -> [f32; 16] {
        self.render_transform.to_matrix()
    }

    pub fn render_size_units(&self) -> [f32; 2] {
        [
            self.canvas_width_units * self.render_transform.scale_x.abs(),
            self.canvas_height_units * self.render_transform.scale_y.abs(),
        ]
    }

    pub fn render_matrix_for_view(&self, screen_w: f32, screen_h: f32) -> [f32; 16] {
        let fitted_transform = fit_render_transform_for_view(
            self.render_transform,
            self.canvas_width_units,
            self.has_layout_size_override,
            screen_w,
            screen_h,
        );
        multiply_matrices(
            make_aspect_projection(screen_w, screen_h, self.canvas_width_units),
            fitted_transform.to_matrix(),
        )
    }
}
