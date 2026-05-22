use sky_engine::ecs::World;
use sky_engine::input::{Input, KeyCode};
use sky_engine::math::{Projection, Transform, Vec2};
use sky_engine::render::{Color, SpriteRenderer};

use crate::model::{
    cell_world, find_path, in_bounds, manhattan, structure_cost, world_to_cell, GameState,
    ItemStack, MapState, MovePurpose, PawnJob, ResourceKind, RimworldTextures, StructureKind,
    SurfaceInfo, ToolMode, WorldVisuals, BUILD_TIME, CAMERA_SPEED, EAT_TIME, GRID_HEIGHT,
    GRID_WIDTH, HARVEST_TIME, HUNGER_DECAY_RATE, PAWN_SPEED, REST_DECAY_RATE, SLEEP_RECOVERY_RATE,
    SOW_TIME, TILE_SIZE, TITLE_UPDATE_FRAMES, ZOOM_MAX, ZOOM_MIN,
};

pub fn install_systems(world: &mut World) {
    world.group("input").add(handle_input_system);
    world.group("simulation").add(needs_and_farms_system);
    world.group("simulation").add(simulation_system);
    world.group("presentation").add(sync_visuals_system);
}

fn handle_input_system(world: &mut World) {
    let input = *world
        .get_resource::<Input>()
        .expect("rimworld requires Input");
    let dt = world.time.delta;
    let surface = world
        .get_resource::<SurfaceInfo>()
        .expect("rimworld requires SurfaceInfo")
        .size;

    let mut camera_input = Vec2::ZERO;
    {
        let game = world.get_resource_mut::<GameState>().unwrap();
        if input.key_pressed(KeyCode::Space) {
            game.paused = !game.paused;
        }
        if input.key_pressed(KeyCode::Digit1) {
            game.tool = ToolMode::Inspect;
        }
        if input.key_pressed(KeyCode::Digit2) {
            game.tool = ToolMode::Stockpile;
        }
        if input.key_pressed(KeyCode::Digit3) {
            game.tool = ToolMode::Harvest;
        }
        if input.key_pressed(KeyCode::Digit4) {
            game.tool = ToolMode::BuildWall;
        }
        if input.key_pressed(KeyCode::Digit5) {
            game.tool = ToolMode::BuildBed;
        }
        if input.key_pressed(KeyCode::Digit6) {
            game.tool = ToolMode::BuildFarm;
        }
        if input.key_pressed(KeyCode::Digit0) {
            game.tool = ToolMode::Cancel;
        }

        if input.key_held(KeyCode::KeyA) || input.key_held(KeyCode::ArrowLeft) {
            camera_input[0] -= 1.0;
        }
        if input.key_held(KeyCode::KeyD) || input.key_held(KeyCode::ArrowRight) {
            camera_input[0] += 1.0;
        }
        if input.key_held(KeyCode::KeyW) || input.key_held(KeyCode::ArrowUp) {
            camera_input[1] += 1.0;
        }
        if input.key_held(KeyCode::KeyS) || input.key_held(KeyCode::ArrowDown) {
            camera_input[1] -= 1.0;
        }

        if camera_input.length_squared() > 0.0 {
            game.camera_center += camera_input.normalized() * (CAMERA_SPEED * dt / game.zoom);
        }

        let zoom_delta = if input.key_held(KeyCode::KeyQ) {
            0.9 * dt
        } else if input.key_held(KeyCode::KeyE) {
            -0.9 * dt
        } else {
            0.0
        } + input.scroll_delta()[1] * 0.10;
        if zoom_delta != 0.0 {
            game.zoom = (game.zoom + zoom_delta).clamp(ZOOM_MIN, ZOOM_MAX);
        }

        clamp_camera(game);
        game.hovered_cell =
            screen_to_cell(game, surface, input.mouse_logical_position().to_array());
    }

    let inspect_mode = world.get_resource::<GameState>().unwrap().tool == ToolMode::Inspect;
    if inspect_mode && input.mouse_left_pressed() {
        world
            .get_resource_mut::<GameState>()
            .unwrap()
            .drag_select_start = Some(input.mouse_logical_position().to_array());
    }
    if inspect_mode && input.mouse_left_released() {
        let hovered = world.get_resource::<GameState>().unwrap().hovered_cell;
        let drag_start = world
            .get_resource_mut::<GameState>()
            .unwrap()
            .drag_select_start
            .take();
        if let Some(start) = drag_start {
            let end = input.mouse_logical_position().to_array();
            if (end[0] - start[0]).abs() > 8.0 || (end[1] - start[1]).abs() > 8.0 {
                select_in_rect(world, start, end, surface);
            } else if let Some(cell) = hovered {
                select_at_cell(world, cell);
            }
        }
    }

    if !inspect_mode && input.mouse_left_pressed() {
        if let Some(cell) = world.get_resource::<GameState>().unwrap().hovered_cell {
            apply_tool(world, cell);
        }
    }

    if input.mouse_right_pressed() {
        let hovered = world.get_resource::<GameState>().unwrap().hovered_cell;
        let selected_pawns = world
            .get_resource::<GameState>()
            .unwrap()
            .selected_pawns
            .clone();
        if let Some(cell) = hovered {
            issue_context_order(world, &selected_pawns, cell);
        }
    }
}

fn needs_and_farms_system(world: &mut World) {
    let dt = world.time.delta;
    {
        let game = world.get_resource_mut::<GameState>().unwrap();
        for pawn in &mut game.pawns {
            pawn.hunger = (pawn.hunger - HUNGER_DECAY_RATE * dt).clamp(0.0, 1.0);
            pawn.rest = (pawn.rest - REST_DECAY_RATE * dt).clamp(0.0, 1.0);
        }
    }

    let map = world.get_resource_mut::<MapState>().unwrap();
    for cell in &mut map.cells {
        if let Some(farm) = cell.farm.as_mut() {
            if farm.planted && !farm.ready {
                farm.growth = (farm.growth
                    + dt * if matches!(cell.terrain, crate::model::TerrainKind::RichSoil) {
                        0.18
                    } else {
                        0.12
                    })
                .clamp(0.0, 1.0);
                if farm.growth >= 1.0 {
                    farm.ready = true;
                }
            }
        }
    }
}

