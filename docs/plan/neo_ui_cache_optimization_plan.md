# Neo UI 缓存优化计划

文档状态：本轮实现完成，后续只保留观察项。

目标不是堆一个全局缓存大类，而是把可复用结果放回它自然产生的层：

- `Ui` 只缓存组合期需要的上一帧布局查询。
- `Runtime` 只缓存 runtime 语义产物，例如 draw list。
- `WgpuRenderer` 只缓存 GPU/text/image/临时向量等 renderer 产物。
- 后续如果需要新缓存，优先做小结构、小模块、局部所有权，不做 `NeoCacheManager` 这种 God Class。

## 当前结论

Neo UI 功能路径已经比较健康。真实场景里最大的问题不是单个算法爆炸，而是静态 UI 每帧重复做了太多确定性工作：上一帧 frame map、draw traversal、glyphon prepare、临时 Vec 分配、GPU buffer 重写。这类工作适合缓存，因为 key 很明确，且不会改变交互语义。

本轮已落地的优化让静态 draw-list 读取从微秒级降到接近 `Arc` clone 成本；真实 stress lab 仍保持功能完整，dialog、context menu、toast、scroll、rounded clip 都正常。

另外，本轮专门验证了 ID interning：`Arc<str>` + shared cache 和移动式小 cache 两个版本都会让 `compose_96_rows` 回退，因此已撤回。这个结论反而说明缓存不能凭感觉加，必须用 bench 卡住。

## 本地测量

机器：RTX 3070，release，vsync off，截图帧约 180。

| 场景 | 优化前参考 | 本轮结果 | 备注 |
| --- | ---: | ---: | --- |
| stress lab 默认 | 512 FPS，frame 1.7 ms | 519 FPS，frame 1.3 ms | HUD: `ui 0.4 / ren 0.0 / ov 0.1`，frame 120 app profile: total `0.661 ms`, surface `0.021`, update `0.470`, submit/present `0.167` |
| stress lab 弹窗/菜单/toast | 405 FPS，frame 3.0 ms | 404 FPS，frame 3.4 ms | HUD: `ui 0.4 / ren 0.0 / ov 0.1`，弹窗/菜单/toast 均可见；frame 120 app profile: total `1.418 ms`, surface `0.034`, update `0.964`, submit/present `0.411` |
| common controls compose 96 rows | 约 1.25-1.43 ms | 1.344-1.469 ms | 保持 1.3-1.4 ms 档；text measure 命中免分配后 Criterion 判定显著改善 |
| common controls draw 96 rows | 约 866-911 us | 11.38-11.47 ns | draw-list 命中路径 |
| common controls hover hit-test | 约 22 us | 21.94-23.30 us | 基本持平，z-order 临时排序不再常态堆分配 |

## 架构边界

| 层 | 可以拥有的缓存 | 不应该拥有的缓存 |
| --- | --- | --- |
| `Ui` | 上一帧 frame 查询 | GPU buffer、text atlas、全局 draw cache、跨帧 ID 大缓存 |
| `Runtime` | tree structure snapshot、draw-list cache、animation/timer 状态 | wgpu 资源、glyph atlas、跨 renderer 批处理 |
| `Layout` | text measurement memo、subtree measure memo | 输入状态、GPU 状态 |
| `WgpuRenderer` | scratch Vec、text buffer/area、image metadata、vertex upload hash、render-op batch | DSL 元素树、callback、focus/input 语义 |
| SkyEngine app runner | frame lifecycle profiling | Neo core 的语义缓存 |

## 缓存清单

