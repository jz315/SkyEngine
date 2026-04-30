use crate::asset::AssetServer;
use crate::diagnostics::Diagnostics;
use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::component::{
    DirectionalLight, PointLight, RenderSettings, SpotLight, Transform,
};
use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::extract::ExtractContext;
use crate::render::gi::DdgiRuntime;
use crate::render::lighting::shadow::{
    append_directional_shadow_views, sync_shadow_views, ShadowDebugResources,
};
use crate::render::lighting::Light2D;
use crate::render::phase::{MeshDrawData, OpaquePhase, PhaseItem, TransparentPhase};
use crate::render::resources::material::SpriteMaterial;
use crate::render::view::{fallback_scene_view, RenderStats, SceneView};
use crate::render::{GpuLight, LightTable, ModelMatrixTable};
use rustc_hash::FxHashMap;

use super::{composer::RenderComposer, elapsed_ms, timing_start, RenderTimingStats};

pub(crate) struct PreviousModelMatrices(pub(crate) Vec<[f32; 16]>);

pub(crate) struct PreparedFrameBuilder {
    surface_format: wgpu::TextureFormat,
    has_surface: bool,
}

impl PreparedFrameBuilder {
    #[inline]
    pub(crate) fn new(gpu: &GpuContext) -> Self {
        Self {
            surface_format: gpu.surface_format(),
            has_surface: gpu.has_surface(),
        }
    }

    pub(crate) fn finalize_views(
        mut views: Vec<SceneView>,
        surface_size: [u32; 2],
    ) -> Vec<SceneView> {
        if views.is_empty() {
            views.push(fallback_scene_view(surface_size));
        }
        views.sort_by_key(|view| (view.order, if view.presents_to_surface() { 1 } else { 0 }));
        let mut cleared_surface = false;
        for (index, view) in views.iter_mut().enumerate() {
            view.set_execution_order(index as i32);
            view.clear_surface = !cleared_surface && view.presents_to_surface();
            if view.clear_surface {
                cleared_surface = true;
            }
            if view.target_size[0] == 0 || view.target_size[1] == 0 {
                view.target_size = view.viewport.size();
            }
        }
        views
    }
}

