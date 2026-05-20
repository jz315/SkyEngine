//! Custom `RenderFeature` demo.
//!
//! Shows how to extend SkyEngine's high-level pipeline without changing
//! engine internals. The feature defined in this example injects a warm
//! fullscreen tint as a custom post-processing step.
//!
//! ```bash
//! cargo run --example custom_feature_demo --features app --release
//! ```

use sky_engine::app::{
    App, AppState, AssetPlugin, FrameContext, InputPlugin, RenderPlugin, WindowPlugin,
};
use sky_engine::ecs::World;
use sky_engine::render::expert::{
    CompiledPass, FullscreenPass, FullscreenPipeline, PhysicalResources, TargetSize, TextureHandle,
};
use sky_engine::render::{
    CameraMarker, Color, MainCamera, PostFxPass, PostFxPassExecuteContext, PostFxPassSetupContext,
    Projection, RenderFeature, RenderPipelineAsset, RenderPipelineBuilder, SpriteFeature,
    SpriteRenderer, Transform, TransparentPhase,
};

const NUM_SPRITES: usize = 1200;

#[derive(Clone, Copy)]
struct Velocity {
    x: f32,
    y: f32,
}

struct CustomFeatureDemo {
    fps_smooth: f32,
    frame_count: u32,
}

impl AppState for CustomFeatureDemo {
    fn update(&mut self, ctx: &mut FrameContext) {
        let dt = ctx.dt;
        let [w, h] = ctx.surface_size();
        let mut query = ctx
            .world
            .query::<(&mut Transform, &SpriteRenderer, &Velocity)>();
        query.for_each(ctx.world, |(transform, sprite, velocity)| {
            transform.position[0] += velocity.x * dt;
            transform.position[1] += velocity.y * dt;
            transform.rotate_z(0.35 * dt);

            let hw = w as f32 * 0.5 + sprite.width;
            let hh = h as f32 * 0.5 + sprite.height;
            if transform.position[0] > hw {
                transform.position[0] = -hw;
            }
            if transform.position[0] < -hw {
                transform.position[0] = hw;
            }
            if transform.position[1] > hh {
                transform.position[1] = -hh;
            }
            if transform.position[1] < -hh {
                transform.position[1] = hh;
            }
        });

        ctx.render();

        let fps_instant = if dt > 0.0 { 1.0 / dt } else { 0.0 };
        self.fps_smooth = if self.fps_smooth == 0.0 {
            fps_instant
        } else {
            self.fps_smooth * 0.92 + fps_instant * 0.08
        };
        self.frame_count += 1;
        if self.frame_count % 30 == 0 {
            let stats = ctx.render_stats();
            ctx.set_title(&format!(
                "SkyEngine — Custom Feature Demo | {:.0} FPS | {} draws | {} sprites",
                self.fps_smooth, stats.draw_calls, stats.sprite_count
            ));
        }
    }
}

#[derive(Default)]
struct WarmTintFeature;

impl RenderFeature for WarmTintFeature {
    fn name(&self) -> &'static str {
        "warm_tint"
    }

    fn register(&mut self, builder: &mut RenderPipelineBuilder) {
        let current = std::mem::take(builder);
        *builder = current.add_postfx(WarmTintPass::default());
    }
}

#[derive(Default)]
struct WarmTintPass {
    runtime: Option<WarmTintRuntime>,
}

struct WarmTintRuntime {
    texture_bgl: wgpu::BindGroupLayout,
    pipeline: FullscreenPipeline,
}

impl WarmTintRuntime {
    fn new(gpu: &sky_engine::gpu::GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let texture_bgl = gpu
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("custom_feature_tint_bgl"),
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
        let pipeline = FullscreenPipeline::new(
            gpu,
            r#"
@group(0) @binding(0) var input_tex: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;

@fragment
fn fs_main(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let color = textureSample(input_tex, input_sampler, in.uv);
    let stripe = 0.92 + 0.08 * sin(in.uv.y * 180.0);
    let tint = vec3<f32>(1.10, 0.97, 0.88);
    return vec4<f32>(color.rgb * tint * stripe, color.a);
}
"#,
            "fs_main",
            &[&texture_bgl],
            target_format,
            None,
            "custom_feature_tint",
        );
        Self {
            texture_bgl,
            pipeline,
        }
    }
}

