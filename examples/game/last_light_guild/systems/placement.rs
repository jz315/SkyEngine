use sky_engine::ecs::{With, World};

use crate::components::{Adventurer, Condition, GridPos};
use crate::layout::room_slots;

pub fn placement_system(world: &mut World) {
    let mut states = Vec::new();
    let mut adventurers = world.query_filtered::<&Condition, With<Adventurer>>();
    adventurers.for_each_with_entity(&mut *world, |entity, condition| {
        states.push((entity, *condition));
    });

    let mut infirmary = 0;
    let mut bunks = 0;
    let mut common = 0;
    let mut training = 0;

    for (entity, condition) in states {
        let pos = if condition.health <= 5 {
            let pos = room_slots(infirmary, 14, 3, 3);
            infirmary += 1;
            pos
        } else if condition.fatigue >= 6 {
            let pos = room_slots(bunks, 3, 3, 3);
            bunks += 1;
            pos
        } else if condition.stress >= 6 {
            let pos = room_slots(common, 25, 12, 3);
            common += 1;
            pos
        } else {
            let pos = room_slots(training, 4, 14, 7);
            training += 1;
            pos
        };

        if let Some(current) = world.get_mut::<GridPos>(entity) {
            *current = pos;
        }
    }
}
