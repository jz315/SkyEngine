use rustc_hash::FxHashMap;

use crate::ecs::{EntityId, World};
use crate::input::{Input, InteractionContext, MouseButton};

use super::{
    hit_test_input, rect_map, resolve_world_layout, UiEvent, UiEventKind, UiEvents, UiId, UiNode,
    UiRect, UiScroll, UiSlider, UiState, UiToggle,
};

/// Update layout and pointer interaction for the retained UI tree.
pub fn update_ui(world: &mut World, input: &Input, surface_size: [f32; 2]) {
    super::ensure_legacy_ui_resources(world);

    let mut resolved = resolve_world_layout(world, surface_size);
    let mut rects = rect_map(&resolved);
    if refresh_scroll_content(world, &resolved, &rects) {
        resolved = resolve_world_layout(world, surface_size);
        rects = rect_map(&resolved);
    }

    let pointer = input
        .mouse_in_window()
        .then(|| input.mouse_logical_position().to_array());
    let input_hovered_node = pointer.and_then(|position| hit_test_input(&resolved, position));
    let scroll_hovered = input_hovered_node.as_ref().map(|node| node.entity);
    let mut scrolled = None;
    let scroll_delta = input.scroll_delta();
    if (scroll_delta[0].abs() > f32::EPSILON || scroll_delta[1].abs() > f32::EPSILON)
        && pointer.is_some()
    {
        if let Some(target) = scroll_hovered.and_then(|entity| find_scroll_target(world, entity)) {
            if apply_scroll_delta(world, target, scroll_delta, &rects) {
                scrolled = Some(target);
                resolved = resolve_world_layout(world, surface_size);
                rects = rect_map(&resolved);
                if refresh_scroll_content(world, &resolved, &rects) {
                    resolved = resolve_world_layout(world, surface_size);
                    rects = rect_map(&resolved);
                }
            }
        }
    }

    let id_by_entity: FxHashMap<EntityId, Option<UiId>> = resolved
        .iter()
        .map(|node| (node.entity, node.id.clone()))
        .collect();
    let visible_enabled: Vec<(EntityId, bool)> = resolved
        .iter()
        .filter(|node| node.visible)
        .map(|node| (node.entity, node.enabled))
        .collect();
    let interactive_by_entity: FxHashMap<EntityId, bool> = resolved
        .iter()
        .map(|node| (node.entity, node.visible && node.enabled))
        .collect();

    let hovered = input_hovered_node.as_ref().map(|node| node.entity);

    let (previous_hovered, previous_pressed) = world
        .get_resource::<UiState>()
        .map(|state| (state.hovered(), state.pressed()))
        .unwrap_or((None, None));

    let mut next_pressed = previous_pressed;
    let mut emitted = Vec::new();

    if let Some(entity) = scrolled {
        emitted.push(event_with_id(
            UiEventKind::ValueChanged,
            entity,
            &id_by_entity,
        ));
    }

    if previous_hovered != hovered {
        if let Some(entity) = previous_hovered {
            emitted.push(event_with_id(
                UiEventKind::HoverEnded,
                entity,
                &id_by_entity,
            ));
        }
        if let Some(entity) = hovered {
            emitted.push(event_with_id(
                UiEventKind::HoverStarted,
                entity,
                &id_by_entity,
            ));
        }
    }

    if input.mouse_left_pressed() {
        if let Some(entity) = hovered {
            next_pressed = Some(entity);
            emitted.push(event_with_id(UiEventKind::Pressed, entity, &id_by_entity));
        }
    }

    if input.mouse_left() || input.mouse_left_released() {
        if let (Some(entity), Some(pointer), Some(rect)) = (
            next_pressed,
            pointer,
            next_pressed.and_then(|entity| rects.get(&entity).copied()),
        ) {
            update_slider_from_pointer(
                world,
                entity,
                pointer,
                rect,
                &interactive_by_entity,
                &id_by_entity,
                &mut emitted,
            );
        }
    }

    if input.mouse_left_released() {
        if let Some(entity) = previous_pressed.or(next_pressed) {
            emitted.push(event_with_id(UiEventKind::Released, entity, &id_by_entity));
            if hovered == Some(entity) {
                if interactive_by_entity.get(&entity).copied().unwrap_or(false) {
                    if let Some(toggle) = world.get_mut::<UiToggle>(entity) {
                        if toggle.toggle() {
                            emitted.push(event_with_id(
                                UiEventKind::ValueChanged,
                                entity,
                                &id_by_entity,
                            ));
                        }
                    }
                }
                emitted.push(event_with_id(UiEventKind::Clicked, entity, &id_by_entity));
            }
        }
        next_pressed = None;
    } else if !input.mouse_left() && next_pressed.is_some() {
        next_pressed = None;
    }

    update_interaction_context(
        world,
        hovered,
        next_pressed,
        hovered.is_some(),
        input.mouse_left_pressed(),
        input.mouse_left(),
        input.mouse_left_released(),
        scrolled.is_some(),
    );

    if let Some(state) = world.get_resource_mut::<UiState>() {
        state.set_layout(surface_size, rects);
        state.set_pointer_position(pointer);
        state.set_hovered(hovered);
        state.set_pressed(next_pressed);
        state.rebuild_interactions(visible_enabled);
    }

    if let Some(events) = world.get_resource_mut::<UiEvents>() {
        for event in emitted {
            events.push(event);
        }
    }
}

