use std::sync::Arc;

use winit::window::Window;

use crate::asset::AssetServer;
use crate::ecs::World;
use crate::render::pipeline::{RenderBackendKind, RenderPipelineAsset};
use crate::render::view::RenderStats;

mod assets;
mod cache;
mod config;
mod error;
mod native;

use self::config::KajiyaRendererConfig;
use self::native::NativeKajiyaRuntime;

use super::scene_renderer::{SceneRenderer, SceneRendererError, SceneRendererInitError};
use super::{SceneSnapshot, SceneSnapshotExtractor, SceneSnapshotStats};

pub type KajiyaSceneSyncStats = SceneSnapshotStats;

pub struct KajiyaSceneRenderer {
    window: Option<Arc<Window>>,
    config: KajiyaRendererConfig,
    vsync: bool,
    surface_size: [u32; 2],
    triangle_only: bool,
    warned: bool,
    frame_index: u64,
    stats: RenderStats,
    snapshot_extractor: SceneSnapshotExtractor,
    snapshot: SceneSnapshot,
    runtime: Option<NativeKajiyaRuntime>,
}

impl KajiyaSceneRenderer {
    pub fn try_new(
        window: Arc<Window>,
        vsync: bool,
        pipeline: RenderPipelineAsset,
    ) -> Result<Self, SceneRendererInitError> {
        let size = window.inner_size();
        let surface_size = [size.width.max(1), size.height.max(1)];
        let triangle_only = pipeline.feature_names.contains(&"kajiya_triangle");
        let config = KajiyaRendererConfig::from_settings(pipeline.kajiya_settings());
        if config.trace_enabled() {
            eprintln!(
                "[SkyEngine][Kajiya] create renderer surface={}x{} vsync={} triangle_only={}",
                surface_size[0], surface_size[1], vsync, triangle_only
            );
        }
        let runtime =
            NativeKajiyaRuntime::try_new(&window, &config, vsync, surface_size, triangle_only)
                .map_err(|error| SceneRendererInitError::Other(error.to_string()))?;
        Ok(Self {
            window: Some(window),
            config,
            vsync,
            surface_size,
            triangle_only,
            warned: false,
            frame_index: 0,
            stats: RenderStats::default(),
            snapshot_extractor: SceneSnapshotExtractor::new(),
            snapshot: SceneSnapshot::default(),
            runtime: Some(runtime),
        })
    }