fn simulation_system(world: &mut World) {
    if world.get_resource::<GameState>().unwrap().paused {
        return;
    }

    let pawn_count = world.get_resource::<GameState>().unwrap().pawns.len();
    for pawn_index in 0..pawn_count {
        let job = {
            let game = world.get_resource_mut::<GameState>().unwrap();
            std::mem::replace(&mut game.pawns[pawn_index].job, PawnJob::Idle)
        };
        let next_job = advance_job(world, pawn_index, job);
        world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index].job = next_job;
        if matches!(
            world.get_resource::<GameState>().unwrap().pawns[pawn_index].job,
            PawnJob::Idle
        ) {
            try_assign_job(world, pawn_index);
        }
    }
}

fn advance_job(world: &mut World, pawn_index: usize, job: PawnJob) -> PawnJob {
    match job {
        PawnJob::Idle => PawnJob::Idle,
        PawnJob::Moving {
            purpose,
            path,
            mut step_index,
        } => {
            if path.is_empty() || step_index >= path.len() {
                return on_arrived(world, pawn_index, purpose);
            }
            let target = Vec2::from_array(cell_world(path[step_index][0], path[step_index][1]));
            let dt = world.time.delta;
            let pawn_pos = world.get_resource::<GameState>().unwrap().pawns[pawn_index].pos;
            let to_target = target - pawn_pos;
            let distance = to_target.length();
            if distance <= PAWN_SPEED * dt {
                world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index].pos = target;
                step_index += 1;
                if step_index >= path.len() {
                    return on_arrived(world, pawn_index, purpose);
                }
            } else {
                world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index].pos +=
                    to_target.normalized() * (PAWN_SPEED * dt);
            }
            PawnJob::Moving {
                purpose,
                path,
                step_index,
            }
        }
        PawnJob::Harvesting { cell, mut progress } => {
            let Some(node) = world
                .get_resource::<MapState>()
                .unwrap()
                .cell(cell[0], cell[1])
                .and_then(|tile| tile.node)
            else {
                return PawnJob::Idle;
            };
            progress += world.time.delta;
            if progress < HARVEST_TIME {
                return PawnJob::Harvesting { cell, progress };
            }

            let yield_stack = node.kind.harvest_yield();
            if let Some(tile) = world
                .get_resource_mut::<MapState>()
                .unwrap()
                .cell_mut(cell[0], cell[1])
            {
                tile.node = None;
                tile.item_reserved = false;
                merge_item(tile, yield_stack);
            }
            PawnJob::Idle
        }
        PawnJob::Building { cell, mut progress } => {
            let Some(blueprint) = world
                .get_resource::<MapState>()
                .unwrap()
                .cell(cell[0], cell[1])
                .and_then(|tile| tile.blueprint)
            else {
                return PawnJob::Idle;
            };
            progress += world.time.delta;
            if progress < BUILD_TIME {
                return PawnJob::Building { cell, progress };
            }
            if let Some(tile) = world
                .get_resource_mut::<MapState>()
                .unwrap()
                .cell_mut(cell[0], cell[1])
            {
                tile.structure = Some(blueprint.kind);
                tile.blueprint = None;
                tile.stockpile = false;
            }
            PawnJob::Idle
        }
        PawnJob::Sowing { cell, mut progress } => {
            progress += world.time.delta;
            if progress < SOW_TIME {
                return PawnJob::Sowing { cell, progress };
            }
            if let Some(tile) = world
                .get_resource_mut::<MapState>()
                .unwrap()
                .cell_mut(cell[0], cell[1])
            {
                if let Some(farm) = tile.farm.as_mut() {
                    farm.planted = true;
                    farm.growth = 0.0;
                    farm.ready = false;
                    farm.reserved = false;
                }
            }
            PawnJob::Idle
        }
        PawnJob::Eating { mut progress } => {
            progress += world.time.delta;
            if progress < EAT_TIME {
                return PawnJob::Eating { progress };
            }
            let pawn = &mut world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index];
            pawn.hunger = (pawn.hunger + 0.58).clamp(0.0, 1.0);
            PawnJob::Idle
        }
        PawnJob::Sleeping { cell } => {
            let dt = world.time.delta;
            let pawn = &mut world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index];
            pawn.rest = (pawn.rest + SLEEP_RECOVERY_RATE * dt).clamp(0.0, 1.0);
            if pawn.rest >= 0.97 {
                return PawnJob::Idle;
            }
            PawnJob::Sleeping { cell }
        }
    }
}

fn on_arrived(world: &mut World, pawn_index: usize, purpose: MovePurpose) -> PawnJob {
    match purpose {
        MovePurpose::Harvest(cell) => PawnJob::Harvesting {
            cell,
            progress: 0.0,
        },
        MovePurpose::Build(cell) => PawnJob::Building {
            cell,
            progress: 0.0,
        },
        MovePurpose::Sow(cell) => PawnJob::Sowing {
            cell,
            progress: 0.0,
        },
        MovePurpose::EatAt(_cell) => PawnJob::Eating { progress: 0.0 },
        MovePurpose::SleepAt(cell) => PawnJob::Sleeping { cell },
        MovePurpose::HaulPickup(pickup, dropoff) => {
            let Some(tile) = world
                .get_resource_mut::<MapState>()
                .unwrap()
                .cell_mut(pickup[0], pickup[1])
            else {
                return PawnJob::Idle;
            };
            let Some(item) = tile.item.take() else {
                tile.item_reserved = false;
                return PawnJob::Idle;
            };
            tile.item_reserved = false;
            world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index].carrying = Some(item);
            let start =
                world_to_cell(world.get_resource::<GameState>().unwrap().pawns[pawn_index].pos);
            let path = find_path(
                world.get_resource::<MapState>().unwrap(),
                start,
                dropoff,
                true,
            );
            if path.is_empty() {
                drop_item_at_current_cell(world, pawn_index);
                PawnJob::Idle
            } else {
                PawnJob::Moving {
                    purpose: MovePurpose::HaulDeliver(dropoff),
                    path,
                    step_index: 0,
                }
            }
        }
        MovePurpose::HaulDeliver(dropoff) => {
            let carried = world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index]
                .carrying
                .take();
            let Some(item) = carried else {
                return PawnJob::Idle;
            };
            let drop_ok = world
                .get_resource::<MapState>()
                .unwrap()
                .cell(dropoff[0], dropoff[1])
                .is_some_and(|tile| tile.stockpile && tile.structure.is_none());
            if drop_ok {
                world
                    .get_resource_mut::<GameState>()
                    .unwrap()
                    .resources
                    .add(item.kind, item.amount);
            } else {
                let pos = world.get_resource::<GameState>().unwrap().pawns[pawn_index].pos;
                drop_stack(world, world_to_cell(pos), item);
            }
            PawnJob::Idle
        }
        MovePurpose::PlayerMove(_cell) => PawnJob::Idle,
    }
}

