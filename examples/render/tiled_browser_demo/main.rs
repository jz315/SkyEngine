//! Tiled sample browser demo.
//!
//! ```bash
//! cargo run --example tiled_browser_demo --features app
//! cargo run --example tiled_browser_demo --features app -- path/to/start-map.tmx
//! ```

use std::path::{Path, PathBuf};

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, SetupContext, WindowPlugin,
};
use sky_engine::asset::{Assets, Handle, TextureAsset};
use sky_engine::ecs::{EntityId, World};
use sky_engine::input::KeyCode;
use sky_engine::math::Vec2;
use sky_engine::render::{
    animate_sprites, CameraMarker, Color, MainCamera, Projection, RenderPipelineAsset,
    RenderSettings, SpriteFeature, SpriteRenderer, TiledImport, TiledMapInstance,
    TiledSpawnOptions, TilemapFeature, TilemapRenderer, Transform, TransparentPhase,
};

struct TiledBrowserDemo {
    samples: Vec<TiledSample>,
    active: usize,
    loaded: Option<usize>,
    camera: Option<EntityId>,
    camera_zoom: f32,
    title_dirty: bool,
    status: String,
}

struct TiledSample {
    label: String,
    path: PathBuf,
    cache: Option<CachedTiledMap>,
    instance: Option<TiledMapInstance>,
    load_error: Option<String>,
}

struct CachedTiledMap {
    import: TiledImport,
    textures: Vec<Handle<TextureAsset>>,
}

impl TiledBrowserDemo {
    fn new() -> Self {
        let requested = map_path_from_args();
        let mut samples = demo_samples();
        let active = if let Some(path) = requested {
            match samples
                .iter()
                .position(|sample| same_path(&sample.path, &path))
            {
                Some(index) => index,
                None => {
                    samples.insert(0, TiledSample::from_path(path));
                    0
                }
            }
        } else {
            0
        };

        Self {
            samples,
            active,
            loaded: None,
            camera: None,
            camera_zoom: 1.0,
            title_dirty: true,
            status: String::new(),
        }
    }

    fn load_active(&mut self, world: &mut World) {
        if self.samples.is_empty() {
            self.status = "no demo maps found".to_string();
            self.title_dirty = true;
            return;
        }
        self.active %= self.samples.len();
        if let Err(error) = self.ensure_sample_instance(world, self.active) {
            let path = self.samples[self.active].path.clone();
            self.status = format!("load failed: {error}");
            eprintln!(
                "[Tiled Browser] Failed to prepare {}: {error}",
                path.display()
            );
            self.title_dirty = true;
            return;
        }

        let path = self.samples[self.active].path.clone();
        if self.loaded != Some(self.active) {
            if let Some(previous) = self.loaded {
                self.set_sample_visible(world, previous, false);
            }
        }
        self.set_sample_visible(world, self.active, true);
        self.loaded = Some(self.active);
        self.sync_loaded_parallax(world);
        self.status =
            "WASD/Arrows: move | Wheel or +/-: zoom | [/]/Q/E: switch | R: reload".to_string();
        eprintln!(
            "[Tiled Browser] Loaded {}/{}: {}",
            self.active + 1,
            self.samples.len(),
            path.display()
        );
        self.title_dirty = true;
    }

    fn ensure_sample_instance(
        &mut self,
        world: &mut World,
        sample_index: usize,
    ) -> Result<(), String> {
        self.preload_sample(world, sample_index)?;
        if self
            .samples
            .get(sample_index)
            .and_then(|sample| sample.instance.as_ref())
            .is_some()
        {
            return Ok(());
        }

        let sample = self
            .samples
            .get(sample_index)
            .ok_or_else(|| format!("sample index {sample_index} is out of range"))?;
        let cache = sample.cache.as_ref().ok_or_else(|| {
            sample
                .load_error
                .clone()
                .unwrap_or_else(|| "map is not cached".into())
        })?;
        let instance = TiledMapInstance::spawn_import_with_textures(
            world,
            &cache.import,
            cache.textures.clone(),
            TiledSpawnOptions::centered(),
        )
        .map_err(|error| error.to_string())?;
        let sample = self
            .samples
            .get_mut(sample_index)
            .expect("sample index was checked above");
        sample.instance = Some(instance);
        self.set_sample_visible(world, sample_index, false);
        Ok(())
    }

