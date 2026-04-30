use std::time::Instant;

use winit::window::Window;

use crate::{asset::AssetServer, math::Projection};

use super::kajiya_assets::{KajiyaAssetSyncStats, KajiyaRenderAssetCache};
use super::kajiya_cache;
use super::kajiya_config::KajiyaRendererConfig;
use super::kajiya_error::KajiyaBackendError;
use super::{SceneCamera, SceneSnapshot};

pub(crate) struct NativeKajiyaRuntime {
    config: KajiyaRendererConfig,
    render_backend: ::kajiya::backend::vulkan::RenderBackend,
    rg_renderer: ::kajiya::rg::renderer::Renderer,
    world_renderer: ::kajiya::world_renderer::WorldRenderer,
    ui_renderer: ::kajiya::ui_renderer::UiRenderer,
    asset_cache: KajiyaRenderAssetCache,
    asset_sync_stats: KajiyaAssetSyncStats,
    render_extent: [u32; 2],
    swapchain_extent: [u32; 2],
    last_frame: Instant,
    frame_index: u64,
    triangle_only: bool,
}

impl NativeKajiyaRuntime {
    pub(crate) fn try_new(
        window: &Window,
        config: &KajiyaRendererConfig,
        vsync: bool,
        surface_size: [u32; 2],
        triangle_only: bool,
    ) -> Result<Self, KajiyaBackendError> {
        kajiya_cache::configure_vfs(config.vendor_root(), config.cache_dir());
        std::env::set_var("SMOL_THREADS", "64");

        let window_handle = KajiyaWindowHandle03::from_window(window)?;
        let swapchain_extent = [surface_size[0].max(1), surface_size[1].max(1)];
        let temporal_upscale_extent = config.temporal_upscale_extent(window, swapchain_extent);
        let render_extent = config.render_extent(temporal_upscale_extent);
        if config.trace_enabled() {
            eprintln!(
                "[SkyEngine][KajiyaNative] init begin root={} cache={} surface={}x{} temporal_upscale={}x{} render={}x{} vsync={} triangle_only={}",
                config.vendor_root().display(),
                config.cache_dir().display(),
                swapchain_extent[0],
                swapchain_extent[1],
                temporal_upscale_extent[0],
                temporal_upscale_extent[1],
                render_extent[0],
                render_extent[1],
                vsync,
                triangle_only
            );
        }

        let render_backend = ::kajiya::backend::RenderBackend::new(
            &window_handle,
            ::kajiya::backend::vulkan::RenderBackendConfig {
                swapchain_extent,
                vsync,
                graphics_debugging: false,
                device_index: config.device_index(),
            },
        )
        .map_err(|error| KajiyaBackendError::create_renderer("Vulkan backend", error))?;

        let lazy_cache = turbosloth::LazyCache::create();
        let mut world_renderer = ::kajiya::world_renderer::WorldRenderer::new(
            render_extent,
            temporal_upscale_extent,
            &render_backend,
            &lazy_cache,
        )
        .map_err(|error| KajiyaBackendError::create_renderer("WorldRenderer", error))?;
        world_renderer.sun_size_multiplier = config.sun_size_multiplier();
        world_renderer.use_taa_jitter = config.taa_jitter_enabled();
        world_renderer.motion_blur_enabled = config.motion_blur_enabled();
        let ui_renderer = ::kajiya::ui_renderer::UiRenderer::default();
        let rg_renderer = ::kajiya::rg::renderer::Renderer::new(&render_backend)
            .map_err(|error| KajiyaBackendError::create_renderer("render graph renderer", error))?;
        let asset_cache = KajiyaRenderAssetCache::new(config.cache_dir().clone());
        if config.trace_enabled() {
            eprintln!(
                "[SkyEngine][KajiyaNative] init ok render_extent={}x{} temporal_upscale={}x{} swapchain={}x{} ray_tracing={} device_index={:?} sun_size={:.3} taa_jitter={} motion_blur={}",
                render_extent[0],
                render_extent[1],
                temporal_upscale_extent[0],
                temporal_upscale_extent[1],
                swapchain_extent[0],
                swapchain_extent[1],
                world_renderer.ray_tracing_enabled(),
                config.device_index(),
                config.sun_size_multiplier(),
                config.taa_jitter_enabled(),
                config.motion_blur_enabled()
            );
        }

        Ok(Self {
            config: config.clone(),
            render_backend,
            rg_renderer,
            world_renderer,
            ui_renderer,
            asset_cache,
            asset_sync_stats: KajiyaAssetSyncStats::default(),
            render_extent,
            swapchain_extent,
            last_frame: Instant::now(),
            frame_index: 0,
            triangle_only,
        })
    }

