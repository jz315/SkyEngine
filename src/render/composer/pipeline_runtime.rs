use std::borrow::Cow;

use crate::gpu::GpuContext;
use crate::render::frame_pipeline::FramePipeline;
use crate::render::output_chain::{
    BloomNode, ColorResolveNode, ToneMapNode, ViewportBlitNode, VignetteNode,
};
use crate::render::pipeline::RenderFeatureNode;

use super::render_composer::RenderComposer;
use super::{ClearColorSeedNode, HeadlessKeepAliveNode};

impl RenderComposer {
    pub(crate) fn ensure_initialized(&mut self, gpu: &GpuContext) {
        if self.pipeline.is_some() {
            return;
        }

        let mut pipeline = FramePipeline::new();
        pipeline.add_view_node(Box::new(ClearColorSeedNode));

        for stage_index in 0..self.compiled.stages.len() {
            let stage_name = self.compiled.stages[stage_index].as_str().to_string();
            let before_stage = self
                .compiled
                .before_stage
                .get(&stage_name)
                .cloned()
                .unwrap_or_default();
            self.install_feature_nodes(
                &mut pipeline,
                &before_stage,
                Some(stage_name.as_str()),
                None,
            );

            let queue_indices = self.compiled.queues_by_stage[stage_index].clone();
            for queue_index in queue_indices {
                let queue_name = self.compiled.queues[queue_index].name().to_string();
                let before_queue = self
                    .compiled
                    .before_queue
                    .get(&queue_name)
                    .cloned()
                    .unwrap_or_default();
                self.install_feature_nodes(
                    &mut pipeline,
                    &before_queue,
                    Some(stage_name.as_str()),
                    Some(queue_name.as_str()),
                );

                let domain_indices = self.compiled.domains_by_queue[queue_index].clone();
                for domain_index in domain_indices {
                    for node in self.domains[domain_index].domain.create_view_nodes(gpu) {
                        pipeline.add_view_node(node);
                    }
                }

                let after_queue = self
                    .compiled
                    .after_queue
                    .get(&queue_name)
                    .cloned()
                    .unwrap_or_default();
                self.install_feature_nodes(
                    &mut pipeline,
                    &after_queue,
                    Some(stage_name.as_str()),
                    Some(queue_name.as_str()),
                );
            }

            let after_stage = self
                .compiled
                .after_stage
                .get(&stage_name)
                .cloned()
                .unwrap_or_default();
            self.install_feature_nodes(
                &mut pipeline,
                &after_stage,
                Some(stage_name.as_str()),
                None,
            );

            if self
                .compiled
                .output_chain
                .postfx_after_stage()
                .is_some_and(|stage| stage.as_str() == stage_name)
            {
                self.install_output_chain(&mut pipeline, gpu);
            }
        }

        let before_present = self.compiled.before_present.clone();
        self.install_feature_nodes(&mut pipeline, &before_present, None, None);
        pipeline.add_view_node(Box::new(HeadlessKeepAliveNode));
        pipeline.add_view_node(Box::new(ViewportBlitNode::new(gpu)));
        self.pipeline = Some(pipeline);
    }

    fn install_feature_nodes(
        &self,
        pipeline: &mut FramePipeline,
        feature_indices: &[usize],
        stage: Option<&str>,
        queue: Option<&str>,
    ) {
        for &feature_index in feature_indices {
            let entry = &self.features[feature_index];
            pipeline.add_view_node(Box::new(RenderFeatureNode {
                feature: entry.feature.clone(),
                injection_point: entry.injection_point.clone(),
                stage: stage.map(|value| Cow::Owned(value.to_string())),
                queue: queue.map(|value| Cow::Owned(value.to_string())),
            }));
        }
    }

    fn install_output_chain(&mut self, pipeline: &mut FramePipeline, gpu: &GpuContext) {
        if !self.compiled.output_chain.has_enabled_nodes() {
            return;
        }
        if self.compiled.output_chain.bloom {
            pipeline.add_view_node(Box::new(BloomNode::new(gpu)));
        }
        if self.compiled.output_chain.vignette {
            pipeline.add_view_node(Box::new(VignetteNode::new(gpu)));
        }
        if self.compiled.output_chain.tonemap {
            pipeline.add_view_node(Box::new(ToneMapNode::new(gpu)));
        }
        if self.compiled.output_chain.color_resolve {
            pipeline.add_view_node(Box::new(ColorResolveNode::new(gpu)));
        }
    }
}
