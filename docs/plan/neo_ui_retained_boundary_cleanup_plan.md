# Neo UI Retained Boundary Cleanup Plan

## Status

Implementation in progress.

Completed:

- Retained dirty normalization removes descendant dirty roots when an ancestor
  already covers them.
- Runtime debug snapshots now distinguish raw dirty scopes from normalized
  dirty layout roots.
- `Ui::clock()` / `UiClock` are available and backed by the existing live
  invalidation bridge.
- Stress Lab's floating `Secret` probe was migrated to `ui.clock()` and renamed
  to `Live Probe`.
- Debug snapshots now include retained scope records, element ancestry records,
  dirty reasons, layout anchors, scroll/clip ancestry, and target vs draw
  frames.
- `UiActionTrace` prints normalized dirty roots alongside raw dirty roots.

Still open:

- Automatic retained boundary ownership from stable element/widget ids.
- Public API cleanup for `scope` / `live_scope` once automatic ownership is
  proven in real examples.
- Control Center and Gallery migration away from hand-authored cache scopes.
- Real-window WGPU probes for filtered rect dumps and screenshot comparison.

This plan replaces the previous reactive signal/scope plan, which presented
`Ui::scope(...)` as the normal app-facing dependency boundary. The retained
boundary concept is still useful internally, but it should not be the primary
mental model exposed to UI authors.

## Problem Statement

The current EUI-NEO Rust port has three related problems:

1. App code must manually choose retained boundaries with `ui.scope(...)`.
2. App code must manually choose frame-time invalidation with
   `ui.live_scope(...)`.
3. Partial layout, retained subtree reuse, animation targets, draw-list caching,
   and WGPU upload caching are now powerful enough that bugs can become
   invisible until a very specific interaction happens.

Recent concrete failures:

- Signal -> Chart tab underline snapped instead of animating because an
  ancestor height-only change was treated like an ancestor position move.
- Stress Lab live/procedural content can appear to float when nested dirty
  boundaries are handled in the wrong order or with stale absolute frames.
- Earlier draw-cache reuse made the UI look frozen when two draw lists had the
  same pointer/length shape but different contents.

The pattern is not one bad line of code. The pattern is that internal retained
UI rules are scattered, underspecified, and exposed directly to app code.

## Retained Model Choice

Neo UI is not currently a fully retained object tree like raw DOM, Qt Widgets,
WPF, or Unity UI. It is a declarative UI builder with retained runtime state.

Current retained pieces:

- interaction state
- focus/hover/active state
- animation state and animation targets
- previous frames
- draw-list and renderer caches
- retained subtrees for partial compose

Current immediate/declarative pieces:

- app code still runs builder closures to describe UI
- elements are recreated as data during compose
- callbacks are refreshed during compose or transferred with retained subtrees

This hybrid model is intentional for now. Moving to a fully retained object API
would require persistent node handles, explicit child insertion/removal/reorder
APIs, mutation-style property setters, callback lifetime ownership, deletion
semantics, and a much heavier Rust borrowing/handle story. That is a different
UI engine direction.

The chosen direction for this plan:

```text
Full compose + full layout is the correctness baseline.
Partial compose/layout is an optimization.
Retained boundaries are internal optimization metadata.
Public code should not manually manage dirty boundaries.
If partial reasoning cannot prove safety, fall back to the baseline.
```

## Design Principle

Public UI code should describe UI and data dependencies:

```rust
ui.stack("interactions.secret")
    .size(124.0, Size::fill())
    .content(|ui| {
        let time = ui.clock().seconds();
        secret_card_contents(ui, time);
    });
```

It should not describe cache policy:

```rust
ui.live_scope("interactions.secret.live", |ui| {
    secret_card(ui, "interactions.secret", state, time);
});
```

The engine may still use retained boundaries internally:

- retained subtree root
- dirty boundary
- layout boundary
- dependency target
- cache invalidation key

But the public model should be:

- element
- widget
- state/signal
- clock/timer
- animation
- effect

## Mature UI Reference Points

The exact API should be SkyEngine-native, but the direction matches mature UI
systems:

- Browser/Web Animations: element-attached animation driven by a timing model,
  not manual DOM rebuilds each frame.
  <https://developer.mozilla.org/en-US/docs/Web/API/Web_Animations_API>
- Qt: property animations are driven by the animation framework and global time
  driver, not by app code manually rebuilding child widgets every frame.
  <https://doc.qt.io/qt-6/animation-overview.html>
- Flutter: `AnimationController` is connected to frame ticks through a ticker.
  <https://api.flutter.dev/flutter/animation/AnimationController-class.html>
- Jetpack Compose: value animations and infinite transitions are explicit
  animation/time dependencies, while recomposition is driven by observed state.
  <https://developer.android.com/develop/ui/compose/animation/value-based>