    #[inline]
    pub(crate) fn swapchain_extent(&self) -> [u32; 2] {
        self.swapchain_extent
    }

    #[inline]
    pub(crate) fn asset_sync_stats(&self) -> KajiyaAssetSyncStats {
        self.asset_sync_stats
    }

    pub(crate) fn render(
        &mut self,
        snapshot: &SceneSnapshot,
        assets: Option<&AssetServer>,
    ) -> Result<(), KajiyaBackendError> {
        let frame = self.frame_index;
        if self.config.should_trace_frame(frame) {
            eprintln!(
                "[SkyEngine][KajiyaNative] render begin frame={} triangle_only={} assets={} swapchain={}x{}",
                frame,
                self.triangle_only,
                assets.is_some(),
                self.swapchain_extent[0],
                self.swapchain_extent[1]
            );
        }
        gpu_profiler::profiler().begin_frame();
        let result = self.render_inner(snapshot, assets);
        gpu_profiler::profiler().end_frame();
        if self.config.should_trace_frame(frame) {
            match &result {
                Ok(()) => eprintln!("[SkyEngine][KajiyaNative] render end frame={} ok", frame),
                Err(error) => eprintln!(
                    "[SkyEngine][KajiyaNative] render end frame={} error={}",
                    frame, error
                ),
            }
        }
        self.frame_index = self.frame_index.wrapping_add(1);
        result
    }

    fn render_inner(
        &mut self,
        snapshot: &SceneSnapshot,
        assets: Option<&AssetServer>,
    ) -> Result<(), KajiyaBackendError> {
        if self.triangle_only {
            return self.render_triangle();
        }

        self.asset_sync_stats =
            self.asset_cache
                .sync_snapshot(&mut self.world_renderer, assets, snapshot);
        if self.config.should_trace_frame(self.frame_index) {
            eprintln!(
                "[SkyEngine][KajiyaNative] asset sync frame={} resident_meshes={} uploaded_meshes={} resident_instances={} topology_changed={}",
                self.frame_index,
                self.asset_sync_stats.resident_meshes,
                self.asset_sync_stats.uploaded_meshes,
                self.asset_sync_stats.resident_instances,
                self.asset_sync_stats.topology_changed
            );
        }
        self.world_renderer.sun_color_multiplier = kajiya_sun_color_multiplier(snapshot);
        if self.world_renderer.ray_tracing_enabled()
            && self.asset_sync_stats.resident_instances > 0
            && (self.asset_sync_stats.topology_changed
                || !self.world_renderer.has_ray_tracing_top_level_acceleration())
        {
            self.world_renderer
                .build_ray_tracing_top_level_acceleration();
        }

        let frame_desc = self.frame_desc(snapshot);
        let swapchain_extent = self.swapchain_extent;

        let prepared_frame = self.rg_renderer.prepare_frame(|rg| {
            rg.debug_hook = self.world_renderer.rg_debug_hook.take();
            let main_img = self.world_renderer.prepare_render_graph(rg, &frame_desc);
            let ui_img = self.ui_renderer.prepare_render_graph(rg);

            let mut swap_chain = rg.get_swap_chain();
            ::kajiya::rg::SimpleRenderPass::new_compute(
                rg.add_pass("final blit"),
                "/shaders/final_blit.hlsl",
            )
            .read(&main_img)
            .read(&ui_img)
            .write(&mut swap_chain)
            .constants((
                main_img.desc().extent_inv_extent_2d(),
                [
                    swapchain_extent[0] as f32,
                    swapchain_extent[1] as f32,
                    1.0 / swapchain_extent[0] as f32,
                    1.0 / swapchain_extent[1] as f32,
                ],
            ))
            .dispatch([swapchain_extent[0], swapchain_extent[1], 1]);
        });

        prepared_frame.map_err(|error| KajiyaBackendError::prepare_frame("world", error))?;
        if self.config.should_trace_frame(self.frame_index) {
            eprintln!(
                "[SkyEngine][KajiyaNative] world prepare_frame ok frame={}",
                self.frame_index
            );
        }

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().clamp(0.0, 1.0 / 15.0);
        self.last_frame = now;

        self.rg_renderer.draw_frame(
            |dynamic_constants| {
                self.world_renderer
                    .prepare_frame_constants(dynamic_constants, &frame_desc, dt)
            },
            &mut self.render_backend.swapchain,
        );
        if self.config.should_trace_frame(self.frame_index) {
            eprintln!(
                "[SkyEngine][KajiyaNative] world draw_frame done frame={} dt={:.4}",
                self.frame_index, dt
            );
        }
        self.world_renderer.retire_frame();
        Ok(())
    }

