# EUI-NEO Rust Retained UI Runtime Standard and Plan

本文是 SkyEngine Rust EUI-NEO UI runtime 的唯一集中计划。后续不要再为
EUI-NEO retained runtime、dirty、focus、popover、layout、renderer feedback
创建新的零散计划文件；新增决策应当合并回本文。

本文按“标准条款”的方式写：先定义是什么，再说明为什么，最后规定怎么落地。

## 1. 结论

Rust EUI-NEO 的 public DSL 可以继续保持 immediate-style declaration，但 runtime
内部必须演进为 retained/reconciled kernel。

目标形态如下：

```text
User DSL / widgets
    -> ViewTree              本帧声明事实
    -> Reconciler            决定 retained 复用、重建、卸载
    -> RetainedTree          提交后的运行时状态
    -> Layout / Layer / Focus / Hit / Draw
    -> Renderer / Platform Effects
```

核心结论：

1. 当前 dropdown、popover、text input、Drift、dirty 相关问题不是单纯 layout
   问题，而是 retained runtime 边界问题。
2. Taffy 只能作为后续 layout backend 候选，不能解决 dirty ownership、callback
   stale、event ordering、focus、IME、layer lifetime。
3. Declaration 只产生事实；runtime kernel 才能做 retained reuse 决策。
4. Runtime 必须有 typed invalidation、role-specific identity、event command、
   explicit input ownership、first-class layer manager 和可追踪 reconciler。
5. 任何不能证明安全的 retained reuse 必须退化为对应 subtree 的 full rebuild。
6. `Runtime` 可以继续是 public facade，但不得成为 god object。

## 2. 适用范围

本文适用于：

- `crates/eui-neo`
- `crates/eui-neo-wgpu`
- `src/ui/neo/`
- EUI-NEO gallery、Stress Lab、headless runtime tests
- dropdown、popover、context menu、dialog、toast、scroll、text input、focus、
  IME、animation、resource readiness 等 retained UI 行为

本文不适用于：

- 重写 SkyEngine 的所有 UI backend
- 一次性替换成第三方 UI 框架
- 立即把 Taffy 设为主 layout engine
- 为旧 string dirty id、旧 callback transfer、旧 popup root hack 保留兼容模式

## 3. 规范用语

本文使用以下规范词：

- 必须：架构正确性要求；不满足则不得声明阶段完成。
- 不得：禁止项；即使短期测试通过也视为架构违规。
- 应当：默认要求；偏离时必须在本文或代码注释中说明理由。
- 可以：允许但不强制。
- 实现定义：实现可以选择具体策略，但必须通过 trace/debug 输出解释该策略。

## 4. 术语

Declaration：DSL 和 widget 在一帧内声明 UI 事实的过程。Declaration 可以产生
node、style、layout hint、callback fact、signal watch、layer intent 和 debug
label。Declaration 不得读取 previous retained tree 来决定复用。

ViewTree：当前帧 declaration 产生的临时树。它只包含本帧事实，不包含上帧保留下来的
runtime 状态。

RetainedTree：runtime 提交后的稳定树。它包含 stable identity、callbacks、
interaction state、focus ownership、layout cache、layer membership、animation state。

DrawList：layout、layer、clip、transform、visual state 都提交之后交给 renderer 的
绘制输入。DrawList 不得修改 application state，也不得注册 signal dependencies。

Reconciler：唯一可以决定 retained reuse、rebuild、mount、unmount、callback
transfer、focus preservation、layout cache reuse 的 subsystem。

Invalidation：typed dirty record。它必须说明 target、source、dirty flags 和
propagation。

Layer：root-level floating UI state。dropdown、popover、context menu、dialog、
tooltip、toast、picker 都是 layer user。

Platform Effects：runtime 提交结果后发送给 host 的副作用，例如 start IME、end IME、
IME cursor rect、cursor shape、capture result。

## 5. 设计理由

当前 runtime 的问题不是缺少更多 if 分支，而是职责混在同一条路径里：

```text
declaration
retained reuse
dirty routing
callback transfer
event dispatch
focus/capture
popover lifetime
layout
hit testing
draw list generation
platform effects
```

这种结构会制造 temporal bug：

- 某个 scope dirty 后，root-layer popup 没有被标记 dirty。
- 复用 scope 时保留了旧 callback 或旧 signal dependency。
- event dispatch 期间直接触发 state change，dirty 收集顺序和 tree mutation 顺序不稳定。
- popover 依赖上一帧 anchor，又不属于父 layout tree。
- backend 输入事件顺序与 headless runtime test 顺序不一致。
- text focus、keyboard focus、IME owner、game input capture 被压成一个 `focused_id`。

因此正确方案不是“把某个 widget 修好”，而是让每类状态有唯一 owner、每个 pass
有明确输入输出、每次 retained reuse 都有可检查的理由。

外部参考只给出结论，不在本文堆调研摘录：

- Dioxus 的 dirty scope 思路说明：dirty 必须有 owner 和 lifecycle cleanup。
- Masonry/Xilem 的 pass model 说明：input、focus、layout、accessibility、paint 应当分阶段。
- Floem 的 text input/focus 设计说明：text editing 是状态机，不是普通 key callback。
- Taffy 的结论：适合替换 box layout 计算，不适合解决 runtime ownership。

## 6. 架构规则

### 6.1 Declaration 规则

Declaration 必须只产生事实。

Declaration 可以产生：

- node facts
- child order facts
- style facts
- layout hint facts
- callback facts
- signal watch facts
- layer intent facts
- debug labels