fn try_assign_job(world: &mut World, pawn_index: usize) {
    let pawn = {
        let game = world.get_resource::<GameState>().unwrap();
        (
            game.pawns[pawn_index].pos,
            game.pawns[pawn_index].hunger,
            game.pawns[pawn_index].rest,
            game.pawns[pawn_index].carrying,
        )
    };
    let current = world_to_cell(pawn.0);

    if pawn.1 < 0.32
        && world
            .get_resource::<GameState>()
            .unwrap()
            .resources
            .amount(ResourceKind::Food)
            > 0
    {
        if world
            .get_resource_mut::<GameState>()
            .unwrap()
            .resources
            .consume(ResourceKind::Food, 1)
        {
            world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index].job =
                PawnJob::Moving {
                    purpose: MovePurpose::EatAt(current),
                    path: vec![current],
                    step_index: 0,
                };
            return;
        }
    }

    if pawn.2 < 0.26 {
        if let Some(bed_cell) = find_nearest_structure(
            world.get_resource::<MapState>().unwrap(),
            current,
            StructureKind::Bed,
        ) {
            let path = find_path(
                world.get_resource::<MapState>().unwrap(),
                current,
                bed_cell,
                true,
            );
            if !path.is_empty() {
                world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index].job =
                    PawnJob::Moving {
                        purpose: MovePurpose::SleepAt(bed_cell),
                        path,
                        step_index: 0,
                    };
                return;
            }
        }
    }

    if pawn.3.is_some() {
        if let Some(dropoff) = find_best_stockpile(world, current) {
            let path = find_path(
                world.get_resource::<MapState>().unwrap(),
                current,
                dropoff,
                true,
            );
            if !path.is_empty() {
                world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index].job =
                    PawnJob::Moving {
                        purpose: MovePurpose::HaulDeliver(dropoff),
                        path,
                        step_index: 0,
                    };
                return;
            }
        }
    }

    if let Some((pickup, dropoff)) = find_haul_job(world, current) {
        if let Some(tile) = world
            .get_resource_mut::<MapState>()
            .unwrap()
            .cell_mut(pickup[0], pickup[1])
        {
            tile.item_reserved = true;
        }
        let path = find_path(
            world.get_resource::<MapState>().unwrap(),
            current,
            pickup,
            true,
        );
        if !path.is_empty() {
            world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index].job =
                PawnJob::Moving {
                    purpose: MovePurpose::HaulPickup(pickup, dropoff),
                    path,
                    step_index: 0,
                };
            return;
        }
        if let Some(tile) = world
            .get_resource_mut::<MapState>()
            .unwrap()
            .cell_mut(pickup[0], pickup[1])
        {
            tile.item_reserved = false;
        }
    }

    if let Some(build_cell) = find_build_job(world, current) {
        let kind = world
            .get_resource::<MapState>()
            .unwrap()
            .cell(build_cell[0], build_cell[1])
            .and_then(|tile| tile.blueprint)
            .map(|bp| bp.kind);
        if let Some(kind) = kind {
            let cost = structure_cost(kind);
            if world
                .get_resource_mut::<GameState>()
                .unwrap()
                .resources
                .consume(cost.kind, cost.amount)
            {
                if let Some(tile) = world
                    .get_resource_mut::<MapState>()
                    .unwrap()
                    .cell_mut(build_cell[0], build_cell[1])
                {
                    if let Some(blueprint) = tile.blueprint.as_mut() {
                        blueprint.reserved = true;
                    }
                }
                let path = find_path(
                    world.get_resource::<MapState>().unwrap(),
                    current,
                    build_cell,
                    true,
                );
                if !path.is_empty() {
                    world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index].job =
                        PawnJob::Moving {
                            purpose: MovePurpose::Build(build_cell),
                            path,
                            step_index: 0,
                        };
                    return;
                }
                world
                    .get_resource_mut::<GameState>()
                    .unwrap()
                    .resources
                    .add(cost.kind, cost.amount);
            }
        }
    }

    if let Some(farm_cell) = find_farm_job(world, current) {
        if let Some(tile) = world
            .get_resource_mut::<MapState>()
            .unwrap()
            .cell_mut(farm_cell[0], farm_cell[1])
        {
            if let Some(farm) = tile.farm.as_mut() {
                farm.reserved = true;
            }
        }
        let purpose = {
            let farm = world
                .get_resource::<MapState>()
                .unwrap()
                .cell(farm_cell[0], farm_cell[1])
                .and_then(|tile| tile.farm)
                .unwrap();
            if farm.ready {
                MovePurpose::Harvest(farm_cell)
            } else {
                MovePurpose::Sow(farm_cell)
            }
        };
        let path = find_path(
            world.get_resource::<MapState>().unwrap(),
            current,
            farm_cell,
            true,
        );
        if !path.is_empty() {
            world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index].job =
                PawnJob::Moving {
                    purpose,
                    path,
                    step_index: 0,
                };
            return;
        }
        if let Some(tile) = world
            .get_resource_mut::<MapState>()
            .unwrap()
            .cell_mut(farm_cell[0], farm_cell[1])
        {
            if let Some(farm) = tile.farm.as_mut() {
                farm.reserved = false;
            }
        }
    }

    if let Some(harvest_cell) = find_harvest_job(world, current) {
        if let Some(tile) = world
            .get_resource_mut::<MapState>()
            .unwrap()
            .cell_mut(harvest_cell[0], harvest_cell[1])
        {
            if let Some(node) = tile.node.as_mut() {
                node.reserved = true;
            }
        }
        let path = find_path(
            world.get_resource::<MapState>().unwrap(),
            current,
            harvest_cell,
            true,
        );
        if !path.is_empty() {
            world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index].job =
                PawnJob::Moving {
                    purpose: MovePurpose::Harvest(harvest_cell),
                    path,
                    step_index: 0,
                };
        }
    }
}

