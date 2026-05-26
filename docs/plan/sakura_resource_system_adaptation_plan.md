# SakuraEngine Resource System Adaptation Plan

Status: draft execution plan  
Scope: compare SakuraEngine's resource system with SkyEngine's current asset/runtime design, identify gaps, and define a staged plan for borrowing the useful parts without copying unsuitable C++ architecture.

## 0. Implementation Progress

Last updated: 2026-05-27

Implemented so far:

- Phase 1 partial: `Assets::load_blocking` no longer depends on a fixed 64-iteration update loop. It now drives a blocking-specific update path, sleeps between pending polls, and adds `load_blocking_with_timeout`.
- Phase 1 partial: asset failures now record a queryable `AssetFailurePhase` through `Assets::failure_phase` / `failure_phase_untyped`, covering read/dependency/install failures without changing `AssetEvent`'s public field layout.
- Phase 2 initial slice: background asset loads now submit to an internal bounded `AssetIoService` worker pool instead of spawning one OS thread per resource load. Queue-full submissions are deferred to later updates.
- Phase 2 partial: I/O worker count and queue capacity are now configurable through `AssetConfig` and `AssetPlugin`.
- Phase 3 initial slice: asset load requests now live in `src/asset/request.rs` with an internal `AssetRequestPhase` boundary, preparing the later request-state-machine extraction from `server.rs`.
- Phase 4 partial: runtime factories now use `begin_install -> AssetInstallResult`, with `Ready` for immediate installs and `Pending(AssetInstallTask)` for cross-frame installs. The old erased synchronous install path has been removed.
- Phase 2/3 partial: background load completions for released assets are now covered by an explicit regression test; stale completion discard is enforced through generation mismatch.
- Phase 7 initial slice: `Assets::stats()` now exposes queue depth, in-flight loads, retained events, reference counts, and per-state record counts.

Verified:

- `cargo test --features asset asset::`
- `cargo test --features app asset::`
- `cargo test --features asset`
- `cargo test`
- `cargo check --features audio,video`

## 1. 结论先行

SakuraEngine 对 SkyEngine 有参考价值，但最值得“抄”的不是具体代码，而是资源系统的分层和生命周期协议：

- `ResourceSystem` 统一调度资源请求。
- `ResourceRegistry` 负责把资源标识解析到真实文件、依赖和 metadata。
- `ResourceFactory` 明确拆开 I/O、反序列化、依赖等待、安装、卸载、跨帧安装轮询。
- 资源记录有显式状态机，能表达 `Loading -> Loaded -> WaitingDependencies -> Installing -> Installed`。
- I/O 服务和资源系统分离，理论上支持优先级、批处理、取消、RAM/VRAM staging。
- handle 持有引用，释放通过系统队列回收，避免直接把释放逻辑散落到调用者。

SkyEngine 已经有一套相当接近的雏形，不是空白：

- `src/asset/` 已有 `Assets` facade、`Handle<T>`、manifest/cook、依赖计数、reload closure、事件队列、后台加载开关、安装预算。
- `src/render/resources/texture_cache.rs` 已有 texture GPU residency、prepare budget、优先队列和事件驱动失效。
- `src/app/services.rs` / `src/app/lifecycle.rs` 已经把 asset/audio/video service update 纳入 app lifecycle。
- RenderGraph 已经实际借鉴过 Sakura 的 reorder、aliasing、blackboard 思路，说明“参考 Sakura 但 Rust 化落地”这条路线可行。

核心差距在于：SkyEngine 的资源系统目前“能用”，但还不是一个完整的运行时资源调度层。最明显的问题是后台加载为每个资源 `std::thread::spawn`、同步加载/安装会卡 app frame、`load_blocking` 轮询不可靠、install 不能跨帧、I/O 没有统一服务、hot reload 需要手动触发、CPU/GPU residency 边界只在 texture 上比较成熟。

所以计划应当是：保留 SkyEngine 当前 Rust API 和 ECS/app ownership 模型，吸收 Sakura 的资源生命周期协议和调度分层，逐步把现有 `asset` 模块从“manifest + handle + factory loader”升级成“可诊断、可预算、可取消、可热重载、可跨帧安装”的资源运行时。

## 2. 不照抄的边界

这些内容不建议照搬：

