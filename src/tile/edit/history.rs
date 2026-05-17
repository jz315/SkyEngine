use super::super::scene::TileMap;
use super::session::TileMapEditSession;
use super::summary::TileMapEditSummary;

/// Optional undo/redo stack for edit summaries.
#[derive(Clone, Debug, Default)]
pub struct TileMapEditHistory {
    undo_stack: Vec<TileMapEditSummary>,
    redo_stack: Vec<TileMapEditSummary>,
}

impl TileMapEditHistory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, summary: TileMapEditSummary) -> bool {
        if summary.is_empty() {
            return false;
        }
        self.undo_stack.push(summary);
        self.redo_stack.clear();
        true
    }

    pub fn undo(&mut self, scene: &mut TileMap) -> Option<TileMapEditSummary> {
        let summary = self.undo_stack.pop()?;
        let mut session = TileMapEditSession::new(scene);
        session.revert_summary(&summary);
        let inverse = session.finish();
        if !inverse.is_empty() {
            self.redo_stack.push(inverse.clone());
        }
        Some(inverse)
    }

    pub fn redo(&mut self, scene: &mut TileMap) -> Option<TileMapEditSummary> {
        let summary = self.redo_stack.pop()?;
        let mut session = TileMapEditSession::new(scene);
        session.revert_summary(&summary);
        let inverse = session.finish();
        if !inverse.is_empty() {
            self.undo_stack.push(inverse.clone());
        }
        Some(inverse)
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }
}
