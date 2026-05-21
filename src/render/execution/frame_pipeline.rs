use rustc_hash::{FxHashMap, FxHashSet};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

pub struct FramePipeline<'nodes, S: ?Sized = ()> {
    setup_nodes: Vec<Box<dyn FrameSetupNode + 'nodes>>,
    view_nodes: Vec<Box<dyn FrameViewNode<S> + 'nodes>>,
    finalize_nodes: Vec<Box<dyn FrameFinalizeNode + 'nodes>>,
    graph: RenderGraph,
    pass_dispatch: FxHashMap<PassHandle, DispatchEntry>,
    completed_views: Vec<CompletedViewState>,
}

impl<'nodes, S: ?Sized> FramePipeline<'nodes, S> {
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

    pub fn add_setup_node(&mut self, node: Box<dyn FrameSetupNode + 'nodes>) -> &mut Self {
        self.setup_nodes.push(node);
        self
    }

    pub fn add_view_node(&mut self, node: Box<dyn FrameViewNode<S> + 'nodes>) -> &mut Self {
        self.view_nodes.push(node);
        self
    }

    pub fn add_finalize_node(&mut self, node: Box<dyn FrameFinalizeNode + 'nodes>) -> &mut Self {
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

    pub fn execute_frame_with_services(
        &mut self,
        ctx: &mut GpuContext,
        frame: &PreparedFrame<'_>,
        services: &mut S,
    ) -> FrameExecutionStats {
        self.prepare_frame_graph(frame);

        let pass_count = match self.graph.compile() {
            Ok(compiled) => compiled.len(),
            Err(error) => {
                eprintln!(
                    "[SkyEngine] Frame render graph compilation failed; skipping frame: {error}"
                );
                return FrameExecutionStats::default();
            }
        };
        let mut stats = FrameExecutionStats {
            passes: pass_count,
            draw_calls: 0,
        };
        let setup_nodes = &mut self.setup_nodes;
        let view_nodes = &mut self.view_nodes;
        let finalize_nodes = &mut self.finalize_nodes;
        let completed_views = &self.completed_views;
        let mut completed_view_lookup = vec![None; frame.view_count()];
        for (completed_index, completed_view) in completed_views.iter().enumerate() {
            completed_view_lookup[completed_view.view_index()] = Some(completed_index);
        }
        let pass_dispatch = &self.pass_dispatch;
        let mut counted_draw_dispatches = FxHashSet::default();

        if let Err(error) = self
            .graph
            .try_execute(ctx, |compiled_pass, ctx, resources| {
                let Some(dispatch) = pass_dispatch.get(&compiled_pass.handle).copied() else {
                    return Ok(());
                };
                let count_draw_calls = counted_draw_dispatches.insert(dispatch);

                match dispatch {
                    DispatchEntry::Setup { node_index } => {
                        let execution = SetupExecutionContext { frame };
                        if count_draw_calls {
                            stats.draw_calls += setup_nodes[node_index].draw_calls(&execution);
                        }
                        setup_nodes[node_index].execute(
                            compiled_pass,
                            ctx,
                            resources,
                            &execution,
                        )?;
                    }
                    DispatchEntry::View {
                        node_index,
                        view_index,
                    } => {
                        let view_state = completed_view_lookup
                            .get(view_index)
                            .and_then(|index| *index)
                            .and_then(|index| completed_views.get(index))
                            .expect("view dispatch should have completed setup state");
                        let execution = ViewExecutionContext {
                            frame,
                            view: frame.view(view_index),
                            view_state,
                            view_index,
                        };
                        if count_draw_calls {
                            stats.draw_calls +=
                                view_nodes[node_index].draw_calls(&execution, services);
                        }
                        view_nodes[node_index].execute(
                            compiled_pass,
                            ctx,
                            resources,
                            &execution,
                            services,
                        )?;
                    }
                    DispatchEntry::Finalize { node_index } => {
                        let execution = FinalizeExecutionContext {
                            frame,
                            completed_views,
                        };
                        if count_draw_calls {
                            stats.draw_calls += finalize_nodes[node_index].draw_calls(&execution);
                        }
                        finalize_nodes[node_index].execute(
                            compiled_pass,
                            ctx,
                            resources,
                            &execution,
                        )?;
                    }
                }
                Ok(())
            })
        {
            eprintln!(
                "[SkyEngine] Frame render graph execution failed; frame may be incomplete: {error}"
            );
        }

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
        let setup_scene_gbuffer = *setup_state.scene_gbuffer();
        let setup_scene_shadows = setup_state.scene_shadows().cloned();
        let mut frame_slots = setup_state.into_slots();
        let mut frame_scene_gbuffer = setup_scene_gbuffer;

        let mut ordered_view_indices: Vec<usize> = (0..frame.view_count()).collect();
        ordered_view_indices.sort_by_key(|&index| frame.view(index).order());
        self.completed_views.reserve(ordered_view_indices.len());

        for &view_index in &ordered_view_indices {
            let view = frame.view(view_index);
            let mut state = PhaseState::with_slots(
                frame.surface_format(),
                frame.has_surface(),
                frame_slots.clone(),
                setup_scene_gbuffer,
                setup_scene_shadows.clone(),
            );
            for node_index in 0..self.view_nodes.len() {
                if !self.view_nodes[node_index].is_enabled(frame)
                    || !self.view_nodes[node_index].is_view_enabled(frame, view)
                {
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
            let (slots, scene_gbuffer, scene_shadows) = state.into_parts();
            self.completed_views.push(CompletedViewState::new(
                view_index,
                view,
                slots,
                scene_gbuffer,
                scene_shadows,
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
                    &mut frame_scene_gbuffer,
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
    pub(crate) fn debug_prepare_frame(&mut self, frame: &PreparedFrame<'_>) {
        self.prepare_frame_graph(frame);
    }

    fn register_passes(&mut self, handles: &[PassHandle], dispatch: DispatchEntry) {
        for &handle in handles {
            self.pass_dispatch.insert(handle, dispatch);
        }
    }
}

impl<'nodes> FramePipeline<'nodes, ()> {
    pub fn execute_frame(
        &mut self,
        ctx: &mut GpuContext,
        frame: &PreparedFrame<'_>,
    ) -> FrameExecutionStats {
        self.execute_frame_with_services(ctx, frame, &mut ())
    }
}

impl<'nodes, S> Default for FramePipeline<'nodes, S> {
    fn default() -> Self {
        Self::new()
    }
}