- 不引入 Sakura 那种全局 singleton `resource_system`。SkyEngine 应继续通过 `World` 资源和 app services 管理生命周期。
- 不照搬 C++ pointer/record 双态 handle。SkyEngine 的 `Handle<T>` / `WeakHandle<T>` 应保持类型安全、generation 校验和 Rust ownership 语义。
- 不把 DirectStorage、CGPU、Sakura VFS 的具体实现移植过来。SkyEngine 现在需要的是抽象 seam，不是平台专用实现。
- 不把 renderer backend 细节塞进 `asset`。GPU residency 应由 render/audio/video 等后端资源层消费 asset events 后自行管理。
- 不把资源系统重写成一个大而全的公共 service API。先保持 `Assets` 作为 app-facing facade，逐步拆内部模块。
- 不把 Sakura 目前也未完成的地方当成标准。例如 Sakura snapshot 中 async serde 被硬关、`FlushResource` 未实现、local registry cancel 是空实现，这些只能作为 warning，不是模板。

## 3. 参考源清单

SakuraEngine 重点参考：

- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/include/SkrRuntime/resource/resource_system.hpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/include/SkrRuntime/resource/resource_handle.h`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/include/SkrRuntime/resource/resource_factory.hpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/include/SkrRuntime/resource/resource_header.hpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/resource_system.cpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/resource_request_impl.hpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/resource_request.cpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/resource_handle.cpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/local_resource_registry.cpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/resource_factory.cpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/resource/resource_header.cpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/agent_docs/core_systems/resource_system.md`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/agent_docs/core_systems/io_service.md`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/include/SkrRuntime/io/ram_io.hpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/include/SkrRuntime/io/vram_io.hpp`
- `_refs/SakuraEngine_ref/engine/modules/engine/runtime/src/io/ram/ram_service.cpp`

SkyEngine 当前实现重点对照：

- `src/asset/mod.rs`
- `src/asset/types.rs`
- `src/asset/server.rs`
- `src/asset/registry.rs`
- `src/asset/texture.rs`
- `src/asset/cook.rs`
- `src/render/resources/texture_cache.rs`
- `src/render/runtime/frame/extract_frame.rs`
- `src/render/runtime/frame/prepare_frame_resources.rs`
- `src/app/services.rs`
- `src/app/lifecycle.rs`
- `src/app/frame.rs`
- `src/audio/assets.rs`
- `src/video/assets.rs`
- `docs/plan/asset_resource_system_plan.md`
- `docs/plan/asset_resource_system_standard.md`
- `docs/plan/asset_smart_handle_migration_plan.md`
- `docs/plan/world_resource_governance_plan.md`

## 4. Sakura 和 SkyEngine 逐项对比

| 维度 | SakuraEngine | SkyEngine 当前状态 | 差距判断 |
| --- | --- | --- | --- |
| 资源入口 | `ResourceSystem` 统一拥有 request queue、registry、factory、records | `Assets` facade / `AssetsInner` 统一维护 manifest、factories、records、events | 方向一致，Sky 更 Rust 化，但内部职责还集中在 `server.rs` |
| 资源标识 | `GUID` / `SResourceRecord*` 双态 handle | `AssetId`、`Handle<T>`、`WeakHandle<T>`、generation、typed runtime id | Sky 更安全，不需要照搬 Sakura handle |
| handle 引用 | `SResourceHandle` RAII 引用计数，drop 释放 | `Handle<T>` 持有 `AssetLease`，drop 走 release channel | 思路一致；需继续巩固 strong/weak 语义文档 |
| 资源状态 | `Unloaded, Loading, Loaded, WaitingDependencies, Installing, Installed, Uninstalling, Unloading, Error` | `AssetState` 基本已有同名状态 | Sky 已对齐状态名，但状态迁移逻辑还没有 request object 化 |
| 加载请求 | `ResourceRequest` 独立对象，`Update()` 驱动状态机 | `AssetsInner::apply_update` / `process_load_completions` 集中驱动 | Sakura 的 request object 值得借鉴，可降低 `server.rs` 复杂度 |
| I/O | Registry 生成 IO request，RAM/VRAM service 分层 | 背景加载直接 `std::fs::read`，每资源 spawn thread | Sky 最大短板之一：需要统一 bounded I/O service |
| 反序列化 | Factory `Deserialize`，理论支持 async serde | Factory `load(ctx)` 同步从 bytes 到 loaded object | Sky API 简洁，但不能表达长时间解析进度 |
| 依赖 | `LoadResource` 收集 dependencies，`WaitingDependencies` 等待 | `LoadedAsset.dependencies` + dependency_ref_count + cycle check | Sky 已有基础，需更好诊断和 request-level 解释 |
| 安装 | `Install` 可返回 installing，`UpdateInstall` 跨帧轮询 | `install` 同步完成，只有每帧安装数量预算 | Sky 需要借鉴 Sakura 的跨帧安装协议 |
| 卸载 | `Unload/Uninstall` 进入 request 状态机 | release queue + dependency release + state cleanup | Sky 可用，但 unload events/diagnostics 还粗 |
| 热重载 | Resource registry 和 request 体系可支撑 | `reload_changed()` 手动触发，依赖闭包 reload | Sky 逻辑不错，但缺 watcher/debounce/自动触发 |
| GPU residency | 有 RAM/VRAM IO 概念，资源安装可贴近 GPU | texture cache 有 GPU upload/residency；其他类型少 | Sky 应保留 backend-owned residency，不把 GPU 放进 asset core |
| 诊断 | request/state/counter 可观测 | AssetEvent 有限；失败/等待/预算缺统一统计 | Sky 需要 request id、状态耗时、队列深度、失败上下文 |
| app 集成 | 系统 update 驱动 request queue | App lifecycle 每帧 update asset/audio/video | Sky 模型更符合当前 engine，继续沿用 |
| Rust 安全 | C++ raw pointer/atomic/counter | Rust typed handle / channels / Result | Sky 不应回退到 pointer 风格 |

