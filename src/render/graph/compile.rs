//! Compilation pipeline: dependency analysis, topological sort, dead-pass
//! culling, execution reordering, and resource lifetime analysis.

use super::*;
use std::collections::VecDeque;

struct DependencyAnalysisResult {
    edges: Vec<Vec<usize>>,
    reverse_edges: Vec<Vec<usize>>,
    topological_order: Vec<usize>,
    dep_levels: Vec<u32>,
    max_dep_level: u32,
}

struct CullingResult {
    alive_set: Vec<bool>,
    culled_count: usize,
}

struct ReorderResult {
    execution_order: Vec<usize>,
}

struct LifetimeAnalysisResult {
    lifetimes: FxHashMap<ResourceRef, ResourceLifetime>,
}

impl RenderGraph {
    // ── Compilation pipeline ────────────────────────────────────────────

    /// Compile the graph: dependency analysis, topological sort, dead-pass
    /// culling, execution reordering, and resource lifetime analysis.
    ///
    /// Returns a list of [`CompiledPass`] in execution order.  Passes that
    /// were culled (no path to a surface write) are excluded.
    ///
    /// This method is **idempotent**: repeated calls return the cached result
    /// without recomputation.  The cache is invalidated when passes or
    /// resources are added.
    ///
    /// Memory alias analysis is deferred to
    /// [`allocate_physical_resources`] where the real surface dimensions are
    /// available for accurate best-fit waste calculations.
    #[must_use]
    pub fn compile(&mut self) -> Result<Vec<CompiledPass>, RenderGraphError> {
        if self.compiled {
            return Ok(self.cached_compiled.clone());
        }

        if self.passes.is_empty() {
            self.clear_compilation_results_for_empty_graph();
            return Ok(Vec::new());
        }

        let dependency = self.analyze_dependencies()?;
        for (idx, level) in dependency.dep_levels.iter().enumerate() {
            self.passes[idx].dep_level = *level;
        }

        let culling = self.cull_dead_passes(&dependency.reverse_edges);
        for (idx, &alive) in culling.alive_set.iter().enumerate() {
            self.passes[idx].alive = alive;
        }

        let reorder = self.reorder_alive_passes(
            &dependency.topological_order,
            &culling.alive_set,
            &dependency.edges,
            &dependency.reverse_edges,
        );
        let lifetime = self.analyze_lifetimes(&reorder.execution_order);
        let compiled_passes = self.compiled_passes_from_order(&reorder.execution_order);

        self.order = reorder.execution_order;
        self.max_dep_level = dependency.max_dep_level;
        self.culled_count = culling.culled_count;
        self.dep_edges = dependency.edges;
        self.dep_reverse_edges = dependency.reverse_edges;
        self.lifetimes = lifetime.lifetimes;
        // alias_groups/alias_stats are computed lazily in allocate_physical_resources()
        self.alias_groups.clear();
        self.alias_stats = None;
        self.alias_redirects.clear();
        self.cached_compiled = compiled_passes.clone();
        self.compiled = true;
        Ok(compiled_passes)
    }

    fn clear_compilation_results_for_empty_graph(&mut self) {
        self.order.clear();
        self.cached_compiled.clear();
        self.lifetimes.clear();
        self.dep_edges.clear();
        self.dep_reverse_edges.clear();
        self.alias_groups.clear();
        self.alias_stats = None;
        self.alias_redirects.clear();
        self.max_dep_level = 0;
        self.culled_count = 0;
        self.compiled = true;
    }

    fn analyze_dependencies(&self) -> Result<DependencyAnalysisResult, RenderGraphError> {
        let n = self.passes.len();
        let mut edges = vec![Vec::<usize>::new(); n];
        let mut indegree = vec![0usize; n];
        let mut reverse_edges = vec![Vec::<usize>::new(); n];
        let mut edge_set: FxHashSet<(usize, usize)> = FxHashSet::default();
        let mut last_writers: Vec<(ResourceRef, usize)> = Vec::new();
        let mut readers_since_write: Vec<(ResourceRef, usize)> = Vec::new();

        let mut add_edge = |from: usize, to: usize| {
            if from == to || !edge_set.insert((from, to)) {
                return;
            }
            edges[from].push(to);
            reverse_edges[to].push(from);
            indegree[to] += 1;
        };

        for (idx, pass) in self.passes.iter().enumerate() {
            let access = pass.access_info(idx);
            debug_assert_eq!(access.pass_index, idx);

            for &resource in access.reads {
                self.validate_pass_resource(access.name, resource)?;
                let mut has_writer = false;
                for &(written_resource, writer) in &last_writers {
                    if !resource_refs_overlap(resource, written_resource) {
                        continue;
                    }
                    add_edge(writer, idx);
                    has_writer = true;
                }
                if !has_writer
                    && !self.resource_has_external_source(resource)
                    && !access
                        .writes
                        .iter()
                        .copied()
                        .any(|written_resource| resource_refs_overlap(resource, written_resource))
                {
                    return Err(RenderGraphError::ReadBeforeWrite {
                        pass: access.name.clone(),
                        resource,
                    });
                }

                if !readers_since_write
                    .iter()
                    .any(|&(read_resource, reader)| read_resource == resource && reader == idx)
                {
                    readers_since_write.push((resource, idx));
                }
            }

            for &resource in access.writes {
                self.validate_pass_resource(access.name, resource)?;
                for &(written_resource, writer) in &last_writers {
                    if resource_refs_overlap(resource, written_resource) {
                        add_edge(writer, idx);
                    }
                }

                for &(read_resource, reader) in &readers_since_write {
                    if resource_refs_overlap(resource, read_resource) {
                        add_edge(reader, idx);
                    }
                }
                readers_since_write
                    .retain(|&(read_resource, _)| !resource_refs_overlap(resource, read_resource));

                last_writers.retain(|&(written_resource, _)| {
                    !resource_refs_overlap(resource, written_resource)
                });
                last_writers.push((resource, idx));
            }
        }

        // Kahn's algorithm with index-sorted ready queue.
        let mut ready = VecDeque::new();
        for (idx, &deg) in indegree.iter().enumerate() {
            if deg == 0 {
                ready.push_back(idx);
            }
        }

        let mut topological_order = Vec::with_capacity(n);
        let mut dep_levels = vec![0u32; n];

        while let Some(node) = ready.pop_front() {
            topological_order.push(node);
            for &next in &edges[node] {
                dep_levels[next] = dep_levels[next].max(dep_levels[node] + 1);
                indegree[next] -= 1;
                if indegree[next] == 0 {
                    let insert_pos = ready.iter().position(|&pending| pending > next);
                    if let Some(pos) = insert_pos {
                        ready.insert(pos, next);
                    } else {
                        ready.push_back(next);
                    }
                }
            }
        }

        if topological_order.len() != n {
            return Err(RenderGraphError::CycleDetected);
        }

        let max_dep_level = dep_levels.iter().copied().max().unwrap_or(0);

        Ok(DependencyAnalysisResult {
            edges,
            reverse_edges,
            topological_order,
            dep_levels,
            max_dep_level,
        })
    }

