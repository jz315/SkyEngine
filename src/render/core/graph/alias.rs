//! Memory aliasing for transient textures.
//!
//! Adapted from SakuraEngine's `MemoryAliasingPhase` (`memory_aliasing_phase.cpp`).
//!
//! # Algorithm
//!
//! Transient textures whose lifetimes don't overlap and whose formats match
//! exactly can share the same physical `RenderTarget`.  This reduces GPU memory
//! usage proportionally to the overlap achieved.
//!
//! The algorithm follows SakuraEngine's bucket-based best-fit approach:
//!
//! 1. Collect all transient, non-imported textures with valid lifetimes.
//! 2. Sort candidates by resolved pixel count (width × height) descending —
//!    large textures first for best packing.
//! 3. For each candidate, try to fit it into an existing `AliasGroup` (bucket):
//!    - The format must match exactly.
//!    - The candidate's lifetime must NOT overlap with any existing member.
//!    - Among compatible groups, pick the one with the least dimensional waste
//!      (best-fit strategy, SakuraEngine lines 137-160).
//! 4. If no compatible group exists, create a new one.
//! 5. The group's dimensions are `max(width) × max(height)` across all members.
//!
//! # wgpu simplification
//!
//! Unlike SakuraEngine's Vulkan-level implementation which tracks sub-offset
//! memory regions within linear heaps, we share entire `RenderTarget` objects.
//! This is the correct abstraction for wgpu's resource model.

use rustc_hash::{FxHashMap, FxHashSet};

use super::types::{
    resolve_target_size, ResourceLifetime, ResourceRef, TextureDesc, TextureFormat, TextureHandle,
};

// ── Alias group ─────────────────────────────────────────────────────────────

/// A group of transient textures that share one physical `RenderTarget`.
///
/// Equivalent to SakuraEngine's `MemoryBucket`, simplified for wgpu (no
/// sub-offset tracking since wgpu doesn't expose linear heap aliasing).
#[derive(Debug)]
pub(crate) struct AliasGroup {
    /// Virtual texture indices that share this physical resource.
    pub members: Vec<usize>,
    /// The shared format (all members must match exactly).
    pub format: TextureFormat,
    /// The shared usage flags (all members must match exactly).
    pub usage: wgpu::TextureUsages,
    /// The shared sample count (all members must match exactly).
    pub sample_count: u32,
    /// The shared mip count (all members must match exactly).
    pub mip_level_count: u32,
    /// The shared array layer count (all members must match exactly).
    pub array_layer_count: u32,
    /// Maximum width needed across all members.
    pub width: u32,
    /// Maximum height needed across all members.
    pub height: u32,
}

// ── Statistics ──────────────────────────────────────────────────────────────

/// Statistics from the aliasing phase.
///
/// Mirrors SakuraEngine's aliasing result statistics (lines 417-438).
#[derive(Debug, Clone)]
pub struct AliasingStats {
    /// Number of alias groups created.
    pub total_groups: usize,
    /// Number of textures successfully aliased (placed in a shared group).
    pub total_aliased_textures: usize,
    /// How many separate textures would be needed without aliasing.
    pub original_texture_count: usize,
    /// Compression ratio: `1.0 - (groups / original_count)`.
    /// Higher is better.  0.0 means no aliasing.  0.5 means 50% reduction.
    pub compression_ratio: f32,
}

// ── Core algorithm ──────────────────────────────────────────────────────────

/// Compute alias groups for transient textures.
///
/// # Arguments
///
/// * `textures` — all virtual texture descriptors.
/// * `lifetimes` — resource lifetime map (from compilation phase 3).
/// * `handle_token` — the current graph handle token.
/// * `surface_size` — presentation surface dimensions for resolving `TargetSize::Surface`.
///
/// # Returns
///
/// A tuple of `(alias_groups, stats)`.  Each group contains >= 1 member.
/// Groups with exactly 1 member represent textures that could not be aliased
/// (they still participate in the allocation fast path).
#[cfg(test)]
pub(crate) fn compute_texture_aliases(
    textures: &[TextureDesc],
    lifetimes: &FxHashMap<ResourceRef, ResourceLifetime>,
    handle_token: u64,
    surface_size: [u32; 2],
) -> (Vec<AliasGroup>, AliasingStats) {
    let texture_usages = textures.iter().map(|desc| desc.usage).collect::<Vec<_>>();
    compute_texture_aliases_with_forbidden_pairs_and_usages(
        textures,
        &texture_usages,
        lifetimes,
        handle_token,
        surface_size,
        &FxHashSet::default(),
    )
}

#[cfg(test)]
pub(crate) fn compute_texture_aliases_with_forbidden_pairs(
    textures: &[TextureDesc],
    lifetimes: &FxHashMap<ResourceRef, ResourceLifetime>,
    handle_token: u64,
    surface_size: [u32; 2],
    forbidden_pairs: &FxHashSet<(usize, usize)>,
) -> (Vec<AliasGroup>, AliasingStats) {
    let texture_usages = textures.iter().map(|desc| desc.usage).collect::<Vec<_>>();
    compute_texture_aliases_with_forbidden_pairs_and_usages(
        textures,
        &texture_usages,
        lifetimes,
        handle_token,
        surface_size,
        forbidden_pairs,
    )
}

