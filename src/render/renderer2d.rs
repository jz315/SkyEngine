//! High-level 2D renderer built on top of SkyEngine's lower-level passes.

use crate::ecs::{PreparedQuery, With, World};
use crate::gpu::GpuContext;
use crate::render::core::fullscreen::{FullscreenPass, FullscreenPipeline};
use crate::render::core::target::RenderTarget;
use crate::render::ecs::{
    BloomSettings, PointLight2D, PrimaryCamera2D, RenderSettings2D, Sprite2D, ToneMapSettings,
    Transform2D, VignetteSettings,
};
use crate::render::light::Light2D;
use crate::render::passes::batch::{Sprite, SpriteBatch};
use crate::render::passes::composite_pass::CompositePass;
use crate::render::passes::light_pass::LightPass;
use crate::render::postfx::{bloom::Bloom, tonemap::ToneMap, vignette::Vignette};
use crate::render::scene2d::{Scene2D, SceneLightItem, SceneSpriteItem};
use crate::render::{Camera2D, Color, Texture};

const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const BLOOM_DRAW_CALLS: usize = 16;
const BLIT_SHADER: &str = r#"
@group(0) @binding(0)
var input_tex: texture_2d<f32>;
@group(0) @binding(1)
var input_sampler: sampler;

@fragment
fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {
    return textureSample(input_tex, input_sampler, in.uv);
}
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderPath2D {
    Unlit,
    LitHdr,
}

/// Immutable renderer configuration preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Renderer2DConfig {
    path: RenderPath2D,
}

impl Renderer2DConfig {
    #[inline]
    pub const fn unlit() -> Self {
        Self {
            path: RenderPath2D::Unlit,
        }
    }

    #[inline]
    pub const fn lit_hdr() -> Self {
        Self {
            path: RenderPath2D::LitHdr,
        }
    }

    #[inline]
    fn uses_hdr(self) -> bool {
        matches!(self.path, RenderPath2D::LitHdr)
    }

    fn default_settings(self) -> RenderSettings2D {
        match self.path {
            RenderPath2D::Unlit => RenderSettings2D {
                clear_color: Color::rgb(0.02, 0.02, 0.06),
                ambient_color: Color::BLACK,
                bloom: BloomSettings {
                    enabled: false,
                    ..Default::default()
                },
                tonemap: ToneMapSettings {
                    enabled: false,
                    ..Default::default()
                },
                vignette: VignetteSettings {
                    enabled: false,
                    ..Default::default()
                },
            },
            RenderPath2D::LitHdr => RenderSettings2D::default(),
        }
    }
}

/// Lightweight rendering stats from the most recent frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct RendererStats {
    pub sprite_count: usize,
    pub light_count: usize,
    pub draw_calls: usize,
    pub passes: usize,
}

struct LitPipeline {
    scene_target: RenderTarget,
    light_target: RenderTarget,
    post_a: RenderTarget,
    post_b: RenderTarget,
    light_pass: LightPass,
    composite_pass: CompositePass,
    bloom: Bloom,
    vignette: Vignette,
    tonemap: ToneMap,
    blit: BlitToSurfacePass,
}

impl LitPipeline {
    fn new(ctx: &GpuContext) -> Self {
        let [width, height] = ctx.surface_size();
        Self {
            scene_target: RenderTarget::new(ctx, width, height, HDR_FORMAT, "renderer2d_scene"),
            light_target: RenderTarget::new(ctx, width, height, HDR_FORMAT, "renderer2d_light"),
            post_a: RenderTarget::new(ctx, width, height, HDR_FORMAT, "renderer2d_post_a"),
            post_b: RenderTarget::new(ctx, width, height, HDR_FORMAT, "renderer2d_post_b"),
            light_pass: LightPass::new(ctx, HDR_FORMAT),
            composite_pass: CompositePass::new(ctx, HDR_FORMAT),
            bloom: Bloom::new(ctx, width, height, HDR_FORMAT),
            vignette: Vignette::new(ctx, HDR_FORMAT),
            tonemap: ToneMap::new(ctx, ctx.surface_format()),
            blit: BlitToSurfacePass::new(ctx, ctx.surface_format()),
        }
    }

    fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        self.scene_target.resize(ctx, width, height, HDR_FORMAT);
        self.light_target.resize(ctx, width, height, HDR_FORMAT);
        self.post_a.resize(ctx, width, height, HDR_FORMAT);
        self.post_b.resize(ctx, width, height, HDR_FORMAT);
        self.bloom.resize(ctx, width, height, HDR_FORMAT);
    }
}

struct BlitToSurfacePass {
    pipeline: FullscreenPipeline,
    texture_bgl: wgpu::BindGroupLayout,
}

