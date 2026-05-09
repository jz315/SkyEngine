use sky_engine::ecs::World;

use crate::resources::{Calendar, GuildStock, TownState};

pub fn town_pressure_system(world: &mut World) {
    let day = world.get_resource::<Calendar>().unwrap().day;
    if day % 3 == 0 {
        world.get_resource_mut::<TownState>().unwrap().danger += 1;
    }
    if world.get_resource::<GuildStock>().unwrap().food == 0 {
        world.get_resource_mut::<TownState>().unwrap().unrest += 1;
    }
}
