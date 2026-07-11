//! Declarative render-graph orchestration.
//!
//! # Architecture
//!
//! The render graph is a **declarative**, **virtual resource** system inspired
//! by SakuraEngine's RenderGraph.  Passes declare their resource reads/writes
//! through builder closures, and the graph automatically:
//!
//! 1. Resolves dependencies via topological sort
//! 2. Culls unused passes (dead-code elimination)
//! 3. Tracks virtual resource lifetimes
//! 4. Allocates/recycles physical `RenderTarget` objects from an internal pool
//! 5. Executes passes in dependency order
//!
//! # Pass types
//!
//! - **Render pass**: rasterisation draw calls (`add_render_pass`)
//! - **Compute pass**: compute shader dispatches (`add_compute_pass`)
//! - **Copy pass**: resource-to-resource copies (`add_copy_pass`)
//!
//! # Example
//!
//! ```rust,ignore
//! let mut graph = RenderGraph::new();
//!
//! let hdr = graph.create_texture(|b| {
//!     b.name("hdr_color")
//!      .size(TargetSize::Surface)
//!      .format(TextureFormat::Rgba16Float);
//! });
//!
//! graph.add_render_pass("scene", |setup| {
//!     setup.write_color(0, hdr);
//! });
//!
//! graph.add_render_pass("post_fx", |setup| {
//!     setup.read(hdr);
//!     setup.write_surface();
//! });
//!
//! // Option A: execute all passes (compile + allocate + run):
//! graph.try_execute(&mut ctx, |pass, ctx, resources| {
//!     // draw using pass.name, pass.color_outputs, etc.
//!     Ok(())
//! })?;
//!
//! // Option B: compile only (for inspection / debug):
//! let compiled: Vec<CompiledPass> = graph.compile()?;
//! for pass in &compiled {
//!     println!("{}: {:?}", pass.name, pass.pass_type);
//! }
//! ```

mod access;
mod alias;
mod allocate;
mod builder;
mod compile;
mod debug;
mod error;
mod execute;
mod pool;
mod queue_diagnostic;
mod reorder;
#[cfg(test)]
mod tests;
mod types;
mod visualize;

use std::borrow::Cow;
use std::sync::atomic::{AtomicU64, Ordering};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::gpu::GpuContext;
use crate::render::gpu::RenderTarget;
use crate::render::resources::blackboard::Blackboard;

pub use alias::AliasingStats;
pub use builder::{BufferBuilder, CopyPassSetup, PassSetup, TextureBuilder};
pub use debug::{
    CopyOpDebug, RenderGraphAliasGroupDebug, RenderGraphAliasMemberDebug,
    RenderGraphAliasRedirectDebug, RenderGraphBufferResourceDebug, RenderGraphDebugDump,
    RenderGraphLifetimeDebug, RenderGraphPassDebug, RenderGraphResourceDebug,
    RenderGraphResourceKind, RenderGraphTextureResourceDebug,
};
#[cfg(feature = "profile")]
pub use error::SkyProfileRenderGraphProfiler;
pub use error::{DebugProfiler, RenderGraphError, RenderGraphProfiler};
pub use queue_diagnostic::{
    QueueAssignmentDiagnostic, QueueDiagnosticClass, QueueScheduleBlocker, QueueScheduleDiagnostic,
    QueueScheduleReason,
};
pub use types::*;
pub use visualize::RenderGraphDotOptions;

use pool::{BufferPoolKey, PoolKey, TransientBufferPool, TransientPool};

static NEXT_HANDLE_TOKEN: AtomicU64 = AtomicU64::new(1);

fn next_handle_token() -> u64 {
    NEXT_HANDLE_TOKEN.fetch_add(1, Ordering::Relaxed)
}

// ── The Render Graph ────────────────────────────────────────────────────────