Declaration 不得：

- 读取 previous retained tree 来决定复用
- 转移 previous callback
- 推断 raw string dirty id 的含义
- 决定 partial layout
- 决定 event target order
- 决定 layer z-order
- 直接修改 input owner、focus owner、IME owner

### 6.2 Tree 规则

Runtime 内部必须区分 `ViewTree`、`RetainedTree` 和 `DrawList`。

`ViewTree` 必须是当前帧临时声明结果。它不得保存 previous-frame retained state。

`RetainedTree` 必须是运行时提交状态。callback、layout cache、focus、layer、
animation 等 retained state 必须以 RetainedTree 或其 subsystem state 为 owner。

`DrawList` 必须是 renderer input。它不得触发 callback，不得写 signal，不得改变
runtime ownership。

### 6.3 Identity 规则

内部 identity 必须 role-specific。以下 id 不得在 subsystem API 中互相混用：

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

`String`
只能作为 debug label，不得作为 reuse、dirty routing、callback lookup、hit testing、
layer ownership 的最终判断依据。

### 6.4 Invalidation 规则

Dirty path 的目标模型是：

```rust
struct Invalidation {
    target: InvalidationTarget,
    source: InvalidationSource,
    flags: DirtyFlags,
    propagation: InvalidationPropagation,
}
```

每条 invalidation 必须回答：

1. 什么 target dirty 了？
2. dirty 来源是什么？
3. 需要哪些 pass？
4. dirty 如何传播？

`dirty_ids: Set<String>` 不得作为 pass scheduling 的长期输入。

Dirty flags 应当映射到 pass flags：

```text
COMPOSE -> compose + reconcile
LAYOUT  -> layout + hit + draw
VISUAL  -> draw
DRAW    -> draw
LAYER   -> layer + hit + draw
FOCUS   -> focus + platform effects
HIT     -> hit
```

如果 invalidation 组合无法安全局部处理，runtime 必须退化为 full rebuild 或 full
layout，并在 trace 中记录原因。

### 6.5 Reconciler 规则

Reconciler 是唯一 retained reuse 决策者。

Reconciler 输入：

- previous RetainedTree
- current ViewTree
- typed invalidations
- identity map
- retained compatibility rules

Reconciler 输出：

- next RetainedTree
- mount/update/remove operations
- callback transfer decisions
- focus preservation or clearing decisions
- layout cache reuse decisions
- trace records

Callback transfer 只有在以下条件全部成立时才允许：

- owning scope reused
- node identity reused
- callback role unchanged
- dependency boundary unchanged
- subtree 没有因为 signal、event、resource、layer、layout 原因被重建

任一条件未知时，必须替换 callback 或重建 subtree。

### 6.6 Event 规则

Event dispatch 必须针对上一帧 committed hit tree。它不得针对正在 rebuild 的树。

规范 pipeline：

```text
1. collect raw platform input
2. normalize to runtime input events
3. route pointer through capture or previous HitTree
4. route keyboard/text/IME through explicit owners
5. collect UiEventCommand records
6. execute callbacks outside tree borrows
7. collect typed invalidations
8. run rewrite passes to quiescence
9. emit platform effects after commit
```

`UiEventCommand` 是 event target selection 和 callback execution 的边界。Event
targeting 可以读 committed hit tree；callback execution 必须在 tree borrow 外进行。

### 6.7 Input、Focus、Text、IME 规则

Runtime 必须维护 explicit input ownership。至少要区分：

```rust
struct InputOwners {
    pointer_hover: Option<NodeId>,
    pointer_active: Option<NodeId>,
    pointer_capture: Option<NodeId>,
    keyboard_focus: Option<NodeId>,
    text_focus: Option<NodeId>,
    ime_owner: Option<NodeId>,
    scroll_owner: Option<NodeId>,
    drag_owner: Option<NodeId>,
}
```


Text input 必须作为编辑状态机处理，而不是普通 key callback。Text-capable widget
必须能表达：

- buffer
- cursor
- selection
- preedit
- clipboard command
- undo/redo
- IME cursor rect
- horizontal scroll

Keyboard routing 必须区分：

- platform shortcuts
- text editing commands
- activation keys
- focus navigation
- widget-specific key handling
- unconsumed app/game input

UI backend 对 app/game input 的 capture 判断必须来自 runtime ownership，不得只看某个
旧 `focused_id`。

### 6.8 Layer 规则

Dropdown、popover、context menu、dialog、tooltip、toast、picker 必须通过
`LayerIntent` 表达 root-layer 内容。

Layer intent 至少应包含：

```rust
struct LayerIntent {
    id: LayerId,
    owner: ScopeId,
    anchor: NodeId,
    fallback_anchor: Option<LayoutRect>,
    open: bool,
    kind: LayerKind,
    placement: PlacementPolicy,
    focus: FocusPolicy,
    outside_click: OutsideClickPolicy,
    z_order: ZOrderPolicy,
}
```

Layer manager 必须负责：

- layer lifecycle
- anchor lookup
- first-frame fallback anchor
- viewport collision
- z-order
- modal input blocking
- outside-click dismissal
- focus restore
- layer hit-test order

Widget-local root popup hack 不得作为长期方案。

### 6.9 Layout、Scroll、Animation 规则

Layout pass 必须只处理 layout inputs 和 layout outputs。它不得处理 dirty ownership、
callback transfer、event dispatch、focus ownership、layer lifecycle。

