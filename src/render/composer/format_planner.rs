use crate::render::ecs::RenderSettings;
use crate::render::frame_pipeline::PreparedFrame;

use super::render_composer::RenderComposer;

pub(crate) struct FormatPlanner<'a> {
    composer: &'a RenderComposer,
}

impl<'a> FormatPlanner<'a> {
    #[inline]
    pub(crate) fn new(composer: &'a RenderComposer) -> Self {
        Self { composer }
    }

    pub(crate) fn resolve_domain_target_formats(
        &self,
        frame: &PreparedFrame<'_>,
    ) -> Vec<wgpu::TextureFormat> {
        let surface_format = frame.surface_format();
        let mut target_formats = vec![surface_format; self.composer.domains.len()];
        let mut current_format = surface_format;

        for stage_index in 0..self.composer.compiled.stages.len() {
            let stage_name = self.composer.compiled.stages[stage_index].as_str();
            if let Some(feature_indices) = self.composer.compiled.before_stage.get(stage_name) {
                self.apply_feature_output_hints(
                    feature_indices,
                    &mut current_format,
                    surface_format,
                    frame,
                );
            }

            for &queue_index in &self.composer.compiled.queues_by_stage[stage_index] {
                let queue_name = self.composer.compiled.queues[queue_index].name();
                if let Some(feature_indices) = self.composer.compiled.before_queue.get(queue_name) {
                    self.apply_feature_output_hints(
                        feature_indices,
                        &mut current_format,
                        surface_format,
                        frame,
                    );
                }

                for &domain_index in &self.composer.compiled.domains_by_queue[queue_index] {
                    target_formats[domain_index] = current_format;
                    current_format = self.composer.domains[domain_index]
                        .domain
                        .output_format_hint(current_format, surface_format);
                }

                if let Some(feature_indices) = self.composer.compiled.after_queue.get(queue_name) {
                    self.apply_feature_output_hints(
                        feature_indices,
                        &mut current_format,
                        surface_format,
                        frame,
                    );
                }
            }

            if let Some(feature_indices) = self.composer.compiled.after_stage.get(stage_name) {
                self.apply_feature_output_hints(
                    feature_indices,
                    &mut current_format,
                    surface_format,
                    frame,
                );
            }

            if self
                .composer
                .compiled
                .output_chain
                .postfx_after_stage()
                .is_some_and(|stage| {
                    stage.as_str() == self.composer.compiled.stages[stage_index].as_str()
                })
            {
                current_format =
                    self.apply_output_chain_hint(frame, current_format, surface_format);
            }
        }

        target_formats
    }

    fn apply_feature_output_hints(
        &self,
        feature_indices: &[usize],
        current_format: &mut wgpu::TextureFormat,
        surface_format: wgpu::TextureFormat,
        frame: &PreparedFrame<'_>,
    ) {
        for &feature_index in feature_indices {
            let feature = &self.composer.features[feature_index].feature;
            if feature.is_enabled(frame) {
                *current_format = feature
                    .output_format_hint()
                    .apply(*current_format, surface_format);
            }
        }
    }

    fn apply_output_chain_hint(
        &self,
        frame: &PreparedFrame<'_>,
        current_format: wgpu::TextureFormat,
        surface_format: wgpu::TextureFormat,
    ) -> wgpu::TextureFormat {
        let settings = frame
            .payload::<RenderSettings>()
            .copied()
            .unwrap_or_default();
        self.composer
            .compiled
            .output_chain
            .output_format_after_settings(settings, current_format, surface_format)
    }
}
