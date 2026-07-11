use std::cmp::Reverse;

use sky_engine::ecs::{EntityId, With, World};

use crate::components::{
    Adventurer, Condition, ContractKind, ContractSpec, Name, Personality, Relationship, Role, Stats,
};
use crate::resources::{Calendar, GuildStock, HudState, TownState};

use super::contracts::ContractChoice;

pub fn select_party_for_contract(world: &World, spec: &ContractSpec) -> Vec<EntityId> {
    select_party(world, spec)
}

pub fn resolve_contract(world: &mut World, contract: ContractChoice, party: &[EntityId]) {
    if party.is_empty() {
        world.get_resource_mut::<HudState>().unwrap().party =
            "Nobody is fit enough for the board.".to_string();
        return;
    }

    let supply_shortage = spend_supplies(world, contract.spec.supply_cost);
    let party_score = party_score(world, &party, &contract.spec);
    let day = world.get_resource::<Calendar>().unwrap().day;
    let danger = world.get_resource::<TownState>().unwrap().danger;
    let twist = deterministic_twist(day, contract.spec.difficulty, party.len() as i32);
    let challenge =
        contract.spec.difficulty * 5 + danger + twist + if supply_shortage { 4 } else { 0 };
    let success = party_score >= challenge;

    let party_names = join_names(world, party);
    if success {
        apply_success(world, &contract, party_score - challenge);
    } else {
        apply_failure(world, challenge - party_score);
    }
    apply_party_aftermath(world, party, &contract.spec, success, supply_shortage);
    apply_relationship_aftermath(world, party, success);

    let result = if success { "cleared" } else { "botched" };
    let shortage = if supply_shortage { " underpacked" } else { "" };
    world.get_resource_mut::<HudState>().unwrap().party = format!(
        "{} {} {} with {}{}.",
        party_names,
        result,
        contract.name,
        contract.spec.kind.label(),
        shortage
    );
}

fn select_party(world: &World, spec: &ContractSpec) -> Vec<EntityId> {
    let mut candidates = Vec::new();
    let adventurers = world
        .query::<(&Name, &Role, &Stats, &Condition, &Personality)>()
        .filter::<With<Adventurer>>();

    adventurers.for_each_with_entity(|entity, (name, role, stats, condition, personality)| {
        if condition.health <= 2 {
            return;
        }
        candidates.push(PartyCandidate {
            entity,
            name: name.0,
            score: adventurer_score(*role, *stats, *condition, *personality, spec),
        });
    });

    candidates.sort_by_key(|candidate| (Reverse(candidate.score), candidate.name));
    candidates
        .into_iter()
        .take(3)
        .map(|candidate| candidate.entity)
        .collect()
}

fn adventurer_score(
    role: Role,
    stats: Stats,
    condition: Condition,
    personality: Personality,
    spec: &ContractSpec,
) -> i32 {
    let role_bonus = if role == spec.preferred_role { 7 } else { 0 };
    let trait_bonus = match spec.kind {
        ContractKind::Rescue | ContractKind::Escort => personality.empathy + personality.caution,
        ContractKind::Hunt => personality.courage - personality.caution,
        ContractKind::Salvage => personality.caution * 2,
        ContractKind::Investigate => personality.caution + personality.empathy,
    };
    stats.get(spec.primary_stat) * 4 + condition.readiness() + role_bonus + trait_bonus
}

fn party_score(world: &World, party: &[EntityId], spec: &ContractSpec) -> i32 {
    party
        .iter()
        .filter_map(|entity| {
            Some(
                adventurer_score(
                    *world.get::<Role>(*entity)?,
                    *world.get::<Stats>(*entity)?,
                    *world.get::<Condition>(*entity)?,
                    *world.get::<Personality>(*entity)?,
                    spec,
                ) / 2,
            )
        })
        .sum()
}

fn spend_supplies(world: &mut World, cost: i32) -> bool {
    let stock = world.get_resource_mut::<GuildStock>().unwrap();
    if stock.supplies >= cost {
        stock.supplies -= cost;
        false
    } else {
        stock.supplies = 0;
        true
    }
}

fn apply_success(world: &mut World, contract: &ContractChoice, margin: i32) {
    {
        let stock = world.get_resource_mut::<GuildStock>().unwrap();
        stock.gold += contract.spec.reward_gold;
        stock.food += contract.spec.reward_food;
        stock.medicine += contract.spec.reward_medicine;
        stock.reputation += contract.spec.reward_reputation;
    }
    {
        let town = world.get_resource_mut::<TownState>().unwrap();
        town.danger = (town.danger - 1).max(0);
        town.unrest = (town.unrest - contract.spec.reward_reputation).max(0);
    }
    world.get_resource_mut::<HudState>().unwrap().headline =
        format!("Success by {margin}. The town breathes easier.");
}

fn apply_failure(world: &mut World, margin: i32) {
    {
        let stock = world.get_resource_mut::<GuildStock>().unwrap();
        stock.reputation = (stock.reputation - 1).max(0);
    }
    {
        let town = world.get_resource_mut::<TownState>().unwrap();
        town.danger += 1;
        town.unrest += 1;
    }
    world.get_resource_mut::<HudState>().unwrap().headline =
        format!("Failure by {margin}. Trouble spreads outside the lights.");
}

fn apply_party_aftermath(
    world: &mut World,
    party: &[EntityId],
    spec: &ContractSpec,
    success: bool,
    supply_shortage: bool,
) {
    for entity in party {
        let Some(condition) = world.get_mut::<Condition>(*entity) else {
            continue;
        };
        condition.fatigue += 2 + spec.difficulty / 2;
        condition.stress += if success { 1 } else { 3 };
        if !success || spec.difficulty >= 5 {
            condition.health -= 1 + i32::from(supply_shortage);
        }
        if spec.kind == ContractKind::Hunt && !success {
            condition.health -= 1;
        }
        condition.clamp();
    }
}

fn apply_relationship_aftermath(world: &mut World, party: &[EntityId], success: bool) {
    if party.len() < 2 {
        return;
    }

    let a = party[0];
    let b = party[1];
    let mut relationships = world.query_mut::<&mut Relationship>();
    relationships.for_each(|relationship| {
        if !relationship_contains(relationship, a, b) {
            return;
        }

        if success {
            relationship.trust += 1;
            relationship.tension = (relationship.tension - 1).max(0);
        } else {
            relationship.trust -= 1;
            relationship.tension += 2;
        }

        relationship.trust = relationship.trust.clamp(-5, 5);
        relationship.tension = relationship.tension.clamp(0, 5);
    });
}

fn deterministic_twist(day: u32, difficulty: i32, party_size: i32) -> i32 {
    let seed = day as i32 * 17 + difficulty * 11 + party_size * 7;
    seed.rem_euclid(7) - 2
}

fn join_names(world: &World, party: &[EntityId]) -> String {
    party
        .iter()
        .filter_map(|entity| world.get::<Name>(*entity).map(|name| name.0))
        .collect::<Vec<_>>()
        .join(", ")
}

fn relationship_contains(relationship: &Relationship, a: EntityId, b: EntityId) -> bool {
    (relationship.a == a && relationship.b == b) || (relationship.a == b && relationship.b == a)
}

struct PartyCandidate {
    entity: EntityId,
    name: &'static str,
    score: i32,
}