fn find_best_stockpile(world: &World, from: [i32; 2]) -> Option<[i32; 2]> {
    let map = world.get_resource::<MapState>().unwrap();
    let mut best = None;
    for y in 0..GRID_HEIGHT as i32 {
        for x in 0..GRID_WIDTH as i32 {
            let Some(cell) = map.cell(x, y) else { continue };
            if !cell.stockpile || cell.structure.is_some() {
                continue;
            }
            let dist = manhattan(from, [x, y]);
            match best {
                Some((_, best_dist)) if dist >= best_dist => {}
                _ => best = Some(([x, y], dist)),
            }
        }
    }
    best.map(|(cell, _)| cell)
}

fn find_haul_job(world: &World, from: [i32; 2]) -> Option<([i32; 2], [i32; 2])> {
    let dropoff = find_best_stockpile(world, from)?;
    let map = world.get_resource::<MapState>().unwrap();
    let mut best = None;
    for y in 0..GRID_HEIGHT as i32 {
        for x in 0..GRID_WIDTH as i32 {
            let Some(cell) = map.cell(x, y) else { continue };
            if cell.item.is_none() || cell.item_reserved || cell.stockpile {
                continue;
            }
            let dist = manhattan(from, [x, y]) + manhattan([x, y], dropoff);
            match best {
                Some((_, best_dist)) if dist >= best_dist => {}
                _ => best = Some(([x, y], dist)),
            }
        }
    }
    best.map(|(pickup, _)| (pickup, dropoff))
}

fn find_build_job(world: &World, from: [i32; 2]) -> Option<[i32; 2]> {
    let map = world.get_resource::<MapState>().unwrap();
    let mut best = None;
    for y in 0..GRID_HEIGHT as i32 {
        for x in 0..GRID_WIDTH as i32 {
            let Some(blueprint) = map.cell(x, y).and_then(|cell| cell.blueprint) else {
                continue;
            };
            if blueprint.reserved {
                continue;
            }
            let dist = manhattan(from, [x, y]);
            match best {
                Some((_, best_dist)) if dist >= best_dist => {}
                _ => best = Some(([x, y], dist)),
            }
        }
    }
    best.map(|(cell, _)| cell)
}

fn find_harvest_job(world: &World, from: [i32; 2]) -> Option<[i32; 2]> {
    let map = world.get_resource::<MapState>().unwrap();
    let mut best = None;
    for y in 0..GRID_HEIGHT as i32 {
        for x in 0..GRID_WIDTH as i32 {
            let node = map.cell(x, y).and_then(|cell| cell.node);
            let Some(node) = node else { continue };
            if !node.designated || node.reserved {
                continue;
            }
            let dist = manhattan(from, [x, y]);
            match best {
                Some((_, best_dist)) if dist >= best_dist => {}
                _ => best = Some(([x, y], dist)),
            }
        }
    }
    best.map(|(cell, _)| cell)
}

fn find_farm_job(world: &World, from: [i32; 2]) -> Option<[i32; 2]> {
    let map = world.get_resource::<MapState>().unwrap();
    let mut best = None;
    for y in 0..GRID_HEIGHT as i32 {
        for x in 0..GRID_WIDTH as i32 {
            let Some(farm) = map.cell(x, y).and_then(|cell| cell.farm) else {
                continue;
            };
            if !farm.designated || farm.reserved {
                continue;
            }
            if farm.planted && !farm.ready {
                continue;
            }
            let dist = manhattan(from, [x, y]);
            match best {
                Some((_, best_dist)) if dist >= best_dist => {}
                _ => best = Some(([x, y], dist)),
            }
        }
    }
    best.map(|(cell, _)| cell)
}

fn find_nearest_structure(map: &MapState, from: [i32; 2], kind: StructureKind) -> Option<[i32; 2]> {
    let mut best = None;
    for y in 0..GRID_HEIGHT as i32 {
        for x in 0..GRID_WIDTH as i32 {
            if map
                .cell(x, y)
                .is_some_and(|cell| cell.structure == Some(kind))
            {
                let dist = manhattan(from, [x, y]);
                match best {
                    Some((_, best_dist)) if dist >= best_dist => {}
                    _ => best = Some(([x, y], dist)),
                }
            }
        }
    }
    best.map(|(cell, _)| cell)
}

fn drop_item_at_current_cell(world: &mut World, pawn_index: usize) {
    let carried = world.get_resource_mut::<GameState>().unwrap().pawns[pawn_index]
        .carrying
        .take();
    let Some(item) = carried else {
        return;
    };
    let pos = world.get_resource::<GameState>().unwrap().pawns[pawn_index].pos;
    drop_stack(world, world_to_cell(pos), item);
}

fn drop_stack(world: &mut World, cell: [i32; 2], stack: ItemStack) {
    let Some(tile) = world
        .get_resource_mut::<MapState>()
        .unwrap()
        .cell_mut(cell[0], cell[1])
    else {
        return;
    };
    merge_item(tile, stack);
}

fn merge_item(tile: &mut crate::model::MapCell, stack: ItemStack) {
    match tile.item {
        Some(mut existing) if existing.kind == stack.kind => {
            existing.amount += stack.amount;
            tile.item = Some(existing);
        }
        Some(_) => {}
        None => tile.item = Some(stack),
    }
}

fn sync_visuals_system(world: &mut World) {
    sync_camera(world);
    sync_map_visuals(world);
    sync_pawn_visuals(world);
    sync_hover_visual(world);
    sync_selection_visual(world);
    update_title(world);
}

fn sync_camera(world: &mut World) {
    let surface_size = world.get_resource::<SurfaceInfo>().unwrap().size;
    let (camera_entity, center, zoom_value) = {
        let visuals = world.get_resource::<WorldVisuals>().unwrap();
        let game = world.get_resource::<GameState>().unwrap();
        (visuals.camera_entity, game.camera_center, game.zoom)
    };
    if let Some(transform) = world.get_mut::<Transform>(camera_entity) {
        *transform = Transform::from_xyz(center.x(), center.y(), 0.0);
    }
    if let Some(projection) = world.get_mut::<Projection>(camera_entity) {
        if let Projection::Orthographic { height, zoom } = projection {
            *height = surface_size[1].max(1.0);
            *zoom = zoom_value;
        }
    }
}