Visual-only animation 不得通过改变 layout input 造成 retained scope 每帧结构变化。
progress fill、chart bar、pulse、meter 等应当使用 transform、clip、opacity、color 或
draw-time state。

Scroll widget 必须保持 viewport、content、scrollbar 和 offset owner 的边界清晰。
滚动 offset dirty 通常应触发 compose/draw/hit；只有 content size 变化时才触发 layout。

Taffy 只有在上述边界稳定后才可以作为 feature-gated experiment 引入。

### 6.10 Renderer、Resource、Platform 规则

Renderer readiness 必须回流为 typed resource invalidation，例如 image ready、font atlas
ready、text shaping cache ready。

Renderer 不得用 broad full redraw 掩盖可表达的 resource dirty path。短期退化可以存在，
但必须 trace。

Platform effects 必须在 runtime commit 后发出。IME start/end、IME cursor rect、cursor
shape、capture result 不得在 layout 或 event targeting 中间直接发出。

### 6.11 Observability 规则

Debug snapshot 必须能回答：

- 这个 scope 为什么 rebuild 或 reuse？
- 哪条 invalidation 导致了哪些 pass？
- callback 是否转移，为什么？
- layer 是否创建、复用、关闭，anchor 来源是什么？
- input event 命中了哪个 owner role？
- focus 为什么改变，IME owner 为什么改变？
- layout 是 full、partial 还是 degraded fallback？
- renderer/resource dirty 为什么触发 redraw？

Trace 不是可选调试糖；它是 retained runtime 正确性的验收条件。

### 6.12 No God Class 规则

Runtime 架构不得出现 god class、god module、god coordinator。

`Runtime` 可以作为 public facade，但它不得拥有以下策略：

- retained reuse policy
- dirty propagation policy
- callback transfer policy
- event routing policy
- focus/navigation policy
- text editing policy
- IME lifecycle policy
- layer placement and dismissal policy
- layout algorithm policy
- renderer/resource invalidation policy
- platform effects policy

这些策略必须分别归属到 focused subsystem。`Runtime` 的职责是组装 subsystem、保存高层状态、
提供 host-facing API、调用 frame coordinator。Frame coordinator 的职责是调度 pass，不是实现
所有 pass 的业务规则。

如果一个改动把大量 unrelated state 或 unrelated branches 加进 `Runtime`、`runtime/mod.rs`
或 frame coordinator，该改动不得视为完成。必须先拆到对应 subsystem。

判断 god class 的机械标准：

- 一个类型同时回答“谁 dirty、谁复用、谁聚焦、谁弹层、谁布局、谁绘制”中的三类以上问题。
- 一个模块同时持有 callback storage、layer state、focus state、layout cache、draw cache 的主要规则。
- pass coordinator 中出现 widget-specific、layer-specific、focus-specific、layout-specific 的策略分支。
- 为了跨模块访问方便，把 internal mutable state 提升成 public/expert API。

以上情况必须拆分，而不是用注释解释。

### 6.13 Decoupling 规则

Runtime 子系统必须通过 typed input/output 交互，而不是互相偷读 mutable internals。

要求：

- Declaration 输出 view facts。
- Invalidation subsystem 输出 pass flags 和 typed dirty records。
- Reconciler 输出 tree operations 和 reuse trace。
- Event subsystem 输出 `UiEventCommand` 和 input invalidations。
- Focus subsystem 输出 ownership changes 和 platform effect requests。
- Layer subsystem 输出 layer stack、geometry、hit ordering 和 layer invalidations。
- Layout subsystem 输出 layout frames and measurement data。
- Draw subsystem 输出 draw list。
- Resource subsystem 输出 resource invalidations。

Subsystem 不得通过 raw string convention、shared mutable maps、hidden side effects 来互相传递语义。
需要跨 subsystem 的信息必须显式建模成 typed record 或 pass output。

允许内部共享 storage，但 ownership 必须明确。共享 storage 不能成为绕过 subsystem contract 的理由。

### 6.14 No Compatibility Mode 规则

本计划不要求兼容旧 internal API、旧 dirty-id 语义、旧 callback transfer 行为、旧 popup root hack、
旧 focus/input ownership 或旧 god-runtime 组织方式。

不得新增 compatibility mode 来保留错误架构。以下形式都禁止作为长期方案：

- `legacy_dirty_ids` 与 typed invalidation 并行长期存在。
- 新 layer manager 外面再包一层保持 widget-local popup hack 的兼容 API。
- 新 focus owner 外面再模拟旧 `focused_id` 作为决策源。
- 新 reconciler 外面再让 DSL/builder 继续直接复用 old elements/callbacks。
- 新 pass coordinator 中保留旧 direct mutation path。
- 为了“不破坏 examples”把 internal runtime state 公开给应用。

临时 adapter 只允许在迁移 PR 内部存在，并且必须满足：

- crate-private；
- 名字标记为 transitional；
- 进入时立即转换成新 typed model；
- 不保留旧语义作为可选择行为；
- 在对应 phase exit criteria 前删除或变为不可达。

当兼容性和目标架构冲突时，目标架构优先。

## 7. Public API Standard

本章规定 EUI-NEO Rust API 的目标形态。它和 runtime 架构同等重要：runtime 可以重构，
但 public API 必须让应用作者知道哪些行为稳定、哪些接口只是低层逃生口、哪些旧形状正在迁移。

### 7.1 API 分层

EUI-NEO API 必须分层暴露：

```text
prelude / widgets       application authoring API
crate root exports      documented public API
testing                 stable test-driver API
expert                  renderer/tooling/debug API
runtime internals       crate-private implementation detail
```

