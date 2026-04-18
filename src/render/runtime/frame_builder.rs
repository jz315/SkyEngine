use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::component::{DirectionalLight, PointLight, RenderSettings, Transform};
use crate::render::execution::{PreparedFrame, PreparedView};
use crate::render::extract::ExtractContext;
use crate::render::lighting::shadow::{append_directional_shadow_views, sync_shadow_views};
use crate::render::lighting::Light2D;
use crate::render::phase::{OpaquePhase, TransparentPhase};
use crate::render::postfx::global_illumination::build_view_gi_probe_grid_payloads;
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
        self.runtime.frame_settings = world
            .get_resource::<RenderSettings>()
            .copied()
            .unwrap_or_default();

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
        let views = PreparedFrameBuilder::finalize_views(views, self.runtime.surface_size);

        for feature in &mut self.plan.runtime_features {
            feature.prepare(gpu, &views);
        }

        let quad_mesh_handle = self.resources.mesh_registry.ensure_builtin_quad(gpu);

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
        sync_shadow_views(
            &mut self.shadows.views,
            gpu,
            &views,
            &opaque_phases,
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
        );

        let gi_payloads = if self.runtime.frame_settings.global_illumination.enabled {
            Some(build_view_gi_probe_grid_payloads(
                &views,
                &opaque_phases,
                &self.resources.draw_functions,
                &model_matrices,
                &lights,
                &self.resources.material_registry,
                &self.resources.mesh_registry,
                self.runtime.frame_settings.global_illumination,
                self.runtime.frame_settings.ambient_color,
            ))
        } else {
            None
        };

        let mut pipeline = self.build_runtime_pipeline(gpu);
        let execution = {
            let builder = PreparedFrameBuilder::new(gpu);
            let gpu_scene = self
                .runtime
                .gpu_scene
                .as_ref()
                .expect("phase runtime should initialize a GpuScene");
            let mut frame = PreparedFrame::new(builder.surface_format, builder.has_surface);
            let _ = frame.insert_payload(&self.runtime.frame_settings);
            let _ = frame.insert_payload(gpu_scene);
            let _ = frame.insert_payload(&model_matrices);
            let _ = frame.insert_payload(&previous_model_matrices);
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
                let _ = prepared_view.insert_payload(view);
                let _ = prepared_view.insert_payload(&opaque_phases[view_index]);
                let _ = prepared_view.insert_payload(&transparent_phases[view_index]);
                if let Some(binding_index) = view.shadow_binding() {
                    let _ = prepared_view.insert_payload(&self.shadows.views[binding_index]);
                }
                if let Some(gi_payloads) = gi_payloads.as_ref() {
                    let _ = prepared_view.insert_payload(&gi_payloads[view_index]);
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
            passes: execution.0.passes,
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
        });
    });
    let mut directional_lights = world.query::<&DirectionalLight>();
    directional_lights.for_each(world, |light| {
        if !light.visible {
            return;
        }
        let dir = {
            let len_sq = light.direction[0] * light.direction[0]
                + light.direction[1] * light.direction[1]
                + light.direction[2] * light.direction[2];
            if len_sq <= f32::EPSILON {
                [0.0, -1.0, 0.0]
            } else {
                let inv_len = len_sq.sqrt().recip();
                [
                    light.direction[0] * inv_len,
                    light.direction[1] * inv_len,
                    light.direction[2] * inv_len,
                ]
            }
        };
        lights.push(GpuLight {
            pos_radius: [dir[0], dir[1], dir[2], 0.0],
            color: [
                light.color.r * light.intensity,
                light.color.g * light.intensity,
                light.color.b * light.intensity,
                light.color.a,
            ],
            falloff: [0.0, 1.0, 0.0, 0.0],
        });
    });
    lights
}

const IDENTITY_MODEL_MATRIX: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];