The shared lesson: invalidation boundaries exist, but normal app code usually
talks in terms of state, elements, animation, and time.

## Target Model

### App-Facing Concepts

| Concept | Public role |
| --- | --- |
| Element id | Stable identity, layout node, hit-test target, animation target |
| Signal / State | User or app data dependency |
| Clock / Timer | Time dependency |
| Transition / Motion | Runtime-driven property animation |
| Effect | Optional explicit side effect tied to an element or state key |

### Internal Concepts

| Concept | Internal role |
| --- | --- |
| Retained boundary | Unit of subtree reuse/rebuild |
| Dirty reason | Why a boundary cannot be reused |
| Layout anchor | Previous/current frame used for partial layout |
| Clip/scroll ancestry | Runtime relationship used for layout, hit test, and draw |
| Cache cell | Reusable cached artifact with a structured key |

## Invalidation Rules

The final engine rules should be:

1. Reading a `Signal` inside an element subtree registers that subtree as a
   dependent.
2. Mutating a `Signal` dirties the smallest safe retained boundary that owns the
   read.
3. Reading `ui.clock()` registers a time dependency for the current retained
   boundary.
4. A time-dependent boundary is dirtied on the requested cadence.
5. Runtime property animation does not require compose; it only requires
   animation ticking, draw-list invalidation, and render.
6. Input does not dirty UI by itself. Callback-driven state changes dirty UI.
7. Dirty descendants are removed when a dirty ancestor already covers them.
8. Partial layout must process dirty roots from outermost to innermost, and must
   never relayout a nested dirty child from stale absolute coordinates after its
   parent has already been rebuilt.

## Algorithmic Model

Treat the current UI as a rooted tree plus a dependency graph.

```text
root
  interactions
    interactions.scroll
      interactions.scroll.viewport
        interactions.scroll.content
          interactions.cards
            interactions.secret
```

Each current-frame element gets a compact `NodeId` and tree metadata:

```rust
type NodeId = u32;
type DepId = u32;

struct NodeMeta {
    id_hash: u64,
    parent: Option<NodeId>,
    depth: u32,
    tin: u32,
    tout: u32,
    frame: LayoutRect,
    scroll_ancestor: Option<NodeId>,
    clip_ancestor: Option<NodeId>,
    retained_owner: Option<NodeId>,
}
```

`tin` / `tout` come from a DFS Euler tour. Ancestor checks are O(1):

```text
is_ancestor(a, b) = tin[a] <= tin[b] && tout[b] <= tout[a]
```

Dependencies are stored against stable element ids, not long-lived `NodeId`s,
because the tree can be rebuilt:

```rust
struct DependencyGraph {
    dep_to_elements: FxHashMap<DepId, SmallVec<[ElementId; 4]>>,
    element_to_deps: FxHashMap<ElementId, SmallVec<[DepId; 4]>>,
}
```

Per compose/frame, stable ids are resolved to current nodes:

```rust
id_to_node: FxHashMap<ElementId, NodeId>
nodes: Vec<NodeMeta>
```

Dirty marking:

```text
changed dep -> dep_to_elements[dep] -> raw dirty element ids
raw dirty ids -> current NodeId through id_to_node
```

Use a bitset plus a vector for dedupe:

```rust
dirty_bits: FixedBitSet
dirty_nodes: Vec<NodeId>
```

Dirty normalization creates an antichain: no retained dirty root is an ancestor
of another retained dirty root.

```text
raw dirty:
  interactions
  interactions.secret

normalized dirty roots:
  interactions
```

Basic normalization algorithm:

```text
sort dirty_nodes by tin
for node in sorted dirty_nodes:
    if the last kept dirty root is an ancestor of node:
        skip node
    else:
        keep node
```

This is O(k log k), where k is the number of raw dirty nodes. The ancestor
check is O(1). If k becomes very large, an Euler-interval union or DSU-style
"next unvisited" range-skip optimization can be added later, but it is not the
primary design.

Do not use DSU/union-find as the main dirty model. DSU is good for merging
sets, cache groups, or range-skip helpers, but it does not answer the core UI
queries well:

- is A an ancestor of B?
- which dirty descendants are covered by A?
- what is the nearest scroll ancestor?
- what is the nearest clip ancestor?
- what layout frame anchors this subtree?

Euler tree metadata answers those directly.

## Correctness Argument

1. Full compose + full layout is always correct because it ignores all retained
   subtree reuse.
2. Reusing a clean retained subtree is correct only if none of its recorded
   dependencies changed and its structural/layout parent context remains valid.
3. If an ancestor dirty root is rebuilt, all descendant dirty nodes are covered
   by that rebuild. Removing descendant dirty nodes from the normalized dirty
   root set does not lose updates.
4. Partial compose with full layout is the first safe optimization target. It
   can reuse clean subtrees while recomputing all final frames.