impl BlitToSurfacePass {
    fn new(ctx: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let texture_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("renderer2d_blit_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        Self {
            pipeline: FullscreenPipeline::new(
                ctx,
                BLIT_SHADER,
                "fs_main",
                &[&texture_bgl],
                target_format,
                None,
                "renderer2d_blit",
            ),
            texture_bgl,
        }
    }

    fn create_bind_group(&self, ctx: &GpuContext, input: &RenderTarget) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("renderer2d_blit_bg"),
            layout: &self.texture_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(ctx.sampler_linear()),
                },
            ],
        })
    }

    fn apply_to_surface(&mut self, ctx: &mut GpuContext, input: &RenderTarget) {
        let bind_group = self.create_bind_group(ctx, input);
        let pipeline = self.pipeline.pipeline(ctx, ctx.surface_format());
        ctx.with_surface_pass("renderer2d_blit", Some(wgpu::Color::BLACK), |pass| {
            pass.set_pipeline(pipeline.as_ref());
            pass.set_bind_group(0, &bind_group, &[]);
            FullscreenPass::draw(pass);
        });
    }

    fn apply_to_target(
        &mut self,
        ctx: &mut GpuContext,
        input: &RenderTarget,
        output: &RenderTarget,
    ) {
        let bind_group = self.create_bind_group(ctx, input);
        let pipeline = self.pipeline.pipeline(ctx, output.format());
        ctx.with_render_pass(
            &wgpu::RenderPassDescriptor {
                label: Some("renderer2d_blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output.view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            },
            |pass| {
                pass.set_pipeline(pipeline.as_ref());
                pass.set_bind_group(0, &bind_group, &[]);
                FullscreenPass::draw(pass);
            },
        );
    }
}

/// High-level ECS-first 2D renderer.
pub struct Renderer2D {
    config: Renderer2DConfig,
    sprite_batch: SpriteBatch,
    lit: Option<LitPipeline>,
    headless_output: Option<RenderTarget>,
    camera_query: PreparedQuery<&'static Camera2D, With<PrimaryCamera2D>>,
    sprite_query: PreparedQuery<(&'static Transform2D, &'static Sprite2D)>,
    light_query: PreparedQuery<(&'static Transform2D, &'static PointLight2D)>,
    scratch_scene: Scene2D,
    scratch_lights: Vec<Light2D>,
    last_stats: RendererStats,
}

impl Renderer2D {
    pub fn new(gpu: &GpuContext, config: Renderer2DConfig) -> Self {
        let lit = config.uses_hdr().then(|| LitPipeline::new(gpu));
        Self {
            config,
            sprite_batch: SpriteBatch::new(gpu),
            lit,
            headless_output: None,
            camera_query: PreparedQuery::new(),
            sprite_query: PreparedQuery::new(),
            light_query: PreparedQuery::new(),
            scratch_scene: Scene2D::new(),
            scratch_lights: Vec::with_capacity(256),
            last_stats: RendererStats::default(),
        }
    }

    pub fn resize(&mut self, gpu: &GpuContext, width: u32, height: u32) {
        if let Some(lit) = self.lit.as_mut() {
            lit.resize(gpu, width, height);
        }
        if let Some(output) = self.headless_output.as_mut() {
            output.resize(gpu, width, height, gpu.surface_format());
        }
    }

    pub fn surface_lost(&mut self) {
        self.headless_output = None;
    }

    pub fn render_world(&mut self, gpu: &mut GpuContext, world: &World) {
        let mut scene = std::mem::take(&mut self.scratch_scene);
        self.extract_world_to_scene(&mut scene, world, gpu.surface_size());
        self.render_scene_internal(gpu, &scene);
        self.scratch_scene = scene;
    }

    pub fn render_scene(&mut self, gpu: &mut GpuContext, scene: &Scene2D) {
        self.render_scene_internal(gpu, scene);
    }

    #[inline]
    pub fn stats(&self) -> RendererStats {
        self.last_stats
    }

    fn extract_world_to_scene(
        &mut self,
        scene: &mut Scene2D,
        world: &World,
        surface_size: [u32; 2],
    ) {
        scene.reset();

        let settings = world
            .get_resource::<RenderSettings2D>()
            .copied()
            .unwrap_or_else(|| self.config.default_settings());
        scene.set_settings(settings);

        let mut selected_camera = None;
        self.camera_query.for_each(world, |camera| {
            if selected_camera.is_none() {
                selected_camera = Some(*camera);
            }
        });
        let mut camera = selected_camera.unwrap_or_else(|| {
            Camera2D::new(surface_size[0].max(1) as f32, surface_size[1].max(1) as f32)
        });
        camera.set_viewport(surface_size[0].max(1) as f32, surface_size[1].max(1) as f32);
        scene.set_camera(camera);

        self.sprite_query.for_each_with_entity(world, |entity, (transform, sprite)| {
            if sprite.visible {
                scene.sprites.push(SceneSpriteItem {
                    transform: *transform,
                    sprite: sprite.clone(),
                    sort_key: ((entity.index() as u64) << 32) | entity.generation() as u64,
                });
            }
        });
        scene.sprites.sort_by(|lhs, rhs| {
            lhs.transform
                .z
                .total_cmp(&rhs.transform.z)
                .then_with(|| texture_sort_key(&lhs.sprite.texture).cmp(&texture_sort_key(&rhs.sprite.texture)))
                .then_with(|| lhs.sort_key.cmp(&rhs.sort_key))
        });

        self.light_query.for_each(world, |(transform, light)| {
            if light.visible {
                scene.lights.push(SceneLightItem {
                    transform: *transform,
                    light: *light,
                });
            }
        });
    }

    fn render_scene_internal(&mut self, gpu: &mut GpuContext, scene: &Scene2D) {
        let [width, height] = gpu.surface_size();
        self.resize(gpu, width, height);
        let has_surface = gpu.has_surface();
        let headless_output = if has_surface {
            None
        } else {
            Some(self.headless_output.get_or_insert_with(|| {
                RenderTarget::new(
                    gpu,
                    width.max(1),
                    height.max(1),
                    gpu.surface_format(),
                    "renderer2d_headless_output",
                )
            }))
        };

        let mut camera = scene
            .camera
            .unwrap_or_else(|| Camera2D::new(width.max(1) as f32, height.max(1) as f32));
        camera.set_viewport(width.max(1) as f32, height.max(1) as f32);

        match self.config.path {
            RenderPath2D::Unlit => {
                let (sprite_count, sprite_draw_calls) =
                    queue_sprites(&mut self.sprite_batch, &scene.sprites);
                if let Some(output) = headless_output {
                    self.sprite_batch.flush_to_target(
                        gpu,
                        &camera,
                        output,
                        Some(scene.settings.clear_color),
                    );
                } else {
                    self.sprite_batch
                        .flush_to_surface(gpu, &camera, Some(scene.settings.clear_color));
                }
                self.last_stats = RendererStats {
                    sprite_count,
                    light_count: 0,
                    draw_calls: sprite_draw_calls,
                    passes: 1,
                };
            }
            RenderPath2D::LitHdr => {
                let sprite_batch = &mut self.sprite_batch;
                let scratch_lights = &mut self.scratch_lights;
                let lit = self.lit.as_mut().expect("lit pipeline should exist");

                apply_settings(lit, scene.settings);

                let (sprite_count, sprite_draw_calls) = queue_sprites(sprite_batch, &scene.sprites);
                sprite_batch.flush_to_target(
                    gpu,
                    &camera,
                    &lit.scene_target,
                    Some(scene.settings.clear_color),
                );

                let light_count = build_lights(scratch_lights, &scene.lights);
                lit.light_pass.render(
                    gpu,
                    scratch_lights,
                    None,
                    &lit.light_target,
                    &camera,
                    scene.settings.ambient_color.to_array(),
                );
                lit.composite_pass.render_to_target(
                    gpu,
                    &lit.scene_target,
                    &lit.light_target,
                    &lit.post_a,
                );

                let mut current_is_a = true;
                let mut passes = 3usize;
                let mut draw_calls =
                    sprite_draw_calls + usize::from(light_count > 0) + 1 /* composite */;

                if scene.settings.bloom.enabled {
                    let (input, output) = if current_is_a {
                        (&lit.post_a, &lit.post_b)
                    } else {
                        (&lit.post_b, &lit.post_a)
                    };
                    lit.bloom.apply(gpu, input, output);
                    current_is_a = !current_is_a;
                    passes += BLOOM_DRAW_CALLS;
                    draw_calls += BLOOM_DRAW_CALLS;
                }

                if scene.settings.vignette.enabled {
                    let (input, output) = if current_is_a {
                        (&lit.post_a, &lit.post_b)
                    } else {
                        (&lit.post_b, &lit.post_a)
                    };
                    lit.vignette.apply_to_target(gpu, input, output);
                    current_is_a = !current_is_a;
                    passes += 1;
                    draw_calls += 1;
                }

                let input = if current_is_a {
                    &lit.post_a
                } else {
                    &lit.post_b
                };
                if let Some(output) = headless_output {
                    if scene.settings.tonemap.enabled {
                        lit.tonemap.apply_to_target(gpu, input, output);
                    } else {
                        lit.blit.apply_to_target(gpu, input, output);
                    }
                } else if scene.settings.tonemap.enabled {
                    lit.tonemap.apply_to_surface(gpu, input);
                } else {
                    lit.blit.apply_to_surface(gpu, input);
                }
                passes += 1;
                draw_calls += 1;

                self.last_stats = RendererStats {
                    sprite_count,
                    light_count,
                    draw_calls,
                    passes,
                };
            }
        }
    }
}

fn texture_sort_key(texture: &Option<Texture>) -> u64 {
    texture
        .as_ref()
        .map(|texture| texture.texture() as *const wgpu::Texture as usize as u64)
        .unwrap_or(0)
}

fn texture_group_key(texture: Option<&Texture>) -> Option<u64> {
    texture.map(|texture| texture.texture() as *const wgpu::Texture as usize as u64)
}

fn apply_settings(lit: &mut LitPipeline, settings: RenderSettings2D) {
    lit.bloom.threshold = settings.bloom.threshold;
    lit.bloom.intensity = settings.bloom.intensity;
    lit.bloom.radius = settings.bloom.radius;

    lit.vignette.intensity = settings.vignette.intensity;
    lit.vignette.smoothness = settings.vignette.smoothness;

    lit.tonemap.exposure = settings.tonemap.exposure;
    lit.tonemap.gamma = settings.tonemap.gamma.max(0.001);
}

fn queue_sprites(batch: &mut SpriteBatch, sprites: &[SceneSpriteItem]) -> (usize, usize) {
    let mut queued = 0usize;
    let mut draw_calls = 0usize;
    let mut active_group: Option<Option<u64>> = None;

    for item in sprites {
        if !item.sprite.visible {
            continue;
        }

        let group = texture_group_key(item.sprite.texture.as_ref());
        if active_group != Some(group) {
            draw_calls += 1;
            if let Some(texture) = item.sprite.texture.as_ref() {
                batch.set_texture(texture);
            } else if active_group.is_some() {
                batch.clear_texture();
            }
            active_group = Some(group);
        }

        batch.draw(
            Sprite::new(
                item.transform.x,
                item.transform.y,
                item.sprite.width * item.transform.scale_x,
                item.sprite.height * item.transform.scale_y,
            )
            .rotation(item.transform.rotation)
            .color(item.sprite.color)
            .uv(
                item.sprite.uv[0],
                item.sprite.uv[1],
                item.sprite.uv[2],
                item.sprite.uv[3],
            ),
        );
        queued += 1;
    }

    (queued, draw_calls)
}

fn build_lights(out: &mut Vec<Light2D>, lights: &[SceneLightItem]) -> usize {
    out.clear();
    for item in lights {
        if item.light.visible {
            out.push(
                Light2D::new(item.transform.x, item.transform.y, item.light.radius)
                    .intensity(item.light.intensity)
                    .color(item.light.color)
                    .temperature(item.light.temperature)
                    .falloff(item.light.falloff),
            );
        }
    }
    out.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_device() -> (wgpu::Device, wgpu::Queue) {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("No suitable GPU adapter found for renderer tests");

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("renderer2d_test_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("Failed to create test GPU device")
    }

    #[test]
    fn renderer_lifecycle_methods_work_headless() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [64, 64]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::lit_hdr());

        renderer.resize(&ctx, 128, 72);
        renderer.surface_lost();

        let scene = Scene2D::new();
        ctx.begin_frame().expect("headless begin_frame should succeed");
        renderer.render_scene(&mut ctx, &scene);
        ctx.end_frame();
    }

    #[test]
    fn render_world_succeeds_with_primary_camera_and_sprite() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());

        let mut world = World::new();
        world.spawn((Camera2D::new(32.0, 32.0), PrimaryCamera2D));
        world.spawn((Transform2D::default(), Sprite2D::new(8.0, 8.0)));

        ctx.begin_frame().expect("headless begin_frame should succeed");
        renderer.render_world(&mut ctx, &world);
        ctx.end_frame();

        assert_eq!(renderer.stats().sprite_count, 1);
        assert_eq!(renderer.stats().passes, 1);
    }

    #[test]
    fn fallback_camera_is_created_when_world_has_none() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [48, 24]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let mut world = World::new();
        let mut scene = Scene2D::new();

        world.spawn((Transform2D::new(1.0, 2.0), Sprite2D::new(4.0, 4.0)));
        renderer.extract_world_to_scene(&mut scene, &world, [48, 24]);

        let camera = scene.camera.expect("fallback camera should be present");
        assert_eq!(camera.viewport_width(), 48.0);
        assert_eq!(camera.viewport_height(), 24.0);
    }

    #[test]
    fn point_lights_extract_from_transform_and_component() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::lit_hdr());
        let mut world = World::new();
        let mut scene = Scene2D::new();

        world.spawn((
            Transform2D::new(12.0, 18.0),
            PointLight2D::new(42.0).intensity(2.5).temperature(3000.0),
        ));

        renderer.extract_world_to_scene(&mut scene, &world, [32, 32]);
        assert_eq!(scene.lights.len(), 1);
        assert_eq!(scene.lights[0].transform.x, 12.0);
        assert_eq!(scene.lights[0].transform.y, 18.0);
        assert_eq!(scene.lights[0].light.radius, 42.0);
        assert_eq!(scene.lights[0].light.intensity, 2.5);
        assert_eq!(scene.lights[0].light.temperature, 3000.0);
    }

    #[test]
    fn render_settings_resource_overrides_defaults() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::lit_hdr());
        let mut world = World::new();
        let mut scene = Scene2D::new();
        let custom = RenderSettings2D {
            clear_color: Color::RED,
            ..RenderSettings2D::default()
        };

        world.insert_resource(custom);
        renderer.extract_world_to_scene(&mut scene, &world, [32, 32]);

        assert_eq!(scene.settings.clear_color.r, Color::RED.r);
        assert_eq!(scene.settings.clear_color.g, Color::RED.g);
    }

    #[test]
    fn unlit_renderer_ignores_hdr_resources() {
        let (device, queue) = create_test_device();
        let mut ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let mut scene = Scene2D::new();

        scene
            .set_settings(RenderSettings2D::default())
            .add_light(Transform2D::new(0.0, 0.0), PointLight2D::new(32.0));

        assert!(renderer.lit.is_none());

        ctx.begin_frame().expect("headless begin_frame should succeed");
        renderer.render_scene(&mut ctx, &scene);
        ctx.end_frame();
    }

    #[test]
    fn extracted_sprites_are_sorted_by_z_then_texture_group() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::unlit());
        let mut world = World::new();
        let mut scene = Scene2D::new();
        let tex_a = Texture::circle(&ctx, 8);
        let tex_b = Texture::checkerboard(&ctx, 8, 2, [255, 255, 255, 255], [0, 0, 0, 255]);

        world.spawn((
            Transform2D::from_xyz(0.0, 0.0, 2.0),
            Sprite2D::new(4.0, 4.0).texture(tex_a.clone()),
        ));
        world.spawn((
            Transform2D::from_xyz(0.0, 0.0, 1.0),
            Sprite2D::new(4.0, 4.0).texture(tex_b.clone()),
        ));
        world.spawn((
            Transform2D::from_xyz(0.0, 0.0, 1.0),
            Sprite2D::new(4.0, 4.0).texture(tex_b.clone()),
        ));

        renderer.extract_world_to_scene(&mut scene, &world, [32, 32]);

        assert_eq!(scene.sprites[0].transform.z, 1.0);
        assert_eq!(scene.sprites[1].transform.z, 1.0);
        assert_eq!(scene.sprites[2].transform.z, 2.0);
        assert!(scene.sprites[0]
            .sprite
            .texture
            .as_ref()
            .unwrap()
            .ptr_eq(scene.sprites[1].sprite.texture.as_ref().unwrap()));
    }

    #[test]
    fn hidden_sprites_and_lights_are_skipped() {
        let (device, queue) = create_test_device();
        let ctx =
            GpuContext::new_headless(device, queue, wgpu::TextureFormat::Bgra8Unorm, [32, 32]);
        let mut renderer = Renderer2D::new(&ctx, Renderer2DConfig::lit_hdr());
        let mut world = World::new();
        let mut scene = Scene2D::new();

        world.spawn((
            Transform2D::default(),
            Sprite2D::new(4.0, 4.0).visible(false),
            PointLight2D::new(32.0).visible(false),
        ));

        renderer.extract_world_to_scene(&mut scene, &world, [32, 32]);
        assert!(scene.sprites.is_empty());
        assert!(scene.lights.is_empty());
    }

    #[test]
    fn scene2d_reset_prevents_stale_draws() {
        let mut scene = Scene2D::new();
        scene.add_sprite(Transform2D::default(), Sprite2D::new(4.0, 4.0));
        scene.add_light(Transform2D::default(), PointLight2D::new(32.0));

        scene.reset();

        assert_eq!(scene.sprite_count(), 0);
        assert_eq!(scene.light_count(), 0);
        assert!(scene.camera().is_none());
    }
}