    fn clear_loaded(&mut self, world: &mut World) {
        if let Some(index) = self.loaded.take() {
            self.set_sample_visible(world, index, false);
        }
    }

    fn spawn_all_instances(&mut self, world: &mut World) {
        for index in 0..self.samples.len() {
            if let Err(error) = self.ensure_sample_instance(world, index) {
                let path = self.samples[index].path.clone();
                eprintln!(
                    "[Tiled Browser] Failed to prepare {}: {error}",
                    path.display()
                );
            }
        }
    }

    fn despawn_sample_instance(&mut self, world: &mut World, index: usize) {
        let Some(sample) = self.samples.get_mut(index) else {
            return;
        };
        let Some(instance) = sample.instance.take() else {
            return;
        };
        instance.despawn(world);
        if self.loaded == Some(index) {
            self.loaded = None;
        }
    }

    fn despawn_all_instances(&mut self, world: &mut World) {
        for index in 0..self.samples.len() {
            self.despawn_sample_instance(world, index);
        }
    }

    fn set_sample_visible(&self, world: &mut World, index: usize, visible: bool) {
        let Some(instance) = self
            .samples
            .get(index)
            .and_then(|sample| sample.instance.as_ref())
        else {
            return;
        };

        for &entity in &instance.entities {
            if let Some(renderer) = world.get_mut::<TilemapRenderer>(entity) {
                renderer.visible = visible;
                renderer.cache_prewarm = !visible;
            }
            if let Some(sprite) = world.get_mut::<SpriteRenderer>(entity) {
                sprite.visible = visible;
            }
        }
    }

    fn preload_all(&mut self, world: &mut World) {
        for index in 0..self.samples.len() {
            if let Err(error) = self.preload_sample(world, index) {
                let path = self.samples[index].path.clone();
                eprintln!(
                    "[Tiled Browser] Failed to preload {}: {error}",
                    path.display()
                );
            }
        }
    }

    fn preload_sample(&mut self, world: &mut World, index: usize) -> Result<(), String> {
        let sample = self
            .samples
            .get(index)
            .ok_or_else(|| format!("sample index {index} is out of range"))?;
        if sample.cache.is_some() {
            return Ok(());
        }

        let asset_server = world
            .get_resource::<Assets>()
            .cloned()
            .ok_or_else(|| "Assets resource is missing".to_string())?;
        let path = sample.path.clone();
        let import = TiledImport::from_file(&path).map_err(|error| error.to_string())?;
        let textures = import
            .load_tileset_textures()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|texture_asset| asset_server.insert_runtime(texture_asset))
            .collect::<Vec<_>>();

