use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use rustc_hash::FxHashMap;

use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::math::{Mat4, Vec3, Vec4};
use crate::render::component::{RenderDebugView, MAX_DIRECTIONAL_SHADOW_CASCADES};
use crate::render::gpu::{RenderTarget, RenderTargetDescriptor};
use crate::render::phase::{MeshDrawData, OpaquePhase, TransparentPhase};
use crate::render::view::{Projection, SceneView, SceneViewKind};
use crate::render::LightTable;
use crate::render::{DirectionalLight, ShadowSamplingMode, ShadowUpdatePolicy, Transform};

use super::{
    create_shadow_pass_bind_group, create_shadow_scene_bind_group, ShadowAtlasLayout,
    ShadowAtlasStats, ShadowPassBindingLayout, ShadowPassUniform, ShadowSceneBindingLayout,
    ShadowUniform, DEFAULT_SHADOW_ATLAS_GUARD_BAND_TEXELS, TRANSPARENT_SHADOW_FORMAT,
};

fn render_debug_log_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("SKY_RENDER_DEBUG_LOG")
            .map(|value| {
                matches!(
                    value.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

fn shadow_sync_log_index() -> Option<u64> {
    static SYNC_INDEX: AtomicU64 = AtomicU64::new(0);
    if !render_debug_log_enabled() {
        return None;
    }
    let index = SYNC_INDEX.fetch_add(1, Ordering::Relaxed);
    index.is_multiple_of(120).then_some(index)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShadowRasterBias {
    pub(crate) constant: i32,
    pub(crate) slope_scale: f32,
    pub(crate) clamp: f32,
}

impl ShadowRasterBias {
    #[inline]
    pub(crate) const fn new(constant: i32, slope_scale: f32, clamp: f32) -> Self {
        Self {
            constant,
            slope_scale,
            clamp,
        }
    }
}

impl Default for ShadowRasterBias {
    fn default() -> Self {
        Self::new(2, 2.0, 0.0)
    }
}

pub(crate) struct ShadowViewBinding {
    enabled: bool,
    target: RenderTarget,
    transparent_target: RenderTarget,
    uniform_buffer: wgpu::Buffer,
    scene_bind_group: wgpu::BindGroup,
    shadow_pass_uniform_buffers: Vec<wgpu::Buffer>,
    shadow_pass_bind_groups: Vec<wgpu::BindGroup>,
    bias: f32,
    radius: f32,
    raster_bias: ShadowRasterBias,
    normal_bias: f32,
    cascade_count: u32,
    caster_count_by_cascade: [usize; MAX_DIRECTIONAL_SHADOW_CASCADES],
    atlas_layout: ShadowAtlasLayout,
    light_direction: [f32; 3],
    caster_count: usize,
    debug_mode: f32,
    cascade_update_mask: u32,
    previous_cascade_signatures: [u64; MAX_DIRECTIONAL_SHADOW_CASCADES],
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
        let transparent_target = RenderTarget::from_descriptor(
            gpu,
            RenderTargetDescriptor::new(1, 1, TRANSPARENT_SHADOW_FORMAT)
                .label("directional_transparent_shadow_map"),
        );
        let uniform_buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("directional_shadow_uniform"),
            size: std::mem::size_of::<ShadowUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (shadow_pass_uniform_buffers, shadow_pass_bind_groups) =
            create_shadow_pass_bindings(gpu.device(), pass_layout.bind_group_layout());
        let scene_bind_group = create_shadow_scene_bind_group(
            gpu.device(),
            scene_layout.bind_group_layout(),
            light_table.buffer(),
            light_table.meta_buffer(),
            &uniform_buffer,
            target.view(),
            sampler,
            transparent_target.view(),
            gpu.sampler_linear(),
        );
        let instance = Self {
            enabled: false,
            target,
            transparent_target,
            uniform_buffer,
            scene_bind_group,
            shadow_pass_uniform_buffers,
            shadow_pass_bind_groups,
            bias: 0.0,
            radius: 0.0,
            raster_bias: ShadowRasterBias::default(),
            normal_bias: 0.0,
            cascade_count: 0,
            caster_count_by_cascade: [0; MAX_DIRECTIONAL_SHADOW_CASCADES],
            atlas_layout: ShadowAtlasLayout::default(),
            light_direction: [0.0, -1.0, 0.0],
            caster_count: 0,
            debug_mode: 0.0,
            cascade_update_mask: 0,
            previous_cascade_signatures: [0; MAX_DIRECTIONAL_SHADOW_CASCADES],
        };
        instance.write_disabled(gpu.queue());
        instance
    }

    fn write_disabled(&self, queue: &wgpu::Queue) {
        let uniform = ShadowUniform {
            light_view_proj: [IDENTITY_MATRIX; MAX_DIRECTIONAL_SHADOW_CASCADES],
            light_direction: [0.0, -1.0, 0.0, 0.0],
            cascade_splits: [0.0; MAX_DIRECTIONAL_SHADOW_CASCADES],
            cascade_params: [[0.0; 4]; MAX_DIRECTIONAL_SHADOW_CASCADES],
            shadow_atlas_mul_add: [0.0; 4],
            shadow_atlas_resolution_rcp: [0.0; 4],
            shadow_params: [0.0, 0.0, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));
    }

    fn disable(
        &mut self,
        gpu: &GpuContext,
        scene_layout: &ShadowSceneBindingLayout,
        sampler: &wgpu::Sampler,
        light_table: &LightTable,
    ) {
        self.enabled = false;
        self.caster_count = 0;
        self.bias = 0.0;
        self.radius = 0.0;
        self.raster_bias = ShadowRasterBias::default();
        self.normal_bias = 0.0;
        self.cascade_count = 0;
        self.caster_count_by_cascade = [0; MAX_DIRECTIONAL_SHADOW_CASCADES];
        self.atlas_layout = ShadowAtlasLayout::default();
        self.light_direction = [0.0, -1.0, 0.0];
        self.debug_mode = 0.0;
        self.cascade_update_mask = 0;
        self.previous_cascade_signatures = [0; MAX_DIRECTIONAL_SHADOW_CASCADES];
        self.scene_bind_group = create_shadow_scene_bind_group(
            gpu.device(),
            scene_layout.bind_group_layout(),
            light_table.buffer(),
            light_table.meta_buffer(),
            &self.uniform_buffer,
            self.target.view(),
            sampler,
            self.transparent_target.view(),
            gpu.sampler_linear(),
        );
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
        let cascade_count = update
            .cascade_count
            .clamp(1, MAX_DIRECTIONAL_SHADOW_CASCADES as u32);
        let atlas_layout = ShadowAtlasLayout::directional_packed(
            update.resolution,
            cascade_count,
            DEFAULT_SHADOW_ATLAS_GUARD_BAND_TEXELS,
        );
        let atlas_size = atlas_layout.atlas_size();
        let force_update = !self.enabled
            || self.cascade_count != cascade_count
            || self.target.width() != atlas_size[0]
            || self.target.height() != atlas_size[1];
        self.target.resize_with(
            gpu,
            RenderTargetDescriptor::new_depth(atlas_size[0], atlas_size[1])
                .label("directional_shadow_atlas"),
        );
        self.transparent_target.resize_with(
            gpu,
            RenderTargetDescriptor::new(atlas_size[0], atlas_size[1], TRANSPARENT_SHADOW_FORMAT)
                .label("directional_transparent_shadow_atlas"),
        );
        self.scene_bind_group = create_shadow_scene_bind_group(
            gpu.device(),
            scene_layout.bind_group_layout(),
            light_table.buffer(),
            light_table.meta_buffer(),
            &self.uniform_buffer,
            self.target.view(),
            sampler,
            self.transparent_target.view(),
            gpu.sampler_linear(),
        );
        if self.shadow_pass_bind_groups.len() != MAX_DIRECTIONAL_SHADOW_CASCADES {
            let (buffers, bind_groups) =
                create_shadow_pass_bindings(gpu.device(), pass_layout.bind_group_layout());
            self.shadow_pass_uniform_buffers = buffers;
            self.shadow_pass_bind_groups = bind_groups;
        }
        self.enabled = true;
        self.bias = update.bias;
        self.radius = update.radius;
        self.raster_bias = update.raster_bias;
        self.normal_bias = update.normal_bias;
        self.cascade_count = cascade_count;
        self.caster_count_by_cascade = update.caster_count_by_cascade;
        self.atlas_layout = atlas_layout;
        self.light_direction = update.light_direction;
        self.caster_count = update.caster_count;
        self.debug_mode = update.debug_mode;
        self.cascade_update_mask = cascade_update_mask(
            update.update_policy,
            force_update,
            cascade_count,
            &self.previous_cascade_signatures,
            &update.cascade_signatures,
        );
        self.previous_cascade_signatures = update.cascade_signatures;
        let mut light_view_proj = [IDENTITY_MATRIX; MAX_DIRECTIONAL_SHADOW_CASCADES];
        let mut cascade_params = [[0.0; 4]; MAX_DIRECTIONAL_SHADOW_CASCADES];
        for index in 0..cascade_count as usize {
            light_view_proj[index] = update.light_view_proj[index];
            cascade_params[index] = [
                update.bias,
                update.texel_world_sizes[index].max(f32::EPSILON),
                update.radius,
                update.receiver_depth_ranges[index].max(f32::EPSILON),
            ];
            let pass_uniform = ShadowPassUniform {
                raster_view_proj: update.raster_view_proj[index],
                depth_view_proj: update.light_view_proj[index],
            };
            gpu.queue().write_buffer(
                &self.shadow_pass_uniform_buffers[index],
                0,
                bytemuck::bytes_of(&pass_uniform),
            );
        }
        let uniform = ShadowUniform {
            light_view_proj,
            light_direction: [
                update.light_direction[0],
                update.light_direction[1],
                update.light_direction[2],
                update.normal_bias,
            ],
            cascade_splits: update.cascade_splits,
            cascade_params,
            shadow_atlas_mul_add: atlas_layout.shadow_atlas_mul_add(),
            shadow_atlas_resolution_rcp: shadow_atlas_resolution_rcp_with_sampling_mode(
                atlas_layout,
                update.sampling_mode,
            ),
            shadow_params: [
                cascade_count as f32,
                update.cascade_blend,
                update.debug_mode,
                1.0 + update.temporal_rotation_seed.clamp(0.0, 0.999_999),
            ],
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
    pub(crate) fn shadow_pass_bind_group(&self, cascade_index: u32) -> &wgpu::BindGroup {
        let index = (cascade_index as usize).min(self.shadow_pass_bind_groups.len() - 1);
        &self.shadow_pass_bind_groups[index]
    }

    #[inline]
    pub(crate) fn target(&self) -> &RenderTarget {
        &self.target
    }

    #[inline]
    pub(crate) fn transparent_target(&self) -> &RenderTarget {
        &self.transparent_target
    }

    #[inline]
    pub(crate) fn cascade_count(&self) -> u32 {
        self.cascade_count
    }

    #[inline]
    pub(crate) fn atlas_layout(&self) -> ShadowAtlasLayout {
        self.atlas_layout
    }

    #[inline]
    pub(crate) fn atlas_stats(&self) -> ShadowAtlasStats {
        if self.enabled {
            self.atlas_layout.stats()
        } else {
            ShadowAtlasStats::default()
        }
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn caster_count(&self) -> usize {
        self.caster_count
    }

    #[inline]
    pub(crate) fn caster_count_by_cascade(&self) -> [usize; MAX_DIRECTIONAL_SHADOW_CASCADES] {
        self.caster_count_by_cascade
    }

    #[inline]
    pub(crate) fn should_update_cascade(&self, cascade_index: u32) -> bool {
        let bit = 1u32.checked_shl(cascade_index).unwrap_or(0);
        self.enabled && bit != 0 && (self.cascade_update_mask & bit) != 0
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn radius(&self) -> f32 {
        self.radius
    }

    #[inline]
    pub(crate) fn raster_bias(&self) -> ShadowRasterBias {
        self.raster_bias
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn normal_bias(&self) -> f32 {
        self.normal_bias
    }

    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn debug_mode(&self) -> f32 {
        self.debug_mode
    }
}

fn create_shadow_pass_bindings(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
) -> (Vec<wgpu::Buffer>, Vec<wgpu::BindGroup>) {
    let mut buffers = Vec::with_capacity(MAX_DIRECTIONAL_SHADOW_CASCADES);
    let mut bind_groups = Vec::with_capacity(MAX_DIRECTIONAL_SHADOW_CASCADES);
    for _cascade in 0..MAX_DIRECTIONAL_SHADOW_CASCADES {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("directional_shadow_pass_uniform"),
            size: std::mem::size_of::<ShadowPassUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = create_shadow_pass_bind_group(device, layout, &buffer);
        buffers.push(buffer);
        bind_groups.push(bind_group);
    }
    (buffers, bind_groups)
}

#[derive(Clone, Copy)]
struct ShadowViewUpdate {
    light_view_proj: [[f32; 16]; MAX_DIRECTIONAL_SHADOW_CASCADES],
    raster_view_proj: [[f32; 16]; MAX_DIRECTIONAL_SHADOW_CASCADES],
    light_direction: [f32; 3],
    resolution: u32,
    bias: f32,
    radius: f32,
    raster_bias: ShadowRasterBias,
    normal_bias: f32,
    texel_world_sizes: [f32; MAX_DIRECTIONAL_SHADOW_CASCADES],
    receiver_depth_ranges: [f32; MAX_DIRECTIONAL_SHADOW_CASCADES],
    cascade_count: u32,
    cascade_splits: [f32; MAX_DIRECTIONAL_SHADOW_CASCADES],
    cascade_blend: f32,
    caster_count: usize,
    caster_count_by_cascade: [usize; MAX_DIRECTIONAL_SHADOW_CASCADES],
    cascade_signatures: [u64; MAX_DIRECTIONAL_SHADOW_CASCADES],
    debug_mode: f32,
    temporal_rotation_seed: f32,
    sampling_mode: ShadowSamplingMode,
    update_policy: ShadowUpdatePolicy,
}

#[derive(Clone, Copy)]
pub(crate) struct DirectionalShadowSetup {
    pub(crate) binding_index: usize,
    cascade_index: u32,
    light_direction: [f32; 3],
    light_view_proj: [f32; 16],
    resolution: u32,
    world_extent: [f32; 2],
    texel_world_size: [f32; 2],
    receiver_depth_extent: f32,
    caster_depth_extent: f32,
    bias: f32,
    radius: f32,
    raster_bias: ShadowRasterBias,
    normal_bias: f32,
    cascade_count: u32,
    cascade_splits: [f32; MAX_DIRECTIONAL_SHADOW_CASCADES],
    cascade_blend: f32,
    sampling_mode: ShadowSamplingMode,
    update_policy: ShadowUpdatePolicy,
}

pub(crate) fn append_directional_shadow_views(
    world: &World,
    views: &mut Vec<SceneView>,
) -> Vec<DirectionalShadowSetup> {
    let base_view_count = views.len();
    let mut lights = Vec::new();
    let query = world.query::<&DirectionalLight>();
    query.for_each(|light| {
        if light.visible && light.casts_shadows {
            lights.push(*light);
        }
    });

    let mut setups = Vec::new();
    let mut shadow_views = Vec::new();
    for (binding_index, view_slot) in views.iter_mut().take(base_view_count).enumerate() {
        let view = *view_slot;
        if view.is_shadow() {
            continue;
        }

        *view_slot = view.with_shadow_binding(binding_index);
        let Some(light) = select_shadow_light(&lights, view.layer_mask) else {
            continue;
        };
        let cascade_count = resolved_cascade_count(light);
        for cascade_index in 0..cascade_count {
            let Some((shadow_view, setup)) =
                build_shadow_view(&view, light, binding_index, cascade_index)
            else {
                continue;
            };
            shadow_views.push(shadow_view);
            setups.push(setup);
        }
    }
    views.extend(shadow_views);
    setups
}

pub(crate) fn sync_shadow_views(
    shadow_views: &mut Vec<ShadowViewBinding>,
    gpu: &GpuContext,
    views: &[SceneView],
    opaque_phases: &[OpaquePhase],
    transparent_phases: &[TransparentPhase],
    model_matrices: &[[f32; 16]],
    shadow_setups: &[DirectionalShadowSetup],
    scene_layout: &ShadowSceneBindingLayout,
    pass_layout: &ShadowPassBindingLayout,
    sampler: &wgpu::Sampler,
    light_table: &LightTable,
    debug_view: RenderDebugView,
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
                shadow_view_by_binding
                    .insert((binding_index, view.shadow_cascade()), (view_index, *view));
            }
        }
    }

    let mut setup_by_binding: FxHashMap<usize, DirectionalShadowSetup> = FxHashMap::default();
    let mut setup_by_cascade: FxHashMap<(usize, u32), DirectionalShadowSetup> =
        FxHashMap::default();
    for setup in shadow_setups {
        setup_by_cascade.insert((setup.binding_index, setup.cascade_index), *setup);
        setup_by_binding
            .entry(setup.binding_index)
            .or_insert(*setup);
    }

    for (binding_index, shadow_view) in shadow_views.iter_mut().enumerate() {
        let log_shadow_details = shadow_sync_log_index();
        let Some(setup) = setup_by_binding.get(&binding_index).copied() else {
            shadow_view.disable(gpu, scene_layout, sampler, light_table);
            continue;
        };
        let mut light_view_proj = [IDENTITY_MATRIX; MAX_DIRECTIONAL_SHADOW_CASCADES];
        let mut caster_count_by_cascade = [0usize; MAX_DIRECTIONAL_SHADOW_CASCADES];
        let mut cascade_signatures = [0u64; MAX_DIRECTIONAL_SHADOW_CASCADES];
        let mut texel_world_sizes = [0.0; MAX_DIRECTIONAL_SHADOW_CASCADES];
        let mut receiver_depth_ranges = [0.0; MAX_DIRECTIONAL_SHADOW_CASCADES];
        let mut raster_view_proj = [IDENTITY_MATRIX; MAX_DIRECTIONAL_SHADOW_CASCADES];
        let mut caster_count = 0usize;
        let mut missing_cascade = false;
        for cascade_index in 0..setup.cascade_count {
            let Some((view_index, scene_view)) = shadow_view_by_binding
                .get(&(binding_index, cascade_index))
                .copied()
            else {
                missing_cascade = true;
                break;
            };
            let Some(opaque_phase) = opaque_phases.get(view_index) else {
                missing_cascade = true;
                break;
            };
            let Some(transparent_phase) = transparent_phases.get(view_index) else {
                missing_cascade = true;
                break;
            };
            let cascade_slot = cascade_index as usize;
            let cascade_caster_count = opaque_phase.len() + transparent_phase.len();
            caster_count_by_cascade[cascade_slot] = cascade_caster_count;
            if let Some(log_index) = log_shadow_details {
                if let Some(cascade_setup) = setup_by_cascade
                    .get(&(binding_index, cascade_index))
                    .copied()
                {
                    eprintln!(
                        "[shadow][sync={}] binding={} cascade={} split=({:.3}->{:.3}) res={} extent=({:.3},{:.3}) texel_world=({:.5},{:.5}) receiver_depth_extent={:.3} caster_depth_extent={:.3} casters={} bias={:.6} raster_bias=({}, {:.3}, {:.3}) normal_bias={:.5} filter_radius={:.4} mode={:?}",
                        log_index,
                        binding_index,
                        cascade_index,
                        if cascade_index == 0 {
                            0.0
                        } else {
                            setup.cascade_splits[cascade_slot - 1]
                        },
                        setup.cascade_splits[cascade_slot],
                        cascade_setup.resolution,
                        cascade_setup.world_extent[0],
                        cascade_setup.world_extent[1],
                        cascade_setup.texel_world_size[0],
                        cascade_setup.texel_world_size[1],
                        cascade_setup.receiver_depth_extent,
                        cascade_setup.caster_depth_extent,
                        cascade_caster_count,
                        cascade_setup.bias,
                        cascade_setup.raster_bias.constant,
                        cascade_setup.raster_bias.slope_scale,
                        cascade_setup.raster_bias.clamp,
                        cascade_setup.normal_bias,
                        cascade_setup.radius,
                        cascade_setup.sampling_mode,
                    );
                }
            }
            let Some(cascade_setup) = setup_by_cascade
                .get(&(binding_index, cascade_index))
                .copied()
            else {
                missing_cascade = true;
                break;
            };
            light_view_proj[cascade_slot] = cascade_setup.light_view_proj;
            raster_view_proj[cascade_slot] = scene_view.view_uniform.view_proj;
            texel_world_sizes[cascade_slot] =
                ((cascade_setup.texel_world_size[0] + cascade_setup.texel_world_size[1]) * 0.5)
                    .max(f32::EPSILON);
            receiver_depth_ranges[cascade_slot] =
                (cascade_setup.receiver_depth_extent * 2.0).max(f32::EPSILON);
            cascade_signatures[cascade_slot] = shadow_cascade_signature(
                &cascade_setup,
                scene_view,
                opaque_phase,
                transparent_phase,
                model_matrices,
            );
            caster_count += cascade_caster_count;
        }
        if missing_cascade {
            shadow_view.disable(gpu, scene_layout, sampler, light_table);
            continue;
        }
        if caster_count == 0 {
            shadow_view.disable(gpu, scene_layout, sampler, light_table);
            continue;
        }
        let update = ShadowViewUpdate {
            light_view_proj,
            raster_view_proj,
            light_direction: setup.light_direction,
            resolution: setup.resolution,
            bias: setup.bias,
            radius: setup.radius,
            raster_bias: setup.raster_bias,
            normal_bias: setup.normal_bias,
            texel_world_sizes,
            receiver_depth_ranges,
            cascade_count: setup.cascade_count,
            cascade_splits: setup.cascade_splits,
            cascade_blend: setup.cascade_blend,
            caster_count,
            caster_count_by_cascade,
            cascade_signatures,
            debug_mode: shadow_debug_mode(debug_view),
            temporal_rotation_seed: shadow_temporal_rotation_seed(views),
            sampling_mode: setup.sampling_mode,
            update_policy: setup.update_policy,
        };
        shadow_view.update(gpu, scene_layout, pass_layout, sampler, light_table, update);
    }
}

fn shadow_debug_mode(debug_view: RenderDebugView) -> f32 {
    match debug_view {
        RenderDebugView::DirectionalShadowCoverage => 1.0,
        RenderDebugView::DirectionalShadowSplitCoverage => 2.0,
        RenderDebugView::DirectionalShadowFade => 3.0,
        RenderDebugView::DirectionalShadowCompareDelta => 4.0,
        RenderDebugView::DirectionalShadowBias => 5.0,
        RenderDebugView::DirectionalShadowPcss => 6.0,
        RenderDebugView::DirectLighting => 7.0,
        RenderDebugView::IndirectLighting => 8.0,
        _ => 0.0,
    }
}

fn shadow_temporal_rotation_seed(views: &[SceneView]) -> f32 {
    let Some(view) = views.iter().find(|view| !view.is_shadow()) else {
        return 0.0;
    };
    if view.temporal.jitter == [0.0, 0.0] && view.temporal.previous_jitter == [0.0, 0.0] {
        0.0
    } else {
        (view.temporal.frame_index % 256) as f32 / 256.0
    }
}

fn shadow_filter_radius(light: DirectionalLight) -> f32 {
    if light.shadow_filter_radius <= 0.0 {
        0.0
    } else {
        light.shadow_filter_radius.max(light.radius).max(0.0)
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
    cascade_index: u32,
) -> Option<(SceneView, DirectionalShadowSetup)> {
    let light_direction = Vec3::from_array(light.direction)
        .try_normalized()?
        .to_array();
    let cascade_count = resolved_cascade_count(light);
    let cascade_splits = resolved_cascade_splits(light, view, cascade_count);
    if cascade_index >= cascade_count {
        return None;
    }
    let full_corners = view_frustum_corners_world(view)?;
    let corners =
        cascade_frustum_corners_world(&full_corners, view, &cascade_splits, cascade_index);
    let light_view = light_view_matrix(light_direction);
    let light_view_matrix = Mat4::from_cols_array(light_view);

    // Ported from WickedEngine's MIT-licensed CreateDirLightShadowCams():
    // transform the camera frustum into light space, fit a bounding sphere,
    // and snap that sphere-aligned box to the shadow texel grid.
    let mut center = [0.0; 3];
    let mut light_space_corners = [[0.0; 3]; 8];
    for (index, corner) in corners.iter().enumerate() {
        let light_space = light_view_matrix
            .transform_point3(Vec3::from_array(*corner))
            .to_array();
        light_space_corners[index] = light_space;
        center[0] += light_space[0];
        center[1] += light_space[1];
        center[2] += light_space[2];
    }
    let center_scale = (light_space_corners.len() as f32).recip();
    center[0] *= center_scale;
    center[1] *= center_scale;
    center[2] *= center_scale;

    let mut radius = 0.0f32;
    for corner in &light_space_corners {
        let delta = [
            corner[0] - center[0],
            corner[1] - center[1],
            corner[2] - center[2],
        ];
        radius =
            radius.max((delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt());
    }
    if !radius.is_finite() || radius <= 0.0 {
        return None;
    }

    let mut min_x = center[0] - radius;
    let mut max_x = center[0] + radius;
    let mut min_y = center[1] - radius;
    let mut max_y = center[1] + radius;
    let min_z = center[2] - radius;

    let resolution = light.shadow_resolution_per_cascade.max(1);
    let texel_size_x = ((max_x - min_x) / resolution as f32).max(f32::EPSILON);
    let texel_size_y = ((max_y - min_y) / resolution as f32).max(f32::EPSILON);
    min_x = (min_x / texel_size_x).floor() * texel_size_x;
    max_x = (max_x / texel_size_x).floor() * texel_size_x;
    min_y = (min_y / texel_size_y).floor() * texel_size_y;
    max_y = (max_y / texel_size_y).floor() * texel_size_y;
    center[0] = (min_x + max_x) * 0.5;
    center[1] = (min_y + max_y) * 0.5;

    // Wicked expands the receiver slice for the real projection, then uses an
    // even coarser Z range for caster culling so off-slice casters can still
    // write into the cascade. The shadow shaders clamp clip Z to emulate the
    // depth-clamp part of that contract without requiring DEPTH_CLIP_CONTROL.
    let receiver_depth_extent = (center[2] - min_z).abs() * 4.0;
    let caster_depth_extent = if view.far.is_finite() {
        view.far.clamp(0.0, 2000.0) * 0.5
    } else {
        0.0
    };
    let culling_depth_extent = receiver_depth_extent.max(caster_depth_extent);
    let min_z = center[2] - receiver_depth_extent;
    let max_z = center[2] + receiver_depth_extent;
    let culling_min_z = center[2] - culling_depth_extent;
    let culling_max_z = center[2] + culling_depth_extent;
    let world_extent = [(max_x - min_x).abs(), (max_y - min_y).abs()];
    let texel_world_size = [
        world_extent[0] / resolution as f32,
        world_extent[1] / resolution as f32,
    ];

    let near = -max_z;
    let far = -min_z;
    if !near.is_finite() || !far.is_finite() || (far - near).abs() <= 1e-5 {
        return None;
    }
    let culling_near = -culling_max_z;
    let culling_far = -culling_min_z;
    if !culling_near.is_finite()
        || !culling_far.is_finite()
        || (culling_far - culling_near).abs() <= 1e-5
    {
        return None;
    }

    let projection = Mat4::orthographic_rh(min_x, max_x, min_y, max_y, near, far);
    let culling_projection =
        Mat4::orthographic_rh(min_x, max_x, min_y, max_y, culling_near, culling_far);
    let inverse_view = light_view_matrix.inverse().to_cols_array();
    let camera_position = [inverse_view[12], inverse_view[13], inverse_view[14]];
    let view_proj = (projection * light_view_matrix).to_cols_array();
    let culling_view_proj = (culling_projection * light_view_matrix).to_cols_array();
    let view_uniform = crate::render::view::ViewUniform {
        view_proj: culling_view_proj,
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
        projection: culling_projection.to_cols_array(),
        inverse_view,
        camera_position: [
            camera_position[0],
            camera_position[1],
            camera_position[2],
            1.0,
        ],
        near_far_time_delta: [culling_near, culling_far, 0.0, 0.0],
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
    )
    .with_shadow_binding_and_cascade(binding_index, cascade_index);

    Some((
        shadow_view,
        DirectionalShadowSetup {
            binding_index,
            cascade_index,
            light_direction,
            light_view_proj: view_proj,
            resolution,
            world_extent,
            texel_world_size,
            receiver_depth_extent,
            caster_depth_extent: culling_depth_extent,
            bias: light.shadow_bias.max(0.0),
            radius: shadow_filter_radius(light),
            raster_bias: ShadowRasterBias::new(
                light.shadow_depth_bias,
                light.shadow_slope_bias.max(0.0),
                0.0,
            ),
            normal_bias: light.shadow_normal_bias.max(0.0),
            cascade_count,
            cascade_splits,
            cascade_blend: light.cascade_blend.max(0.0),
            sampling_mode: light.shadow_sampling_mode,
            update_policy: light.shadow_update_policy,
        },
    ))
}

fn shadow_atlas_resolution_rcp_with_sampling_mode(
    atlas_layout: ShadowAtlasLayout,
    sampling_mode: ShadowSamplingMode,
) -> [f32; 4] {
    let mut params = atlas_layout.shadow_atlas_resolution_rcp();
    params[3] = sampling_mode.shader_code();
    params
}

fn cascade_update_mask(
    policy: ShadowUpdatePolicy,
    force_update: bool,
    cascade_count: u32,
    previous_signatures: &[u64; MAX_DIRECTIONAL_SHADOW_CASCADES],
    next_signatures: &[u64; MAX_DIRECTIONAL_SHADOW_CASCADES],
) -> u32 {
    let active_mask = if cascade_count >= MAX_DIRECTIONAL_SHADOW_CASCADES as u32 {
        (1u32 << MAX_DIRECTIONAL_SHADOW_CASCADES) - 1
    } else {
        (1u32 << cascade_count.max(1)) - 1
    };
    if force_update || matches!(policy, ShadowUpdatePolicy::EveryFrame) {
        return active_mask;
    }

    let mut mask = 0u32;
    for cascade in 0..cascade_count.min(MAX_DIRECTIONAL_SHADOW_CASCADES as u32) {
        let index = cascade as usize;
        if previous_signatures[index] != next_signatures[index] {
            mask |= 1u32 << cascade;
        }
    }
    mask
}

fn shadow_cascade_signature(
    setup: &DirectionalShadowSetup,
    scene_view: SceneView,
    opaque_phase: &OpaquePhase,
    transparent_phase: &TransparentPhase,
    model_matrices: &[[f32; 16]],
) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    setup.binding_index.hash(&mut hasher);
    setup.cascade_index.hash(&mut hasher);
    setup.resolution.hash(&mut hasher);
    setup.bias.to_bits().hash(&mut hasher);
    setup.radius.to_bits().hash(&mut hasher);
    setup.raster_bias.constant.hash(&mut hasher);
    setup.raster_bias.slope_scale.to_bits().hash(&mut hasher);
    setup.raster_bias.clamp.to_bits().hash(&mut hasher);
    setup.normal_bias.to_bits().hash(&mut hasher);
    setup.light_view_proj.hash_f32_array(&mut hasher);
    setup.receiver_depth_extent.to_bits().hash(&mut hasher);
    setup.caster_depth_extent.to_bits().hash(&mut hasher);
    setup.cascade_count.hash(&mut hasher);
    for split in setup.cascade_splits {
        split.to_bits().hash(&mut hasher);
    }
    setup.cascade_blend.to_bits().hash(&mut hasher);
    setup.sampling_mode.hash(&mut hasher);
    setup.update_policy.hash(&mut hasher);
    scene_view.shadow_cascade().hash(&mut hasher);
    scene_view
        .view_uniform
        .view_proj
        .hash_f32_array(&mut hasher);
    hash_shadow_phase_items(opaque_phase.items(), model_matrices, &mut hasher);
    hash_shadow_phase_items(transparent_phase.items(), model_matrices, &mut hasher);
    hasher.finish()
}

fn hash_shadow_phase_items(
    items: &[crate::render::phase::PhaseItem],
    model_matrices: &[[f32; 16]],
    hasher: &mut impl Hasher,
) {
    items.len().hash(&mut *hasher);
    for item in items {
        item.draw_function_id.hash(&mut *hasher);
        item.entity.hash(&mut *hasher);
        item.batch_key.hash(&mut *hasher);
        let draw = *item.data::<MeshDrawData>();
        draw.hash(&mut *hasher);
        if let Some(model) = model_matrices.get(draw.model_slot() as usize) {
            (*model).hash_f32_array(&mut *hasher);
        }
    }
}

trait HashF32Array {
    fn hash_f32_array(self, hasher: &mut impl Hasher);
}

impl<const N: usize> HashF32Array for [f32; N] {
    fn hash_f32_array(self, hasher: &mut impl Hasher) {
        for value in self {
            value.to_bits().hash(hasher);
        }
    }
}

fn resolved_cascade_count(light: DirectionalLight) -> u32 {
    light
        .cascade_count
        .clamp(1, MAX_DIRECTIONAL_SHADOW_CASCADES as u32)
}

fn resolved_cascade_splits(
    light: DirectionalLight,
    view: &SceneView,
    cascade_count: u32,
) -> [f32; MAX_DIRECTIONAL_SHADOW_CASCADES] {
    let mut splits = [view.far.max(view.near + 1.0); MAX_DIRECTIONAL_SHADOW_CASCADES];
    let near = view.near.max(0.0);
    let far = if view.far.is_finite() && view.far > near {
        view.far
    } else {
        near + 1.0
    };
    let active_count = cascade_count.clamp(1, MAX_DIRECTIONAL_SHADOW_CASCADES as u32) as usize;
    let mut previous = near;
    for (index, split) in splits.iter_mut().take(active_count).enumerate() {
        let fallback = near + (far - near) * ((index + 1) as f32 / active_count as f32);
        let requested = light.cascade_distances[index];
        let candidate = if requested.is_finite() && requested > previous {
            requested
        } else {
            fallback
        };
        *split = candidate.clamp(previous + 1e-4, far);
        previous = *split;
    }
    splits
}

fn cascade_frustum_corners_world(
    full_corners: &[[f32; 3]; 8],
    view: &SceneView,
    cascade_splits: &[f32; MAX_DIRECTIONAL_SHADOW_CASCADES],
    cascade_index: u32,
) -> [[f32; 3]; 8] {
    let near = view.near.max(0.0);
    let far = view.far.max(1e-4);
    let depth_span = (far - near).max(1e-4);
    let near_split = if cascade_index == 0 {
        0.0
    } else {
        (cascade_splits[cascade_index as usize - 1] - near) / depth_span
    }
    .clamp(0.0, 1.0);
    let far_split = (cascade_splits[cascade_index as usize].clamp(near, far) - near) / depth_span;
    let far_split = far_split.clamp(near_split, 1.0);

    let mut corners = [[0.0; 3]; 8];
    for corner in 0..4 {
        let near = full_corners[corner];
        let far = full_corners[corner + 4];
        corners[corner] = lerp_point(near, far, near_split);
        corners[corner + 4] = lerp_point(near, far, far_split);
    }
    corners
}

fn lerp_point(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn view_frustum_corners_world(view: &SceneView) -> Option<[[f32; 3]; 8]> {
    // Wicked builds directional shadow cascades from the camera with jitter removed.
    // Keep the shadow projection stable even when the main view is TAA-jittered.
    let inverse_view_proj = Mat4::from_cols_array(view.unjittered_view_proj_matrix).inverse();
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

fn light_view_matrix(light_direction: [f32; 3]) -> [f32; 16] {
    let up_guess = if light_direction[1].abs() > 0.98 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    Mat4::look_to_rh(
        Vec3::from_array([0.0, 0.0, 0.0]),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::render::view::ProjectionViewUniformExt;
    use crate::render::view::ViewportRect;

    fn project_point(view_proj: [f32; 16], point: [f32; 3]) -> [f32; 3] {
        let clip = Mat4::from_cols_array(view_proj)
            * Vec4::from_array([point[0], point[1], point[2], 1.0]);
        let inv_w = clip.w().recip();
        [clip.x() * inv_w, clip.y() * inv_w, clip.z() * inv_w]
    }

    fn assert_in_range(label: &str, value: f32, min: f32, max: f32, tolerance: f32) {
        assert!(
            value >= min - tolerance && value <= max + tolerance,
            "{label}: {value} not in [{min}, {max}]"
        );
    }

    #[test]
    fn directional_shadow_setup_resolves_fixed_cascade_contract() {
        let projection = Projection::perspective(60.0_f32.to_radians(), 0.1, 100.0);
        let main_view = SceneView::new(
            0,
            ViewportRect::from_surface_size([128, 128]),
            [128, 128],
            false,
            u32::MAX,
            Transform::default(),
            projection,
            projection.view_uniform(Transform::default(), [128, 128]),
            false,
        );
        let mut world = World::new();
        world.spawn((DirectionalLight::new([0.3, -1.0, 0.2])
            .cascade_count(3)
            .cascade_distances([12.0, 36.0, 90.0, 0.0])
            .cascade_blend(0.15)
            .shadow_resolution_per_cascade(512)
            .pcss_shadows(),));

        let mut views = vec![main_view];
        let setups = append_directional_shadow_views(&world, &mut views);

        assert_eq!(views.len(), 4);
        assert_eq!(setups.len(), 3);
        assert_eq!(setups[0].cascade_count, 3);
        assert_eq!(setups[0].cascade_index, 0);
        assert_eq!(setups[1].cascade_index, 1);
        assert_eq!(setups[2].cascade_index, 2);
        assert_eq!(views[1].shadow_cascade(), 0);
        assert_eq!(views[2].shadow_cascade(), 1);
        assert_eq!(views[3].shadow_cascade(), 2);
        assert_eq!(setups[0].cascade_splits, [12.0, 36.0, 90.0, 100.0]);
        assert_eq!(setups[0].cascade_blend, 0.15);
        assert_eq!(setups[0].resolution, 512);
        assert_eq!(setups[0].sampling_mode, ShadowSamplingMode::Pcss);
    }

    #[test]
    fn demo_directional_shadow_views_cover_their_receiver_cascades() {
        let camera_position = [0.0, 3.573_966, 10.239_987];
        let camera_rotation =
            crate::math::Quat::from_xyzw_array([-0.089_878_55, 0.0, 0.0, 0.995_952_7]);
        let camera_transform =
            Transform::from_xyz(camera_position[0], camera_position[1], camera_position[2])
                .with_rotation_quat(camera_rotation);
        let projection = Projection::perspective(55.0_f32.to_radians(), 0.1, 80.0);
        let main_view = SceneView::new(
            0,
            ViewportRect::from_surface_size([1280, 720]),
            [1280, 720],
            false,
            u32::MAX,
            camera_transform,
            projection,
            projection.view_uniform(camera_transform, [1280, 720]),
            false,
        );
        let mut world = World::new();
        world.spawn((DirectionalLight::new([0.58, -1.0, 0.34])
            .cascade_count(4)
            .cascade_distances([5.5, 13.0, 30.0, 80.0])
            .shadow_resolution_per_cascade(2048)
            .radius(0.055)
            .shadow_bias(0.0008)
            .shadow_depth_bias(3)
            .shadow_slope_bias(1.8)
            .shadow_normal_bias(0.002)
            .pcss_shadows(),));

        let mut views = vec![main_view];
        let setups = append_directional_shadow_views(&world, &mut views);
        assert_eq!(setups.len(), 4);
        assert_eq!(views.len(), 5);

        let full_corners = view_frustum_corners_world(&main_view)
            .expect("demo main view should have invertible view projection");
        for cascade_index in 0..4 {
            let corners = cascade_frustum_corners_world(
                &full_corners,
                &main_view,
                &setups[0].cascade_splits,
                cascade_index,
            );
            let shadow_view = views
                .iter()
                .find(|view| view.is_shadow() && view.shadow_cascade() == cascade_index)
                .copied()
                .expect("shadow view should exist for cascade");
            for (corner_index, corner) in corners.iter().copied().enumerate() {
                let ndc = project_point(shadow_view.view_uniform.view_proj, corner);
                assert_in_range(
                    &format!("cascade {cascade_index} corner {corner_index} x"),
                    ndc[0],
                    -1.0,
                    1.0,
                    0.002,
                );
                assert_in_range(
                    &format!("cascade {cascade_index} corner {corner_index} y"),
                    ndc[1],
                    -1.0,
                    1.0,
                    0.002,
                );
                assert_in_range(
                    &format!("cascade {cascade_index} corner {corner_index} z"),
                    ndc[2],
                    0.0,
                    1.0,
                    0.002,
                );
                assert!(
                    shadow_view.frustum().intersects_sphere(corner, 0.01),
                    "cascade {cascade_index} corner {corner_index} should pass shadow frustum culling"
                );
            }
        }
    }

    #[test]
    fn cascade_frustum_splits_are_relative_to_camera_near_plane() {
        let projection = Projection::perspective(60.0_f32.to_radians(), 1.0, 11.0);
        let main_view = SceneView::new(
            0,
            ViewportRect::from_surface_size([128, 128]),
            [128, 128],
            false,
            u32::MAX,
            Transform::default(),
            projection,
            projection.view_uniform(Transform::default(), [128, 128]),
            false,
        );
        let full_corners = view_frustum_corners_world(&main_view)
            .expect("test view should have invertible view projection");
        let cascade_splits = [1.0, 6.0, 11.0, 11.0];

        let first = cascade_frustum_corners_world(&full_corners, &main_view, &cascade_splits, 0);
        let second = cascade_frustum_corners_world(&full_corners, &main_view, &cascade_splits, 1);

        for corner in 0..4 {
            assert_eq!(
                first[corner + 4],
                full_corners[corner],
                "a split at the camera near plane should not advance into the frustum"
            );
            assert_eq!(
                second[corner], full_corners[corner],
                "the next cascade should start exactly at the previous near-plane split"
            );
        }
    }

    #[test]
    fn directional_shadow_corners_ignore_taa_jittered_view_projection() {
        let projection = Projection::perspective(60.0_f32.to_radians(), 0.1, 100.0);
        let transform = Transform::from_xyz(0.0, 1.0, 8.0);
        let mut main_view = SceneView::new(
            0,
            ViewportRect::from_surface_size([128, 128]),
            [128, 128],
            false,
            u32::MAX,
            transform,
            projection,
            projection.view_uniform(transform, [128, 128]),
            false,
        );
        let unjittered_corners =
            view_frustum_corners_world(&main_view).expect("main view should produce corners");
        let mut jittered_uniform = main_view.view_uniform;
        jittered_uniform.view_proj[12] += 0.25;
        jittered_uniform.view_proj[13] -= 0.25;
        main_view.set_jittered_view_uniform(jittered_uniform);

        assert_ne!(
            main_view.view_uniform.view_proj,
            main_view.unjittered_view_proj_matrix
        );
        assert_eq!(
            view_frustum_corners_world(&main_view)
                .expect("jittered main view should produce corners"),
            unjittered_corners
        );
    }

    #[test]
    fn directional_shadow_view_keeps_light_ray_casters_outside_receiver_slice() {
        let projection = Projection::perspective(60.0_f32.to_radians(), 0.1, 120.0);
        let main_view = SceneView::new(
            0,
            ViewportRect::from_surface_size([256, 256]),
            [256, 256],
            false,
            u32::MAX,
            Transform::default(),
            projection,
            projection.view_uniform(Transform::default(), [256, 256]),
            false,
        );
        let light = DirectionalLight::new([0.35, -1.0, -0.25])
            .cascade_count(4)
            .cascade_distances([8.0, 24.0, 60.0, 120.0])
            .shadow_resolution_per_cascade(512);

        let full_corners = view_frustum_corners_world(&main_view)
            .expect("test view should have invertible view projection");
        let cascade_splits =
            resolved_cascade_splits(light, &main_view, resolved_cascade_count(light));
        let corners = cascade_frustum_corners_world(&full_corners, &main_view, &cascade_splits, 0);
        let mut center_world = [0.0; 3];
        let light_view = Mat4::from_cols_array(light_view_matrix(
            Vec3::from_array(light.direction)
                .try_normalized()
                .expect("test light direction should normalize")
                .to_array(),
        ));
        let mut center_light_z = 0.0;
        let mut min_light_z = f32::INFINITY;
        for corner in corners {
            center_world[0] += corner[0];
            center_world[1] += corner[1];
            center_world[2] += corner[2];
            let light_space = light_view
                .transform_point3(Vec3::from_array(corner))
                .to_array();
            center_light_z += light_space[2];
            min_light_z = min_light_z.min(light_space[2]);
        }
        for axis in &mut center_world {
            *axis *= 0.125;
        }
        center_light_z *= 0.125;
        let previous_receiver_depth_extent = (center_light_z - min_light_z).abs() * 4.0;

        let (shadow_view, setup) =
            build_shadow_view(&main_view, light, 0, 0).expect("cascade 0 should build");
        assert!(
            setup.caster_depth_extent > setup.receiver_depth_extent + 8.0,
            "caster culling range should be wider than the tight receiver slice"
        );
        assert!(
            setup.receiver_depth_extent >= previous_receiver_depth_extent - 0.001,
            "sampling projection should keep the receiver slice depth range"
        );

        let light_direction = Vec3::from_array(light.direction)
            .try_normalized()
            .expect("test light direction should normalize");
        let caster_offset =
            (previous_receiver_depth_extent + 4.0).min(setup.caster_depth_extent - 1.0);
        let caster_center = Vec3::from_array(center_world) - light_direction * caster_offset;
        assert!(
            shadow_view
                .frustum()
                .intersects_sphere(caster_center.to_array(), 0.5),
            "casters between the light and the receiver slice must survive shadow-view culling"
        );
    }

    #[test]
    fn packed_shadow_atlas_mul_add_matches_wicked_cascade_mapping() {
        let layout = ShadowAtlasLayout::directional_packed(512, 4, 1.0);
        let mul_add = layout.shadow_atlas_mul_add();
        let atlas_rcp = layout.shadow_atlas_resolution_rcp();

        assert_eq!(mul_add, [0.25, 1.0, 0.0, 0.0]);
        assert_eq!(atlas_rcp, [1.0 / 2048.0, 1.0 / 512.0, 1.0, 0.0]);
        assert_eq!(
            shadow_atlas_resolution_rcp_with_sampling_mode(layout, ShadowSamplingMode::DitheredPcf),
            [1.0 / 2048.0, 1.0 / 512.0, 1.0, 1.0]
        );

        for cascade in 0..4 {
            let left = ((0.0 + cascade as f32) * mul_add[0]) + mul_add[2];
            let right = ((1.0 + cascade as f32) * mul_add[0]) + mul_add[2];
            assert_eq!(left, cascade as f32 * 0.25);
            assert_eq!(right, (cascade + 1) as f32 * 0.25);
        }
    }

    #[test]
    fn static_shadow_update_policy_only_marks_changed_cascades() {
        let previous = [11, 22, 33, 44];
        let same = previous;
        let changed = [11, 99, 33, 88];

        assert_eq!(
            cascade_update_mask(
                ShadowUpdatePolicy::StaticWhenUnchanged,
                false,
                4,
                &previous,
                &same
            ),
            0
        );
        assert_eq!(
            cascade_update_mask(
                ShadowUpdatePolicy::StaticWhenUnchanged,
                false,
                4,
                &previous,
                &changed
            ),
            0b1010
        );
        assert_eq!(
            cascade_update_mask(ShadowUpdatePolicy::EveryFrame, false, 3, &previous, &same),
            0b0111
        );
    }
}
