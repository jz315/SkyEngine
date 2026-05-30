//! Backend-neutral renderer selection and backend implementations.

mod scene_renderer;
mod snapshot;
mod wgpu;
mod wgpu_asset_bridge;

#[cfg(feature = "kajiya-renderer")]
mod kajiya;
#[cfg(feature = "renderling-renderer")]
mod renderling;

use std::sync::Arc;

use winit::window::Window;

use crate::render::pipeline::{RenderBackendKind, RenderPipelineAsset};

pub use scene_renderer::{
    SceneFrame, SceneFrameClearReason, SceneFrameSkipReason, SceneRenderOutcome, SceneRenderer,
    SceneRendererError, SceneRendererInitError,
};
pub use snapshot::{
    SceneCamera, SceneDirectionalLight, SceneMeshInstance, ScenePointLight, SceneSnapshot,
    SceneSnapshotExtractor, SceneSnapshotStats, SceneSpotLight,
};
pub use wgpu::WgpuSceneRenderer;

#[cfg(feature = "kajiya-renderer")]
pub use kajiya::{KajiyaSceneRenderer, KajiyaSceneSyncStats};
#[cfg(feature = "renderling-renderer")]
pub use renderling::{RenderlingSceneRenderer, RenderlingSceneSyncStats};

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
        Some(RenderBackendKind::Renderling) => {
            let pipeline = pipeline.expect("pipeline kind came from Some");
            #[cfg(feature = "renderling-renderer")]
            {
                Ok(Box::new(RenderlingSceneRenderer::try_new(
                    window, vsync, pipeline,
                )?))
            }
            #[cfg(not(feature = "renderling-renderer"))]
            {
                let _ = (window, vsync, pipeline);
                Err(SceneRendererInitError::RenderlingUnavailable(
                    "RenderPipelineAsset::renderling_3d() requires the `renderling-renderer` feature"
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
    use crate::asset::{AssetConfig, Assets, TextureAsset};
    use crate::ecs::World;
    use crate::gpu::GpuContext;
    use crate::math::Projection;
    use crate::render::asset::{
        MeshAsset, MeshAssetDescriptor, MeshVertexLayout, StandardMaterialAsset,
    };
    use crate::render::component::{
        Camera, DirectionalLight, MeshRenderer, PointLight, RenderSettings, SpotLight, Transform,
    };
    use crate::render::pipeline::RenderPipelineAsset;
    use crate::render::resources::material::StandardMaterial;
    use crate::render::resources::texture_cache::SharedRenderAssetCache;
    use crate::render::runtime::RenderRuntime;

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Vertex {
        position: [f32; 3],
        uv: [f32; 2],
    }

    fn create_test_device() -> (::wgpu::Device, ::wgpu::Queue) {
        let instance =
            ::wgpu::Instance::new(::wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&::wgpu::RequestAdapterOptions {
                power_preference: ::wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            }))
            .expect("No suitable GPU adapter found for backend tests");

        pollster::block_on(adapter.request_device(&::wgpu::DeviceDescriptor {
            label: Some("backend_test_device"),
            required_features: ::wgpu::Features::empty(),
            required_limits: ::wgpu::Limits::default(),
            memory_hints: ::wgpu::MemoryHints::Performance,
            ..Default::default()
        }))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn scene_frame_requires_render_or_clear_before_it_is_presentable() {
        let mut frame = SceneFrame::new(RenderBackendKind::Wgpu);
        assert!(!frame.is_presentable());
        assert_eq!(frame.render_outcome(), SceneRenderOutcome::NotRendered);

        frame.set_render_outcome(SceneRenderOutcome::Skipped(
            SceneFrameSkipReason::MissingPipeline,
        ));
        assert!(!frame.is_presentable());

        frame.set_render_outcome(SceneRenderOutcome::Cleared(
            SceneFrameClearReason::MissingPipeline,
        ));
        assert!(frame.is_presentable());

        frame.mark_pre_present_notified();
        assert!(frame.pre_present_notified());
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
        let mut render_runtime = RenderRuntime::from_asset(RenderPipelineAsset::builder().build());
        let assets = Assets::with_empty_manifest(AssetConfig::default());

        let mesh = assets.insert_runtime(triangle_mesh("backend_triangle"));
        let texture = assets.insert_runtime(TextureAsset::white_pixel());
        let material =
            assets.insert_runtime(StandardMaterialAsset::new().albedo_texture(texture.clone()));
        let render_assets = SharedRenderAssetCache::default();

        let mut cache = super::wgpu_asset_bridge::WgpuRenderAssetCache::default();
        let first = cache
            .sync_mesh(&gpu, &mut render_runtime, &assets, mesh.clone())
            .expect("mesh should upload");
        let second = cache
            .sync_mesh(&gpu, &mut render_runtime, &assets, mesh)
            .expect("mesh should stay resident");

        assert_eq!(first, second);
        assert!(render_runtime.mesh(first).is_some());
        assert_eq!(cache.stats().resident_meshes, 1);
        assert_eq!(cache.stats().resident_standard_materials, 0);

        let material_first = cache.sync_standard_material(
            &gpu,
            &mut render_runtime,
            &assets,
            &render_assets,
            material.clone(),
        );
        assert!(
            material_first.is_some(),
            "material shell should sync even when texture waits for the GPU queue"
        );
        assert_eq!(
            render_assets
                .borrow_mut()
                .texture_readiness(Some(&assets), &texture),
            crate::render::TextureReadiness::GpuQueued
        );
        render_assets.borrow_mut().prepare_queued_textures(&gpu);
        assert_eq!(
            render_assets
                .borrow_mut()
                .texture_readiness(Some(&assets), &texture),
            crate::render::TextureReadiness::GpuReady
        );
        assets
            .replace_runtime(
                &material,
                StandardMaterialAsset::new().albedo_texture(texture),
            )
            .expect("runtime material replace should work");
        let material_second = cache
            .sync_standard_material(&gpu, &mut render_runtime, &assets, &render_assets, material)
            .expect("material should stay resident");
        assert!(render_runtime
            .material_erased::<StandardMaterial>(material_second)
            .is_ok());
        let stats = cache.stats();
        assert_eq!(stats.resident_meshes, 1);
        assert_eq!(stats.resident_standard_materials, 1);
        assert_eq!(stats.resident_assets(), 2);
    }

    #[test]
    fn wgpu_asset_cache_invalidates_mesh_and_material_from_asset_events() {
        let (device, queue) = create_test_device();
        let gpu =
            GpuContext::new_headless(device, queue, ::wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut render_runtime = RenderRuntime::from_asset(RenderPipelineAsset::builder().build());
        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let mesh = assets.insert_runtime(triangle_mesh("backend_event_triangle"));
        let material = assets.insert_runtime(StandardMaterialAsset::new());
        let render_assets = SharedRenderAssetCache::default();
        let mut cache = super::wgpu_asset_bridge::WgpuRenderAssetCache::default();

        let mesh_handle = cache
            .sync_mesh(&gpu, &mut render_runtime, &assets, mesh.clone())
            .expect("mesh should upload");
        let material_handle = cache
            .sync_standard_material(
                &gpu,
                &mut render_runtime,
                &assets,
                &render_assets,
                material.clone(),
            )
            .expect("material should upload");

        let mut cursor = crate::asset::AssetEventCursor::default();
        for event in assets.events_since(&mut cursor) {
            cache.handle_asset_event(&mut render_runtime, &assets, event);
        }
        assert!(render_runtime.mesh(mesh_handle).is_some());
        assert!(render_runtime
            .material_erased::<StandardMaterial>(material_handle)
            .is_ok());
        assert_eq!(cache.stats().resident_assets(), 2);

        assets
            .replace_runtime(&mesh, triangle_mesh("backend_event_triangle_reloaded"))
            .expect("mesh runtime replacement should emit an installed event");
        assets
            .replace_runtime(&material, StandardMaterialAsset::new().roughness(0.42))
            .expect("material runtime replacement should emit an installed event");
        for event in assets.events_since(&mut cursor) {
            cache.handle_asset_event(&mut render_runtime, &assets, event);
        }

        assert!(render_runtime.mesh(mesh_handle).is_none());
        assert!(render_runtime
            .material_erased::<StandardMaterial>(material_handle)
            .is_err());
        assert_eq!(cache.stats().resident_assets(), 0);
        let reloaded_mesh = cache
            .sync_mesh(&gpu, &mut render_runtime, &assets, mesh)
            .expect("mesh should resync after replacement");
        assert!(render_runtime.mesh(reloaded_mesh).is_some());
        assert_eq!(cache.stats().resident_meshes, 1);
        assert_eq!(cache.stats().resident_standard_materials, 0);
    }

    #[test]
    fn scene_snapshot_extracts_neutral_3d_scene() {
        let mut world = World::new();
        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let mesh = assets.insert_runtime(triangle_mesh("snapshot_triangle"));
        let material = assets.insert_runtime(StandardMaterialAsset::new());
        world.insert_resource(assets);

        let mesh_entity = world.spawn((
            Transform::from_xyz(1.0, 2.0, 3.0),
            MeshRenderer::new(mesh, material).shadow_lod_cascades(2),
        ));
        let directional_light = world.spawn((
            Transform::default(),
            DirectionalLight::default().radius(0.035),
        ));
        let point_light = world.spawn((
            Transform::from_xyz(2.0, 3.0, 4.0),
            PointLight::new(12.0).intensity(3.0),
        ));
        let spot_light = world.spawn((
            Transform::from_xyz(-2.0, 5.0, 1.0),
            SpotLight::new(18.0)
                .direction([0.0, -1.0, 0.0])
                .cone_angles(0.25, 0.55)
                .shadow_resolution(512),
        ));
        let _camera = world.spawn((
            Transform::default(),
            Camera::new(),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        ));
        world.insert_resource(RenderSettings {
            global_illumination: crate::render::gi::providers::ssgi::global_illumination(
                crate::render::gi::providers::ssgi::SsgiSettings::default(),
            ),
            ..Default::default()
        });

        let mut extractor = SceneSnapshotExtractor::new();
        let mut snapshot = SceneSnapshot::default();
        extractor.extract_into(&world, &mut snapshot);
        assert_eq!(
            snapshot.stats(),
            SceneSnapshotStats {
                mesh_instances: 1,
                directional_lights: 1,
                point_lights: 1,
                spot_lights: 1,
                cameras: 1,
            }
        );
        assert!(matches!(
            &snapshot.render_settings().global_illumination,
            crate::render::GlobalIllumination::Provider(config)
                if config.id == crate::render::gi::providers::ssgi::SSGI_PROVIDER_ID
        ));
        assert!(
            (snapshot
                .directional_light(directional_light)
                .expect("directional light should be synced")
                .radius
                - 0.035)
                .abs()
                < 0.0001
        );
        assert_eq!(
            snapshot
                .point_light(point_light)
                .expect("point light should be synced")
                .position,
            [2.0, 3.0, 4.0]
        );
        let synced_spot = snapshot
            .spot_light(spot_light)
            .expect("spot light should be synced");
        assert_eq!(synced_spot.position, [-2.0, 5.0, 1.0]);
        assert_eq!(synced_spot.shadow_resolution, 512);
        assert!(synced_spot.casts_shadows);
        let first_transform = snapshot
            .mesh_instance(mesh_entity)
            .expect("mesh instance should be synced")
            .transform;
        let mesh_instance = snapshot
            .mesh_instance(mesh_entity)
            .expect("mesh instance should be synced");
        assert!(mesh_instance.casts_shadows);
        assert_eq!(mesh_instance.shadow_cascade_mask, 0b0011);

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
        let assets = Assets::with_empty_manifest(AssetConfig::default());
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
        let mut frame = renderer
            .begin_frame()
            .expect("test kajiya begin_frame should succeed");
        let _ = renderer.render_world(&mut frame, &world);
        renderer.end_frame(frame);
        assert_eq!(
            renderer.sync_stats(),
            KajiyaSceneSyncStats {
                mesh_instances: 1,
                directional_lights: 1,
                point_lights: 0,
                spot_lights: 0,
                cameras: 1,
            }
        );
        assert!(renderer.snapshot().mesh_instance(mesh_entity).is_some());
    }
}