    fn cull_dead_passes(&self, reverse_edges: &[Vec<usize>]) -> CullingResult {
        let n = self.passes.len();
        let mut alive_set = vec![false; n];
        for (idx, pass) in self.passes.iter().enumerate() {
            let access = pass.access_info(idx);
            debug_assert_eq!(access.pass_index, idx);
            if access
                .writes
                .iter()
                .copied()
                .any(|resource| self.resource_has_external_sink(resource))
            {
                alive_set[idx] = true;
            }
        }

        let mut changed = true;
        while changed {
            changed = false;
            for idx in (0..n).rev() {
                if !alive_set[idx] {
                    continue;
                }
                for &dependency in &reverse_edges[idx] {
                    if !alive_set[dependency] {
                        alive_set[dependency] = true;
                        changed = true;
                    }
                }
            }
        }

        let culled_count = alive_set.iter().filter(|&&alive| !alive).count();
        CullingResult {
            alive_set,
            culled_count,
        }
    }

    fn reorder_alive_passes(
        &self,
        topological_order: &[usize],
        alive_set: &[bool],
        edges: &[Vec<usize>],
        reverse_edges: &[Vec<usize>],
    ) -> ReorderResult {
        let alive_order = topological_order
            .iter()
            .copied()
            .filter(|&i| alive_set[i])
            .collect::<Vec<_>>();

        // Reshuffle passes within valid topological orderings to maximize
        // resource affinity between adjacent passes.  This compresses
        // resource lifetimes and improves GPU cache locality.
        let execution_order = reorder::reorder_for_affinity(
            &alive_order,
            &self.passes,
            &edges,
            &reverse_edges,
            &reorder::ReorderConfig::default(),
        );

        ReorderResult { execution_order }
    }

    fn analyze_lifetimes(&self, execution_order: &[usize]) -> LifetimeAnalysisResult {
        let mut lifetimes: FxHashMap<ResourceRef, ResourceLifetime> = FxHashMap::default();
        let record_lifetime = |lifetimes: &mut FxHashMap<ResourceRef, ResourceLifetime>,
                               resource: ResourceRef,
                               exec_order: usize| {
            lifetimes
                .entry(resource)
                .and_modify(|lt| {
                    lt.last_use = exec_order;
                })
                .or_insert(ResourceLifetime {
                    first_use: exec_order,
                    last_use: exec_order,
                });

            if let ResourceRef::TextureSubresource(subresource) = resource {
                lifetimes
                    .entry(ResourceRef::Texture(subresource.texture))
                    .and_modify(|lt| {
                        lt.last_use = exec_order;
                    })
                    .or_insert(ResourceLifetime {
                        first_use: exec_order,
                        last_use: exec_order,
                    });
            }
        };
        for (exec_order, &pass_idx) in execution_order.iter().enumerate() {
            let access = self.passes[pass_idx].access_info(pass_idx);
            for resource in access.reads.iter().chain(access.writes.iter()) {
                record_lifetime(&mut lifetimes, *resource, exec_order);
            }
        }

        // Memory alias analysis is deferred to allocate_physical_resources()
        // where the real surface_size is known.  Using [0,0] here would make
        // best-fit waste calculations useless for Surface/Scale textures.
        LifetimeAnalysisResult { lifetimes }
    }

    fn compiled_passes_from_order(&self, execution_order: &[usize]) -> Vec<CompiledPass> {
        execution_order
            .iter()
            .map(|&idx| {
                let access = self.passes[idx].access_info(idx);
                CompiledPass {
                    handle: PassHandle(idx, self.handle_token),
                    index: idx,
                    name: access.name.clone(),
                    pass_type: access.pass_type,
                    reads: access.reads.to_vec(),
                    writes: access.writes.to_vec(),
                    color_outputs: access.color_outputs.to_vec(),
                    depth_stencil: access.depth_stencil,
                    copy_ops: access.copy_ops.to_vec(),
                    flags: access.flags,
                    dep_level: self.passes[idx].dep_level,
                }
            })
            .collect()
    }
}
