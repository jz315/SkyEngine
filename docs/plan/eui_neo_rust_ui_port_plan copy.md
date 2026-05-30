# EUI-NEO Rust Retained Runtime 重构标准与实施计划

## 0. 结论

EUI-NEO 的 public API 可以继续保持 immediate-style declaration，但 runtime 内部必须改为 **retained/reconciled kernel**。

最终形态是：

```text
用户 DSL / widgets
    -> ViewTree：本帧声明事实
    -> Reconciler：决定 retained 复用、重建、卸载
    -> RetainedTree：运行时提交状态
    -> Layout / Layer / Compose / Hit / Draw
    -> Renderer / Platform Effects
```

本计划的核心判断如下：

1. 当前问题不是单纯 layout 问题，而是 retained runtime 边界问题。
2. Taffy 只能解决部分布局计算，不能解决 dirty ownership、callback stale、event ordering、focus、IME、layer lifetime。
3. 第一阶段必须优先保证正确性和可观测性，不追求 partial compose 性能。
4. runtime 必须先建立 pass coordinator、typed invalidation、role-specific identity、reconciler、layer manager、input ownership。
5. 任何 retained reuse 都必须由 reconciler 证明安全；不能由 DSL 或 widget builder 自己复用旧节点、旧 callback、旧 layer。
6. dropdown、popover、context menu、dialog、tooltip、toast 必须走 LayerIntent，不得继续使用 widget-local root popup hack。
7. event dispatch 必须只针对上一帧 committed hit tree；callback 执行之后再进入 invalidation 和 rewrite pass。
8. Runtime 可以是 public facade，但不得成为 god object。

---

## 1. 适用范围

本计划适用于：

* `crates/eui-neo`
* `crates/eui-neo-wgpu`
* SkyEngine 的 `src/ui/neo/` adapter
* Stress Lab / gallery / headless runtime tests
* dropdown、popover、text input、scroll、focus、dialog、toast 等 retained UI 行为

本计划不包含：

* 一次性替换成第三方 UI 框架
* 立即引入 Taffy 作为主 layout engine
* 立即重写所有 widget 外观
* 通过 backend hack 修复 runtime 行为
* 为旧 dirty-id、旧 callback transfer、旧 popup root hack 保留兼容模式

---

## 2. 规范用语

本文使用以下规范词：

* **必须**：架构正确性要求，不满足则不得进入下一阶段。
* **不得**：禁止项，即使短期测试通过也视为架构违规。
* **应当**：默认要求，偏离时必须写明理由。
* **可以**：允许但不强制。
* **实现定义**：实现可以选择具体策略，但必须能通过 trace/debug 输出解释。

---

## 3. 当前问题定义

### 3.1 现象

当前 Stress Lab dropdown、Drift、popover、text input 等问题表现为：

* field scope dirty 了，但 root popup 没有 dirty。
* scope 被复用后保留了旧 callback 或旧 signal dependency。
* event dispatch 过程中直接触发 state change，dirty 收集和 tree mutation 顺序不稳定。
* popover 使用上一帧 anchor，但又不属于父 layout tree。
* window input event 与 per-frame input snapshot 可能不一致。
* Runtime-level tests 能通过，但真实 backend event ordering 会暴露 bug。

### 3.2 根因

根因不是某一个 widget 写错，而是 runtime 同时混合了以下职责：

```text
declaration
retained reuse
callback transfer
dirty routing
layout
hit testing
focus/capture
layer lifetime
platform input
draw list generation
```

因此局部修复会继续制造 temporal bug。正确方向是建立 kernel boundary，使每类 bug 能定位到一个明确 subsystem。

---

## 4. 总体设计决议

### D1. Declaration 只产生事实，不做 retained 决策

DSL 和 widget declaration 可以创建：

* node facts
* style facts
* layout hints
* signal watches
* callback facts
* layer intents
* debug labels

DSL 和 widget declaration 不得：

* 读取 previous retained tree 来决定复用
* 转移 previous callbacks
* 推断 dirty id 的含义
* 决定 partial layout
* 决定 layer z-order
* 决定 event dispatch order

### D2. Runtime Kernel 负责 retained 决策

Runtime kernel 必须拥有：

* identity assignment
* invalidation collection / merge
* reconciliation
* frame pass scheduling
* focus / capture ownership
* layer state
* layout / compose state
* hit tree generation
* draw list generation
* platform effects queue

### D3. Runtime 内部必须拆成三棵树

