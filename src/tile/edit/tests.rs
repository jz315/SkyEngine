use super::*;
use crate::tile::{
    CellCoord, GridSpec, LayerId, LayerRole, ObjectVisual, PaletteId, PropertyValue, SceneTile,
    TileDefId, TileLayer, TileMap, TileMapId, TileMapSize, TileObject, TileObjectId, TileRef,
};

#[test]
fn edit_session_removes_objects_from_layer_membership() {
    let layer = LayerId(3);
    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(4, 4),
    );
    scene
        .layers
        .push(TileLayer::objects(layer, "Props", LayerRole::Props));

    let object_id = {
        let object = TileObject::new(TileObjectId(0), layer, CellCoord::new(1, 2));
        let mut session = TileMapEditSession::new(&mut scene);
        let object_id = session.place_object(layer, object);
        let summary = session.finish();
        let object = scene
            .objects
            .get(object_id)
            .expect("created object")
            .clone();
        assert_eq!(
            summary.object_changes,
            vec![ObjectChange::Created { object }]
        );
        object_id
    };

    assert!(scene.objects.get(object_id).is_some());
    assert_eq!(
        scene.layers[0]
            .data
            .as_objects()
            .expect("object layer")
            .objects,
        vec![object_id]
    );

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.remove_object(object_id);
        session.finish()
    };

    assert!(scene.objects.get(object_id).is_none());
    assert!(scene.layers[0]
        .data
        .as_objects()
        .expect("object layer")
        .objects
        .is_empty());
    assert_eq!(
        summary.object_changes,
        vec![ObjectChange::Removed {
            object: TileObject::new(object_id, layer, CellCoord::new(1, 2)),
        }]
    );
    assert_eq!(summary.changed_layers, vec![layer]);
}

#[test]
fn edit_session_updates_object_properties_in_place() {
    let layer = LayerId(4);
    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(4, 4),
    );
    scene
        .layers
        .push(TileLayer::objects(layer, "Props", LayerRole::Props));
    let object = TileObject::new(TileObjectId(0), layer, CellCoord::new(0, 0));
    let object_id = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.place_object(layer, object)
    };

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.set_property(PropertyTarget::Object(object_id), "kind", "lamp");
        session.finish()
    };

    assert!(matches!(
        scene
            .objects
            .get(object_id)
            .and_then(|object| object.properties.get("kind")),
        Some(PropertyValue::String(value)) if value == "lamp"
    ));
    assert_eq!(summary.changed_layers, vec![layer]);
    assert_eq!(
        summary.property_changes,
        vec![PropertyChange {
            target: PropertyTarget::Object(object_id),
            key: "kind".to_string(),
            old: None,
            new: Some(PropertyValue::String("lamp".to_string())),
        }]
    );
}

#[test]
fn edit_session_removes_properties_with_old_values() {
    let layer = LayerId(4);
    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(4, 4),
    );
    let mut tile_layer = TileLayer::tiles(layer, "Ground", LayerRole::Ground);
    tile_layer.properties.insert("kind", "floor");
    scene.layers.push(tile_layer);

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.remove_property(PropertyTarget::Layer(layer), "kind");
        session.finish()
    };

    assert!(scene.layers[0].properties.get("kind").is_none());
    assert_eq!(
        summary.property_changes,
        vec![PropertyChange {
            target: PropertyTarget::Layer(layer),
            key: "kind".to_string(),
            old: Some(PropertyValue::String("floor".to_string())),
            new: None,
        }]
    );
    assert_eq!(summary.changed_layers, vec![layer]);
}

#[test]
fn edit_session_records_tile_old_and_new_values() {
    let layer = LayerId(4);
    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(4, 4),
    );
    let first = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(1)));
    let second = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(2)));
    let mut tile_layer = TileLayer::tiles(layer, "Ground", LayerRole::Ground);
    tile_layer.set_tile(CellCoord::new(1, 1), Some(first));
    scene.layers.push(tile_layer);

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.set_tile(layer, CellCoord::new(1, 1), Some(second));
        session.finish()
    };

    assert_eq!(
        summary.tile_changes,
        vec![TileChange {
            layer,
            cell: CellCoord::new(1, 1),
            old: Some(first),
            new: Some(second),
        }]
    );
    assert_eq!(
        summary.dirty_cells,
        vec![DirtyCell {
            layer,
            cell: CellCoord::new(1, 1)
        }]
    );
}

