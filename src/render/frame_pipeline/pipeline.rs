use rustc_hash::FxHashMap;

use crate::gpu::GpuContext;
use crate::render::graph::{PassHandle, RenderGraph};

use super::nodes::{
    FinalizeExecutionContext, FrameFinalizeNode, FrameSetupNode, FrameViewNode,
    SetupExecutionContext, ViewExecutionContext,
};
use super::payload::PreparedFrame;
use super::slots::{CompletedViewState, FinalizePhaseState, PhaseState};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameExecutionStats {
    pub passes: usize,
    pub draw_calls: usize,
}

#[derive(Debug, Clone, Copy)]
enum DispatchEntry {
    Setup {
        node_index: usize,
    },
    View {
        node_index: usize,
        view_index: usize,
    },
    Finalize {
        node_index: usize,
    },
}

pub struct FramePipeline {
    setup_nodes: Vec<Box<dyn FrameSetupNode>>,
    view_nodes: Vec<Box<dyn FrameViewNode>>,
    finalize_nodes: Vec<Box<dyn FrameFinalizeNode>>,
    graph: RenderGraph,
    pass_dispatch: FxHashMap<PassHandle, DispatchEntry>,
    completed_views: Vec<CompletedViewState>,
}

impl FramePipeline {
    pub fn new() -> Self {
        Self {
            setup_nodes: Vec::new(),
            view_nodes: Vec::new(),
            finalize_nodes: Vec::new(),
            graph: RenderGraph::new(),
            pass_dispatch: FxHashMap::default(),
            completed_views: Vec::with_capacity(4),
        }
    }

    pub fn add_setup_node(&mut self, node: Box<dyn FrameSetupNode>) -> &mut Self {
        self.setup_nodes.push(node);
        self
    }

    pub fn add_view_node(&mut self, node: Box<dyn FrameViewNode>) -> &mut Self {
        self.view_nodes.push(node);
        self
    }

    pub fn add_finalize_node(&mut self, node: Box<dyn FrameFinalizeNode>) -> &mut Self {
        self.finalize_nodes.push(node);
        self
    }

    pub fn resize(&mut self, ctx: &GpuContext, width: u32, height: u32) {
        for node in &mut self.setup_nodes {
            node.resize(ctx, width, height);
        }
        for node in &mut self.view_nodes {
            node.resize(ctx, width, height);
        }
        for node in &mut self.finalize_nodes {
            node.resize(ctx, width, height);
        }
    }

    pub fn execute_frame(
        &mut self,
        ctx: &mut GpuContext,
        frame: &PreparedFrame<'_>,
    ) -> FrameExecutionStats {
        self.prepare_frame_graph(frame);

        let mut stats = FrameExecutionStats::default();
        let setup_nodes = &mut self.setup_nodes;
        let view_nodes = &mut self.view_nodes;
        let finalize_nodes = &mut self.finalize_nodes;
        let completed_views = &self.completed_views;
        let pass_dispatch = &self.pass_dispatch;

        self.graph.execute(ctx, |compiled_pass, ctx, resources| {
            let Some(dispatch) = pass_dispatch.get(&compiled_pass.handle).copied() else {
                return Ok(());
            };

            stats.passes += 1;

            match dispatch {
                DispatchEntry::Setup { node_index } => {
                    let execution = SetupExecutionContext { frame };
                    stats.draw_calls += setup_nodes[node_index].draw_calls(&execution);
                    setup_nodes[node_index].execute(compiled_pass, ctx, resources, &execution)?;
                }
                DispatchEntry::View {
                    node_index,
                    view_index,
                } => {
                    let execution = ViewExecutionContext {
                        frame,
                        view: frame.view(view_index),
                        view_index,
                    };
                    stats.draw_calls += view_nodes[node_index].draw_calls(&execution);
                    view_nodes[node_index].execute(compiled_pass, ctx, resources, &execution)?;
                }
                DispatchEntry::Finalize { node_index } => {
                    let execution = FinalizeExecutionContext {
                        frame,
                        completed_views,
                    };
                    stats.draw_calls += finalize_nodes[node_index].draw_calls(&execution);
                    finalize_nodes[node_index].execute(
                        compiled_pass,
                        ctx,
                        resources,
                        &execution,
                    )?;
                }
            }
            Ok(())
        });

        stats
    }

