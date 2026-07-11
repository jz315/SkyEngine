use super::*;

pub(crate) fn collect_gi_triangles(
    renderables: &[crate::render::gi::GiRenderable<'_>],
) -> Vec<GpuGiTriangle> {
    let mut triangles = Vec::new();
    for renderable in renderables {
        if !renderable.opaque {
            continue;
        }
        let range = renderable
            .triangle_range
            .start
            .min(renderable.ray_triangles.len())
            ..renderable
                .triangle_range
                .end
                .min(renderable.ray_triangles.len());
        for triangle in &renderable.ray_triangles[range] {
            triangles.push(GpuGiTriangle::from_triangle(
                *triangle,
                renderable.model,
                renderable.material,
            ));
        }
    }
    triangles
}

pub(crate) fn ddgi_origin(settings: DdgiSettings, view: &SceneView) -> [f32; 3] {
    if !settings.volume.scroll_with_main_camera {
        return settings.volume.origin;
    }
    let spacing = settings.volume.spacing.max(0.05);
    let counts = settings.volume.counts.map(|value| value.max(1));
    let snapped_center = [
        (view.camera_position[0] / spacing).round() * spacing,
        (view.camera_position[1] / spacing).round() * spacing,
        (view.camera_position[2] / spacing).round() * spacing,
    ];
    [
        snapped_center[0] - (counts[0].saturating_sub(1)) as f32 * spacing * 0.5,
        snapped_center[1] - (counts[1].saturating_sub(1)) as f32 * spacing * 0.5,
        snapped_center[2] - (counts[2].saturating_sub(1)) as f32 * spacing * 0.5,
    ]
}

pub(crate) fn ddgi_atlas_size(counts: [u32; 3], resolution: u32) -> (u32, u32) {
    let tile_resolution = ddgi_tile_resolution(resolution);
    (
        counts[0].saturating_mul(tile_resolution).max(1),
        counts[1]
            .saturating_mul(counts[2])
            .saturating_mul(tile_resolution)
            .max(1),
    )
}

pub(crate) fn ddgi_tile_resolution(resolution: u32) -> u32 {
    resolution.saturating_add(DDGI_ATLAS_BORDER_TEXELS * 2)
}