impl PostFxPass for WarmTintPass {
    fn name(&self) -> &'static str {
        "warm_tint"
    }

    fn setup(&mut self, ctx: &mut PostFxPassSetupContext<'_, '_>) {
        let input = ctx
            .state()
            .current_color()
            .expect("warm_tint requires a current color input");
        let target_size = ctx.view().target_size();
        let output = ctx.graph().create_texture(|builder| {
            builder
                .name("warm_tint_out")
                .size(TargetSize::Exact(target_size[0], target_size[1]))
                .format(input.format());
        });
        ctx.graph().add_render_pass(self.name(), |setup| {
            setup.read(input.handle());
            setup.write_color(0, output);
        });
        ctx.state().set_current_color(output, input.format());
    }

    fn execute(
        &mut self,
        ctx: &mut PostFxPassExecuteContext<'_, '_>,
    ) -> Result<(), sky_engine::render::expert::RenderGraphError> {
        let (gpu, pass, resources, _execution) = ctx.split();
        let input = first_read_texture(pass, self.name());
        let output = first_write_texture(pass, self.name());
        let input_rt = require_render_target(resources, input, self.name(), "input");
        let output_rt = require_render_target(resources, output, self.name(), "output");

        let runtime = self
            .runtime
            .get_or_insert_with(|| WarmTintRuntime::new(gpu, output_rt.format()));
        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("custom_feature_tint_bg"),
            layout: &runtime.texture_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input_rt.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(gpu.sampler_linear()),
                },
            ],
        });
        let pipeline = runtime.pipeline.pipeline(gpu, output_rt.format());
        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output_rt.view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut frame = gpu.frame();
        let mut render_pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(self.name()),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        render_pass.set_pipeline(pipeline.as_ref());
        render_pass.set_bind_group(0, &bind_group, &[]);
        FullscreenPass::draw(&mut render_pass);
        Ok(())
    }

    fn draw_calls(
        &self,
        _execution: &sky_engine::render::expert::ViewExecutionContext<'_>,
    ) -> usize {
        1
    }
}

fn first_read_texture(pass: &CompiledPass, node_name: &str) -> TextureHandle {
    pass.reads
        .iter()
        .find_map(|resource| match resource {
            sky_engine::render::expert::ResourceRef::Texture(handle) => Some(*handle),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{node_name} should read an input texture"))
}

fn first_write_texture(pass: &CompiledPass, node_name: &str) -> TextureHandle {
    pass.writes
        .iter()
        .find_map(|resource| match resource {
            sky_engine::render::expert::ResourceRef::Texture(handle) => Some(*handle),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{node_name} should write an output texture"))
}

fn require_render_target<'a>(
    resources: &'a PhysicalResources<'a>,
    handle: TextureHandle,
    node_name: &str,
    label: &str,
) -> &'a sky_engine::render::expert::RenderTarget {
    resources
        .render_target(handle)
        .unwrap_or_else(|| panic!("{node_name} {label} target should be allocated"))
}

fn main() {
    let mut rng = SimpleRng::new(7);
    let mut world = World::new();
    world.spawn((
        Transform::default(),
        CameraMarker::new(),
        Projection::orthographic(640.0),
        MainCamera,
    ));

    for _ in 0..NUM_SPRITES {
        let size = rng.range(10.0, 28.0);
        let hue = rng.range(0.0, 360.0);
        world.spawn((
            Transform::from_xy(rng.range(-480.0, 480.0), rng.range(-320.0, 320.0)),
            SpriteRenderer::new(size, size).color(Color::hsl(hue, 0.85, 0.60)),
            Velocity {
                x: rng.range(-80.0, 80.0),
                y: rng.range(-80.0, 80.0),
            },
        ));
    }

    let pipeline = RenderPipelineAsset::builder()
        .add_feature(SpriteFeature::unlit())
        .add_phase(TransparentPhase::new())
        .add_feature(WarmTintFeature)
        .build();

    world
        .install(WindowPlugin::new(
            "SkyEngine — Custom Feature Demo",
            960,
            640,
        ))
        .unwrap();
    world.install(InputPlugin).unwrap();
    world.install(AssetPlugin::default()).unwrap();
    world.install(RenderPlugin::pipeline(pipeline)).unwrap();

    App::new(world).run(CustomFeatureDemo {
        fps_smooth: 0.0,
        frame_count: 0,
    });
}

struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E3779B97F4A7C15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
}
