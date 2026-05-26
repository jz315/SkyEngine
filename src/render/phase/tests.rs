use crate::render::phase::{
    opaque_sort_key, transparent_sort_key, DrawError, DrawFunctionId, DrawFunctionRegistry,
    DrawSprite, MeshDrawData, OpaquePhase, PhaseItem, SpriteDrawData, TransparentPhase,
};
use crate::render::view::{Projection, ProjectionViewUniformExt, SceneView};
use crate::render::{SortingLayer, SpriteMaterial, Transform, ViewportRect};

fn make_view() -> SceneView {
    let projection = Projection::orthographic_fixed(64.0, 64.0);
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
        transparent_sort_key(SortingLayer(0), 2, near, &view),
        DrawFunctionId::from_raw(0),
        entity_a,
        2,
        MeshDrawData::new(mesh, fake_material, 0),
    ));
    phase.add_item(PhaseItem::new(
        transparent_sort_key(SortingLayer(0), 1, far, &view),
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

#[test]
fn draw_function_rejects_wrong_phase_payload_kind_before_execution() {
    let mesh = crate::render::expert::Mesh::QUAD;
    let fake_material = crate::render::MaterialHandle::new::<SpriteMaterial>(0, 0);
    let entity = crate::ecs::EntityId::new(1, 0);
    let mut draw_functions = DrawFunctionRegistry::new();
    let sprite_draw = draw_functions.register(DrawSprite::new());
    let item = PhaseItem::new(
        0,
        sprite_draw,
        entity,
        0,
        MeshDrawData::new(mesh, fake_material, 0),
    );

    let err = draw_functions
        .validate_phase_payloads(sprite_draw, std::slice::from_ref(&item))
        .unwrap_err();

    assert!(matches!(
        err,
        DrawError::PhasePayloadMismatch { id, .. } if id == sprite_draw
    ));
}

#[test]
fn sprite_draw_data_preserves_public_f32_semantics() {
    let material = crate::render::MaterialHandle::new::<SpriteMaterial>(3, 1);
    let size = [12.5, 4097.25];
    let color = [1.25, -0.25, 0.5, 0.75];
    let uv = [-0.5, 0.25, 1.5, 2.0];

    let data = SpriteDrawData::new(material, size, color, uv);

    assert_eq!(data.size(), size);
    assert_eq!(data.color(), color);
    assert_eq!(data.uv_rect(), uv);
}