        let sample = self
            .samples
            .get_mut(index)
            .expect("sample index was checked above");
        sample.cache = Some(CachedTiledMap { import, textures });
        sample.load_error = None;
        Ok(())
    }

    fn unload_sample_cache(&mut self, world: &mut World, index: usize) {
        self.despawn_sample_instance(world, index);
        let Some(sample) = self.samples.get_mut(index) else {
            return;
        };
        let Some(cache) = sample.cache.take() else {
            return;
        };
        drop(cache);
    }

    fn unload_all_caches(&mut self, world: &mut World) {
        for index in 0..self.samples.len() {
            self.unload_sample_cache(world, index);
        }
    }

    fn reload_active(&mut self, world: &mut World) {
        if self.samples.is_empty() {
            return;
        }
        self.active %= self.samples.len();
        self.unload_sample_cache(world, self.active);
        self.load_active(world);
    }

    fn switch_by(&mut self, world: &mut World, delta: isize) {
        if self.samples.is_empty() {
            return;
        }
        let len = self.samples.len() as isize;
        self.active = (self.active as isize + delta).rem_euclid(len) as usize;
        self.load_active(world);
    }

    fn handle_input(&mut self, ctx: &mut FrameContext<'_>) {
        if ctx.input.key_pressed(KeyCode::Escape) {
            ctx.request_exit();
            return;
        }
        if ctx.input.key_pressed(KeyCode::PageDown)
            || ctx.input.key_pressed(KeyCode::BracketRight)
            || ctx.input.key_pressed(KeyCode::KeyE)
        {
            self.switch_by(ctx.world, 1);
        } else if ctx.input.key_pressed(KeyCode::PageUp)
            || ctx.input.key_pressed(KeyCode::BracketLeft)
            || ctx.input.key_pressed(KeyCode::KeyQ)
        {
            self.switch_by(ctx.world, -1);
        } else if ctx.input.key_pressed(KeyCode::KeyR) {
            self.reload_active(ctx.world);
        }
    }

    fn update_camera(&mut self, ctx: &mut FrameContext<'_>) {
        let Some(camera) = self.camera else {
            return;
        };

        let mut direction = [0.0f32, 0.0f32];
        if ctx.input.key_held(KeyCode::KeyA) || ctx.input.key_held(KeyCode::ArrowLeft) {
            direction[0] -= 1.0;
        }
        if ctx.input.key_held(KeyCode::KeyD) || ctx.input.key_held(KeyCode::ArrowRight) {
            direction[0] += 1.0;
        }
        if ctx.input.key_held(KeyCode::KeyW) || ctx.input.key_held(KeyCode::ArrowUp) {
            direction[1] += 1.0;
        }
        if ctx.input.key_held(KeyCode::KeyS) || ctx.input.key_held(KeyCode::ArrowDown) {
            direction[1] -= 1.0;
        }

        let length = (direction[0] * direction[0] + direction[1] * direction[1]).sqrt();
        if length > f32::EPSILON {
            let speed = 820.0 / self.camera_zoom.max(0.1);
            let delta = speed * ctx.dt / length;
            if let Some(transform) = ctx.world.get_mut::<Transform>(camera) {
                transform.position[0] += direction[0] * delta;
                transform.position[1] += direction[1] * delta;
            }
        }

        let scroll_zoom = ctx.input.scroll_delta()[1].clamp(-6.0, 6.0);
        let mut zoom_factor = 1.0;
        if scroll_zoom.abs() > f32::EPSILON {
            zoom_factor *= 1.12_f32.powf(scroll_zoom);
        }
        if ctx.input.key_pressed(KeyCode::Equal) {
            zoom_factor *= 1.2;
        }
        if ctx.input.key_pressed(KeyCode::Minus) {
            zoom_factor /= 1.2;
        }
        if (zoom_factor - 1.0).abs() > f32::EPSILON {
            self.camera_zoom = (self.camera_zoom * zoom_factor).clamp(0.1, 12.0);
            if let Some(projection) = ctx.world.get_mut::<Projection>(camera) {
                set_projection_zoom(projection, self.camera_zoom);
            }
            self.title_dirty = true;
        }

        if ctx.input.key_pressed(KeyCode::Digit0) {
            self.camera_zoom = 1.0;
            if let Some(transform) = ctx.world.get_mut::<Transform>(camera) {
                transform.position[0] = 0.0;
                transform.position[1] = 0.0;
            }
            if let Some(projection) = ctx.world.get_mut::<Projection>(camera) {
                set_projection_zoom(projection, self.camera_zoom);
            }
            self.title_dirty = true;
        }

        snap_camera_to_pixel_grid(ctx.world, camera, ctx.physical_surface_size().to_array());
        self.sync_loaded_parallax(ctx.world);
    }

    fn sync_loaded_parallax(&self, world: &mut World) {
        let (Some(loaded), Some(camera_position)) = (&self.loaded, self.camera_position(world))
        else {
            return;
        };
        let Some(instance) = self
            .samples
            .get(*loaded)
            .and_then(|sample| sample.instance.as_ref())
        else {
            return;
        };
        instance.sync_parallax(world, camera_position);
    }

    fn camera_position(&self, world: &World) -> Option<[f32; 2]> {
        let camera = self.camera?;
        let transform = world.get::<Transform>(camera)?;
        Some([transform.position[0], transform.position[1]])
    }

    fn update_title(&mut self, ctx: &FrameContext<'_>) {
        if !self.title_dirty {
            return;
        }
        let (index, count, label) = if self.samples.is_empty() {
            (0, 0, "No Maps".to_string())
        } else {
            (
                self.active + 1,
                self.samples.len(),
                self.samples[self.active].label.clone(),
            )
        };
        ctx.set_title(&format!(
            "SkyEngine - Tiled Browser | {index}/{count} {label} | Zoom {:.2}x | {}",
            self.camera_zoom, self.status
        ));
        self.title_dirty = false;
    }
}

