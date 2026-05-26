# SakuraEngine Render Graph Adaptation Plan

Status: draft evidence-based plan

Scope: compare the local SakuraEngine render graph reference with SkyEngine's
current render graph and frame execution model, then define a staged plan for
borrowing useful ideas without copying backend-specific C++/CGPU architecture.

This document is intentionally factual.  Statements under "Current Facts" are
based on files that exist in this repository or in the local SakuraEngine
reference snapshot.  Statements under "Plan" or "Recommendation" are proposals
derived from those facts.

## 1. Executive Summary

SkyEngine has already borrowed meaningful ideas from SakuraEngine's render
graph, especially:

- declarative virtual resources and pass dependencies;
- Sakura-inspired execution reordering;
- Sakura-inspired transient texture aliasing;
- a Sakura-inspired typed blackboard;
- GraphViz DOT export;
- scheduling hint flags that are currently informational.

The highest-value next steps are not to port SakuraEngine wholesale.  The
highest-value next steps are:

1. Make SkyEngine's existing render graph more observable.
2. Split compile-time analysis into inspectable internal result structs.
3. Add a dry-run queue scheduling diagnostic before implementing real
   multi-queue execution.
4. Extend resource/view/bind-group caching only where SkyEngine has measured
   pressure.
5. Preserve SkyEngine's current `FramePipeline` / `PreparedFrame` /
   `PreparedView` composition boundary.

The main reason is architectural mismatch.  SakuraEngine's render graph is a
C++17 CGPU-backed system that owns command pools, fences, descriptor/bind-table
pools, explicit barriers, multi-queue scheduling, and per-frame node
allocation.  SkyEngine's current backend is `wgpu`, and `wgpu` intentionally
owns resource state transitions that SakuraEngine handles explicitly.

## 2. Source Evidence

### 2.1 SkyEngine sources used

- `AGENTS.md`
- `src/render/AGENTS.md`
- `src/render/graph/AGENTS.md`
- `src/render/graph/mod.rs`
- `src/render/graph/compile.rs`
- `src/render/graph/reorder.rs`
- `src/render/graph/alias.rs`
- `src/render/graph/allocate.rs`
- `src/render/graph/execute.rs`
- `src/render/graph/types.rs`
- `src/render/graph/builder.rs`
- `src/render/graph/pool.rs`
- `src/render/graph/visualize.rs`
- `src/render/resources/blackboard.rs`
- `src/render/execution/frame_pipeline.rs`
- `src/render/runtime/pipeline_runtime.rs`
- `docs/architecture/render_deep_dive.md`
- `docs/plan/render_god_object_refactor_plan.md`
- `docs/plan/sakura_resource_system_adaptation_plan.md`

### 2.2 SakuraEngine local reference sources used

- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/claude.md`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/src/backend/graph_backend.cpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/src/backend/object_pools.cpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/src/backend/render_graphviz.cpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/src/frontend/graph_builders.cpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/src/frontend/graph_frontend.cpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/include/SkrRenderGraph/frontend/base_types.hpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/include/SkrRenderGraph/frontend/render_graph.hpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/include/SkrRenderGraph/frontend/blackboard.hpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/include/SkrRenderGraph/phases_v2/queue_schedule.hpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/include/SkrRenderGraph/phases_v2/schedule_reorder.hpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/include/SkrRenderGraph/phases_v2/memory_aliasing_phase.hpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/include/SkrRenderGraph/phases_v2/barrier_generation_phase.hpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/include/SkrRenderGraph/phases_v2/bind_table_phase.hpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/src/phases_v2/schedule_reorder.cpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/src/phases_v2/memory_aliasing_phase.cpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/src/phases_v2/cross_queue_sync_analysis.cpp`
- `_refs/SakuraEngine_ref/engine/modules/render/render_graph/src/phases_v2/barrier_generation_phase.cpp`

### 2.3 Evidence caveats

- The SakuraEngine local reference contains both summary documentation
  (`claude.md`) and C++ source.  When the summary and source differ, the source
  should be treated as the stronger fact for implementation comparison.
- One observed difference: `claude.md` describes the phase chain as memory
  aliasing -> resource allocation -> barrier generation, but
  `src/backend/graph_backend.cpp` constructs and executes
  `BarrierGenerationPhase` before `ResourceAllocationPhase`.
- `graph_backend.cpp` constructs `MemoryAliasingPhase` with
  `MemoryAliasingConfig{ .aliasing_tier = EAliasingTier::Tier0 }`, even though
  the headers and documentation describe support for multiple aliasing tiers.

## 3. Current SkyEngine Facts

### 3.1 Render module boundary

- `src/render/AGENTS.md` says SkyEngine's default native renderer is the `wgpu`
  path: `RenderPipelineAsset` / `RenderPipelineBuilder` -> `RenderRuntime`,
  installed through app-facing `SceneRenderer`.
