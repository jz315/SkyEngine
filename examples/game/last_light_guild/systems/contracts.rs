use sky_engine::ecs::{Commands, EntityId, With, World};
use sky_engine::render::{SortingLayer, SpriteRenderer};

use crate::components::{
    ContractKind, ContractMarker, ContractSlot, ContractSpec, GridPos, Name, PrimaryStat, Role,
};
use crate::layout::{grid_to_world, TILE};
use crate::resources::{BoardCursor, GameFlow, TownState};

pub fn contract_board_system(world: &mut World) {
    clear_contracts(world);

    let day = world
        .get_resource::<crate::resources::Calendar>()
        .unwrap()
        .day;
    let danger = world.get_resource::<TownState>().unwrap().danger;
    let start = {
        let cursor = world.get_resource_mut::<BoardCursor>().unwrap();
        let start = cursor.next;
        cursor.next = (cursor.next + 2) % TEMPLATES.len();
        start
    };

    for slot in 0..3 {
        let template = TEMPLATES[(start + day as usize + slot) % TEMPLATES.len()];
        let difficulty = template.base_difficulty + (danger / 3) + slot as i32;
        let board_pos = GridPos {
            x: 26 + slot as i32,
            y: 12 + slot as i32,
        };
        world.spawn((
            Name(template.title),
            board_pos,
            ContractMarker,
            ContractSlot(slot),
            ContractSpec {
                kind: template.kind,
                difficulty,
                reward_gold: template.reward_gold + difficulty,
                reward_food: template.reward_food,
                reward_medicine: template.reward_medicine,
                reward_reputation: template.reward_reputation,
                supply_cost: template.supply_cost + slot as i32,
                primary_stat: template.primary_stat,
                preferred_role: template.preferred_role,
            },
            grid_to_world(board_pos, 0.35),
            SpriteRenderer::new(TILE * 0.76, TILE * 0.76).color(template.kind.color()),
            SortingLayer(4),
        ));
    }
}

pub fn contract_by_slot(world: &World, slot: usize) -> Option<ContractChoice> {
    let mut found = None;
    let mut contracts =
        world.query_filtered::<(&Name, &ContractSpec, &ContractSlot), With<ContractMarker>>();
    contracts.for_each_with_entity(world, |entity, (name, spec, contract_slot)| {
        if contract_slot.0 == slot {
            found = Some(ContractChoice {
                entity,
                slot: contract_slot.0,
                name: name.0,
                spec: *spec,
            });
        }
    });
    found
}

pub fn contract_choices(world: &World) -> Vec<ContractChoice> {
    let mut choices = Vec::new();
    let mut contracts =
        world.query_filtered::<(&Name, &ContractSpec, &ContractSlot), With<ContractMarker>>();
    contracts.for_each_with_entity(world, |entity, (name, spec, slot)| {
        choices.push(ContractChoice {
            entity,
            slot: slot.0,
            name: name.0,
            spec: *spec,
        });
    });
    choices.sort_by_key(|choice| choice.slot);
    choices
}

pub fn sync_contract_visuals(world: &mut World) {
    let selected_slot = world
        .get_resource::<GameFlow>()
        .map(|flow| flow.selected_slot)
        .unwrap_or(0);
    let mut query =
        world.query_filtered::<(&ContractSlot, &ContractSpec, &mut SpriteRenderer), With<ContractMarker>>();
    query.for_each(world, |(slot, spec, sprite)| {
        let selected = slot.0 == selected_slot;
        let color = spec.kind.color();
        sprite.color = if selected {
            sky_engine::render::Color::new(
                (color.r + 0.18).min(1.0),
                (color.g + 0.18).min(1.0),
                (color.b + 0.18).min(1.0),
                1.0,
            )
        } else {
            color
        };
        sprite.width = TILE * if selected { 0.96 } else { 0.76 };
        sprite.height = TILE * if selected { 0.96 } else { 0.76 };
    });
}

fn clear_contracts(world: &mut World) {
    let mut commands = Commands::new();
    let mut contracts = world.query_filtered::<&ContractMarker, With<ContractMarker>>();
    contracts.for_each_with_entity(&mut *world, |entity, _| {
        commands.despawn(entity);
    });
    commands.apply(world);
}

#[derive(Clone, Copy)]
pub struct ContractChoice {
    pub entity: EntityId,
    pub slot: usize,
    pub name: &'static str,
    pub spec: ContractSpec,
}

#[derive(Clone, Copy)]
struct ContractTemplate {
    title: &'static str,
    kind: ContractKind,
    base_difficulty: i32,
    reward_gold: i32,
    reward_food: i32,
    reward_medicine: i32,
    reward_reputation: i32,
    supply_cost: i32,
    primary_stat: PrimaryStat,
    preferred_role: Role,
}

const TEMPLATES: [ContractTemplate; 6] = [
    ContractTemplate {
        title: "granary rats",
        kind: ContractKind::Hunt,
        base_difficulty: 2,
        reward_gold: 6,
        reward_food: 5,
        reward_medicine: 0,
        reward_reputation: 1,
        supply_cost: 1,
        primary_stat: PrimaryStat::Might,
        preferred_role: Role::Vanguard,
    },
    ContractTemplate {
        title: "missing apothecary",
        kind: ContractKind::Rescue,
        base_difficulty: 3,
        reward_gold: 7,
        reward_food: 0,
        reward_medicine: 3,
        reward_reputation: 2,
        supply_cost: 2,
        primary_stat: PrimaryStat::Spirit,
        preferred_role: Role::Medic,
    },
    ContractTemplate {
        title: "moonwell salvage",
        kind: ContractKind::Salvage,
        base_difficulty: 4,
        reward_gold: 14,
        reward_food: 0,
        reward_medicine: 1,
        reward_reputation: 0,
        supply_cost: 3,
        primary_stat: PrimaryStat::Wits,
        preferred_role: Role::Occultist,
    },
    ContractTemplate {
        title: "pilgrim escort",
        kind: ContractKind::Escort,
        base_difficulty: 3,
        reward_gold: 9,
        reward_food: 2,
        reward_medicine: 0,
        reward_reputation: 2,
        supply_cost: 2,
        primary_stat: PrimaryStat::Finesse,
        preferred_role: Role::Scout,
    },
    ContractTemplate {
        title: "old keep smoke",
        kind: ContractKind::Investigate,
        base_difficulty: 4,
        reward_gold: 10,
        reward_food: 0,
        reward_medicine: 2,
        reward_reputation: 1,
        supply_cost: 2,
        primary_stat: PrimaryStat::Wits,
        preferred_role: Role::Broker,
    },
    ContractTemplate {
        title: "wolf shrine",
        kind: ContractKind::Hunt,
        base_difficulty: 5,
        reward_gold: 13,
        reward_food: 3,
        reward_medicine: 0,
        reward_reputation: 2,
        supply_cost: 3,
        primary_stat: PrimaryStat::Might,
        preferred_role: Role::Vanguard,
    },
];