```text
ViewTree
    本帧 DSL 产生的临时声明树。

RetainedTree
    runtime 提交后的稳定树，保存 identity、callbacks、layout cache、
    interaction state、focus、layers、animation state。

DrawList
    layout / layer / compose 之后交给 renderer 的绘制命令。
```

ViewTree 不得保存 previous-frame retained state。

DrawList 不得修改 application state，也不得注册 signal dependencies。

### D4. id 必须 role-specific

以下 id 必须是不同类型：

```rust
WidgetId
ScopeId
NodeId
ElementId
SignalId
CallbackId
LayerId
ResourceId
```

这些 id 不得在 subsystem API 中以 raw string 传递。

string 只能作为 debug label，不能作为 reuse、dirty routing、callback lookup、hit testing、layer ownership 的判断依据。

### D5. dirty 必须变成 typed invalidation

旧模型：

```text
dirty_ids: Set<String>
```

目标模型：

```rust
struct Invalidation {
    target: InvalidationTarget,
    source: InvalidationSource,
    flags: DirtyFlags,
    propagation: InvalidationPropagation,
}
```

每条 invalidation 必须回答：

1. 什么对象 dirty 了？
2. 为什么 dirty？
3. 需要哪些 pass？
4. dirty 如何传播？

### D6. Reconciler 是唯一 retained reuse 决策者

只有 reconciler 可以决定：

* node 是否复用
* scope 是否复用
* callback 是否转移
* layout cache 是否保留
* animation state 是否保留
* focus 是否保留或清除
* layer intent 是否继续有效

如果不能证明复用安全，必须 full rebuild 对应 subtree。

### D7. Event dispatch 不得直接修改 retained tree

event pipeline 必须是：

```text
previous committed HitTree
    -> target selection
    -> collect UiEventCommand
    -> drop tree borrows
    -> run callbacks
    -> state/signal mutations
    -> typed invalidations
    -> rewrite passes
```

event dispatch 期间不得直接 mutate RetainedTree。

### D8. Layer 是 first-class runtime state

popover、dropdown、context menu、dialog、tooltip、toast 必须通过 `LayerIntent` 声明。

widget 不得直接把 popup root 当普通 child hack 到 root layer。

### D9. Input ownership 必须显式建模

runtime 必须区分：

```rust
pointer_hover
pointer_active
pointer_capture
keyboard_focus
text_focus
ime_owner
scroll_owner
drag_owner
```

不能用一个 `focused_id` 同时代表 keyboard、text、IME、capture、scroll。

### D10. Taffy 只能在 layout boundary 稳定之后引入

Taffy 不得作为 dirty、event、focus、callback、layer 的解决方案。

Taffy 可以在后期作为 `LayoutBackend` 替代手写 layout，但前提是：

* LayoutInput 稳定
* LayoutOutput 稳定
* ComposePass 独立存在
* HitTree 使用 committed geometry
* Layer anchor 使用明确 frame source

---

## 5. 目标模块结构

### 5.1 `runtime/frame.rs`

是什么：

* frame coordinator
* pass order scheduler
* rewrite loop owner

为什么：

* 当前 frame path 里混合 input、callback、dirty、compose、layout、render。
* 需要一个小 coordinator 控制顺序，但不能变成 god class。

怎么做：

```rust
pub struct FrameCoordinator {
    flags: PassFlags,
    rewrite_limit: usize,
}

impl FrameCoordinator {
    pub fn run_frame(&mut self, kernel: &mut RuntimeKernel, input: FrameInput) -> FrameOutput;
}
```

验收：

* 每个 pass 的执行顺序可 trace。
* 每个 pass 消费自己的 flag。
* 超过 rewrite limit 时输出 diagnostic，不死循环。
* coordinator 不包含 widget-specific branch。

---

### 5.2 `runtime/invalidation.rs`

是什么：

* typed dirty model
* invalidation queue
* invalidation merge rules

为什么：

* raw string dirty id 无法表达 dirty 原因、目标角色、传播范围。
* dropdown popup、callback、layout、focus、resource 不应共用同一条 dirty 路径。

怎么做：