- The `wgpu` runtime uses `RenderFeature`, `Extractor`, `DrawFunction`,
  `PipelineStep::{Phase, Compute, Graph, Pass, PostFx}`, and `GpuScene` /
  `GpuTableManager`.
- `PreparedFrame` / `PreparedView` + `FramePipeline` are the composition
  boundary for heterogeneous `wgpu` renderer families.
- Optional Kajiya and Renderling backends consume backend-neutral
  `SceneSnapshot`; they are not the same execution path as `RenderRuntime`.
- `GpuScene` is documented as shared scene upload state for the high-level
  `wgpu` runtime, not a universal home for every renderer-specific cache.

### 3.2 RenderGraph role

- `src/render/graph/AGENTS.md` defines `RenderGraph` as a declarative
  render-graph system for GPU workload orchestration on top of `wgpu`.
- Passes declare virtual resource reads/writes through builder closures.
- The graph resolves dependencies, culls dead passes, tracks lifetimes, pools
  transient GPU resources, and executes passes in topological order.
- The module is explicitly inspired by SakuraEngine's `SkrRenderGraph`.
- The current implementation is single-queue.
- Physical resources are `wgpu::Texture`, `wgpu::Buffer`, and SkyEngine
  `RenderTarget`.

### 3.3 Current RenderGraph compile pipeline

`src/render/graph/AGENTS.md` describes the compile path as:

1. Dependency analysis.
2. Dead-pass culling.
3. SakuraEngine-inspired execution reordering.
4. Resource lifetime analysis.
5. Deferred memory alias analysis in `allocate_physical_resources()`.

`src/render/graph/compile.rs` implements these facts:

- Dependency analysis scans passes in declaration order.
- Read edges are added from overlapping last writers.
- Write edges are added from overlapping last writers and prior readers.
- `ReadBeforeWrite` is emitted for non-external, non-persistent resources read
  before a write.
- Kahn's algorithm is used with an index-sorted ready queue.
- Cycles produce `RenderGraphError::CycleDetected`.
- Dead pass culling starts from resources with external sinks and walks
  dependencies backwards.
- Reordering calls `reorder::reorder_for_affinity(...)`.
- Lifetime analysis records `first_use` / `last_use` per resource after
  reordering.
- Compilation is cached and idempotent until graph resources or passes are
  changed.

### 3.4 Current handle model

`src/render/graph/AGENTS.md` and `types.rs` establish:

- Texture, buffer, and pass handles are `(usize, u64)` style handles.
- The `usize` indexes graph arrays.
- The `u64` is a per-graph `handle_token`.
- `reset()` changes the handle token so stale handles are invalidated.
- Handle validity is checked before use; stale and foreign handles produce
  `InvalidResourceHandle` errors.
- `ResourceRef` currently includes `Surface`, `Texture`, `TextureSubresource`,
  and `Buffer`.

### 3.5 Current pass and resource model

`src/render/graph/types.rs` establishes:

- Pass types are `Render`, `Compute`, and `Copy`.
- `PassFlags` currently include:
  - `PREFER_ASYNC_COMPUTE`;
  - `COMPUTE_INTENSIVE`;
  - `VERTEX_BOUND_INTENSIVE`;
  - `PIXEL_BOUND_INTENSIVE`;
  - `BANDWIDTH_INTENSIVE`.
- `CopyOp` includes:
  - `TextureToTexture`;
  - `BufferToBuffer`;
  - `BufferToTexture`;
  - `UploadToTexture`.
- Render passes support MRT color outputs and depth/stencil declarations.
- Texture descriptors track size, format, usage, sample count, mip count,
  array layer count, transient/persistent status, and optional import.
- Buffer descriptors track size, usage, transient/persistent status, and
  optional import.
- `CompiledPass` is returned to the caller and contains pass type, reads,
  writes, attachment declarations, copy ops, flags, and dependency level.
- The graph does not store pass execution closures; callers execute compiled
  passes through the `try_execute` closure.

### 3.6 Current execution model

`src/render/graph/execute.rs` establishes:

- `try_execute()` compiles if needed, validates copy passes, allocates physical
  resources, iterates compiled passes, executes copy passes internally, invokes
  the caller closure for render/compute passes, and releases transients.
- `try_execute_profiled()` adds `RenderGraphProfiler` callbacks.
- Copy passes flush an active frame encoder before running copy commands.
- `UploadToTexture` uses `queue.write_texture()` and can force a submit
  boundary inside copy execution.
- There is a diagnostic environment variable
  `SKY_RENDER_GRAPH_TRACE_ALIAS` for aliasing trace output.
- The graph flushes the active frame encoder before aliased physical texture
  ownership changes.
