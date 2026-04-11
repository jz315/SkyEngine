use crate::render::light::Light2D;

use super::super::scene_cache::SceneCache2D;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct GpuSpriteRecord {
    pub transform: [f32; 4],
    pub rotation: [f32; 4],
    pub color: [f32; 4],
    pub uv_rect: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct GpuLightRecord {
    pub pos_radius: [f32; 4],
    pub color: [f32; 4],
    pub falloff: [f32; 4],
}

pub(super) fn sprite_record_for_slot(scene: &SceneCache2D, slot: usize) -> GpuSpriteRecord {
    let Some(item) = scene.sprite_item(slot) else {
        return GpuSpriteRecord::default();
    };
    let (sin_a, cos_a) = item.transform.rotation_z().sin_cos();
    GpuSpriteRecord {
        transform: [
            item.transform.x(),
            item.transform.y(),
            item.sprite.width * item.transform.scale_x(),
            item.sprite.height * item.transform.scale_y(),
        ],
        rotation: [sin_a, cos_a, item.transform.z(), 0.0],
        color: item.sprite.color.to_array(),
        uv_rect: item.sprite.uv,
    }
}

pub(super) fn light_record_for_slot(scene: &SceneCache2D, slot: usize) -> GpuLightRecord {
    let Some(item) = scene.light_item(slot) else {
        return GpuLightRecord::default();
    };
    let light = Light2D::new(item.transform.x(), item.transform.y(), item.light.radius)
        .intensity(item.light.intensity)
        .color(item.light.color)
        .temperature(item.light.temperature)
        .falloff(item.light.falloff);
    GpuLightRecord {
        pos_radius: [light.position[0], light.position[1], light.radius, 0.0],
        color: light.effective_color(),
        falloff: [light.falloff.max(0.001), 50.0, 0.0, 0.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::EntityId;
    use crate::render::domains::sprite::{SceneCache2D, SceneSpriteItem};
    use crate::render::{OrderInLayer, SortingLayer, SpriteRenderer, Transform};

    #[test]
    fn sprite_record_preserves_world_z_for_projection() {
        let mut scene = SceneCache2D::new();
        scene.upsert_sprite(
            EntityId::new(1, 0),
            SceneSpriteItem {
                transform: Transform::from_xyz(12.0, -3.0, -7.5)
                    .with_scale(2.0, 3.0)
                    .with_rotation(0.25),
                sprite: SpriteRenderer::new(10.0, 20.0),
                sorting_layer: SortingLayer(2),
                order_in_layer: OrderInLayer(5),
                sort_key: 1,
                texture_sort_key: 0,
            },
            1,
        );

        let record = sprite_record_for_slot(&scene, 0);
        assert_eq!(record.transform, [12.0, -3.0, 20.0, 60.0]);
        assert_eq!(record.rotation[2], -7.5);
        assert!(record.rotation[0].is_finite());
        assert!(record.rotation[1].is_finite());
    }
}