5. Partial layout is a later optimization and must only run when its parent
   layout context is proven stable. Otherwise it must fall back to full layout.
6. Runtime animation is separate from compose dirty. Animation ticking changes
   draw frames and render dirtiness; it should not force app-level recomposition.

This means the engine should optimize only along proven-safe paths. The fallback
is not a failure mode; it is the correctness baseline.

## Phase 1: Correctness Before API Redesign

### Goals

- Fix the current nested dirty layout hazard.
- Keep public APIs mostly unchanged.
- Add regression tests before broad refactors.

### Work

1. Add retained dirty normalization:

   ```text
   input dirty set:
     page.panel
     page.panel.child.live

   normalized dirty roots:
     page.panel
   ```

2. Sort dirty roots deterministically by depth and lexical id.

3. Use normalized dirty roots for:

   - partial layout
   - structural compatibility checks
   - debug snapshots

4. Preserve the original raw dirty list for diagnostics.

5. Add tests:

   - parent and child dirty together does not double-layout child
   - live child inside scroll content moves with scroll offset
   - live child inside clipped viewport is clipped by viewport
   - Signal -> Chart underline still animates
   - Chart -> Motion underline still animates

### Acceptance Criteria

- `Secret` content cannot visually detach from `interactions.scroll.content`.
- A dirty ancestor and dirty descendant produce one layout root in normalized
  layout work.
- Existing `eui-neo` tests pass.
- Stress Lab tab and segment tests pass.

## Phase 2: Observability

### Goals

Make retained UI bugs explainable without screenshots and guesses.

### Work

1. Extend `UiDebugSnapshot` with per-boundary diagnostics:

   ```rust
   pub struct ScopeDebugRecord {
       pub id: String,
       pub parent_scope: Option<String>,
       pub dirty: bool,
       pub raw_dirty: bool,
       pub normalized_dirty_root: bool,
       pub dirty_reasons: Vec<DirtyReason>,
       pub action: ScopeComposeAction,
       pub previous_roots: usize,
       pub current_roots: usize,
       pub layout_anchor: Option<LayoutRect>,
       pub scroll_ancestor: Option<String>,
       pub clip_ancestor: Option<String>,
   }
   ```

2. Add element ancestry diagnostics:

   ```rust
   pub struct ElementDebugRecord {
       pub id: String,
       pub parent: Option<String>,
       pub retained_boundary: Option<String>,
       pub scroll_ancestor: Option<String>,
       pub clip_ancestor: Option<String>,
       pub target_frame: LayoutRect,
       pub draw_frame: Option<LayoutRect>,
   }
   ```

3. Add filtered dumps:

   ```text
   SKY_NEO_DEBUG_ELEMENT=interactions.secret
   SKY_NEO_DEBUG_SCOPE=interactions
   SKY_NEO_DEBUG_DIRTY=1
   ```

4. Extend `UiActionTrace` with:

   - dirty reasons after input
   - normalized dirty roots after compose
   - changed target frames
   - changed draw frames after each animation tick

### Acceptance Criteria

- A failed test can print exactly why a boundary was rebuilt or reused.
- A floating element report shows its parent, scroll ancestor, clip ancestor,
  target frame, and draw frame.
- Debug output distinguishes raw dirty scopes from normalized layout roots.

## Phase 3: Public API Direction

### Goals

Move normal app code away from public retained-boundary management.

### New API Shape

Preferred:

```rust
ui.stack("interactions.secret")
    .size(124.0, Size::fill())
    .content(|ui| {
        let seconds = ui.clock().seconds();
        ui.text("title")
            .text(format!("Live {}", ((seconds * 2.0).sin() > 0.0) as i32))
            .build();
    });
```

Optional cadence API:

```rust
let pulse = ui.clock().every(Duration::from_millis(100));
```

Explicit advanced boundary API should move under an expert namespace or be
renamed to make the internal nature clear:

```rust
ui.expert().retained_boundary("debug.boundary", |ui| {
    ...
});
```

### Public API Changes

Add:

- `Ui::clock() -> UiClock`
- `UiClock::seconds() -> f32`
- `UiClock::frame_index() -> u64`
- `UiClock::every(Duration) -> ClockTick`
- internal dependency registration when clock values are read

Deprecate for normal code:

- `Ui::live_scope(...)`
- `Ui::scope(...)` as the required dependency boundary

Keep temporarily:

- `Ui::scope(...)` for migration and tests
- `Ui::live_scope(...)` as an internal bridge until clock dependencies land

Remove from demos after migration:

- direct `live_scope` usage
- direct scope wrapping whose only purpose is cache control

## Phase 4: Automatic Boundary Ownership

### Goal

Make the engine infer retained boundaries from stable element ids.

### Work