impl AppState for TiledBrowserDemo {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let world = &mut *ctx.world;
        world
            .stage(sky_engine::ecs::Update)
            .add_exclusive(animate_sprites);

        self.camera = Some(world.spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            CameraMarker::new(),
            Projection::orthographic(1320.0),
            MainCamera,
        )));

        world.insert_resource(RenderSettings {
            clear_color: Color::rgb(0.025, 0.03, 0.034),
            ..Default::default()
        });
        eprintln!(
            "[Tiled Browser] WASD/Arrows move, wheel or +/- zoom, [/]/Q/E switch maps, R reloads, Esc exits."
        );
        self.preload_all(world);
        self.spawn_all_instances(world);
        self.load_active(world);
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        self.handle_input(ctx);
        self.update_camera(ctx);
        ctx.render();
        self.update_title(ctx);
    }

    fn shutdown(&mut self, world: &mut World) {
        self.clear_loaded(world);
        self.despawn_all_instances(world);
        self.unload_all_caches(world);
    }
}

fn set_projection_zoom(projection: &mut Projection, zoom_value: f32) {
    match projection {
        Projection::Orthographic { zoom, .. } | Projection::OrthographicFixed { zoom, .. } => {
            *zoom = zoom_value;
        }
        Projection::Perspective { .. } => {}
    }
}

fn snap_camera_to_pixel_grid(world: &mut World, camera: EntityId, surface_size: [u32; 2]) {
    let Some(projection) = world.get::<Projection>(camera).copied() else {
        return;
    };
    let Some(visible_size) = projection.orthographic_size(Vec2::new(
        surface_size[0].max(1) as f32,
        surface_size[1].max(1) as f32,
    )) else {
        return;
    };
    let pixel_step = [
        visible_size.x() / surface_size[0].max(1) as f32,
        visible_size.y() / surface_size[1].max(1) as f32,
    ];
    if pixel_step[0] <= f32::EPSILON || pixel_step[1] <= f32::EPSILON {
        return;
    }
    if let Some(transform) = world.get_mut::<Transform>(camera) {
        transform.position[0] = (transform.position[0] / pixel_step[0]).round() * pixel_step[0];
        transform.position[1] = (transform.position[1] / pixel_step[1]).round() * pixel_step[1];
    }
}

fn main() {
    let mut world = World::new();
    world
        .install(WindowPlugin::new("SkyEngine - Tiled Browser", 960, 720).with_vsync(false))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world
        .install(RenderPlugin::pipeline(
            RenderPipelineAsset::builder()
                .add_feature(TilemapFeature::unlit())
                .add_feature(SpriteFeature::unlit())
                .add_phase(TransparentPhase::new())
                .build(),
        ))
        .unwrap();

    App::new(world).run(TiledBrowserDemo::new());
}

fn map_path_from_args() -> Option<PathBuf> {
    std::env::args_os().nth(1).map(PathBuf::from)
}

fn demo_samples() -> Vec<TiledSample> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("assets")
        .join("tiled");
    let examples = assets.join("tiled").join("examples");
    [
        assets.join("sewers.tmx"),
        examples.join("desert.tmx"),
        examples.join("orthogonal-outside.tmx"),
        examples.join("perspective_walls.tmx"),
        examples.join("isometric_grass_and_water.tmx"),
        examples.join("isometric_staggered_grass_and_water.tmx"),
        examples.join("hexagonal-mini.tmx"),
        examples.join("test_hexagonal_tile_60x60x30.tmx"),
        examples.join(r"forest\forest.tmx"),
    ]
    .into_iter()
    .filter(|path| path.exists())
    .map(TiledSample::from_path)
    .collect()
}

impl TiledSample {
    fn from_path(path: PathBuf) -> Self {
        let label = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("map")
            .replace(['_', '-'], " ");
        Self {
            label,
            path,
            cache: None,
            instance: None,
            load_error: None,
        }
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