```rust
pub enum InvalidationTarget {
    Scope(ScopeId),
    Node(NodeId),
    Element(ElementId),
    Layer(LayerId),
    Resource(ResourceId),
    Runtime,
}

pub enum InvalidationSource {
    Signal(SignalId),
    Event(EventId),
    Timer,
    Resource,
    Runtime,
}

bitflags::bitflags! {
    pub struct DirtyFlags: u32 {
        const COMPOSE   = 1 << 0;
        const RECONCILE = 1 << 1;
        const LAYOUT    = 1 << 2;
        const LAYER     = 1 << 3;
        const FOCUS     = 1 << 4;
        const HIT       = 1 << 5;
        const DRAW      = 1 << 6;
        const PLATFORM  = 1 << 7;
    }
}

pub enum InvalidationPropagation {
    SelfOnly,
    Children,
    Subtree,
    Ancestors,
    LayerOwner,
    Global,
}
```

验收：

* 所有 pass scheduling 只读 `Invalidation`，不读 raw dirty string。
* debug snapshot 可以说明每个 dirty scope 的 source、target、flags、propagation。
* popover/layer dirty 有明确 target。
* signal set 能产生 typed invalidation。

---

### 5.3 `runtime/identity.rs`

是什么：

* runtime 内部 id 类型定义与分配器

为什么：

* 当前 string id 同时代表 element、scope、callback、draw、hit、layer，生命周期无法清理干净。

怎么做：

```rust
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ScopeId(NonZeroU64);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct NodeId(NonZeroU64);

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct LayerId(NonZeroU64);
```

实现要求：

* subsystem API 不接受 raw string 作为 runtime identity。
* debug label 单独保存。
* 不同 role id 不得直接比较。
* 如果从同一个 allocator 派生，也必须保持类型不可混用。

验收：

* string prefix 不再用于判断 parent/child。
* 两个 id label 相似不会影响 dirty routing。
* layer id 与 parent widget id 不可互换。

---

### 5.4 `runtime/view_tree.rs`

是什么：

* DSL 每帧产生的声明事实树

为什么：

* declaration 必须与 retained state 解耦。
* 先构建完整 ViewTree，再由 reconciler 处理 retained reuse。

怎么做：

```rust
pub struct ViewNode {
    pub key: ViewKey,
    pub kind: WidgetKind,
    pub props: ViewProps,
    pub style: StyleFacts,
    pub layout: LayoutHints,
    pub callbacks: Vec<CallbackFact>,
    pub signal_watches: Vec<SignalWatchFact>,
    pub layer_intents: Vec<LayerIntent>,
    pub children: Vec<ViewNode>,
    pub debug_label: Option<String>,
}
```

验收：

* ViewTree 不持有 RetainedNode 引用。
* ViewTree 不保存 old callback。
* ViewTree 不决定 node reuse。
* ViewTree 可在 test 中 snapshot。

---

### 5.5 `runtime/retained_tree.rs`

是什么：

* runtime committed tree
* 保存稳定 identity、callbacks、layout cache、interaction state

为什么：

* retained state 必须有明确 ownership 和 teardown 规则。

怎么做：

```rust
pub struct RetainedNode {
    pub node_id: NodeId,
    pub scope_id: ScopeId,
    pub kind: WidgetKind,
    pub props_hash: PropsHash,
    pub callbacks: CallbackSlots,
    pub layout_cache: LayoutCache,
    pub interaction: InteractionState,
    pub children: Vec<NodeId>,
}
```

验收：

* subtree remove 会清理 callbacks、signal deps、layer intents、focus/capture ownership。
* retained tree 可 snapshot。
* retained node lifecycle 由 reconciler 控制。

---

### 5.6 `runtime/reconcile.rs`

是什么：

* ViewTree 与 RetainedTree 的 diff/rebuild/reuse 系统

为什么：

* callback stale、scope stale、signal stale 的根因是 reuse 决策散落在 DSL/build 期间。

怎么做：

```text
reconcile(old, view):
    if no old:
        mount fresh subtree

    if old.key != view.key:
        replace subtree

    if old.kind != view.kind:
        replace subtree

    update props/style/layout facts

    replace callbacks from ViewTree

    reconcile children by explicit key first

    fallback by index/kind only when safe

    remove stale children and dispose owned runtime resources
```

callback transfer 规则：

只有同时满足以下条件，旧 callback 才能保留：

1. owning scope reused
2. node identity reused
3. callback role unchanged
4. callback dependency boundary unchanged
5. subtree 未因 callback capture 风险被重建

如果任一条件未知，必须替换 callback 或重建 subtree。

验收：

* `Ui::reuse_retained_element` 不再直接做 reuse 决策。
* `UiCallbacks::transfer_for_elements` 不再按 element name 盲转 callback。
* reconcile trace 能解释每个 reused/rebuilt/rejected decision。
* stale callback bug 可通过一条 trace 定位。