    fn render_triangle(&mut self) -> Result<(), KajiyaBackendError> {
        let swapchain_extent = self.swapchain_extent;
        if self.config.should_trace_frame(self.frame_index) {
            eprintln!(
                "[SkyEngine][KajiyaNative] triangle prepare begin frame={} extent={}x{} shader=/shaders/sky_debug_triangle.hlsl",
                self.frame_index, swapchain_extent[0], swapchain_extent[1]
            );
        }
        let prepared_frame = self.rg_renderer.prepare_frame(|rg| {
            let mut swap_chain = rg.get_swap_chain();
            ::kajiya::rg::SimpleRenderPass::new_compute(
                rg.add_pass("sky debug triangle"),
                "/shaders/sky_debug_triangle.hlsl",
            )
            .write(&mut swap_chain)
            .constants((
                [
                    swapchain_extent[0] as f32,
                    swapchain_extent[1] as f32,
                    1.0 / swapchain_extent[0] as f32,
                    1.0 / swapchain_extent[1] as f32,
                ],
                [0.1f32, 0.9, 1.0, 1.0],
            ))
            .dispatch([swapchain_extent[0], swapchain_extent[1], 1]);
        });

        prepared_frame
            .map_err(|error| KajiyaBackendError::prepare_frame("debug triangle", error))?;
        if self.config.should_trace_frame(self.frame_index) {
            eprintln!(
                "[SkyEngine][KajiyaNative] triangle prepare_frame ok frame={}",
                self.frame_index
            );
        }

        self.rg_renderer.draw_frame(
            |_| ::kajiya::rg::renderer::FrameConstantsLayout {
                globals_offset: 0,
                instance_dynamic_parameters_offset: 0,
                triangle_lights_offset: 0,
            },
            &mut self.render_backend.swapchain,
        );
        if self.config.should_trace_frame(self.frame_index) {
            eprintln!(
                "[SkyEngine][KajiyaNative] triangle draw_frame done frame={}",
                self.frame_index
            );
        }
        Ok(())
    }

    fn frame_desc(&self, snapshot: &SceneSnapshot) -> ::kajiya::frame_desc::WorldFrameDesc {
        let camera = snapshot.cameras().next().map(|(_, camera)| camera);
        let camera_matrices = camera
            .map(|camera| self.camera_matrices(camera))
            .unwrap_or_else(|| default_camera_matrices(self.render_extent));
        let sun_direction = kajiya_sun_direction(snapshot);

        ::kajiya::frame_desc::WorldFrameDesc {
            camera_matrices,
            render_extent: self.render_extent,
            sun_direction,
        }
    }

