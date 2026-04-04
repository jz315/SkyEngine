//! Execution reordering for cache locality and resource lifetime compression.
//!
//! Adapted from SakuraEngine's `ExecutionReorderPhase` (`schedule_reorder.cpp`).
//!
//! # Algorithm
//!
//! Given a valid topological execution order, this module reshuffles passes to
//! improve GPU cache locality by "attracting" passes that share resources closer
//! together.  The approach is a single forward pass with bounded look-ahead:
//!
//! 1. For each position `i` in the order, scan positions `i+2 .. i+1+limit`.
//! 2. For each candidate, compute the Jaccard resource affinity with the pass
//!    at position `i`.
//! 3. If affinity exceeds the threshold AND moving the candidate forward is
//!    DAG-safe (no dependency path between the candidate and any skipped pass),
//!    move it to position `i+1`.
//!
//! The safety check uses BFS through the dependency edges to detect transitive
//! dependencies, matching SakuraEngine's `has_path_between_passes`.

use rustc_hash::FxHashSet;

use super::types::{PassEntry, ResourceRef};

// ── Configuration ───────────────────────────────────────────────────────────

/// Configuration for execution reordering.
///
/// Matches SakuraEngine's `ExecutionReorderConfig`.
pub(crate) struct ReorderConfig {
    /// Attract passes that share resources to improve cache locality.
    pub enable_cache_opt: bool,
    /// Compress resource lifetimes by pulling consumers closer to producers.
    pub enable_lifetime_opt: bool,
    /// Maximum number of positions a pass can be pulled forward.
    /// SakuraEngine default: 10.
    pub max_attraction_distance: usize,
    /// Minimum Jaccard similarity to consider attraction.
    /// SakuraEngine default: 0.1.
    pub min_affinity_score: f32,
}

impl Default for ReorderConfig {
    fn default() -> Self {
        Self {
            enable_cache_opt: true,
            enable_lifetime_opt: true,
            max_attraction_distance: 10,
            min_affinity_score: 0.1,
        }
    }
}

// ── Core algorithm ──────────────────────────────────────────────────────────

/// Reorder alive passes for cache locality and lifetime compression.
///
/// Takes a valid topological `order` (indices into `passes`) and returns a
/// reordered sequence that still respects all dependency edges.
///
/// # Arguments
///
/// * `order` — topologically sorted pass indices (alive only).
/// * `passes` — the full pass array (indexed by pass index).
/// * `edges` — forward dependency edges: `edges[u]` lists passes that depend on `u`.
/// * `reverse_edges` — backward edges: `reverse_edges[v]` lists passes that `v` depends on.
/// * `config` — tuning knobs.
pub(crate) fn reorder_for_affinity(
    order: &[usize],
    passes: &[PassEntry],
    edges: &[Vec<usize>],
    reverse_edges: &[Vec<usize>],
    config: &ReorderConfig,
) -> Vec<usize> {
    if order.len() < 2 || (!config.enable_cache_opt && !config.enable_lifetime_opt) {
        return order.to_vec();
    }

    let mut result = order.to_vec();

    // Single forward pass: for each position, find the best candidate to move
    // to the next position.  Mirrors SakuraEngine's `optimize_queue_with_graph`.
    for current_pos in 0..result.len().saturating_sub(1) {
        let current_pass = result[current_pos];

        let mut best_candidate_pos: Option<usize> = None;
        // When lifetime_opt is enabled, should_attract() already approved
        // any candidate with shared resources.  Use 0.0 so even low-affinity
        // candidates can win the tie-break.  When only cache_opt is active,
        // use the configured threshold as the floor.
        let mut best_affinity: f32 = if config.enable_lifetime_opt {
            0.0
        } else {
            config.min_affinity_score
        };

        // Scan subsequent positions within attraction distance.
        let max_scan = (current_pos + 1 + config.max_attraction_distance).min(result.len());
        for candidate_pos in (current_pos + 1)..max_scan {
            let candidate_pass = result[candidate_pos];

            // Check whether these passes share resources and should be attracted.
            if !should_attract(
                current_pass,
                candidate_pass,
                passes,
                config.enable_cache_opt,
                config.enable_lifetime_opt,
                config.min_affinity_score,
            ) {
                continue;
            }

            // Safety: verify moving `candidate_pos` → `current_pos + 1` doesn't
            // violate any DAG dependency.
            if !can_attract_safely(&result, edges, reverse_edges, current_pos, candidate_pos) {
                continue;
            }

            // Pick the candidate with highest affinity.
            let affinity = calculate_resource_affinity(current_pass, candidate_pass, passes);
            if affinity > best_affinity {
                best_affinity = affinity;
                best_candidate_pos = Some(candidate_pos);
            }
        }

        // Move the best candidate to position `current_pos + 1`.
        if let Some(from) = best_candidate_pos {
            if from != current_pos + 1 {
                attract_pass(&mut result, from, current_pos + 1);
            }
        }
    }

    // Debug assertion: verify the result is still a valid topological order.
    debug_assert!(
        is_valid_topological_order(&result, edges),
        "reorder_for_affinity produced an invalid topological order"
    );

    result
}

// ── Attraction criteria ─────────────────────────────────────────────────────