fn sync_map_visuals(world: &mut World) {
    let cell_visuals = world.get_resource::<WorldVisuals>().unwrap().cells.clone();
    let cells = world.get_resource::<MapState>().unwrap().cells.clone();
    let tree_texture = world
        .get_resource::<RimworldTextures>()
        .map(|textures| textures.tree.clone());
    for y in 0..GRID_HEIGHT as i32 {
        for x in 0..GRID_WIDTH as i32 {
            let idx = crate::model::cell_index(x, y);
            let cell = cells[idx];
            let visual = cell_visuals[idx];

            if let Some(sprite) = world.get_mut::<SpriteRenderer>(visual.ground) {
                sprite.color = match cell.terrain {
                    crate::model::TerrainKind::Grass => Color::rgb(
                        0.10 + cell.grass_density * 0.08,
                        0.42 + cell.grass_density * 0.30,
                        0.10 + cell.grass_density * 0.05,
                    ),
                    crate::model::TerrainKind::RichSoil => Color::rgb(0.46, 0.34, 0.16),
                    crate::model::TerrainKind::Gravel => Color::rgb(0.28, 0.29, 0.30),
                };
            }

            if let Some(sprite) = world.get_mut::<SpriteRenderer>(visual.zone) {
                sprite.visible = cell.stockpile || cell.farm.is_some();
                sprite.color = if cell.stockpile {
                    Color::new(0.22, 0.70, 1.0, 0.20)
                } else if let Some(farm) = cell.farm {
                    if farm.ready {
                        Color::new(0.98, 0.82, 0.26, 0.25)
                    } else if farm.planted {
                        Color::new(0.24, 0.80, 0.34, 0.20)
                    } else {
                        Color::new(0.38, 0.64, 0.20, 0.16)
                    }
                } else {
                    Color::new(0.0, 0.0, 0.0, 0.0)
                };
            }

            if let Some(sprite) = world.get_mut::<SpriteRenderer>(visual.content) {
                sprite.visible = false;
                sprite.width = TILE_SIZE - 8.0;
                sprite.height = TILE_SIZE - 8.0;
                sprite.texture = None;
                if let Some(structure) = cell.structure {
                    sprite.visible = true;
                    match structure {
                        StructureKind::Wall => {
                            sprite.color = Color::rgb(0.74, 0.78, 0.82);
                            sprite.width = TILE_SIZE - 4.0;
                            sprite.height = TILE_SIZE - 4.0;
                        }
                        StructureKind::Bed => {
                            sprite.color = Color::rgb(0.87, 0.58, 0.30);
                            sprite.width = TILE_SIZE - 6.0;
                            sprite.height = TILE_SIZE - 10.0;
                        }
                    }
                } else if let Some(node) = cell.node {
                    sprite.visible = true;
                    match node.kind {
                        crate::model::NodeKind::Tree => {
                            sprite.texture = tree_texture.clone();
                            sprite.color = if node.designated {
                                Color::new(0.78, 0.92, 0.78, 1.0)
                            } else {
                                Color::WHITE
                            };
                            sprite.width = TILE_SIZE * 1.05;
                            sprite.height = sprite.width * (120.0 / 86.0);
                        }
                        crate::model::NodeKind::Ore => {
                            sprite.color = Color::rgb(0.58, 0.68, 0.84);
                            sprite.width = TILE_SIZE - 6.0;
                            sprite.height = TILE_SIZE - 8.0;
                        }
                        crate::model::NodeKind::BerryBush => {
                            sprite.color = Color::rgb(0.90, 0.18, 0.40);
                            sprite.width = TILE_SIZE - 8.0;
                            sprite.height = TILE_SIZE - 8.0;
                        }
                    }
                } else if let Some(item) = cell.item {
                    sprite.visible = true;
                    sprite.width = TILE_SIZE * 0.48;
                    sprite.height = TILE_SIZE * 0.48;
                    sprite.color = match item.kind {
                        ResourceKind::Wood => Color::rgb(0.63, 0.40, 0.22),
                        ResourceKind::Steel => Color::rgb(0.58, 0.68, 0.82),
                        ResourceKind::Food => Color::rgb(0.92, 0.24, 0.42),
                    };
                } else if let Some(farm) = cell.farm {
                    if farm.planted {
                        sprite.visible = true;
                        sprite.width = TILE_SIZE - 8.0;
                        sprite.height = TILE_SIZE - 8.0;
                        let g = 0.36 + farm.growth * 0.46;
                        let r = 0.22 + farm.growth * 0.46;
                        sprite.color = if farm.ready {
                            Color::rgb(0.92, 0.82, 0.22)
                        } else {
                            Color::rgb(r, g, 0.16)
                        };
                    }
                }
            }

            if let Some(sprite) = world.get_mut::<SpriteRenderer>(visual.overlay) {
                sprite.visible = false;
                sprite.width = TILE_SIZE - 4.0;
                sprite.height = TILE_SIZE - 4.0;
                if let Some(blueprint) = cell.blueprint {
                    sprite.visible = true;
                    sprite.color = match blueprint.kind {
                        StructureKind::Wall => Color::new(0.56, 0.86, 1.0, 0.48),
                        StructureKind::Bed => Color::new(1.0, 0.74, 0.34, 0.48),
                    };
                } else if cell.node.is_some_and(|node| node.designated) {
                    sprite.visible = true;
                    sprite.color = Color::new(1.0, 0.46, 0.26, 0.26);
                }
            }
        }
    }
}

