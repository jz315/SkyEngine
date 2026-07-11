use rustc_hash::{FxHashMap, FxHashSet};

use crate::gpu::GpuContext;
use crate::render::graph::{PassHandle, RenderGraph, RenderGraphError};

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

/// Reusable execution state kept independently from the borrowed frame nodes.
///
/// Runtime nodes borrow declaration-time pipeline steps, so the node list is
/// rebuilt for each frame. Keeping the graph and scratch storage here lets the
/// runtime retain transient GPU pools and allocation capacity across those
/// short-lived node wrappers.
pub(crate) struct FramePipelineCache {
    graph: RenderGraph,
    pass_dispatch: FxHashMap<PassHandle, DispatchEntry>,
    completed_views: Vec<CompletedViewState>,
    completed_view_lookup: Vec<Option<usize>>,
    counted_draw_dispatches: FxHashSet<DispatchEntry>,
    ordered_view_indices: Vec<usize>,
}

impl Default for FramePipelineCache {
    fn default() -> Self {
        Self {
            graph: RenderGraph::new(),
            pass_dispatch: FxHashMap::default(),
            completed_views: Vec::with_capacity(4),
            completed_view_lookup: Vec::with_capacity(4),
            counted_draw_dispatches: FxHashSet::default(),
            ordered_view_indices: Vec::with_capacity(4),
        }
    }
}

impl FramePipelineCache {
    pub(crate) fn invalidate_gpu_resources(&mut self) {
        self.graph.destroy_physical_resources();
    }
}

#[cfg(test)]
impl FramePipelineCache {
    pub(crate) fn cached_transient_texture_ptr(&self) -> Option<usize> {
        self.graph.cached_transient_texture_ptr()
    }
}

pub struct FramePipeline<'nodes, S: ?Sized = ()> {
    setup_nodes: Vec<Box<dyn FrameSetupNode + 'nodes>>,
    view_nodes: Vec<Box<dyn FrameViewNode<S> + 'nodes>>,
    finalize_nodes: Vec<Box<dyn FrameFinalizeNode + 'nodes>>,
    cache: FramePipelineCache,
}

impl<'nodes, S: ?Sized> FramePipeline<'nodes, S> {
    pub fn new() -> Self {
        Self::with_cache(FramePipelineCache::default())
    }

    pub(crate) fn with_cache(cache: FramePipelineCache) -> Self {
        Self {
            setup_nodes: Vec::new(),
            view_nodes: Vec::new(),
            finalize_nodes: Vec::new(),
            cache,
        }
    }

    pub(crate) fn into_cache(self) -> FramePipelineCache {
        self.cache
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
        match self.try_execute_frame_with_services(ctx, frame, services) {
            Ok(stats) => stats,
            Err(error) => {
                log::error!(
                    target: "sky_engine::render::graph",
                    "frame render graph failed; skipping incomplete frame: {error}"
                );
                FrameExecutionStats::default()
            }
        }
    }

    /// Execute a prepared frame and preserve render-graph failures for the
    /// caller instead of reducing them to empty statistics.
    pub fn try_execute_frame_with_services(
        &mut self,
        ctx: &mut GpuContext,
        frame: &PreparedFrame<'_>,
        services: &mut S,
    ) -> Result<FrameExecutionStats, RenderGraphError> {
        self.prepare_frame_graph(frame);

        self.cache.graph.ensure_compiled()?;
        let pass_count = self.cache.graph.alive_pass_count();
        let mut stats = FrameExecutionStats {
            passes: pass_count,
            draw_calls: 0,
        };
        let setup_nodes = &mut self.setup_nodes;
        let view_nodes = &mut self.view_nodes;
        let finalize_nodes = &mut self.finalize_nodes;
        let completed_views = &self.cache.completed_views;
        let completed_view_lookup = &mut self.cache.completed_view_lookup;
        completed_view_lookup.clear();
        completed_view_lookup.resize(frame.view_count(), None);
        for (completed_index, completed_view) in completed_views.iter().enumerate() {
            completed_view_lookup[completed_view.view_index()] = Some(completed_index);
        }
        let pass_dispatch = &self.cache.pass_dispatch;
        let counted_draw_dispatches = &mut self.cache.counted_draw_dispatches;
        counted_draw_dispatches.clear();

        let mut execute_pass =
            |compiled_pass: &crate::render::graph::CompiledPass,
             ctx: &mut GpuContext,
             resources: &crate::render::graph::PhysicalResources<'_>| {
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
            };