- `execute_copy_pass()` intentionally takes `&self`, not `&mut self`, because
  `PhysicalResources` holds shared borrows during execution.

### 3.7 Current physical resource management

`src/render/graph/allocate.rs` and `pool.rs` establish:

- Texture usage and buffer usage can be augmented after compile based on copy
  operations.
- Memory alias analysis is deferred to allocation because surface-relative
  target sizes require the real surface size.
- Alias groups are computed before texture allocation.
- Each alias group with multiple members gets one shared `RenderTarget`.
- Secondary alias members redirect to the primary texture index.
- Non-aliased transient textures are acquired from `TransientPool`.
- Non-transient graph-owned textures are persistent and can be resized.
- Persistent texture and buffer caches exist in `RenderGraph`.
- Transient buffers use `TransientBufferPool`.
- Imported textures and buffers are never pooled or resized.

### 3.8 Current Sakura-derived pieces in SkyEngine

Current explicit Sakura references:

- `src/render/graph/mod.rs`: module docs say the graph is inspired by
  SakuraEngine's RenderGraph.
- `src/render/graph/AGENTS.md`: says the graph is inspired by
  SakuraEngine's `SkrRenderGraph`.
- `src/render/graph/reorder.rs`: says it is adapted from SakuraEngine's
  `ExecutionReorderPhase` (`schedule_reorder.cpp`).
- `src/render/graph/alias.rs`: says it is adapted from SakuraEngine's
  `MemoryAliasingPhase` (`memory_aliasing_phase.cpp`).
- `src/render/resources/blackboard.rs`: says the typed key-value blackboard is
  directly inspired by SakuraEngine's RenderGraph blackboard system.

### 3.9 Current visualization and diagnostics

- `src/render/graph/visualize.rs` exports GraphViz DOT.
- Current DOT output includes resource nodes, pass nodes, read/write edges,
  pass type colors, and dashed culled passes.
- `export_dot()` warns if called before compile because alive/culled state may
  be inaccurate.
- `RenderGraphProfiler` exists in `error.rs`.
- `DebugProfiler` exists.
- Alias stats are available through `alias_stats()`.
- `alias_group_count()` only counts groups where actual sharing occurs.

### 3.10 Current limitations documented by SkyEngine

`src/render/graph/AGENTS.md` lists these current limitations:

- Single queue only.
- No multi-queue scheduling.
- No async compute dispatch.
- `PassFlags::PREFER_ASYNC_COMPUTE` is recorded but unused.
- No barrier generation, by design, because `wgpu` handles resource state
  transitions internally.
- No bind-table management; descriptor set / bind group creation is left to the
  caller.
- Texture-only aliasing; buffer aliasing is not implemented.
- Physical graph textures are still backed by `RenderTarget`, so render graph
  managed physical textures assume 2D attachments even though sample/mip/array
  metadata is tracked.
- Remaining limitations are tracked against the SakuraEngine reference
  implementation's 12-phase pipeline.

### 3.11 Current FramePipeline relationship

`src/render/execution/frame_pipeline.rs` establishes:

- `FramePipeline` owns a `RenderGraph`.
- `execute_frame_with_services()` prepares the frame graph, compiles it, then
  executes it through `graph.try_execute(...)`.
- `FramePipeline` maps `PassHandle` values to dispatch entries for setup,
  view, and finalize nodes.
- Setup, view, and finalize nodes declare graph passes during setup.
- The closure passed to `RenderGraph` dispatches a compiled pass back to the
  owning `FramePipeline` node.

`src/render/runtime/pipeline_runtime.rs` establishes:

- `build_runtime_pipeline()` converts declaration-time `PipelineStep`s into
  `FramePipeline` nodes.
- `PipelineStep::Phase`, `Compute`, `Graph`, `Pass`, and `PostFx` map to
  specific node types.
- `SceneColorSeedNode`, `HeadlessKeepAliveNode`, and `ViewportBlitNode` are
  inserted around user pipeline steps.

## 4. Current SakuraEngine Facts

### 4.1 Overall architecture

The local SakuraEngine render graph reference describes `SkrRenderGraph` as:

- C++17;
- namespace `skr::RG`;
- a declarative builder-pattern frontend;
- a modular multi-phase compilation pipeline;
- a backend that owns GPU resource management and execution;
- a CGPU-backed system with command pools, command buffers, fences, resource
  pools, bind table pools, and explicit execution phases.

### 4.2 Documented Sakura phase pipeline

`_refs/.../render_graph/claude.md` describes 12 phases:

1. `CullPhase`
2. `PassInfoAnalysis`
3. `PassDependencyAnalysis`
4. `QueueSchedule`
5. `ExecutionReorderPhase`
6. `CrossQueueSyncAnalysis`
7. `ResourceLifetimeAnalysis`
8. `MemoryAliasingPhase`
9. `ResourceAllocationPhase`
10. `BarrierGenerationPhase`
11. `BindTablePhase`
12. `PassExecutionPhase`