#[test]
fn edit_session_ignores_noop_tile_and_property_sets() {
    let layer = LayerId(4);
    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(4, 4),
    );
    let tile = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(1)));
    let mut tile_layer = TileLayer::tiles(layer, "Ground", LayerRole::Ground);
    tile_layer.set_tile(CellCoord::new(1, 1), Some(tile));
    tile_layer.properties.insert("kind", "floor");
    scene.layers.push(tile_layer);

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.set_tile(layer, CellCoord::new(1, 1), Some(tile));
        session.set_property(PropertyTarget::Layer(layer), "kind", "floor");
        session.finish()
    };

    assert!(summary.tile_changes.is_empty());
    assert!(summary.dirty_cells.is_empty());
    assert!(summary.changed_layers.is_empty());
    assert!(summary.property_changes.is_empty());
}

#[test]
fn edit_session_moves_objects_between_layers() {
    let first = LayerId(4);
    let second = LayerId(5);
    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(4, 4),
    );
    scene
        .layers
        .push(TileLayer::objects(first, "Props", LayerRole::Props));
    scene
        .layers
        .push(TileLayer::objects(second, "Upper", LayerRole::Upper));
    let object = TileObject::new(TileObjectId(0), first, CellCoord::new(0, 0));
    let object_id = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.place_object(first, object)
    };

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.move_object(object_id, second, CellCoord::new(2, 3));
        session.finish()
    };

    let object = scene.objects.get(object_id).expect("moved object");
    assert_eq!(object.layer, second);
    assert_eq!(object.cell, CellCoord::new(2, 3));
    assert!(scene.layers[0]
        .data
        .as_objects()
        .expect("first layer")
        .objects
        .is_empty());
    assert_eq!(
        scene.layers[1]
            .data
            .as_objects()
            .expect("second layer")
            .objects,
        vec![object_id]
    );
    assert_eq!(
        summary.object_changes,
        vec![ObjectChange::Moved {
            id: object_id,
            from_layer: first,
            to_layer: second,
            from_cell: CellCoord::new(0, 0),
            to_cell: CellCoord::new(2, 3),
        }]
    );
    assert_eq!(summary.changed_layers, vec![first, second]);
}

#[test]
fn edit_session_records_object_visual_changes() {
    let layer = LayerId(4);
    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(4, 4),
    );
    scene
        .layers
        .push(TileLayer::objects(layer, "Props", LayerRole::Props));
    let object = TileObject::new(TileObjectId(0), layer, CellCoord::new(0, 0));
    let object_id = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.place_object(layer, object)
    };
    let visual = ObjectVisual::Tile(TileRef::new(PaletteId(1), TileDefId(2)));

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.set_object_visual(object_id, visual.clone());
        session.finish()
    };

    assert_eq!(scene.objects.get(object_id).expect("object").visual, visual);
    assert_eq!(
        summary.object_changes,
        vec![ObjectChange::VisualChanged {
            id: object_id,
            old: ObjectVisual::None,
            new: visual,
        }]
    );
    assert_eq!(summary.changed_layers, vec![layer]);
}