## 5. 对 SkyEngine 当前问题的反思

### 5.1 `server.rs` 正在承担太多职责

`src/asset/server.rs` 同时做了 facade、record store、manifest lookup、factory dispatch、background load spawning、load completion、dependency reference、reload closure、release queue、events 和 state transition。短期能迭代很快，但继续加 hot reload、priority、cancel、cross-frame install 后会变成风险点。

需要拆的不是 public API，而是内部职责：

- `store`: record/generation/state/leases。
- `request`: request state machine。
- `io`: bounded background read/decode worker。
- `registry`: manifest/provider lookup。
- `install`: install budget and poll protocol。
- `events`: typed asset event generation。
- `diagnostics`: stats and tracing payload。

### 5.2 后台加载策略不够像引擎

当前背景加载的关键问题不是“有线程”，而是没有资源调度器：

- 每个 asset 一个 OS thread，遇到大量资源时没有 back pressure。
- 没有优先级，camera-near texture 和远处 texture 一视同仁。
- 没有取消，释放 handle 后后台 load 仍会跑完。
- 没有进度，app/editor 只能知道最终完成或失败。
- 没有 worker shutdown 语义，未来 app teardown 可能变复杂。

Sakura 的 IO service 不一定要照搬，但“资源系统不直接 `std::fs::read`，而是提交 request 给 I/O 层”是应该抄的。

### 5.3 `load_blocking` 的语义需要修正

`load_blocking` 现在通过固定次数调用 `update()` 等待 background completion。这类写法的问题是：

- 没有 deadline。
- 没有 sleep/yield。
- 对真实异步 I/O 完成时间不可靠。
- 可能在 app 线程上产生忙等。
- 对依赖链和安装等待的失败上下文不清晰。

应改成显式 blocking path：

- 如果 asset 还没启动，走同步 read/decode，或提交到 worker 后 wait on request。
- 支持 timeout/deadline variant。
- 等待期间持续推进依赖和 install state。
- 返回错误时说明卡在 lookup/read/decode/dependency/install 的哪一步。

### 5.4 install 目前只有数量预算，没有时间/进度协议

`install_budget_per_update` 只能限制每帧完成几个安装，不能拆分单个超重安装。例如音频 decode、未来 mesh upload/material prepare 等都可能在一个 `install()` 调用里卡住。

Sakura 的 `Install -> Installing -> UpdateInstall` 协议值得吸收。SkyEngine 版本可以 Rust 化为：

- 默认 factory 仍可实现同步 install。
- 高风险资源类型可返回 `InstallTask`。
- `InstallTask::poll(&mut InstallContext, budget)` 每帧推进。
- `AssetState::Installing` 记录 task、开始时间、progress、last error context。

这样不会强迫所有 asset 类型都复杂化，但给重资源一条正路。

### 5.5 CPU asset 和 backend residency 的边界要更明确

SkyEngine texture 已经有 `RenderAssetCache`，这是正确方向：asset core 管 CPU 侧身份、bytes、metadata、events，render runtime 管 GPU residency、upload、eviction。

不要把 Sakura 的 VRAM IO 直接变成 `asset` 的 GPU owning 层。应该定义 backend-neutral hooks：

- asset event 告诉后端“某 asset installed/reloaded/unloaded/failed”。
- render/audio/video 后端各自维护 native resource cache。
- asset core 提供 pin/lease/priority metadata，而不直接拥有 GPU/audio/video device 对象。

### 5.6 文档存在代际冲突，需要先统一口径

现有计划文档中有一处需要重新定稿：旧的 asset standard 曾把 `Handle<T>` 定义为弱身份、`AssetRef<T>` 定义为强引用；较新的 smart handle migration 和当前代码已经倾向 `Handle<T>` 为强 handle、`WeakHandle<T>` 为弱身份。

本计划建议选择当前实现方向：