fn sync_pawn_visuals(world: &mut World) {
    let selected_pawns = world
        .get_resource::<GameState>()
        .unwrap()
        .selected_pawns
        .clone();
    let pawn_data = {
        let game = world.get_resource::<GameState>().unwrap();
        game.pawns
            .iter()
            .enumerate()
            .map(|(index, pawn)| {
                (
                    index,
                    pawn.visual.body,
                    pawn.visual.shadow,
                    pawn.pos,
                    pawn.carrying,
                    pawn.tint,
                    pawn.hunger,
                    pawn.rest,
                )
            })
            .collect::<Vec<_>>()
    };
    for (index, body, shadow, pos, carrying, tint, hunger, rest) in pawn_data {
        if let Some(transform) = world.get_mut::<Transform>(body) {
            *transform = Transform::from_xyz(pos.x(), pos.y(), 0.42);
        }
        if let Some(sprite) = world.get_mut::<SpriteRenderer>(body) {
            let is_selected = selected_pawns.contains(&index);
            sprite.width = if is_selected {
                TILE_SIZE * 0.92
            } else {
                TILE_SIZE * 0.82
            };
            sprite.height = if is_selected {
                TILE_SIZE * 0.92
            } else {
                TILE_SIZE * 0.82
            };
            let fatigue = (1.0 - rest) * 0.28;
            let hunger_tint = (1.0 - hunger) * 0.35;
            sprite.color = carrying.map_or(
                Color::new(
                    (tint.r + hunger_tint).min(1.0),
                    (tint.g - fatigue).max(0.12),
                    (tint.b - fatigue * 0.5).max(0.12),
                    1.0,
                ),
                |item| match item.kind {
                    ResourceKind::Wood => Color::rgb(0.88, 0.67, 0.38),
                    ResourceKind::Steel => Color::rgb(0.76, 0.86, 1.0),
                    ResourceKind::Food => Color::rgb(1.0, 0.42, 0.56),
                },
            );
        }
        if let Some(transform) = world.get_mut::<Transform>(shadow) {
            *transform = Transform::from_xyz(pos.x(), pos.y() - 5.0, 0.34);
        }
    }
}

fn sync_hover_visual(world: &mut World) {
    let (hover_entity, hovered_cell, tool) = {
        let visuals = world.get_resource::<WorldVisuals>().unwrap();
        let game = world.get_resource::<GameState>().unwrap();
        (visuals.hover_entity, game.hovered_cell, game.tool)
    };
    if let Some(sprite) = world.get_mut::<SpriteRenderer>(hover_entity) {
        sprite.visible = hovered_cell.is_some();
        sprite.color = match tool {
            ToolMode::Inspect => Color::new(1.0, 1.0, 1.0, 0.14),
            ToolMode::Stockpile => Color::new(0.22, 0.70, 1.0, 0.22),
            ToolMode::Harvest => Color::new(1.0, 0.46, 0.26, 0.24),
            ToolMode::BuildWall => Color::new(0.72, 0.82, 1.0, 0.24),
            ToolMode::BuildBed => Color::new(0.86, 0.70, 0.36, 0.24),
            ToolMode::BuildFarm => Color::new(0.34, 0.80, 0.26, 0.22),
            ToolMode::Cancel => Color::new(1.0, 0.22, 0.22, 0.18),
        };
    }
    if let Some(cell) = hovered_cell {
        if let Some(transform) = world.get_mut::<Transform>(hover_entity) {
            let pos = cell_world(cell[0], cell[1]);
            *transform = Transform::from_xyz(pos[0], pos[1], 0.26);
        }
    }
}

fn sync_selection_visual(world: &mut World) {
    let (selection_entity, selected_cell, selected_pawn) = {
        let visuals = world.get_resource::<WorldVisuals>().unwrap();
        let game = world.get_resource::<GameState>().unwrap();
        (
            visuals.selection_entity,
            game.selected_cell,
            game.selected_pawn,
        )
    };
    let selection_pos = if let Some(cell) = selected_cell {
        Some(cell_world(cell[0], cell[1]))
    } else if let Some(index) = selected_pawn {
        world
            .get_resource::<GameState>()
            .unwrap()
            .pawns
            .get(index)
            .map(|pawn| [pawn.pos.x(), pawn.pos.y()])
    } else {
        None
    };
    if let Some(sprite) = world.get_mut::<SpriteRenderer>(selection_entity) {
        sprite.visible = selection_pos.is_some();
    }
    if let Some(pos) = selection_pos {
        if let Some(transform) = world.get_mut::<Transform>(selection_entity) {
            *transform = Transform::from_xyz(pos[0], pos[1], 0.30);
        }
    }
}

fn update_title(world: &mut World) {
    let (
        next_frame_count,
        tool,
        paused,
        resources,
        hovered_cell,
        selected_pawn,
        selected_count,
        pawn_summary,
    ) = {
        let game = world.get_resource::<GameState>().unwrap();
        (
            game.frame_count + 1,
            game.tool,
            game.paused,
            (
                game.resources.wood,
                game.resources.steel,
                game.resources.food,
            ),
            game.hovered_cell,
            game.selected_pawn,
            game.selected_pawns.len(),
            game.pawns
                .iter()
                .map(|pawn| match &pawn.job {
                    PawnJob::Idle => format!("{} idle", pawn.name),
                    PawnJob::Moving { purpose, .. } => match purpose {
                        MovePurpose::Harvest(_) => format!("{} walk→harvest", pawn.name),
                        MovePurpose::Build(_) => format!("{} walk→build", pawn.name),
                        MovePurpose::Sow(_) => format!("{} walk→sow", pawn.name),
                        MovePurpose::EatAt(_) => format!("{} walk→eat", pawn.name),
                        MovePurpose::SleepAt(_) => format!("{} walk→sleep", pawn.name),
                        MovePurpose::HaulPickup(_, _) => format!("{} walk→haul", pawn.name),
                        MovePurpose::HaulDeliver(_) => format!("{} deliver", pawn.name),
                        MovePurpose::PlayerMove(_) => format!("{} moving", pawn.name),
                    },
                    PawnJob::Harvesting { .. } => format!("{} harvesting", pawn.name),
                    PawnJob::Building { .. } => format!("{} building", pawn.name),
                    PawnJob::Sowing { .. } => format!("{} sowing", pawn.name),
                    PawnJob::Eating { .. } => format!("{} eating", pawn.name),
                    PawnJob::Sleeping { .. } => format!("{} sleeping", pawn.name),
                })
                .collect::<Vec<_>>()
                .join(" | "),
        )
    };

    if next_frame_count % TITLE_UPDATE_FRAMES != 0 {
        world.get_resource_mut::<GameState>().unwrap().frame_count = next_frame_count;
        return;
    }

    let hover = hovered_cell
        .map(|cell| describe_cell(world.get_resource::<MapState>().unwrap(), cell))
        .unwrap_or_else(|| "Hover a tile".to_string());
    let selection = selected_pawn
        .and_then(|index| world.get_resource::<GameState>().unwrap().pawns.get(index))
        .map(|pawn| format!("selected={} ({})", pawn.name, selected_count))
        .unwrap_or_else(|| format!("selected=none ({})", selected_count));

    let game = world.get_resource_mut::<GameState>().unwrap();
    game.frame_count = next_frame_count;
    game.title = format!(
        "SkyEngine — Rimworld Prototype | tool={} | {} | wood={} steel={} food={} | {} | {} | {} | 1 inspect 2 stockpile 3 harvest 4 wall 5 bed 6 farm 0 cancel",
        tool.label(),
        if paused { "paused" } else { "running" },
        resources.0,
        resources.1,
        resources.2,
        hover,
        selection,
        pawn_summary
    );
}