fn update_interaction_context(
    world: &mut World,
    hovered: Option<EntityId>,
    pressed: Option<EntityId>,
    hovered_blocks_input: bool,
    left_pressed: bool,
    left_held: bool,
    left_released: bool,
    scrolled: bool,
) {
    if world.get_resource::<InteractionContext>().is_none() {
        world.insert_resource(InteractionContext::default());
    }
    let Some(interaction) = world.get_resource_mut::<InteractionContext>() else {
        return;
    };
    interaction.set_hovered(hovered.filter(|_| hovered_blocks_input));
    interaction.set_pressed(pressed);
    if scrolled {
        interaction.consume_scroll();
    }
    if hovered_blocks_input && (left_pressed || left_released) {
        interaction.consume_pointer(MouseButton::Left);
    }
    if (left_held || left_released) && pressed.is_some() {
        interaction.consume_pointer(MouseButton::Left);
    }
}

fn refresh_scroll_content(
    world: &mut World,
    resolved: &[super::layout::ResolvedUiNode],
    rects: &FxHashMap<EntityId, UiRect>,
) -> bool {
    let query = world.query::<(&UiNode, Option<&UiScroll>)>();
    let mut parents = FxHashMap::default();
    let mut scroll_entities = Vec::new();
    query.for_each_with_entity(|entity, (node, scroll)| {
        parents.insert(entity, node.parent);
        if scroll.is_some() {
            scroll_entities.push(entity);
        }
    });

    let mut content_by_entity = FxHashMap::default();
    for &entity in &scroll_entities {
        if let Some(rect) = rects.get(&entity).copied() {
            content_by_entity.insert(entity, [rect.width, rect.height]);
        }
    }

    for child in resolved {
        let Some(Some(parent)) = parents.get(&child.entity).copied() else {
            continue;
        };
        let Some(content) = content_by_entity.get_mut(&parent) else {
            continue;
        };
        let Some(parent_rect) = rects.get(&parent).copied() else {
            continue;
        };
        let offset = world
            .get::<UiScroll>(parent)
            .map(|scroll| scroll.offset)
            .unwrap_or([0.0, 0.0]);
        content[0] = content[0].max(child.rect.right() - parent_rect.x + offset[0]);
        content[1] = content[1].max(child.rect.bottom() - parent_rect.y + offset[1]);
    }

    let mut changed = false;
    for entity in scroll_entities {
        let Some(viewport) = rects.get(&entity).copied() else {
            continue;
        };
        let content = content_by_entity
            .get(&entity)
            .copied()
            .unwrap_or([viewport.width, viewport.height]);
        if let Some(scroll) = world.get_mut::<UiScroll>(entity) {
            let next_content = [
                content[0].max(scroll.min_content_size[0]),
                content[1].max(scroll.min_content_size[1]),
            ];
            if scroll.content_size != next_content {
                scroll.content_size = next_content;
                changed = true;
            }
            changed |= scroll.clamp_offset(viewport);
        }
    }
    changed
}