- `Handle<T>`: strong lease，clone 增加 lease，drop 进入 release queue。
- `WeakHandle<T>`: identity only，不保持资源存活，需要通过 `Assets` upgrade/resolve。
- 如果需要直接访问 installed asset，可另设 `AssetRead<T>` / `AssetView<T>` 这类临时 borrow guard，而不要把 `AssetRef<T>` 再作为主概念。

后续应更新旧计划，避免后续实现者照旧文档反向迁移。

## 6. 目标架构

目标不是重写成 Sakura，而是把 SkyEngine 资产系统演进为下面这组内部层：

```text
App / World
  |
  v
Assets facade
  |
  +-- AssetStore
  |     - records
  |     - generations
  |     - leases
  |     - typed asset slots
  |
  +-- AssetRegistry
  |     - manifest lookup
  |     - file/provider resolution
  |     - dependency metadata
  |
  +-- AssetRequestQueue
  |     - request ids
  |     - state machine
  |     - priority/cancel/progress
  |
  +-- AssetIoService
  |     - bounded worker pool
  |     - read/decode jobs
  |     - completion channel
  |
  +-- AssetInstallQueue
  |     - per-frame budget
  |     - install tasks
  |     - dependency waiting
  |
  +-- AssetEvents / AssetDiagnostics
        - loaded/installed/reloaded/unloaded/failed
        - queue depth/state time/failure context

RenderRuntime / Audio / Video / Tools
  |
  v
Backend-owned residency caches
  - texture GPU cache
  - mesh/material GPU cache
  - audio buffer/stream cache
  - video frame queues
```

Public API 目标：

- 继续以 `sky_engine::asset::Assets` 作为主入口。
- 保留 `Handle<T>` / `WeakHandle<T>` 类型安全能力。
- 保留 `AssetRuntimeFactory` 简洁路径，同时扩展 optional async/install task 能力。
- 不要求游戏代码知道 request object、worker pool、registry provider 的内部结构。

## 7. 分阶段执行计划

### Phase 0: 文档和现状对齐

目标：在动代码前，先消除文档漂移，避免越改越乱。

工作项：

1. 对 `docs/plan/asset_resource_system_plan.md`、`docs/plan/asset_resource_system_standard.md`、`docs/plan/asset_smart_handle_migration_plan.md` 做一次只读审计，列出已完成、过时、仍有效的条目。
2. 明确 strong handle 标准：`Handle<T>` 保持资源 lease，`WeakHandle<T>` 仅身份。
3. 明确 `Assets` 是 app-facing facade，不新增全局 `ResourceSystem`。
4. 给未来代码改造建立 issue-style checklist，避免同一问题散落多个计划文件。
5. 为当前 asset 模块补一张实际状态图，和本计划目标状态图对齐。

交付物：

- 更新或追加一份 asset docs reconciliation note。
- 不改变运行时代码。

验收：

- 计划文档之间不再互相矛盾。
- 新实现者能从 docs 判断 `Handle<T>` 到底是不是 strong。

### Phase 1: 修正 correctness 和阻塞语义

目标：先修会导致误判、卡顿或难排查的基础问题。

工作项：

1. 重写 `load_blocking`：
   - 不再用固定 `0..64` 轮询作为完成条件。
   - 增加明确的 blocking read/decode path，或等待 request completion。
   - 增加 timeout/deadline variant，例如 `load_blocking_with_timeout`。
   - 等待期间推进依赖和 install。
   - 错误中带上 asset id、kind、path、current state。
2. 梳理 stale generation 行为：
   - dropped handle release 到达时，如果 record generation 已变化，必须 no-op。
   - reload 后旧 weak handle resolve 必须按既定语义处理。
3. 强化 `AssetEvent`：
   - 区分 loaded/installed/reloaded/unloaded/failed。
   - failure event 带 phase：lookup/read/decode/dependency/install。
4. 为 dependency wait 添加诊断：
   - 依赖缺失、依赖失败、循环依赖、仍在等待应能区分。
5. 补测试：
   - blocking load 成功。
   - blocking load 失败。
   - blocking load 依赖链。
   - release stale generation 不误删新资源。
   - dependency failure 正确传播。

建议涉及文件：

- `src/asset/server.rs`
- `src/asset/types.rs`
- `src/asset/registry.rs`
- `src/asset/mod.rs`

验收命令：

```powershell
cargo test asset::
cargo test
```

### Phase 2: 引入 bounded Asset I/O service

目标：把每资源 `std::thread::spawn` 改成统一、有 back pressure 的 I/O 层。

工作项：

1. 新增 `src/asset/io.rs`。
2. 定义内部类型：
   - `AssetIoService`
   - `AssetIoRequest`
   - `AssetIoResponse`
   - `AssetIoPriority`
   - `AssetIoCancelToken`
3. worker pool 配置进入 `AssetConfig`：
   - `io_worker_threads`
   - `io_queue_capacity`
   - `default_priority`
   - `shutdown_timeout`
