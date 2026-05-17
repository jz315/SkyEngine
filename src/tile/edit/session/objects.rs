use super::super::super::grid::CellCoord;
use super::super::super::layer::LayerId;
use super::super::super::object::{ObjectVisual, TileObject, TileObjectId};
use super::super::summary::ObjectChange;
use super::TileMapEditSession;

impl TileMapEditSession<'_> {
    pub fn place_object(&mut self, layer: LayerId, mut object: TileObject) -> TileObjectId {
        object.layer = layer;
        let id = self.scene.objects.insert(object);
        if let Some(layer_ref) = self.scene.layer_mut(layer) {
            layer_ref.add_object_id(id);
        }
        let object = self
            .scene
            .objects
            .get(id)
            .expect("inserted tile object should be readable")
            .clone();
        self.summary
            .object_changes
            .push(ObjectChange::Created { object });
        self.summary.mark_layer(layer);
        id
    }

    pub fn remove_object(&mut self, id: TileObjectId) {
        if let Some(object) = self.scene.objects.remove(id) {
            if let Some(layer_ref) = self.scene.layer_mut(object.layer) {
                layer_ref.remove_object_id(id);
            }
            self.summary.object_changes.push(ObjectChange::Removed {
                object: object.clone(),
            });
            self.summary.mark_layer(object.layer);
        }
    }

    pub fn move_object(&mut self, id: TileObjectId, layer: LayerId, cell: CellCoord) {
        let Some(mut object) = self.scene.objects.get(id).cloned() else {
            return;
        };
        let from_layer = object.layer;
        let from_cell = object.cell;
        if from_layer == layer && from_cell == cell {
            return;
        }

        if from_layer != layer {
            if let Some(layer_ref) = self.scene.layer_mut(from_layer) {
                layer_ref.remove_object_id(id);
            }
            if let Some(layer_ref) = self.scene.layer_mut(layer) {
                layer_ref.add_object_id(id);
            }
        }

        object.layer = layer;
        object.cell = cell;
        let _ = self.scene.objects.insert(object);
        self.summary.object_changes.push(ObjectChange::Moved {
            id,
            from_layer,
            to_layer: layer,
            from_cell,
            to_cell: cell,
        });
        self.summary.mark_layer(from_layer);
        self.summary.mark_layer(layer);
    }

    pub fn set_object_visual(&mut self, id: TileObjectId, visual: ObjectVisual) {
        let Some(object) = self.scene.objects.get_mut(id) else {
            return;
        };
        if object.visual == visual {
            return;
        }
        let old = object.visual.clone();
        object.visual = visual.clone();
        let layer = object.layer;
        self.summary
            .object_changes
            .push(ObjectChange::VisualChanged {
                id,
                old,
                new: visual,
            });
        self.summary.mark_layer(layer);
    }
}
