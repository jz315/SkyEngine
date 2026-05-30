//! wgpu renderer support for `eui-neo`.

mod fonts;
mod renderer;
mod shaders;

pub use renderer::{
    compose_fullscreen_shader, create_backdrop_texture_bind_group_layout,
    create_fullscreen_pipeline, create_image_pipeline, create_image_texture_bind_group_layout,
    create_polygon_pipeline, create_rect_pipeline, create_screen_bind_group,
    create_screen_bind_group_layout, create_screen_buffer, draw_fullscreen_triangle,
    image_vertex_layout, polygon_vertex_layout, rect_vertex_layout, GpuImage, ImagePixels,
    ImageState, NeoWgpuResources, NeoWgpuShaders, NoResources, RenderStatus, Resources,
    ScreenUniform, Target, TargetTexture, WgpuRenderer,
};
pub use shaders::{
    default_neo_wgpu_shaders, NEO_CAPTURE_SHADER, NEO_IMAGE_SHADER, NEO_POLYGON_SHADER,
    NEO_RECT_SHADER,
};

/// Rect/background vertex ABI consumed by the Neo rect WGSL shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NeoRectVertex {
    pub position: [f32; 2],
    pub local_pos: [f32; 2],
    pub rect: [f32; 4],
    pub fill: [f32; 4],
    pub gradient_start: [f32; 4],
    pub gradient_end: [f32; 4],
    pub border: [f32; 4],
    pub params: [f32; 4],
    pub flags: [f32; 4],
    pub clip_rect: [f32; 4],
    pub clip_params: [f32; 4],
}

/// Polygon vertex ABI consumed by the Neo polygon WGSL shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NeoPolygonVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub clip_rect: [f32; 4],
    pub clip_params: [f32; 4],
}

/// Image vertex ABI consumed by the Neo image WGSL shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct NeoImageVertex {
    pub position: [f32; 2],
    pub local_pos: [f32; 2],
    pub rect: [f32; 4],
    pub uv: [f32; 2],
    pub tint: [f32; 4],
    pub params: [f32; 4],
    pub clip_rect: [f32; 4],
    pub clip_params: [f32; 4],
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_layout_strides_match_vertex_types() {
        assert_eq!(
            rect_vertex_layout().array_stride,
            std::mem::size_of::<NeoRectVertex>() as u64
        );
        assert_eq!(
            polygon_vertex_layout().array_stride,
            std::mem::size_of::<NeoPolygonVertex>() as u64
        );
        assert_eq!(
            image_vertex_layout().array_stride,
            std::mem::size_of::<NeoImageVertex>() as u64
        );
    }

    #[test]
    fn fullscreen_shader_composition_includes_vertex_entry() {
        let shader = compose_fullscreen_shader(
            "@fragment fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> { return vec4<f32>(in.uv, 0.0, 1.0); }",
        );

        assert!(shader.contains("fn vs_fullscreen"));
        assert!(shader.contains("fn fs_main"));
    }

    #[test]
    fn default_shader_bundle_contains_expected_entries() {
        let shaders = default_neo_wgpu_shaders();

        assert!(shaders.rect.contains("fn vs_main"));
        assert!(shaders.rect.contains("fn fs_main"));
        assert!(shaders.polygon.contains("fn vs_main"));
        assert!(shaders.image.contains("fn fs_main"));
        assert!(shaders.capture.contains("FullscreenOutput"));
    }
}