`prelude` 是普通应用的首选入口。它应当包含 `Runtime`、`FrameInput`、`Frame`、`Ui`、
常用 primitives、widgets、signals、skin、event payload 和测试常用类型。

`expert` 是 renderer、debugger、tooling、benchmark 使用的低层入口。应用代码不应依赖
`expert` 来构建普通 UI。

`runtime::*` 内部模块不得因为方便而扩大 public surface。新增 public API 必须先归类到
`prelude`、crate root、`testing` 或 `expert`。

Host/backend integration 是 EUI-NEO 的使用方，不是本库 API 分层的一部分。SkyEngine
可以验证、适配和消费 EUI-NEO API，但不得反向定义 EUI-NEO 的 public API。

### 7.2 API 稳定等级

每个 public API 应当属于以下等级之一：

```text
Stable Authoring API
    普通应用可以长期依赖。破坏性修改必须有迁移理由。

Stable Runtime API
    host/backend 使用。包括 Runtime、FrameInput、Frame、FrameResult、platform effects。

Stable Testing API
    tests 和 examples 使用。允许补充能力，但不得随内部重构随意破坏。

Expert API
    renderer/tooling/diagnostics 使用。可以比 authoring API 更低层，但必须有文档契约。

Transitional API
    迁移期间保留。必须在本文写明退出条件。

Internal API
    crate-private。不得从 crate root、prelude 或 expert 暴露。
```

当前应当视为 transitional 的形状：

- 以 raw `String` 表示 runtime role 的 public/debug API。
- 让 `DirtyInput` 暴露过多 dirty routing 细节的 API。
- 直接暴露 retained compose 原因作为应用逻辑判断依据的 API。
- 需要用户手动拼接 popup/root-layer id 的 API。

Transitional API 可以保留用于兼容 examples，但不得作为新功能的推荐入口。

### 7.3 命名与 Rust 形态

Rust API 必须优先 Rust idiom，而不是机械复制 C++ 命名。

要求：

- public 方法使用 `snake_case`。
- builder 方法返回 `Self` 或 builder type，保持链式声明。
- boolean setter 优先使用语义明确的名字，例如 `.disabled(true)`、`.focusable(true)`。
- callback 使用 `on_click`、`on_press`、`on_text_input`、`on_scroll` 等 `on_*` 前缀。
- type 名使用 `UpperCamelCase`。
- enum variant 使用 `UpperCamelCase`。
- bitflags 或 dirty flags 使用清晰常量名。
- 不得为了 C++ 熟悉度添加长期 CamelCase alias。

C++ EUI-NEO 是行为参考，不是 Rust API 拼写参考。Rust API 可以调整命名、所有权和类型边界，
但必须保留已经认定为语义要求的 callback ordering、layout clamp、layer/focus/input 行为。

### 7.4 Authoring API

Authoring API 的中心对象是 `Ui`。用户应当通过 `Ui` 和 `widgets` 声明界面：

```rust
runtime.frame(input, |ui, screen| {
    ui.column("root")
        .fill()
        .spacing(8.0)
        .children(|ui| {
            widgets::button(ui, "save")
                .label("Save")
                .on_click(|| {})
                .build();
        });
});
```

`Ui` API 必须满足：

- `Ui` 只声明本帧 UI facts。
- `Ui` 不暴露 retained reuse 控制。
- `Ui` 可以读取上一帧 committed read-only facts，例如 `response`、`previous_frame`、
  `is_focused`。
- 上一帧读取 API 不得允许用户改变 retained reuse 决策。
- `Ui` 不得暴露 callback storage、scope roots、dirty owner stack、previous callbacks。
- `Ui` 不得要求用户理解 runtime internal tree。

Primitive authoring API 应当保持小而正交：

```text
ui.row(id)
ui.column(id)
ui.stack(id)
ui.rect(id)
ui.text(id)
ui.image(id)
ui.nine_slice(id)
ui.polygon(id)
ui.scroll_y(id)
ui.popover(id)
```

Widget authoring API 应当通过 `widgets::*` 暴露：

```text
widgets::button
widgets::checkbox
widgets::dropdown
widgets::input
widgets::slider
widgets::tabs
widgets::dialog
widgets::toast
widgets::scroll_y
widgets::popover
```

Widget builder 不得要求用户手动管理 retained scope、dirty owner、layer root 或 callback
transfer。复杂 widget 可以暴露 state/signal binding，但 dirty lifecycle 必须由 runtime 管理。

### 7.5 ID API

Public authoring API 可以继续接受 string-like ids，因为这对手写 UI 足够友好。但 runtime
内部必须转换为 typed identity。

目标 public 形态：

```rust
pub struct UiId(/* opaque */);

impl From<&'static str> for UiId;
impl From<String> for UiId;
```

推荐 builder 入口：

```rust
pub fn row(&mut self, id: impl Into<UiId>) -> ElementBuilder<'_>;
pub fn button(ui: &mut Ui, id: impl Into<UiId>) -> ButtonBuilder<'_>;
```

规范：

- Public `UiId` 是 authoring key，不等于 `NodeId`、`ScopeId`、`LayerId`。
- Runtime 可以从 `UiId` 派生 typed internal ids，但派生规则不得泄漏为 public contract。
- `UiId` 字符串内容只能作为 stable authoring key 和 debug label。
- 不得要求用户通过字符串前缀表达 parent/child、layer ownership 或 dirty propagation。
- Widget 可以生成 child ids，但生成规则必须局部、稳定、可 debug。