        #[cfg(feature = "profile")]
        {
            let mut profiler = crate::render::graph::SkyProfileRenderGraphProfiler::new();
            self.cache
                .graph
                .try_execute_profiled(ctx, &mut profiler, &mut execute_pass)?;
        }
        #[cfg(not(feature = "profile"))]
        {
            self.cache.graph.try_execute(ctx, &mut execute_pass)?;
        }

        Ok(stats)
    }

    fn prepare_frame_graph(&mut self, frame: &PreparedFrame<'_>) {
        self.cache.graph.clear_frame();
        self.cache.pass_dispatch.clear();
        self.cache.completed_views.clear();

        let mut setup_state = PhaseState::new(frame.surface_format(), frame.has_surface());
        for node_index in 0..self.setup_nodes.len() {
            if !self.setup_nodes[node_index].is_enabled(frame) {
                continue;
            }

            let pass_count_before = self.cache.graph.pass_count();
            {
                let node = &mut self.setup_nodes[node_index];
                node.setup(&mut self.cache.graph, &mut setup_state, frame);
            }
            self.register_passes_from(pass_count_before, DispatchEntry::Setup { node_index });
        }
        let setup_scene_gbuffer = *setup_state.scene_gbuffer();
        let setup_scene_shadows = setup_state.scene_shadows().cloned();
        let mut frame_slots = setup_state.into_slots();
        let mut frame_scene_gbuffer = setup_scene_gbuffer;

        self.cache.ordered_view_indices.clear();
        self.cache
            .ordered_view_indices
            .extend(0..frame.view_count());
        self.cache
            .ordered_view_indices
            .sort_by_key(|&index| frame.view(index).order());
        self.cache
            .completed_views
            .reserve(self.cache.ordered_view_indices.len());

        for ordered_index in 0..self.cache.ordered_view_indices.len() {
            let view_index = self.cache.ordered_view_indices[ordered_index];
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

                let pass_count_before = self.cache.graph.pass_count();
                {
                    let node = &mut self.view_nodes[node_index];
                    node.setup(&mut self.cache.graph, &mut state, frame, view);
                }
                self.register_passes_from(
                    pass_count_before,
                    DispatchEntry::View {
                        node_index,
                        view_index,
                    },
                );
            }
            let (slots, scene_gbuffer, scene_shadows) = state.into_parts();
            self.cache.completed_views.push(CompletedViewState::new(
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

            let pass_count_before = self.cache.graph.pass_count();
            {
                let mut state = FinalizePhaseState::new(
                    frame.surface_format(),
                    frame.has_surface(),
                    &mut frame_slots,
                    &mut frame_scene_gbuffer,
                    &self.cache.completed_views,
                );
                let node = &mut self.finalize_nodes[node_index];
                node.setup(&mut self.cache.graph, &mut state, frame);
            }
            self.register_passes_from(pass_count_before, DispatchEntry::Finalize { node_index });
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

    fn register_passes_from(&mut self, start: usize, dispatch: DispatchEntry) {
        let end = self.cache.graph.pass_count();
        for index in start..end {
            let handle = self
                .cache
                .graph
                .pass_handle_at(index)
                .expect("newly declared render-graph pass should have a valid handle");
            self.cache.pass_dispatch.insert(handle, dispatch);
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

    pub fn try_execute_frame(
        &mut self,
        ctx: &mut GpuContext,
        frame: &PreparedFrame<'_>,
    ) -> Result<FrameExecutionStats, RenderGraphError> {
        self.try_execute_frame_with_services(ctx, frame, &mut ())
    }
}

impl<'nodes, S> Default for FramePipeline<'nodes, S> {
    fn default() -> Self {
        Self::new()
    }
}