pub(crate) fn compute_texture_aliases_with_forbidden_pairs_and_usages(
    textures: &[TextureDesc],
    texture_usages: &[wgpu::TextureUsages],
    lifetimes: &FxHashMap<ResourceRef, ResourceLifetime>,
    handle_token: u64,
    surface_size: [u32; 2],
    forbidden_pairs: &FxHashSet<(usize, usize)>,
) -> (Vec<AliasGroup>, AliasingStats) {
    debug_assert_eq!(textures.len(), texture_usages.len());
    // ── Step 1: Collect candidates ──────────────────────────────────────
    // Only transient, non-imported textures with a valid lifetime are eligible.
    let mut candidates: Vec<(usize, [u32; 2])> = Vec::new();

    for (tex_idx, desc) in textures.iter().enumerate() {
        if !desc.transient || desc.imported.is_some() {
            continue;
        }

        let handle = TextureHandle(tex_idx, handle_token);
        let resource = ResourceRef::Texture(handle);
        if !lifetimes.contains_key(&resource) {
            continue;
        }

        let [w, h] = resolve_target_size(surface_size, desc.size);
        candidates.push((tex_idx, [w, h]));
    }

    if candidates.is_empty() {
        return (
            Vec::new(),
            AliasingStats {
                total_groups: 0,
                total_aliased_textures: 0,
                original_texture_count: 0,
                compression_ratio: 0.0,
            },
        );
    }

    // ── Step 2: Sort by size descending ──────────────────────────────────
    // SakuraEngine sorts by memory_size descending.  We use pixel count as proxy.
    candidates.sort_by(|a, b| {
        let pixels_a = (a.1[0] as u64) * (a.1[1] as u64);
        let pixels_b = (b.1[0] as u64) * (b.1[1] as u64);
        pixels_b.cmp(&pixels_a)
    });

    // ── Step 3: Best-fit bucket assignment ───────────────────────────────
    // Mirrors SakuraEngine's `perform_memory_aliasing` (lines 108-199).
    let mut groups: Vec<AliasGroup> = Vec::new();
    let original_count = candidates.len();

    for &(tex_idx, [w, h]) in &candidates {
        let desc = &textures[tex_idx];
        let usage = texture_usages[tex_idx];
        let handle = TextureHandle(tex_idx, handle_token);
        let resource = ResourceRef::Texture(handle);
        let lifetime = &lifetimes[&resource];

        let mut best_group_idx: Option<usize> = None;
        let mut best_waste: u64 = u64::MAX;

        // Try best-fit into existing groups (SakuraEngine lines 137-160).
        for (group_idx, group) in groups.iter().enumerate() {
            if group
                .members
                .iter()
                .any(|&member_idx| alias_pair_forbidden(tex_idx, member_idx, forbidden_pairs))
            {
                continue;
            }
            if !can_fit_in_group(
                tex_idx,
                desc.format,
                usage,
                desc.sample_count,
                desc.mip_level_count,
                desc.array_layer_count,
                lifetime,
                group,
                textures,
                lifetimes,
                handle_token,
            ) {
                continue;
            }

            // Calculate waste: how much bigger the group becomes.
            let waste = calculate_group_waste(w, h, group);
            if waste < best_waste {
                best_waste = waste;
                best_group_idx = Some(group_idx);
            }
        }

        if let Some(idx) = best_group_idx {
            // Add to existing group.
            let group = &mut groups[idx];
            group.members.push(tex_idx);
            group.width = group.width.max(w);
            group.height = group.height.max(h);
        } else {
            // Create new group.
            groups.push(AliasGroup {
                members: vec![tex_idx],
                format: desc.format,
                usage,
                sample_count: desc.sample_count,
                mip_level_count: desc.mip_level_count,
                array_layer_count: desc.array_layer_count,
                width: w,
                height: h,
            });
        }
    }

    // ── Step 4: Compute statistics ──────────────────────────────────────
    // Mirrors SakuraEngine's `calculate_aliasing_statistics` (lines 417-438).
    let total_groups = groups.len();
    let total_aliased = groups
        .iter()
        .filter(|g| g.members.len() > 1)
        .map(|g| g.members.len())
        .sum::<usize>();
    let compression_ratio = if original_count > 0 {
        1.0 - (total_groups as f32 / original_count as f32)
    } else {
        0.0
    };

    let stats = AliasingStats {
        total_groups,
        total_aliased_textures: total_aliased,
        original_texture_count: original_count,
        compression_ratio,
    };

    (groups, stats)
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Check if a candidate texture can fit in a group.
///
/// Requirements (mirroring SakuraEngine's `can_resources_alias` + format check):
/// 1. Format must match exactly.
/// 2. Candidate lifetime must not conflict with any existing member.
fn can_fit_in_group(
    candidate_idx: usize,
    candidate_format: TextureFormat,
    candidate_usage: wgpu::TextureUsages,
    candidate_sample_count: u32,
    candidate_mip_level_count: u32,
    candidate_array_layer_count: u32,
    candidate_lifetime: &ResourceLifetime,
    group: &AliasGroup,
    textures: &[TextureDesc],
    lifetimes: &FxHashMap<ResourceRef, ResourceLifetime>,
    handle_token: u64,
) -> bool {
    // Format must match exactly (no cross-format aliasing in wgpu).
    if candidate_format != group.format {
        return false;
    }
    if candidate_usage != group.usage {
        return false;
    }
    if candidate_sample_count != group.sample_count {
        return false;
    }
    if candidate_mip_level_count != group.mip_level_count {
        return false;
    }
    if candidate_array_layer_count != group.array_layer_count {
        return false;
    }

    // Check lifetime conflict with every existing member.
    // Mirrors SakuraEngine's `resources_conflict_in_time` (lines 370-383)
    // via `ResourceLifetime::conflicts_with` (lines 134-139).
    for &member_idx in &group.members {
        if member_idx == candidate_idx {
            return false; // same texture
        }

        let member_handle = TextureHandle(member_idx, handle_token);
        let member_resource = ResourceRef::Texture(member_handle);
        if let Some(member_lifetime) = lifetimes.get(&member_resource) {
            if lifetimes_conflict(candidate_lifetime, member_lifetime) {
                return false;
            }
        } else {
            // No lifetime info — conservative: don't alias.
            return false;
        }
    }

    let _ = textures; // used for potential future checks
    true
}

#[inline]
fn alias_pair_key(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

#[inline]
fn alias_pair_forbidden(a: usize, b: usize, forbidden_pairs: &FxHashSet<(usize, usize)>) -> bool {
    forbidden_pairs.contains(&alias_pair_key(a, b))
}

/// Check if two resource lifetimes conflict (overlap in execution order).
///
/// Mirrors SakuraEngine's `ResourceLifetime::conflicts_with` (lines 134-139):
/// ```c++
/// return !(end_dependency_level < other.start_dependency_level ||
///          other.end_dependency_level < start_dependency_level);
/// ```
#[inline]
fn lifetimes_conflict(a: &ResourceLifetime, b: &ResourceLifetime) -> bool {
    !(a.last_use < b.first_use || b.last_use < a.first_use)
}

/// Calculate "waste" from adding a texture of `(w, h)` to an existing group.
///
/// Returns the total pixel count of the resulting group.  By comparing total
/// pixels rather than just expansion delta, we naturally prefer smaller groups
/// (better fit).  This matches SakuraEngine's `calculate_bucket_waste` intent
/// (lines 659-689): minimize wasted space.
fn calculate_group_waste(w: u32, h: u32, group: &AliasGroup) -> u64 {
    let new_w = group.width.max(w) as u64;
    let new_h = group.height.max(h) as u64;

    // Total pixels of the resulting shared texture.
    // Smaller total = better fit.
    new_w * new_h
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::graph::types::*;
    use std::borrow::Cow;

    const TOKEN: u64 = 42;

    fn make_group(
        members: Vec<usize>,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> AliasGroup {
        AliasGroup {
            members,
            format,
            usage: DEFAULT_TEXTURE_USAGE,
            sample_count: 1,
            mip_level_count: 1,
            array_layer_count: 1,
            width,
            height,
        }
    }

    fn make_group_with_layers(
        members: Vec<usize>,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        array_layer_count: u32,
    ) -> AliasGroup {
        AliasGroup {
            members,
            format,
            usage: DEFAULT_TEXTURE_USAGE,
            sample_count: 1,
            mip_level_count: 1,
            array_layer_count,
            width,
            height,
        }
    }

    fn make_tex(name: &'static str, format: wgpu::TextureFormat, transient: bool) -> TextureDesc {
        TextureDesc {
            name: Cow::Borrowed(name),
            size: TargetSize::Exact(256, 256),
            format,
            usage: DEFAULT_TEXTURE_USAGE,
            sample_count: 1,
            mip_level_count: 1,
            array_layer_count: 1,
            transient,
            imported: None,
        }
    }

    fn make_tex_sized(name: &'static str, w: u32, h: u32) -> TextureDesc {
        TextureDesc {
            name: Cow::Borrowed(name),
            size: TargetSize::Exact(w, h),
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: DEFAULT_TEXTURE_USAGE,
            sample_count: 1,
            mip_level_count: 1,
            array_layer_count: 1,
            transient: true,
            imported: None,
        }
    }

    fn make_tex_with_size(
        name: &'static str,
        size: TargetSize,
        format: wgpu::TextureFormat,
    ) -> TextureDesc {
        TextureDesc {
            name: Cow::Borrowed(name),
            size,
            format,
            usage: DEFAULT_TEXTURE_USAGE,
            sample_count: 1,
            mip_level_count: 1,
            array_layer_count: 1,
            transient: true,
            imported: None,
        }
    }

    fn make_lifetime(first: usize, last: usize) -> ResourceLifetime {
        ResourceLifetime {
            first_use: first,
            last_use: last,
        }
    }

    fn insert_lifetime(
        map: &mut FxHashMap<ResourceRef, ResourceLifetime>,
        idx: usize,
        first: usize,
        last: usize,
    ) {
        map.insert(
            ResourceRef::Texture(TextureHandle(idx, TOKEN)),
            make_lifetime(first, last),
        );
    }

    // ── Basic / empty ───────────────────────────────────────────────────

    #[test]
    fn no_candidates_produces_empty() {
        let textures: Vec<TextureDesc> = vec![];
        let lifetimes = FxHashMap::default();
        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert!(groups.is_empty());
        assert_eq!(stats.total_groups, 0);
        assert_eq!(stats.original_texture_count, 0);
        assert_eq!(stats.compression_ratio, 0.0);
    }

    #[test]
    fn single_transient_texture_one_group() {
        let textures = vec![make_tex("only", wgpu::TextureFormat::Rgba8Unorm, true)];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 2);

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 1);
        assert_eq!(stats.total_aliased_textures, 0);
        assert_eq!(stats.compression_ratio, 0.0); // 1 - 1/1 = 0
    }

    // ── Filtering ───────────────────────────────────────────────────────

    #[test]
    fn imported_excluded() {
        let mut tex = make_tex("imported", wgpu::TextureFormat::Rgba8Unorm, false);
        tex.transient = false;

        let textures = vec![tex];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 2);

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert!(groups.is_empty());
        assert_eq!(stats.original_texture_count, 0);
    }

    #[test]
    fn persistent_excluded() {
        let textures = vec![make_tex(
            "persistent",
            wgpu::TextureFormat::Rgba8Unorm,
            false,
        )];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 2);

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert!(groups.is_empty());
        assert_eq!(stats.original_texture_count, 0);
    }

    #[test]
    fn texture_without_lifetime_excluded() {
        let textures = vec![make_tex("orphan", wgpu::TextureFormat::Rgba8Unorm, true)];
        let lifetimes = FxHashMap::default();

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert!(groups.is_empty());
        assert_eq!(stats.original_texture_count, 0);
    }

    #[test]
    fn mix_of_transient_and_persistent() {
        let textures = vec![
            make_tex("p0", wgpu::TextureFormat::Rgba8Unorm, false),
            make_tex("t0", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("p1", wgpu::TextureFormat::Rgba8Unorm, false),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 5);
        insert_lifetime(&mut lifetimes, 1, 0, 5);
        insert_lifetime(&mut lifetimes, 2, 0, 5);

        let (_groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(stats.original_texture_count, 1);
    }

    // ── Lifetime conflict logic ─────────────────────────────────────────

    #[test]
    fn lifetime_conflict_full_overlap() {
        let a = make_lifetime(0, 5);
        let b = make_lifetime(2, 3);
        assert!(lifetimes_conflict(&a, &b));
        assert!(lifetimes_conflict(&b, &a));
    }

    #[test]
    fn lifetime_conflict_exact_boundary() {
        let a = make_lifetime(0, 2);
        let b = make_lifetime(2, 4);
        assert!(
            lifetimes_conflict(&a, &b),
            "touching boundaries should conflict"
        );
        assert!(lifetimes_conflict(&b, &a));
    }

    #[test]
    fn lifetime_no_conflict_adjacent() {
        let a = make_lifetime(0, 1);
        let b = make_lifetime(2, 3);
        assert!(!lifetimes_conflict(&a, &b));
        assert!(!lifetimes_conflict(&b, &a));
    }

    #[test]
    fn lifetime_no_conflict_widely_separated() {
        let a = make_lifetime(0, 5);
        let b = make_lifetime(100, 200);
        assert!(!lifetimes_conflict(&a, &b));
    }

    #[test]
    fn lifetime_conflict_identical() {
        let a = make_lifetime(3, 3);
        let b = make_lifetime(3, 3);
        assert!(
            lifetimes_conflict(&a, &b),
            "identical single-step lifetimes conflict"
        );
    }

    #[test]
    fn lifetime_single_step_no_conflict() {
        let a = make_lifetime(5, 5);
        let b = make_lifetime(6, 6);
        assert!(!lifetimes_conflict(&a, &b));
    }

    #[test]
    fn lifetime_conflict_one_contains_other() {
        let outer = make_lifetime(0, 100);
        let inner = make_lifetime(25, 75);
        assert!(lifetimes_conflict(&outer, &inner));
        assert!(lifetimes_conflict(&inner, &outer));
    }

    #[test]
    fn lifetime_conflict_off_by_one() {
        // last_use=5, first_use=5 → overlaps at 5.
        assert!(lifetimes_conflict(
            &make_lifetime(0, 5),
            &make_lifetime(5, 10)
        ));
        // last_use=4, first_use=5 → 4 < 5, no overlap.
        assert!(!lifetimes_conflict(
            &make_lifetime(0, 4),
            &make_lifetime(5, 10)
        ));
    }

    // ── Aliasing correctness ────────────────────────────────────────────

    #[test]
    fn no_alias_for_overlapping_lifetimes() {
        let textures = vec![
            make_tex("t0", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("t1", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 2);
        insert_lifetime(&mut lifetimes, 1, 1, 3);

        let (groups, _stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 2, "overlapping lifetimes must not alias");
    }

    #[test]
    fn alias_non_overlapping_same_format() {
        let textures = vec![
            make_tex("t0", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("t1", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 2, 3);

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 1, "non-overlapping same-format should alias");
        assert_eq!(groups[0].members.len(), 2);
        assert_eq!(stats.total_aliased_textures, 2);
        assert!(stats.compression_ratio > 0.0);
    }

    #[test]
    fn no_alias_across_formats() {
        let textures = vec![
            make_tex("t0", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("t1", wgpu::TextureFormat::Rgba16Float, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 2, 3);

        let (groups, _stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 2, "different formats must not alias");
    }

    #[test]
    fn three_sequential_textures_share_one_group() {
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("c", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 2, 3);
        insert_lifetime(&mut lifetimes, 2, 4, 5);

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 3);
        assert_eq!(stats.original_texture_count, 3);
        assert!(stats.compression_ratio > 0.6);
    }

    #[test]
    fn all_overlapping_worst_case() {
        let textures: Vec<TextureDesc> = (0..5)
            .map(|_| make_tex("overlap", wgpu::TextureFormat::Rgba8Unorm, true))
            .collect();
        let mut lifetimes = FxHashMap::default();
        for i in 0..5 {
            insert_lifetime(&mut lifetimes, i, 0, 5);
        }

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 5, "all overlapping → 5 separate groups");
        assert_eq!(stats.total_aliased_textures, 0);
        assert_eq!(stats.compression_ratio, 0.0);
    }

    #[test]
    fn interleaved_lifetimes_two_groups() {
        // T0: [0,2], T1: [1,3], T2: [3,5], T3: [4,6]
        // T0-T1 overlap. T2-T3 overlap. T0-T2 no conflict (2<3). T1-T3 no conflict (3<4).
        let textures: Vec<TextureDesc> = (0..4)
            .map(|_| make_tex("t", wgpu::TextureFormat::Rgba8Unorm, true))
            .collect();
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 2);
        insert_lifetime(&mut lifetimes, 1, 1, 3);
        insert_lifetime(&mut lifetimes, 2, 3, 5);
        insert_lifetime(&mut lifetimes, 3, 4, 6);

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 2, "interleaved should produce 2 groups");
        assert_eq!(stats.total_aliased_textures, 4);
    }

    #[test]
    fn chain_of_short_lifetimes_maximal_aliasing() {
        let n = 10;
        let textures: Vec<TextureDesc> = (0..n)
            .map(|_| make_tex("short", wgpu::TextureFormat::Rgba8Unorm, true))
            .collect();
        let mut lifetimes = FxHashMap::default();
        for i in 0..n {
            insert_lifetime(&mut lifetimes, i, i, i);
        }

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 1, "10 non-overlapping textures → 1 group");
        assert_eq!(groups[0].members.len(), n);
        assert!(stats.compression_ratio > 0.89);
    }

    #[test]
    fn single_pass_lifetime_same_step_conflicts() {
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 5, 5);
        insert_lifetime(&mut lifetimes, 1, 5, 5);

        let (groups, _stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 2, "same-step lifetimes conflict");
    }

    #[test]
    fn single_pass_lifetime_sequential_aliases() {
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 5, 5);
        insert_lifetime(&mut lifetimes, 1, 6, 6);

        let (groups, _stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(
            groups.len(),
            1,
            "adjacent single-step lifetimes should alias"
        );
    }

    // ── Dimensions ──────────────────────────────────────────────────────

    #[test]
    fn group_dimensions_are_max() {
        let textures = vec![
            make_tex_sized("t0", 256, 256),
            make_tex_sized("t1", 512, 128),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 2, 3);

        let (groups, _) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].width, 512);
        assert_eq!(groups[0].height, 256);
    }

    #[test]
    fn best_fit_selects_closest_dimensions() {
        let textures = vec![
            make_tex_sized("big", 512, 512),
            make_tex_sized("small", 256, 256),
            make_tex_sized("tiny", 240, 240),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 0, 1);
        insert_lifetime(&mut lifetimes, 2, 2, 3);

        let (groups, _) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 2);
        let t2_group = groups.iter().find(|g| g.members.contains(&2)).unwrap();
        assert!(
            t2_group.members.contains(&1),
            "T2 should be in T1's group (best-fit), members: {:?}",
            t2_group.members
        );
    }

    #[test]
    fn single_pixel_textures() {
        let textures = vec![make_tex_sized("a", 1, 1), make_tex_sized("b", 1, 1)];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 0);
        insert_lifetime(&mut lifetimes, 1, 1, 1);

        let (groups, _) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].width, 1);
        assert_eq!(groups[0].height, 1);
    }

    #[test]
    fn asymmetric_dimensions_expand_correctly() {
        let textures = vec![
            make_tex_sized("wide", 1024, 64),
            make_tex_sized("tall", 64, 1024),
            make_tex_sized("square", 512, 512),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 0);
        insert_lifetime(&mut lifetimes, 1, 1, 1);
        insert_lifetime(&mut lifetimes, 2, 2, 2);

        let (groups, _) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].width, 1024, "max width across all members");
        assert_eq!(groups[0].height, 1024, "max height across all members");
    }

    #[test]
    fn surface_size_resolution() {
        let textures = vec![
            make_tex_with_size(
                "surf_a",
                TargetSize::Surface,
                wgpu::TextureFormat::Rgba8Unorm,
            ),
            make_tex_with_size(
                "surf_b",
                TargetSize::Surface,
                wgpu::TextureFormat::Rgba8Unorm,
            ),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 0);
        insert_lifetime(&mut lifetimes, 1, 1, 1);

        let (groups, _) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [1920, 1080]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].width, 1920);
        assert_eq!(groups[0].height, 1080);
    }

    #[test]
    fn scale_size_resolution() {
        let textures = vec![
            make_tex_with_size(
                "half_a",
                TargetSize::Scale(0.5),
                wgpu::TextureFormat::Rgba8Unorm,
            ),
            make_tex_with_size(
                "half_b",
                TargetSize::Scale(0.5),
                wgpu::TextureFormat::Rgba8Unorm,
            ),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 0);
        insert_lifetime(&mut lifetimes, 1, 1, 1);

        let (groups, _) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [1920, 1080]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].width, 960);
        assert_eq!(groups[0].height, 540);
    }

    #[test]
    fn mixed_target_sizes_in_one_group() {
        let textures = vec![
            make_tex_with_size("surf", TargetSize::Surface, wgpu::TextureFormat::Rgba8Unorm),
            make_tex_with_size(
                "exact",
                TargetSize::Exact(800, 600),
                wgpu::TextureFormat::Rgba8Unorm,
            ),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 0);
        insert_lifetime(&mut lifetimes, 1, 1, 1);

        let (groups, _) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [1920, 1080]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].width, 1920, "max(1920, 800)");
        assert_eq!(groups[0].height, 1080, "max(1080, 600)");
    }

    // ── Sorting and determinism ─────────────────────────────────────────

    #[test]
    fn large_textures_placed_first_form_anchor() {
        let textures = vec![
            make_tex_sized("small", 64, 64),
            make_tex_sized("large", 1024, 1024),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 0);
        insert_lifetime(&mut lifetimes, 1, 1, 1);

        let (groups, _) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].width, 1024);
        assert_eq!(groups[0].height, 1024);
    }

    // ── Multiple format groups ──────────────────────────────────────────

    #[test]
    fn three_formats_three_groups() {
        let textures = vec![
            make_tex("r8a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("r8b", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("f16a", wgpu::TextureFormat::Rgba16Float, true),
            make_tex("f16b", wgpu::TextureFormat::Rgba16Float, true),
            make_tex("d32a", wgpu::TextureFormat::Depth32Float, true),
            make_tex("d32b", wgpu::TextureFormat::Depth32Float, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 0);
        insert_lifetime(&mut lifetimes, 1, 1, 1);
        insert_lifetime(&mut lifetimes, 2, 0, 0);
        insert_lifetime(&mut lifetimes, 3, 1, 1);
        insert_lifetime(&mut lifetimes, 4, 0, 0);
        insert_lifetime(&mut lifetimes, 5, 1, 1);

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 3, "each format gets its own group");
        assert_eq!(stats.original_texture_count, 6);
        assert!((stats.compression_ratio - 0.5).abs() < 0.01);
    }

    #[test]
    fn srgb_vs_linear_different_groups() {
        let textures = vec![
            make_tex("linear", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("srgb", wgpu::TextureFormat::Rgba8UnormSrgb, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 0);
        insert_lifetime(&mut lifetimes, 1, 1, 1);

        let (groups, _stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 2, "sRGB and linear are different formats");
    }

    #[test]
    fn bgra_vs_rgba_different_groups() {
        let textures = vec![
            make_tex("rgba", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("bgra", wgpu::TextureFormat::Bgra8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 0);
        insert_lifetime(&mut lifetimes, 1, 1, 1);

        let (groups, _) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 2, "BGRA and RGBA are different formats");
    }

    // ── Statistics ──────────────────────────────────────────────────────

    #[test]
    fn stats_compression_ratio() {
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("c", wgpu::TextureFormat::Rgba16Float, true),
            make_tex("d", wgpu::TextureFormat::Rgba16Float, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 2, 3);
        insert_lifetime(&mut lifetimes, 2, 0, 1);
        insert_lifetime(&mut lifetimes, 3, 2, 3);

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 2);
        assert_eq!(stats.original_texture_count, 4);
        assert!((stats.compression_ratio - 0.5).abs() < 0.01);
    }

    #[test]
    fn stats_no_aliasing_when_all_overlap() {
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("c", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 10);
        insert_lifetime(&mut lifetimes, 1, 0, 10);
        insert_lifetime(&mut lifetimes, 2, 0, 10);

        let (_groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(stats.total_aliased_textures, 0);
        assert_eq!(stats.compression_ratio, 0.0);
    }

    #[test]
    fn stats_aliased_count_only_multi_member_groups() {
        // 3 textures: T0 and T1 alias, T2 alone.
        // total_aliased should be 2 (the pair), not 3.
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("c", wgpu::TextureFormat::Rgba16Float, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 0);
        insert_lifetime(&mut lifetimes, 1, 1, 1);
        insert_lifetime(&mut lifetimes, 2, 0, 5);

        let (_groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(stats.total_aliased_textures, 2);
    }

    // ── Stress / large ──────────────────────────────────────────────────

    #[test]
    fn stress_20_sequential_textures() {
        let n = 20;
        let textures: Vec<TextureDesc> = (0..n)
            .map(|_| make_tex("s", wgpu::TextureFormat::Rgba8Unorm, true))
            .collect();
        let mut lifetimes = FxHashMap::default();
        for i in 0..n {
            insert_lifetime(&mut lifetimes, i, i * 2, i * 2 + 1);
        }

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), n);
        assert_eq!(stats.total_aliased_textures, n);
        assert!(stats.compression_ratio > 0.94);
    }

    #[test]
    fn stress_pairwise_overlap_pattern() {
        // T0=[0,1], T1=[1,2], T2=[2,3], ... — each overlaps its neighbor.
        let n = 10;
        let textures: Vec<TextureDesc> = (0..n)
            .map(|_| make_tex("pw", wgpu::TextureFormat::Rgba8Unorm, true))
            .collect();
        let mut lifetimes = FxHashMap::default();
        for i in 0..n {
            insert_lifetime(&mut lifetimes, i, i, i + 1);
        }

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert!(
            groups.len() <= 3,
            "pairwise overlap should produce ≤ 3 groups, got {}",
            groups.len()
        );
        assert!(stats.total_aliased_textures > 0);
    }

    #[test]
    fn stress_50_textures_alternating_two_formats() {
        // 50 textures alternating between two formats, sequential lifetimes.
        // Should produce 2 groups (one per format).
        let n = 50;
        let textures: Vec<TextureDesc> = (0..n)
            .map(|i| {
                if i % 2 == 0 {
                    make_tex("even", wgpu::TextureFormat::Rgba8Unorm, true)
                } else {
                    make_tex("odd", wgpu::TextureFormat::Rgba16Float, true)
                }
            })
            .collect();
        let mut lifetimes = FxHashMap::default();
        for i in 0..n {
            insert_lifetime(&mut lifetimes, i, i, i);
        }

        let (groups, stats) = compute_texture_aliases(&textures, &lifetimes, TOKEN, [800, 600]);
        assert_eq!(groups.len(), 2, "two formats → two groups");
        assert_eq!(stats.original_texture_count, n);
        assert!((stats.compression_ratio - (1.0 - 2.0 / n as f32)).abs() < 0.01);
    }

    // ── Waste calculation ───────────────────────────────────────────────

    #[test]
    fn waste_no_expansion() {
        let group = make_group(vec![0], wgpu::TextureFormat::Rgba8Unorm, 512, 512);
        let waste = calculate_group_waste(256, 256, &group);
        assert_eq!(waste, 512 * 512);
    }

    #[test]
    fn waste_with_expansion() {
        let group = make_group(vec![0], wgpu::TextureFormat::Rgba8Unorm, 256, 256);
        let waste = calculate_group_waste(512, 512, &group);
        assert_eq!(waste, 512 * 512);
    }

    #[test]
    fn waste_comparison_prefers_smaller() {
        let small_group = make_group(vec![0], wgpu::TextureFormat::Rgba8Unorm, 128, 128);
        let large_group = make_group(vec![1], wgpu::TextureFormat::Rgba8Unorm, 1024, 1024);
        let waste_small = calculate_group_waste(100, 100, &small_group);
        let waste_large = calculate_group_waste(100, 100, &large_group);
        assert!(
            waste_small < waste_large,
            "smaller group should have less waste"
        );
    }

    #[test]
    fn waste_partial_width_expansion() {
        // Group is 256×512. Adding 512×256.
        // Result: 512×512 = 262144 pixels.
        let group = make_group(vec![0], wgpu::TextureFormat::Rgba8Unorm, 256, 512);
        let waste = calculate_group_waste(512, 256, &group);
        assert_eq!(waste, 512 * 512);
    }

    // ── can_fit_in_group ────────────────────────────────────────────────

    #[test]
    fn cant_fit_same_texture_index() {
        let group = make_group(vec![0], wgpu::TextureFormat::Rgba8Unorm, 256, 256);
        let textures = vec![make_tex("t0", wgpu::TextureFormat::Rgba8Unorm, true)];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 5);
        let lt = make_lifetime(0, 5);

        let result = can_fit_in_group(
            0,
            wgpu::TextureFormat::Rgba8Unorm,
            DEFAULT_TEXTURE_USAGE,
            1,
            1,
            1,
            &lt,
            &group,
            &textures,
            &lifetimes,
            TOKEN,
        );
        assert!(!result, "same texture index can't be added twice");
    }

    #[test]
    fn cant_fit_wrong_format() {
        let group = make_group(vec![0], wgpu::TextureFormat::Rgba8Unorm, 256, 256);
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba16Float, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 2, 3);
        let lt = make_lifetime(2, 3);

        let result = can_fit_in_group(
            1,
            wgpu::TextureFormat::Rgba16Float,
            DEFAULT_TEXTURE_USAGE,
            1,
            1,
            1,
            &lt,
            &group,
            &textures,
            &lifetimes,
            TOKEN,
        );
        assert!(!result, "format mismatch should prevent fitting");
    }

    #[test]
    fn can_fit_compatible() {
        let group = make_group(vec![0], wgpu::TextureFormat::Rgba8Unorm, 256, 256);
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 2, 3);
        let lt = make_lifetime(2, 3);

        let result = can_fit_in_group(
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            DEFAULT_TEXTURE_USAGE,
            1,
            1,
            1,
            &lt,
            &group,
            &textures,
            &lifetimes,
            TOKEN,
        );
        assert!(
            result,
            "compatible format and non-overlapping lifetime should fit"
        );
    }

    #[test]
    fn cant_fit_missing_lifetime() {
        let group = make_group(vec![0], wgpu::TextureFormat::Rgba8Unorm, 256, 256);
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let lifetimes = FxHashMap::default(); // no lifetimes at all
        let lt = make_lifetime(2, 3);

        let result = can_fit_in_group(
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            DEFAULT_TEXTURE_USAGE,
            1,
            1,
            1,
            &lt,
            &group,
            &textures,
            &lifetimes,
            TOKEN,
        );
        assert!(!result, "missing member lifetime should prevent fitting");
    }

    #[test]
    fn can_fit_multi_member_group() {
        // Group already has indices [0, 1]. Check if index 2 can fit.
        let group = make_group(vec![0, 1], wgpu::TextureFormat::Rgba8Unorm, 256, 256);
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("c", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 2, 3);
        insert_lifetime(&mut lifetimes, 2, 4, 5); // no conflict with either
        let lt = make_lifetime(4, 5);

        let result = can_fit_in_group(
            2,
            wgpu::TextureFormat::Rgba8Unorm,
            DEFAULT_TEXTURE_USAGE,
            1,
            1,
            1,
            &lt,
            &group,
            &textures,
            &lifetimes,
            TOKEN,
        );
        assert!(
            result,
            "index 2 should fit — no lifetime overlap with 0 or 1"
        );
    }

    #[test]
    fn cant_fit_conflicts_with_second_member() {
        // Group has [0, 1]. Index 2 conflicts with index 1 but not 0.
        let group = make_group(vec![0, 1], wgpu::TextureFormat::Rgba8Unorm, 256, 256);
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("c", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 3, 5);
        insert_lifetime(&mut lifetimes, 2, 4, 6); // conflicts with idx 1 @ [3,5]
        let lt = make_lifetime(4, 6);

        let result = can_fit_in_group(
            2,
            wgpu::TextureFormat::Rgba8Unorm,
            DEFAULT_TEXTURE_USAGE,
            1,
            1,
            1,
            &lt,
            &group,
            &textures,
            &lifetimes,
            TOKEN,
        );
        assert!(!result, "conflict with 2nd member should reject");
    }

    #[test]
    fn cant_fit_mismatched_sample_count() {
        let group = make_group(vec![0], wgpu::TextureFormat::Rgba8Unorm, 256, 256);
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 2, 3);
        let lt = make_lifetime(2, 3);

        let result = can_fit_in_group(
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            DEFAULT_TEXTURE_USAGE,
            4,
            1,
            1,
            &lt,
            &group,
            &textures,
            &lifetimes,
            TOKEN,
        );
        assert!(!result, "sample-count mismatch should prevent fitting");
    }

    #[test]
    fn cant_fit_mismatched_array_layer_count() {
        let group = make_group_with_layers(vec![0], wgpu::TextureFormat::Rgba8Unorm, 256, 256, 4);
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 2, 3);
        let lt = make_lifetime(2, 3);

        let result = can_fit_in_group(
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            DEFAULT_TEXTURE_USAGE,
            1,
            1,
            1,
            &lt,
            &group,
            &textures,
            &lifetimes,
            TOKEN,
        );
        assert!(!result, "array-layer-count mismatch should prevent fitting");
    }

    #[test]
    fn alias_rejects_mismatched_texture_usage() {
        let mut group = make_group(vec![0], wgpu::TextureFormat::Rgba8Unorm, 256, 256);
        group.usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 1);
        insert_lifetime(&mut lifetimes, 1, 2, 3);
        let lt = make_lifetime(2, 3);

        let result = can_fit_in_group(
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            1,
            1,
            1,
            &lt,
            &group,
            &textures,
            &lifetimes,
            TOKEN,
        );
        assert!(!result, "texture usage mismatch should prevent fitting");
    }

    #[test]
    fn forbidden_pairs_prevent_aliasing_even_when_lifetimes_do_not_overlap() {
        let textures = vec![
            make_tex("a", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("b", wgpu::TextureFormat::Rgba8Unorm, true),
            make_tex("c", wgpu::TextureFormat::Rgba8Unorm, true),
        ];
        let mut lifetimes = FxHashMap::default();
        insert_lifetime(&mut lifetimes, 0, 0, 0);
        insert_lifetime(&mut lifetimes, 1, 1, 1);
        insert_lifetime(&mut lifetimes, 2, 2, 2);

        let mut forbidden_pairs = FxHashSet::default();
        forbidden_pairs.insert((0, 1));

        let (groups, stats) = compute_texture_aliases_with_forbidden_pairs(
            &textures,
            &lifetimes,
            TOKEN,
            [800, 600],
            &forbidden_pairs,
        );

        assert!(
            groups
                .iter()
                .all(|group| !(group.members.contains(&0) && group.members.contains(&1))),
            "forbidden texture pair must not share an alias group"
        );
        assert_eq!(stats.original_texture_count, 3);
        assert!(
            stats.total_groups >= 2,
            "forbidding one compatible pair should force at least two groups"
        );
    }
}