/// Declarative render graph with virtual resources and automatic scheduling.
///
/// The graph is split into two phases:
///
/// 1. **Declaration** — register virtual resources and passes with their
///    read/write dependencies using builder closures.
/// 2. **Execution** — call [`Self::compile`] then [`Self::execute`] to allocate physical
///    resources and run passes in dependency order.
///
/// Passes don't contain closures; instead, `compile()` returns a
/// `Vec<CompiledPass>` and the caller drives execution.  For the common
/// "self-contained graph" case, use [`Self::execute`] which does everything.
pub struct RenderGraph {
    handle_token: u64,

    // Virtual resource descriptors
    textures: Vec<TextureDesc>,
    buffers: Vec<BufferDesc>,

    // Name → handle lookup (Sakura-style)
    texture_names: FxHashMap<String, TextureHandle>,
    buffer_names: FxHashMap<String, BufferHandle>,

    // Passes (declaration only, no closures)
    passes: Vec<PassEntry>,

    // Compilation results (cached to avoid per-frame recomputation)
    order: Vec<usize>,
    cached_compiled: Vec<CompiledPass>,
    compiled: bool,
    max_dep_level: u32,
    culled_count: usize,

    // Dependency edges (kept for reorder phase)
    dep_edges: Vec<Vec<usize>>,
    dep_reverse_edges: Vec<Vec<usize>>,

    // Resource lifetime tracking
    lifetimes: FxHashMap<ResourceRef, ResourceLifetime>,

    // Memory aliasing (SakuraEngine-inspired)
    alias_groups: Vec<alias::AliasGroup>,
    alias_stats: Option<alias::AliasingStats>,
    /// Redirect map: secondary alias member tex_idx → primary tex_idx.
    /// Used so all members in an alias group share one physical RenderTarget.
    alias_redirects: FxHashMap<usize, usize>,

    // Physical resource management
    physical_textures: Vec<Option<RenderTarget>>,
    physical_buffers: Vec<Option<wgpu::Buffer>>,
    persistent_texture_cache: FxHashMap<String, Vec<RenderTarget>>,
    persistent_buffer_cache: FxHashMap<String, Vec<wgpu::Buffer>>,
    transient_pool: TransientPool,
    transient_buffer_pool: TransientBufferPool,

    // Cross-pass data sharing
    blackboard: Blackboard,