fn apply_tool(world: &mut World, cell: [i32; 2]) {
    let tool = world.get_resource::<GameState>().unwrap().tool;
    let Some(tile) = world
        .get_resource_mut::<MapState>()
        .unwrap()
        .cell_mut(cell[0], cell[1])
    else {
        return;
    };
    match tool {
        ToolMode::Inspect => {}
        ToolMode::Stockpile => {
            if tile.structure.is_none() {
                tile.stockpile = !tile.stockpile;
            }
        }
        ToolMode::Harvest => {
            if let Some(node) = tile.node.as_mut() {
                node.designated = !node.designated;
            }
        }
        ToolMode::BuildWall => {
            if tile.structure.is_none() && tile.blueprint.is_none() && tile.node.is_none() {
                tile.blueprint = Some(crate::model::Blueprint {
                    kind: StructureKind::Wall,
                    reserved: false,
                });
                tile.stockpile = false;
            }
        }
        ToolMode::BuildBed => {
            if tile.structure.is_none() && tile.blueprint.is_none() && tile.node.is_none() {
                tile.blueprint = Some(crate::model::Blueprint {
                    kind: StructureKind::Bed,
                    reserved: false,
                });
                tile.stockpile = false;
            }
        }
        ToolMode::BuildFarm => {
            if tile.structure.is_none() && tile.node.is_none() && tile.farm.is_none() {
                tile.farm = Some(crate::model::FarmPlot {
                    designated: true,
                    reserved: false,
                    planted: false,
                    growth: 0.0,
                    ready: false,
                });
            } else if let Some(farm) = tile.farm.as_mut() {
                farm.designated = !farm.designated;
            }
        }
        ToolMode::Cancel => {
            if let Some(node) = tile.node.as_mut() {
                node.designated = false;
                node.reserved = false;
            }
            if let Some(farm) = tile.farm.as_mut() {
                farm.designated = false;
                farm.reserved = false;
            }
            tile.item_reserved = false;
            tile.blueprint = None;
            if tile.structure.is_none() {
                tile.stockpile = false;
            }
        }
    }
}

fn select_at_cell(world: &mut World, cell: [i32; 2]) {
    let selected = {
        let game = world.get_resource::<GameState>().unwrap();
        game.pawns
            .iter()
            .enumerate()
            .find_map(|(index, pawn)| (world_to_cell(pawn.pos) == cell).then_some(index))
    };
    let game = world.get_resource_mut::<GameState>().unwrap();
    game.selected_pawn = selected;
    game.selected_pawns = selected.into_iter().collect();
    game.selected_cell = Some(cell);
}

fn select_in_rect(world: &mut World, start: [f32; 2], end: [f32; 2], surface: [f32; 2]) {
    let (min_x, max_x) = if start[0] <= end[0] {
        (start[0], end[0])
    } else {
        (end[0], start[0])
    };
    let (min_y, max_y) = if start[1] <= end[1] {
        (start[1], end[1])
    } else {
        (end[1], start[1])
    };
    let selected = {
        let game = world.get_resource::<GameState>().unwrap();
        game.pawns
            .iter()
            .enumerate()
            .filter_map(|(index, pawn)| {
                let screen = world_to_screen(game, surface, [pawn.pos.x(), pawn.pos.y()]);
                (screen[0] >= min_x
                    && screen[0] <= max_x
                    && screen[1] >= min_y
                    && screen[1] <= max_y)
                    .then_some(index)
            })
            .collect::<Vec<_>>()
    };
    let game = world.get_resource_mut::<GameState>().unwrap();
    game.selected_pawn = selected.first().copied();
    game.selected_pawns = selected;
    game.selected_cell = game
        .selected_pawn
        .map(|index| world_to_cell(game.pawns[index].pos));
}

fn issue_context_order(world: &mut World, selected_pawns: &[usize], target: [i32; 2]) {
    if selected_pawns.is_empty() {
        return;
    }
    let target_cell = world
        .get_resource::<MapState>()
        .unwrap()
        .cell(target[0], target[1])
        .copied();
    if let Some(cell) = target_cell {
        if cell.blueprint.is_some() {
            issue_build_order(world, selected_pawns[0], target);
            return;
        }
        if cell.node.is_some() {
            issue_harvest_order(world, selected_pawns[0], target);
            return;
        }
        if cell.farm.is_some() {
            issue_farm_order(world, selected_pawns[0], target);
            return;
        }
        if cell.item.is_some() && !cell.stockpile {
            issue_haul_order(world, selected_pawns[0], target);
            return;
        }
    }
    for (offset_index, pawn_index) in selected_pawns.iter().copied().enumerate() {
        let offset_target = formation_target(target, offset_index);
        issue_move_order(world, pawn_index, offset_target);
    }
}

fn issue_move_order(world: &mut World, pawn_index: usize, target: [i32; 2]) {
    let start = {
        let game = world.get_resource::<GameState>().unwrap();
        if pawn_index >= game.pawns.len() {
            return;
        }
        world_to_cell(game.pawns[pawn_index].pos)
    };
    let path = find_path(
        world.get_resource::<MapState>().unwrap(),
        start,
        target,
        true,
    );
    if path.is_empty() {
        return;
    }
    let game = world.get_resource_mut::<GameState>().unwrap();
    game.selected_pawn = Some(pawn_index);
    if !game.selected_pawns.contains(&pawn_index) {
        game.selected_pawns.push(pawn_index);
    }
    game.selected_cell = Some(target);
    game.pawns[pawn_index].job = PawnJob::Moving {
        purpose: MovePurpose::PlayerMove(target),
        path,
        step_index: 0,
    };
}

