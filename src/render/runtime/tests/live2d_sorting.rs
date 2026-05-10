use super::common::*;

#[cfg(feature = "live2d")]
#[test]
fn live2d_scene_sort_and_layer_visibility_follow_queue_policy() {
    let base = vec![
        Live2DSceneInstance {
            entity: EntityId::new(3, 0),
            model_index: 0,
            transform: Transform::from_xyz(0.0, 0.0, 0.8),
            layer_mask: 0b0001,
            sorting_layer: SortingLayer(10),
        },
        Live2DSceneInstance {
            entity: EntityId::new(1, 0),
            model_index: 1,
            transform: Transform::from_xyz(0.0, 0.0, 0.2),
            layer_mask: 0b0010,
            sorting_layer: SortingLayer(0),
        },
        Live2DSceneInstance {
            entity: EntityId::new(2, 0),
            model_index: 2,
            transform: Transform::from_xyz(0.0, 0.0, 0.1),
            layer_mask: 0b0010,
            sorting_layer: SortingLayer(10),
        },
    ];

    let mut transparent = base.clone();
    sort_live2d_scene_instances(&mut transparent, RenderQueueSort::TransparentScene, None);
    assert_eq!(
        transparent
            .iter()
            .map(|item| (item.entity.index(), item.sorting_layer.0))
            .collect::<Vec<_>>(),
        vec![(1, 0), (2, 10), (3, 10)]
    );

    let mut opaque = base.clone();
    sort_live2d_scene_instances(&mut opaque, RenderQueueSort::OpaqueDepthFrontToBack, None);
    assert_eq!(
        opaque
            .iter()
            .map(|item| (item.entity.index(), item.transform.z()))
            .collect::<Vec<_>>(),
        vec![(2, 0.1), (1, 0.2), (3, 0.8)]
    );

    let projection = Projection::orthographic_fixed(64.0, 64.0);
    let view = SceneView::new(
        0,
        ViewportRect::new(0, 0, 64, 64),
        [64, 64],
        true,
        0b0010,
        Transform::default(),
        projection,
        projection.view_uniform(Transform::default(), [64, 64]),
        true,
    );
    assert!(!live2d_instance_visible_in_view(&base[0], &view));
    assert!(live2d_instance_visible_in_view(&base[1], &view));
    assert!(live2d_instance_visible_in_view(&base[2], &view));
}

#[cfg(feature = "live2d")]
#[test]
fn live2d_perspective_sort_uses_view_relative_depth() {
    let mut instances = vec![
        Live2DSceneInstance {
            entity: EntityId::new(1, 0),
            model_index: 0,
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            layer_mask: u32::MAX,
            sorting_layer: SortingLayer(0),
        },
        Live2DSceneInstance {
            entity: EntityId::new(2, 0),
            model_index: 1,
            transform: Transform::from_xyz(0.0, 0.0, 5.0),
            layer_mask: u32::MAX,
            sorting_layer: SortingLayer(0),
        },
    ];
    let projection = Projection::perspective(60.0f32.to_radians(), 0.1, 1000.0);
    let perspective_view = SceneView::new(
        0,
        ViewportRect::new(0, 0, 64, 64),
        [64, 64],
        true,
        u32::MAX,
        Transform::from_xyz(0.0, 0.0, 10.0),
        projection,
        projection.view_uniform(Transform::from_xyz(0.0, 0.0, 10.0), [64, 64]),
        false,
    );

    sort_live2d_scene_instances(
        &mut instances,
        RenderQueueSort::TransparentScene,
        Some(&perspective_view),
    );
    assert_eq!(
        instances
            .iter()
            .map(|instance| instance.entity.index())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );

    sort_live2d_scene_instances(
        &mut instances,
        RenderQueueSort::OpaqueDepthFrontToBack,
        Some(&perspective_view),
    );
    assert_eq!(
        instances
            .iter()
            .map(|instance| instance.entity.index())
            .collect::<Vec<_>>(),
        vec![2, 1]
    );
}
