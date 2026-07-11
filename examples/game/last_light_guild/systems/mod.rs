mod contracts;
mod expedition;
mod placement;
mod pressure;
mod recovery;
mod visuals;

use sky_engine::ecs::{ExclusiveSystem, Update, World};
use sky_engine::input::{Input, KeyCode};

use crate::components::GridPos;
use crate::resources::{Calendar, GameFlow, GamePhase, GuildStock, HudState, SimClock, TownState};
use crate::ui::{self, GuildAction, GuildUi};

pub fn install_systems(world: &mut World) {
    world
        .stage(Update)
        .add_exclusive(input_system)
        .add_exclusive(DayBootstrapSystem)
        .add_exclusive(phase_system)
        .add_exclusive(presentation_system);
}

struct DayBootstrapSystem;

impl ExclusiveSystem for DayBootstrapSystem {
    fn init(&mut self, world: &mut World) {
        start_new_day(world);
    }

    fn run(&mut self, _world: &mut World) {}
}

fn input_system(world: &mut World) {
    let input = *world
        .get_resource::<Input>()
        .expect("last_light_guild requires Input");

    if input.key_pressed(KeyCode::Digit1) {
        select_contract(world, 0);
    }
    if input.key_pressed(KeyCode::Digit2) {
        select_contract(world, 1);
    }
    if input.key_pressed(KeyCode::Digit3) {
        select_contract(world, 2);
    }
    if input.key_pressed(KeyCode::Space) || input.key_pressed(KeyCode::Enter) {
        launch_selected_contract(world);
    }
    if input.key_pressed(KeyCode::KeyN) {
        next_day(world);
    }
    if input.key_pressed(KeyCode::KeyP) {
        toggle_pause(world);
    }

    for action in ui::drain_actions(world) {
        match action {
            GuildAction::SelectContract(slot) => select_contract(world, slot),
            GuildAction::Launch => launch_selected_contract(world),
            GuildAction::NextDay => next_day(world),
            GuildAction::Pause => toggle_pause(world),
        }
    }
}

fn phase_system(world: &mut World) {
    let dt = world.time.delta;
    visuals::animate_adventurers(world, dt);
    visuals::sync_transforms(world, dt);
    visuals::sync_followers(world);
    contracts::sync_contract_visuals(world);

    let paused = world
        .get_resource::<SimClock>()
        .map(|clock| clock.paused)
        .unwrap_or(false);
    if !paused {
        update_phase(world, dt);
    }
}

fn presentation_system(world: &mut World) {
    let Some(ui) = world.get_resource::<GuildUi>().copied() else {
        return;
    };
    ui::sync_ui(world, ui);
}

pub fn toggle_pause(world: &mut World) {
    let clock = world.get_resource_mut::<SimClock>().unwrap();
    clock.paused = !clock.paused;
}

pub fn start_new_day(world: &mut World) {
    begin_day(world);
    recovery::recovery_system(world);
    pressure::town_pressure_system(world);
    contracts::contract_board_system(world);
    placement::placement_system(world);
    let flow = world.get_resource_mut::<GameFlow>().unwrap();
    flow.phase = GamePhase::Planning;
    flow.active_contract = None;
    flow.party.clear();
    flow.timer = 0.0;
}

pub fn select_contract(world: &mut World, slot: usize) {
    let Some(flow) = world.get_resource_mut::<GameFlow>() else {
        return;
    };
    if flow.phase != GamePhase::Planning {
        return;
    }
    flow.selected_slot = slot.min(2);
}

pub fn launch_selected_contract(world: &mut World) {
    let selected_slot = world
        .get_resource::<GameFlow>()
        .map(|flow| flow.selected_slot)
        .unwrap_or(0);
    let Some(contract) = contracts::contract_by_slot(world, selected_slot) else {
        world.get_resource_mut::<HudState>().unwrap().party =
            "Pick a contract from the board first.".to_string();
        return;
    };

    let party = expedition::select_party_for_contract(world, &contract.spec);
    if party.is_empty() {
        world.get_resource_mut::<HudState>().unwrap().party =
            "Nobody is fit enough for that job.".to_string();
        return;
    }

    for (index, entity) in party.iter().enumerate() {
        if let Some(pos) = world.get_mut::<GridPos>(*entity) {
            *pos = GridPos {
                x: 27 + index as i32 * 2,
                y: 17,
            };
        }
    }

    let party_names = party_names(world, &party);
    let flow = world.get_resource_mut::<GameFlow>().unwrap();
    flow.phase = GamePhase::Departing;
    flow.active_contract = Some(contract.entity);
    flow.party = party;
    flow.timer = 0.0;
    world.get_resource_mut::<HudState>().unwrap().party =
        format!("{party_names} are walking to the road gate.");
}

