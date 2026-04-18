use crate::render::phase::{
    opaque_sort_key, transparent_sort_key, DrawFunctionId, MeshDrawData, OpaquePhase, PhaseItem,
    TransparentPhase,
};
use crate::render::view::{Projection, SceneView};
use crate::render::{OrderInLayer, SortingLayer, SpriteMaterial, Transform, ViewportRect};

fn make_view() -> SceneView {
    let projection = Projection::orthographic(64.0, 64.0);
    SceneView::new(
        0,
        ViewportRect::from_surface_size([64, 64]),
        [64, 64],
        false,
        u32::MAX,
        Transform::default(),
        projection,
        projection.view_uniform(Transform::default(), [64, 64]),
        true,
    )
}

#[test]
fn transparent_phase_sorts_by_sort_key_then_batch_then_entity() {
    let mesh = crate::render::expert::Mesh::QUAD;
    let fake_material = crate::render::MaterialHandle::new::<SpriteMaterial>(0, 0);
    let view = make_view();

    let near = Transform::from_xyz(0.0, 0.0, 5.0);
    let far = Transform::from_xyz(0.0, 0.0, 1.0);

    let entity_a = crate::ecs::EntityId::new(2, 0);
    let entity_b = crate::ecs::EntityId::new(1, 0);

    let mut phase = TransparentPhase::new();
    phase.add_item(PhaseItem::new(
        transparent_sort_key(SortingLayer(0), OrderInLayer(0), 2, near, &view),
        DrawFunctionId::from_raw(0),
        entity_a,
        2,
        MeshDrawData::new(mesh, fake_material, 0),
    ));
    phase.add_item(PhaseItem::new(
        transparent_sort_key(SortingLayer(0), OrderInLayer(0), 1, far, &view),
        DrawFunctionId::from_raw(0),
        entity_b,
        1,
        MeshDrawData::new(mesh, fake_material, 0),
    ));

    phase.sort();

    let items = phase.items();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].batch_key, 1);
    assert_eq!(items[1].batch_key, 2);
}

#[test]
fn opaque_phase_sorts_front_to_back() {
    let mesh = crate::render::expert::Mesh::QUAD;
    let fake_material = crate::render::MaterialHandle::new::<SpriteMaterial>(0, 0);
    let view = make_view();

    let near = Transform::from_xyz(0.0, 0.0, 1.0);
    let far = Transform::from_xyz(0.0, 0.0, 5.0);

    let mut phase = OpaquePhase::new();
    phase.add_item(PhaseItem::new(
        opaque_sort_key(0, crate::ecs::EntityId::new(2, 0), far, &view),
        DrawFunctionId::from_raw(0),
        crate::ecs::EntityId::new(2, 0),
        0,
        MeshDrawData::new(mesh, fake_material, 0),
    ));
    phase.add_item(PhaseItem::new(
        opaque_sort_key(0, crate::ecs::EntityId::new(1, 0), near, &view),
        DrawFunctionId::from_raw(0),
        crate::ecs::EntityId::new(1, 0),
        0,
        MeshDrawData::new(mesh, fake_material, 0),
    ));

    phase.sort();

    let items = phase.items();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].entity.index(), 1);
    assert_eq!(items[1].entity.index(), 2);
}