/// Decide whether `target_pass` should be attracted toward `current_pass`.
///
/// Mirrors SakuraEngine's `should_attract_passes` (lines 141-173).
fn should_attract(
    current_pass: usize,
    target_pass: usize,
    passes: &[PassEntry],
    check_cache: bool,
    check_lifetime: bool,
    min_affinity: f32,
) -> bool {
    if !check_cache && !check_lifetime {
        return false;
    }

    let shared = shared_resource_count(current_pass, target_pass, passes);
    if shared == 0 {
        return false;
    }

    // Lifetime optimisation: any shared resources mean potential compression.
    if check_lifetime {
        return true;
    }

    // Cache optimisation: check affinity threshold.
    if check_cache {
        let affinity = calculate_resource_affinity(current_pass, target_pass, passes);
        return affinity >= min_affinity;
    }

    false
}

// ── Safety check ────────────────────────────────────────────────────────────

/// Check whether moving the pass at `target_pos` to `current_pos + 1` is safe.
///
/// Mirrors SakuraEngine's `can_attract_pass_safely` (lines 109-138) +
/// `has_path_between_passes` (lines 176-221).
///
/// A move is unsafe if there exists a dependency path between the target pass
/// and any "intermediate" pass (those that would be jumped over).
fn can_attract_safely(
    order: &[usize],
    _edges: &[Vec<usize>],
    reverse_edges: &[Vec<usize>],
    current_pos: usize,
    target_pos: usize,
) -> bool {
    if current_pos >= target_pos {
        return false;
    }

    let target_pass = order[target_pos];

    // Check every pass between current_pos+1 and target_pos (exclusive).
    for intermediate_pos in (current_pos + 1)..target_pos {
        let intermediate_pass = order[intermediate_pos];

        // target_pass cannot depend on intermediate (would violate order if moved before it).
        if has_path(target_pass, intermediate_pass, reverse_edges) {
            return false;
        }

        // NOTE: We do NOT check `has_path(intermediate, target)` here.
        // If intermediate depended on target (target → intermediate in the DAG),
        // then the original topological sort would have placed target BEFORE
        // intermediate.  Since target_pos > intermediate_pos, that path cannot
        // exist in a valid topological order.  Checking it would only waste
        // BFS traversals.
    }

    true
}

/// BFS through reverse_edges to check if `from` transitively depends on `to`.
///
/// Returns `true` if there is a directed path from `to` → `from` in the DAG
/// (i.e., `from` depends on `to`).
///
/// Mirrors SakuraEngine's `has_path_between_passes` (lines 176-221).
fn has_path(from: usize, to: usize, reverse_edges: &[Vec<usize>]) -> bool {
    if from == to {
        return false;
    }

    // `reverse_edges[from]` = list of passes that `from` depends on.
    // We BFS backwards from `from` to see if we reach `to`.

    // Quick direct check.
    let deps = &reverse_edges[from];
    if deps.contains(&to) {
        return true;
    }

    // BFS for transitive dependencies.
    let mut visited = FxHashSet::default();
    let mut queue: Vec<usize> = Vec::new();

    for &dep in deps {
        if dep == to {
            return true;
        }
        if visited.insert(dep) {
            queue.push(dep);
        }
    }

    let mut head = 0;
    while head < queue.len() {
        let current = queue[head];
        head += 1;

        for &next_dep in &reverse_edges[current] {
            if next_dep == to {
                return true;
            }
            if visited.insert(next_dep) {
                queue.push(next_dep);
            }
        }
    }

    false
}

// ── Resource affinity ───────────────────────────────────────────────────────

/// Jaccard similarity of resource sets between two passes.
///
/// `|shared| / |union|` where `|union| = |A| + |B| - |shared|`.
///
/// Mirrors SakuraEngine's `calculate_resource_affinity_from_shared` (lines 277-295).
fn calculate_resource_affinity(pass_a: usize, pass_b: usize, passes: &[PassEntry]) -> f32 {
    let a = &passes[pass_a];
    let b = &passes[pass_b];

    let set_a: FxHashSet<ResourceRef> = a.reads.iter().chain(a.writes.iter()).copied().collect();
    let set_b: FxHashSet<ResourceRef> = b.reads.iter().chain(b.writes.iter()).copied().collect();

    let shared = set_a.intersection(&set_b).count() as u32;
    let total = (set_a.len() + set_b.len()) as u32;

    if total == 0 || shared == 0 {
        return 0.0;
    }

    let union_size = total - shared;
    if union_size == 0 {
        return 0.0;
    }

    shared as f32 / union_size as f32
}

/// Count of shared resources between two passes.
///
/// Both sides are deduplicated: if a pass reads AND writes the same resource,
/// it counts as one.  This is consistent with `calculate_resource_affinity`.
fn shared_resource_count(pass_a: usize, pass_b: usize, passes: &[PassEntry]) -> usize {
    let a = &passes[pass_a];
    let b = &passes[pass_b];

    let set_a: FxHashSet<ResourceRef> = a.reads.iter().chain(a.writes.iter()).copied().collect();
    let set_b: FxHashSet<ResourceRef> = b.reads.iter().chain(b.writes.iter()).copied().collect();

    set_a.intersection(&set_b).count()
}

// ── Move helper ─────────────────────────────────────────────────────────────

/// Move an element from `from_pos` to `to_pos` by removing and reinserting.
///
/// Mirrors SakuraEngine's `attract_pass` (lines 251-268).
fn attract_pass(order: &mut Vec<usize>, from_pos: usize, to_pos: usize) {
    if from_pos == to_pos || from_pos >= order.len() || to_pos >= order.len() {
        return;
    }
    let element = order.remove(from_pos);
    order.insert(to_pos, element);
}

