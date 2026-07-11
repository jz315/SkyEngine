use sky_engine::ecs::{EntityId, With, World};
use sky_engine::render::Color;
use sky_engine::ui::{
    UiAlign, UiAnchor, UiButton, UiEventKind, UiEvents, UiId, UiLayout, UiLength, UiNode, UiPanel,
    UiRect, UiText,
};

use crate::components::{Adventurer, Condition, Name, Role, Stats};
use crate::resources::{Calendar, GameFlow, GamePhase, GuildStock, HudState, TownState};
use crate::systems;

#[derive(Clone, Copy)]
pub enum GuildAction {
    SelectContract(usize),
    Launch,
    NextDay,
    Pause,
}

#[derive(Clone, Copy)]
pub struct GuildUi {
    hud_text: EntityId,
    roster_text: EntityId,
    contract_buttons: [EntityId; 3],
    launch_button: EntityId,
    next_day_button: EntityId,
    pause_button: EntityId,
}

pub fn spawn_ui(world: &mut World) -> GuildUi {
    let hud_panel = world.spawn((
        UiNode::panel(520.0, 132.0)
            .anchor(UiAnchor::TopLeft)
            .at(16.0, 14.0)
            .z(100)
            .layout(UiLayout::column(
                UiRect::new(16.0, 12.0, 16.0, 12.0),
                10.0,
                UiAlign::Stretch,
            )),
        UiPanel::new(Color::rgba8(15, 18, 24, 230)),
    ));
    let hud_text = world.spawn((
        UiNode::panel(1.0, 88.0)
            .child_of(hud_panel)
            .width(UiLength::Percent(1.0)),
        UiText::new("")
            .size(16.0)
            .color(Color::rgba8(234, 236, 214, 255)),
    ));

    let contract_panel = world.spawn((
        UiNode::panel(390.0, 304.0)
            .anchor(UiAnchor::TopRight)
            .at(16.0, 14.0)
            .z(100)
            .layout(UiLayout::column(
                UiRect::new(16.0, 14.0, 16.0, 14.0),
                10.0,
                UiAlign::Stretch,
            )),
        UiPanel::new(Color::rgba8(20, 16, 14, 232)),
    ));
    let contract_title = world.spawn((
        UiNode::panel(1.0, 28.0)
            .child_of(contract_panel)
            .width(UiLength::Percent(1.0)),
        UiText::new("Contract Board")
            .size(20.0)
            .color(Color::rgba8(248, 224, 166, 255)),
    ));
    let contract_buttons = [
        spawn_button(world, contract_panel, "contract_0", "Contract 1"),
        spawn_button(world, contract_panel, "contract_1", "Contract 2"),
        spawn_button(world, contract_panel, "contract_2", "Contract 3"),
    ];
    let launch_button = spawn_button(world, contract_panel, "launch", "Launch Expedition");
    let next_day_button = spawn_button(world, contract_panel, "next_day", "Next Day");

    let roster_panel = world.spawn((
        UiNode::panel(560.0, 172.0)
            .anchor(UiAnchor::BottomLeft)
            .at(16.0, 18.0)
            .z(100)
            .layout(UiLayout::column(
                UiRect::new(16.0, 12.0, 16.0, 12.0),
                8.0,
                UiAlign::Stretch,
            )),
        UiPanel::new(Color::rgba8(15, 20, 22, 226)),
    ));
    let _roster_title = world.spawn((
        UiNode::panel(1.0, 24.0)
            .child_of(roster_panel)
            .width(UiLength::Percent(1.0)),
        UiText::new("Roster")
            .size(19.0)
            .color(Color::rgba8(194, 236, 212, 255)),
    ));
    let roster_text = world.spawn((
        UiNode::panel(1.0, 112.0)
            .child_of(roster_panel)
            .width(UiLength::Percent(1.0)),
        UiText::new("")
            .size(14.0)
            .color(Color::rgba8(216, 232, 218, 255)),
    ));

    let pause_button = world.spawn((
        UiNode::panel(104.0, 38.0)
            .id(UiId::new("pause"))
            .anchor(UiAnchor::BottomRight)
            .at(18.0, 18.0)
            .z(110),
        UiButton::new("Pause"),
    ));

    let _ = contract_title;
    GuildUi {
        hud_text,
        roster_text,
        contract_buttons,
        launch_button,
        next_day_button,
        pause_button,
    }
}