短期如果仍使用 `impl Into<String>`，新增 API 应当避免把该字符串命名为 `node_id`、
`scope_id`、`layer_id`。public 文档中应称为 `id` 或 `key`。

### 7.6 Element Builder API

`ElementBuilder` 是 primitive element 的 fluent API。它应当覆盖以下类别：

```text
layout: x/y/position/width/height/size/fill/wrap/min/max/margin/padding/spacing/align
visual: color/gradient/border/shadow/radius/opacity/blur/transform
text: text/font/font_size/font_weight/text_color/wrap/align/line_height
image: image_ref/image_fit/flip
input: interactive/focusable/disabled/cursor/state colors/callbacks
animation: transition/motion/visual transforms
clip: clip/rounded_clip/clip_to_radius
children: children/build/content closure
```

Builder setter 必须只写 declaration facts。它不得：

- 读取 previous retained tree 来决定是否写入 fact。
- 注册 signal dependency，除非该 setter 明确是 signal-binding API。
- 执行 callback。
- 修改 runtime input owners。
- 直接创建 root-layer content，除非通过 `LayerIntent`。

Builder callback API 必须只声明 callback facts。Callback 的 storage、freshness 和 transfer
由 runtime/reconciler 管理。

### 7.7 Widget API

Widget API 应当是 domain-level builder，而不是让用户拼 primitive internals。

每个 widget builder 必须说明：

- required inputs
- optional styling hooks
- state/signal bindings
- callbacks
- focus behavior
- keyboard behavior
- layer behavior
- dirty behavior
- accessibility role, when available

Dropdown、context menu、date/time/color picker、dialog、tooltip、toast、popover 等浮层
widget 的 public API 不得要求用户手动：

- 调用 `with_root_layer`
- 查 previous frame anchor 并自己定位
- 拼接 `.popup` dirty id
- 管理 outside-click dismissal
- restore focus
- 维护 z-index

它们应当通过 public builder 声明 intent：

```rust
widgets::dropdown(ui, "quality")
    .items(["Low", "Medium", "High"])
    .value_signal(value)
    .open_signal(open)
    .placement(PopoverPlacement::BelowStart)
    .on_changed(|value| {})
    .build();
```

Runtime 内部将该声明转换为 `LayerIntent`、focus policy、event commands 和 typed
invalidations。

### 7.8 Signal and State API

`State` / `Signal` 是 authoring API，但 dirty routing 是 runtime API。

Authoring API 目标：

```rust
let selected = state.signal("selected", 0);

widgets::dropdown(ui, "preset")
    .value_signal(selected)
    .build();
```

规范：

- Reading a signal during declaration registers dependency for the current scope owner。
- Writing a signal records dirty facts;用户不应手动指定 dirty propagation。
- Signal dirty output 可以被 host/backend 收集，但进入 runtime 后必须转换为
  `Invalidation`。
- Signal API 不得泄漏 scope dependency graph 的存储结构。
- Rebuild scope 前必须清除旧 signal dependencies；reuse scope 时保留有效 dependencies。

长期目标是让应用代码处理 signals，而 runtime 处理 invalidations。应用代码不应根据
`InvalidationTarget` 或 `PassFlags` 写业务逻辑。

### 7.9 Runtime and Frame API

`Runtime` 是 host-facing facade。目标主入口应当是单一 frame API：

```rust
pub fn frame<R>(
    &mut self,
    input: FrameInput,
    build: impl FnOnce(&mut Ui, Screen) -> R,
) -> FrameResult<R>;
```

`FrameInput` 应当包含：

```text
screen
delta_seconds
pointer events
scroll events
keyboard events
text/IME events
dirty records from State/Signal/resource
force_full_compose debug switch
```

`Frame` 应当包含：

```text
draw_list
needs_render
needs_compose
full_redraw/debug degradation status
platform effects
focused_ime_rect or IME effect
capture result
```

规范：

- `Runtime::frame` 必须执行完整规范帧顺序。
- `Runtime::frame_incremental` 可以作为 convenience API，但不得有不同 event/order
  semantics。
- `compose_tree_with_dirty` 这类低层 API 应当逐步降级为 testing/expert/internal，不应作为普通
  app 推荐入口。
- Runtime public API 不得暴露内部 pass coordinator 的 mutable state。
- Force full compose 是 debug/testing 能力，不是修业务 bug 的 public contract。

### 7.10 Event, Focus, and Text API

Event payload API 应当稳定、值语义、host-neutral。

Public event types 应当覆盖：

```text
PointerEvent
ScrollEvent
KeyboardEvent
TextInputEvent
ImeEvent
DragEvent
```

现有 `KeyboardEvent` 如果同时承担 key、text、IME，需要拆分或明确字段语义。

Callback API 必须区分：

```text
on_press
on_click
on_context_menu
on_drag
on_scroll
on_key
on_text_input
on_focus_changed
on_timer
```

Text editing API 不应只暴露 raw keyboard callback。Text widget/input widget 应提供状态绑定，
runtime 负责 text command、selection、preedit、clipboard、IME rect。

Focus API 应当以声明和查询为主：

```text
.focusable(true)
.request_focus(signal/command)       future
ui.is_focused(id)
runtime.has_keyboard_capture()
```

不得要求应用直接写 runtime focus owner 字段。

### 7.11 Layer API

Public layer API 应当让应用声明 intent，而不是操作 root-layer internals。

基本 authoring surface：