// ── Validation ──────────────────────────────────────────────────────────────

/// Verify that `order` respects all forward edges: for every edge (u → v),
/// u appears before v in the order.
fn is_valid_topological_order(order: &[usize], edges: &[Vec<usize>]) -> bool {
    // Build position map: pass_index → position in order.
    let mut pos = vec![usize::MAX; edges.len()];
    for (i, &pass_idx) in order.iter().enumerate() {
        if pass_idx < pos.len() {
            pos[pass_idx] = i;
        }
    }

    for (u, successors) in edges.iter().enumerate() {
        if pos.get(u).copied() == Some(usize::MAX) {
            // u not in order (culled) — skip.
            continue;
        }
        for &v in successors {
            let pos_u = pos.get(u).copied().unwrap_or(usize::MAX);
            let pos_v = pos.get(v).copied().unwrap_or(usize::MAX);
            if pos_u == usize::MAX || pos_v == usize::MAX {
                continue; // one or both culled
            }
            if pos_u >= pos_v {
                return false;
            }
        }
    }

    true
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::graph::types::*;
    use std::borrow::Cow;

    fn make_pass(reads: &[ResourceRef], writes: &[ResourceRef]) -> PassEntry {
        PassEntry {
            name: Cow::Borrowed("test"),
            pass_type: PassType::Render,
            reads: reads.to_vec(),
            writes: writes.to_vec(),
            color_outputs: Vec::new(),
            depth_stencil: None,
            copy_ops: Vec::new(),
            flags: PassFlags::empty(),
            dep_level: 0,
            alive: true,
        }
    }

    fn tex(idx: usize) -> ResourceRef {
        ResourceRef::Texture(TextureHandle(idx, 1))
    }

    fn buf(idx: usize) -> ResourceRef {
        ResourceRef::Buffer(BufferHandle(idx, 1))
    }

    fn default_config() -> ReorderConfig {
        ReorderConfig::default()
    }

    /// Helper: assert every forward edge (u → v) satisfies pos(u) < pos(v).
    fn assert_valid_topo(label: &str, result: &[usize], edges: &[Vec<usize>]) {
        assert!(
            is_valid_topological_order(result, edges),
            "{}: produced invalid topological order: {:?}",
            label,
            result,
        );
    }

    /// Helper: assert result is a permutation of the original order.
    fn assert_same_elements(result: &[usize], expected: &[usize]) {
        let mut a = result.to_vec();
        let mut b = expected.to_vec();
        a.sort();
        b.sort();
        assert_eq!(a, b, "result is not a permutation of the input");
    }

    // ── Basic cases ─────────────────────────────────────────────────────

    #[test]
    fn identity_order_single_pass() {
        let passes = vec![make_pass(&[], &[tex(0)])];
        let edges = vec![vec![]];
        let rev = vec![vec![]];
        let order = vec![0];
        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn identity_order_empty() {
        let passes: Vec<PassEntry> = vec![];
        let edges: Vec<Vec<usize>> = vec![];
        let rev: Vec<Vec<usize>> = vec![];
        let order: Vec<usize> = vec![];
        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());
        assert!(result.is_empty());
    }

    #[test]
    fn two_independent_passes_no_shared_resources() {
        // No shared resources → no attraction → order unchanged.
        let passes = vec![make_pass(&[], &[tex(0)]), make_pass(&[], &[tex(1)])];
        let edges = vec![vec![], vec![]];
        let rev = vec![vec![], vec![]];
        let order = vec![0, 1];
        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn attracts_shared_resources() {
        let t0 = tex(0);
        let t1 = tex(1);
        let passes = vec![
            make_pass(&[], &[t0]),       // P0: writes T0
            make_pass(&[], &[t1]),       // P1: writes T1
            make_pass(&[t0], &[tex(2)]), // P2: reads T0, writes T2
        ];
        let edges = vec![vec![2], vec![], vec![]];
        let rev = vec![vec![], vec![], vec![0]];
        let order = vec![0, 1, 2];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        assert_eq!(result[0], 0, "P0 stays first");
        assert_eq!(result[1], 2, "P2 attracted after P0 (shared T0)");
        assert_eq!(result[2], 1, "P1 pushed to last");
    }

    #[test]
    fn preserves_all_dependencies() {
        let t0 = tex(0);
        let t1 = tex(1);
        let t2 = tex(2);
        let passes = vec![
            make_pass(&[], &[t0]),
            make_pass(&[t0], &[t1]),
            make_pass(&[t1], &[t2]),
            make_pass(&[t2], &[ResourceRef::Surface]),
        ];
        let edges = vec![vec![1], vec![2], vec![3], vec![]];
        let rev = vec![vec![], vec![0], vec![1], vec![2]];
        let order = vec![0, 1, 2, 3];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());
        assert_eq!(result, vec![0, 1, 2, 3]);
    }

    #[test]
    fn diamond_dependency_valid() {
        let t0 = tex(0);
        let t1 = tex(1);
        let t2 = tex(2);
        let passes = vec![
            make_pass(&[], &[t0]),
            make_pass(&[t0], &[t1]),
            make_pass(&[t0], &[t2]),
            make_pass(&[t1, t2], &[ResourceRef::Surface]),
        ];
        let edges = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let rev = vec![vec![], vec![0], vec![0], vec![1, 2]];
        let order = vec![0, 1, 2, 3];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        let pos = |p: usize| result.iter().position(|&x| x == p).unwrap();
        assert!(pos(0) < pos(1));
        assert!(pos(0) < pos(2));
        assert!(pos(1) < pos(3));
        assert!(pos(2) < pos(3));
    }

    #[test]
    fn respects_max_attraction_distance() {
        let t0 = tex(0);
        let mut passes = vec![make_pass(&[], &[t0])];
        for i in 1..=15 {
            passes.push(make_pass(&[], &[tex(100 + i)]));
        }
        passes.push(make_pass(&[t0], &[ResourceRef::Surface]));

        let n = passes.len();
        let mut edges = vec![vec![]; n];
        let mut rev = vec![vec![]; n];
        edges[0].push(n - 1);
        rev[n - 1].push(0);

        let order: Vec<usize> = (0..n).collect();
        let config = ReorderConfig {
            max_attraction_distance: 10,
            ..Default::default()
        };
        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &config);

        let pos_p16 = result.iter().position(|&x| x == n - 1).unwrap();
        assert!(
            pos_p16 > 10,
            "P16 should not be attracted (distance {})",
            pos_p16
        );
    }

    #[test]
    fn disabled_config_returns_identity() {
        let t0 = tex(0);
        let passes = vec![
            make_pass(&[], &[t0]),
            make_pass(&[], &[tex(1)]),
            make_pass(&[t0], &[tex(2)]),
        ];
        let edges = vec![vec![2], vec![], vec![]];
        let rev = vec![vec![], vec![], vec![0]];
        let order = vec![0, 1, 2];

        let config = ReorderConfig {
            enable_cache_opt: false,
            enable_lifetime_opt: false,
            ..Default::default()
        };
        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &config);
        assert_eq!(result, order);
    }

    // ── has_path tests ──────────────────────────────────────────────────

    #[test]
    fn has_path_direct() {
        let rev = vec![vec![], vec![0]];
        assert!(has_path(1, 0, &rev));
        assert!(!has_path(0, 1, &rev));
    }

    #[test]
    fn has_path_transitive() {
        let rev = vec![vec![], vec![0], vec![1]];
        assert!(has_path(2, 0, &rev));
        assert!(!has_path(0, 2, &rev));
    }

    #[test]
    fn has_path_no_self_loop() {
        let rev = vec![vec![0]];
        assert!(!has_path(0, 0, &rev));
    }

    #[test]
    fn has_path_diamond() {
        // 0 → 1, 0 → 2, 1 → 3, 2 → 3
        let rev = vec![vec![], vec![0], vec![0], vec![1, 2]];
        assert!(has_path(3, 0, &rev));
        assert!(has_path(3, 1, &rev));
        assert!(has_path(3, 2, &rev));
        assert!(!has_path(0, 3, &rev));
        assert!(!has_path(1, 2, &rev)); // siblings, no path
        assert!(!has_path(2, 1, &rev));
    }

    #[test]
    fn has_path_long_chain() {
        // 0 → 1 → 2 → ... → 19
        let rev: Vec<Vec<usize>> = (0..20)
            .map(|i| if i == 0 { vec![] } else { vec![i - 1] })
            .collect();
        assert!(has_path(19, 0, &rev));
        assert!(has_path(10, 0, &rev));
        assert!(!has_path(0, 19, &rev));
        assert!(!has_path(5, 10, &rev)); // 5 does NOT depend on 10
    }

    #[test]
    fn has_path_disconnected_components() {
        // Two chains: 0→1→2 and 3→4→5, no cross links.
        let rev = vec![vec![], vec![0], vec![1], vec![], vec![3], vec![4]];
        assert!(has_path(2, 0, &rev));
        assert!(has_path(5, 3, &rev));
        assert!(!has_path(2, 3, &rev));
        assert!(!has_path(5, 0, &rev));
    }

    // ── Jaccard affinity tests ──────────────────────────────────────────

    #[test]
    fn jaccard_affinity_full_overlap() {
        let t0 = tex(0);
        let passes = vec![make_pass(&[], &[t0]), make_pass(&[t0], &[])];
        let affinity = calculate_resource_affinity(0, 1, &passes);
        assert!((affinity - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn jaccard_affinity_no_overlap() {
        let passes = vec![make_pass(&[], &[tex(0)]), make_pass(&[], &[tex(1)])];
        let affinity = calculate_resource_affinity(0, 1, &passes);
        assert_eq!(affinity, 0.0);
    }

    #[test]
    fn jaccard_affinity_partial() {
        let t0 = tex(0);
        let passes = vec![make_pass(&[], &[t0, tex(1)]), make_pass(&[t0], &[tex(2)])];
        // shared=1, union=3. affinity = 1/3
        let affinity = calculate_resource_affinity(0, 1, &passes);
        assert!((affinity - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn jaccard_affinity_both_empty() {
        let passes = vec![make_pass(&[], &[]), make_pass(&[], &[])];
        let affinity = calculate_resource_affinity(0, 1, &passes);
        assert_eq!(affinity, 0.0);
    }

    #[test]
    fn jaccard_affinity_one_empty() {
        let passes = vec![make_pass(&[], &[tex(0), tex(1)]), make_pass(&[], &[])];
        let affinity = calculate_resource_affinity(0, 1, &passes);
        assert_eq!(affinity, 0.0);
    }

    #[test]
    fn jaccard_readwrite_dedup() {
        // Pass A reads AND writes T0 — the resource set should deduplicate.
        let t0 = tex(0);
        let passes = vec![
            make_pass(&[t0], &[t0, tex(1)]), // A: reads T0, writes T0+T1
            make_pass(&[t0], &[tex(2)]),     // B: reads T0, writes T2
        ];
        // A's unique set = {T0, T1}, B's = {T0, T2}. shared=1, union=3
        let affinity = calculate_resource_affinity(0, 1, &passes);
        assert!((affinity - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn jaccard_with_surface_resource() {
        // Surface is a valid ResourceRef participating in affinity.
        let passes = vec![
            make_pass(&[], &[ResourceRef::Surface, tex(0)]),
            make_pass(&[ResourceRef::Surface], &[tex(1)]),
        ];
        // shared = {Surface}, A={Surface,T0}, B={Surface,T1}. union=3, affinity=1/3
        let affinity = calculate_resource_affinity(0, 1, &passes);
        assert!((affinity - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn jaccard_with_buffer_resource() {
        // Buffer resources should participate in affinity just like textures.
        let b0 = buf(0);
        let passes = vec![make_pass(&[], &[b0, tex(0)]), make_pass(&[b0], &[tex(1)])];
        let affinity = calculate_resource_affinity(0, 1, &passes);
        assert!(
            affinity > 0.0,
            "buffer sharing should produce nonzero affinity"
        );
    }

    #[test]
    fn jaccard_many_shared_few_unique() {
        // A shares 5 resources with B, each has 1 unique.
        // shared=5, A=6, B=6, union=7. affinity = 5/7 ≈ 0.714
        let shared: Vec<ResourceRef> = (0..5).map(|i| tex(i)).collect();
        let passes = vec![
            make_pass(&shared, &[tex(100)]),
            make_pass(&shared, &[tex(101)]),
        ];
        let affinity = calculate_resource_affinity(0, 1, &passes);
        assert!((affinity - 5.0 / 7.0).abs() < 0.01);
    }

    // ── Topological order validation ────────────────────────────────────

    #[test]
    fn is_valid_topo_order_accepts_correct() {
        let edges = vec![vec![1], vec![2], vec![]];
        assert!(is_valid_topological_order(&[0, 1, 2], &edges));
    }

    #[test]
    fn is_valid_topo_order_rejects_incorrect() {
        let edges = vec![vec![1], vec![2], vec![]];
        assert!(!is_valid_topological_order(&[2, 1, 0], &edges));
    }

    #[test]
    fn is_valid_topo_partial_order() {
        // 0→2, 1→2.  Both [0,1,2] and [1,0,2] are valid.
        let edges = vec![vec![2], vec![2], vec![]];
        assert!(is_valid_topological_order(&[0, 1, 2], &edges));
        assert!(is_valid_topological_order(&[1, 0, 2], &edges));
        assert!(!is_valid_topological_order(&[2, 0, 1], &edges));
    }

    #[test]
    fn is_valid_topo_with_culled_nodes() {
        // Edges exist but nodes not in the order (culled).
        let edges = vec![vec![1], vec![2], vec![3], vec![]];
        // Order only has 0 and 3 (1, 2 culled).
        assert!(is_valid_topological_order(&[0, 3], &edges));
    }

    // ── Advanced reorder scenarios ──────────────────────────────────────

    #[test]
    fn transitive_dependency_blocks_attraction() {
        // P0 writes T0, P1 depends on P0, P2 reads T0 AND depends on P1.
        // P2 shares T0 with P0 and would want to be attracted, but it
        // transitively depends on P1 via P0→P1→P2. Moving P2 before P1
        // would violate the P1→P2 dependency.
        let t0 = tex(0);
        let t1 = tex(1);
        let passes = vec![
            make_pass(&[], &[t0]),           // P0: writes T0
            make_pass(&[t0], &[t1]),         // P1: reads T0, writes T1
            make_pass(&[t0, t1], &[tex(2)]), // P2: reads T0+T1
        ];
        let edges = vec![vec![1, 2], vec![2], vec![]];
        let rev = vec![vec![], vec![0], vec![0, 1]];
        let order = vec![0, 1, 2];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        // P2 is already at position 2, right after P1. It can't move before P1.
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn highest_affinity_wins_among_candidates() {
        // P0 writes T0+T1, P1 independent, P2 reads T0 only, P3 reads T0+T1.
        // P3 has higher affinity with P0 than P2 does.  Both are safe to move.
        // P3 should be attracted first.
        let t0 = tex(0);
        let t1 = tex(1);
        let passes = vec![
            make_pass(&[], &[t0, t1]),       // P0: {T0, T1}
            make_pass(&[], &[tex(99)]),      // P1: unrelated
            make_pass(&[t0], &[tex(2)]),     // P2: {T0, T2} — affinity 1/3
            make_pass(&[t0, t1], &[tex(3)]), // P3: {T0, T1, T3} — affinity 2/3
        ];
        let edges = vec![vec![2, 3], vec![], vec![], vec![]];
        let rev = vec![vec![], vec![], vec![0], vec![0]];
        let order = vec![0, 1, 2, 3];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        assert_eq!(result[0], 0);
        assert_eq!(result[1], 3, "P3 should win (higher affinity with P0)");
        assert_valid_topo("highest_affinity", &result, &edges);
        assert_same_elements(&result, &order);
    }

    #[test]
    fn cascading_attraction() {
        // P0→T0, P1→T1, P2→T2, P3 reads T0, P4 reads T1.
        // After P3 is attracted to P0, the next iteration should attract P4 to P1.
        // Order: [P0 P1 P2 P3 P4]
        //   step 0: P0 attracts P3 → [P0 P3 P1 P2 P4]
        //   step 2: P1 attracts P4 → [P0 P3 P1 P4 P2]
        let t0 = tex(0);
        let t1 = tex(1);
        let passes = vec![
            make_pass(&[], &[t0]),       // P0
            make_pass(&[], &[t1]),       // P1
            make_pass(&[], &[tex(2)]),   // P2 — unrelated filler
            make_pass(&[t0], &[tex(3)]), // P3 — shares T0 with P0
            make_pass(&[t1], &[tex(4)]), // P4 — shares T1 with P1
        ];
        let edges = vec![vec![3], vec![4], vec![], vec![], vec![]];
        let rev = vec![vec![], vec![], vec![], vec![0], vec![1]];
        let order = vec![0, 1, 2, 3, 4];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        // P3 should be next to P0, P4 should be next to P1.
        let pos = |p: usize| result.iter().position(|&x| x == p).unwrap();
        assert_eq!(pos(3), pos(0) + 1, "P3 adjacent to P0");
        // P4 should be close to P1.
        assert!(
            (pos(4) as isize - pos(1) as isize).unsigned_abs() <= 2,
            "P4 should be near P1 after cascading, positions: {:?}",
            result
        );
        assert_valid_topo("cascading", &result, &edges);
    }

    #[test]
    fn wide_fan_out_all_siblings_independent() {
        // P0 is root, P1..P5 all read T0 from P0, no inter-sibling deps.
        // P6 writes T99 (unrelated filler between P0 and siblings).
        // Original order: [P0, P6, P1, P2, P3, P4, P5]
        // P1 should be attracted to right after P0, pushing P6 later.
        let t0 = tex(0);
        let passes = vec![
            make_pass(&[], &[t0]),       // P0
            make_pass(&[t0], &[tex(1)]), // P1
            make_pass(&[t0], &[tex(2)]), // P2
            make_pass(&[t0], &[tex(3)]), // P3
            make_pass(&[t0], &[tex(4)]), // P4
            make_pass(&[t0], &[tex(5)]), // P5
            make_pass(&[], &[tex(99)]),  // P6 — filler
        ];
        let edges = vec![
            vec![1, 2, 3, 4, 5],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        ];
        let rev = vec![vec![], vec![0], vec![0], vec![0], vec![0], vec![0], vec![]];
        let order = vec![0, 6, 1, 2, 3, 4, 5];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        // P0 stays first. One of P1-P5 should be attracted to position 1.
        assert_eq!(result[0], 0);
        let pos1_pass = result[1];
        assert!(
            (1..=5).contains(&pos1_pass),
            "A sibling should be attracted to position 1, got P{}",
            pos1_pass
        );
        assert_valid_topo("fan_out", &result, &edges);
        assert_same_elements(&result, &order);
    }

    #[test]
    fn two_independent_subgraphs_share_resource() {
        // Subgraph A: P0 → P1 (via T0)
        // Subgraph B: P2 → P3 (via T1)
        // P0 and P2 also share T_common — but P0→P2 has no DAG edge.
        // Original: [P0, P2, P1, P3]
        // P1 shares T0 with P0 and should be attracted after P0.
        let t0 = tex(0);
        let t1 = tex(1);
        let t_common = tex(99);
        let passes = vec![
            make_pass(&[], &[t0, t_common]), // P0
            make_pass(&[t0], &[tex(2)]),     // P1 — depends on P0
            make_pass(&[t_common], &[t1]),   // P2 — reads t_common
            make_pass(&[t1], &[tex(3)]),     // P3 — depends on P2
        ];
        let edges = vec![vec![1, 2], vec![], vec![3], vec![]];
        let rev = vec![vec![], vec![0], vec![0], vec![2]];
        let order = vec![0, 2, 1, 3];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        assert_valid_topo("two_subgraphs", &result, &edges);
        assert_same_elements(&result, &order);
    }

    #[test]
    fn all_passes_share_one_resource() {
        // Every pass reads/writes T0. This stress-tests the algorithm:
        // chain: P0 → P1 → P2 → P3 → P4
        // No reordering should be possible (strict chain), but affinity is 1.0
        // for all pairs.
        let t0 = tex(0);
        let n = 5;
        let mut passes = vec![make_pass(&[], &[t0])];
        for i in 1..n {
            passes.push(make_pass(&[t0], &[t0, tex(100 + i)]));
        }
        let mut edges = vec![vec![]; n];
        let mut rev = vec![vec![]; n];
        for i in 0..n - 1 {
            edges[i].push(i + 1);
            rev[i + 1].push(i);
        }
        let order: Vec<usize> = (0..n).collect();

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        // Strict chain → no movement possible.
        assert_eq!(result, order);
    }

    #[test]
    fn fork_join_with_unrelated_interloper() {
        // P0 → P1 and P0 → P2.  P1 and P2 join at P3.
        // P_interloper is unrelated, positioned between P1 and P2.
        //
        //     P0 → P1  → P3
        //     P0 → P2 ↗
        //           P_int (no deps, no shared resources)
        //
        // Original: [P0, P1, P_int, P2, P3]
        // The reorder should try to move P2 closer to P1 (they share nothing
        // with P1 directly, but both share T0 from P0). P_int has no affinity
        // anywhere.
        let t0 = tex(0);
        let t1 = tex(1);
        let t2 = tex(2);
        let passes = vec![
            make_pass(&[], &[t0]),                         // P0
            make_pass(&[t0], &[t1]),                       // P1
            make_pass(&[t0], &[t2]),                       // P2
            make_pass(&[t1, t2], &[ResourceRef::Surface]), // P3
            make_pass(&[], &[tex(50)]),                    // P4 (interloper)
        ];
        let edges = vec![vec![1, 2], vec![3], vec![3], vec![], vec![]];
        let rev = vec![vec![], vec![0], vec![0], vec![1, 2], vec![]];
        let order = vec![0, 1, 4, 2, 3];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        let pos = |p: usize| result.iter().position(|&x| x == p).unwrap();
        assert!(pos(0) < pos(1));
        assert!(pos(0) < pos(2));
        assert!(pos(1) < pos(3));
        assert!(pos(2) < pos(3));
        assert_valid_topo("fork_join_interloper", &result, &edges);
        assert_same_elements(&result, &order);
    }

    #[test]
    fn attraction_blocked_by_intermediate_dependency() {
        // P0 writes T0, P1 unrelated, P2 reads T0 but depends on P1.
        // P2 cannot be moved before P1 even though it shares T0 with P0.
        //
        // DAG: P0 → P2, P1 → P2
        let t0 = tex(0);
        let passes = vec![
            make_pass(&[], &[t0]),               // P0
            make_pass(&[], &[tex(1)]),           // P1
            make_pass(&[t0, tex(1)], &[tex(2)]), // P2 — depends on both P0 and P1
        ];
        let edges = vec![vec![2], vec![2], vec![]];
        let rev = vec![vec![], vec![], vec![0, 1]];
        let order = vec![0, 1, 2];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        // P2 can't move before P1. Order must remain [0, 1, 2].
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn multiple_blockers_in_sequence() {
        // P0 writes T0. P1→P2→P3 is a chain (no shared resources with P0).
        // P4 reads T0 (shares with P0).
        // P4 cannot jump over P1,P2,P3 if there are dep edges between them
        // and P4.
        //
        // DAG: P0→P4, P1→P2, P2→P3, P3→P4
        let t0 = tex(0);
        let passes = vec![
            make_pass(&[], &[t0]),                // P0
            make_pass(&[], &[tex(10)]),           // P1
            make_pass(&[tex(10)], &[tex(11)]),    // P2
            make_pass(&[tex(11)], &[tex(12)]),    // P3
            make_pass(&[t0, tex(12)], &[tex(4)]), // P4 — depends on P0 and P3
        ];
        let edges = vec![vec![4], vec![2], vec![3], vec![4], vec![]];
        let rev = vec![vec![], vec![], vec![1], vec![2], vec![0, 3]];
        let order = vec![0, 1, 2, 3, 4];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        // P4 depends on P3 which depends on P2 which depends on P1.
        // So P4 can't move before any of them.
        assert_eq!(result, vec![0, 1, 2, 3, 4]);
        assert_valid_topo("multiple_blockers", &result, &edges);
    }

    #[test]
    fn attraction_distance_exactly_one() {
        // With max_attraction_distance=1, only the immediate next position is
        // considered.  If the next pass doesn't share resources, no swap happens.
        let t0 = tex(0);
        let passes = vec![
            make_pass(&[], &[t0]),
            make_pass(&[], &[tex(1)]),   // filler
            make_pass(&[t0], &[tex(2)]), // shares T0 but 2 positions away
        ];
        let edges = vec![vec![2], vec![], vec![]];
        let rev = vec![vec![], vec![], vec![0]];
        let order = vec![0, 1, 2];

        let config = ReorderConfig {
            max_attraction_distance: 1,
            ..Default::default()
        };
        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &config);

        // P2 is at distance 2 from P0, but max_attraction_distance=1 only
        // scans position 1. So P2 can't be attracted.
        assert_eq!(result, vec![0, 1, 2]);
    }

    #[test]
    fn attraction_distance_exactly_fits() {
        // max_attraction_distance=2. P2 is exactly 2 positions from P0.
        // Should be attracted.
        let t0 = tex(0);
        let passes = vec![
            make_pass(&[], &[t0]),
            make_pass(&[], &[tex(1)]),
            make_pass(&[t0], &[tex(2)]),
        ];
        let edges = vec![vec![2], vec![], vec![]];
        let rev = vec![vec![], vec![], vec![0]];
        let order = vec![0, 1, 2];

        let config = ReorderConfig {
            max_attraction_distance: 2,
            ..Default::default()
        };
        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &config);

        assert_eq!(result[0], 0);
        assert_eq!(result[1], 2, "P2 should be attracted (within distance 2)");
    }

    #[test]
    fn cache_only_mode_respects_threshold() {
        // enable_lifetime_opt=false, enable_cache_opt=true.
        // Only high-affinity pairs should be attracted.
        let t0 = tex(0);
        let passes = vec![
            make_pass(&[], &[t0, tex(1), tex(2), tex(3), tex(4)]), // P0: 5 resources
            make_pass(&[], &[tex(99)]),                            // P1: filler
            make_pass(&[t0], &[tex(5), tex(6), tex(7), tex(8)]),   // P2: shares only T0 with P0
        ];
        // Affinity = 1 / (5+5-1) = 1/9 ≈ 0.111
        let edges = vec![vec![2], vec![], vec![]];
        let rev = vec![vec![], vec![], vec![0]];
        let order = vec![0, 1, 2];

        // With threshold 0.5, affinity 0.111 is below threshold.
        let config = ReorderConfig {
            enable_cache_opt: true,
            enable_lifetime_opt: false,
            min_affinity_score: 0.5,
            ..Default::default()
        };
        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &config);

        assert_eq!(
            result,
            vec![0, 1, 2],
            "low affinity should not trigger cache-only attraction"
        );
    }

    #[test]
    fn lifetime_only_mode_ignores_threshold() {
        // enable_lifetime_opt=true, enable_cache_opt=false.
        // Any shared resource should trigger attraction, regardless of Jaccard.
        let t0 = tex(0);
        let passes = vec![
            make_pass(&[], &[t0, tex(1), tex(2), tex(3), tex(4)]), // 5 resources
            make_pass(&[], &[tex(99)]),
            make_pass(&[t0], &[tex(5), tex(6), tex(7), tex(8)]), // shares only T0
        ];
        let edges = vec![vec![2], vec![], vec![]];
        let rev = vec![vec![], vec![], vec![0]];
        let order = vec![0, 1, 2];

        let config = ReorderConfig {
            enable_cache_opt: false,
            enable_lifetime_opt: true,
            min_affinity_score: 0.5, // should be ignored in lifetime mode
            ..Default::default()
        };
        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &config);

        assert_eq!(
            result[1], 2,
            "lifetime mode should attract regardless of threshold"
        );
    }

    #[test]
    fn stress_50_independent_passes_with_sparse_sharing() {
        // 50 passes, mostly independent. P0 shares T0 with P5 and P8.
        // With max_attraction_distance=10, both are reachable.
        let n = 50;
        let t0 = tex(0);
        let mut passes = Vec::with_capacity(n);
        passes.push(make_pass(&[], &[t0])); // P0
        for i in 1..n {
            if i == 5 || i == 8 {
                passes.push(make_pass(&[t0], &[tex(1000 + i)]));
            } else {
                passes.push(make_pass(&[], &[tex(1000 + i)]));
            }
        }

        let mut edges = vec![vec![]; n];
        let mut rev = vec![vec![]; n];
        edges[0].push(5);
        edges[0].push(8);
        rev[5].push(0);
        rev[8].push(0);

        let order: Vec<usize> = (0..n).collect();

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        assert_valid_topo("stress_50", &result, &edges);
        assert_same_elements(&result, &order);

        // One of P5/P8 should be attracted near P0.
        let pos0 = result.iter().position(|&x| x == 0).unwrap();
        let pos5 = result.iter().position(|&x| x == 5).unwrap();
        let pos8 = result.iter().position(|&x| x == 8).unwrap();
        let nearest = pos5.min(pos8);
        assert!(
            nearest <= pos0 + 2,
            "One of P5/P8 should be attracted near P0, pos0={}, pos5={}, pos8={}",
            pos0,
            pos5,
            pos8
        );
    }

    #[test]
    fn output_preserves_all_elements() {
        // Regardless of any reordering, the output must contain exactly
        // the same elements as the input.
        let t0 = tex(0);
        let passes = vec![
            make_pass(&[], &[t0]),
            make_pass(&[], &[tex(1)]),
            make_pass(&[t0], &[tex(2)]),
            make_pass(&[], &[tex(3)]),
            make_pass(&[tex(3)], &[ResourceRef::Surface]),
        ];
        let edges = vec![vec![2], vec![], vec![], vec![4], vec![]];
        let rev = vec![vec![], vec![], vec![0], vec![], vec![3]];
        let order = vec![0, 1, 3, 2, 4];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());

        assert_same_elements(&result, &order);
        assert_valid_topo("preserve_elements", &result, &edges);
    }

    #[test]
    fn already_optimal_unchanged() {
        // P0→T0, P1 reads T0 (already adjacent). No improvement possible.
        let t0 = tex(0);
        let passes = vec![
            make_pass(&[], &[t0]),
            make_pass(&[t0], &[ResourceRef::Surface]),
        ];
        let edges = vec![vec![1], vec![]];
        let rev = vec![vec![], vec![0]];
        let order = vec![0, 1];

        let result = reorder_for_affinity(&order, &passes, &edges, &rev, &default_config());
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn attract_pass_noop_same_position() {
        let mut v = vec![1, 2, 3, 4];
        attract_pass(&mut v, 2, 2);
        assert_eq!(v, vec![1, 2, 3, 4]);
    }

    #[test]
    fn attract_pass_out_of_bounds() {
        let mut v = vec![1, 2, 3];
        attract_pass(&mut v, 10, 1);
        assert_eq!(v, vec![1, 2, 3]);
    }

    #[test]
    fn attract_pass_forward() {
        let mut v = vec![10, 20, 30, 40, 50];
        attract_pass(&mut v, 3, 1); // move element at pos 3 (40) to pos 1
        assert_eq!(v, vec![10, 40, 20, 30, 50]);
    }

    #[test]
    fn shared_resource_count_empty() {
        let passes = vec![make_pass(&[], &[]), make_pass(&[], &[])];
        assert_eq!(shared_resource_count(0, 1, &passes), 0);
    }

    #[test]
    fn shared_resource_count_all_shared() {
        let t0 = tex(0);
        let t1 = tex(1);
        let passes = vec![make_pass(&[t0, t1], &[]), make_pass(&[t0, t1], &[])];
        assert_eq!(shared_resource_count(0, 1, &passes), 2);
    }
}