    // Lightweight diagnostics for cache-audit evidence.
    view_stats: PhysicalResourceViewStatsCounters,
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderGraph {
    fn texture_handle_is_valid(&self, handle: TextureHandle) -> bool {
        handle.1 == self.handle_token && handle.0 < self.textures.len()
    }

    fn texture_subresource_is_valid(&self, subresource: TextureSubresource) -> bool {
        if !self.texture_handle_is_valid(subresource.texture) {
            return false;
        }
        let desc = &self.textures[subresource.texture.0];
        if subresource.mip_level_count == 0 || subresource.array_layer_count == 0 {
            return false;
        }
        let Some(mip_end) = subresource
            .base_mip_level
            .checked_add(subresource.mip_level_count)
        else {
            return false;
        };
        let Some(layer_end) = subresource
            .base_array_layer
            .checked_add(subresource.array_layer_count)
        else {
            return false;
        };
        subresource.base_mip_level < desc.mip_level_count
            && mip_end <= desc.mip_level_count
            && subresource.base_array_layer < desc.array_layer_count
            && layer_end <= desc.array_layer_count
    }

    fn buffer_handle_is_valid(&self, handle: BufferHandle) -> bool {
        handle.1 == self.handle_token && handle.0 < self.buffers.len()
    }

    fn resource_handle_is_valid(&self, resource: ResourceRef) -> bool {
        match resource {
            ResourceRef::Surface => true,
            ResourceRef::Texture(handle) => self.texture_handle_is_valid(handle),
            ResourceRef::TextureSubresource(subresource) => {
                self.texture_subresource_is_valid(subresource)
            }
            ResourceRef::Buffer(handle) => self.buffer_handle_is_valid(handle),
        }
    }

    /// Returns `true` if this resource has data that originates from outside the
    /// current frame's graph execution. Used during dependency analysis to
    /// suppress `ReadBeforeWrite` errors for imported resources, persistent
    /// graph-owned resources, and the presentation surface.
    fn resource_has_external_source(&self, resource: ResourceRef) -> bool {
        match resource {
            ResourceRef::Surface => true,
            ResourceRef::Texture(handle) => self
                .textures
                .get(handle.0)
                .is_some_and(|desc| desc.imported.is_some() || !desc.transient),
            ResourceRef::TextureSubresource(subresource) => self
                .textures
                .get(subresource.texture.0)
                .is_some_and(|desc| desc.imported.is_some() || !desc.transient),
            ResourceRef::Buffer(handle) => self
                .buffers
                .get(handle.0)
                .is_some_and(|desc| desc.imported.is_some() || !desc.transient),
        }
    }

    /// Returns `true` if writes to this resource are visible outside the graph.
    /// Used during dead-pass culling: any pass that writes to an external sink
    /// is considered alive.
    ///
    /// NOTE: The implementation intentionally mirrors `resource_has_external_source`.
    /// Imported resources and persistent graph-owned resources are both external
    /// sources/sinks from the compiler's point of view: callers can observe
    /// their contents across frame boundaries.
    fn resource_has_external_sink(&self, resource: ResourceRef) -> bool {
        match resource {
            ResourceRef::Surface => true,
            ResourceRef::Texture(handle) => self
                .textures
                .get(handle.0)
                .is_some_and(|desc| desc.imported.is_some() || !desc.transient),
            ResourceRef::TextureSubresource(subresource) => self
                .textures
                .get(subresource.texture.0)
                .is_some_and(|desc| desc.imported.is_some() || !desc.transient),
            ResourceRef::Buffer(handle) => self
                .buffers
                .get(handle.0)
                .is_some_and(|desc| desc.imported.is_some() || !desc.transient),
        }
    }

    fn resource_is_live(&self, resource: ResourceRef) -> bool {
        self.lifetimes
            .keys()
            .copied()
            .any(|live| resource_refs_overlap(live, resource))
    }

    fn validate_pass_resource(
        &self,
        pass: &str,
        resource: ResourceRef,
    ) -> Result<(), RenderGraphError> {
        if self.resource_handle_is_valid(resource) {
            return Ok(());
        }
        Err(RenderGraphError::InvalidResourceHandle {
            pass: Some(Cow::Owned(pass.to_owned())),
            resource,
        })
    }

    pub fn new() -> Self {
        Self {
            handle_token: next_handle_token(),
            textures: Vec::new(),
            buffers: Vec::new(),
            texture_names: FxHashMap::default(),
            buffer_names: FxHashMap::default(),
            passes: Vec::new(),
            order: Vec::new(),
            cached_compiled: Vec::new(),
            compiled: false,
            max_dep_level: 0,
            culled_count: 0,
            dep_edges: Vec::new(),
            dep_reverse_edges: Vec::new(),
            lifetimes: FxHashMap::default(),
            alias_groups: Vec::new(),
            alias_stats: None,
            alias_redirects: FxHashMap::default(),
            physical_textures: Vec::new(),
            physical_buffers: Vec::new(),
            persistent_texture_cache: FxHashMap::default(),
            persistent_buffer_cache: FxHashMap::default(),
            transient_pool: TransientPool::new(),
            transient_buffer_pool: TransientBufferPool::new(),
            blackboard: Blackboard::new(),
            view_stats: PhysicalResourceViewStatsCounters::default(),
        }
    }

    fn stash_persistent_texture(&mut self, name: &str, target: RenderTarget) {
        self.persistent_texture_cache
            .entry(name.to_string())
            .or_default()
            .push(target);
    }

    fn take_persistent_texture(&mut self, name: &str) -> Option<RenderTarget> {
        let mut remove_entry = false;
        let target = self
            .persistent_texture_cache
            .get_mut(name)
            .and_then(|targets| {
                let target = targets.pop();
                remove_entry = targets.is_empty();
                target
            });
        if remove_entry {
            self.persistent_texture_cache.remove(name);
        }
        target
    }

    fn stash_persistent_buffer(&mut self, name: &str, buffer: wgpu::Buffer) {
        self.persistent_buffer_cache
            .entry(name.to_string())
            .or_default()
            .push(buffer);
    }

    fn take_persistent_buffer(&mut self, name: &str) -> Option<wgpu::Buffer> {
        let mut remove_entry = false;
        let buffer = self
            .persistent_buffer_cache
            .get_mut(name)
            .and_then(|buffers| {
                let buffer = buffers.pop();
                remove_entry = buffers.is_empty();
                buffer
            });
        if remove_entry {
            self.persistent_buffer_cache.remove(name);
        }
        buffer
    }

    // ── Resource creation ───────────────────────────────────────────────

    /// Create a virtual texture resource using a builder closure.
    pub fn create_texture(&mut self, build: impl FnOnce(&mut TextureBuilder)) -> TextureHandle {
        let mut builder = TextureBuilder::new();
        build(&mut builder);
        let handle = TextureHandle(self.textures.len(), self.handle_token);
        let name_str = builder.name.to_string();
        if let Some(old) = self.texture_names.insert(name_str.clone(), handle) {
            eprintln!(
                "[SkyEngine] RenderGraph: duplicate texture name {:?} \
                 (old handle {:?} shadowed by {:?})",
                name_str, old, handle
            );
        }
        self.textures.push(TextureDesc {
            name: builder.name,
            size: builder.size,
            format: builder.format,
            usage: builder.usage,
            sample_count: builder.sample_count,
            mip_level_count: builder.mip_level_count,
            array_layer_count: builder.array_layer_count,
            transient: builder.transient,
            imported: builder.imported,
        });
        self.compiled = false;
        handle
    }

    /// Create a virtual buffer resource using a builder closure.
    pub fn create_buffer(&mut self, build: impl FnOnce(&mut BufferBuilder)) -> BufferHandle {
        let mut builder = BufferBuilder::new();
        build(&mut builder);
        let handle = BufferHandle(self.buffers.len(), self.handle_token);
        let name_str = builder.name.to_string();
        if let Some(old) = self.buffer_names.insert(name_str.clone(), handle) {
            eprintln!(
                "[SkyEngine] RenderGraph: duplicate buffer name {:?} \
                 (old handle {:?} shadowed by {:?})",
                name_str, old, handle
            );
        }
        self.buffers.push(BufferDesc {
            name: builder.name,
            size_bytes: builder.size_bytes,
            usage: builder.usage,
            transient: builder.transient,
            imported: builder.imported,
        });
        self.compiled = false;
        handle
    }

    /// Look up a texture handle by its name.
    #[must_use]
    pub fn get_texture(&self, name: &str) -> Option<TextureHandle> {
        self.texture_names.get(name).copied()
    }

    /// Look up a buffer handle by its name.
    #[must_use]
    pub fn get_buffer(&self, name: &str) -> Option<BufferHandle> {
        self.buffer_names.get(name).copied()
    }

    /// Access the blackboard for storing shared data.
    pub fn blackboard(&mut self) -> &mut Blackboard {
        &mut self.blackboard
    }

    /// Read-only access to the blackboard.
    pub fn blackboard_ref(&self) -> &Blackboard {
        &self.blackboard
    }

    // ── Pass registration ───────────────────────────────────────────────

    /// Add a render (rasterisation) pass to the graph.
    pub fn add_render_pass(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        setup_fn: impl FnOnce(&mut PassSetup),
    ) -> PassHandle {
        self.add_pass_inner(name.into(), PassType::Render, setup_fn)
    }

    /// Add a compute pass to the graph.
    pub fn add_compute_pass(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        setup_fn: impl FnOnce(&mut PassSetup),
    ) -> PassHandle {
        self.add_pass_inner(name.into(), PassType::Compute, setup_fn)
    }

    /// Add a copy pass with explicit copy operations.
    pub fn add_copy_pass(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        setup_fn: impl FnOnce(&mut CopyPassSetup),
    ) -> PassHandle {
        let mut setup = CopyPassSetup::new();
        setup_fn(&mut setup);
        let handle = PassHandle(self.passes.len(), self.handle_token);
        self.passes.push(PassEntry {
            name: name.into(),
            pass_type: PassType::Copy,
            reads: setup.reads,
            writes: setup.writes,
            color_outputs: Vec::new(),
            depth_stencil: None,
            copy_ops: setup.ops,
            flags: setup.flags,
            dep_level: 0,
            alive: true,
        });
        self.compiled = false;
        handle
    }

    fn add_pass_inner(
        &mut self,
        name: Cow<'static, str>,
        pass_type: PassType,
        setup_fn: impl FnOnce(&mut PassSetup),
    ) -> PassHandle {
        let mut setup = PassSetup::new();
        setup_fn(&mut setup);
        let handle = PassHandle(self.passes.len(), self.handle_token);
        self.passes.push(PassEntry {
            name,
            pass_type,
            reads: setup.reads,
            writes: setup.writes,
            color_outputs: setup.color_outputs,
            depth_stencil: setup.depth_stencil,
            copy_ops: Vec::new(),
            flags: setup.flags,
            dep_level: 0,
            alive: true,
        });
        self.compiled = false;
        handle
    }

    // ── Compilation pipeline ────────────────────────────────────────────
    // See compile.rs

    // ── Physical resource management ────────────────────────────────────
    // See allocate.rs

    // ── Query ───────────────────────────────────────────────────────────

    /// Number of passes (including culled).
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    /// Number of alive passes after compilation.
    pub fn alive_pass_count(&self) -> usize {
        self.order.len()
    }

    /// Number of culled passes.
    pub fn culled_count(&self) -> usize {
        self.culled_count
    }

    /// Maximum dependency level (graph depth).
    pub fn max_dep_level(&self) -> u32 {
        self.max_dep_level
    }

    /// Memory aliasing statistics from the last compilation.
    pub fn alias_stats(&self) -> Option<&alias::AliasingStats> {
        self.alias_stats.as_ref()
    }

    /// Number of alias groups where actual sharing occurs (>1 member).
    pub fn alias_group_count(&self) -> usize {
        self.alias_groups
            .iter()
            .filter(|g| g.members.len() > 1)
            .count()
    }

    /// View-resolution counters collected during the most recent graph
    /// execution.
    ///
    /// These are diagnostics for cache audits. They do not change scheduling,
    /// resource allocation, or view creation behavior.
    #[must_use]
    pub fn physical_resource_view_stats(&self) -> PhysicalResourceViewStats {
        self.view_stats.snapshot()
    }

    /// Return pass handles for all passes declared at or after `start`.
    pub(crate) fn pass_handles_from(&self, start: usize) -> Vec<PassHandle> {
        (start..self.passes.len())
            .map(|index| PassHandle(index, self.handle_token))
            .collect()
    }

    /// Clear per-frame declaration state while keeping reusable pools alive.
    ///
    /// Unlike [`reset`], this does not destroy the transient pools themselves.
    /// Any currently allocated transient resources are first returned to those
    /// pools so the next frame can reuse them.
    pub(crate) fn clear_frame(&mut self) {
        for tex_idx in 0..self.textures.len() {
            let (is_transient, is_imported, name) = {
                let desc = &self.textures[tex_idx];
                (
                    desc.transient,
                    desc.imported.is_some(),
                    desc.name.to_string(),
                )
            };
            if is_transient {
                if self.alias_redirects.contains_key(&tex_idx) {
                    if let Some(slot) = self.physical_textures.get_mut(tex_idx) {
                        slot.take();
                    }
                    continue;
                }
                if let Some(target) = self
                    .physical_textures
                    .get_mut(tex_idx)
                    .and_then(Option::take)
                {
                    self.transient_pool.release(
                        PoolKey {
                            format: target.format(),
                            usage: target.usage(),
                            width: target.width(),
                            height: target.height(),
                            sample_count: target.sample_count(),
                            mip_level_count: target.mip_level_count(),
                            array_layer_count: target.array_layer_count(),
                        },
                        target,
                    );
                }
            } else if !is_imported {
                if let Some(target) = self
                    .physical_textures
                    .get_mut(tex_idx)
                    .and_then(Option::take)
                {
                    self.stash_persistent_texture(&name, target);
                }
            } else if let Some(slot) = self.physical_textures.get_mut(tex_idx) {
                slot.take();
            }
        }

        for buf_idx in 0..self.buffers.len() {
            let (is_transient, is_imported, name) = {
                let desc = &self.buffers[buf_idx];
                (
                    desc.transient,
                    desc.imported.is_some(),
                    desc.name.to_string(),
                )
            };
            if is_transient {
                if let Some(buffer) = self
                    .physical_buffers
                    .get_mut(buf_idx)
                    .and_then(Option::take)
                {
                    self.transient_buffer_pool.release(
                        BufferPoolKey {
                            size_bytes: buffer.size(),
                            usage: buffer.usage(),
                        },
                        buffer,
                    );
                }
            } else if !is_imported {
                if let Some(buffer) = self
                    .physical_buffers
                    .get_mut(buf_idx)
                    .and_then(Option::take)
                {
                    self.stash_persistent_buffer(&name, buffer);
                }
            } else if let Some(slot) = self.physical_buffers.get_mut(buf_idx) {
                slot.take();
            }
        }

        self.textures.clear();
        self.buffers.clear();
        self.texture_names.clear();
        self.buffer_names.clear();
        self.passes.clear();
        self.order.clear();
        self.cached_compiled.clear();
        self.dep_edges.clear();
        self.dep_reverse_edges.clear();
        self.lifetimes.clear();
        self.alias_groups.clear();
        self.alias_stats = None;
        self.alias_redirects.clear();
        self.physical_textures.clear();
        self.physical_buffers.clear();
        self.handle_token = next_handle_token();
        self.blackboard.clear();
        self.max_dep_level = 0;
        self.culled_count = 0;
        self.compiled = false;
    }

    /// Destroy all currently owned physical resources.
    pub fn destroy_physical_resources(&mut self) {
        for target in &mut self.physical_textures {
            target.take();
        }
        for buffer in &mut self.physical_buffers {
            buffer.take();
        }
        self.persistent_texture_cache.clear();
        self.persistent_buffer_cache.clear();
        self.transient_pool.destroy_all();
        self.transient_buffer_pool.destroy_all();
    }

    /// Clear all passes and resources, keeping no declaration state.
    pub fn reset(&mut self) {
        debug_assert!(
            self.physical_textures.iter().all(|t| t.is_none()),
            "RenderGraph::reset() called with live physical textures — \
             call destroy_physical_resources() first"
        );
        debug_assert!(
            self.physical_buffers.iter().all(|b| b.is_none()),
            "RenderGraph::reset() called with live physical buffers — \
             call destroy_physical_resources() first"
        );
        debug_assert!(
            self.persistent_texture_cache
                .values()
                .all(|targets| targets.is_empty()),
            "RenderGraph::reset() called with cached persistent textures — \
             call destroy_physical_resources() first"
        );
        debug_assert!(
            self.persistent_buffer_cache
                .values()
                .all(|buffers| buffers.is_empty()),
            "RenderGraph::reset() called with cached persistent buffers — \
             call destroy_physical_resources() first"
        );
        self.textures.clear();
        self.buffers.clear();
        self.texture_names.clear();
        self.buffer_names.clear();
        self.passes.clear();
        self.order.clear();
        self.cached_compiled.clear();
        self.dep_edges.clear();
        self.dep_reverse_edges.clear();
        self.lifetimes.clear();
        self.alias_groups.clear();
        self.alias_stats = None;
        self.alias_redirects.clear();
        self.physical_textures.clear();
        self.physical_buffers.clear();
        self.persistent_texture_cache.clear();
        self.persistent_buffer_cache.clear();
        self.transient_pool = TransientPool::new();
        self.transient_buffer_pool = TransientBufferPool::new();
        self.handle_token = next_handle_token();
        self.blackboard.clear();
        self.view_stats.reset();
        self.max_dep_level = 0;
        self.culled_count = 0;
        self.compiled = false;
    }

    // ── Execution ───────────────────────────────────────────────────────
    // See execute.rs

    // ── Visualization ───────────────────────────────────────────────────
    // See visualize.rs
}