---

### 5.7 `runtime/events.rs`

是什么：

* input event normalization
* hit target selection
* callback command collection
* callback dispatch boundary

为什么：

* 当前 `Runtime::update_pointer` 混合 hit testing、focus、active、callback、state mutation、dirty marking。

怎么做：

```text
raw platform input
    -> normalized RuntimeInputEvent
    -> route against previous committed HitTree or capture owner
    -> collect UiEventCommand
    -> drop tree borrows
    -> execute callbacks
    -> emit invalidations
```

验收：

* pointer event 使用 previous committed hit tree。
* callback 执行时不持有 retained tree mutable borrow。
* pointer down/up/click 顺序可 trace。
* event consumed 状态可被 SkyEngine adapter 查询。

---

### 5.8 `runtime/focus.rs`

是什么：

* focus、capture、keyboard、text、IME、scroll、drag ownership manager

为什么：

* 一个 `focused_id` 无法表达 UI 是否应 capture app/game input。

怎么做：

```rust
pub struct InputOwners {
    pub pointer_hover: Option<NodeId>,
    pub pointer_active: Option<NodeId>,
    pub pointer_capture: Option<NodeId>,
    pub keyboard_focus: Option<NodeId>,
    pub text_focus: Option<NodeId>,
    pub ime_owner: Option<NodeId>,
    pub scroll_owner: Option<NodeId>,
    pub drag_owner: Option<NodeId>,
}
```

FocusPass 必须：

* 验证 requested focus 是否仍存在。
* 重新计算 focused path。
* 在 focus changed 时清理 text/IME ownership。
* focused node removed 时选择 fallback。
* 区分 pointer focus 与 keyboard focus-visible。

验收：

* text input active 时 keyboard/text 不泄漏给 game input。
* pointer capture 离开原 rect 后仍能稳定收到 release/cancel。
* focused node 被 remove 后不会留下 stale focus。
* IME start/end/move 都从 committed geometry 产生。

---

### 5.9 `runtime/layers.rs`

是什么：

* first-class layer stack
* popover/dialog/tooltip/toast/dropdown 管理器

为什么：

* floating UI 不属于普通 parent layout tree，但又有 logical owner、anchor、focus、dismissal、z-order。

怎么做：

```rust
pub struct LayerIntent {
    pub layer_id: LayerId,
    pub owner_scope: ScopeId,
    pub anchor: Option<NodeId>,
    pub fallback_anchor: Option<LayoutRect>,
    pub kind: LayerKind,
    pub open: bool,
    pub placement: PlacementPolicy,
    pub focus: LayerFocusPolicy,
    pub outside_click: OutsideClickPolicy,
    pub z_order: ZOrderPolicy,
}
```

LayerManager 必须负责：

* layer create/reuse/remove
* anchor lookup
* fallback anchor
* first-frame placement
* z-order
* topmost hit testing
* outside click dismissal
* modal input blocking
* focus restore
* layer cleanup on owner dispose

验收：

* dropdown open 只产生一个 layer。
* outside click 关闭 layer 且不误点击底层内容。
* layer z-order 优先于 DOM/tree order。
* dropdown 不再需要 widget-local dirty owner hack。

---

### 5.10 `runtime/layout.rs` 与 `runtime/compose.rs`

是什么：

* LayoutPass 计算 local target frames。
* ComposePass 计算 global frames、transforms、clips、scroll offsets、hit geometry input。

为什么：

* scroll、animation、popover、Drift bug 需要区分 layout frame 与 composed/draw frame。
* Taffy 后续只能替代 LayoutPass，不能替代 ComposePass。

怎么做：

```text
LayoutInput:
    retained node tree
    layout hints
    text measurement inputs
    viewport constraints

LayoutOutput:
    local target frame
    content size
    baseline / text metrics

ComposeOutput:
    global frame
    clip
    transform
    hit bounds
    draw bounds
```

验收：

* visual-only animation 不触发布局。
* scroll offset 只触发 compose/hit/draw，不强制 parent layout。
* Drift probe 能定位差异属于 layout、compose、layer、hit、draw、renderer 哪一层。
* LayoutBackend 可 feature-gate 替换。

---

### 5.11 `runtime/trace.rs`

是什么：

* 结构化 correctness trace
* profile stream
* debug snapshot

为什么：

* retained UI bug 很多是 temporal bug，单看最终像素不够。

怎么做：

必须至少提供：