    #[cfg(test)]
    fn new_with_size(surface_size: [u32; 2]) -> Self {
        Self {
            window: None,
            config: KajiyaRendererConfig::from_settings(Default::default()),
            vsync: false,
            surface_size,
            triangle_only: false,
            warned: false,
            frame_index: 0,
            stats: RenderStats::default(),
            snapshot_extractor: SceneSnapshotExtractor::new(),
            snapshot: SceneSnapshot::default(),
            runtime: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests(surface_size: [u32; 2]) -> Self {
        Self::new_with_size(surface_size)
    }

    #[inline]
    pub fn snapshot(&self) -> &SceneSnapshot {
        &self.snapshot
    }

    #[inline]
    pub fn sync_stats(&self) -> KajiyaSceneSyncStats {
        self.snapshot.stats()
    }

    fn sync_world(&mut self, world: &World) {
        self.snapshot_extractor
            .extract_into(world, &mut self.snapshot);
        let stats = self.snapshot.stats();
        self.stats.view_count = stats.cameras;
        self.stats.light_count = stats.directional_lights + stats.spot_lights;
        self.stats.draw_calls = stats.mesh_instances;
        self.stats.resident_render_assets = stats.mesh_instances;
    }

    fn ensure_runtime(&mut self) -> Option<&mut NativeKajiyaRuntime> {
        if self.runtime.is_none() {
            let Some(window) = self.window.as_ref() else {
                if !self.warned {
                    eprintln!(
                        "[SkyEngine] Kajiya renderer test stub is active without a native runtime"
                    );
                    self.warned = true;
                }
                return None;
            };

            match NativeKajiyaRuntime::try_new(
                window,
                &self.config,
                self.vsync,
                self.surface_size,
                self.triangle_only,
            ) {
                Ok(runtime) => {
                    if self.config.trace_enabled() {
                        eprintln!(
                            "[SkyEngine][Kajiya] recreated native runtime surface={}x{} triangle_only={}",
                            self.surface_size[0], self.surface_size[1], self.triangle_only
                        );
                    }
                    self.runtime = Some(runtime);
                    self.warned = false;
                }
                Err(error) => {
                    if !self.warned {
                        eprintln!("[SkyEngine] Failed to recreate Kajiya renderer: {error}");
                        self.warned = true;
                    }
                    return None;
                }
            }
        }

        self.runtime.as_mut()
    }
}

impl SceneRenderer for KajiyaSceneRenderer {
    fn backend_kind(&self) -> RenderBackendKind {
        RenderBackendKind::Kajiya
    }

    fn begin_frame(&mut self) -> Result<(), SceneRendererError> {
        if self.config.should_trace_frame(self.frame_index) {
            eprintln!(
                "[SkyEngine][Kajiya] begin_frame frame={} surface={}x{}",
                self.frame_index, self.surface_size[0], self.surface_size[1]
            );
        }
        Ok(())
    }

    fn end_frame(&mut self) {
        if self.config.should_trace_frame(self.frame_index) {
            eprintln!("[SkyEngine][Kajiya] end_frame frame={}", self.frame_index);
        }
        self.frame_index = self.frame_index.wrapping_add(1);
    }

    fn render_world(&mut self, world: &World) {
        let frame = self.frame_index;
        self.sync_world(world);
        let scene_stats = self.snapshot.stats();
        let trace = self.config.should_trace_frame(frame);
        if trace {
            eprintln!(
                "[SkyEngine][Kajiya] render_world frame={} triangle_only={} cameras={} meshes={} lights={} has_assets={}",
                frame,
                self.triangle_only,
                scene_stats.cameras,
                scene_stats.mesh_instances,
                scene_stats.directional_lights,
                world.contains_resource::<AssetServer>()
            );
        }
        let snapshot = self.snapshot.clone();
        let assets = world.get_resource::<AssetServer>().cloned();
        if let Some(runtime) = self.ensure_runtime() {
            if trace {
                eprintln!("[SkyEngine][Kajiya] native render begin frame={}", frame);
            }
            if let Err(error) = runtime.render(&snapshot, assets.as_ref()) {
                if !self.warned {
                    eprintln!("[SkyEngine] Kajiya render failed: {error}");
                    self.warned = true;
                }
            } else {
                let asset_stats = runtime.asset_sync_stats();
                self.stats.resident_render_assets = asset_stats.resident_meshes;
                self.stats.uploaded_render_assets = asset_stats.uploaded_meshes;
                self.stats.draw_calls = asset_stats.resident_instances;
                if trace {
                    eprintln!(
                        "[SkyEngine][Kajiya] native render ok frame={} resident_meshes={} uploaded_meshes={} resident_instances={}",
                        frame,
                        asset_stats.resident_meshes,
                        asset_stats.uploaded_meshes,
                        asset_stats.resident_instances
                    );
                }
                self.warned = false;
            }
        } else if trace {
            eprintln!(
                "[SkyEngine][Kajiya] native runtime unavailable frame={}",
                frame
            );
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.surface_size = [width.max(1), height.max(1)];
        let needs_rebuild = self
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.swapchain_extent() != self.surface_size);
        if needs_rebuild {
            if self.config.trace_enabled() {
                eprintln!(
                    "[SkyEngine][Kajiya] resize requested surface={}x{}; native runtime will be rebuilt",
                    self.surface_size[0], self.surface_size[1]
                );
            }
            self.runtime = None;
        }
    }

    fn surface_lost(&mut self) {}

    fn stats(&self) -> RenderStats {
        self.stats
    }

    fn surface_size(&self) -> [u32; 2] {
        self.surface_size
    }

    fn adapter_name(&self) -> &str {
        "Kajiya"
    }

    fn backend_name(&self) -> &str {
        "Vulkan"
    }
}