#[test]
fn edit_session_reverts_summary_and_returns_redo_summary() {
    let tile_layer_id = LayerId(4);
    let object_layer_id = LayerId(5);
    let first_tile = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(1)));
    let second_tile = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(2)));
    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(4, 4),
    );
    let mut tile_layer = TileLayer::tiles(tile_layer_id, "Ground", LayerRole::Ground);
    tile_layer.set_tile(CellCoord::new(0, 0), Some(first_tile));
    scene.layers.push(tile_layer);
    scene.layers.push(TileLayer::objects(
        object_layer_id,
        "Props",
        LayerRole::Props,
    ));
    scene.properties.insert("weather", "sunny");
    let object_id = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.place_object(
            object_layer_id,
            TileObject::new(TileObjectId(0), object_layer_id, CellCoord::new(0, 0)),
        )
    };

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.set_tile(tile_layer_id, CellCoord::new(0, 0), Some(second_tile));
        session.set_property(PropertyTarget::Scene, "weather", "rain");
        session.move_object(object_id, object_layer_id, CellCoord::new(2, 0));
        session.finish()
    };

    let undo_summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.revert_summary(&summary);
        session.finish()
    };

    assert_eq!(scene.layers[0].tile(CellCoord::new(0, 0)), Some(first_tile));
    assert!(matches!(
        scene.properties.get("weather"),
        Some(PropertyValue::String(value)) if value == "sunny"
    ));
    assert_eq!(
        scene.objects.get(object_id).expect("object").cell,
        CellCoord::new(0, 0)
    );
    assert_eq!(
        undo_summary.tile_changes,
        vec![TileChange {
            layer: tile_layer_id,
            cell: CellCoord::new(0, 0),
            old: Some(second_tile),
            new: Some(first_tile),
        }]
    );
    assert_eq!(
        undo_summary.property_changes,
        vec![PropertyChange {
            target: PropertyTarget::Scene,
            key: "weather".to_string(),
            old: Some(PropertyValue::String("rain".to_string())),
            new: Some(PropertyValue::String("sunny".to_string())),
        }]
    );
    assert_eq!(
        undo_summary.object_changes,
        vec![ObjectChange::Moved {
            id: object_id,
            from_layer: object_layer_id,
            to_layer: object_layer_id,
            from_cell: CellCoord::new(2, 0),
            to_cell: CellCoord::new(0, 0),
        }]
    );
}

#[test]
fn edit_history_undoes_and_redoes_summaries() {
    let layer = LayerId(4);
    let first_tile = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(1)));
    let second_tile = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(2)));
    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(4, 4),
    );
    let mut tile_layer = TileLayer::tiles(layer, "Ground", LayerRole::Ground);
    tile_layer.set_tile(CellCoord::new(0, 0), Some(first_tile));
    scene.layers.push(tile_layer);

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.set_tile(layer, CellCoord::new(0, 0), Some(second_tile));
        session.finish()
    };
    let mut history = TileMapEditHistory::new();

    assert!(history.record(summary));
    assert!(history.can_undo());
    assert!(!history.can_redo());

    let undo_summary = history.undo(&mut scene).expect("undo summary");
    assert_eq!(scene.layers[0].tile(CellCoord::new(0, 0)), Some(first_tile));
    assert!(!undo_summary.is_empty());
    assert!(!history.can_undo());
    assert!(history.can_redo());

    let redo_summary = history.redo(&mut scene).expect("redo summary");
    assert_eq!(
        scene.layers[0].tile(CellCoord::new(0, 0)),
        Some(second_tile)
    );
    assert!(!redo_summary.is_empty());
    assert!(history.can_undo());
    assert!(!history.can_redo());
}

#[test]
fn edit_history_ignores_empty_summaries_and_clears_redo_on_record() {
    let layer = LayerId(4);
    let first_tile = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(1)));
    let second_tile = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(2)));
    let third_tile = SceneTile::new(TileRef::new(PaletteId(1), TileDefId(3)));
    let mut scene = TileMap::new(
        TileMapId(1),
        "edit",
        GridSpec::orthogonal([16, 16]),
        TileMapSize::new(4, 4),
    );
    let mut tile_layer = TileLayer::tiles(layer, "Ground", LayerRole::Ground);
    tile_layer.set_tile(CellCoord::new(0, 0), Some(first_tile));
    scene.layers.push(tile_layer);

    let mut history = TileMapEditHistory::new();
    assert!(!history.record(TileMapEditSummary::default()));

    let summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.set_tile(layer, CellCoord::new(0, 0), Some(second_tile));
        session.finish()
    };
    assert!(history.record(summary));
    assert!(history.undo(&mut scene).is_some());
    assert!(history.can_redo());

    let next_summary = {
        let mut session = TileMapEditSession::new(&mut scene);
        session.set_tile(layer, CellCoord::new(0, 0), Some(third_tile));
        session.finish()
    };
    assert!(history.record(next_summary));
    assert!(history.can_undo());
    assert!(!history.can_redo());
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.redo_len(), 0);
}
