use rustc_hash::FxHashMap;

use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::math::{Mat4, Vec3, Vec4};
use crate::render::gpu::{RenderTarget, RenderTargetDescriptor};
use crate::render::phase::OpaquePhase;
use crate::render::view::{Projection, SceneView, SceneViewKind};
use crate::render::LightTable;
use crate::render::{DirectionalLight, Transform};

use super::{
    create_shadow_pass_bind_group, create_shadow_scene_bind_group, ShadowPassBindingLayout,
    ShadowSceneBindingLayout, ShadowUniform,
};

pub(crate) struct ShadowViewBinding {
    enabled: bool,
    target: RenderTarget,
    uniform_buffer: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    shadow_pass_bind_group: wgpu::BindGroup,
    bias: f32,
    light_direction: [f32; 3],
    caster_count: usize,
}

impl ShadowViewBinding {
    pub(crate) fn new(
        gpu: &GpuContext,
        scene_layout: &ShadowSceneBindingLayout,
        pass_layout: &ShadowPassBindingLayout,
        sampler: &wgpu::Sampler,
        light_table: &LightTable,
    ) -> Self {
        let target = RenderTarget::from_descriptor(
            gpu,
            RenderTargetDescriptor::new_depth(1, 1).label("directional_shadow_map"),
        );
        let uniform_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("directional_shadow_uniform"),
            size: std::mem::size_of::<ShadowUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scene_bind_group = create_shadow_scene_bind_group(
            gpu.device(),
            scene_layout.bind_group_layout(),
            light_table.buffer(),
            light_table.meta_buffer(),
            &uniform_buffer,
            target.view(),
            sampler,
        );
        let shadow_pass_bind_group = create_shadow_pass_bind_group(
            gpu.device(),
            pass_layout.bind_group_layout(),
            &uniform_buffer,
        );
        let instance = Self {
            enabled: false,
            target,
            uniform_buffer,
            scene_bind_group,
            shadow_pass_bind_group,
            bias: 0.0,
            light_direction: [0.0, -1.0, 0.0],
            caster_count: 0,
        };
        instance.write_disabled(gpu.queue());
        instance
    }