```rust
ui.popover("menu")
    .anchor("button")
    .fallback_anchor(rect)
    .placement(PopoverPlacement::BelowStart)
    .open(open)
    .outside_click(OutsideClickPolicy::Close)
    .children(|ui| {})
    .build();
```

规范：

- Public API 可以暴露 `PopoverPlacement`、`OutsideClickPolicy`、`FocusPolicy` 等策略类型。
- Public API 不暴露 `LayerState` mutable access。
- Layer z-order 默认由 runtime policy 决定；用户可以通过高层 semantic kind 调整，不应手写全局
  z-index 来修 ordering。
- `fallback_anchor` 是首帧 placement 能力，不是长期绕过 anchor lookup 的定位 API。

### 7.12 Skin, Theme, and Styling API

Skin/theme API 必须与 retained runtime 解耦。

规范：

- Skin registry 可以作为 Runtime resource。
- Widget builder 可以接受 style key 或 explicit style override。
- Styling 不得要求用户修改 retained element internals。
- Skin change 必须产生 typed invalidation。
- Visual-only style change 不应触发 full layout，除非它影响 measured size。

### 7.13 Testing API

`testing` API 是 retained runtime 的验收工具，不是内部实现泄漏。

Testing API 必须支持：

- 构造 frame input。
- 点击、输入、滚动、键盘、IME、drag。
- 查询 response、focus、text focus、layer open state、draw commands。
- 读取 debug snapshot 和 action trace。
- 驱动与 `Runtime::frame` 相同的 frame order。

Testing API 不得依赖内部 string prefix 规则来定位行为。测试可以通过 public `UiId` 或
debug label 查询，但断言应关注行为和 trace，而不是当前内部存储布局。

### 7.14 Expert API

`expert` API 面向 renderer、tooling、diagnostics、benchmarks。它可以暴露：

- `UiDrawList` and draw commands
- layout measurement helpers
- debug traces
- element snapshots
- cache stats
- renderer resource hooks

`expert` API 不得暴露让应用绕过 runtime lifecycle 的 mutable retained state。凡是会改变
focus、layer、callbacks、signal dependencies、retained tree 的能力，都不应放入 `expert`
作为普通 escape hatch。



### 7.15 API Evolution Rules

新增或修改 public API 必须回答：

1. 它属于哪个 API level？
2. 它是 stable、expert 还是 transitional？
3. 它是否泄漏 runtime internals？
4. 它是否要求用户理解 dirty/reconciler/layer internals？
5. 它是否能被 testing API 驱动？
6. 它是否有 debug trace？
7. 它是否与 C++ EUI-NEO 语义一致，若不一致，SkyEngine adaptation 是什么？

破坏性 API 修改允许发生在 `ui-neo` 实验阶段，但必须满足：

- 删除旧形状时同时更新 examples/tests/docs。
- 不保留错误语义的兼容层。
- 给出迁移后的推荐写法。
- 不把内部临时类型提升为长期 public API。

## 8. 模块目标

`Runtime` 保持 public facade，但内部职责应当收敛到以下模块：

```text
runtime/mod.rs             public facade and subsystem wiring
runtime/frame.rs           pass coordinator and frame order
runtime/invalidation.rs    typed invalidation and pass flags
runtime/ids.rs             role-specific internal ids
runtime/reconcile.rs       retained reuse and callback transfer
runtime/tree.rs            retained tree storage and lifecycle cleanup
runtime/interaction.rs     event commands and input routing
runtime/focus.rs           focus, capture, keyboard/text/IME owners
runtime/layers.rs          layer intents, stack, anchors, dismissal
runtime/layout.rs          layout input/output boundary
runtime/composition.rs     declaration orchestration and frame output
runtime/resources.rs       resource readiness invalidation
runtime/debug.rs           traces and snapshots
```

已有模块可以逐步迁移，不要求一次性文件改名。但阶段完成时，职责必须落在正确模块。

`Runtime` 和 frame coordinator 不得拥有 widget policy、reconciler policy、layer policy、
focus policy、layout policy、renderer policy。它们只负责连接和调度。

模块拆分不是可选优化，而是阶段验收条件。任何新增 subsystem 行为都必须落到对应模块；
如果暂时无法放入对应模块，应先新增 focused module，而不是继续扩大 `Runtime` 或
`runtime/mod.rs`。

## 9. 落地阶段

每个阶段都必须满足 exit criteria 才能进入下一阶段。文档阶段名是实施顺序，不是 git
分支名。

### Phase 0: Baseline and Regression Net

目的：先把当前行为固定住，让后续重构有可比较基线。

改动：

- 保留 full-compose fallback 或 comparison mode。
- 为 dropdown、text input、popover、scroll、focus 增加 real frame-order tests。
- Debug snapshot 保留 retained reason、dirty inputs、pass flags、input owners。
- Stress Lab 和 gallery 的关键路径必须能截图验证。

验收：

- 能区分 widget bug、layout bug、retained reuse bug、backend event-order bug。
- 当前已知 dropdown/input 类问题有 regression tests。
- `Runtime::frame` 和 backend path 使用同一帧顺序。

### Phase 1: Typed Invalidation Becomes the Dirty Spine

目的：停止用 raw string dirty id 决定 pass scheduling。

改动：

- `DirtyInput` 进入 runtime 后立即转换为 `Invalidation`。
- Signal dirty 必须携带 owner、source、flags。
- Runtime pass flags 只由 invalidation merge 产生。
- Signal dependency rebuild 前清理旧 scope dependencies。
- Scope remove 时清理 signal dependencies、callbacks、layer intents、retained nodes。

验收：