    fn camera_matrices(
        &self,
        camera: &SceneCamera,
    ) -> kajiya_rust_shaders_shared::camera::CameraMatrices {
        let view_to_world = kajiya_mat4(camera.transform);
        let world_to_view = view_to_world.inverse();
        let view_to_clip = kajiya_projection_matrix(camera.projection, self.render_extent);
        let clip_to_view = view_to_clip.inverse();

        kajiya_rust_shaders_shared::camera::CameraMatrices {
            view_to_clip,
            clip_to_view,
            world_to_view,
            view_to_world,
        }
    }
}

fn default_camera_matrices(
    render_extent: [u32; 2],
) -> kajiya_rust_shaders_shared::camera::CameraMatrices {
    let view_to_world =
        ::kajiya::math::Mat4::from_translation(::kajiya::math::Vec3::new(0.0, 0.0, 5.0));
    let world_to_view = view_to_world.inverse();
    let view_to_clip =
        kajiya_perspective_infinite_reverse_z(60.0f32.to_radians(), 0.1, render_extent);
    let clip_to_view = view_to_clip.inverse();

    kajiya_rust_shaders_shared::camera::CameraMatrices {
        view_to_clip,
        clip_to_view,
        world_to_view,
        view_to_world,
    }
}

#[inline]
fn kajiya_mat4(cols: [f32; 16]) -> ::kajiya::math::Mat4 {
    ::kajiya::math::Mat4::from_cols_array(&cols)
}

fn kajiya_sun_direction(snapshot: &SceneSnapshot) -> ::kajiya::math::Vec3 {
    snapshot
        .directional_lights()
        .next()
        .map(|(_, light)| {
            // SkyEngine stores the direction light travels. Kajiya wants the
            // opposite vector: the world-space direction towards the sun.
            -::kajiya::math::Vec3::new(light.direction[0], light.direction[1], light.direction[2])
                .normalize_or_zero()
        })
        .filter(|dir| dir.length_squared() > f32::EPSILON)
        .unwrap_or(::kajiya::math::Vec3::new(0.3, 1.0, 0.2).normalize())
}

fn kajiya_sun_color_multiplier(snapshot: &SceneSnapshot) -> ::kajiya::math::Vec3 {
    snapshot
        .directional_lights()
        .next()
        .map(|(_, light)| {
            ::kajiya::math::Vec3::new(light.color[0], light.color[1], light.color[2])
                * light.intensity.max(0.0)
        })
        .filter(|color| color.length_squared() > f32::EPSILON)
        .unwrap_or(::kajiya::math::Vec3::ONE)
}

fn kajiya_projection_matrix(
    projection: Projection,
    render_extent: [u32; 2],
) -> ::kajiya::math::Mat4 {
    match projection {
        Projection::Perspective {
            vertical_fov_radians,
            near,
            ..
        } => kajiya_perspective_infinite_reverse_z(vertical_fov_radians, near, render_extent),
        Projection::Orthographic { .. } | Projection::OrthographicFixed { .. } => {
            let viewport = crate::math::Vec2::new(render_extent[0] as f32, render_extent[1] as f32);
            kajiya_mat4(projection.projection_matrix(viewport).to_cols_array())
        }
    }
}