1. Treat stable element ids as possible retained boundaries.
2. Record signal/clock dependencies against the nearest stable element boundary.
3. Allow widgets to declare internal boundaries for expensive subtrees.
4. Keep explicit expert boundaries for special cases.
5. Add heuristics conservatively:

   - a widget root is a boundary
   - a scroll content root is a boundary
   - a popover root is a boundary
   - a virtual list viewport/content pair is a boundary

### Non-Goal

Do not attempt a compiler-like optimizer in this phase. Correctness and
diagnostics matter more than maximum reuse.

## Phase 5: Demo Migration

### Stress Lab

Replace:

```rust
ui.live_scope("interactions.secret.live", |ui| {
    secret_card(ui, "interactions.secret", state_store, time);
});
```

With:

```rust
secret_card(ui, "interactions.secret", state_store);
```

and inside the card:

```rust
let time = ui.clock().seconds();
```

Also rename the visible label:

```text
Secret 0 / Secret 1
```

to:

```text
Live Probe 0 / Live Probe 1
```

The old name is too mysterious for a diagnostic demo.

### Control Center

Do not require hand-authored scopes for nav/page selection. Bind selection to
signals/widgets and let the runtime decide invalidation.

### Gallery

Add one section that intentionally demonstrates:

- signal-driven update
- clock-driven update
- runtime transition update
- scroll clipping with live content

## Phase 6: Regression Test Matrix

### Unit Tests

Run:

```text
cargo test --manifest-path crates/eui-neo/Cargo.toml
```

Required cases:

- nested dirty normalization
- scroll + live child
- clip + live child
- clock read dirties only owner boundary
- transition animation does not require compose
- dirty reason reporting

### Renderer Tests

Run:

```text
cargo test --manifest-path crates/eui-neo-wgpu/Cargo.toml
```

Required cases:

- draw-list revision changes invalidate collect cache
- vertex upload cache keys change on draw revision
- filtered WGPU dump can locate a target rect

### Stress Lab Tests

Run:

```text
cargo test --features ui-neo --example ui_neo_stress_lab -- --nocapture
```

Required cases:

- Signal -> Chart underline animates
- Chart -> Motion underline animates
- segment indicator animates
- live probe moves with scroll content
- chart bars continue animating after tab switch

### Real Window Probe

Run a real app probe with:

```text
SKY_NEO_LAB_AUTO_CLICK_CHART_FRAME=20
SKY_NEO_WGPU_DUMP_FILTER=signals.tabs.indicator
SKY_NEO_SCREENSHOT_PATH=target/neo_probe_1.png
SKY_NEO_SCREENSHOT_FRAME=23
SKY_NEO_SCREENSHOT_PATH_2=target/neo_probe_2.png
SKY_NEO_SCREENSHOT_FRAME_2=35
SKY_NEO_EXIT_AFTER_SCREENSHOT=1
cargo run --features ui-neo --example ui_neo_stress_lab
```

The WGPU rect dump must show gradual movement instead of snapping.

## Risk Assessment

### Low Risk

- Dirty normalization before partial layout.
- More debug fields.
- More tests.

### Medium Risk

- `ui.clock()` dependency registration.
- Migrating demos from manual live scopes.
- Distinguishing compose invalidation from render-only animation ticking.

### High Risk

- Removing public scope immediately.
- Inferring all retained boundaries automatically without enough diagnostics.
- Optimizing partial layout before correctness is proven.

## Recommended Execution Order

1. Implement dirty normalization and nested dirty tests.
2. Add scroll/live regression test for the floating `Secret` class of bugs.
3. Extend debug snapshots with raw dirty vs normalized dirty roots.
4. Add `Ui::clock()` as a narrow API, initially backed by the same internal live
   invalidation machinery.
5. Migrate Stress Lab live content from `live_scope` to `ui.clock()`.
6. Hide `live_scope` from examples.
7. Move explicit retained-boundary APIs toward `expert`.
8. Revisit automatic boundary inference only after diagnostics are good.

## Stop Conditions

Do not proceed to API removal if any of these are true:

- Stress Lab still has floating live content.
- Debug output cannot explain why a boundary rebuilt.
- Real WGPU probes disagree with runtime/draw-list traces.
- A signal change dirties the entire UI without an explicit fallback reason.

## Final Target

UI authors write this:

```rust
ui.row("toolbar").content(|ui| {
    widgets::tabs(ui, "page.tabs")
        .signal(page_signal)
        .items(["Overview", "Charts", "Motion"])
        .build();

    ui.text("clock")
        .text(format!("{:.1}", ui.clock().seconds()))
        .build();
});
```

The engine internally decides:

- what depends on `page_signal`
- what depends on `clock`
- what can be reused
- what needs compose
- what only needs animation tick/render
- what must be clipped or scrolled

Public code describes UI. The retained engine explains and optimizes it.