- Debug snapshot 能显示每条 dirty 的 source、target、flags、propagation。
- Rebuilt scope 不再保留旧 signal edges。
- Reused scope 保留有效 dependencies。
- 不再有 pass 通过 string prefix 推断 dirty ancestry。

### Phase 2: Frame Coordinator and Event Commands

目的：让帧顺序显式化，让 event targeting 与 callback execution 解耦。

改动：

- 引入或收敛 `runtime/frame.rs` 的 pass coordinator。
- `update_pointer`、keyboard、scroll、text path 改为 collect `UiEventCommand` 后统一执行。
- Callback execution 必须在 tree borrow 外进行。
- Input 后收集 dirty，再进入 rewrite passes。

规范帧循环：

```text
input pass
event command collection
callback execution
invalidation merge
compose/reconcile/layout/layer/focus/hit/draw rewrite loop
commit
platform effects
```

验收：

- `update_pointer` 不再拥有整条 event/callback/dirty/rebuild 链。
- 点击 dropdown field 产生 click command 和 layer/scope invalidation。
- same-frame outside click + text input 不会发送到 stale text owner。
- Event trace 能显示 raw event、target role、command、callback、invalidation。

### Phase 3: Role-Specific Identity

目的：停止把一个字符串同时当 widget、scope、node、callback、layer、dirty owner。

改动：

- 添加 internal id wrapper：`ScopeId`、`NodeId`、`CallbackId`、`LayerId` 等。
- Subsystem API 优先使用 typed id。
- 保留 debug label，但不得参与核心判断。
- 为“同前缀但非父子”的 id 增加测试。

验收：

- Dirty normalization 不依赖 string prefix。
- Layer id 与 parent widget id 不会误用。
- Callback lookup 和 hit target 使用明确 role。
- Debug snapshot 同时显示 typed id 和 readable label。

### Phase 4: Reconciler Extraction

目的：把 retained reuse 决策从 DSL/builder 散点中移出。

改动：

- `ViewTree` facts 与 retained state 分离。
- Reconciler 统一决定 mount/update/remove/reuse。
- Callback transfer 有 explicit rule 和 trace。
- Layout cache、animation state、focus preservation 由 reconciler 或对应 subsystem
  基于 reconciler result 决定。

验收：

- DSL/widget declaration 不再读取 previous retained tree 决定复用。
- 每个 rebuild/reuse 都有 `RetainedComposeReason` 或更细的 reconcile trace。
- Callback stale bug 能从 trace 定位。
- 不确定 reuse 时走 subtree rebuild。

### Phase 5: Layer Manager

目的：把 root-layer UI 从 widget-local hack 改为 first-class runtime state。

改动：

- `popover` 产出 `LayerIntent`，由 `runtime/layers.rs` 管理。
- dropdown/context menu/dialog/toast/pickers 逐步迁移到 layer manager。
- Outside-click、modal blocking、z-order、focus restore 由 layer policy 处理。
- Anchor 使用 previous committed geometry；首帧必须支持 fallback anchor。

验收：

- Dropdown popup dirty 不再依赖 widget-local string hack。
- Popover z-order 和 hit-test order deterministic。
- Modal layer 能阻止底层 input。
- Layer trace 能显示 created/reused/closed、anchor source、outside-click dismissal。

### Phase 6: Focus, Text, IME, and Capture

目的：让输入所有权成为 runtime first-class state。

改动：

- `keyboard_focus`、`text_focus`、`ime_owner`、`pointer_capture`、`scroll_owner`、
  `drag_owner` 分离。
- Text input 引入或收敛为编辑状态机。
- Focus pass 负责 focus path、focus-visible、fallback focus、IME owner 更新。
- Platform effects pass 统一发出 IME start/end/moved。

验收：

- UI keyboard capture 不再只依赖旧 `focused_id`。
- Text editing keys 不会被普通 widget key callback 抢走。
- Focused text node 移除后，text focus 和 IME owner 被清理或迁移。
- IME rect 在 layout commit 后更新。

### Phase 7: Layout Boundary Cleanup

目的：让 layout 可以被测试、替换、优化，而不牵扯 dirty/event/focus/layer。

改动：

- 定义 layout input/output structs。
- Layout cache invalidation 与 paint invalidation 分离。
- Scroll viewport/content measurement 规则写入 runtime tests。
- Visual-only animation 从 layout input 中移出。

验收：

- Layout backend 可以 behind trait 或 internal facade 替换。
- Scroll offset 不导致无意义 full layout。
- Animation samples 不造成 retained structure drift。
- Drift 类 bug 可从 layout frame、draw frame、transform、clip trace 中定位。

### Phase 8: Optional Taffy Experiment

目的：在 runtime ownership 稳定后，验证是否用 Taffy 减少手写 layout 复杂度。

改动：

- Feature-gated Taffy backend。
- row/column/fill/grow/min/max/margin/padding 映射到 Taffy style。
- stack/absolute/popover/root layer 保留 runtime-specific 逻辑。
- Gallery、Stress Lab、scroll、popover 对比现有 layout frames。

验收：

- 开启 Taffy 不改变 public widget API。
- 差异有 snapshot 或 trace 记录。
- Taffy 不参与 dirty、event、focus、callback、layer 决策。

### Phase 9: Renderer and Resource Feedback

目的：让 renderer/resource readiness 以 typed invalidation 回到 runtime。

改动：

- Image/font/text atlas readiness 产生 `ResourceDirty`。
- Renderer facade 保持稳定。
- Resource dirty 精确触发 redraw/recompose/layout。

验收：