```text
InvalidationTrace
ReconcileTrace
EventTrace
LayerTrace
FocusTrace
LayoutTrace
DrawTrace
```

验收：

* 每个 dropdown/input regression 都有 trace assertion。
* screenshot-only test 不算通过。
* pass coordinator 超限时输出 surviving flags 和最近 invalidation source。

---

## 6. 标准 Frame Pipeline

每帧必须遵守以下顺序：

```text
1. Collect raw platform input.
2. Normalize input into RuntimeInputEvent.
3. Route pointer input through capture owner or previous committed HitTree.
4. Route keyboard/text/IME through focus/input owners.
5. Collect UiEventCommand records.
6. Execute callbacks outside retained tree borrows.
7. Collect signal/resource/focus/layer invalidations.
8. Compose ViewTree.
9. Reconcile ViewTree with previous RetainedTree.
10. Layout retained tree.
11. Resolve layers.
12. Compose global frames, clips, transforms, scroll offsets.
13. Update focus/capture/IME platform effect data.
14. Build HitTree.
15. Build DrawList.
16. Render.
17. Commit frame state for next frame.
18. Emit platform effects.
```

实现可以拆分或合并相邻 pass，但不得改变以下边界：

* event dispatch 不得使用 partially rebuilt tree。
* callback 不得在 hit tree traversal 中直接 mutate retained tree。
* declaration 不得读取 previous retained node 来决定 reuse。
* layer placement 必须来自 committed/fallback anchor。
* platform IME effects 必须在 focus/layout/compose 提交后发出。

---

## 7. Pass Coordinator 规则

### 7.1 PassFlags

```rust
pub struct PassFlags {
    pub request_event: bool,
    pub request_mutation: bool,
    pub request_compose_ui: bool,
    pub request_reconcile: bool,
    pub request_layout: bool,
    pub request_layer: bool,
    pub request_focus: bool,
    pub request_compose_geometry: bool,
    pub request_hit: bool,
    pub request_draw: bool,
    pub request_platform_effects: bool,
}
```

### 7.2 Rewrite Loop

```text
REWRITE_PASSES_MAX = 4

while flags remain and iteration < REWRITE_PASSES_MAX:
    run requested passes in canonical order
    each pass clears its consumed flag
    each pass may emit later flags
    if a later pass emits earlier flag:
        repeat rewrite loop

if flags remain:
    record diagnostic
    request another frame
```

### 7.3 Zombie Flag 禁止项

pass 执行后，如果对应 request/needs flag 仍然存在，必须满足以下之一：

* flag 被明确转成 later-frame request；
* pass 输出 diagnostic；
* pass 将工作升级为 full rebuild；
* pass 失败并阻止 phase 通过验收。

不得无解释地保留 zombie flag。

---

## 8. Signal 与 Scope 规则

### 8.1 Scope owns dependencies

`ScopeId` 拥有：

* signal dependencies
* callback facts
* child scopes
* layer intents
* retained nodes bounded by the scope

### 8.2 Scope rerun 前必须清理旧依赖

```text
before recomposing scope:
    remove scope from old signal subscribers
    clear scope -> signal edges

during declaration:
    signal.read() records current scope observer

after declaration:
    commit new edges
```

### 8.3 Signal set 行为

```text
signal.set(value):
    if value changed:
        for observer in signal_to_observers:
            emit Invalidation {
                target: Scope(observer),
                source: Signal(signal_id),
                flags: COMPOSE | RECONCILE | DRAW,
                propagation: implementation-defined,
            }
```

验收：

* conditional scope 从 signal A 切到 signal B 后，A 不再 dirty 该 scope。
* removed scope 不再接收 signal invalidation。
* signal dependency graph 可 snapshot。

---

## 9. 实施阶段

### Phase 0. Baseline Lock

目的：

* 先锁住当前行为，避免重构期间无法判断是新 bug 还是旧 bug。

任务：

* 增加 full-compose comparison mode。
* 给 Stress Lab dropdown/input/drift 增加 baseline snapshots。
* 增加 backend event-order test，不只测 Runtime direct calls。
* 添加最小 trace skeleton：event、dirty、layout、draw。

产出：

* `runtime/trace.rs`
* `tests/dropdown_event_order.rs`
* `tests/input_capture.rs`
* `tests/drift_probe.rs`

退出条件：

* 现有 bug 能被稳定复现。
* 每个复现用例至少有 event trace 和 dirty trace。
* CI 可以运行 baseline tests。