### 4.3 Source-observed Sakura phase execution

`src/backend/graph_backend.cpp` constructs these phases in this order:

1. `CullPhase`
2. `PassInfoAnalysis`
3. `PassDependencyAnalysis`
4. `QueueSchedule`
5. `ExecutionReorderPhase`
6. `ResourceLifetimeAnalysis`
7. `CrossQueueSyncAnalysis`
8. `MemoryAliasingPhase`
9. `BarrierGenerationPhase`
10. `ResourceAllocationPhase`
11. `BindTablePhase`
12. `PassExecutionPhase`

This order differs from the phase order text in `claude.md` for
`CrossQueueSyncAnalysis` vs `ResourceLifetimeAnalysis`, and for
`BarrierGenerationPhase` vs `ResourceAllocationPhase`.

### 4.4 Sakura frontend facts

The local reference includes:

- nested graph/pass/resource builders;
- render, compute, copy, and present pass builders;
- texture, buffer, and acceleration-structure builders;
- pass nodes and resource nodes;
- typed resource edges;
- a blackboard;
- object handles with sub-handle types for SRV/RTV/DSV/UAV/ranges;
- node and edge factory allocation.

### 4.5 Sakura backend facts

The local reference includes:

- `RenderGraphBackend`, derived from `RenderGraph`;
- `RenderGraphFrameExecutor`;
- frame executors indexed by `RG_MAX_FRAME_IN_FLIGHT`;
- command pool and command buffer ownership;
- fence ownership;
- marker buffer for command trace diagnostics;
- texture pool;
- texture view pool;
- buffer pool;
- buffer view pool;
- bind table pool;
- merged bind table pool;
- garbage collection by critical frame.

### 4.6 Sakura queue scheduling facts

`queue_schedule.hpp` defines:

- queue types: `Graphics`, `AsyncCompute`, `Copy`;
- queue capability structs;
- `QueueScheduleConfig` with async compute and copy queue enable flags;
- maximum async compute queue and copy queue counts;
- `TimelineScheduleResult` containing all queues, per-queue schedules, and
  pass-to-queue assignments;
- pass classification and least-loaded compute queue selection helpers.

### 4.7 Sakura synchronization and barrier facts

The local Sakura reference includes:

- `CrossQueueSyncAnalysis`, described as using SSIS to minimize cross-queue
  synchronization points.
- `BarrierGenerationPhase`, generating barriers for state transitions,
  cross-queue sync, memory aliasing, and execution dependencies.
- `PassExecutionPhase`, which consumes queue schedule, reorder, sync, barrier,
  allocation, and bind-table phase outputs.

These are backend-level concerns in SakuraEngine because CGPU exposes explicit
queues, barriers, command buffers, and descriptor/bind-table management.

### 4.8 Sakura memory aliasing facts

The local reference includes:

- `MemoryAliasingPhase`;
- `MemoryBucket`;
- `MemoryAliasTransition`;
- config with aliasing tiers;
- compression statistics;
- resource-to-bucket mapping;
- alias barrier identification.

`graph_backend.cpp` currently constructs the phase with `EAliasingTier::Tier0`.

### 4.9 Sakura visualization facts

The local reference includes `src/backend/render_graphviz.cpp`, described by
`claude.md` as a detailed GraphViz generator.  The source references phase
outputs such as queue schedule, barrier, aliasing, and lifetime results.

## 5. Comparison Matrix

| Dimension | SkyEngine current fact | SakuraEngine reference fact | Adaptation judgment |
| --- | --- | --- | --- |
| Backend API | `wgpu` plus Sky `GpuContext` / `RenderTarget` | CGPU, explicit queues, command pools, fences, barriers | Do not port low-level backend ownership directly |
| High-level frame composition | `PreparedFrame` / `PreparedView` + `FramePipeline` | RenderGraph backend directly executes graph phases | Preserve Sky composition boundary |
| Pass declaration | Builder closures declare reads/writes; no stored execution closures in graph | Builder lambdas declare passes and execution callbacks | Keep Sky closure-at-execution model |
| Compile phases | Dependency, culling, reorder, lifetime; alias deferred to allocation | 12 named phases | Split Sky internal result structs gradually; do not force 12 public phases |
| Reordering | Already adapted from Sakura | `ExecutionReorderPhase` | Keep and improve diagnostics/tests |
| Memory aliasing | Already adapted for transient textures using `RenderTarget` sharing | `MemoryAliasingPhase` with buckets and transitions | Keep wgpu simplification; do not port heap sub-offset aliasing |
| Queue scheduling | Single queue; async flags unused | Graphics/async compute/copy queues and pass assignments | Add dry-run diagnostics before real multi-queue |
| Cross-queue sync | Not implemented | SSIS analysis | Defer until real multi-queue exists |
| Barrier generation | Not implemented by design; wgpu owns resource transitions | Explicit barrier generation | Do not implement real barriers under wgpu; keep any barrier-like reasoning diagnostic-only |
| Bind tables | Not part of graph; caller handles bind groups | Bind table phase and pools | Consider bind group/view caches outside core graph |
| Resource pools | Transient texture and buffer pools; persistent caches | Texture/buffer/view/bind-table pools | Add only measured caches; do not copy all pools |
| Visualization | Basic DOT export | Detailed DOT from phase outputs | High-value, low-risk adaptation |
| Diagnostics | Profiler hooks, alias stats, trace env var | Phase outputs, marker buffer, GraphViz | Expand Sky debug dumps and pass/resource reports |
| Handles | `(index, token)` stale-handle protection | typed C++ object handles and sub-handles | Keep Sky handle model; selectively improve resource access descriptors |
| Acceleration structures | Not in current Sky RenderGraph `ResourceRef` | Sakura has acceleration structure resources | Do not add until Sky has a concrete ray tracing path |