fn find_scroll_target(world: &World, start: EntityId) -> Option<EntityId> {
    let mut current = Some(start);
    let mut visited = Vec::new();
    while let Some(entity) = current {
        if visited.contains(&entity) {
            return None;
        }
        visited.push(entity);
        if world.get::<UiScroll>(entity).is_some() {
            return Some(entity);
        }
        current = world.get::<UiNode>(entity).and_then(|node| node.parent);
    }
    None
}

fn apply_scroll_delta(
    world: &mut World,
    target: EntityId,
    raw_delta: [f32; 2],
    rects: &FxHashMap<EntityId, UiRect>,
) -> bool {
    let Some(viewport) = rects.get(&target).copied() else {
        return false;
    };
    let Some(scroll) = world.get_mut::<UiScroll>(target) else {
        return false;
    };
    let delta = [
        -scroll_units_to_pixels(raw_delta[0], scroll.wheel_speed),
        -scroll_units_to_pixels(raw_delta[1], scroll.wheel_speed),
    ];
    scroll.scroll_by(delta, viewport)
}

fn scroll_units_to_pixels(value: f32, wheel_speed: f32) -> f32 {
    if value.abs() <= 10.0 {
        value * wheel_speed
    } else {
        value
    }
}

fn event_with_id(
    kind: UiEventKind,
    entity: EntityId,
    ids: &FxHashMap<EntityId, Option<UiId>>,
) -> UiEvent {
    UiEvent::new(kind, entity, ids.get(&entity).cloned().flatten())
}