4. `AssetsInner` 不再直接 `std::thread::spawn`。
5. worker job 初期只做 file read + factory load，先不拆 async serde。
6. 支持取消：
   - handle 释放且无 lease 时，可标记未开始 request canceled。
   - 已开始的 file read 可允许跑完，但 completion 到达时丢弃。
7. 支持优先级：
   - 初期可先排序 pending queue。
   - 后续由 render/camera/tooling 提供 priority hints。
8. 支持 app shutdown：
   - drop `Assets` 时关闭 worker sender。
   - join workers 或明确 detach 策略。

建议涉及文件：

- `src/asset/io.rs` 新增
- `src/asset/server.rs`
- `src/asset/types.rs`
- `src/app/services.rs`

验收：

- 大量 asset 请求不会创建大量 OS threads。
- worker shutdown 测试稳定。
- 释放未开始资源后不会继续安装。
- 背景加载行为和现有 public API 兼容。

验收命令：

```powershell
cargo test asset::
cargo test --features app asset::
```

### Phase 3: 把资源请求状态机对象化

目标：借鉴 Sakura `ResourceRequest::Update()` 的结构，把状态迁移从 `server.rs` 巨型流程中拆出来。

工作项：

1. 新增 `src/asset/request.rs`。
2. 定义内部 `AssetRequest`：
   - request id
   - asset id
   - generation
   - current phase
   - priority
   - dependency handles
   - started/completed timestamps
   - last progress
   - cancel flag
3. 定义 `AssetRequestPhase`，和 `AssetState` 对齐但不完全等价：
   - `Queued`
   - `Loading`
   - `Decoding`
   - `WaitingDependencies`
   - `ReadyToInstall`
   - `Installing`
   - `Installed`
   - `Unloading`
   - `Failed`
   - `Canceled`
4. `AssetState` 保持 public/debug-facing 简化状态。
5. `AssetsInner::update()` 改成：
   - drain releases
   - submit new requests
   - poll I/O completions
   - advance requests
   - apply install budget
   - emit events
6. request transition 必须集中写测试，不依赖真实文件系统。

验收：

- `server.rs` 复杂度下降。
- 每个 request 可以解释自己为什么等待。
- 后续 install task 和 diagnostics 有落点。

验收命令：

```powershell
cargo test asset::request
cargo test asset::
```

### Phase 4: 引入跨帧 install task

目标：补上 Sakura `Install -> Installing -> UpdateInstall` 的能力，但保持 Rust API 简洁。

建议 API：

```rust
pub enum AssetInstallResult<T> {
    Ready(T),
    Pending(Box<dyn AssetInstallTask<Output = T> + Send>),
}

pub trait AssetInstallTask {
    type Output;

    fn poll_install(
        &mut self,
        ctx: &mut AssetInstallContext<'_>,
        budget: AssetInstallBudget,
    ) -> AssetInstallPoll<Self::Output>;
}
```

实际落地可以更保守：

- 第一版不必让 trait object 暴露到 public API。
- 先在内部支持 `InstallTask`，同步 factory 自动包装成 immediate ready。
- 资源类型需要时再 opt-in。

工作项：

1. 将 `AssetRuntimeFactory` 收敛到跨帧安装模型：
   - 使用 `begin_install(...) -> AssetInstallResult<Asset>` 作为唯一 runtime install 入口。
   - 同步安装返回 `AssetInstallResult::Ready(asset)`。
   - 跨帧安装返回 `AssetInstallResult::Pending(task)`。
   - 不保留旧 erased 同步 `install(...)` 兼容路径。
2. `AssetState::Installing` 持有 install task。
3. `AssetConfig` 增加时间预算：
   - `install_budget_per_update` 保留。
   - 新增 `install_time_budget`。
4. `update()` 每帧 poll installing tasks。
5. 首批迁移高风险类型：
   - audio decode/install。
   - large texture CPU preparation。
   - 后续 mesh/material/gltf。

验收：

- 一个超重 asset install 不会独占整帧。
- 现有简单 asset factory 不需要改代码。
- installing 状态可被 diagnostics 观测。

验收命令：

```powershell
cargo test asset::
cargo test --features app
```

### Phase 5: Registry / Provider / VFS 层

目标：吸收 Sakura registry 的好处，让资源定位不再只等于本地 manifest + path。

工作项：

1. 新增或重构 `AssetRegistry` trait：
   - `resolve(asset_id) -> AssetLocation`
   - `read_metadata(asset_id) -> AssetMetadata`
   - `dependencies(asset_id) -> Vec<AssetId>`
   - `watch_key(asset_id) -> Option<WatchKey>`