fn kajiya_perspective_infinite_reverse_z(
    vertical_fov_radians: f32,
    near: f32,
    render_extent: [u32; 2],
) -> ::kajiya::math::Mat4 {
    let width = render_extent[0].max(1) as f32;
    let height = render_extent[1].max(1) as f32;
    let aspect = width / height.max(f32::EPSILON);
    let fov = vertical_fov_radians.max(0.001);
    let h = (0.5 * fov).cos() / (0.5 * fov).sin();
    let w = h / aspect.max(f32::EPSILON);
    let znear = near.max(0.0001);

    ::kajiya::math::Mat4::from_cols(
        ::kajiya::math::Vec4::new(w, 0.0, 0.0, 0.0),
        ::kajiya::math::Vec4::new(0.0, h, 0.0, 0.0),
        ::kajiya::math::Vec4::new(0.0, 0.0, 0.0, -1.0),
        ::kajiya::math::Vec4::new(0.0, 0.0, znear, 0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::super::SceneSnapshotExtractor;
    use super::*;
    use crate::{
        ecs::World,
        render::backend::kajiya_config,
        render::{DirectionalLight, Transform},
    };
    use kajiya::camera::LookThroughCamera;

    fn assert_mat4_close(actual: ::kajiya::math::Mat4, expected: ::kajiya::math::Mat4) {
        let actual = actual.to_cols_array();
        let expected = expected.to_cols_array();
        for (index, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
            assert!(
                (actual - expected).abs() <= 1e-6,
                "matrix element {index} mismatch: actual {actual}, expected {expected}"
            );
        }
    }

    #[test]
    fn perspective_projection_matches_kajiya_camera_lens() {
        let extent = [1600, 900];
        let fov_degrees = 52.0f32;
        let near = 0.01;
        let projection = kajiya_projection_matrix(
            Projection::perspective(fov_degrees.to_radians(), near, 500.0),
            extent,
        );
        let expected = (::kajiya::math::Vec3::ZERO, ::kajiya::math::Quat::IDENTITY)
            .through(&::kajiya::camera::CameraLens {
                near_plane_distance: near,
                aspect_ratio: extent[0] as f32 / extent[1] as f32,
                vertical_fov: fov_degrees,
            })
            .view_to_clip;

        assert_mat4_close(projection, expected);
    }

    #[test]
    fn directional_light_direction_is_converted_to_kajiya_sun_direction() {
        let mut world = World::new();
        world.spawn((
            Transform::default(),
            DirectionalLight::new([0.0, -1.0, 0.0]),
        ));

        let snapshot = SceneSnapshotExtractor::new().extract(&world);
        let sun_direction = kajiya_sun_direction(&snapshot);

        assert!((sun_direction - ::kajiya::math::Vec3::Y).length() <= 1e-6);
    }

    #[test]
    fn render_extent_follows_temporal_upsampling() {
        assert_eq!(
            kajiya_config::render_extent_for([1280, 720], 1.0),
            [1280, 720]
        );
        assert_eq!(
            kajiya_config::render_extent_for([1280, 720], 2.0),
            [640, 360]
        );
        assert_eq!(kajiya_config::render_extent_for([1, 1], 8.0), [1, 1]);
    }
}

struct KajiyaWindowHandle03 {
    raw: raw_window_handle_03::RawWindowHandle,
}

impl KajiyaWindowHandle03 {
    fn from_window(window: &Window) -> Result<Self, KajiyaBackendError> {
        platform_window_handle_03(window).map(|raw| Self { raw })
    }
}

unsafe impl raw_window_handle_03::HasRawWindowHandle for KajiyaWindowHandle03 {
    fn raw_window_handle(&self) -> raw_window_handle_03::RawWindowHandle {
        self.raw
    }
}

#[cfg(target_os = "windows")]
fn platform_window_handle_03(
    window: &Window,
) -> Result<raw_window_handle_03::RawWindowHandle, KajiyaBackendError> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = window
        .window_handle()
        .map_err(|error| KajiyaBackendError::WindowHandle(error.to_string()))?;

    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => {
            let mut legacy = raw_window_handle_03::windows::WindowsHandle::empty();
            legacy.hwnd = handle.hwnd.get() as *mut _;
            legacy.hinstance = handle
                .hinstance
                .map_or(std::ptr::null_mut(), |value| value.get() as *mut _);
            Ok(raw_window_handle_03::RawWindowHandle::Windows(legacy))
        }
        other => Err(KajiyaBackendError::UnsupportedWindowHandle(format!(
            "Kajiya native runtime requires a Win32 window handle on Windows, got {other:?}"
        ))),
    }
}

#[cfg(not(target_os = "windows"))]
fn platform_window_handle_03(
    _window: &Window,
) -> Result<raw_window_handle_03::RawWindowHandle, KajiyaBackendError> {
    Err(KajiyaBackendError::UnsupportedWindowHandle(
        "Kajiya native runtime currently has a raw-window-handle bridge only for Windows".into(),
    ))
}
