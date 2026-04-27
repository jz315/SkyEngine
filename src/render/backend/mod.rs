//! Backend-neutral renderer selection and backend implementations.

mod scene_renderer;
mod snapshot;
mod wgpu;
mod wgpu_assets;

#[cfg(feature = "kajiya-renderer")]
mod kajiya;
#[cfg(feature = "kajiya-renderer")]
mod kajiya_assets;
#[cfg(feature = "kajiya-renderer")]
mod kajiya_cache;
#[cfg(feature = "kajiya-renderer")]
mod kajiya_config;
#[cfg(feature = "kajiya-renderer")]
mod kajiya_native;

use std::sync::Arc;

use winit::window::Window;

use crate::render::pipeline::{RenderBackendKind, RenderPipelineAsset};

pub use scene_renderer::{SceneRenderer, SceneRendererError, SceneRendererInitError};
pub use snapshot::{
    SceneCamera, SceneDirectionalLight, SceneMeshInstance, SceneSnapshot, SceneSnapshotExtractor,
    SceneSnapshotStats,
};
pub use wgpu::WgpuSceneRenderer;

#[cfg(feature = "kajiya-renderer")]
pub use kajiya::{KajiyaSceneRenderer, KajiyaSceneSyncStats};

pub fn create_scene_renderer(
    window: Arc<Window>,
    vsync: bool,
    pipeline: Option<RenderPipelineAsset>,
) -> Result<Box<dyn SceneRenderer>, SceneRendererInitError> {
    match pipeline.as_ref().map(RenderPipelineAsset::backend_kind) {
        Some(RenderBackendKind::Kajiya) => {
            let pipeline = pipeline.expect("pipeline kind came from Some");
            #[cfg(feature = "kajiya-renderer")]
            {
                Ok(Box::new(KajiyaSceneRenderer::try_new(
                    window, vsync, pipeline,
                )?))
            }
            #[cfg(not(feature = "kajiya-renderer"))]
            {
                let _ = (window, vsync, pipeline);
                Err(SceneRendererInitError::KajiyaUnavailable(
                    "RenderPipelineAsset::kajiya_3d() requires the `kajiya-renderer` feature"
                        .into(),
                ))
            }
        }
        Some(RenderBackendKind::Wgpu) | None => Ok(Box::new(WgpuSceneRenderer::try_new(
            window, vsync, pipeline,
        )?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{AssetConfig, AssetServer, TextureAsset};
    use crate::ecs::World;
    use crate::gpu::GpuContext;
    use crate::math::Projection;
    use crate::render::assets::{
        MeshAsset, MeshAssetDescriptor, MeshVertexLayout, StandardMaterialAsset,
    };
    use crate::render::component::{Camera, DirectionalLight, MeshRenderer, Transform};
    use crate::render::pipeline::RenderPipelineAsset;
    use crate::render::resources::material::StandardMaterial;
    use crate::render::runtime::RenderComposer;

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        uv: [f32; 2],
    }

    fn create_test_device() -> (::wgpu::Device, ::wgpu::Queue) {
        let instance = ::wgpu::Instance::new(&::wgpu::InstanceDescriptor::default());
        let adapter =
            pollster::block_on(instance.request_adapter(&::wgpu::RequestAdapterOptions {
                power_preference: ::wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            }))
            .expect("No suitable GPU adapter found for backend tests");

        pollster::block_on(adapter.request_device(
            &::wgpu::DeviceDescriptor {
                label: Some("backend_test_device"),
                required_features: ::wgpu::Features::empty(),
                required_limits: ::wgpu::Limits::default(),
                memory_hints: ::wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    fn triangle_mesh(label: &'static str) -> MeshAsset {
        MeshAsset::from_raw(MeshAssetDescriptor::new(
            bytemuck::cast_slice(&[
                Vertex {
                    position: [0.0, 0.0, 0.0],
                    uv: [0.0, 0.0],
                },
                Vertex {
                    position: [1.0, 0.0, 0.0],
                    uv: [1.0, 0.0],
                },
                Vertex {
                    position: [0.0, 1.0, 0.0],
                    uv: [0.0, 1.0],
                },
            ]),
            3,
            MeshVertexLayout::position_uv(),
            label,
        ))
    }

    #[test]
    fn wgpu_asset_cache_uploads_cpu_mesh_once_per_asset_source() {
        let (device, queue) = create_test_device();
        let gpu =
            GpuContext::new_headless(device, queue, ::wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut composer = RenderComposer::from_asset(RenderPipelineAsset::builder().build());
        let assets = AssetServer::with_empty_manifest(AssetConfig::default());

        let mesh = assets.insert_runtime(triangle_mesh("backend_triangle"));
        let texture = assets.insert_runtime(TextureAsset::white_pixel());
        let material = assets.insert_runtime(StandardMaterialAsset::new().albedo_texture(texture));

        let mut cache = super::wgpu_assets::WgpuRenderAssetCache::default();
        let first = cache
            .sync_mesh(&gpu, &mut composer, &assets, mesh)
            .expect("mesh should upload");
        let second = cache
            .sync_mesh(&gpu, &mut composer, &assets, mesh)
            .expect("mesh should stay resident");

        assert_eq!(first, second);
        assert!(composer.mesh(first).is_some());

        let material_first = cache
            .sync_standard_material(&gpu, &mut composer, &assets, material)
            .expect("material should upload");
        let material_second = cache
            .sync_standard_material(&gpu, &mut composer, &assets, material)
            .expect("material should stay resident");
        assert_eq!(material_first, material_second);
        assert!(composer
            .materials::<StandardMaterial>()
            .get(material_first)
            .is_some());
    }

    #[test]
    fn scene_snapshot_extracts_neutral_3d_scene() {
        let mut world = World::new();
        let assets = AssetServer::with_empty_manifest(AssetConfig::default());
        let mesh = assets.insert_runtime(triangle_mesh("snapshot_triangle"));
        let material = assets.insert_runtime(StandardMaterialAsset::new());
        world.insert_resource(assets);

        let mesh_entity = world.spawn((
            Transform::from_xyz(1.0, 2.0, 3.0),
            MeshRenderer::new(mesh, material),
        ));
        let _light = world.spawn((Transform::default(), DirectionalLight::default()));
        let _camera = world.spawn((
            Transform::default(),
            Camera::new(),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        ));

        let mut extractor = SceneSnapshotExtractor::new();
        let mut snapshot = SceneSnapshot::default();
        extractor.extract_into(&world, &mut snapshot);
        assert_eq!(
            snapshot.stats(),
            SceneSnapshotStats {
                mesh_instances: 1,
                directional_lights: 1,
                cameras: 1,
            }
        );
        let first_transform = snapshot
            .mesh_instance(mesh_entity)
            .expect("mesh instance should be synced")
            .transform;

        world
            .get_mut::<Transform>(mesh_entity)
            .expect("mesh transform should exist")
            .position[0] = 5.0;
        extractor.extract_into(&world, &mut snapshot);
        let updated_transform = snapshot
            .mesh_instance(mesh_entity)
            .expect("mesh instance should stay synced")
            .transform;
        assert_ne!(first_transform, updated_transform);

        world.despawn(mesh_entity);
        extractor.extract_into(&world, &mut snapshot);
        assert_eq!(snapshot.stats().mesh_instances, 0);
    }

    #[cfg(feature = "kajiya-renderer")]
    #[test]
    fn kajiya_scene_renderer_uses_neutral_scene_snapshot() {
        let mut world = World::new();
        let assets = AssetServer::with_empty_manifest(AssetConfig::default());
        let mesh = assets.insert_runtime(triangle_mesh("kajiya_snapshot_triangle"));
        let material = assets.insert_runtime(StandardMaterialAsset::new());
        world.insert_resource(assets);

        let mesh_entity = world.spawn((
            Transform::from_xyz(1.0, 2.0, 3.0),
            MeshRenderer::new(mesh, material),
        ));
        let _light = world.spawn((Transform::default(), DirectionalLight::default()));
        let _camera = world.spawn((
            Transform::default(),
            Camera::new(),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        ));

        let mut renderer = KajiyaSceneRenderer::new_for_tests([128, 96]);
        renderer.render_world(&world);
        assert_eq!(
            renderer.sync_stats(),
            KajiyaSceneSyncStats {
                mesh_instances: 1,
                directional_lights: 1,
                cameras: 1,
            }
        );
        assert!(renderer.snapshot().mesh_instance(mesh_entity).is_some());
    }
}
