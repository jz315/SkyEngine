//! Dynamic diffuse global illumination built around DDGI probe volumes.

mod ssgi;

use std::borrow::Cow;

use crate::gpu::GpuContext;
use crate::math::{Mat4, Vec3};
use crate::render::component::{DdgiSettings, GiDebugMode, GlobalIlluminationSettings};
use crate::render::gpu::{RenderTarget, RenderTargetDescriptor};
use crate::render::phase::{DrawFunctionRegistry, MeshDrawData, OpaquePhase};
use crate::render::resources::material::{MaterialRegistry, StandardMaterial};
use crate::render::resources::mesh::{Mesh, MeshRegistry, RayTriangle};
use crate::render::view::{Color, SceneView};
use crate::render::GpuLight;

pub(crate) use ssgi::{SsgiComputeGraphResources, SSGI_COMPUTE_RESOURCES_BLACKBOARD};
pub use ssgi::{SsgiComputeTextureLayout, SsgiPass, SsgiResources};

pub(crate) const DDGI_IRRADIANCE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(crate) const DDGI_VISIBILITY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(crate) const DDGI_WORKGROUP_SIZE: u32 = 8;
pub(crate) const DDGI_ATLAS_BORDER_TEXELS: u32 = 1;
pub(crate) const DDGI_SHADER: &str = include_str!("../shaders/gi/ddgi_update.wgsl");

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
    fn from_triangle(triangle: RayTriangle, transform: Mat4, material: &StandardMaterial) -> Self {
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

#[derive(Clone, Copy)]
pub(crate) struct DdgiSceneResources<'a> {
    pub(crate) uniform_buffer: &'a wgpu::Buffer,
    pub(crate) irradiance_view: &'a wgpu::TextureView,
    pub(crate) visibility_view: &'a wgpu::TextureView,
    pub(crate) sampler: &'a wgpu::Sampler,
}

pub(crate) struct DdgiRuntime {
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
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
}

impl DdgiRuntime {
    pub(crate) fn new(gpu: &GpuContext) -> Self {
        let bind_group_layout = create_ddgi_update_bind_group_layout(gpu.device());
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
        Self {
            bind_group_layout,
            bind_group,
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
        }
    }

    pub(crate) fn prepare(
        &mut self,
        gpu: &GpuContext,
        settings: GlobalIlluminationSettings,
        views: &[SceneView],
        opaque_phases: &[OpaquePhase],
        draw_functions: &DrawFunctionRegistry,
        model_matrices: &[[f32; 16]],
        lights: &[GpuLight],
        material_registry: &MaterialRegistry,
        mesh_registry: &MeshRegistry,
        ambient_color: Color,
    ) {
        let Some((scene_view_index, scene_view)) =
            views.iter().enumerate().find(|(_, view)| !view.is_shadow())
        else {
            self.write_disabled(gpu);
            return;
        };
        if !settings.uses_ddgi() {
            self.write_disabled(gpu);
            return;
        }
        let ddgi = settings.ddgi;
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

        let triangles = collect_gi_triangles(
            opaque_phases
                .get(scene_view_index)
                .map(std::slice::from_ref)
                .unwrap_or(&[]),
            draw_functions,
            model_matrices,
            material_registry,
            mesh_registry,
        );
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
        self.ensure_light_capacity(gpu, lights.len().max(1));
        if !lights.is_empty() {
            gpu.queue()
                .write_buffer(&self.light_buffer, 0, bytemuck::cast_slice(lights));
        }
        gpu.queue().write_buffer(
            &self.light_meta_buffer,
            0,
            bytemuck::bytes_of(&DdgiLightMeta {
                count: lights.len() as u32,
                _pad: [0; 3],
            }),
        );

        let debug_mode = match settings.debug {
            GiDebugMode::Off => 0,
            GiDebugMode::Probes => 1,
            GiDebugMode::Irradiance => 2,
            GiDebugMode::Visibility => 3,
            GiDebugMode::RayBudget => 4,
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
            ambient: ambient_color.to_array(),
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
    }

    pub(crate) fn scene_resources(&self) -> DdgiSceneResources<'_> {
        DdgiSceneResources {
            uniform_buffer: &self.uniform_buffer,
            irradiance_view: self.current_irradiance().view(),
            visibility_view: self.current_visibility().view(),
            sampler: &self.sampler,
        }
    }

    #[inline]
    pub(crate) fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    #[inline]
    pub(crate) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    #[inline]
    pub(crate) fn dispatch_size(&self) -> (u32, u32) {
        (
            self.irradiance_atlas_width.max(self.visibility_atlas_width),
            self.irradiance_atlas_height
                .max(self.visibility_atlas_height),
        )
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

fn collect_gi_triangles(
    opaque_phases: &[OpaquePhase],
    draw_functions: &DrawFunctionRegistry,
    model_matrices: &[[f32; 16]],
    material_registry: &MaterialRegistry,
    mesh_registry: &MeshRegistry,
) -> Vec<GpuGiTriangle> {
    let Some(materials) = material_registry.try_materials::<StandardMaterial>() else {
        return Vec::new();
    };
    let standard_type = std::any::TypeId::of::<StandardMaterial>();
    let mut triangles = Vec::new();
    for phase in opaque_phases {
        for item in phase.items() {
            if draw_functions.material_type_id(item.draw_function_id) != Some(standard_type) {
                continue;
            }
            let draw = *item.data::<MeshDrawData>();
            let Some(mesh) = mesh_registry.get(draw.mesh_handle()) else {
                continue;
            };
            let Some(ray_mesh) = mesh.ray_mesh() else {
                continue;
            };
            let Some(triangle_range) = ray_triangle_range_for_sub_mesh(mesh, draw.sub_mesh_index())
            else {
                continue;
            };
            let Some(material) = materials.get(draw.material_handle::<StandardMaterial>()) else {
                continue;
            };
            if material.alpha_mode.is_transparent() {
                continue;
            }
            let model = Mat4::from_cols_array(
                *model_matrices
                    .get(draw.model_slot() as usize)
                    .unwrap_or(&IDENTITY_MATRIX),
            );
            for triangle in &ray_mesh.triangles()[triangle_range] {
                triangles.push(GpuGiTriangle::from_triangle(*triangle, model, material));
            }
        }
    }
    triangles
}

fn ray_triangle_range_for_sub_mesh(
    mesh: &Mesh,
    sub_mesh_index: u32,
) -> Option<std::ops::Range<usize>> {
    let ray_mesh = mesh.ray_mesh()?;
    let sub_mesh = mesh.sub_meshes().get(sub_mesh_index as usize)?;
    if !mesh.has_indices() {
        return Some(0..ray_mesh.triangles().len());
    }
    let index_offset = if sub_mesh.index_count == 0 {
        0
    } else {
        sub_mesh.index_offset
    };
    let index_count = if sub_mesh.index_count == 0 {
        mesh.index_count()
    } else {
        sub_mesh.index_count
    };
    let start = (index_offset / 3) as usize;
    let end = ((index_offset + index_count) / 3) as usize;
    Some(start.min(ray_mesh.triangles().len())..end.min(ray_mesh.triangles().len()))
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

pub(crate) const IDENTITY_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

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
}