pub fn next_day(world: &mut World) {
    let phase = world.get_resource::<GameFlow>().unwrap().phase;
    if phase == GamePhase::Planning || phase == GamePhase::Results {
        start_new_day(world);
    }
}

pub fn title(world: &World) -> String {
    let day = world.get_resource::<Calendar>().unwrap().day;
    let stock = world.get_resource::<GuildStock>().unwrap();
    let town = world.get_resource::<TownState>().unwrap();
    let clock = world.get_resource::<SimClock>().unwrap();
    let hud = world.get_resource::<HudState>().unwrap();
    let flow = world.get_resource::<GameFlow>().unwrap();
    let pause = if clock.paused { "PAUSED" } else { "RUNNING" };
    format!(
        "Last Light Guild | {pause} | {} | day {day} | gold {} food {} med {} supplies {} rep {} | danger {} unrest {} | {} | {}",
        flow.phase.label(),
        stock.gold,
        stock.food,
        stock.medicine,
        stock.supplies,
        stock.reputation,
        town.danger,
        town.unrest,
        hud.headline,
        hud.party
    )
}

fn begin_day(world: &mut World) {
    let day = {
        let calendar = world.get_resource_mut::<Calendar>().unwrap();
        calendar.day += 1;
        calendar.day
    };
    world.get_resource_mut::<HudState>().unwrap().headline =
        format!("Day {day}: guild bell before sunrise.");
}

fn update_phase(world: &mut World, dt: f32) {
    let phase = world.get_resource::<GameFlow>().unwrap().phase;
    match phase {
        GamePhase::Planning | GamePhase::Results => {}
        GamePhase::Departing => {
            let party = world.get_resource::<GameFlow>().unwrap().party.clone();
            if visuals::entities_settled(world, &party) {
                let flow = world.get_resource_mut::<GameFlow>().unwrap();
                flow.phase = GamePhase::Resolving;
                flow.timer = 0.75;
                world.get_resource_mut::<HudState>().unwrap().headline =
                    "The party is beyond the torchline.".to_string();
            }
        }
        GamePhase::Resolving => {
            let ready = {
                let flow = world.get_resource_mut::<GameFlow>().unwrap();
                flow.timer -= dt;
                flow.timer <= 0.0
            };
            if ready {
                resolve_active_contract(world);
            }
        }
        GamePhase::Returning => {
            let party = world.get_resource::<GameFlow>().unwrap().party.clone();
            if visuals::entities_settled(world, &party) {
                let flow = world.get_resource_mut::<GameFlow>().unwrap();
                flow.phase = GamePhase::Results;
                flow.timer = 0.0;
                world.get_resource_mut::<HudState>().unwrap().headline =
                    "Results posted. Press N or Next Day.".to_string();
            }
        }
    }
}

fn resolve_active_contract(world: &mut World) {
    let Some(contract_entity) = world.get_resource::<GameFlow>().unwrap().active_contract else {
        return;
    };
    let Some(slot) = world
        .get_resource::<GameFlow>()
        .map(|flow| flow.selected_slot)
    else {
        return;
    };
    let Some(contract) = contracts::contract_by_slot(world, slot) else {
        return;
    };
    if contract.entity != contract_entity {
        return;
    }

    let party = world.get_resource::<GameFlow>().unwrap().party.clone();
    expedition::resolve_contract(world, contract, &party);
    let _ = world.despawn(contract_entity);
    placement::placement_system(world);

    let flow = world.get_resource_mut::<GameFlow>().unwrap();
    flow.phase = GamePhase::Returning;
    flow.active_contract = None;
}

fn party_names(world: &World, party: &[sky_engine::ecs::EntityId]) -> String {
    party
        .iter()
        .filter_map(|entity| {
            world
                .get::<crate::components::Name>(*entity)
                .map(|name| name.0)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn contract_choices(world: &World) -> Vec<contracts::ContractChoice> {
    contracts::contract_choices(world)
}
