use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::math::{Mat4, Vec3, Vec4};
use crate::render::gpu::{RenderTarget, RenderTargetDescriptor};
use crate::render::phase::{MeshDrawData, OpaquePhase, TransparentPhase};
use crate::render::view::{Projection, SceneView, SceneViewKind};
use crate::render::LightTable;
use crate::render::{DirectionalLight, ShadowSamplingMode, ShadowUpdatePolicy, Transform};
use crate::render::{RenderDebugView, MAX_DIRECTIONAL_SHADOW_CASCADES};

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

mod cascade;
#[cfg(test)]
mod tests;

pub(crate) use cascade::*;

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

#[derive(Default)]
pub(crate) struct DirectionalShadowViewScratch {
    lights: Vec<DirectionalLight>,
    shadow_views: Vec<SceneView>,
}

#[cfg(test)]
pub(crate) fn append_directional_shadow_views(
    world: &World,
    views: &mut Vec<SceneView>,
) -> Vec<DirectionalShadowSetup> {
    let mut setups = Vec::new();
    let mut scratch = DirectionalShadowViewScratch::default();
    append_directional_shadow_views_into(world, views, &mut setups, &mut scratch);
    setups
}

pub(crate) fn append_directional_shadow_views_into(
    world: &World,
    views: &mut Vec<SceneView>,
    setups: &mut Vec<DirectionalShadowSetup>,
    scratch: &mut DirectionalShadowViewScratch,
) {
    let base_view_count = views.len();
    setups.clear();
    scratch.lights.clear();
    scratch.shadow_views.clear();
    let query = world.query::<&DirectionalLight>();
    query.for_each(|light| {
        if light.visible && light.casts_shadows {
            scratch.lights.push(*light);
        }
    });

    for (binding_index, view_slot) in views.iter_mut().take(base_view_count).enumerate() {
        let view = *view_slot;
        if view.is_shadow() {
            continue;
        }

        *view_slot = view.with_shadow_binding(binding_index);
        let Some(light) = select_shadow_light(&scratch.lights, view.layer_mask) else {
            continue;
        };
        let cascade_count = resolved_cascade_count(light);
        for cascade_index in 0..cascade_count {
            let Some((shadow_view, setup)) =
                build_shadow_view(&view, light, binding_index, cascade_index)
            else {
                continue;
            };
            scratch.shadow_views.push(shadow_view);
            setups.push(setup);
        }
    }
    views.append(&mut scratch.shadow_views);
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

    for (binding_index, shadow_view) in shadow_views.iter_mut().enumerate() {
        let log_shadow_details = shadow_sync_log_index();
        let Some(setup) = shadow_setups
            .iter()
            .copied()
            .find(|setup| setup.binding_index == binding_index)
        else {
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
            // Directional cascades are a fixed, very small set. Linear scans
            // avoid constructing three hash maps every frame and are cheaper
            // for the normal one-camera/four-cascade case.
            let Some((view_index, scene_view)) =
                views.iter().copied().enumerate().find(|(_, view)| {
                    view.kind == SceneViewKind::DirectionalShadow
                        && view.shadow_binding() == Some(binding_index)
                        && view.shadow_cascade() == cascade_index
                })
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
            let Some(cascade_setup) = shadow_setups.iter().copied().find(|candidate| {
                candidate.binding_index == binding_index && candidate.cascade_index == cascade_index
            }) else {
                missing_cascade = true;
                break;
            };
            if let Some(log_index) = log_shadow_details {
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