    fn prepare_frame_graph(&mut self, frame: &PreparedFrame<'_>) {
        self.graph.clear_frame();
        self.pass_dispatch.clear();
        self.completed_views.clear();

        let mut setup_state = PhaseState::new(frame.surface_format(), frame.has_surface());
        for node_index in 0..self.setup_nodes.len() {
            if !self.setup_nodes[node_index].is_enabled(frame) {
                continue;
            }

            let pass_count_before = self.graph.pass_count();
            {
                let node = &mut self.setup_nodes[node_index];
                node.setup(&mut self.graph, &mut setup_state, frame);
            }
            let new_passes = self.graph.pass_handles_from(pass_count_before);
            self.register_passes(&new_passes, DispatchEntry::Setup { node_index });
        }
        let mut frame_slots = setup_state.into_slots();

        let mut ordered_view_indices: Vec<usize> = (0..frame.view_count()).collect();
        ordered_view_indices.sort_by_key(|&index| frame.view(index).order());
        self.completed_views.reserve(ordered_view_indices.len());

        for &view_index in &ordered_view_indices {
            let view = frame.view(view_index);
            let mut state = PhaseState::with_slots(
                frame.surface_format(),
                frame.has_surface(),
                frame_slots.clone(),
            );
            for node_index in 0..self.view_nodes.len() {
                if !self.view_nodes[node_index].is_enabled(frame) {
                    continue;
                }

                let pass_count_before = self.graph.pass_count();
                {
                    let node = &mut self.view_nodes[node_index];
                    node.setup(&mut self.graph, &mut state, frame, view);
                }
                let new_passes = self.graph.pass_handles_from(pass_count_before);
                self.register_passes(
                    &new_passes,
                    DispatchEntry::View {
                        node_index,
                        view_index,
                    },
                );
            }
            self.completed_views.push(CompletedViewState::new(
                view_index,
                view,
                state.into_slots(),
            ));
        }

        for node_index in 0..self.finalize_nodes.len() {
            if !self.finalize_nodes[node_index].is_enabled(frame) {
                continue;
            }

            let pass_count_before = self.graph.pass_count();
            {
                let mut state = FinalizePhaseState::new(
                    frame.surface_format(),
                    frame.has_surface(),
                    &mut frame_slots,
                    &self.completed_views,
                );
                let node = &mut self.finalize_nodes[node_index];
                node.setup(&mut self.graph, &mut state, frame);
            }
            let new_passes = self.graph.pass_handles_from(pass_count_before);
            self.register_passes(&new_passes, DispatchEntry::Finalize { node_index });
        }
    }

    #[inline]
    pub fn view_node_count(&self) -> usize {
        self.view_nodes.len()
    }

    #[cfg(test)]
    pub(crate) fn debug_last_light_ambient(&self) -> Option<[f32; 4]> {
        self.view_nodes
            .iter()
            .find_map(|node| node.debug_last_light_ambient())
    }

    #[cfg(test)]
    pub(crate) fn graph_debug_declared_pass_names(&self) -> Vec<String> {
        self.graph.debug_declared_pass_names()
    }

    #[cfg(test)]
    pub(crate) fn graph_debug_declared_surface_loads(
        &self,
        pass_name: &str,
    ) -> Vec<crate::render::graph::LoadOp> {
        self.graph.debug_declared_surface_loads(pass_name)
    }

    #[cfg(test)]
    pub(crate) fn debug_prepare_frame(&mut self, frame: &PreparedFrame<'_>) {
        self.prepare_frame_graph(frame);
    }

    fn register_passes(&mut self, handles: &[PassHandle], dispatch: DispatchEntry) {
        for &handle in handles {
            self.pass_dispatch.insert(handle, dispatch);
        }
    }
}

impl Default for FramePipeline {
    fn default() -> Self {
        Self::new()
    }
}