    fn write_disabled(&self, queue: &wgpu::Queue) {
        let uniform = ShadowUniform {
            light_view_proj: IDENTITY_MATRIX,
            light_direction: [0.0, -1.0, 0.0, 0.0],
            shadow_params: [0.0, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn disable(&mut self, gpu: &GpuContext) {
        self.enabled = false;
        self.caster_count = 0;
        self.bias = 0.0;
        self.light_direction = [0.0, -1.0, 0.0];
        self.write_disabled(gpu.queue());
    }

    fn update(
        &mut self,
        gpu: &GpuContext,
        scene_layout: &ShadowSceneBindingLayout,
        pass_layout: &ShadowPassBindingLayout,
        sampler: &wgpu::Sampler,
        light_table: &LightTable,
        update: ShadowViewUpdate,
    ) {
        self.target.resize_with(
            gpu,
            RenderTargetDescriptor::new_depth(update.resolution, update.resolution)
                .label("directional_shadow_map"),
        );
        self.scene_bind_group = create_shadow_scene_bind_group(
            gpu.device(),
            scene_layout.bind_group_layout(),
            light_table.buffer(),
            light_table.meta_buffer(),
            &self.uniform_buffer,
            self.target.view(),
            sampler,
        );
        self.shadow_pass_bind_group = create_shadow_pass_bind_group(
            gpu.device(),
            pass_layout.bind_group_layout(),
            &self.uniform_buffer,
        );
        self.enabled = true;
        self.bias = update.bias;
        self.light_direction = update.light_direction;
        self.caster_count = update.caster_count;
        let uniform = ShadowUniform {
            light_view_proj: update.light_view_proj,
            light_direction: [
                update.light_direction[0],
                update.light_direction[1],
                update.light_direction[2],
                0.0,
            ],
            shadow_params: [update.bias, 0.0, 0.0, 1.0],
        };
        gpu.queue()
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    #[inline]
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    #[inline]
    pub(crate) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.scene_bind_group
    }

    #[inline]
    pub(crate) fn shadow_pass_bind_group(&self) -> &wgpu::BindGroup {
        &self.shadow_pass_bind_group
    }

    #[inline]
    pub(crate) fn target(&self) -> &RenderTarget {
        &self.target
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn caster_count(&self) -> usize {
        self.caster_count
    }
}

#[derive(Clone, Copy)]
struct ShadowViewUpdate {
    light_view_proj: [f32; 16],
    light_direction: [f32; 3],
    resolution: u32,
    bias: f32,
    caster_count: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct DirectionalShadowSetup {
    pub(crate) binding_index: usize,
    light_direction: [f32; 3],
    resolution: u32,
    bias: f32,
}

pub(crate) fn append_directional_shadow_views(
    world: &World,
    views: &mut Vec<SceneView>,
) -> Vec<DirectionalShadowSetup> {
    let base_view_count = views.len();
    let mut lights = Vec::new();
    let mut query = world.query::<&DirectionalLight>();
    query.for_each(world, |light| {
        if light.visible && light.casts_shadows {
            lights.push(*light);
        }
    });

    let mut setups = Vec::new();
    let mut shadow_views = Vec::new();
    for binding_index in 0..base_view_count {
        let view = views[binding_index];
        if view.is_shadow() {
            continue;
        }

        views[binding_index] = view.with_shadow_binding(binding_index);
        let Some(light) = select_shadow_light(&lights, view.layer_mask) else {
            continue;
        };
        let Some((shadow_view, setup)) = build_shadow_view(&view, light, binding_index) else {
            continue;
        };
        shadow_views.push(shadow_view);
        setups.push(setup);
    }
    views.extend(shadow_views);
    setups
}

pub(crate) fn sync_shadow_views(
    shadow_views: &mut Vec<ShadowViewBinding>,
    gpu: &GpuContext,
    views: &[SceneView],
    opaque_phases: &[OpaquePhase],
    shadow_setups: &[DirectionalShadowSetup],
    scene_layout: &ShadowSceneBindingLayout,
    pass_layout: &ShadowPassBindingLayout,
    sampler: &wgpu::Sampler,
    light_table: &LightTable,
) {
    let binding_count = views
        .iter()
        .filter_map(|view| view.shadow_binding())
        .max()
        .map_or(0, |max_binding| max_binding + 1);
    while shadow_views.len() < binding_count {
        shadow_views.push(ShadowViewBinding::new(
            gpu,
            scene_layout,
            pass_layout,
            sampler,
            light_table,
        ));
    }
    if shadow_views.len() > binding_count {
        shadow_views.truncate(binding_count);
    }

    let mut shadow_view_by_binding = FxHashMap::default();
    for (view_index, view) in views.iter().enumerate() {
        if view.kind == SceneViewKind::DirectionalShadow {
            if let Some(binding_index) = view.shadow_binding() {
                shadow_view_by_binding.insert(binding_index, (view_index, *view));
            }
        }
    }

    let mut setup_by_binding = FxHashMap::default();
    for setup in shadow_setups {
        setup_by_binding.insert(setup.binding_index, *setup);
    }

    for (binding_index, shadow_view) in shadow_views.iter_mut().enumerate() {
        let Some(setup) = setup_by_binding.get(&binding_index).copied() else {
            shadow_view.disable(gpu);
            continue;
        };
        let Some((view_index, scene_view)) = shadow_view_by_binding.get(&binding_index).copied()
        else {
            shadow_view.disable(gpu);
            continue;
        };
        let caster_count = opaque_phases.get(view_index).map_or(0, OpaquePhase::len);
        if caster_count == 0 {
            shadow_view.disable(gpu);
            continue;
        }
        let update = ShadowViewUpdate {
            light_view_proj: scene_view.view_uniform.view_proj,
            light_direction: setup.light_direction,
            resolution: setup.resolution,
            bias: setup.bias,
            caster_count,
        };
        shadow_view.update(gpu, scene_layout, pass_layout, sampler, light_table, update);
    }
}

fn select_shadow_light(lights: &[DirectionalLight], layer_mask: u32) -> Option<DirectionalLight> {
    lights
        .iter()
        .copied()
        .filter(|light| light.layer_mask & layer_mask != 0)
        .max_by(|lhs, rhs| lhs.intensity.total_cmp(&rhs.intensity))
}

fn build_shadow_view(
    view: &SceneView,
    light: DirectionalLight,
    binding_index: usize,
) -> Option<(SceneView, DirectionalShadowSetup)> {
    let light_direction = Vec3::from_array(light.direction)
        .try_normalized()?
        .to_array();
    let corners = view_frustum_corners_world(view)?;
    let center = average_points(&corners);
    let light_view = light_view_matrix(center, light_direction);

    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut min_depth = f32::INFINITY;
    let mut max_depth = f32::NEG_INFINITY;

    for corner in &corners {
        let light_space = Mat4::from_cols_array(light_view)
            .transform_point3(Vec3::from_array(*corner))
            .to_array();
        min_x = min_x.min(light_space[0]);
        max_x = max_x.max(light_space[0]);
        min_y = min_y.min(light_space[1]);
        max_y = max_y.max(light_space[1]);
        let depth = -light_space[2];
        min_depth = min_depth.min(depth);
        max_depth = max_depth.max(depth);
    }

    if !min_x.is_finite()
        || !max_x.is_finite()
        || !min_y.is_finite()
        || !max_y.is_finite()
        || !min_depth.is_finite()
        || !max_depth.is_finite()
    {
        return None;
    }

    let padding_xy = 0.5;
    let padding_depth = 2.0;
    min_x -= padding_xy;
    max_x += padding_xy;
    min_y -= padding_xy;
    max_y += padding_xy;
    min_depth = (min_depth - padding_depth).max(0.0);
    max_depth += padding_depth;

    let projection = Mat4::orthographic_rh(min_x, max_x, min_y, max_y, min_depth, max_depth);
    let inverse_view = Mat4::from_cols_array(light_view).inverse().to_cols_array();
    let camera_position = [inverse_view[12], inverse_view[13], inverse_view[14]];
    let view_proj = (projection * Mat4::from_cols_array(light_view)).to_cols_array();
    let resolution = light.shadow_map_size.max(1);
    let view_uniform = crate::render::view::ViewUniform {
        view_proj,
        camera: [
            camera_position[0],
            camera_position[1],
            camera_position[2],
            1.0,
        ],
        viewport: [
            resolution as f32,
            resolution as f32,
            (resolution as f32).recip(),
            (resolution as f32).recip(),
        ],
        view: light_view,
        projection: projection.to_cols_array(),
        inverse_view,
        camera_position: [
            camera_position[0],
            camera_position[1],
            camera_position[2],
            1.0,
        ],
        near_far_time_delta: [min_depth, max_depth, 0.0, 0.0],
    };
    let shadow_view = SceneView::from_parts(
        view.order,
        view.viewport,
        [resolution, resolution],
        false,
        SceneViewKind::DirectionalShadow,
        Some(binding_index),
        view.layer_mask,
        Transform::from_xyz(camera_position[0], camera_position[1], camera_position[2]),
        Projection::orthographic_fixed((max_x - min_x).max(1e-3), (max_y - min_y).max(1e-3)),
        view_uniform,
        false,
    );

    Some((
        shadow_view,
        DirectionalShadowSetup {
            binding_index,
            light_direction,
            resolution,
            bias: light.shadow_bias.max(0.0),
        },
    ))
}

fn view_frustum_corners_world(view: &SceneView) -> Option<[[f32; 3]; 8]> {
    let inverse_view_proj = Mat4::from_cols_array(view.view_uniform.view_proj).inverse();
    let corners = [
        [-1.0, -1.0, 0.0],
        [1.0, -1.0, 0.0],
        [1.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0],
        [-1.0, -1.0, 1.0],
        [1.0, -1.0, 1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];

    let mut world = [[0.0; 3]; 8];
    for (index, corner) in corners.into_iter().enumerate() {
        let clip = Vec4::from_array([corner[0], corner[1], corner[2], 1.0]);
        let homogeneous = inverse_view_proj * clip;
        if homogeneous.w().abs() <= 1e-6 {
            return None;
        }
        let inv_w = homogeneous.w().recip();
        world[index] = [
            homogeneous.x() * inv_w,
            homogeneous.y() * inv_w,
            homogeneous.z() * inv_w,
        ];
    }
    Some(world)
}

fn average_points(points: &[[f32; 3]]) -> [f32; 3] {
    let mut sum = [0.0; 3];
    for point in points {
        sum[0] += point[0];
        sum[1] += point[1];
        sum[2] += point[2];
    }
    let scale = (points.len() as f32).recip();
    [sum[0] * scale, sum[1] * scale, sum[2] * scale]
}

fn light_view_matrix(origin: [f32; 3], light_direction: [f32; 3]) -> [f32; 16] {
    let up_guess = if light_direction[1].abs() > 0.98 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    Mat4::look_to_rh(
        Vec3::from_array(origin),
        Vec3::from_array(light_direction),
        Vec3::from_array(up_guess),
    )
    .to_cols_array()
}

pub(crate) const IDENTITY_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];
