use sky_engine::ecs::{
    dynamic::{DynamicBundle, DynamicQuery, WorldDynamicExt},
    ComponentType, World,
};
pub struct VelocityComponent {
    pub x: f32,
    pub y: f32,
}

pub struct PositionComponent {
    pub x: f32,
    pub y: f32,
}
#[allow(dead_code)]
static mut COUNT: usize = 0;
fn main() {
    let _ty_a: ComponentType = sky_engine::ecs::component_type::<VelocityComponent>();
    let _ty_b: ComponentType = sky_engine::ecs::component_type::<PositionComponent>();

    let mut world = World::new();
    for _ in 0..1024 {
        world
            .spawn_dynamic(
                DynamicBundle::new()
                    .with(VelocityComponent { x: 1.0, y: 2.0 })
                    .with(PositionComponent { x: 0.0, y: 0.0 }),
            )
            .unwrap();
    }

    let mut query = DynamicQuery::builder()
        .write::<PositionComponent>()
        .read::<VelocityComponent>()
        .build()
        .unwrap();

    query
        .for_each_chunk_mut(&mut world, |mut chunk| {
            let (positions, velocities) =
                chunk.write_read::<PositionComponent, VelocityComponent>(0, 1)?;
            for (position, velocity) in positions.iter_mut().zip(velocities) {
                position.x += velocity.x * 0.1 + 1.0;
                position.y += velocity.y * 0.1 + 1.0;
            }
            Ok(())
        })
        .unwrap();
}
