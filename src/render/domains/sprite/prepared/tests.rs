use super::*;
use crate::ecs::EntityId;
use crate::gpu::GpuContext;
use crate::render::domains::sprite::SceneSpriteItem;
use crate::render::{
    Camera2D, OrderInLayer, Projection, SortingLayer, SpriteRenderer, Texture, Transform,
    ViewportRect,
};

fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .expect("No suitable GPU adapter found for sprite prepare tests");

    pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("sprite_prepare_test_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .expect("Failed to create sprite prepare test GPU device")
}

fn make_scene_view(order: i32, camera_z: f32, perspective: bool) -> SceneView {
    let projection = if perspective {
        Projection::perspective(60.0f32.to_radians(), 0.1, 1000.0)
    } else {
        Projection::orthographic(64.0, 64.0)
    };
    let transform = Transform::from_xyz(0.0, 0.0, camera_z);
    SceneView::new(
        order,
        ViewportRect::new(0, 0, 64, 64),
        [64, 64],
        true,
        u32::MAX,
        transform,
        projection,
        projection.view_uniform(transform, [64, 64]),
        if perspective {
            None
        } else {
            Some({
                let mut camera = Camera2D::new(64.0, 64.0);
                camera.position = [transform.x(), transform.y()];
                camera.rotation = transform.rotation_z();
                camera
            })
        },
    )
}

fn make_sprite_item(z: f32, sort_key: u64) -> SceneSpriteItem {
    SceneSpriteItem {
        transform: Transform::from_xyz(0.0, 0.0, z),
        sprite: SpriteRenderer::new(1.0, 1.0),
        sorting_layer: SortingLayer(0),
        order_in_layer: OrderInLayer(0),
        sort_key,
        texture_sort_key: 0,
    }
}

fn texture_sort_key(texture: &Texture) -> u64 {
    texture.texture() as *const wgpu::Texture as usize as u64
}

#[test]
fn perspective_transparent_sort_is_back_to_front_per_view_depth() {
    let mut scene = SceneCache2D::new();
    scene.set_sort_policy(RenderQueueSort::TransparentScene);
    scene.upsert_sprite(EntityId::new(1, 0), make_sprite_item(0.0, 1), 1);
    scene.upsert_sprite(EntityId::new(2, 0), make_sprite_item(5.0, 2), 1);

    let mut prepared = PreparedRenderWorld2D::new();
    prepared.prepare_scene(&mut scene, &[make_scene_view(0, 10.0, true)], [64, 64]);

    assert_eq!(prepared.visible_sprite_slots, vec![0, 1]);
}

#[test]
fn perspective_opaque_sort_is_front_to_back_per_view_depth() {
    let mut scene = SceneCache2D::new();
    scene.set_sort_policy(RenderQueueSort::OpaqueDepthFrontToBack);
    scene.upsert_sprite(EntityId::new(1, 0), make_sprite_item(0.0, 1), 1);
    scene.upsert_sprite(EntityId::new(2, 0), make_sprite_item(5.0, 2), 1);

    let mut prepared = PreparedRenderWorld2D::new();
    prepared.prepare_scene(&mut scene, &[make_scene_view(0, 10.0, true)], [64, 64]);

    assert_eq!(prepared.visible_sprite_slots, vec![1, 0]);
}

#[test]
fn texture_sort_key_reduces_draw_span_fragmentation_for_equal_depth_sprites() {
    let (device, queue) = create_test_device();
    let ctx = GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
    let texture_a = Texture::white_pixel(&ctx);
    let texture_b = Texture::checkerboard(&ctx, 2, 1, [255, 0, 0, 255], [0, 0, 0, 255]);

    let mut scene = SceneCache2D::new();
    scene.set_sort_policy(RenderQueueSort::TransparentScene);
    scene.upsert_sprite(
        EntityId::new(1, 0),
        SceneSpriteItem {
            sprite: SpriteRenderer::new(1.0, 1.0).texture(texture_a.clone()),
            texture_sort_key: texture_sort_key(&texture_a),
            ..make_sprite_item(0.0, 1)
        },
        1,
    );
    scene.upsert_sprite(
        EntityId::new(2, 0),
        SceneSpriteItem {
            sprite: SpriteRenderer::new(1.0, 1.0).texture(texture_b.clone()),
            texture_sort_key: texture_sort_key(&texture_b),
            ..make_sprite_item(0.0, 2)
        },
        1,
    );
    scene.upsert_sprite(
        EntityId::new(3, 0),
        SceneSpriteItem {
            sprite: SpriteRenderer::new(1.0, 1.0).texture(texture_a.clone()),
            texture_sort_key: texture_sort_key(&texture_a),
            ..make_sprite_item(0.0, 3)
        },
        1,
    );

    let mut prepared = PreparedRenderWorld2D::new();
    prepared.prepare_scene(&mut scene, &[make_scene_view(0, 0.0, false)], [64, 64]);

    assert_eq!(prepared.draw_spans.len(), 2);
    assert_eq!(
        prepared
            .draw_spans
            .iter()
            .map(|span| span.instance_count)
            .sum::<u32>(),
        3
    );
}