fn update_slider_from_pointer(
    world: &mut World,
    entity: EntityId,
    pointer: [f32; 2],
    rect: super::UiRect,
    interactive_by_entity: &FxHashMap<EntityId, bool>,
    id_by_entity: &FxHashMap<EntityId, Option<UiId>>,
    emitted: &mut Vec<UiEvent>,
) {
    if !interactive_by_entity.get(&entity).copied().unwrap_or(false) {
        return;
    }
    if let Some(slider) = world.get_mut::<UiSlider>(entity) {
        if slider.set_from_x(pointer[0], rect) {
            emitted.push(event_with_id(
                UiEventKind::ValueChanged,
                entity,
                id_by_entity,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs::World;
    use crate::input::{InteractionContext, MouseButton};

    use super::super::{
        update_ui, UiEventKind, UiEvents, UiNode, UiScroll, UiSlider, UiState, UiToggle,
    };
    use crate::input::Input;

    fn input_at(x: f32, y: f32) -> Input {
        let mut input = Input::new();
        input.set_mouse_position(x, y);
        input
    }

    #[test]
    fn hover_press_release_click_events_are_queued() {
        let mut world = World::new();
        let button = world.spawn((UiNode::panel(100.0, 40.0).id("play"),));

        let mut input = input_at(20.0, 20.0);
        update_ui(&mut world, &input, [800.0, 600.0]);
        {
            let events: Vec<_> = world
                .get_resource_mut::<UiEvents>()
                .unwrap()
                .drain()
                .map(|event| event.kind)
                .collect();
            assert_eq!(events, vec![UiEventKind::HoverStarted]);
        }

        input.mouse_button_down(MouseButton::Left as usize);
        update_ui(&mut world, &input, [800.0, 600.0]);
        assert_eq!(
            world.get_resource::<UiState>().unwrap().pressed(),
            Some(button)
        );
        {
            let events: Vec<_> = world
                .get_resource_mut::<UiEvents>()
                .unwrap()
                .drain()
                .map(|event| event.kind)
                .collect();
            assert!(events.contains(&UiEventKind::Pressed));
        }

        input.begin_frame();
        input.mouse_button_up(MouseButton::Left as usize);
        update_ui(&mut world, &input, [800.0, 600.0]);
        let events: Vec<_> = world
            .get_resource_mut::<UiEvents>()
            .unwrap()
            .drain()
            .map(|event| event.kind)
            .collect();
        assert!(events.contains(&UiEventKind::Released));
        assert!(events.contains(&UiEventKind::Clicked));
    }

    #[test]
    fn press_and_release_in_same_frame_still_clicks() {
        let mut world = World::new();
        let button = world.spawn((UiNode::panel(100.0, 40.0).id("play"),));

        let mut input = input_at(20.0, 20.0);
        input.mouse_button_down(MouseButton::Left as usize);
        input.mouse_button_up(MouseButton::Left as usize);
        update_ui(&mut world, &input, [800.0, 600.0]);

        assert_eq!(world.get_resource::<UiState>().unwrap().pressed(), None);
        let events: Vec<_> = world
            .get_resource_mut::<UiEvents>()
            .unwrap()
            .drain()
            .map(|event| (event.kind, event.entity))
            .collect();
        assert!(events.contains(&(UiEventKind::Pressed, button)));
        assert!(events.contains(&(UiEventKind::Released, button)));
        assert!(events.contains(&(UiEventKind::Clicked, button)));
    }

    #[test]
    fn wants_pointer_is_true_for_hovered_node() {
        let mut world = World::new();
        world.spawn((UiNode::panel(100.0, 40.0),));
        let input = input_at(20.0, 20.0);
        update_ui(&mut world, &input, [800.0, 600.0]);
        assert!(world.get_resource::<UiState>().unwrap().wants_pointer());
    }

    #[test]
    fn ui_hit_consumes_left_pointer_for_later_systems() {
        let mut world = World::new();
        world.spawn((UiNode::panel(100.0, 40.0),));

        let mut input = input_at(20.0, 20.0);
        input.mouse_button_down(MouseButton::Left as usize);
        update_ui(&mut world, &input, [800.0, 600.0]);

        let interaction = world.get_resource::<InteractionContext>().unwrap();
        assert!(interaction.pointer_consumed(MouseButton::Left));
    }

    #[test]
    fn input_transparent_ui_does_not_consume_left_pointer() {
        let mut world = World::new();
        world.spawn((UiNode::panel(100.0, 40.0).input_transparent(),));

        let mut input = input_at(20.0, 20.0);
        input.mouse_button_down(MouseButton::Left as usize);
        update_ui(&mut world, &input, [800.0, 600.0]);

        let interaction = world.get_resource::<InteractionContext>().unwrap();
        assert!(!interaction.pointer_consumed(MouseButton::Left));
    }

    #[test]
    fn wheel_scrolls_hovered_scroll_container() {
        let mut world = World::new();
        let panel = world.spawn((
            UiNode::panel(100.0, 80.0),
            UiScroll::vertical().content_size(100.0, 180.0),
        ));
        world.spawn((UiNode::panel(100.0, 30.0).child_of(panel).at(0.0, 100.0),));

        let mut input = input_at(20.0, 20.0);
        input.add_scroll_delta(0.0, -1.0);
        update_ui(&mut world, &input, [800.0, 600.0]);

        assert_eq!(world.get::<UiScroll>(panel).unwrap().offset[1], 40.0);
        let events: Vec<_> = world
            .get_resource_mut::<UiEvents>()
            .unwrap()
            .drain()
            .map(|event| (event.kind, event.entity))
            .collect();
        assert!(events.contains(&(UiEventKind::ValueChanged, panel)));
    }

    #[test]
    fn wheel_scroll_target_can_be_a_child_of_scroll_container() {
        let mut world = World::new();
        let panel = world.spawn((
            UiNode::panel(100.0, 80.0),
            UiScroll::vertical().content_size(100.0, 180.0),
        ));
        let _child = world.spawn((UiNode::panel(100.0, 30.0).child_of(panel),));

        let mut input = input_at(20.0, 20.0);
        input.add_scroll_delta(0.0, -1.0);
        update_ui(&mut world, &input, [800.0, 600.0]);

        assert_eq!(world.get::<UiScroll>(panel).unwrap().offset[1], 40.0);
    }

    #[test]
    fn slider_drag_updates_value_and_emits_change() {
        let mut world = World::new();
        let slider = world.spawn((
            UiNode::panel(200.0, 24.0).id("volume"),
            UiSlider::new(0.0, 0.0, 1.0),
        ));

        let mut input = input_at(150.0, 12.0);
        input.mouse_button_down(MouseButton::Left as usize);
        update_ui(&mut world, &input, [800.0, 600.0]);

        let value = world.get::<UiSlider>(slider).unwrap().value;
        assert!((value - 0.75).abs() < 0.001);
        let events: Vec<_> = world
            .get_resource_mut::<UiEvents>()
            .unwrap()
            .drain()
            .map(|event| event.kind)
            .collect();
        assert!(events.contains(&UiEventKind::ValueChanged));

        input.begin_frame();
        input.set_mouse_position(50.0, 12.0);
        update_ui(&mut world, &input, [800.0, 600.0]);
        let value = world.get::<UiSlider>(slider).unwrap().value;
        assert!((value - 0.25).abs() < 0.001);
    }

    #[test]
    fn slider_click_updates_value_even_when_released_same_frame() {
        let mut world = World::new();
        let slider = world.spawn((
            UiNode::panel(200.0, 24.0).id("volume"),
            UiSlider::new(0.0, 0.0, 1.0),
        ));

        let mut input = input_at(150.0, 12.0);
        input.mouse_button_down(MouseButton::Left as usize);
        input.mouse_button_up(MouseButton::Left as usize);
        update_ui(&mut world, &input, [800.0, 600.0]);

        let value = world.get::<UiSlider>(slider).unwrap().value;
        assert!((value - 0.75).abs() < 0.001);
        let events: Vec<_> = world
            .get_resource_mut::<UiEvents>()
            .unwrap()
            .drain()
            .map(|event| event.kind)
            .collect();
        assert!(events.contains(&UiEventKind::ValueChanged));
        assert!(events.contains(&UiEventKind::Clicked));
    }

    #[test]
    fn toggle_click_flips_checked_and_emits_change() {
        let mut world = World::new();
        let toggle = world.spawn((
            UiNode::panel(140.0, 30.0).id("assist"),
            UiToggle::new(false).label("Assist"),
        ));

        let mut input = input_at(20.0, 15.0);
        input.mouse_button_down(MouseButton::Left as usize);
        update_ui(&mut world, &input, [800.0, 600.0]);
        let _ = world
            .get_resource_mut::<UiEvents>()
            .unwrap()
            .drain()
            .count();

        input.begin_frame();
        input.mouse_button_up(MouseButton::Left as usize);
        update_ui(&mut world, &input, [800.0, 600.0]);

        assert!(world.get::<UiToggle>(toggle).unwrap().checked);
        let events: Vec<_> = world
            .get_resource_mut::<UiEvents>()
            .unwrap()
            .drain()
            .map(|event| event.kind)
            .collect();
        assert!(events.contains(&UiEventKind::ValueChanged));
        assert!(events.contains(&UiEventKind::Clicked));
    }

    #[test]
    fn toggle_same_frame_press_release_flips_checked() {
        let mut world = World::new();
        let toggle = world.spawn((
            UiNode::panel(140.0, 30.0).id("assist"),
            UiToggle::new(false).label("Assist"),
        ));

        let mut input = input_at(20.0, 15.0);
        input.mouse_button_down(MouseButton::Left as usize);
        input.mouse_button_up(MouseButton::Left as usize);
        update_ui(&mut world, &input, [800.0, 600.0]);

        assert!(world.get::<UiToggle>(toggle).unwrap().checked);
        let events: Vec<_> = world
            .get_resource_mut::<UiEvents>()
            .unwrap()
            .drain()
            .map(|event| event.kind)
            .collect();
        assert!(events.contains(&UiEventKind::ValueChanged));
        assert!(events.contains(&UiEventKind::Clicked));
    }
}