impl RenderComposer {
    pub fn render_world(&mut self, gpu: &mut GpuContext, world: &World) {
        self.ensure_registered_materials(gpu);
        self.ensure_builtin_meshes(gpu);
        self.ensure_phase_runtime(gpu);
        self.ensure_shadow_runtime(gpu);
        {
            let pipeline_cache = self.material_pipeline_cache_mut();
            pipeline_cache.new_frame();
            pipeline_cache.garbage_collect();
        }
        self.runtime.surface_size = gpu.surface_size();
        self.runtime.history.begin_frame(gpu);
        self.runtime.frame_settings = world
            .get_resource::<RenderSettings>()
            .copied()
            .unwrap_or_default();
        self.runtime.render_assets.begin_frame();
        if let Some(sprite_materials) = self
            .resources
            .material_registry
            .try_materials_mut::<SpriteMaterial>()
        {
            sprite_materials.clear();
        }

        let resolved_transforms = self.resolve_scene_transforms(world);
        let frame_start = timing_start();

        let mut views = self.collect_world_views(world, &resolved_transforms);
        for feature in &mut self.plan.runtime_features {
            feature.extract(world, &resolved_transforms, self.runtime.surface_size);
        }
        for feature in &self.plan.runtime_features {
            feature.collect_views(&mut views);
        }
        let shadow_setups = append_directional_shadow_views(world, &mut views);
        let mut views = PreparedFrameBuilder::finalize_views(views, self.runtime.surface_size);
        self.runtime.temporal.update_views(
            &mut views,
            self.runtime.frame_settings.temporal_aa.enabled,
            self.runtime.frame_settings.temporal_aa.jitter_scale,
        );

        for feature in &mut self.plan.runtime_features {
            feature.prepare(gpu, &views);
        }

        let quad_mesh_handle = self.resources.mesh_registry.ensure_builtin_quad(gpu);
        let asset_server = world.get_resource::<AssetServer>().cloned();
        if let Some(asset_server) = asset_server.as_ref() {
            for event in asset_server.events_since(&mut self.runtime.asset_event_cursor) {
                self.runtime.render_assets.handle_asset_event(event);
            }
        }

        let mut opaque_phases = Vec::with_capacity(views.len());
        let mut transparent_phases = Vec::with_capacity(views.len());
        for (_view_index, view) in views.iter().enumerate() {
            let mut opaque_phase = OpaquePhase::new();
            let mut transparent_phase = TransparentPhase::new();
            for extractor in &mut self.plan.extractors {
                extractor
                    .extract(
                        world,
                        &resolved_transforms,
                        view,
                        &mut ExtractContext {
                            gpu,
                            asset_server: asset_server.as_ref(),
                            render_assets: &mut self.runtime.render_assets,
                            material_registry: &mut self.resources.material_registry,
                            mesh_registry: &self.resources.mesh_registry,
                            opaque_phase: &mut opaque_phase,
                            transparent_phase: &mut transparent_phase,
                            quad_mesh_handle,
                        },
                    )
                    .expect("registered extractor should succeed");
            }
            for feature in &self.plan.runtime_features {
                feature.append_phase_items(_view_index, &mut opaque_phase, &mut transparent_phase);
            }
            opaque_phase.sort();
            transparent_phase.sort();
            opaque_phases.push(opaque_phase);
            transparent_phases.push(transparent_phase);
        }

        let mut entity_to_model_slot = FxHashMap::default();
        let mut model_matrices = vec![IDENTITY_MODEL_MATRIX];
        for phase in &mut opaque_phases {
            self.resources.draw_functions.assign_model_matrices(
                phase.items_mut(),
                &resolved_transforms,
                &mut entity_to_model_slot,
                &mut model_matrices,
            );
        }
        for phase in &mut transparent_phases {
            self.resources.draw_functions.assign_model_matrices(
                phase.items_mut(),
                &resolved_transforms,
                &mut entity_to_model_slot,
                &mut model_matrices,
            );
        }

        let mut previous_model_matrices = model_matrices.clone();
        for (entity, slot) in &entity_to_model_slot {
            if let Some(previous) = self.runtime.previous_model_by_entity.get(entity) {
                previous_model_matrices[*slot as usize] = *previous;
            }
        }
        let previous_model_matrices = PreviousModelMatrices(previous_model_matrices);

        let lights = collect_gpu_lights(world, &resolved_transforms);
        {
            let gpu_scene = self
                .runtime
                .gpu_scene
                .as_mut()
                .expect("phase runtime should initialize a GpuScene");
            gpu_scene
                .table_mut::<ModelMatrixTable>()
                .set_all(gpu, &model_matrices);
            gpu_scene.table_mut::<LightTable>().set_all(gpu, &lights);
            gpu_scene.upload_all(gpu.queue());
        }
        {
            let ddgi = self
                .runtime
                .ddgi
                .get_or_insert_with(|| DdgiRuntime::new(gpu));
            ddgi.prepare(
                gpu,
                self.runtime.frame_settings.global_illumination,
                &views,
                &opaque_phases,
                &self.resources.draw_functions,
                &model_matrices,
                &lights,
                &self.resources.material_registry,
                &self.resources.mesh_registry,
                self.runtime.frame_settings.ambient_color,
            );
        }
        let ddgi_resources = self
            .runtime
            .ddgi
            .as_ref()
            .expect("DDGI runtime should be initialized before shadow bindings")
            .scene_resources();
        sync_shadow_views(
            &mut self.shadows.views,
            gpu,
            &views,
            &opaque_phases,
            &transparent_phases,
            &model_matrices,
            &shadow_setups,
            self.shadows
                .layout
                .as_ref()
                .expect("shadow runtime should initialize a shared shadow layout"),
            self.shadows
                .pass_layout
                .as_ref()
                .expect("shadow runtime should initialize a shared shadow-pass layout"),
            self.shadows
                .compare_sampler
                .as_ref()
                .expect("shadow runtime should initialize a shared shadow sampler"),
            self.runtime
                .gpu_scene
                .as_ref()
                .expect("phase runtime should initialize a GpuScene")
                .table::<LightTable>(),
            ddgi_resources,
            self.runtime.frame_settings.debug_view,
        );
        let shadow_debug_resources = self
            .shadows
            .views
            .iter()
            .find_map(ShadowDebugResources::from_directional_shadow);
        let shadow_stats = collect_shadow_stats(&self.shadows.views);
        let shadow_draw_calls = count_shadow_draw_calls(
            &views,
            &opaque_phases,
            &transparent_phases,
            &self.shadows.views,
        );

        let mut pipeline = self.build_runtime_pipeline(gpu);
        let render_asset_stats = self
            .runtime
            .render_assets
            .finish_frame(world.get_resource::<Diagnostics>());
        let execution = {
            let builder = PreparedFrameBuilder::new(gpu);
            let gpu_scene = self
                .runtime
                .gpu_scene
                .as_ref()
                .expect("phase runtime should initialize a GpuScene");
            let mut frame = PreparedFrame::new(builder.surface_format, builder.has_surface);
            let _ = frame.insert_payload(&self.runtime.frame_settings);
            let _ = frame.insert_payload(&self.runtime.history);
            let _ = frame.insert_payload(gpu_scene);
            let _ = frame.insert_payload(&model_matrices);
            let _ = frame.insert_payload(&previous_model_matrices);
            let _ = frame.insert_payload(
                self.runtime
                    .ddgi
                    .as_ref()
                    .expect("DDGI runtime should be initialized before frame build"),
            );
            let _ = frame.insert_payload(
                self.shadows
                    .layout
                    .as_ref()
                    .expect("shadow runtime should initialize a shared shadow layout"),
            );
            let _ = frame.insert_payload(
                self.shadows
                    .pass_layout
                    .as_ref()
                    .expect("shadow runtime should initialize a shared shadow-pass layout"),
            );
            if let Some(shadow_debug_resources) = shadow_debug_resources.as_ref() {
                let _ = frame.insert_payload(shadow_debug_resources);
            }
            for feature in &self.plan.runtime_features {
                feature.insert_frame_payloads(&mut frame);
            }

            for (view_index, view) in views.iter().enumerate() {
                let mut prepared_view = PreparedView::new(
                    view.execution_order(),
                    view.viewport,
                    view.target_size,
                    view.clear_surface,
                );
                prepared_view.set_history_key(view.history_key());
                let _ = prepared_view.insert_payload(view);
                let _ = prepared_view.insert_payload(&opaque_phases[view_index]);
                let _ = prepared_view.insert_payload(&transparent_phases[view_index]);
                if let Some(binding_index) = view.shadow_binding() {
                    let _ = prepared_view.insert_payload(&self.shadows.views[binding_index]);
                }
                for feature in &self.plan.runtime_features {
                    feature.insert_view_payloads(view_index, view, &mut prepared_view);
                }
                frame.add_view(prepared_view);
            }

            let execute_start = timing_start();
            let execution = pipeline.execute_frame(gpu, &frame);
            let execute_ms = elapsed_ms(execute_start);
            (execution, execute_ms)
        };

        self.runtime.last_stats = RenderStats {
            step_count: self.plan.steps.len(),
            view_count: views.len(),
            light_count: lights.len(),
            draw_calls: execution.0.draw_calls,
            shadow_cascade_count: shadow_stats.cascade_count,
            shadow_caster_count: shadow_stats.caster_count,
            shadow_caster_count_by_cascade: shadow_stats.caster_count_by_cascade,
            shadow_draw_calls,
            shadow_draw_calls_by_cascade: count_shadow_draw_calls_by_cascade(
                &views,
                &opaque_phases,
                &transparent_phases,
                &self.shadows.views,
            ),
            shadow_atlas_width: shadow_stats.atlas_size[0],
            shadow_atlas_height: shadow_stats.atlas_size[1],
            shadow_atlas_rect_count: shadow_stats.rect_count,
            shadow_atlas_used_pixel_ratio: shadow_stats.used_pixel_ratio,
            shadow_atlas_guard_band_texels: shadow_stats.guard_band_texels,
            passes: execution.0.passes,
            resident_render_assets: render_asset_stats.resident_assets,
            uploaded_render_assets: render_asset_stats.uploaded_assets,
            loading_render_assets: render_asset_stats.loading_assets,
            missing_render_assets: render_asset_stats.missing_assets,
            failed_render_assets: render_asset_stats.failed_assets,
            timings: RenderTimingStats {
                frame_ms: elapsed_ms(frame_start),
                execute_ms: execution.1,
                ..Default::default()
            },
            ..Default::default()
        };

        self.runtime.previous_model_by_entity.clear();
        self.runtime.previous_model_by_entity.extend(
            resolved_transforms
                .iter()
                .map(|(entity, transform)| (entity, transform.to_matrix4().to_cols_array())),
        );
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct FrameShadowStats {
    cascade_count: usize,
    caster_count: usize,
    caster_count_by_cascade: [usize; crate::render::component::MAX_DIRECTIONAL_SHADOW_CASCADES],
    atlas_size: [u32; 2],
    rect_count: usize,
    used_pixel_ratio: f32,
    guard_band_texels: f32,
}

fn collect_shadow_stats(
    shadow_views: &[crate::render::lighting::shadow::ShadowViewBinding],
) -> FrameShadowStats {
    let mut stats = FrameShadowStats::default();
    let mut atlas_area = 0u64;
    let mut used_area = 0f64;

    for shadow_view in shadow_views.iter().filter(|shadow| shadow.enabled()) {
        let atlas = shadow_view.atlas_stats();
        stats.cascade_count += atlas.active_cascade_count as usize;
        stats.caster_count += shadow_view.caster_count();
        for (index, count) in shadow_view
            .caster_count_by_cascade()
            .into_iter()
            .enumerate()
        {
            stats.caster_count_by_cascade[index] += count;
        }
        stats.rect_count += atlas.used_rect_count as usize;
        stats.atlas_size[0] = stats.atlas_size[0].max(atlas.atlas_size[0]);
        stats.atlas_size[1] = stats.atlas_size[1].max(atlas.atlas_size[1]);
        stats.guard_band_texels = stats.guard_band_texels.max(atlas.guard_band_texels);
        let area = atlas.atlas_size[0] as u64 * atlas.atlas_size[1] as u64;
        atlas_area += area;
        used_area += atlas.used_pixel_ratio as f64 * area as f64;
    }

    if atlas_area > 0 {
        stats.used_pixel_ratio = (used_area / atlas_area as f64) as f32;
    }

    stats
}

fn count_shadow_draw_calls(
    views: &[SceneView],
    opaque_phases: &[OpaquePhase],
    transparent_phases: &[TransparentPhase],
    shadow_views: &[crate::render::lighting::shadow::ShadowViewBinding],
) -> usize {
    views
        .iter()
        .enumerate()
        .filter(|(_, view)| view.is_shadow())
        .filter(|(_, view)| {
            view.shadow_binding()
                .and_then(|binding| shadow_views.get(binding))
                .is_some_and(|shadow| {
                    shadow.enabled() && shadow.should_update_cascade(view.shadow_cascade())
                })
        })
        .map(|(index, _)| {
            opaque_phases
                .get(index)
                .map_or(0, |phase| count_shadow_batches(phase.items()))
                + transparent_phases
                    .get(index)
                    .map_or(0, |phase| count_shadow_batches(phase.items()))
        })
        .sum()
}

fn count_shadow_draw_calls_by_cascade(
    views: &[SceneView],
    opaque_phases: &[OpaquePhase],
    transparent_phases: &[TransparentPhase],
    shadow_views: &[crate::render::lighting::shadow::ShadowViewBinding],
) -> [usize; crate::render::component::MAX_DIRECTIONAL_SHADOW_CASCADES] {
    let mut draw_calls = [0usize; crate::render::component::MAX_DIRECTIONAL_SHADOW_CASCADES];
    for (view_index, view) in views
        .iter()
        .enumerate()
        .filter(|(_, view)| view.is_shadow())
    {
        let cascade = view.shadow_cascade() as usize;
        if cascade >= draw_calls.len() {
            continue;
        }
        let Some(shadow_view) = view
            .shadow_binding()
            .and_then(|binding| shadow_views.get(binding))
        else {
            continue;
        };
        if !shadow_view.enabled() || !shadow_view.should_update_cascade(view.shadow_cascade()) {
            continue;
        }
        draw_calls[cascade] += opaque_phases
            .get(view_index)
            .map_or(0, |phase| count_shadow_batches(phase.items()))
            + transparent_phases
                .get(view_index)
                .map_or(0, |phase| count_shadow_batches(phase.items()));
    }
    draw_calls
}

fn count_shadow_batches(items: &[PhaseItem]) -> usize {
    let mut draws = 0usize;
    let mut cursor = 0usize;
    while cursor < items.len() {
        let base = *items[cursor].data::<MeshDrawData>();
        let base_draw_function = items[cursor].draw_function_id;
        let base_material = base.material_handle::<crate::render::StandardMaterial>();
        let mut batch_end = cursor + 1;
        while batch_end < items.len() {
            let next = *items[batch_end].data::<MeshDrawData>();
            let next_draw_function = items[batch_end].draw_function_id;
            if next.mesh_handle() != base.mesh_handle()
                || next.sub_mesh_index() != base.sub_mesh_index()
                || next_draw_function != base_draw_function
                || next.material_handle::<crate::render::StandardMaterial>() != base_material
            {
                break;
            }
            batch_end += 1;
        }
        draws += 1;
        cursor = batch_end;
    }
    draws
}

fn collect_gpu_lights(
    world: &World,
    transforms: &crate::render::view::ResolvedSceneTransforms,
) -> Vec<GpuLight> {
    let mut lights = Vec::new();
    let mut point_lights = world.query::<(&Transform, &PointLight)>();
    point_lights.for_each_with_entity(world, |entity, (transform, light)| {
        if !light.visible {
            return;
        }
        let transform = transforms.get(entity).unwrap_or(*transform);
        let light = Light2D::new(transform.x(), transform.y(), light.radius)
            .intensity(light.intensity)
            .color(light.color)
            .temperature(light.temperature)
            .falloff(light.falloff);
        lights.push(GpuLight {
            pos_radius: [
                light.position[0],
                light.position[1],
                transform.z(),
                light.radius,
            ],
            color: light.effective_color(),
            falloff: [light.falloff.max(0.001), 0.0, 0.0, 0.0],
            dir_shadow: [0.0, 0.0, 0.0, -1.0],
        });
    });
    let mut spot_lights = world.query::<(&Transform, &SpotLight)>();
    spot_lights.for_each_with_entity(world, |entity, (transform, light)| {
        if !light.visible {
            return;
        }
        let transform = transforms.get(entity).unwrap_or(*transform);
        let direction = normalized_or(light.direction, [0.0, -1.0, 0.0]);
        let [inner_cos, outer_cos] = light.resolved_cone_cosines();
        let light_2d = Light2D::new(transform.x(), transform.y(), light.radius)
            .intensity(light.intensity)
            .color(light.color)
            .temperature(light.temperature)
            .falloff(light.falloff);
        lights.push(GpuLight {
            pos_radius: [
                light_2d.position[0],
                light_2d.position[1],
                transform.z(),
                light_2d.radius,
            ],
            color: light_2d.effective_color(),
            falloff: [light.falloff.max(0.001), 2.0, inner_cos, outer_cos],
            dir_shadow: [direction[0], direction[1], direction[2], -1.0],
        });
    });
    let mut directional_lights = world.query::<&DirectionalLight>();
    directional_lights.for_each(world, |light| {
        if !light.visible {
            return;
        }
        let dir = normalized_or(light.direction, [0.0, -1.0, 0.0]);
        lights.push(GpuLight {
            pos_radius: [dir[0], dir[1], dir[2], 0.0],
            color: [
                light.color.r * light.intensity,
                light.color.g * light.intensity,
                light.color.b * light.intensity,
                light.color.a,
            ],
            falloff: [0.0, 1.0, 0.0, 0.0],
            dir_shadow: [0.0, 0.0, 0.0, -1.0],
        });
    });
    lights
}

fn normalized_or(direction: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let len_sq =
        direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2];
    if len_sq <= f32::EPSILON {
        fallback
    } else {
        let inv_len = len_sq.sqrt().recip();
        [
            direction[0] * inv_len,
            direction[1] * inv_len,
            direction[2] * inv_len,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::render::component::{DirectionalLight, PointLight, SpotLight};
    use crate::render::view::ResolvedSceneTransforms;
    use crate::render::{Color, GpuLightKind};

    #[test]
    fn collect_gpu_lights_uploads_spot_cone_records() {
        let mut world = World::new();
        world.spawn((
            Transform::from_xyz(1.0, 2.0, 3.0),
            PointLight::new(4.0).intensity(0.5),
        ));
        world.spawn((
            Transform::from_xyz(-1.0, 6.0, 2.0),
            SpotLight::new(9.0)
                .color(Color::rgb(0.5, 0.75, 1.0))
                .direction([0.0, -2.0, 0.0])
                .cone_angles(0.25, 0.5),
        ));
        world.spawn((DirectionalLight::new([0.0, -1.0, 0.0]),));

        let lights = collect_gpu_lights(&world, &ResolvedSceneTransforms::default());

        assert_eq!(lights.len(), 3);
        assert_eq!(lights[0].kind(), GpuLightKind::Point);
        assert_eq!(lights[1].kind(), GpuLightKind::Spot);
        assert_eq!(lights[1].pos_radius, [-1.0, 6.0, 2.0, 9.0]);
        assert_eq!(lights[1].dir_shadow, [0.0, -1.0, 0.0, -1.0]);
        assert!(lights[1].falloff[2] > lights[1].falloff[3]);
        assert_eq!(lights[2].kind(), GpuLightKind::Directional);
    }
}

const IDENTITY_MODEL_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];
