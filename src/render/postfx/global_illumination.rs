//! Unified hybrid global illumination.

use glam::{Mat4 as GlamMat4, Vec3 as GlamVec3};
use wgpu::util::DeviceExt;

use crate::gpu::GpuContext;
use crate::render::component::GlobalIlluminationSettings;
use crate::render::gpu::RenderTarget;
use crate::render::gpu::{FullscreenPass, FullscreenPipeline};
use crate::render::phase::{DrawFunctionRegistry, MeshDrawData, OpaquePhase};
use crate::render::postfx::PostFx;
use crate::render::resources::material::{
    AlphaMode, MaterialRegistry, StandardMaterial, UnlitMaterial,
};
use crate::render::resources::mesh::{BoundingSphere, MeshRegistry};
use crate::render::view::{Color, SceneView};

const GLOBAL_ILLUMINATION_SHADER: &str = include_str!("../shaders/postfx/global_illumination.wgsl");

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlobalIlluminationUniform {
    inverse_projection: [f32; 16],
    inverse_view: [f32; 16],
    viewport: [f32; 4],
    probe_origin_spacing: [f32; 4],
    probe_counts: [u32; 4],
    ambient_color: [f32; 4],
    ground_color: [f32; 4],
    params0: [f32; 4], // intensity, probe_strength, detail_strength, occlusion_strength
    params1: [f32; 4], // detail_radius_px, detail_depth_reject, detail_normal_reject, sky_boost
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ProbeGridSample {
    pub sh0: [f32; 4],
    pub shx: [f32; 4],
    pub shy: [f32; 4],
    pub shz: [f32; 4],
}

#[derive(Debug, Clone)]
pub(crate) struct ViewGiProbeGridPayload {
    pub origin: [f32; 3],
    pub spacing: f32,
    pub counts: [u32; 3],
    pub probes: Vec<ProbeGridSample>,
}

impl ViewGiProbeGridPayload {
    #[inline]
    pub(crate) fn disabled() -> Self {
        Self {
            origin: [0.0, 0.0, 0.0],
            spacing: 1.0,
            counts: [1, 1, 1],
            probes: vec![ProbeGridSample::default()],
        }
    }
}

#[derive(Clone, Copy)]
struct SurfaceProxy {
    center: GlamVec3,
    radius: f32,
    radiance: GlamVec3,
}

pub(crate) fn build_view_gi_probe_grid_payloads(
    views: &[SceneView],
    opaque_phases: &[OpaquePhase],
    draw_functions: &DrawFunctionRegistry,
    model_matrices: &[[f32; 16]],
    lights: &[crate::render::GpuLight],
    material_registry: &MaterialRegistry,
    mesh_registry: &MeshRegistry,
    settings: GlobalIlluminationSettings,
    ambient_color: Color,
) -> Vec<ViewGiProbeGridPayload> {
    views
        .iter()
        .zip(opaque_phases.iter())
        .map(|(view, phase)| {
            if !settings.enabled || view.is_shadow() {
                return ViewGiProbeGridPayload::disabled();
            }

            let counts = [
                settings.probe_volume.counts[0].max(1),
                settings.probe_volume.counts[1].max(1),
                settings.probe_volume.counts[2].max(1),
            ];
            let spacing = settings.probe_volume.spacing.max(0.5);
            let origin = snapped_probe_origin(view.camera_position, counts, spacing);
            let proxies = extract_surface_proxies(
                phase,
                draw_functions,
                model_matrices,
                lights,
                material_registry,
                mesh_registry,
                settings,
                ambient_color,
            );

            let probe_count = counts[0] as usize * counts[1] as usize * counts[2] as usize;
            let mut probes = Vec::with_capacity(probe_count);
            for z in 0..counts[2] {
                for y in 0..counts[1] {
                    for x in 0..counts[0] {
                        let probe_position = GlamVec3::new(
                            origin[0] + x as f32 * spacing,
                            origin[1] + y as f32 * spacing,
                            origin[2] + z as f32 * spacing,
                        );
                        let irradiance = accumulate_probe_lighting(
                            probe_position,
                            lights,
                            &proxies,
                            settings,
                            ambient_color,
                        );
                        probes.push(irradiance);
                    }
                }
            }

            ViewGiProbeGridPayload {
                origin,
                spacing,
                counts,
                probes,
            }
        })
        .collect()
}

fn snapped_probe_origin(camera_position: [f32; 3], counts: [u32; 3], spacing: f32) -> [f32; 3] {
    let camera = GlamVec3::from_array(camera_position);
    let snapped_center = (camera / spacing).round() * spacing;
    let half = GlamVec3::new(
        (counts[0].saturating_sub(1)) as f32 * 0.5 * spacing,
        (counts[1].saturating_sub(1)) as f32 * 0.5 * spacing,
        (counts[2].saturating_sub(1)) as f32 * 0.5 * spacing,
    );
    (snapped_center - half).to_array()
}

fn extract_surface_proxies(
    phase: &OpaquePhase,
    draw_functions: &DrawFunctionRegistry,
    model_matrices: &[[f32; 16]],
    lights: &[crate::render::GpuLight],
    material_registry: &MaterialRegistry,
    mesh_registry: &MeshRegistry,
    settings: GlobalIlluminationSettings,
    ambient_color: Color,
) -> Vec<SurfaceProxy> {
    let mut proxies = Vec::new();
    for item in phase.items() {
        let draw = *item.data::<MeshDrawData>();
        let material_type = draw_functions.material_type_id(item.draw_function_id);
        let Some(mesh) = mesh_registry.get(draw.mesh_handle()) else {
            continue;
        };
        let Some(sub_mesh) = mesh.sub_meshes().get(draw.sub_mesh_index() as usize) else {
            continue;
        };
        let Some(model_matrix) = model_matrices.get(draw.model_slot() as usize).copied() else {
            continue;
        };
        let sphere = transformed_sphere(model_matrix, sub_mesh.bounding_sphere);
        if !sphere.radius.is_finite() || sphere.radius <= 0.001 {
            continue;
        }

        if material_type == Some(std::any::TypeId::of::<StandardMaterial>()) {
            let Some(storage) = material_registry.try_materials::<StandardMaterial>() else {
                continue;
            };
            let Some(material) = storage.get(draw.material_handle::<StandardMaterial>()) else {
                continue;
            };
            if material.alpha_mode != AlphaMode::Opaque {
                continue;
            }
            let radiance = proxy_radiance_from_standard(
                sphere.center,
                lights,
                material,
                settings,
                ambient_color,
            );
            proxies.push(SurfaceProxy {
                center: GlamVec3::from_array(sphere.center),
                radius: sphere.radius.max(0.25),
                radiance,
            });
            continue;
        }

        if material_type == Some(std::any::TypeId::of::<UnlitMaterial>()) {
            let Some(storage) = material_registry.try_materials::<UnlitMaterial>() else {
                continue;
            };
            let Some(material) = storage.get(draw.material_handle::<UnlitMaterial>()) else {
                continue;
            };
            let radiance = proxy_radiance_from_unlit(material, settings);
            proxies.push(SurfaceProxy {
                center: GlamVec3::from_array(sphere.center),
                radius: sphere.radius.max(0.25),
                radiance,
            });
        }
    }
    proxies
}

fn transformed_sphere(matrix: [f32; 16], sphere: BoundingSphere) -> BoundingSphere {
    if !sphere.radius.is_finite() {
        return BoundingSphere::UNBOUNDED;
    }
    let transform = GlamMat4::from_cols_array(&matrix);
    let center = transform.transform_point3(GlamVec3::from_array(sphere.center));
    let scale = transform
        .x_axis
        .truncate()
        .length()
        .max(transform.y_axis.truncate().length())
        .max(transform.z_axis.truncate().length());
    BoundingSphere::new(center.to_array(), sphere.radius * scale)
}

fn proxy_radiance_from_standard(
    center: [f32; 3],
    lights: &[crate::render::GpuLight],
    material: &StandardMaterial,
    settings: GlobalIlluminationSettings,
    ambient_color: Color,
) -> GlamVec3 {
    let albedo = GlamVec3::new(material.albedo.r, material.albedo.g, material.albedo.b);
    let emissive = GlamVec3::new(
        material.emissive.r,
        material.emissive.g,
        material.emissive.b,
    );
    let diffuse_weight = (1.0 - material.metallic.clamp(0.0, 1.0) * 0.85)
        * (0.55 + material.roughness.clamp(0.0, 1.0) * 0.45);
    let incident = estimate_proxy_incident_lighting(center, lights, ambient_color, settings)
        .min(GlamVec3::splat(1.1));
    albedo * incident * settings.probe_volume.bounce_strength * diffuse_weight
        + emissive * (settings.probe_volume.emissive_strength * 0.72)
}

fn proxy_radiance_from_unlit(
    material: &UnlitMaterial,
    settings: GlobalIlluminationSettings,
) -> GlamVec3 {
    GlamVec3::new(material.color.r, material.color.g, material.color.b)
        * (settings.probe_volume.emissive_strength * 0.24)
}

fn estimate_proxy_incident_lighting(
    center: [f32; 3],
    lights: &[crate::render::GpuLight],
    ambient_color: Color,
    settings: GlobalIlluminationSettings,
) -> GlamVec3 {
    let mut incident = GlamVec3::new(ambient_color.r, ambient_color.g, ambient_color.b) * 0.035;
    let center = GlamVec3::from_array(center);
    for light in lights {
        let color = GlamVec3::new(light.color[0], light.color[1], light.color[2]);
        if light.falloff[1] > 0.5 {
            incident += color * settings.probe_volume.light_injection * 0.085;
            continue;
        }

        let position = GlamVec3::new(
            light.pos_radius[0],
            light.pos_radius[1],
            light.pos_radius[2],
        );
        let radius = light.pos_radius[3].max(0.001);
        let distance = position.distance(center);
        let normalized = distance / radius;
        let attenuation = 1.0 / (1.0 + normalized * normalized * light.falloff[0].max(0.5));
        incident += color * attenuation * settings.probe_volume.light_injection * 0.28;
    }
    incident
}

fn luminance(color: GlamVec3) -> f32 {
    color.x * 0.2126 + color.y * 0.7152 + color.z * 0.0722
}

fn accumulate_probe_lighting(
    probe_position: GlamVec3,
    lights: &[crate::render::GpuLight],
    proxies: &[SurfaceProxy],
    settings: GlobalIlluminationSettings,
    ambient_color: Color,
) -> ProbeGridSample {
    let _ = lights;
    let mut sh0 = GlamVec3::new(ambient_color.r, ambient_color.g, ambient_color.b) * 0.018;
    let mut shx = GlamVec3::ZERO;
    let mut shy = GlamVec3::ZERO;
    let mut shz = GlamVec3::ZERO;
    let mut directional_accum = GlamVec3::ZERO;
    let mut directional_energy = 0.0;
    let mut coverage = 0.0;

    for proxy in proxies {
        let to_proxy = proxy.center - probe_position;
        let distance_sq = to_proxy.length_squared().max(0.0001);
        let distance = distance_sq.sqrt();
        let direction = to_proxy / distance;
        let reach = proxy.radius * settings.probe_volume.proxy_radius_scale
            + settings.probe_volume.spacing * 0.95;
        let normalized = distance / reach.max(0.001);
        let locality = (1.0 - normalized).clamp(0.0, 1.0);
        let weight = 1.0 / (1.0 + normalized * normalized * 2.45);
        let contribution = proxy.radiance * weight * (0.32 + locality * 0.68);
        let energy = luminance(contribution).max(0.0);

        sh0 += contribution * 0.88;
        shx += contribution * direction.x * 0.58;
        shy += contribution * direction.y * 0.58;
        shz += contribution * direction.z * 0.58;
        directional_accum += direction * energy;
        directional_energy += energy;
        coverage += weight * (proxy.radius / (distance + proxy.radius.max(0.001)));
    }

    let directionality = if directional_energy > 0.0001 {
        (directional_accum.length() / directional_energy).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let enclosure = (1.0 - (-coverage * 0.9).exp()).clamp(0.0, 1.0);
    let linear_scale = 0.72 + directionality * 0.34;

    ProbeGridSample {
        sh0: [sh0.x.min(1.8), sh0.y.min(1.8), sh0.z.min(1.8), enclosure],
        shx: [
            shx.x * linear_scale,
            shx.y * linear_scale,
            shx.z * linear_scale,
            directionality,
        ],
        shy: [
            shy.x * linear_scale,
            shy.y * linear_scale,
            shy.z * linear_scale,
            0.0,
        ],
        shz: [
            shz.x * linear_scale,
            shz.y * linear_scale,
            shz.z * linear_scale,
            0.0,
        ],
    }
}

pub struct GlobalIllumination {
    pipeline: FullscreenPipeline,
    scene_bgl: wgpu::BindGroupLayout,
    input_bgl: wgpu::BindGroupLayout,
    params_buffer: wgpu::Buffer,
    params_bind_group: wgpu::BindGroup,
    probe_bgl: wgpu::BindGroupLayout,
}

impl GlobalIllumination {
    pub fn new(ctx: &GpuContext, target_format: wgpu::TextureFormat) -> Self {
        let scene_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("global_illumination_scene_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });

        let input_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("global_illumination_input_bgl"),
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

        let params_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("global_illumination_params_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let probe_bgl = ctx
            .device()
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("global_illumination_probe_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let params_buffer = ctx.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("global_illumination_params"),
            size: std::mem::size_of::<GlobalIlluminationUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params_bind_group = ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("global_illumination_params_bg"),
            layout: &params_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            }],
        });

        let pipeline = FullscreenPipeline::new(
            ctx,
            GLOBAL_ILLUMINATION_SHADER,
            "fs_main",
            &[&scene_bgl, &params_bgl, &input_bgl, &probe_bgl],
            target_format,
            None,
            "global_illumination_pipeline",
        );

        Self {
            pipeline,
            scene_bgl,
            input_bgl,
            params_buffer,
            params_bind_group,
            probe_bgl,
        }
    }

    #[inline]
    pub fn resize(&mut self, _ctx: &GpuContext, _width: u32, _height: u32) {}

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_to_target(
        &mut self,
        ctx: &mut GpuContext,
        input: &RenderTarget,
        depth: &RenderTarget,
        normal: &RenderTarget,
        albedo: &RenderTarget,
        material: &RenderTarget,
        emissive: &RenderTarget,
        output: &RenderTarget,
        scene_view: &SceneView,
        settings: GlobalIlluminationSettings,
        payload: &ViewGiProbeGridPayload,
        ambient_color: Color,
    ) {
        let scene_bg = self.create_scene_bg(ctx, depth, normal, albedo, material, emissive);
        let input_bg = self.create_input_bg(ctx, input);
        let probe_bg = self.create_probe_bg(ctx, payload);
        self.write_uniform(ctx, scene_view, settings, payload, ambient_color);

        let color_attachments = [Some(wgpu::RenderPassColorAttachment {
            view: output.view(),
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })];
        let pipeline = self.pipeline.pipeline(ctx, output.format());
        let mut frame = ctx.frame();
        let mut pass = frame.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("global_illumination_pass"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(pipeline.as_ref());
        pass.set_bind_group(0, &scene_bg, &[]);
        pass.set_bind_group(1, &self.params_bind_group, &[]);
        pass.set_bind_group(2, &input_bg, &[]);
        pass.set_bind_group(3, &probe_bg, &[]);
        FullscreenPass::draw(&mut pass);
    }

    fn create_scene_bg(
        &self,
        ctx: &GpuContext,
        depth: &RenderTarget,
        normal: &RenderTarget,
        albedo: &RenderTarget,
        material: &RenderTarget,
        emissive: &RenderTarget,
    ) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("global_illumination_scene_bg"),
            layout: &self.scene_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(depth.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(normal.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(albedo.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(material.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(emissive.view()),
                },
            ],
        })
    }

    fn create_input_bg(&self, ctx: &GpuContext, input: &RenderTarget) -> wgpu::BindGroup {
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("global_illumination_input_bg"),
            layout: &self.input_bgl,
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

    fn create_probe_bg(
        &self,
        ctx: &GpuContext,
        payload: &ViewGiProbeGridPayload,
    ) -> wgpu::BindGroup {
        let probe_buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("global_illumination_probe_buffer"),
                contents: bytemuck::cast_slice(&payload.probes),
                usage: wgpu::BufferUsages::STORAGE,
            });
        ctx.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("global_illumination_probe_bg"),
            layout: &self.probe_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: probe_buffer.as_entire_binding(),
            }],
        })
    }

    fn write_uniform(
        &self,
        ctx: &GpuContext,
        scene_view: &SceneView,
        settings: GlobalIlluminationSettings,
        payload: &ViewGiProbeGridPayload,
        ambient_color: Color,
    ) {
        let inverse_projection = crate::math::Mat4::from_cols_array(scene_view.projection_matrix)
            .inverse()
            .to_cols_array();
        let ground_color = [
            ambient_color.r * 0.42,
            ambient_color.g * 0.38,
            ambient_color.b * 0.34,
            ambient_color.a,
        ];
        let uniform = GlobalIlluminationUniform {
            inverse_projection,
            inverse_view: scene_view.inverse_view,
            viewport: [
                scene_view.target_size[0] as f32,
                scene_view.target_size[1] as f32,
                1.0 / scene_view.target_size[0].max(1) as f32,
                1.0 / scene_view.target_size[1].max(1) as f32,
            ],
            probe_origin_spacing: [
                payload.origin[0],
                payload.origin[1],
                payload.origin[2],
                payload.spacing.max(0.001),
            ],
            probe_counts: [payload.counts[0], payload.counts[1], payload.counts[2], 0],
            ambient_color: ambient_color.to_array(),
            ground_color,
            params0: [
                settings.intensity.max(0.0),
                settings.probe_strength.max(0.0),
                settings.detail_strength.max(0.0),
                settings.occlusion_strength.clamp(0.0, 1.0),
            ],
            params1: [
                settings.detail.radius_px.max(1.0),
                settings.detail.depth_reject.max(0.001),
                settings.detail.normal_reject.max(0.001),
                settings.sky_boost.max(0.0),
            ],
        };
        ctx.queue()
            .write_buffer(&self.params_buffer, 0, bytemuck::bytes_of(&uniform));
    }
}

impl PostFx for GlobalIllumination {
    fn apply_to_target(
        &mut self,
        _ctx: &mut GpuContext,
        _input: &RenderTarget,
        _output: &RenderTarget,
    ) {
        panic!("GlobalIllumination requires scene-view and probe payload context");
    }
}