pub fn drain_actions(world: &mut World) -> Vec<GuildAction> {
    let mut actions = Vec::new();
    if let Some(events) = world.get_resource_mut::<UiEvents>() {
        for event in events.drain() {
            if event.kind != UiEventKind::Clicked {
                continue;
            }
            let Some(id) = event.id else {
                continue;
            };
            match id.as_str() {
                "contract_0" => actions.push(GuildAction::SelectContract(0)),
                "contract_1" => actions.push(GuildAction::SelectContract(1)),
                "contract_2" => actions.push(GuildAction::SelectContract(2)),
                "launch" => actions.push(GuildAction::Launch),
                "next_day" => actions.push(GuildAction::NextDay),
                "pause" => actions.push(GuildAction::Pause),
                _ => {}
            }
        }
    }
    actions
}

pub fn sync_ui(world: &mut World, ui: GuildUi) {
    let calendar_day = world.get_resource::<Calendar>().unwrap().day;
    let stock = *world.get_resource::<GuildStock>().unwrap();
    let town = *world.get_resource::<TownState>().unwrap();
    let flow = world.get_resource::<GameFlow>().unwrap().clone();
    let hud = world.get_resource::<HudState>().unwrap().clone();
    let phase = flow.phase;
    let selected_slot = flow.selected_slot;

    set_ui_text(
        world,
        ui.hud_text,
        format!(
            "Day {} | {}\nGold {}  Food {}  Medicine {}  Supplies {}  Rep {}\nDanger {}  Unrest {}\n{}\n{}",
            calendar_day,
            phase.label(),
            stock.gold,
            stock.food,
            stock.medicine,
            stock.supplies,
            stock.reputation,
            town.danger,
            town.unrest,
            hud.headline,
            hud.party
        ),
    );

    set_ui_text(world, ui.roster_text, roster_text(world));

    let choices = systems::contract_choices(world);
    for slot in 0..ui.contract_buttons.len() {
        let label = choices
            .iter()
            .find(|choice| choice.slot == slot)
            .map(|choice| {
                format!(
                    "{}{} | {} d{} / {} / {}g {}f {}m +{}r",
                    if selected_slot == slot { "> " } else { "" },
                    choice.name,
                    choice.spec.kind.label(),
                    choice.spec.difficulty,
                    choice.spec.primary_stat.label(),
                    choice.spec.reward_gold,
                    choice.spec.reward_food,
                    choice.spec.reward_medicine,
                    choice.spec.reward_reputation
                )
            })
            .unwrap_or_else(|| "empty".to_string());
        set_button(
            world,
            ui.contract_buttons[slot],
            &label,
            phase == GamePhase::Planning,
        );
    }

    set_button(
        world,
        ui.launch_button,
        "Launch Expedition",
        phase == GamePhase::Planning,
    );
    set_button(
        world,
        ui.next_day_button,
        "Next Day",
        matches!(phase, GamePhase::Planning | GamePhase::Results),
    );
    set_button(world, ui.pause_button, "Pause", true);
}

fn spawn_button(
    world: &mut World,
    parent: EntityId,
    id: &'static str,
    label: &'static str,
) -> EntityId {
    world.spawn((
        UiNode::panel(1.0, 42.0)
            .id(UiId::new(id))
            .child_of(parent)
            .width(UiLength::Percent(1.0)),
        UiButton::new(label),
    ))
}

fn set_ui_text(world: &mut World, entity: EntityId, text: impl Into<String>) {
    if let Some(ui_text) = world.get_mut::<UiText>(entity) {
        ui_text.text = text.into();
    }
}

fn set_button(world: &mut World, entity: EntityId, label: &str, enabled: bool) {
    if let Some(node) = world.get_mut::<UiNode>(entity) {
        node.enabled = enabled;
        node.visible = true;
    }
    if let Some(button) = world.get_mut::<UiButton>(entity) {
        button.label = label.to_string();
    }
}

fn roster_text(world: &World) -> String {
    let mut rows = Vec::new();
    let query = world
        .query::<(&Name, &Role, &Stats, &Condition)>()
        .filter::<With<Adventurer>>();
    query.for_each(|(name, role, stats, condition)| {
        rows.push(format!(
            "{} [{}] hp {} stress {} fatigue {} | M{} F{} W{} S{}",
            name.0,
            role.label(),
            condition.health,
            condition.stress,
            condition.fatigue,
            stats.might,
            stats.finesse,
            stats.wits,
            stats.spirit
        ));
    });
    rows.sort();
    rows.join("\n")
}
