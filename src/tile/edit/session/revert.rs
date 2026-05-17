use super::super::summary::{ObjectChange, TileMapEditSummary};
use super::TileMapEditSession;

impl TileMapEditSession<'_> {
    pub fn revert_summary(&mut self, summary: &TileMapEditSummary) {
        for change in summary.object_changes.iter().rev() {
            match change {
                ObjectChange::Created { object } => self.remove_object(object.id),
                ObjectChange::Removed { object } => {
                    self.place_object(object.layer, object.clone());
                }
                ObjectChange::Moved {
                    id,
                    from_layer,
                    from_cell,
                    ..
                } => self.move_object(*id, *from_layer, *from_cell),
                ObjectChange::VisualChanged { id, old, .. } => {
                    self.set_object_visual(*id, old.clone());
                }
            }
        }
        for change in summary.property_changes.iter().rev() {
            match &change.old {
                Some(value) => self.set_property(change.target.clone(), &change.key, value.clone()),
                None => self.remove_property(change.target.clone(), &change.key),
            }
        }
        for change in summary.tile_changes.iter().rev() {
            self.set_tile(change.layer, change.cell, change.old);
        }
    }
}