2. 当前 manifest registry 作为 `LocalManifestRegistry`。
3. 支持多 provider：
   - local file provider。
   - cooked package provider。
   - memory/test provider。
4. `AssetIoService` 只接受 resolved location，不做 manifest lookup。
5. cook manifest 和 runtime manifest 字段统一命名。
6. 测试用 memory provider 替代临时文件，减少 asset 单测对 filesystem 的依赖。

验收：

- runtime load 不关心资产来自 loose file 还是 package。
- tests 可以用 memory provider 构造失败/延迟/依赖场景。
- hot reload 能从 registry 拿到 watch key。

验收命令：

```powershell
cargo test asset::registry
cargo test asset::
```

### Phase 6: 后端 residency 标准化

目标：不把 GPU/audio/video native resource 放进 asset core，但建立统一约定，让后端缓存可观测、可预算、可响应 reload。

工作项：

1. 将 texture cache 的经验整理为 `BackendResidencyCache` 设计约定，而不是立即抽 trait。
2. 为 texture cache 补齐：
   - memory budget。
   - LRU/priority eviction。
   - pinning。
   - reload generation check。
   - upload failure event。
3. 资产事件携带 enough context：
   - asset id。
   - generation。
   - kind/type id。
   - dependency version/reload marker。
4. 后续新增：
   - mesh GPU cache。
   - material/pipeline cache。
   - audio buffer/stream cache。
   - video frame/source cache。
5. 保持 ownership：
   - `Assets`: CPU asset identity and lifecycle。
   - `RenderRuntime`: GPU texture/mesh/material residency。
   - `AudioServer`: decoded/streaming audio residency。
   - `VideoServer`: video source/frame residency。

验收：

- texture reload 后不会使用旧 generation GPU resource。
- residency eviction 不影响 strong asset handle 的 CPU 语义。
- render/audio/video backend 可以独立测试。

验收命令：

```powershell
cargo test --features app render::runtime::tests
cargo test --features app
```

### Phase 7: Hot reload 自动化和诊断

目标：把 `reload_changed()` 从手动工具函数升级为 editor/dev 可依赖的服务。

工作项：

1. 增加可选 file watcher：
   - feature gated 或 config controlled。
   - debounce。
   - batch changes per frame。
2. reload 过程输出解释：
   - changed asset。
   - affected dependent closure。
   - skipped reason。
   - failed phase。
3. diagnostics 增加：
   - queue depth。
   - active request count。
   - per-state count。
   - average wait/read/decode/install time。
   - failed request list。
4. app diagnostics 集成：
   - `src/diagnostics/` event。
   - console output 可选择展示 asset stats。
5. editor-facing API：
   - query reload status。
   - force reload asset。
   - freeze reload during edit transaction。

验收：

- 修改 loose file 后，dev config 下自动 reload。
- 依赖资源会按闭包 reload。
- 失败不会让旧 installed asset 立即消失，除非策略明确要求。

验收命令：

```powershell
cargo test asset::
cargo test --features app
```

### Phase 8: Cooking 扩展点和 runtime factory 对齐

目标：把 cooked asset pipeline 从 hard-coded kind 分支升级为可扩展注册。

工作项：

1. 定义 cooker registry：
   - runtime asset kind。
   - source extensions。
   - output cooked kind/version。
   - dependency extraction。
   - incremental hash。
2. 让 runtime `AssetRuntimeFactory` 和 cooker metadata 对齐：
   - type id。
   - kind string。
   - version。
   - dependency schema。
3. 逐步迁移：
   - texture。
   - font。
   - audio。
   - mesh/material/gltf。
4. cook manifest 记录：
   - source hash。
   - cooker version。
   - dependency hash。
   - platform/profile。
5. verify 不再只检查固定 kind，而是询问 registered cooker。

验收：

- 新 asset kind 不需要改 cook 中央 match。
- cook verify 能发现 cooker version drift。
- runtime load error 能指出 cooked schema mismatch。

验收命令：

```powershell
cargo test asset::cook
cargo test asset::
```

### Phase 9: API 收口和文档清理

目标：避免实现完成后留下多套相互竞争的名词。

工作项：

1. 更新 `README.md` / `README_zh.md` 的 asset quick-start。
2. 更新 `docs/` 中 asset 标准文档：
   - `Assets`
   - `Handle<T>`
   - `WeakHandle<T>`
   - factory。
   - cooker。
   - hot reload。
3. 删除或标记过期的执行计划段落。
4. 给 examples 增加最小覆盖：
   - load texture。
   - hot reload texture。
   - load asset with dependency。
   - custom asset factory。
5. 对 app/render example 做 compatibility check。

验收命令：

```powershell
cargo test
cargo test --features app
cargo check --examples --features app
```

## 8. 最小可行路线

如果不想一次铺太大，建议先走这条 MVP：

