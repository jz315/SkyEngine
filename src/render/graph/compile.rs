//! Compilation pipeline: dependency analysis, topological sort, dead-pass
//! culling, execution reordering, and resource lifetime analysis.

use super::*;
use std::collections::VecDeque;

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
    /// Memory alias analysis (Phase 4) is deferred to
    /// [`allocate_physical_resources`] where the real surface dimensions are
    /// available for accurate best-fit waste calculations.
    #[must_use]
    pub fn compile(&mut self) -> Result<Vec<CompiledPass>, RenderGraphError> {
        if self.compiled {
            return Ok(self.cached_compiled.clone());
        }

        let n = self.passes.len();
        if n == 0 {
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
            return Ok(Vec::new());
        }

        // ── Phase 1: Dependency analysis ────────────────────────────────
        let mut edges = vec![Vec::<usize>::new(); n];
        let mut indegree = vec![0usize; n];
        let mut reverse_edges = vec![Vec::<usize>::new(); n];
        let mut edge_set: FxHashSet<(usize, usize)> = FxHashSet::default();
        let mut last_writer_for: FxHashMap<ResourceRef, usize> = FxHashMap::default();
        let mut readers_since_write: FxHashMap<ResourceRef, Vec<usize>> = FxHashMap::default();

        let mut add_edge = |from: usize, to: usize| {
            if from == to || !edge_set.insert((from, to)) {
                return;
            }
            edges[from].push(to);
            reverse_edges[to].push(from);
            indegree[to] += 1;
        };

        for (idx, pass) in self.passes.iter().enumerate() {
            for &resource in &pass.reads {
                self.validate_pass_resource(&pass.name, resource)?;
                if let Some(&writer) = last_writer_for.get(&resource) {
                    add_edge(writer, idx);
                } else if !self.resource_has_external_source(resource)
                    && !pass.writes.contains(&resource)
                {
                    return Err(RenderGraphError::ReadBeforeWrite {
                        pass: pass.name.clone(),
                        resource,
                    });
                }

                let readers = readers_since_write.entry(resource).or_default();
                if readers.last().copied() != Some(idx) {
                    readers.push(idx);
                }
            }

            for &resource in &pass.writes {
                self.validate_pass_resource(&pass.name, resource)?;
                if let Some(&writer) = last_writer_for.get(&resource) {
                    add_edge(writer, idx);
                }

                if let Some(readers) = readers_since_write.get_mut(&resource) {
                    for &reader in readers.iter() {
                        add_edge(reader, idx);
                    }
                    readers.clear();
                }

                last_writer_for.insert(resource, idx);
            }
        }

        // Kahn's algorithm with index-sorted ready queue.
        let mut ready = VecDeque::new();
        for (idx, &deg) in indegree.iter().enumerate() {
            if deg == 0 {
                ready.push_back(idx);
            }
        }

        let mut order = Vec::with_capacity(n);
        let mut dep_levels = vec![0u32; n];

        while let Some(node) = ready.pop_front() {
            order.push(node);
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

        if order.len() != n {
            return Err(RenderGraphError::CycleDetected);
        }

        let mut max_dep_level = 0u32;
        for (idx, level) in dep_levels.iter().enumerate() {
            self.passes[idx].dep_level = *level;
            max_dep_level = max_dep_level.max(*level);
        }

        // ── Phase 2: Cull dead passes ───────────────────────────────────
        for pass in &mut self.passes {
            pass.alive = false;
        }

        let mut alive_set = vec![false; n];
        for (idx, pass) in self.passes.iter().enumerate() {
            if pass
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

        let mut culled = 0usize;
        for (idx, &alive) in alive_set.iter().enumerate() {
            self.passes[idx].alive = alive;
            if !alive {
                culled += 1;
            }
        }

        let order: Vec<usize> = order.into_iter().filter(|&i| alive_set[i]).collect();

        // ── Phase 2.5: Execution reordering (SakuraEngine-inspired) ─────
        // Reshuffle passes within valid topological orderings to maximize
        // resource affinity between adjacent passes.  This compresses
        // resource lifetimes and improves GPU cache locality.
        let order = reorder::reorder_for_affinity(
            &order,
            &self.passes,
            &edges,
            &reverse_edges,
            &reorder::ReorderConfig::default(),
        );

        // ── Phase 3: Resource lifetime analysis ─────────────────────────
        let mut lifetimes: FxHashMap<ResourceRef, ResourceLifetime> = FxHashMap::default();
        for (exec_order, &pass_idx) in order.iter().enumerate() {
            let pass = &self.passes[pass_idx];
            for resource in pass.reads.iter().chain(pass.writes.iter()) {
                lifetimes
                    .entry(*resource)
                    .and_modify(|lt| {
                        lt.last_use = exec_order;
                    })
                    .or_insert(ResourceLifetime {
                        first_use: exec_order,
                        last_use: exec_order,
                    });
            }
        }

        // Note: Memory alias analysis (Phase 4) is deferred to
        // allocate_physical_resources() where the real surface_size is known.
        // Using [0,0] here would make best-fit waste calculations useless for
        // Surface/Scale textures.

        // Build the CompiledPass list
        let compiled_passes: Vec<CompiledPass> = order
            .iter()
            .map(|&idx| {
                let pass = &self.passes[idx];
                CompiledPass {
                    handle: PassHandle(idx, self.handle_token),
                    index: idx,
                    name: pass.name.clone(),
                    pass_type: pass.pass_type,
                    reads: pass.reads.clone(),
                    writes: pass.writes.clone(),
                    color_outputs: pass.color_outputs.clone(),
                    depth_stencil: pass.depth_stencil,
                    copy_ops: pass.copy_ops.clone(),
                    flags: pass.flags,
                    dep_level: pass.dep_level,
                }
            })
            .collect();

        self.order = order;
        self.max_dep_level = max_dep_level;
        self.culled_count = culled;
        self.dep_edges = edges;
        self.dep_reverse_edges = reverse_edges;
        self.lifetimes = lifetimes;
        // alias_groups/alias_stats are computed lazily in allocate_physical_resources()
        self.alias_groups.clear();
        self.alias_stats = None;
        self.alias_redirects.clear();
        self.cached_compiled = compiled_passes.clone();
        self.compiled = true;
        Ok(compiled_passes)
    }
}