| 优先级 | 区域 | 缓存产物 | 所有者 | 失效条件 | 状态 |
| --- | --- | --- | --- | --- | --- |
| P0 | fixed/fill text layout | 固定尺寸 text leaf 跳过测量 | `layout.rs` | 宽/高变成 wrap-content | 已完成 |
| P0 | previous frame lookup | 上一帧 frame 按需查询并缓存结果 | `Ui` | 下一次 compose 移入新的 previous roots | 已完成 |
| P0 | draw list | `UiDrawList` 命令流 | `Runtime` | compose 结构/视觉变化、输入状态变化、动画 tick、字体/skin 变化 | 已完成 |
| P0 | draw list clone | `Arc<[UiDrawCommand]>` | `UiDrawList` | 新 draw list 生成 | 已完成 |
| P0 | renderer scratch | vertices/items/ops 临时 Vec | `WgpuRenderer` | renderer 销毁或容量策略调整 | 已完成 |
| P0 | text buffer | glyphon `Buffer` | `WgpuRenderer::TextLayer` | text/font/size/weight/wrap/box 变化 | 已有，继续保留 |
| P0 | text area prepare | glyphon `TextArea` key | `WgpuRenderer::TextLayer` | 位置、clip、颜色、viewport、buffer 变化 | 已完成首版 |
| P0 | text measure cache hit | 借用 `&str` 查找，miss 才保存 text | `DefaultTextSystem` | 字体注册、cache 超预算 | 已完成 |
| P1 | static vertex upload | vertex bytes hash | `WgpuVertexBuffer` | size 或内容 hash 变化 | 已完成 |
| P1 | image metadata | texture/bind group/UV/size | `WgpuRenderer` image cache | image revision、UV、size 变化 | 已有，继续强化 |
| P1 | ID 分配 | page-prefixed ID interning | `Ui` 或小型 id arena | page 或 local id 变化 | 已验证不适合：两版实现让 compose 回退，已撤回 |
| P1 | 静态文本 | label/icon shared string | builder/text helper | 文本变化 | 不改公开文本类型；已用 text measure cache 命中免分配覆盖主要收益 |
| P1 | render-op grouping | collected vertices/items/ops | `WgpuRenderer` | draw-list 指针、逻辑尺寸、sRGB、image cache revision 变化 | 已完成 |
| P1 | hit-test order | 反向交互顺序/z-order | `Runtime` 小缓存 | layout、z-index、clip、interactive/disabled 变化 | 待评估 |
| P1 | z-order | sibling sort order | runtime/draw 局部 helper | child order 或 z-index 变化 | 局部完成：`SmallVec` 去常态堆分配 |
| P1 | app frame rest | lifecycle phase timings | app runner | env `SKY_APP_PROFILE` 开启时采样 | 已完成 |
| P2 | layout subtree | measured subtree | layout 小缓存 | subtree signature 或 parent constraint 变化 | 暂缓：当前收益不支撑脏区复杂度 |
| P2 | draw subtree | subtree command slice | draw 小缓存 | visual、frame、clip、animation 变化 | 暂缓：draw-list 已接近 clone 成本 |
| P2 | offscreen static UI | 合成后的 UI texture | renderer | UI dirty、animation、resize、image revision | 暂缓 |

## 本轮已完成

1. `layout.rs` 增加 leaf fast path。固定尺寸 text 不再调用 text measure，wrap-content text 宽高共用一次测量。
2. `dsl.rs` 删除每帧完整 previous-frame HashMap 构建，改为移动 previous roots，并对 `previous_frame()` 做本帧按需缓存。
3. `draw.rs` 把 `UiDrawList` 改成 `Arc<[UiDrawCommand]>`，缓存命中时 clone 极轻。
4. `runtime.rs` 增加 draw-list 缓存和精确失效点，避免静态帧重复 draw traversal。
5. `runtime.rs` 保持结构签名轻量，只用来判断是否需要 render，不把完整 draw key 塞进 compose 热路径。
6. `eui-neo-wgpu` renderer 复用 scratch Vec，缓存 text area prepare key，跳过静态 vertex buffer 重写。
7. `eui-neo-wgpu` renderer 按 draw-list `Arc` 指针缓存 collected vertices/items/ops，不再每帧重走 command collect。
8. `runtime.rs` 把 render/compose/full-redraw 脏标记收成小 helper，减少后续缓存漏失效风险。
9. runtime/draw 的不稳定 z-order 临时排序改为 `SmallVec`，避免常见 sibling 数量下的堆分配。
10. 增加 `SKY_NEO_PROFILE` 轻量 profiling 输出，用于拆 compose/render 阶段耗时。
11. `DefaultTextSystem` 的测量缓存改成 header + 碰撞桶，命中时不再为 key 构造 `String`，只有 miss 才保存 text。
12. app runner 增加 `SKY_APP_PROFILE`，拆出 setup/input/assets/tick/surface_begin/update/screenshot/pre_present/end_submit_present/aux 等阶段，用来解释 stress lab 的 `rest`。
13. 为 render phase 的 `TilemapDrawData` 补齐 `PhasePayload` / `payload_kind`，解除当前工作树中阻塞 UI bench 的类型约束。

