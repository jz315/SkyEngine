use sky_engine::ecs::{With, World};

use crate::components::{Adventurer, Condition};
use crate::resources::{GuildStock, HudState};

pub fn recovery_system(world: &mut World) {
    let adventurers = count_adventurers(world);
    let wounded = count_wounded(world);

    let (fed, medicine_used) = {
        let stock = world.get_resource_mut::<GuildStock>().unwrap();
        let fed = stock.food >= adventurers;
        if fed {
            stock.food -= adventurers;
        } else {
            stock.food = 0;
        }
        let medicine_used = wounded.min(stock.medicine);
        stock.medicine -= medicine_used;
        (fed, medicine_used)
    };

    let mut healed = medicine_used;
    let mut adventurers = world
        .query_mut::<&mut Condition>()
        .filter::<With<Adventurer>>();
    adventurers.for_each(|condition| {
        if fed {
            condition.fatigue -= 2;
            condition.stress -= 1;
        } else {
            condition.fatigue += 1;
            condition.stress += 2;
        }

        if condition.health < 10 {
            if healed > 0 {
                condition.health += 2;
                healed -= 1;
            } else {
                condition.health += 1;
            }
        }

        condition.clamp();
    });

    if !fed {
        world.get_resource_mut::<HudState>().unwrap().headline =
            "Food ran short. Stress rises across the guild.".to_string();
    }
}

fn count_adventurers(world: &World) -> i32 {
    let mut count = 0;
    let query = world.query::<&Adventurer>().filter::<With<Adventurer>>();
    query.for_each(|_| {
        count += 1;
    });
    count
}

fn count_wounded(world: &World) -> i32 {
    let mut count = 0;
    let query = world.query::<&Condition>().filter::<With<Adventurer>>();
    query.for_each(|condition| {
        if condition.health < 10 {
            count += 1;
        }
    });
    count
}