- Image ready 不再只能 broad full redraw。
- Font/text metrics 变化会触发必要 layout。
- Renderer trace 能解释 redraw 来源。

### Phase 10: Compatibility Removal and API Polish

目的：删除过渡形状，收束 public/internal API。

改动：

- 删除旧 dirty-id frame APIs。
- 删除隐式 `String -> DirtyInput` 语义依赖。
- 删除 widget-local popup root hack。
- 删除直接 callback transfer 旧路径。
- 将 public API 按 Stable Authoring、Stable Runtime、Testing、Expert、Transitional 分层整理。
- 为 retained UI authoring 补齐推荐写法，避免 examples 继续使用 internal/transitional API。
- 文档更新到 README、AGENTS、examples。

验收：

- 没有旧兼容路径继续维持错误语义。
- Gallery、Stress Lab、examples 编译和截图验证通过。
- Debug snapshot 能覆盖 retained runtime 的主要决策。

## 10. 当前实施状态

截至本文重写时，代码中已经有部分目标切片：

- `runtime/invalidation.rs` 已存在 typed invalidation 和 pass flags。
- `DirtyInput` 已进入 public frame path。
- `runtime/interaction.rs` 已有 `UiEventCommand` 雏形。
- `InputOwners` 已开始区分 pointer、keyboard、text、IME、scroll、drag owner。
- `runtime/layers.rs` 已有 `LayerIntent` 初始形状。
- Debug snapshot 已包含 retained reason、invalidations、pass flags。
- Public API 目前已经有 `prelude`、`widgets`、`testing`、`expert` 分层雏形，但分层语义还没有完全收束。

因此下一步不是重新开荒，而是把这些切片从“局部存在”推进到“作为唯一通路”。

优先执行顺序：

1. Phase 1：typed invalidation 成为唯一 dirty spine。
2. Phase 2：frame coordinator 和 event command 成为唯一 event path。
3. Phase 3：内部 typed ids 逐步替代 string role overloading。
4. Phase 4：reconciler 抽取，停止 DSL/builder 决定 reuse。
5. Phase 5：popover/dropdown 迁入 layer manager。
6. API pass：同步标注 stable/expert/transitional surface，避免新代码继续依赖旧形状。

## 11. 禁止项

以下做法不得作为长期方案：

- 用 raw string prefix 推断 parent/child、dirty ownership 或 layer ownership。
- widget declaration 读取 previous retained node 来决定复用。
- 通过 element name 匹配直接转移 callback。
- event dispatch 期间直接修改 retained structure。
- dropdown/popover root content 继续作为 ad hoc normal child 注入。
- 用 Taffy 解释 dirty、event、focus、callback、layer bug。
- 把 `Runtime`、`runtime/mod.rs` 或 pass coordinator 改成所有策略的集中大类。
- 用 full redraw/full rebuild 永久掩盖可表达的 typed dirty path。
- 保留兼容层来维持旧 dirty、event、callback、layer、focus 语义。
- 把 internal runtime 类型为了方便直接导出到 `prelude`。
- 让 public widget API 要求用户手动管理 retained/layer/dirty internals。
- 新增 god class、god module 或 god coordinator。
- 为了迁移方便长期保留 legacy mode、compat mode、dual path。
- 让 subsystem 通过 raw string convention 或 shared mutable internals 偷传语义。

## 12. 验证命令

Core EUI-NEO tests：

```bash
cargo test --manifest-path crates/eui-neo/Cargo.toml
```

WGPU renderer tests：

```bash
cargo test --manifest-path crates/eui-neo-wgpu/Cargo.toml
```

SkyEngine neo adapter tests：

```bash
cargo test --features ui-neo ui::neo
```

Example build checks：

```bash
cargo check --examples --features ui-neo
```

Stress Lab visual verification：

```bash
SKY_NEO_SCREENSHOT_PATH=target/neo-stress.png \
SKY_NEO_SCREENSHOT_FRAME=45 \
SKY_NEO_EXIT_AFTER_SCREENSHOT=1 \
cargo run --example ui_neo_stress_lab --features ui-neo --release
```

Gallery overlay states should also be verified with the existing
`SKY_NEO_GALLERY_*` environment variables when layer, focus, picker, dialog, or
popover behavior changes.

## 13. Completion Definition

本计划完成时，应当满足：

- Public DSL 仍然是 ergonomic immediate-style declaration。
- Public API 已按 authoring、runtime、testing、expert、internal 分层。
- Transitional API 有明确退出条件并已清理旧错误语义。
- Runtime 内部已有明确 retained/reconciled kernel。
- Runtime、frame coordinator 和 `runtime/mod.rs` 没有变成 god class/god module。
- 子系统通过 typed records 和 pass outputs 解耦，而不是共享隐式 mutable 语义。
- 旧 dirty、callback、popup、focus、event direct-mutation 路径没有兼容模式残留。
- Dirty path 使用 typed invalidation，而不是 raw string dirty routing。
- Event path 使用 command collection，而不是边 hit-test 边直接 mutation。
- Input ownership 区分 pointer、keyboard、text、IME、scroll、drag。
- Popover/dropdown/dialog/toast/picker 通过 layer manager 管理。
- Reconciler 是唯一 retained reuse 决策者。
- Layout boundary 可替换，Taffy 可选但不承担 runtime ownership。
- Debug trace 能解释 scope rebuild/reuse、callback transfer、layer placement、
  focus/IME、resource redraw。
- Stress Lab、gallery、headless tests、SkyEngine adapter tests 全部通过。
