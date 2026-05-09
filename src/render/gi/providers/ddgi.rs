//! Dynamic diffuse global illumination provider built around probe volumes.

use std::borrow::Cow;

use crate::gpu::GpuContext;
use crate::math::{Mat4, Vec3};
use crate::render::component::GlobalIllumination;
use crate::render::gi::{
    downcast_settings, GiMaterial, GiProviderFactory, GiProviderId, GiProviderRuntime,
    GiSamplingBinding, GiSceneInput, GiSettings, GiShaderDescriptor, GiUpdateDescriptor,
};
use crate::render::gpu::ComputePipelineCache;
use crate::render::gpu::{RenderTarget, RenderTargetDescriptor};
use crate::render::graph::{PassFlags, RenderGraphError};
use crate::render::pipeline::ComputePassExecuteContext;
use crate::render::resources::mesh::RayTriangle;
use crate::render::view::{Color, SceneView};
use crate::render::GpuLight;

pub const DDGI_PROVIDER_ID: GiProviderId = "sky.ddgi";

pub(crate) const DDGI_IRRADIANCE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(crate) const DDGI_VISIBILITY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(crate) const DDGI_WORKGROUP_SIZE: u32 = 8;
pub(crate) const DDGI_ATLAS_BORDER_TEXELS: u32 = 1;
pub(crate) const DDGI_SHADER: &str = include_str!("../../shaders/gi/ddgi_update.wgsl");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DdgiDebugMode {
    #[default]
    Off,
    Probes,
    Irradiance,
    Visibility,
    RayBudget,
}

#[derive(Clone, Copy, Debug)]
pub struct DdgiVolumeSettings {
    pub origin: [f32; 3],
    pub spacing: f32,
    pub counts: [u32; 3],
    pub scroll_with_main_camera: bool,
}

