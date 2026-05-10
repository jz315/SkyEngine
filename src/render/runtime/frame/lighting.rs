use crate::ecs::World;
use crate::render::component::{DirectionalLight, PointLight, SpotLight, Transform};
use crate::render::lighting::Light2D;
use crate::render::GpuLight;

pub(crate) fn collect_gpu_lights(
    world: &World,
    transforms: &crate::render::view::ResolvedSceneTransforms,
) -> Vec<GpuLight> {
    let mut lights = Vec::new();
    let mut point_lights = world.query::<(&Transform, &PointLight)>();
    point_lights.for_each_with_entity(world, |entity, (transform, light)| {
        if !light.visible {
            return;
        }
        let transform = transforms.get(entity).unwrap_or(*transform);
        let light = Light2D::new(transform.x(), transform.y(), light.radius)
            .intensity(light.intensity)
            .color(light.color)
            .temperature(light.temperature)
            .falloff(light.falloff);
        lights.push(GpuLight {
            pos_radius: [
                light.position[0],
                light.position[1],
                transform.z(),
                light.radius,
            ],
            color: light.effective_color(),
            falloff: [light.falloff.max(0.001), 0.0, 0.0, 0.0],
            dir_shadow: [0.0, 0.0, 0.0, -1.0],
        });
    });
    let mut spot_lights = world.query::<(&Transform, &SpotLight)>();
    spot_lights.for_each_with_entity(world, |entity, (transform, light)| {
        if !light.visible {
            return;
        }
        let transform = transforms.get(entity).unwrap_or(*transform);
        let direction = normalized_or(light.direction, [0.0, -1.0, 0.0]);
        let [inner_cos, outer_cos] = light.resolved_cone_cosines();
        let light_2d = Light2D::new(transform.x(), transform.y(), light.radius)
            .intensity(light.intensity)
            .color(light.color)
            .temperature(light.temperature)
            .falloff(light.falloff);
        lights.push(GpuLight {
            pos_radius: [
                light_2d.position[0],
                light_2d.position[1],
                transform.z(),
                light_2d.radius,
            ],
            color: light_2d.effective_color(),
            falloff: [light.falloff.max(0.001), 2.0, inner_cos, outer_cos],
            dir_shadow: [direction[0], direction[1], direction[2], -1.0],
        });
    });
    let mut directional_lights = world.query::<&DirectionalLight>();
    directional_lights.for_each(world, |light| {
        if !light.visible {
            return;
        }
        let dir = normalized_or(light.direction, [0.0, -1.0, 0.0]);
        lights.push(GpuLight {
            pos_radius: [dir[0], dir[1], dir[2], 0.0],
            color: [
                light.color.r * light.intensity,
                light.color.g * light.intensity,
                light.color.b * light.intensity,
                light.color.a,
            ],
            falloff: [0.0, 1.0, 0.0, 0.0],
            dir_shadow: [0.0, 0.0, 0.0, -1.0],
        });
    });
    lights
}

fn normalized_or(direction: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let len_sq =
        direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2];
    if len_sq <= f32::EPSILON {
        fallback
    } else {
        let inv_len = len_sq.sqrt().recip();
        [
            direction[0] * inv_len,
            direction[1] * inv_len,
            direction[2] * inv_len,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::render::component::{DirectionalLight, PointLight, SpotLight};
    use crate::render::view::ResolvedSceneTransforms;
    use crate::render::{Color, GpuLightKind};

    #[test]
    fn collect_gpu_lights_uploads_spot_cone_records() {
        let mut world = World::new();
        world.spawn((
            Transform::from_xyz(1.0, 2.0, 3.0),
            PointLight::new(4.0).intensity(0.5),
        ));
        world.spawn((
            Transform::from_xyz(-1.0, 6.0, 2.0),
            SpotLight::new(9.0)
                .color(Color::rgb(0.5, 0.75, 1.0))
                .direction([0.0, -2.0, 0.0])
                .cone_angles(0.25, 0.5),
        ));
        world.spawn((DirectionalLight::new([0.0, -1.0, 0.0]),));

        let lights = collect_gpu_lights(&world, &ResolvedSceneTransforms::default());

        assert_eq!(lights.len(), 3);
        assert_eq!(lights[0].kind(), GpuLightKind::Point);
        assert_eq!(lights[1].kind(), GpuLightKind::Spot);
        assert_eq!(lights[1].pos_radius, [-1.0, 6.0, 2.0, 9.0]);
        assert_eq!(lights[1].dir_shadow, [0.0, -1.0, 0.0, -1.0]);
        assert!(lights[1].falloff[2] > lights[1].falloff[3]);
        assert_eq!(lights[2].kind(), GpuLightKind::Directional);
    }
}