1. Phase 1: 修 `load_blocking`、错误语义、dependency failure。
2. Phase 2: 引入 bounded I/O service，替换 thread-per-load。
3. Phase 3: request state machine 对象化。
4. Phase 4: install task 协议，但只迁移一个真实重资源类型。
5. Phase 7: 基础 diagnostics。

这五步完成后，SkyEngine 的资源系统就会从“可用模块”跃迁到“引擎级调度层”。Phase 5/6/8 再扩 registry、residency、cooking，风险会低很多。

## 9. 具体代码落点建议

新增文件：

- `src/asset/io.rs`: bounded worker pool、I/O request/response、priority/cancel。
- `src/asset/request.rs`: request state machine、phase transition、progress。
- `src/asset/install.rs`: install task、budget、poll result。
- `src/asset/store.rs`: record/generation/lease 管理，逐步从 `server.rs` 拆出。
- `src/asset/events.rs`: AssetEvent 和 failure phase 细化。
- `src/asset/diagnostics.rs`: stats snapshot、state time、queue depth。
- `src/asset/provider.rs`: registry/provider/VFS abstraction。

优先修改：

- `src/asset/server.rs`: 从 all-in-one orchestrator 逐步变为 coordinator。
- `src/asset/types.rs`: config、state、error、handle semantics。
- `src/asset/registry.rs`: manifest registry -> provider-backed registry。
- `src/asset/cook.rs`: cooker registry 化。
- `src/render/resources/texture_cache.rs`: residency diagnostics/budget/reload generation。
- `src/app/services.rs`: asset update stats surfaced to app diagnostics。
- `src/audio/assets.rs`: 作为跨帧 install 首批试点。

不建议优先修改：

- `src/main.rs`: scratch/local playground，不作为 API 方向。
- ECS query/chunk 热路径：资源计划不应牵连 ECS hot path。
- RenderGraph internals：除非 texture residency 需要 asset event 接口，否则不要动 graph compilation/alias/reorder。

## 10. 测试矩阵

### Unit tests

- handle strong lease clone/drop。
- weak handle resolve/failed resolve。
- release queue stale generation。
- request phase transition。
- dependency success/failure/cycle。
- blocking load timeout。
- background worker cancellation。
- install task polling。
- reload changed dependent closure。
- registry provider memory/local lookup。
- cook registry version mismatch。

### Integration tests

- app lifecycle 每帧 update assets。
- texture asset installed 后 render cache upload。
- texture reload 后 render cache invalidation。
- audio asset load/install 不阻塞 frame。
- hot reload debounce。

### Stress tests

- 1000 small assets background load，不创建 1000 OS threads。
- 大依赖图 load/reload。
- load 和 release 同帧交错。
- load failure + reload success。
- install task 超预算多帧完成。

### Commands

```powershell
cargo test asset::
cargo test
cargo test --features app
cargo test --features app render::runtime::tests
cargo check --examples --features app
```

如果改到 UI/neo/yakui/vn/audio/video feature，再按 AGENTS.md 中对应命令补跑。

## 11. 设计验收标准

完成后应满足：

- Public API 仍然以 `sky_engine::asset::Assets` 为主入口。
- `Handle<T>` strong、`WeakHandle<T>` weak 的语义清晰且有测试。
- 同时请求大量资源时线程数量有上限。
- asset load 可以被取消或至少 completion 被安全丢弃。
- 单个重 install 可以跨帧推进。
- asset failure 能说明失败阶段和具体资源。
- dependency wait 可诊断，不再只是“没装好”。
- hot reload 可以自动触发，并能解释 reload closure。
- render/audio/video native residency 不在 asset core 中乱耦合。
- texture cache 至少有 generation-safe reload。
- cooking pipeline 能注册新 asset kind，不需要持续改中心 match。
- docs 不再出现 `Handle<T>` strong/weak 两套说法。

## 12. 风险和应对

| 风险 | 表现 | 应对 |
| --- | --- | --- |
| 过度抽象 | 为了像 Sakura 引入太多 trait/service，简单资源也变复杂 | public API 保持 `Assets`，复杂度放内部 optional path |
| 文档漂移 | 新计划和旧计划互相打架 | Phase 0 先做 reconciliation |
| handle 语义反复 | `Handle<T>` strong/weak 来回摇摆 | 选择当前实现方向：strong `Handle<T>` + weak `WeakHandle<T>` |
| worker 生命周期 bug | app shutdown 卡住或后台线程访问已释放状态 | worker pool 明确 drop/join/cancel policy |
| install task 泛型复杂 | trait object + associated type 难落地 | 使用统一 `begin_install -> AssetInstallResult`，同步 factory 显式返回 `Ready` |
| 后端耦合 | asset core 开始拥有 GPU/audio/video device | residency cache 归 backend，asset 只发事件和 metadata |
| 热重载破坏旧资源 | reload 失败导致可用资源消失 | 默认保留 last good installed asset |
| 锁竞争 | asset update 每帧锁太多 | request queue 单 owner update，worker 只通过 channel 交付 completion |
| 测试依赖文件系统 | 单测慢且 flaky | memory provider + fake IO service |
| 兼容性破坏 | examples / user code 编译失败 | 用户已接受不兼容改动；以更干净的 runtime factory API 为准，并跑 examples check |