---

### Phase 1. Pass Coordinator Skeleton

目的：

* 把 runtime frame path 从单体流程改为可观察 pass sequence。

任务：

* 新增 `runtime/frame.rs`。
* 定义 `PassFlags`。
* 将现有 frame path 包进 coordinator。
* 暂时允许 pass 内部仍调用旧实现，但 pass 顺序必须固定。
* 加 rewrite limit 和 diagnostic。

产出：

* `FrameCoordinator`
* `FrameTrace`
* `PassFlags`

退出条件：

* 所有 frame pass 顺序可 snapshot。
* coordinator 不包含 widget-specific logic。
* 超过 rewrite limit 时不死循环。
* 旧功能在 full-compose fallback 下仍可运行。

---

### Phase 2. Typed Invalidation

目的：

* 替换 raw dirty string routing。

任务：

* 新增 `runtime/invalidation.rs`。
* 定义 `InvalidationTarget`、`InvalidationSource`、`DirtyFlags`、`Propagation`。
* signal write 输出 typed invalidation。
* pass coordinator 只消费 invalidation，不解释 raw string。
* 保留 transitional adapter，但 adapter 必须立即转换为 typed invalidation。

产出：

* typed invalidation queue
* invalidation merge rules
* invalidation trace snapshot

退出条件：

* debug 输出能说明每个 dirty 的 source/target/flags。
* popover/layer dirty 有明确 Layer target。
* 没有 pass 通过 string prefix 推断 dirty ancestry。
* stale dirty id 问题有 regression test。

---

### Phase 3. Role-Specific Identity

目的：

* 停止 string id 多角色复用。

任务：

* 新增 `runtime/identity.rs`。
* 添加 `ScopeId`、`NodeId`、`ElementId`、`CallbackId`、`LayerId`、`SignalId`。
* subsystem API 改为接受 typed id。
* debug label 与 runtime identity 分离。
* 清理 manually constructed related string ids。

产出：

* id allocator
* debug label map
* identity audit tests

退出条件：

* 不同 id role 不能直接比较。
* layer id 不能误用为 node id。
* callback id 不能误用为 element id。
* prefix 相似的 string label 不影响 dirty routing。

---

### Phase 4. ViewTree + Reconciler Extraction

目的：

* 中央化 retained reuse 和 callback transfer。

任务：

* 新增 `ViewTree`。
* DSL/build 只产生 ViewNode facts。
* 新增 `runtime/reconcile.rs`。
* 将 `Ui::reuse_retained_element` 迁出 declaration path。
* 将 callback transfer 移入 reconciler。
* 默认策略先 correctness-first：不能证明安全就 rebuild。

产出：

* `ViewTree`
* `RetainedTree` lifecycle hooks
* `ReconcileTrace`
* callback transfer proof rules

退出条件：

* declaration 不再读取 previous retained node 来决定 reuse。
* callback stale bug 可以从 ReconcileTrace 解释。
* callback 不再按 element name 盲转。
* full rebuild fallback 可用。

---

### Phase 5. Layer and Portal Manager

目的：

* 把 dropdown/popover/dialog/toast 变成 first-class layer state。

任务：

* 新增 `runtime/layers.rs`。
* 定义 `LayerIntent`。
* 实现 layer registry / layer stack。
* dropdown 改为声明 LayerIntent。
* 实现 outside-click dismissal、modal blocking、focus restore。
* layer hit testing 先于 base tree。

产出：

* `LayerManager`
* `LayerStack`
* `LayerTrace`
* dropdown layer migration

退出条件：

* 点击 dropdown trigger 只打开一个 layer。
* outside click 关闭 layer 且不触发底层 click。
* dropdown 选择项后 value 更新，layer close policy 正确执行。
* popover z-order 与 hit target deterministic。
* widget-local popup dirty hack 删除或不可达。

---

### Phase 6. Event / Focus / Capture Manager

目的：

* 明确 UI input ownership，阻止 UI 和 game input 双消费。

任务：

* 新增 `runtime/events.rs` 和 `runtime/focus.rs`。
* event target selection 与 callback execution 分离。
* 定义 `InputOwners`。
* 实现 pointer capture、keyboard focus、text focus、IME owner。
* SkyEngine adapter 使用 `UiCaptureState`，不再只看 `focused_id().is_some()`。
* 增加 keyboard/text/IME route tests。

产出：

* `UiEventCommand`
* `InputOwners`
* `UiCaptureState`
* `FocusTrace`
* `EventTrace`