## 6. Design Constraints for SkyEngine

These constraints are derived from existing SkyEngine docs and code:

1. `RenderGraph` remains a low-level declarative pass/resource backend.
2. `FramePipeline` remains the primary frame execution engine for the `wgpu`
   runtime.
3. `PreparedFrame` / `PreparedView` remain the cross-feature composition
   boundary.
4. `GpuScene` remains shared GPU table/upload state, not a universal cache.
5. Kajiya and Renderling continue to consume `SceneSnapshot`, not the
   `RenderRuntime` `FramePipeline` path.
6. `wgpu` resource state transitions should not be replaced with an explicit
   Sakura-style barrier system.
7. Render graph handle validation must continue using `handle_token`.
8. `compile()` remains the single source of truth for execution order and alive
   state.
9. `compile()` remains idempotent and cached.
10. Copy pass setup must continue to declare reads/writes so dependency
    analysis sees copy operations.
11. Heavy per-frame allocations should not be added to the compilation path.

## 7. What SkyEngine Should Not Copy

### 7.1 Do not copy CGPU barrier generation into wgpu execution

SakuraEngine needs explicit barrier generation because its backend exposes
explicit GPU state transitions and synchronization.  SkyEngine's graph runs on
`wgpu`, and `src/render/graph/AGENTS.md` explicitly states that there is no
barrier generation by design because `wgpu` handles resource state transitions
internally.

Recommendation: if barrier-like reasoning is useful, implement it only as a
debug report that explains inferred hazards and queue constraints.  Do not emit
or simulate real GPU barriers in the core `wgpu` execution path.

### 7.2 Do not replace FramePipeline with a Sakura-style backend executor

Sakura's backend owns graph execution directly.  Sky's `FramePipeline` maps
graph passes back to setup/view/finalize nodes and runtime pipeline steps.  This
is already integrated with `PreparedFrame`, `PreparedView`, phases, post-fx,
viewport blit, and runtime services.

Recommendation: improve `FramePipeline` and graph inspection together.  Do not
make `RenderGraph` the owner of all renderer family execution.

### 7.3 Do not copy C++ node factories or stack allocator first

Sakura's `NodeAndEdgeFactory` and stack allocator match its C++ object graph.
Sky's graph is Rust vectors, maps, handles, and owned descriptors.

Recommendation: only add arena/smallvec-style allocation after a benchmark
shows render graph compile allocation is a measurable cost.

### 7.4 Do not add acceleration structures without a concrete rendering path

Sakura's graph has acceleration structure resources.  Sky's current graph
resources are surface, texture, texture subresource, and buffer.  Adding
acceleration structures without a ray tracing path would expand API surface
without an engine use case.

### 7.5 Do not generalize GpuScene into a universal render graph state bag

Sky docs repeatedly define `GpuScene` as shared view/model/light table state.
Renderer-family caches should remain local or enter through typed
frame/view payloads.

## 8. Plan

### Milestone 1: Evidence-Preserving RenderGraph Debug Dump

Goal: make the current graph state observable before changing algorithms.

Add an expert/debug-only report type, for example:

```rust
pub struct RenderGraphDebugDump {
    pub passes: Vec<RenderGraphPassDebug>,
    pub resources: Vec<RenderGraphResourceDebug>,
    pub lifetimes: Vec<RenderGraphLifetimeDebug>,
    pub aliasing: Option<AliasingStats>,
    pub alias_groups: Vec<RenderGraphAliasGroupDebug>,
    pub culled_count: usize,
    pub max_dep_level: u32,
}
```

Required facts to expose:

- declaration order;
- compiled execution order;
- pass type;
- pass flags;
- dependency level;
- alive/culled state;
- reads and writes;
- color/depth outputs;
- copy operations;
- resource names and descriptors;
- resource lifetime first/last use;
- alias groups and alias redirects;
- imported/persistent/transient classification;
- surface/imported/persistent sink/source classification.

Implementation notes:

- Keep this as read-only API.
- It can be gated as expert/debug if public API surface is a concern.
- Do not change scheduling, allocation, or execution behavior.
- Reuse existing `CompiledPass`, `ResourceLifetime`, `AliasingStats`, and graph
  descriptor data.
- Avoid forcing `compile()` to allocate heavy debug data every frame; generate
  the report only on request.

Acceptance criteria:

- A test can build a graph, call `compile()`, request the debug dump, and assert
  pass order, culled state, and lifetimes.
- A graph with aliasable transient textures reports alias groups after
  `allocate_physical_resources()`.
- A graph with dead passes reports which passes are culled.

Validation:

- `cargo test --features app graph`

### Milestone 2: Enhanced GraphViz Output

Goal: borrow Sakura's visualization depth without adopting Sakura's backend.

Current Sky DOT output includes resources, passes, read/write edges, pass type
colors, and culled styling.  Extend it optionally with debug-dump information:

- compiled execution index;
- dependency level;
- pass flags;
- culled/alive status;
- resource lifetime span;
- transient/persistent/imported classification;
- alias group membership;
- copy op labels;
- color/depth output labels;
- surface/imported sink/source markers.

Implementation notes:

- Keep existing `export_dot()` stable if needed.
- Add `export_dot_with_options(...)` or a new expert method rather than making
  all DOT output noisy by default.
- DOT generation should use already-computed report data where possible.

Acceptance criteria:

- Existing DOT tests remain valid.
- New tests cover subresource labels, culled pass styling, and alias group
  labeling.
- Calling DOT before `compile()` still clearly reports that alive/culled state
  may be inaccurate.

Validation:

- `cargo test --features app graph`

### Milestone 3: Internal Analysis Result Structs

Goal: make Sky's compile path easier to inspect and evolve while preserving
public behavior.

Current `compile.rs` computes dependency edges, topological order, dead-pass
state, reordering, and lifetimes in one method.  Split internal data into
private or crate-private result structs:

```rust
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
```

This borrows Sakura's phase-output clarity, not its exact phase API.

Rules:

- Preserve `RenderGraph::compile()` as the single public compile entry point.
- Preserve cached/idempotent compile behavior.
- Preserve deterministic ready-queue ordering.
- Preserve current error behavior.
- Preserve all handle validation through `handle_token`.
- Keep allocations modest; do not store duplicate large data unless the debug
  dump requests it.

Acceptance criteria:

- Existing render graph tests pass without behavioral changes.
- New unit tests can target analysis helpers if they are exposed under
  `#[cfg(test)]`.
- No public API break is introduced unless explicitly approved later.

Validation:

- `cargo test --features app graph`
- If public render/app APIs change unexpectedly: `cargo check --examples --features app`

### Milestone 4: PassInfoAnalysis-Inspired Access Summary

Goal: create a single internal summary of pass/resource access facts.

Sakura has `PassInfoAnalysis` that extracts pass resource accesses and
performance info.  Sky can add a smaller `PassAccessInfo` layer:

```rust
struct PassAccessInfo {
    pass_index: usize,
    pass_type: PassType,
    flags: PassFlags,
    reads: Vec<ResourceRef>,
    writes: Vec<ResourceRef>,
    color_outputs: Vec<ColorOutput>,
    depth_stencil: Option<DepthStencilOutput>,
    copy_ops: Vec<CopyOp>,
}
```

Potential uses:

- debug dump;
- DOT output;
- copy validation context;
- dependency analysis input;
- future queue-scheduling dry run;
- lifetime reporting;
- tests that assert a pass's declared GPU contract.

Rules:

- This should not become a second source of truth that can diverge from
  `PassEntry`.
- It can be generated from `PassEntry` on demand or during compile.
- It should not add renderer-family-specific state.

Acceptance criteria:

- Dependency analysis and debug dump use the same access interpretation.
- Copy passes expose their inferred reads/writes in the debug dump.
- Read/write/readwrite dedup behavior remains covered by existing tests.

Validation:

- `cargo test --features app graph`

### Milestone 5: Queue Scheduling Dry Run

Goal: use Sakura's queue scheduling model as a diagnostic before implementing
real multi-queue execution.

Current fact: Sky is single-queue, and `PREFER_ASYNC_COMPUTE` is recorded but
unused.  Add a dry-run analysis that does not alter execution order:

```rust
pub struct QueueScheduleDiagnostic {
    pub pass_assignments: Vec<QueueAssignmentDiagnostic>,
    pub async_compute_candidates: usize,
    pub copy_queue_candidates: usize,
    pub blockers: Vec<QueueScheduleBlocker>,
}
```

Suggested queue classes for diagnostics:

- `Graphics`: render passes and any pass that writes the surface;
- `ComputeCandidate`: compute passes with `PREFER_ASYNC_COMPUTE`;
- `CopyCandidate`: copy passes;
- `GraphicsRequired`: passes blocked from async/copy by current resource or
  graph limitations.

This is not real scheduling.  It is an explanation layer that tells us what
would be eligible if a future multi-queue backend existed.

Facts that should be reported:

- pass type;
- pass flags;
- dependency level;
- resources shared with graphics passes;
- imported/persistent/surface interactions;
- reason a pass is not eligible.

Non-goals:

- no cross-queue synchronization;
- no async command encoders;
- no actual queue submission;
- no change to `try_execute()`;
- no behavior change in frame rendering.

Acceptance criteria:

- A graph with compute pass + async flag reports it as an async candidate.
- A pass that writes the surface reports graphics requirement.
- Existing execution order is unchanged.

Validation:

- `cargo test --features app graph`

### Milestone 6: Resource and View Cache Audit

Goal: decide, with measurements, whether Sakura-style view/bind-table pooling
maps to Sky's `wgpu` renderer.

Current fact:

- Sky has transient texture/buffer pools and persistent caches.
- Sakura has texture pool, buffer pool, texture view pool, buffer view pool,
  bind table pool, and merged bind table pool.
- Sky `PhysicalResources` can create or resolve views for physical textures.
- Bind group creation is currently outside `RenderGraph`.

Audit tasks:

1. Identify where Sky creates repeated texture views per frame.
2. Identify where material/pass bind groups are recreated.
3. Measure whether this is visible in frame time or allocation profiles.
4. Decide whether caching belongs in:
   - `RenderGraph` physical resources;
   - `RenderTarget`;
   - material/resource cache;
   - pass-specific runtime state;
   - `GpuScene` table manager;
   - another renderer-family-local cache.

Recommendation:

- Do not add bind table management to core `RenderGraph`.
- If view caching is useful, prefer a small `wgpu::TextureView` cache keyed by
  texture identity + view descriptor in the owner that already manages the
  texture.
- If bind group caching is useful, keep it near material/pass/resource caches,
  not in graph scheduling.

Acceptance criteria:

- Audit document or code comments identify measured hot spots.
- Any cache addition has tests for invalidation on resize/format/usage changes.
- No graph-level bind-table abstraction is added without a concrete caller.

Validation:

- `cargo test --features app graph`
- Relevant render/runtime tests for the owner touched.

### Milestone 7: GPU Error Trace Labels and Pass Markers

Goal: borrow Sakura's "command trace after GPU failure" idea in a `wgpu`-native
way.

Current Sakura fact:

- `RenderGraphFrameExecutor` has a marker buffer and marker messages used to
  report failed commands after device loss.

Sky-compatible proposal:

- Ensure render graph command encoders, render/compute passes, copy passes, and
  key resources have labels that include graph pass names.
- Add optional debug scopes around pass execution where `wgpu` supports them.
- Integrate `RenderGraphProfiler` names with runtime timing stats where
  appropriate.
- Add a debug dump path that records the last compiled pass order when graph
  execution fails.

Non-goals:

- no CGPU marker buffer port;
- no persistent mapped GPU marker buffer unless a `wgpu`-compatible need is
  proven;
- no dependency on Sakura's executor model.

Acceptance criteria:

- A forced execution error reports the pass name and graph state needed to
  locate the failing pass.
- Copy pass validation errors include source/destination resource context.
- Existing `RenderGraphError` display output remains useful.

Validation:

- `cargo test --features app graph`
- Targeted tests around execution error propagation.

### Milestone 8: Optional Real Multi-Queue Design Gate

Goal: explicitly defer real multi-queue execution until the diagnostic and
workload evidence justifies it.

Prerequisites:

- Queue scheduling dry-run exists.
- At least one real Sky workload shows enough compute/copy work to benefit.
- `wgpu` backend constraints for queue submission and synchronization are
  documented.
- Frame encoder lifecycle with `GpuContext::begin_frame()` / `end_frame()` /
  `flush()` is reviewed.
- Surface presentation order and copy pass submit boundaries are reviewed.

Only after those facts exist should Sky consider:

- async compute queue submission;
- cross-queue dependency representation;
- queue-specific pass batches;
- synchronization diagnostics;
- per-queue profiling.

This milestone is intentionally a gate, not an implementation commitment.

## 9. Suggested File-Level Work Breakdown

### 9.1 Low-risk docs and diagnostics

- `src/render/graph/types.rs`: add debug structs if they belong near graph
  types.