## 下一步

1. 用 `SKY_APP_PROFILE=1` 继续对比 Live2D 和 Neo：重点看 `surface_begin` 与 `end_submit_present`，不要只看 UI 自身时间。
2. hit-test order 缓存暂不强上；当前 hot path 没明显回退，真正缓存需要和树 revision/clip/interactive 签名绑定，避免维护成本超过收益。
3. layout subtree cache 暂不急着上，当前 compose 主要问题已经不在单点布局算法；等真实页面出现稳定 >2 ms compose 再做脏区设计。
4. ID interning 本轮已验证为负收益，不再作为默认优化方向；除非改成更低成本的 arena/typed-id 结构并用 bench 证明。

## 不做

- 不做一个集中式 God Class 缓存管理器。
- 不缓存 callback 结果、hover/click/focus 结果、IME 状态、scroll offset clamp 结果。
- 不保留没有数据收益的 ID interning；减少分配不是目标本身，frame time 才是。
- 不为了 FPS 关掉弹窗、滚动、rounded clip、text、modal blocking、animation、asset/image 功能。
- 不用超大 synthetic tree 作为唯一依据，真实 stress lab 和 control-center 类场景优先。

## 验证命令

```powershell
cargo test --manifest-path crates/eui-neo/Cargo.toml
cargo test --manifest-path crates/eui-neo-wgpu/Cargo.toml
cargo test --features ui-neo ui::neo
cargo check --example ui_neo_stress_lab --features ui-neo
cargo bench --bench neo_ui --features ui-neo -- neo_ui_common_controls/compose_96_rows --exact
cargo bench --bench neo_ui --features ui-neo -- neo_ui_common_controls/draw_96_rows --exact
cargo bench --bench neo_ui --features ui-neo -- neo_ui_common_controls/hover_hit_test_96_rows --exact
```

真实场景截图：

```powershell
$env:SKY_NEO_SCREENSHOT_PATH='C:\Coding\SkyEngine\target\neo_stress_probe.png'
$env:SKY_NEO_SCREENSHOT_FRAME='180'
$env:SKY_NEO_EXIT_AFTER_SCREENSHOT='1'
$env:SKY_APP_PROFILE='1'
cargo run --example ui_neo_stress_lab --features ui-neo --release
```

弹窗/菜单/toast 压力：

```powershell
$env:SKY_NEO_LAB_DIALOG_OPEN='1'
$env:SKY_NEO_LAB_TOAST_VISIBLE='1'
$env:SKY_NEO_LAB_CONTEXT_OPEN='1'
$env:SKY_NEO_SCREENSHOT_PATH='C:\Coding\SkyEngine\target\neo_stress_overlays_probe.png'
$env:SKY_NEO_SCREENSHOT_FRAME='180'
$env:SKY_NEO_EXIT_AFTER_SCREENSHOT='1'
$env:SKY_APP_PROFILE='1'
cargo run --example ui_neo_stress_lab --features ui-neo --release
```

已知验证限制：

- `cargo check --examples --features ui-neo` 当前被 `examples/game/fog_lantern_station/app.rs` 阻塞，该新例子把 `sky_engine::ui::neo::Color` 直接 `.into()` 成 `sky_engine::math::Color`，不是本轮 Neo cache 改动导致。
- `cargo test --features app tilemap` 当前被 `src/asset/server.rs` 里 `AssetRecord::install_task` 字段缺失阻塞，不是本轮 Neo cache 或 tilemap phase payload 改动导致。
