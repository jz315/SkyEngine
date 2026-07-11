use std::collections::HashSet;

use crate::ecs::World;
use crate::render::{MainCamera, Transform};

use super::server::AudioServer;
use super::types::{AudioError, AudioPlaybackSettings};
use super::{AudioEmitter2D, AudioListener2D};

impl AudioServer {
    pub fn sync_world(&self, world: &World) -> Result<(), AudioError> {
        self.consume_asset_events();

        let (listener_position, listener_rotation) = find_listener_pose(world);
        self.set_listener_pose(listener_position, listener_rotation)?;

        let mut seen = HashSet::new();
        let emitters = world.query::<(&Transform, &AudioEmitter2D)>();
        emitters.for_each_with_entity(|entity, (transform, emitter)| {
            if !emitter.enabled || !emitter.autoplay {
                let _ = self.stop_emitter_binding(entity);
                return;
            }

            let settings = AudioPlaybackSettings {
                bus: emitter.bus,
                gain: emitter.gain,
                pitch: emitter.pitch,
                pan: 0.0,
                looped: emitter.looped,
                spatial: Some(emitter.spatial.with_position(transform.x(), transform.y())),
            };

            if self
                .sync_emitter_binding(entity, &emitter.asset, settings)
                .is_ok()
            {
                seen.insert(entity);
            }
        });

        self.finish_emitter_sync(&seen)?;
        Ok(())
    }
}

fn find_listener_pose(world: &World) -> ([f32; 2], f32) {
    let explicit = world.query::<(&AudioListener2D, &Transform)>();
    let mut result = None;
    explicit.for_each(|(listener, transform)| {
        if result.is_none() && listener.enabled {
            result = Some(([transform.x(), transform.y()], transform.rotation_z()));
        }
    });
    if let Some(result) = result {
        return result;
    }

    let camera = world.query::<(&MainCamera, &Transform)>();
    let mut fallback = None;
    camera.for_each(|(_, transform)| {
        if fallback.is_none() {
            fallback = Some(([transform.x(), transform.y()], transform.rotation_z()));
        }
    });
    fallback.unwrap_or(([0.0, 0.0], 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::render::MainCamera;

    #[test]
    fn explicit_listener_wins_over_camera() {
        let mut world = World::new();
        world.spawn((MainCamera, Transform::from_xy(5.0, 6.0).with_rotation(1.0)));
        world.spawn((
            AudioListener2D::default(),
            Transform::from_xy(1.0, 2.0).with_rotation(0.25),
        ));

        let (position, rotation) = find_listener_pose(&world);
        assert_eq!(position, [1.0, 2.0]);
        assert!((rotation - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn primary_camera_is_used_when_no_explicit_listener_exists() {
        let mut world = World::new();
        world.spawn((MainCamera, Transform::from_xy(3.0, 4.0).with_rotation(0.75)));

        let (position, rotation) = find_listener_pose(&world);
        assert_eq!(position, [3.0, 4.0]);
        assert!(
            (rotation - 0.75).abs() <= 1.0e-6,
            "expected 0.75 radians, got {rotation:?}"
        );
    }
}