退出条件：

* text input active 时，typed text 不进入 game input。
* pointer capture 离开原 hit rect 后仍收到 release。
* focused node remove 后 focus fallback 正确。
* IME start/end/move 来自 committed geometry。
* dropdown outside click 与 modal blocking 行为稳定。

---

### Phase 7. Layout / Compose Boundary

目的：

* 为 Drift 诊断和未来 Taffy 打基础。

任务：

* 新增 `runtime/layout.rs` 和 `runtime/compose.rs`。
* 定义 `LayoutInput`、`LayoutOutput`、`ComposeOutput`。
* 把 scroll offset、transform、clip、global frame 从 LayoutPass 中分离。
* hit tree 使用 ComposeOutput。
* draw list 使用 ComposeOutput。
* Drift probe 同时记录 layout frame、global frame、hit frame、draw frame、renderer scissor。

产出：

* layout snapshot
* compose snapshot
* geometry trace
* drift regression suite

退出条件：

* scroll offset 不触发 parent layout。
* visual-only animation 只触发 compose/draw，不触发布局。
* Drift bug 能定位到具体 boundary。
* LayoutBackend 可替换但默认仍用现有 layout。

---

### Phase 8. Memo / Partial Retained Optimization

目的：

* 在正确性稳定后恢复性能优化。

任务：

* 增加 explicit `memo_scope`。
* memo scope 必须声明 deps。
* scheduler 决定是否执行 closure。
* clean scope 通过 `ViewNode::ReuseScope(scope_id)` 交给 reconciler。
* callback capture 不安全时拒绝 memo reuse。

产出：

```rust
ui.memo_scope("controls", deps, |ui| {
    draw_controls(ui);
});
```

退出条件：

* memo reuse 有 ReconcileTrace。
* deps 变化会 rebuild。
* focus/layer/resource invalidation 触及 scope 时会拒绝 reuse。
* callback freshness 有测试覆盖。

---

### Phase 9. Optional Taffy Backend

目的：

* 在 runtime contracts 稳定后减少手写 layout 复杂度。

任务：

* 新增 feature-gated Taffy backend。
* 将 row/column/fill/grow/min/max/margin/padding 映射到 Taffy。
* stack/absolute/popover 等保留 runtime-specific 逻辑。
* 对比 gallery 和 Stress Lab 的 LayoutOutput。

产出：

* `LayoutBackend` trait
* `TaffyLayoutBackend`
* layout diff tests

退出条件：

* 开关 Taffy 不改变 widget API。
* layout 差异有文档和测试。
* Taffy 不参与 dirty/event/focus/layer/callback 决策。

---

### Phase 10. Renderer and Resource Feedback

目的：

* renderer-driven dirty 也走 typed invalidation。

任务：

* font/image/texture readiness 返回 `ResourceDirty`。
* renderer 不直接 request broad full redraw。
* draw/resource invalidation 细分到 node/layer/resource。
* wgpu facade 保持稳定。

产出：

* `ResourceInvalidation`
* renderer feedback queue
* resource readiness tests

退出条件：

* image/font ready 只触发必要 redraw/recompose。
* renderer 不知道 signal、scope、widget 内部逻辑。
* resource dirty trace 可解释。

---

## 10. 测试矩阵

### 10.1 Dropdown / Layer

必须覆盖：

* click trigger opens exactly one layer
* click outside closes layer
* outside click does not click covered base content
* selecting item updates value
* selecting item closes or preserves layer according to policy
* reopening dropdown uses fresh callbacks
* layer z-order beats base tree order
* owner scope removal disposes layer

### 10.2 Signal / Scope

必须覆盖：

* scope rerun clears old signal dependencies
* conditional branch switch does not leave stale subscribers
* removed scope receives no dirty
* parent dirty runs before child dirty
* dirty descendant removed by parent rebuild becomes stale and ignored

### 10.3 Callback / Reconcile

必须覆盖：

* callback transfer accepted only with proof
* callback transfer rejected when role changes
* callback transfer rejected when dependency boundary changes
* structural mismatch forces rebuild
* ReconcileTrace contains reuse/rebuild reason

### 10.4 Event / Input

必须覆盖：

* pointer down/up/click order
* callback executes outside tree borrows
* pointer capture survives moving outside rect
* scroll routes to deepest scroll owner that can consume delta
* keyboard focus and text focus are distinct
* text input captures typed characters from app/game input
* IME owner start/end/move
* focused node removal clears text/IME owner

