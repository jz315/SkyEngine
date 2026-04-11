use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::render::ecs::RenderSettings;
use crate::render::frame_pipeline::{PreparedFrame, PreparedView};
use crate::render::scene::{fallback_scene_view, RenderStats, SceneView};
use crate::render::stats::{elapsed_ms, timing_start, RenderTimingStats};

use super::format_planner::FormatPlanner;
use super::render_composer::RenderComposer;

pub(crate) struct PreparedFrameBuilder<'a> {
    composer: &'a RenderComposer,
    surface_format: wgpu::TextureFormat,
    has_surface: bool,
}

impl<'a> PreparedFrameBuilder<'a> {
    #[inline]
    pub(crate) fn new(composer: &'a RenderComposer, gpu: &GpuContext) -> Self {
        Self {
            composer,
            surface_format: gpu.surface_format(),
            has_surface: gpu.has_surface(),
        }
    }

    pub(crate) fn finalize_views(&self, mut views: Vec<SceneView>) -> Vec<SceneView> {
        if views.is_empty() {
            views.push(fallback_scene_view(self.composer.surface_size));
        }
        views.sort_by_key(|view| view.order);
        for (index, view) in views.iter_mut().enumerate() {
            view.clear_surface = index == 0;
            if view.target_size[0] == 0 || view.target_size[1] == 0 {
                view.target_size = view.viewport.size();
            }
        }
        views
    }

    pub(crate) fn build_format_probe_frame(&self, views: &'a [SceneView]) -> PreparedFrame<'a> {
        let mut frame = PreparedFrame::new(self.surface_format, self.has_surface);
        let _ = frame.insert_payload(&self.composer.frame_settings);
        for view in views {
            let mut prepared_view = PreparedView::new(
                view.order,
                view.viewport,
                view.target_size,
                view.clear_surface,
            );
            let _ = prepared_view.insert_payload(view);
            frame.add_view(prepared_view);
        }
        frame
    }

    pub(crate) fn build_prepared_frame(&self, views: &'a [SceneView]) -> PreparedFrame<'a> {
        let mut frame = PreparedFrame::new(self.surface_format, self.has_surface);
        let _ = frame.insert_payload(&self.composer.frame_settings);
        for entry in &self.composer.domains {
            entry.domain.insert_frame_payloads(&mut frame);
        }
        for (view_index, view) in views.iter().enumerate() {
            let mut prepared_view = PreparedView::new(
                view.order,
                view.viewport,
                view.target_size,
                view.clear_surface,
            );
            let _ = prepared_view.insert_payload(view);
            for entry in &self.composer.domains {
                entry
                    .domain
                    .insert_view_payloads(view_index, view, &mut prepared_view);
            }
            frame.add_view(prepared_view);
        }
        frame
    }
}

impl RenderComposer {
    pub fn render_world(&mut self, gpu: &mut GpuContext, world: &World) {
        self.ensure_initialized(gpu);
        self.surface_size = gpu.surface_size();
        self.frame_settings = world
            .get_resource::<RenderSettings>()
            .copied()
            .unwrap_or_default();

        let resolved_transforms = self.resolve_scene_transforms(world);
        let frame_start = timing_start();

        for entry in &mut self.domains {
            entry.domain.configure_queue_sort(entry.sort_policy);
        }
        for entry in &mut self.domains {
            entry
                .domain
                .extract(world, &resolved_transforms, self.surface_size);
        }

        let mut views = self.collect_world_views(world, &resolved_transforms);
        for entry in &self.domains {
            entry.domain.collect_views(&mut views);
        }
        let views = PreparedFrameBuilder::new(self, gpu).finalize_views(views);
        let domain_target_formats = {
            let format_probe_frame =
                PreparedFrameBuilder::new(self, gpu).build_format_probe_frame(&views);
            FormatPlanner::new(self).resolve_domain_target_formats(&format_probe_frame)
        };
        for (entry, target_format) in self.domains.iter_mut().zip(domain_target_formats) {
            entry.domain.configure_target_format(target_format);
        }
        for entry in &mut self.domains {
            entry.domain.prepare(gpu, world, &views);
        }

        let mut pipeline = self
            .pipeline
            .take()
            .expect("RenderComposer should be initialized before rendering");
        let frame = PreparedFrameBuilder::new(self, gpu).build_prepared_frame(&views);
        let execute_start = timing_start();
        let execution = pipeline.execute_frame(gpu, &frame);
        let execute_ms = elapsed_ms(execute_start);
        self.pipeline = Some(pipeline);

        let mut stats = RenderStats {
            producer_count: self.domains.len(),
            view_count: views.len(),
            draw_calls: execution.draw_calls,
            passes: execution.passes,
            timings: RenderTimingStats {
                frame_ms: elapsed_ms(frame_start),
                execute_ms,
                ..Default::default()
            },
            ..Default::default()
        };
        for entry in &self.domains {
            entry.domain.populate_stats(&mut stats);
        }
        self.last_stats = stats;
    }
}