- `src/render/graph/visualize.rs`: add richer DOT output.
- `src/render/graph/mod.rs`: expose read-only debug accessors.
- `src/render/graph/compile.rs`: preserve compile behavior while returning or
  storing analysis result snapshots.
- `src/render/graph/tests/`: add debug and visualization regression tests.

### 9.2 Medium-risk compile refactor

- `src/render/graph/compile.rs`: extract dependency/culling/lifetime helper
  functions or submodules.
- `src/render/graph/reorder.rs`: keep algorithm behavior stable; only improve
  diagnostic outputs unless tests and benchmark justify algorithm changes.
- `src/render/graph/alias.rs`: keep wgpu `RenderTarget` aliasing model.
- `src/render/graph/allocate.rs`: keep alias computation deferred until real
  surface dimensions are known.

### 9.3 Runtime integration checks

- `src/render/execution/frame_pipeline.rs`: add debug report hooks only if they
  do not disturb pass dispatch.
- `src/render/runtime/pipeline_runtime.rs`: no expected changes for the first
  milestones.
- `src/render/runtime/`: only touch runtime stats if profiler/report data needs
  app-facing exposure.

## 10. Tests and Verification

Run after documentation-only changes:

- No code tests required, but markdown paths should be checked manually.

Run after render graph code changes:

- `cargo test --features app graph`

Run after public render/app API changes:

- `cargo check --examples --features app`

Run after runtime-level render changes:

- `cargo test --features app render::runtime::tests`

Run after graph changes that touch GPU allocation/execution:

- `cargo test --features app graph`
- Any affected tests that use `create_test_device()` require a GPU-capable
  environment.

## 11. Risk Register

| Risk | Why it matters | Mitigation |
| --- | --- | --- |
| Accidentally changing compile order | RenderGraph compile is the source of truth for execution order | Keep current tests; add order snapshots for new debug APIs |
| Making debug reports too expensive | Graph may compile once and execute many frames | Generate reports on demand; avoid heavy default allocations |
| Treating wgpu like Vulkan/DX12 | Explicit Sakura barriers do not map directly | Keep barrier reasoning diagnostic-only |
| Over-centralizing renderer state in RenderGraph | Sky's composition boundary is `PreparedFrame` / `PreparedView` / `FramePipeline` | Keep renderer-family data in typed payloads or local caches |
| Overusing `GpuScene` | Docs say `GpuScene` is shared table/upload state, not a universal state bag | Add only cross-feature tables there |
| Copying Sakura C++ ownership patterns | Raw handles, node factories, and per-frame allocators do not map directly to Rust ownership | Keep Rust handles and ownership; optimize only after profiling |
| Adding real async compute too early | Multi-queue sync can be complex and backend-dependent | Build dry-run diagnostics first |
| View/bind-group cache invalidation bugs | Resizes and format/usage changes can invalidate GPU objects | Tie caches to owners and add invalidation tests |

## 12. Fact-Based Open Questions

These questions are deliberately left open because the current evidence does
not answer them:

1. Which current Sky render workloads recreate enough views or bind groups for
   caching to matter?
2. Which passes, if any, are compute-heavy enough to justify real async compute?
3. Does `wgpu` backend behavior on the target platforms expose enough queue
   control for Sakura-style scheduling to be useful?
4. Should debug dumps be public expert API, test-only API, or diagnostics-only
   runtime output?
5. Should DOT output include all debug data by default, or should it remain
   compact with opt-in detail?
6. Can alias stats and graph pass stats be folded into existing render runtime
   timing stats without coupling graph internals to app diagnostics?

## 13. Recommended Sequence

1. Add `RenderGraphDebugDump`.
2. Extend DOT output using the debug dump.
3. Extract internal analysis result structs from `compile.rs`.
4. Add `PassAccessInfo` or equivalent access summary.
5. Add queue scheduling dry-run diagnostics.
6. Audit view and bind-group creation before adding caches.
7. Improve pass labels and failure reports.
8. Revisit real multi-queue execution only after diagnostics show a concrete
   workload benefit.

## 14. Summary

SkyEngine's render graph already follows the part of SakuraEngine that fits:
declarative passes, virtual resources, dependency-driven execution, reordering,
aliasing, blackboard-style sharing, and visualization.  The remaining Sakura
ideas with the best fit are observability and phase-output clarity.

The Sakura ideas with the weakest fit are explicit barrier generation,
descriptor/bind-table ownership inside the graph, C++ node allocation patterns,
and immediate multi-queue execution.  Those are tied to Sakura's CGPU backend
and should not be ported into SkyEngine's `wgpu` path without measured need and
a backend-specific design review.

The practical route is therefore incremental: make the current graph easy to
inspect, preserve current behavior, add diagnostic scheduling, and only then
consider deeper backend work.