impl Default for DdgiVolumeSettings {
    fn default() -> Self {
        Self {
            origin: [-14.0, -4.0, -14.0],
            spacing: 1.85,
            counts: [16, 8, 16],
            scroll_with_main_camera: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DdgiSettings {
    pub volume: DdgiVolumeSettings,
    pub rays_per_probe: u32,
    pub probes_per_frame: u32,
    pub hysteresis: f32,
    pub normal_bias: f32,
    pub view_bias: f32,
    pub max_ray_distance: f32,
    pub irradiance_resolution: u32,
    pub visibility_resolution: u32,
    pub bounces: u32,
    pub debug: DdgiDebugMode,
}

impl Default for DdgiSettings {
    fn default() -> Self {
        Self {
            volume: DdgiVolumeSettings::default(),
            rays_per_probe: 64,
            probes_per_frame: 128,
            hysteresis: 0.92,
            normal_bias: 0.08,
            view_bias: 0.20,
            max_ray_distance: 40.0,
            irradiance_resolution: 6,
            visibility_resolution: 6,
            bounces: 2,
            debug: DdgiDebugMode::Off,
        }
    }
}

#[inline]
pub fn global_illumination(settings: DdgiSettings) -> GlobalIllumination {
    GlobalIllumination::provider(crate::render::gi::GiProviderConfig::new(
        DDGI_PROVIDER_ID,
        settings,
    ))
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct DdgiUniform {
    pub(crate) origin_spacing: [f32; 4],
    pub(crate) counts_enabled: [u32; 4],
    pub(crate) irradiance_atlas_params: [u32; 4],
    pub(crate) visibility_atlas_params: [u32; 4],
    pub(crate) trace_params: [f32; 4],
    pub(crate) frame_params: [u32; 4],
    pub(crate) ambient: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct DdgiLightMeta {
    pub(crate) count: u32,
    _pad: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuGiTriangle {
    p0: [f32; 4],
    p1: [f32; 4],
    p2: [f32; 4],
    normal_emissive: [f32; 4],
    albedo: [f32; 4],
}

impl GpuGiTriangle {
    fn from_triangle(triangle: RayTriangle, transform: Mat4, material: GiMaterial) -> Self {
        let p0 = transform.transform_point3(Vec3::from_array(triangle.positions[0]));
        let p1 = transform.transform_point3(Vec3::from_array(triangle.positions[1]));
        let p2 = transform.transform_point3(Vec3::from_array(triangle.positions[2]));
        let normal = (p1 - p0)
            .cross(p2 - p0)
            .try_normalized()
            .unwrap_or(Vec3::new(0.0, 1.0, 0.0));
        Self {
            p0: [p0.x(), p0.y(), p0.z(), 0.0],
            p1: [p1.x(), p1.y(), p1.z(), 0.0],
            p2: [p2.x(), p2.y(), p2.z(), 0.0],
            normal_emissive: [
                normal.x(),
                normal.y(),
                normal.z(),
                luminance(material.emissive),
            ],
            albedo: [
                material.albedo.r,
                material.albedo.g,
                material.albedo.b,
                material.metallic.clamp(0.0, 1.0),
            ],
        }
    }

    fn bounds(&self) -> GiBounds {
        let mut bounds = GiBounds::empty();
        bounds.include_point([self.p0[0], self.p0[1], self.p0[2]]);
        bounds.include_point([self.p1[0], self.p1[1], self.p1[2]]);
        bounds.include_point([self.p2[0], self.p2[1], self.p2[2]]);
        bounds
    }

    fn centroid(&self) -> [f32; 3] {
        [
            (self.p0[0] + self.p1[0] + self.p2[0]) / 3.0,
            (self.p0[1] + self.p1[1] + self.p2[1]) / 3.0,
            (self.p0[2] + self.p1[2] + self.p2[2]) / 3.0,
        ]
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuGiBvhNode {
    bounds_min: [f32; 4],
    bounds_max: [f32; 4],
    meta: [u32; 4],
}

impl GpuGiBvhNode {
    fn leaf(bounds: GiBounds, triangle_index: u32) -> Self {
        Self {
            bounds_min: [bounds.min[0], bounds.min[1], bounds.min[2], 0.0],
            bounds_max: [bounds.max[0], bounds.max[1], bounds.max[2], 0.0],
            meta: [0, 0, triangle_index, 1],
        }
    }

    fn inner(bounds: GiBounds, left: u32, right: u32) -> Self {
        Self {
            bounds_min: [bounds.min[0], bounds.min[1], bounds.min[2], 0.0],
            bounds_max: [bounds.max[0], bounds.max[1], bounds.max[2], 0.0],
            meta: [left, right, 0, 0],
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct GiBounds {
    min: [f32; 3],
    max: [f32; 3],
}

impl GiBounds {
    fn empty() -> Self {
        Self {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        }
    }

    fn include_point(&mut self, point: [f32; 3]) {
        for (axis, value) in point.into_iter().enumerate() {
            self.min[axis] = self.min[axis].min(value);
            self.max[axis] = self.max[axis].max(value);
        }
    }

    fn include_bounds(&mut self, bounds: GiBounds) {
        for axis in 0..3 {
            self.min[axis] = self.min[axis].min(bounds.min[axis]);
            self.max[axis] = self.max[axis].max(bounds.max[axis]);
        }
    }

    fn extent(&self) -> [f32; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
}

fn build_gpu_bvh(triangles: &[GpuGiTriangle]) -> Vec<GpuGiBvhNode> {
    if triangles.is_empty() {
        return Vec::new();
    }
    let capped_len = triangles.len().min(u32::MAX as usize);
    let mut indices = (0..capped_len)
        .map(|index| index as u32)
        .collect::<Vec<_>>();
    let mut nodes = Vec::with_capacity(capped_len.saturating_mul(2).saturating_sub(1));
    build_gpu_bvh_node(&mut nodes, triangles, &mut indices);
    nodes
}

fn build_gpu_bvh_node(
    nodes: &mut Vec<GpuGiBvhNode>,
    triangles: &[GpuGiTriangle],
    indices: &mut [u32],
) -> u32 {
    let node_index = nodes.len() as u32;
    nodes.push(GpuGiBvhNode::default());

    let mut bounds = GiBounds::empty();
    let mut centroid_bounds = GiBounds::empty();
    for index in indices.iter().copied() {
        let triangle = &triangles[index as usize];
        bounds.include_bounds(triangle.bounds());
        centroid_bounds.include_point(triangle.centroid());
    }

    if indices.len() == 1 {
        nodes[node_index as usize] = GpuGiBvhNode::leaf(bounds, indices[0]);
        return node_index;
    }

    let extent = centroid_bounds.extent();
    let split_axis = if extent[0] >= extent[1] && extent[0] >= extent[2] {
        0
    } else if extent[1] >= extent[2] {
        1
    } else {
        2
    };
    indices.sort_by(|left, right| {
        let left_centroid = triangles[*left as usize].centroid()[split_axis];
        let right_centroid = triangles[*right as usize].centroid()[split_axis];
        left_centroid.total_cmp(&right_centroid)
    });
    let mid = (indices.len() / 2).max(1);
    let (left_indices, right_indices) = indices.split_at_mut(mid);
    let left = build_gpu_bvh_node(nodes, triangles, left_indices);
    let right = build_gpu_bvh_node(nodes, triangles, right_indices);
    nodes[node_index as usize] = GpuGiBvhNode::inner(bounds, left, right);
    node_index
}

#[inline]
fn luminance(color: Color) -> f32 {
    color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722
}

pub struct DdgiProviderFactory;

impl GiProviderFactory for DdgiProviderFactory {
    fn id(&self) -> GiProviderId {
        DDGI_PROVIDER_ID
    }

    fn create(&self, gpu: &GpuContext) -> Box<dyn GiProviderRuntime> {
        Box::new(DdgiRuntime::new(gpu))
    }
}

pub(crate) struct DdgiRuntime {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    sampling_bind_group_layout: wgpu::BindGroupLayout,
    sampling_bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    triangle_buffer: wgpu::Buffer,
    node_buffer: wgpu::Buffer,
    light_buffer: wgpu::Buffer,
    light_meta_buffer: wgpu::Buffer,
    irradiance_a: RenderTarget,
    irradiance_b: RenderTarget,
    visibility_a: RenderTarget,
    visibility_b: RenderTarget,
    sampler: wgpu::Sampler,
    triangle_capacity: usize,
    node_capacity: usize,
    light_capacity: usize,
    current_is_a: bool,
    irradiance_atlas_width: u32,
    irradiance_atlas_height: u32,
    visibility_atlas_width: u32,
    visibility_atlas_height: u32,
    frame_index: u32,
    force_full_update: bool,
    history_valid: bool,
    history_origin: [f32; 3],
    history_spacing: f32,
    history_counts: [u32; 3],
    history_irradiance_resolution: u32,
    history_visibility_resolution: u32,
    last_uniform: DdgiUniform,
    update_pipeline: Option<ComputePipelineCache>,
}

impl DdgiRuntime {
    pub(crate) fn new(gpu: &GpuContext) -> Self {
        let bind_group_layout = create_ddgi_update_bind_group_layout(gpu.device());
        let sampling_bind_group_layout = create_ddgi_sampling_bind_group_layout(gpu.device());
        let uniform_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddgi_uniform"),
            size: std::mem::size_of::<DdgiUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let triangle_buffer = create_storage_buffer(
            gpu,
            "ddgi_triangles",
            1,
            std::mem::size_of::<GpuGiTriangle>(),
        );
        let node_buffer = create_storage_buffer(
            gpu,
            "ddgi_bvh_nodes",
            1,
            std::mem::size_of::<GpuGiBvhNode>(),
        );
        let light_buffer =
            create_storage_buffer(gpu, "ddgi_lights", 1, std::mem::size_of::<GpuLight>());
        let light_meta_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("ddgi_light_meta"),
            size: std::mem::size_of::<DdgiLightMeta>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let irradiance_a =
            create_ddgi_texture(gpu, 1, 1, DDGI_IRRADIANCE_FORMAT, "ddgi_irradiance_a");
        let irradiance_b =
            create_ddgi_texture(gpu, 1, 1, DDGI_IRRADIANCE_FORMAT, "ddgi_irradiance_b");
        let visibility_a =
            create_ddgi_texture(gpu, 1, 1, DDGI_VISIBILITY_FORMAT, "ddgi_visibility_a");
        let visibility_b =
            create_ddgi_texture(gpu, 1, 1, DDGI_VISIBILITY_FORMAT, "ddgi_visibility_b");
        let sampler = gpu.device().create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ddgi_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let last_uniform = disabled_uniform();
        gpu.queue()
            .write_buffer(&uniform_buffer, 0, bytemuck::bytes_of(&last_uniform));
        gpu.queue().write_buffer(
            &light_meta_buffer,
            0,
            bytemuck::bytes_of(&DdgiLightMeta::default()),
        );
        let bind_group = create_ddgi_update_bind_group(
            gpu.device(),
            &bind_group_layout,
            &uniform_buffer,
            &triangle_buffer,
            &node_buffer,
            &light_buffer,
            &light_meta_buffer,
            irradiance_b.view(),
            irradiance_a.view(),
            visibility_b.view(),
            visibility_a.view(),
            &sampler,
        );
        let sampling_bind_group = create_ddgi_sampling_bind_group(
            gpu.device(),
            &sampling_bind_group_layout,
            &uniform_buffer,
            irradiance_a.view(),
            visibility_a.view(),
            &sampler,
        );
        Self {
            bind_group_layout,
            bind_group,
            sampling_bind_group_layout,
            sampling_bind_group,
            uniform_buffer,
            triangle_buffer,
            node_buffer,
            light_buffer,
            light_meta_buffer,
            irradiance_a,
            irradiance_b,
            visibility_a,
            visibility_b,
            sampler,
            triangle_capacity: 1,
            node_capacity: 1,
            light_capacity: 1,
            current_is_a: true,
            irradiance_atlas_width: 1,
            irradiance_atlas_height: 1,
            visibility_atlas_width: 1,
            visibility_atlas_height: 1,
            frame_index: 0,
            force_full_update: true,
            history_valid: false,
            history_origin: [0.0; 3],
            history_spacing: 1.0,
            history_counts: [1; 3],
            history_irradiance_resolution: 1,
            history_visibility_resolution: 1,
            last_uniform,
            update_pipeline: None,
        }
    }

    pub(crate) fn prepare(
        &mut self,
        gpu: &GpuContext,
        ddgi: DdgiSettings,
        scene: &GiSceneInput<'_>,
    ) {
        let Some(scene_view) = scene.primary_view else {
            self.write_disabled(gpu);
            return;
        };
        let counts = ddgi.volume.counts.map(|value| value.max(1));
        let irradiance_res = ddgi.irradiance_resolution.clamp(2, 16);
        let visibility_res = ddgi.visibility_resolution.clamp(2, 16);
        let (irradiance_atlas_width, irradiance_atlas_height) =
            ddgi_atlas_size(counts, irradiance_res);
        let (visibility_atlas_width, visibility_atlas_height) =
            ddgi_atlas_size(counts, visibility_res);
        self.resize_atlas_if_needed(
            gpu,
            irradiance_atlas_width,
            irradiance_atlas_height,
            visibility_atlas_width,
            visibility_atlas_height,
        );
        let origin = ddgi_origin(ddgi, scene_view);
        let spacing = ddgi.volume.spacing.max(0.05);
        self.invalidate_history_if_volume_changed(
            origin,
            spacing,
            counts,
            irradiance_res,
            visibility_res,
        );
        self.current_is_a = !self.current_is_a;

        let triangles = collect_gi_triangles(scene.renderables);
        let bvh_nodes = build_gpu_bvh(&triangles);
        self.ensure_triangle_capacity(gpu, triangles.len().max(1));
        if !triangles.is_empty() {
            gpu.queue()
                .write_buffer(&self.triangle_buffer, 0, bytemuck::cast_slice(&triangles));
        }
        self.ensure_node_capacity(gpu, bvh_nodes.len().max(1));
        if !bvh_nodes.is_empty() {
            gpu.queue()
                .write_buffer(&self.node_buffer, 0, bytemuck::cast_slice(&bvh_nodes));
        }
        self.ensure_light_capacity(gpu, scene.lights.len().max(1));
        if !scene.lights.is_empty() {
            gpu.queue()
                .write_buffer(&self.light_buffer, 0, bytemuck::cast_slice(scene.lights));
        }
        gpu.queue().write_buffer(
            &self.light_meta_buffer,
            0,
            bytemuck::bytes_of(&DdgiLightMeta {
                count: scene.lights.len() as u32,
                _pad: [0; 3],
            }),
        );

        let debug_mode = match ddgi.debug {
            DdgiDebugMode::Off => 0,
            DdgiDebugMode::Probes => 1,
            DdgiDebugMode::Irradiance => 2,
            DdgiDebugMode::Visibility => 3,
            DdgiDebugMode::RayBudget => 4,
        };
        let probe_count = counts[0]
            .saturating_mul(counts[1])
            .saturating_mul(counts[2])
            .max(1);
        let probes_per_frame = if self.force_full_update {
            probe_count
        } else {
            ddgi.probes_per_frame.max(1)
        };
        self.last_uniform = DdgiUniform {
            origin_spacing: [origin[0], origin[1], origin[2], spacing],
            counts_enabled: [counts[0], counts[1], counts[2], 1],
            irradiance_atlas_params: [
                irradiance_atlas_width,
                irradiance_atlas_height,
                irradiance_res,
                bvh_nodes.len() as u32,
            ],
            visibility_atlas_params: [
                visibility_atlas_width,
                visibility_atlas_height,
                visibility_res,
                0,
            ],
            trace_params: [
                ddgi.max_ray_distance.max(0.1),
                ddgi.hysteresis.clamp(0.0, 0.99),
                ddgi.normal_bias.max(0.0),
                ddgi.view_bias.max(0.0),
            ],
            frame_params: [
                self.frame_index,
                ddgi.rays_per_probe.max(1),
                probes_per_frame,
                ddgi.bounces.max(1).min(4) | (debug_mode << 16),
            ],
            ambient: scene.ambient_color.to_array(),
        };
        self.frame_index = self.frame_index.wrapping_add(1);
        self.force_full_update = false;
        gpu.queue().write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&self.last_uniform),
        );
        let (write_target, read_target) = if self.current_is_a {
            (&self.irradiance_a, &self.irradiance_b)
        } else {
            (&self.irradiance_b, &self.irradiance_a)
        };
        let (write_visibility, read_visibility) = if self.current_is_a {
            (&self.visibility_a, &self.visibility_b)
        } else {
            (&self.visibility_b, &self.visibility_a)
        };
        self.bind_group = create_ddgi_update_bind_group(
            gpu.device(),
            &self.bind_group_layout,
            &self.uniform_buffer,
            &self.triangle_buffer,
            &self.node_buffer,
            &self.light_buffer,
            &self.light_meta_buffer,
            write_target.view(),
            read_target.view(),
            write_visibility.view(),
            read_visibility.view(),
            &self.sampler,
        );
        self.refresh_sampling_bind_group(gpu);
    }

    #[inline]
    pub(crate) fn dispatch_size(&self) -> (u32, u32) {
        (
            self.irradiance_atlas_width.max(self.visibility_atlas_width),
            self.irradiance_atlas_height
                .max(self.visibility_atlas_height),
        )
    }

    fn refresh_sampling_bind_group(&mut self, gpu: &GpuContext) {
        self.sampling_bind_group = create_ddgi_sampling_bind_group(
            gpu.device(),
            &self.sampling_bind_group_layout,
            &self.uniform_buffer,
            self.current_irradiance().view(),
            self.current_visibility().view(),
            &self.sampler,
        );
    }

    fn current_irradiance(&self) -> &RenderTarget {
        if self.current_is_a {
            &self.irradiance_a
        } else {
            &self.irradiance_b
        }
    }

    fn current_visibility(&self) -> &RenderTarget {
        if self.current_is_a {
            &self.visibility_a
        } else {
            &self.visibility_b
        }
    }

    fn write_disabled(&mut self, gpu: &GpuContext) {
        self.last_uniform = disabled_uniform();
        self.frame_index = 0;
        self.force_full_update = true;
        self.history_valid = false;
        gpu.queue().write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&self.last_uniform),
        );
        self.bind_group = create_ddgi_update_bind_group(
            gpu.device(),
            &self.bind_group_layout,
            &self.uniform_buffer,
            &self.triangle_buffer,
            &self.node_buffer,
            &self.light_buffer,
            &self.light_meta_buffer,
            self.current_irradiance().view(),
            self.current_irradiance().view(),
            self.current_visibility().view(),
            self.current_visibility().view(),
            &self.sampler,
        );
        self.refresh_sampling_bind_group(gpu);
    }

    fn resize_atlas_if_needed(
        &mut self,
        gpu: &GpuContext,
        irradiance_width: u32,
        irradiance_height: u32,
        visibility_width: u32,
        visibility_height: u32,
    ) {
        if self.irradiance_atlas_width == irradiance_width
            && self.irradiance_atlas_height == irradiance_height
            && self.visibility_atlas_width == visibility_width
            && self.visibility_atlas_height == visibility_height
        {
            return;
        }
        self.irradiance_a = create_ddgi_texture(
            gpu,
            irradiance_width,
            irradiance_height,
            DDGI_IRRADIANCE_FORMAT,
            "ddgi_irradiance_a",
        );
        self.irradiance_b = create_ddgi_texture(
            gpu,
            irradiance_width,
            irradiance_height,
            DDGI_IRRADIANCE_FORMAT,
            "ddgi_irradiance_b",
        );
        self.visibility_a = create_ddgi_texture(
            gpu,
            visibility_width,
            visibility_height,
            DDGI_VISIBILITY_FORMAT,
            "ddgi_visibility_a",
        );
        self.visibility_b = create_ddgi_texture(
            gpu,
            visibility_width,
            visibility_height,
            DDGI_VISIBILITY_FORMAT,
            "ddgi_visibility_b",
        );
        self.irradiance_atlas_width = irradiance_width;
        self.irradiance_atlas_height = irradiance_height;
        self.visibility_atlas_width = visibility_width;
        self.visibility_atlas_height = visibility_height;
        self.current_is_a = true;
        self.frame_index = 0;
        self.force_full_update = true;
        self.history_valid = false;
        self.refresh_sampling_bind_group(gpu);
    }

    fn invalidate_history_if_volume_changed(
        &mut self,
        origin: [f32; 3],
        spacing: f32,
        counts: [u32; 3],
        irradiance_resolution: u32,
        visibility_resolution: u32,
    ) {
        let origin_changed = self
            .history_origin
            .iter()
            .zip(origin)
            .any(|(previous, current)| (*previous - current).abs() > 0.0001);
        let changed = !self.history_valid
            || origin_changed
            || (self.history_spacing - spacing).abs() > 0.0001
            || self.history_counts != counts
            || self.history_irradiance_resolution != irradiance_resolution
            || self.history_visibility_resolution != visibility_resolution;
        if !changed {
            return;
        }

        self.history_valid = true;
        self.history_origin = origin;
        self.history_spacing = spacing;
        self.history_counts = counts;
        self.history_irradiance_resolution = irradiance_resolution;
        self.history_visibility_resolution = visibility_resolution;
        self.frame_index = 0;
        self.force_full_update = true;
    }

    fn ensure_triangle_capacity(&mut self, gpu: &GpuContext, required: usize) {
        if required <= self.triangle_capacity {
            return;
        }
        self.triangle_capacity = required.next_power_of_two();
        self.triangle_buffer = create_storage_buffer(
            gpu,
            "ddgi_triangles",
            self.triangle_capacity,
            std::mem::size_of::<GpuGiTriangle>(),
        );
    }

    fn ensure_node_capacity(&mut self, gpu: &GpuContext, required: usize) {
        if required <= self.node_capacity {
            return;
        }
        self.node_capacity = required.next_power_of_two();
        self.node_buffer = create_storage_buffer(
            gpu,
            "ddgi_bvh_nodes",
            self.node_capacity,
            std::mem::size_of::<GpuGiBvhNode>(),
        );
    }

    fn ensure_light_capacity(&mut self, gpu: &GpuContext, required: usize) {
        if required <= self.light_capacity {
            return;
        }
        self.light_capacity = required.next_power_of_two();
        self.light_buffer = create_storage_buffer(
            gpu,
            "ddgi_lights",
            self.light_capacity,
            std::mem::size_of::<GpuLight>(),
        );
    }
}

fn collect_gi_triangles(renderables: &[crate::render::gi::GiRenderable<'_>]) -> Vec<GpuGiTriangle> {
    let mut triangles = Vec::new();
    for renderable in renderables {
        if !renderable.opaque {
            continue;
        }
        let range = renderable
            .triangle_range
            .start
            .min(renderable.ray_triangles.len())
            ..renderable
                .triangle_range
                .end
                .min(renderable.ray_triangles.len());
        for triangle in &renderable.ray_triangles[range] {
            triangles.push(GpuGiTriangle::from_triangle(
                *triangle,
                renderable.model,
                renderable.material,
            ));
        }
    }
    triangles
}

fn ddgi_origin(settings: DdgiSettings, view: &SceneView) -> [f32; 3] {
    if !settings.volume.scroll_with_main_camera {
        return settings.volume.origin;
    }
    let spacing = settings.volume.spacing.max(0.05);
    let counts = settings.volume.counts.map(|value| value.max(1));
    let snapped_center = [
        (view.camera_position[0] / spacing).round() * spacing,
        (view.camera_position[1] / spacing).round() * spacing,
        (view.camera_position[2] / spacing).round() * spacing,
    ];
    [
        snapped_center[0] - (counts[0].saturating_sub(1)) as f32 * spacing * 0.5,
        snapped_center[1] - (counts[1].saturating_sub(1)) as f32 * spacing * 0.5,
        snapped_center[2] - (counts[2].saturating_sub(1)) as f32 * spacing * 0.5,
    ]
}

fn ddgi_atlas_size(counts: [u32; 3], resolution: u32) -> (u32, u32) {
    let tile_resolution = ddgi_tile_resolution(resolution);
    (
        counts[0].saturating_mul(tile_resolution).max(1),
        counts[1]
            .saturating_mul(counts[2])
            .saturating_mul(tile_resolution)
            .max(1),
    )
}

fn ddgi_tile_resolution(resolution: u32) -> u32 {
    resolution.saturating_add(DDGI_ATLAS_BORDER_TEXELS * 2)
}

fn create_storage_buffer(
    gpu: &GpuContext,
    label: &'static str,
    count: usize,
    stride: usize,
) -> wgpu::Buffer {
    gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (count.max(1) * stride) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_ddgi_texture(
    gpu: &GpuContext,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    label: impl Into<Cow<'static, str>>,
) -> RenderTarget {
    RenderTarget::from_descriptor(
        gpu,
        RenderTargetDescriptor::new(width, height, format)
            .usage(
                wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
            )
            .label(label),
    )
}

fn create_ddgi_sampling_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ddgi_sampling_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<DdgiUniform>() as u64)
                            .expect("DdgiUniform has non-zero size"),
                    ),
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
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn create_ddgi_sampling_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    irradiance: &wgpu::TextureView,
    visibility: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ddgi_sampling_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(irradiance),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(visibility),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

pub(crate) fn create_ddgi_update_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("ddgi_update_bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: DDGI_IRRADIANCE_FORMAT,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: DDGI_VISIBILITY_FORMAT,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 7,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 8,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 9,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

fn create_ddgi_update_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    triangle_buffer: &wgpu::Buffer,
    node_buffer: &wgpu::Buffer,
    light_buffer: &wgpu::Buffer,
    light_meta_buffer: &wgpu::Buffer,
    output_irradiance: &wgpu::TextureView,
    previous_irradiance: &wgpu::TextureView,
    output_visibility: &wgpu::TextureView,
    previous_visibility: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("ddgi_update_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: triangle_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: light_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: light_meta_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(output_irradiance),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(previous_irradiance),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::TextureView(output_visibility),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: node_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::TextureView(previous_visibility),
            },
        ],
    })
}

fn disabled_uniform() -> DdgiUniform {
    DdgiUniform {
        origin_spacing: [0.0, 0.0, 0.0, 1.0],
        counts_enabled: [1, 1, 1, 0],
        irradiance_atlas_params: [1, 1, 1, 0],
        visibility_atlas_params: [1, 1, 1, 0],
        trace_params: [1.0, 0.0, 0.0, 0.0],
        frame_params: [0, 1, 1, 1],
        ambient: [0.0, 0.0, 0.0, 1.0],
    }
}

const DDGI_SAMPLING_SHADER: &str = r#"
struct GiDdgiUniform {
    origin_spacing: vec4<f32>,
    counts_enabled: vec4<u32>,
    irradiance_atlas_params: vec4<u32>,
    visibility_atlas_params: vec4<u32>,
    trace_params: vec4<f32>,
    frame_params: vec4<u32>,
    ambient: vec4<f32>,
};

@group(2) @binding(0)
var<uniform> gi_ddgi: GiDdgiUniform;
@group(2) @binding(1)
var t_gi_ddgi_irradiance: texture_2d<f32>;
@group(2) @binding(2)
var t_gi_ddgi_visibility: texture_2d<f32>;
@group(2) @binding(3)
var s_gi_ddgi: sampler;

const GI_DDGI_ATLAS_BORDER: u32 = 1u;

fn gi_ddgi_probe_tile_resolution(resolution: u32) -> u32 {
    return max(1u, resolution) + GI_DDGI_ATLAS_BORDER * 2u;
}

fn gi_ddgi_probe_uv(coord: vec3<u32>, direction: vec3<f32>, atlas_params: vec4<u32>) -> vec2<f32> {
    let res = max(1u, atlas_params.z);
    let tile_res = gi_ddgi_probe_tile_resolution(res);
    let cy = max(1u, gi_ddgi.counts_enabled.y);
    let oct = oct_encode(direction);
    let local = (oct * 0.5 + vec2<f32>(0.5)) * f32(max(1u, res) - 1u);
    let pixel = vec2<f32>(
        f32(coord.x * tile_res + GI_DDGI_ATLAS_BORDER) + local.x,
        f32((coord.z * cy + coord.y) * tile_res + GI_DDGI_ATLAS_BORDER) + local.y,
    );
    return (pixel + vec2<f32>(0.5)) / vec2<f32>(
        f32(max(1u, atlas_params.x)),
        f32(max(1u, atlas_params.y)),
    );
}

fn gi_ddgi_irradiance_probe(coord: vec3<u32>, normal: vec3<f32>) -> vec3<f32> {
    return max(
        textureSampleLevel(
            t_gi_ddgi_irradiance,
            s_gi_ddgi,
            gi_ddgi_probe_uv(coord, normal, gi_ddgi.irradiance_atlas_params),
            0.0,
        ).rgb,
        vec3<f32>(0.0),
    );
}

fn gi_ddgi_visibility_probe(coord: vec3<u32>, direction: vec3<f32>) -> vec2<f32> {
    return textureSampleLevel(
        t_gi_ddgi_visibility,
        s_gi_ddgi,
        gi_ddgi_probe_uv(coord, direction, gi_ddgi.visibility_atlas_params),
        0.0,
    ).rg;
}

fn gi_ddgi_probe_world_position(coord: vec3<u32>) -> vec3<f32> {
    return gi_ddgi.origin_spacing.xyz + vec3<f32>(coord) * max(gi_ddgi.origin_spacing.w, 0.001);
}

fn gi_ddgi_visibility_weight(coord: vec3<u32>, world_position: vec3<f32>, normal: vec3<f32>) -> f32 {
    let probe_position = gi_ddgi_probe_world_position(coord);
    let to_point = world_position - probe_position;
    let distance = length(to_point);
    if (distance <= 0.0001) {
        return 1.0;
    }
    let direction = to_point / distance;
    let moments = gi_ddgi_visibility_probe(coord, direction);
    let mean = moments.x;
    let variance = max(moments.y - mean * mean, 0.0001);
    if (distance <= mean + gi_ddgi.trace_params.w) {
        return max(dot(normal, -direction), 0.1);
    }
    let chebyshev = variance / (variance + (distance - mean) * (distance - mean));
    return clamp(chebyshev * max(dot(normal, -direction), 0.1), 0.0, 1.0);
}

fn gi_ddgi_sample(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    if (gi_ddgi.counts_enabled.w == 0u) {
        return vec3<f32>(0.0);
    }
    let spacing = max(gi_ddgi.origin_spacing.w, 0.001);
    let counts = vec3<f32>(
        f32(max(1u, gi_ddgi.counts_enabled.x)),
        f32(max(1u, gi_ddgi.counts_enabled.y)),
        f32(max(1u, gi_ddgi.counts_enabled.z)),
    );
    let local = (world_position + normal * gi_ddgi.trace_params.z - gi_ddgi.origin_spacing.xyz) / spacing;
    let clamped = clamp(local, vec3<f32>(0.0), counts - vec3<f32>(1.0));
    let base_coord = vec3<u32>(floor(clamped));
    let next_coord = min(
        base_coord + vec3<u32>(1u),
        vec3<u32>(
            max(1u, gi_ddgi.counts_enabled.x) - 1u,
            max(1u, gi_ddgi.counts_enabled.y) - 1u,
            max(1u, gi_ddgi.counts_enabled.z) - 1u,
        ),
    );
    let frac = fract(clamped);
    let c000 = vec3<u32>(base_coord.x, base_coord.y, base_coord.z);
    let c100 = vec3<u32>(next_coord.x, base_coord.y, base_coord.z);
    let c010 = vec3<u32>(base_coord.x, next_coord.y, base_coord.z);
    let c110 = vec3<u32>(next_coord.x, next_coord.y, base_coord.z);
    let c001 = vec3<u32>(base_coord.x, base_coord.y, next_coord.z);
    let c101 = vec3<u32>(next_coord.x, base_coord.y, next_coord.z);
    let c011 = vec3<u32>(base_coord.x, next_coord.y, next_coord.z);
    let c111 = vec3<u32>(next_coord.x, next_coord.y, next_coord.z);
    let w000 = (1.0 - frac.x) * (1.0 - frac.y) * (1.0 - frac.z) * gi_ddgi_visibility_weight(c000, world_position, normal);
    let w100 = frac.x * (1.0 - frac.y) * (1.0 - frac.z) * gi_ddgi_visibility_weight(c100, world_position, normal);
    let w010 = (1.0 - frac.x) * frac.y * (1.0 - frac.z) * gi_ddgi_visibility_weight(c010, world_position, normal);
    let w110 = frac.x * frac.y * (1.0 - frac.z) * gi_ddgi_visibility_weight(c110, world_position, normal);
    let w001 = (1.0 - frac.x) * (1.0 - frac.y) * frac.z * gi_ddgi_visibility_weight(c001, world_position, normal);
    let w101 = frac.x * (1.0 - frac.y) * frac.z * gi_ddgi_visibility_weight(c101, world_position, normal);
    let w011 = (1.0 - frac.x) * frac.y * frac.z * gi_ddgi_visibility_weight(c011, world_position, normal);
    let w111 = frac.x * frac.y * frac.z * gi_ddgi_visibility_weight(c111, world_position, normal);
    let accum =
        gi_ddgi_irradiance_probe(c000, normal) * w000 +
        gi_ddgi_irradiance_probe(c100, normal) * w100 +
        gi_ddgi_irradiance_probe(c010, normal) * w010 +
        gi_ddgi_irradiance_probe(c110, normal) * w110 +
        gi_ddgi_irradiance_probe(c001, normal) * w001 +
        gi_ddgi_irradiance_probe(c101, normal) * w101 +
        gi_ddgi_irradiance_probe(c011, normal) * w011 +
        gi_ddgi_irradiance_probe(c111, normal) * w111;
    let weight = max(w000 + w100 + w010 + w110 + w001 + w101 + w011 + w111, 0.0001);
    return accum / weight;
}

fn gi_debug_mode() -> u32 {
    return gi_ddgi.frame_params.w >> 16u;
}

fn gi_ddgi_probe_count() -> u32 {
    return max(1u, gi_ddgi.counts_enabled.x * gi_ddgi.counts_enabled.y * gi_ddgi.counts_enabled.z);
}

fn gi_ddgi_probe_index(coord: vec3<u32>) -> u32 {
    return coord.x + coord.y * gi_ddgi.counts_enabled.x + coord.z * gi_ddgi.counts_enabled.x * gi_ddgi.counts_enabled.y;
}

fn gi_ddgi_nearest_probe_coord(world_position: vec3<f32>) -> vec3<u32> {
    let spacing = max(gi_ddgi.origin_spacing.w, 0.001);
    let counts = vec3<f32>(
        f32(max(1u, gi_ddgi.counts_enabled.x)),
        f32(max(1u, gi_ddgi.counts_enabled.y)),
        f32(max(1u, gi_ddgi.counts_enabled.z)),
    );
    let local = (world_position - gi_ddgi.origin_spacing.xyz) / spacing;
    return vec3<u32>(clamp(round(local), vec3<f32>(0.0), counts - vec3<f32>(1.0)));
}

fn gi_ddgi_probe_is_in_update_budget(index: u32) -> bool {
    let budget = max(1u, gi_ddgi.frame_params.z);
    let count = gi_ddgi_probe_count();
    if (budget >= count) {
        return true;
    }
    let start = (gi_ddgi.frame_params.x * budget) % count;
    let end = start + budget;
    if (end <= count) {
        return index >= start && index < end;
    }
    return index >= start || index < (end % count);
}

fn gi_debug_color(world_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let mode = gi_debug_mode();
    if (mode == 2u) {
        return gi_ddgi_sample(world_position, normal);
    }
    let coord = gi_ddgi_nearest_probe_coord(world_position + normal * gi_ddgi.trace_params.z);
    let probe_position = gi_ddgi_probe_world_position(coord);
    let dist = distance(world_position, probe_position);
    let spacing = max(gi_ddgi.origin_spacing.w, 0.001);
    if (mode == 1u) {
        let radius = spacing * 0.14;
        let shell = smoothstep(radius, radius * 0.55, dist);
        return mix(vec3<f32>(0.02, 0.03, 0.04), vec3<f32>(0.1, 0.75, 1.0), shell);
    }
    if (mode == 3u) {
        let direction = safe_normalize(world_position - probe_position);
        let moments = gi_ddgi_visibility_probe(coord, direction);
        let mean_distance = clamp(moments.x / max(gi_ddgi.trace_params.x, 0.001), 0.0, 1.0);
        let visibility = gi_ddgi_visibility_weight(coord, world_position, normal);
        return vec3<f32>(visibility, mean_distance, moments.y / max(gi_ddgi.trace_params.x * gi_ddgi.trace_params.x, 0.001));
    }
    if (mode == 4u) {
        let index = gi_ddgi_probe_index(coord);
        if (gi_ddgi_probe_is_in_update_budget(index)) {
            return vec3<f32>(1.0, 0.65, 0.08);
        }
        return vec3<f32>(0.025, 0.04, 0.07);
    }
    return vec3<f32>(0.0);
}

fn gi_sample_indirect_diffuse(
    world_position: vec3<f32>,
    normal: vec3<f32>,
    base_color: vec3<f32>,
    metallic: f32,
    roughness: f32,
) -> vec3<f32> {
    let sampled = gi_ddgi_sample(world_position, normal) * base_color * (1.0 - metallic);
    if (gi_ddgi.counts_enabled.w == 0u) {
        return base_color * (1.0 - metallic) * mix(0.0015, 0.0065, roughness);
    }
    return sampled;
}
"#;

impl GiProviderRuntime for DdgiRuntime {
    fn prepare(&mut self, gpu: &GpuContext, scene: &GiSceneInput<'_>, settings: &dyn GiSettings) {
        let Some(settings) = downcast_settings::<DdgiSettings>(settings, DDGI_PROVIDER_ID) else {
            self.write_disabled(gpu);
            return;
        };
        DdgiRuntime::prepare(self, gpu, *settings, scene);
    }

    fn update_descriptor(&self) -> Option<GiUpdateDescriptor> {
        let (width, height) = self.dispatch_size();
        if self.last_uniform.counts_enabled[3] == 0 || width == 0 || height == 0 {
            return None;
        }
        Some(GiUpdateDescriptor {
            label: "gi_update",
            bind_group_layout: self.bind_group_layout.clone(),
            bind_group: self.bind_group.clone(),
            shader: DDGI_SHADER,
            entry_point: "cs_main",
            dispatch_size: [width, height, 1],
            workgroup_size: [DDGI_WORKGROUP_SIZE, DDGI_WORKGROUP_SIZE, 1],
            flags: PassFlags::PREFER_ASYNC_COMPUTE | PassFlags::COMPUTE_INTENSIVE,
        })
    }

    fn update(
        &mut self,
        ctx: &mut ComputePassExecuteContext<'_, '_>,
    ) -> Result<(), RenderGraphError> {
        let Some((shader, entry_point, label, bind_group_layout, dispatch_size, workgroup_size)) =
            self.update_descriptor().map(|descriptor| {
                (
                    descriptor.shader,
                    descriptor.entry_point,
                    descriptor.label,
                    descriptor.bind_group_layout,
                    descriptor.dispatch_size,
                    descriptor.workgroup_size,
                )
            })
        else {
            return Ok(());
        };
        let (gpu, _pass, _resources, _execution) = ctx.split();
        let pipeline = self
            .update_pipeline
            .get_or_insert_with(|| {
                ComputePipelineCache::new(gpu, shader, entry_point, &[&bind_group_layout], label)
            })
            .pipeline(gpu);
        let mut frame = gpu.frame();
        let mut pass = frame.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some(label),
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(
            dispatch_size[0].div_ceil(workgroup_size[0].max(1)),
            dispatch_size[1].div_ceil(workgroup_size[1].max(1)),
            dispatch_size[2].div_ceil(workgroup_size[2].max(1)),
        );
        Ok(())
    }

    fn sampling_binding(&self) -> GiSamplingBinding {
        GiSamplingBinding {
            layout: self.sampling_bind_group_layout.clone(),
            bind_group: self.sampling_bind_group.clone(),
        }
    }

    fn shader_descriptor(&self) -> GiShaderDescriptor {
        GiShaderDescriptor {
            key: "ddgi",
            source: DDGI_SAMPLING_SHADER,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle(p0: [f32; 3], p1: [f32; 3], p2: [f32; 3]) -> GpuGiTriangle {
        GpuGiTriangle {
            p0: [p0[0], p0[1], p0[2], 0.0],
            p1: [p1[0], p1[1], p1[2], 0.0],
            p2: [p2[0], p2[1], p2[2], 0.0],
            normal_emissive: [0.0, 1.0, 0.0, 0.0],
            albedo: [1.0, 1.0, 1.0, 0.0],
        }
    }

    #[test]
    fn bvh_empty_scene_has_no_nodes() {
        assert!(build_gpu_bvh(&[]).is_empty());
    }

    #[test]
    fn bvh_single_triangle_builds_leaf() {
        let nodes = build_gpu_bvh(&[triangle(
            [-1.0, 0.0, 2.0],
            [2.0, 0.5, 2.0],
            [0.0, 3.0, -1.0],
        )]);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].meta, [0, 0, 0, 1]);
        assert_eq!(nodes[0].bounds_min, [-1.0, 0.0, -1.0, 0.0]);
        assert_eq!(nodes[0].bounds_max, [2.0, 3.0, 2.0, 0.0]);
    }

    #[test]
    fn bvh_multiple_triangles_builds_interior_nodes_and_leaves() {
        let triangles = [
            triangle([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            triangle([4.0, 0.0, 0.0], [5.0, 0.0, 0.0], [4.0, 1.0, 0.0]),
            triangle([-3.0, 0.0, 2.0], [-2.0, 0.0, 2.0], [-3.0, 1.0, 3.0]),
        ];
        let nodes = build_gpu_bvh(&triangles);

        assert_eq!(nodes.len(), triangles.len() * 2 - 1);
        assert_eq!(nodes[0].meta[3], 0);
        assert_eq!(nodes[0].bounds_min, [-3.0, 0.0, 0.0, 0.0]);
        assert_eq!(nodes[0].bounds_max, [5.0, 1.0, 3.0, 0.0]);

        let mut leaf_indices = nodes
            .iter()
            .filter(|node| node.meta[3] == 1)
            .map(|node| node.meta[2])
            .collect::<Vec<_>>();
        leaf_indices.sort_unstable();
        assert_eq!(leaf_indices, [0, 1, 2]);
    }

    #[test]
    fn ddgi_atlas_size_uses_requested_probe_resolution() {
        assert_eq!(ddgi_atlas_size([3, 2, 4], 5), (21, 56));
    }

    #[test]
    fn disabled_uniform_disables_both_ddgi_atlases() {
        let uniform = disabled_uniform();

        assert_eq!(uniform.counts_enabled, [1, 1, 1, 0]);
        assert_eq!(uniform.irradiance_atlas_params, [1, 1, 1, 0]);
        assert_eq!(uniform.visibility_atlas_params, [1, 1, 1, 0]);
    }

    #[test]
    fn ddgi_symbols_stay_inside_provider_boundary() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut files = vec![
            manifest_dir.join("src/render/runtime/frame_builder.rs"),
            manifest_dir.join("src/render/pipeline/asset.rs"),
            manifest_dir.join("src/render/gi/mod.rs"),
            manifest_dir.join("src/render/shaders/materials/standard_material.wgsl"),
            manifest_dir.join("src/render/shaders/materials/standard_material_normal_mapped.wgsl"),
        ];

        let shadow_dir = manifest_dir.join("src/render/lighting/shadow");
        for entry in std::fs::read_dir(&shadow_dir).expect("shadow directory should exist") {
            let path = entry
                .expect("shadow directory entry should be readable")
                .path();
            if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }

        for path in files {
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            for symbol in ["Ddgi", "DDGI", "ddgi"] {
                assert!(
                    !source.contains(symbol),
                    "`{}` must not contain DDGI symbol `{symbol}`; keep DDGI provider-private",
                    path.strip_prefix(&manifest_dir).unwrap_or(&path).display()
                );
            }
        }
    }
}