## 13. Sakura 可直接借鉴的点

### 13.1 状态机命名和阶段

SkyEngine 已经有类似状态，可以保留并补全 request-level phase。建议直接采用 Sakura 的大阶段：

- unloaded。
- loading。
- loaded/decoded。
- waiting dependencies。
- installing。
- installed。
- uninstalling/unloading。
- error/failed。

但 Rust 代码里应区分：

- `AssetState`: record 的外部可见状态。
- `AssetRequestPhase`: request 的内部详细状态。

### 13.2 Factory 生命周期拆分

Sakura 的 factory 把加载、反序列化、安装、卸载拆开，这一点应借鉴。SkyEngine 可以演化为：

- `load`: bytes/source -> loaded CPU intermediate。
- `dependencies`: loaded intermediate -> dependency handles。
- `begin_install`: loaded + installed dependencies -> installed asset 或 install task。
- `poll_install`: 跨帧推进。
- `uninstall`: 释放 installed asset 的 runtime-side hooks。

### 13.3 Request queue 作为诊断中心

Sakura 的 request object 使每个资源请求可以单独 update、wait、计数。SkyEngine 可以借鉴为 diagnostics：

- request id。
- asset id。
- state enter time。
- current phase。
- wait reason。
- dependency blockers。
- last error。

### 13.4 I/O 和 resource system 分离

Sakura 的 IO service 不是必须照抄，但“resource system 提交 I/O request，不直接文件读取”的边界应采用。这样未来能自然接入：

- package file。
- memory provider。
- editor virtual file。
- network/download cache。
- platform-specific fast path。

## 14. Sakura 不成熟之处的警示

这次参考也暴露了 Sakura snapshot 自身的问题：

- async serde 代码路径被硬关，说明设计有但落地未完全稳定。
- local registry cancel 是空实现，说明取消语义不能只靠接口存在。
- `FlushResource` 未实现，说明 unload/flush 边界是难点。
- C++ raw pointer handle 需要非常谨慎的 lifetime 管理，Rust 不应复制。
- IO service 文档强，但具体平台能力和 engine runtime 绑定较深，不能直接移植。

对 SkyEngine 的启发是：每个新增接口都必须有一个真实资源类型和测试来证明，而不是先铺完整宏伟架构。

## 15. 建议第一批 PR 切分

### PR 1: Asset docs reconciliation

- 更新 asset 相关计划文档。
- 固化 `Handle<T>` strong 语义。
- 增加当前/目标状态图。
- 不改 runtime。

### PR 2: Blocking load and error diagnostics

- 修正 `load_blocking`。
- 增加 failure phase。
- 补 dependency failure tests。

### PR 3: Bounded I/O service

- 新增 `asset::io` 内部模块。
- 替换 `std::thread::spawn` per asset。
- 增加 cancellation/drop tests。

### PR 4: Request state machine

- 新增 `asset::request`。
- 从 `server.rs` 移出状态迁移。
- 保持 public API 不变。

### PR 5: Cross-frame install

- 新增 install task 协议。
- 同步 factory 显式返回 `AssetInstallResult::Ready`。
- 迁移一个重资源作为示例。

### PR 6: Diagnostics and hot reload

- 增加 asset stats。
- 可选 watcher。
- reload explanation events。

### PR 7: Registry/provider and cooker registry

- provider abstraction。
- memory provider tests。
- cooker registry。

## 16. 最终判断

SkyEngine 不需要“照抄 SakuraEngine 的资源系统”，因为 SkyEngine 已经有更符合 Rust、ECS app lifecycle 和 typed handle 的基础。但 SkyEngine 应该认真抄 Sakura 的三件事：

1. 资源请求必须成为显式状态机，而不是散在 `server.rs` 的流程判断。
2. 加载、依赖等待、安装、卸载必须是可预算、可诊断、可跨帧推进的生命周期。
3. I/O/registry/backend residency 必须分层，asset core 只做身份、状态、依赖、事件和 CPU 侧生命周期。

按照本计划推进，SkyEngine 的资源系统可以保持当前 API 亲和力，同时补上引擎级资源调度能力。这比直接移植 Sakura 更稳，也更符合 SkyEngine 现有架构。