fn issue_harvest_order(world: &mut World, pawn_index: usize, target: [i32; 2]) {
    if let Some(tile) = world
        .get_resource_mut::<MapState>()
        .unwrap()
        .cell_mut(target[0], target[1])
    {
        if let Some(node) = tile.node.as_mut() {
            node.designated = true;
            node.reserved = true;
        }
    }
    assign_single_target_job(world, pawn_index, target, MovePurpose::Harvest(target));
}

fn issue_build_order(world: &mut World, pawn_index: usize, target: [i32; 2]) {
    if let Some(tile) = world
        .get_resource_mut::<MapState>()
        .unwrap()
        .cell_mut(target[0], target[1])
    {
        if let Some(blueprint) = tile.blueprint.as_mut() {
            blueprint.reserved = true;
        }
    }
    assign_single_target_job(world, pawn_index, target, MovePurpose::Build(target));
}

fn issue_farm_order(world: &mut World, pawn_index: usize, target: [i32; 2]) {
    if let Some(tile) = world
        .get_resource_mut::<MapState>()
        .unwrap()
        .cell_mut(target[0], target[1])
    {
        if let Some(farm) = tile.farm.as_mut() {
            farm.designated = true;
            farm.reserved = true;
        }
    }
    let purpose = {
        let farm = world
            .get_resource::<MapState>()
            .unwrap()
            .cell(target[0], target[1])
            .and_then(|tile| tile.farm);
        match farm {
            Some(farm) if farm.ready => MovePurpose::Harvest(target),
            _ => MovePurpose::Sow(target),
        }
    };
    assign_single_target_job(world, pawn_index, target, purpose);
}

fn issue_haul_order(world: &mut World, pawn_index: usize, pickup: [i32; 2]) {
    let Some(dropoff) = find_best_stockpile(world, pickup) else {
        return;
    };
    if let Some(tile) = world
        .get_resource_mut::<MapState>()
        .unwrap()
        .cell_mut(pickup[0], pickup[1])
    {
        tile.item_reserved = true;
    }
    assign_single_target_job(
        world,
        pawn_index,
        pickup,
        MovePurpose::HaulPickup(pickup, dropoff),
    );
}

fn assign_single_target_job(
    world: &mut World,
    pawn_index: usize,
    target: [i32; 2],
    purpose: MovePurpose,
) {
    let start = world_to_cell(world.get_resource::<GameState>().unwrap().pawns[pawn_index].pos);
    let path = find_path(
        world.get_resource::<MapState>().unwrap(),
        start,
        target,
        true,
    );
    if path.is_empty() {
        return;
    }
    let game = world.get_resource_mut::<GameState>().unwrap();
    game.selected_pawn = Some(pawn_index);
    game.selected_pawns = vec![pawn_index];
    game.selected_cell = Some(target);
    game.pawns[pawn_index].job = PawnJob::Moving {
        purpose,
        path,
        step_index: 0,
    };
}

fn formation_target(center: [i32; 2], offset_index: usize) -> [i32; 2] {
    let offsets = [
        [0, 0],
        [1, 0],
        [-1, 0],
        [0, 1],
        [0, -1],
        [1, 1],
        [-1, 1],
        [1, -1],
        [-1, -1],
    ];
    let offset = offsets[offset_index.min(offsets.len() - 1)];
    [center[0] + offset[0], center[1] + offset[1]]
}

fn world_to_screen(game: &GameState, surface: [f32; 2], world_pos: [f32; 2]) -> [f32; 2] {
    let width = surface[0].max(1.0);
    let height = surface[1].max(1.0);
    [
        (world_pos[0] - game.camera_center.x()) * game.zoom + width * 0.5,
        (game.camera_center.y() - world_pos[1]) * game.zoom + height * 0.5,
    ]
}

fn describe_cell(map: &MapState, cell: [i32; 2]) -> String {
    let Some(tile) = map.cell(cell[0], cell[1]) else {
        return "outside".to_string();
    };
    let mut parts = vec![format!("tile {},{}", cell[0], cell[1])];
    parts.push(match tile.terrain {
        crate::model::TerrainKind::Grass => "grass".to_string(),
        crate::model::TerrainKind::RichSoil => "rich soil".to_string(),
        crate::model::TerrainKind::Gravel => "gravel".to_string(),
    });
    if tile.stockpile {
        parts.push("stockpile".to_string());
    }
    if let Some(farm) = tile.farm {
        parts.push(if farm.ready {
            "farm ready".to_string()
        } else if farm.planted {
            format!("farm growing {:.0}%", farm.growth * 100.0)
        } else {
            "farm empty".to_string()
        });
    }
    if let Some(structure) = tile.structure {
        parts.push(structure.label().to_string());
    }
    if let Some(blueprint) = tile.blueprint {
        parts.push(format!("{} blueprint", blueprint.kind.label()));
    }
    if let Some(node) = tile.node {
        let mut label = node.kind.label().to_string();
        if node.designated {
            label.push_str(" designated");
        }
        parts.push(label);
    }
    if let Some(item) = tile.item {
        parts.push(format!("{} {}", item.amount, item.kind.label()));
    }
    parts.join(" | ")
}

fn clamp_camera(game: &mut GameState) {
    let world_w = GRID_WIDTH as f32 * TILE_SIZE;
    let world_h = GRID_HEIGHT as f32 * TILE_SIZE;
    let padding = TILE_SIZE * 2.0 / game.zoom;
    game.camera_center[0] = game.camera_center[0].clamp(-padding, world_w + padding);
    game.camera_center[1] = game.camera_center[1].clamp(-padding, world_h + padding);
}

fn screen_to_cell(game: &GameState, surface_size: [f32; 2], mouse: [f32; 2]) -> Option<[i32; 2]> {
    let width = surface_size[0].max(1.0);
    let height = surface_size[1].max(1.0);
    let world = Vec2::new(
        game.camera_center.x() + (mouse[0] / width - 0.5) * width / game.zoom,
        game.camera_center.y() + (0.5 - mouse[1] / height) * height / game.zoom,
    );
    let cell = world_to_cell(world);
    in_bounds(cell[0], cell[1]).then_some(cell)
}