### 10.5 Layout / Compose / Drift

必须覆盖：

* layout frame stable when only transform changes
* compose frame updates on scroll
* hit frame matches composed geometry
* draw primitive frame matches composed geometry
* renderer scissor matches draw clip
* popover anchor source recorded
* fallback anchor use recorded

### 10.6 Renderer / Resource

必须覆盖：

* font ready triggers text layout/draw as needed
* image ready triggers draw/resource invalidation
* resource dirty does not force unrelated scope rebuild

---

## 11. 验证命令

Core EUI-NEO tests:

```bash
cargo test --manifest-path crates/eui-neo/Cargo.toml
```

WGPU renderer tests:

```bash
cargo test --manifest-path crates/eui-neo-wgpu/Cargo.toml
```

SkyEngine neo adapter tests:

```bash
cargo test --features ui-neo ui::neo
```

Stress Lab compile and tests:

```bash
cargo test --features ui-neo --example ui_neo_stress_lab
```

Example build checks:

```bash
cargo check --examples --features ui-neo
```

Visual verification:

```bash
SKY_NEO_SCREENSHOT_PATH=target/neo-stress.png \
SKY_NEO_SCREENSHOT_FRAME=45 \
SKY_NEO_EXIT_AFTER_SCREENSHOT=1 \
cargo run --example ui_neo_stress_lab --features ui-neo --release
```

---

## 12. 停止与回滚规则

每个 phase 必须遵守：

1. 先加 trace，再重构。
2. 先让 bug 可复现，再修复。
3. 保留 full-compose fallback，直到 retained path 通过同等测试。
4. 每个 phase 必须有退出条件。
5. 若 phase 引入大量 widget regression，冻结该 phase。
6. 冻结后先补 comparison trace，不得继续堆修复。
7. public API 可以改，但必须是为了消除错误 boundary，而不是暴露内部状态。
8. transitional adapter 必须内部使用、明确命名、在 phase exit 前删除或不可达。

---

## 13. 禁止项

以下行为视为架构违规：

* 用 raw string id 推断 ancestry、dirty ownership、callback identity。
* declaration 期间读取 previous retained node 来决定复用。
* callback 按 element name 直接 transfer。
* event dispatch 期间直接 mutate retained tree。
* popup root 作为普通 child 临时插入 root。
* `Runtime` 或 pass coordinator 堆积 widget-specific behavior。
* backend 修 runtime 行为。
* 用 Taffy 解决 dirty/event/focus/layer/callback 问题。
* screenshot-only regression test。
* 保留旧 dirty-id 模型作为 compatibility mode。
* 无 trace 的 retained reuse。

---

## 14. 最小可落地顺序

如果只能做最小闭环，按以下顺序执行：

```text
1. Phase 0: baseline + trace skeleton
2. Phase 1: pass coordinator
3. Phase 2: typed invalidation
4. Phase 4: ViewTree + conservative reconciler
5. Phase 5: layer manager, first migrate dropdown
6. Phase 6: input/focus/capture ownership
7. Phase 7: layout/compose boundary
```

其中第一批必须修通的真实场景是：

```text
dropdown trigger click
    -> opens one layer
    -> layer positioned from committed/fallback anchor
    -> item click updates selected value
    -> callbacks are fresh
    -> layer closes according to policy
    -> reopening sees new state
    -> outside click closes without hitting base content
```

这个场景贯穿 event、signal、invalidation、reconcile、layer、hit、draw，是 retained runtime 最小验收闭环。

---

## 15. 最终完成定义

本计划完成时，必须满足：

1. public DSL 仍然能声明 UI。
2. declaration 不再做 retained reuse。
3. typed invalidation 完全替代 raw dirty-id routing。
4. role-specific id 覆盖 runtime subsystem API。
5. reconciler 是唯一 reuse/callback transfer 决策点。
6. layer manager 管理 dropdown/popover/dialog/toast。
7. input ownership 区分 pointer、keyboard、text、IME、scroll、drag。
8. layout 与 compose 分离。
9. headless harness 能驱动真实 frame/event order。
10. 每个 retained bug 都能通过 trace 解释：

    * 谁 dirty
    * 为什么 dirty
    * 哪个 pass 消费
    * 是否 reuse
    * 为什么 reuse 或 rebuild
    * hit/focus/layer/draw 使用了哪份 committed geometry

达到以上条件后，才可以继续做 partial compose、memo scope、Taffy backend 和性能优化。
