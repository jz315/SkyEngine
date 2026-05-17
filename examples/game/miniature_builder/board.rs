use sky_engine::ecs::World;
use sky_engine::input::InputActions;

use crate::actions::{
    ACTION_BOARD_PLACE, ACTION_BOARD_REMOVE, ACTION_BOARD_RESET, ACTION_BOARD_UNDO,
};
use crate::geometry::{cell_index, neighbors, COLS, ROWS};
use crate::hud::HudState;
use crate::model::{make_cells, Cell, Counts, Family, PlacedStructure, BLUEPRINTS};
use crate::selection::{BuildSelection, HoverState};

pub const START_BUDGET: i32 = 120;
pub const TARGET_SCORE: i32 = 86;

pub struct BoardState {
    pub cells: Vec<Cell>,
    pub budget: i32,
    pub score: i32,
    pub counts: Counts,
    pub placed_order: Vec<(usize, usize)>,
    pub victory: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardIntent {
    Place {
        row: usize,
        col: usize,
        blueprint: usize,
        orientation: usize,
    },
    Remove {
        row: usize,
        col: usize,
    },
    Undo,
    Reset,
}

#[derive(Default, Debug)]
pub struct BoardIntentQueue(pub Vec<BoardIntent>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardDelta {
    SetStructure {
        row: usize,
        col: usize,
        structure: Option<PlacedStructure>,
    },
}

#[derive(Default, Debug)]
pub struct BoardDeltaQueue(pub Vec<BoardDelta>);

pub fn collect_board_intents(world: &mut World) {
    let Some(actions) = world.get_resource::<InputActions>() else {
        return;
    };
    let hover = world
        .get_resource::<HoverState>()
        .map(|hover| hover.cell)
        .unwrap_or(None);
    let selection = world
        .get_resource::<BuildSelection>()
        .copied()
        .unwrap_or_default();
    let place = actions.action_pressed(ACTION_BOARD_PLACE);
    let remove = actions.action_pressed(ACTION_BOARD_REMOVE);
    let undo = actions.action_pressed(ACTION_BOARD_UNDO);
    let reset = actions.action_pressed(ACTION_BOARD_RESET);

    let Some(queue) = world.get_resource_mut::<BoardIntentQueue>() else {
        return;
    };
    if reset {
        queue.0.push(BoardIntent::Reset);
    }
    if undo {
        queue.0.push(BoardIntent::Undo);
    }
    if let Some((row, col)) = hover {
        if place {
            queue.0.push(BoardIntent::Place {
                row,
                col,
                blueprint: selection.selected,
                orientation: selection.orientation,
            });
        }
        if remove {
            queue.0.push(BoardIntent::Remove { row, col });
        }
    }
}

pub fn apply_board_intents(world: &mut World) {
    let Some(mut intents) = world.remove_resource::<BoardIntentQueue>() else {
        return;
    };
    if intents.0.is_empty() {
        world.insert_resource(intents);
        return;
    }

    let Some(mut board) = world.remove_resource::<BoardState>() else {
        world.insert_resource(intents);
        return;
    };
    let Some(mut deltas) = world.remove_resource::<BoardDeltaQueue>() else {
        world.insert_resource(board);
        world.insert_resource(intents);
        return;
    };

    let mut last_message = None;
    for intent in intents.0.drain(..) {
        let outcome = match intent {
            BoardIntent::Place {
                row,
                col,
                blueprint,
                orientation,
            } => board.place_at(blueprint, orientation, row, col),
            BoardIntent::Remove { row, col } => board.remove_at(row, col, true),
            BoardIntent::Undo => board.undo_last(),
            BoardIntent::Reset => board.reset(),
        };
        deltas.0.extend(outcome.deltas);
        last_message = Some(outcome.message);
    }

    if let Some(hud) = world.get_resource_mut::<HudState>() {
        if board.victory {
            hud.message = "commission complete - keep building or press R".to_string();
        } else if let Some(message) = last_message {
            hud.message = message;
        }
    }

    world.insert_resource(deltas);
    world.insert_resource(board);
    world.insert_resource(intents);
}

impl BoardState {
    pub fn new() -> Self {
        Self {
            cells: make_cells(),
            budget: START_BUDGET,
            score: 0,
            counts: Counts::default(),
            placed_order: Vec::new(),
            victory: false,
        }
    }

    pub fn seed_starter_layout(&mut self) {
        for (blueprint, orientation, row, col) in [(0, 0, 2, 1), (3, 1, 8, 5), (6, 3, 2, 8)] {
            let _ = self.place_at(blueprint, orientation, row, col);
        }
    }

    pub fn can_afford(&self, selected: usize, row: usize, col: usize) -> bool {
        let new_cost = BLUEPRINTS[selected].cost;
        let old_cost = self.cells[cell_index(row, col)]
            .structure
            .map(|structure| BLUEPRINTS[structure.blueprint].cost)
            .unwrap_or(0);
        self.budget + old_cost >= new_cost
    }

    pub fn reset(&mut self) -> BoardOutcome {
        let mut deltas = Vec::new();
        for row in 0..ROWS {
            for col in 0..COLS {
                let index = cell_index(row, col);
                if self.cells[index].structure.take().is_some() {
                    deltas.push(BoardDelta::SetStructure {
                        row,
                        col,
                        structure: None,
                    });
                }
            }
        }
        self.budget = START_BUDGET;
        self.score = 0;
        self.counts = Counts::default();
        self.placed_order.clear();
        self.victory = false;
        BoardOutcome {
            message: "new commission started".to_string(),
            deltas,
        }
    }

    pub fn place_at(
        &mut self,
        selected: usize,
        orientation: usize,
        row: usize,
        col: usize,
    ) -> BoardOutcome {
        let blueprint = BLUEPRINTS[selected];
        let idx = cell_index(row, col);
        let old_cost = self.cells[idx]
            .structure
            .map(|structure| BLUEPRINTS[structure.blueprint].cost)
            .unwrap_or(0);
        if self.budget + old_cost < blueprint.cost {
            return BoardOutcome {
                message: format!(
                    "need {} more coins",
                    blueprint.cost - self.budget - old_cost
                ),
                deltas: Vec::new(),
            };
        }

        if self.cells[idx].structure.is_some() {
            self.budget += old_cost;
        }
        self.cells[idx].structure = Some(PlacedStructure {
            blueprint: selected,
            orientation,
        });
        self.budget -= blueprint.cost;
        self.placed_order.push((row, col));
        self.recompute_score();
        BoardOutcome {
            message: format!("placed {}", blueprint.name),
            deltas: vec![BoardDelta::SetStructure {
                row,
                col,
                structure: self.cells[idx].structure,
            }],
        }
    }

    pub fn remove_at(&mut self, row: usize, col: usize, refund: bool) -> BoardOutcome {
        let idx = cell_index(row, col);
        let Some(structure) = self.cells[idx].structure.take() else {
            return BoardOutcome {
                message: "nothing to remove".to_string(),
                deltas: Vec::new(),
            };
        };
        if refund {
            self.budget += BLUEPRINTS[structure.blueprint].cost;
        }
        self.recompute_score();
        BoardOutcome {
            message: "removed piece".to_string(),
            deltas: vec![BoardDelta::SetStructure {
                row,
                col,
                structure: None,
            }],
        }
    }

    pub fn undo_last(&mut self) -> BoardOutcome {
        while let Some((row, col)) = self.placed_order.pop() {
            if self.cells[cell_index(row, col)].structure.is_some() {
                let mut outcome = self.remove_at(row, col, true);
                outcome.message = "undid last placement".to_string();
                return outcome;
            }
        }
        BoardOutcome {
            message: "nothing to undo".to_string(),
            deltas: Vec::new(),
        }
    }

    fn recompute_score(&mut self) {
        let mut score = 0;
        let mut counts = Counts::default();
        for row in 0..ROWS {
            for col in 0..COLS {
                let idx = cell_index(row, col);
                let Some(structure) = self.cells[idx].structure else {
                    continue;
                };
                let def = BLUEPRINTS[structure.blueprint];
                score += def.appeal;
                if def.family.preferred_zone() == self.cells[idx].zone {
                    score += 4;
                } else if self.cells[idx].zone == crate::model::Zone::Commons {
                    score += 1;
                }

                for (nr, nc) in neighbors(row, col) {
                    if let Some(other) = self.cells[cell_index(nr, nc)].structure {
                        let other_family = BLUEPRINTS[other.blueprint].family;
                        if other_family == def.family {
                            score += 1;
                        }
                    }
                }

                match def.family {
                    Family::Farm => counts.farm += 1,
                    Family::Dungeon => counts.dungeon += 1,
                    Family::Library => counts.library += 1,
                }
            }
        }

        self.score = score;
        self.counts = counts;
        self.victory = self.score >= TARGET_SCORE
            && self.counts.farm >= 3
            && self.counts.dungeon >= 3
            && self.counts.library >= 3;
    }
}

#[derive(Debug)]
pub struct BoardOutcome {
    pub message: String,
    pub deltas: Vec<BoardDelta>,
}

#[cfg(test)]
mod tests {
    use super::{BoardState, START_BUDGET, TARGET_SCORE};

    #[test]
    fn place_success_and_budget_changes() {
        let mut board = BoardState::new();
        let outcome = board.place_at(0, 0, 2, 2);
        assert_eq!(outcome.message, "placed Corn rows");
        assert_eq!(
            board.budget,
            START_BUDGET - crate::model::BLUEPRINTS[0].cost
        );
        assert!(board.cells[crate::geometry::cell_index(2, 2)]
            .structure
            .is_some());
        assert_eq!(outcome.deltas.len(), 1);
    }

    #[test]
    fn place_rejects_when_budget_is_too_low() {
        let mut board = BoardState::new();
        board.budget = 0;
        let outcome = board.place_at(7, 0, 1, 1);
        assert!(outcome.message.starts_with("need "));
        assert!(board.cells[crate::geometry::cell_index(1, 1)]
            .structure
            .is_none());
        assert!(outcome.deltas.is_empty());
    }

    #[test]
    fn place_replaces_existing_structure() {
        let mut board = BoardState::new();
        let _ = board.place_at(0, 0, 2, 2);
        let budget_after_first = board.budget;
        let _ = board.place_at(3, 1, 2, 2);
        let current = board.cells[crate::geometry::cell_index(2, 2)]
            .structure
            .expect("replaced structure");
        assert_eq!(current.blueprint, 3);
        assert_ne!(board.budget, budget_after_first);
    }

    #[test]
    fn remove_clears_structure_and_refunds() {
        let mut board = BoardState::new();
        let _ = board.place_at(0, 0, 2, 2);
        let spent_budget = board.budget;
        let outcome = board.remove_at(2, 2, true);
        assert_eq!(outcome.message, "removed piece");
        assert!(board.cells[crate::geometry::cell_index(2, 2)]
            .structure
            .is_none());
        assert!(board.budget > spent_budget);
    }

    #[test]
    fn undo_removes_latest_live_placement() {
        let mut board = BoardState::new();
        let _ = board.place_at(0, 0, 2, 2);
        let _ = board.place_at(3, 1, 4, 4);
        let outcome = board.undo_last();
        assert_eq!(outcome.message, "undid last placement");
        assert!(board.cells[crate::geometry::cell_index(4, 4)]
            .structure
            .is_none());
        assert!(board.cells[crate::geometry::cell_index(2, 2)]
            .structure
            .is_some());
    }

    #[test]
    fn reset_clears_board_and_budget() {
        let mut board = BoardState::new();
        let _ = board.place_at(0, 0, 2, 2);
        let _ = board.place_at(3, 1, 4, 4);
        let outcome = board.reset();
        assert_eq!(board.budget, START_BUDGET);
        assert_eq!(board.score, 0);
        assert!(!board.victory);
        assert!(board.cells.iter().all(|cell| cell.structure.is_none()));
        assert_eq!(outcome.deltas.len(), 2);
    }

    #[test]
    fn score_and_victory_recompute() {
        let mut board = BoardState::new();
        for (blueprint, row, col) in [
            (0, 0, 0),
            (1, 0, 1),
            (2, 0, 2),
            (3, 8, 8),
            (4, 8, 7),
            (5, 8, 6),
            (6, 1, 9),
            (7, 2, 9),
            (8, 3, 9),
        ] {
            let _ = board.place_at(blueprint, 0, row, col);
        }
        assert!(board.score >= TARGET_SCORE);
        assert_eq!(board.counts.farm, 3);
        assert_eq!(board.counts.dungeon, 3);
        assert_eq!(board.counts.library, 3);
        assert!(board.victory);
    }
}
