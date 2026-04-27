# SkyEngine Diagnostics

`sky_engine::diagnostics` 提供结构化诊断事件。它用于引擎内部 warning/error，也适合游戏和工具层上报可读、可过滤、可去重的问题。

导出：

```rust
use sky_engine::diagnostics::{
    write_diagnostic_events, DiagnosticConsole, DiagnosticCursor, DiagnosticEvent,
    DiagnosticField, DiagnosticId, DiagnosticKey, DiagnosticSeverity, DiagnosticSubsystem,
    Diagnostics, EngineDiagnosticKind,
};
```

## DiagnosticEvent

构造：

```rust
DiagnosticEvent::info(id, subsystem, message)
DiagnosticEvent::warning(id, subsystem, message)
DiagnosticEvent::error(id, subsystem, message)
```

示例：

```rust
use sky_engine::diagnostics::{
    DiagnosticEvent, DiagnosticSubsystem, Diagnostics,
};

let diagnostics = Diagnostics::new();

diagnostics.report(
    DiagnosticEvent::warning(
        "render.texture.missing",
        DiagnosticSubsystem::render(),
        "A sprite referenced a missing texture.",
    )
    .with_title("Texture is missing")
    .with_help("Check the asset manifest and texture handle.")
    .with_field("asset", "player.png"),
);
```

可附加信息：

```rust
event.with_entity(entity)
event.with_frame(frame)
event.with_title(title)
event.with_help(help)
event.with_field(name, value)
event.with_once_key(key)
event.field(name)
```

## Severity / Subsystem

Severity：

- `Info`
- `Warning`
- `Error`

Subsystem helpers：

```rust
DiagnosticSubsystem::engine()
DiagnosticSubsystem::ecs()
DiagnosticSubsystem::render()
DiagnosticSubsystem::asset()
DiagnosticSubsystem::gpu()
DiagnosticSubsystem::app()
DiagnosticSubsystem::input()
DiagnosticSubsystem::audio()
DiagnosticSubsystem::live2d()
```

也可以自定义：

```rust
DiagnosticSubsystem::new("game.ai")
```

## Diagnostics Store

`Diagnostics` 是 ring-buffer 风格 store。

```rust
let diagnostics = Diagnostics::new();
let diagnostics = Diagnostics::with_capacity(1024);
```

写入：

```rust
diagnostics.report(event);
diagnostics.report_once(event);
```

读取：

```rust
diagnostics.entries() -> Vec<DiagnosticEvent>
diagnostics.cursor() -> DiagnosticCursor
diagnostics.events_since(&mut cursor) -> Vec<DiagnosticEvent>
diagnostics.missed_since(cursor) -> u64
diagnostics.dropped_count() -> u64
diagnostics.capacity() -> usize
diagnostics.len() -> usize
diagnostics.is_empty() -> bool
diagnostics.clear()
```

ECS resource：

```rust
Diagnostics::resource(&mut world).report(event);
```

如果 world 里没有 `Diagnostics` resource，这个 helper 会插入一个。

## Once 去重

`report_once` 依赖 `DiagnosticKey`。如果 event 没设置 once key，通常会使用事件 id 相关逻辑，具体以当前实现为准。推荐显式设置：

```rust
diagnostics.report_once(
    DiagnosticEvent::warning(
        "live2d.parameter.missing",
        DiagnosticSubsystem::live2d(),
        "A Live2D parameter does not exist.",
    )
    .with_field("parameter", "ParamAngleX")
    .with_once_key("live2d.parameter.missing:ParamAngleX"),
);
```

适合：

- 缺失 asset
- shader fallback
- 参数名拼错
- 某实体只需要报告一次的配置问题

## Console 输出

`DiagnosticConsole`：

- `Off`
- `WarningsAndErrors`
- `All`

写出：

```rust
let written = write_diagnostic_events(
    &mut std::io::stderr(),
    events.iter(),
    DiagnosticConsole::WarningsAndErrors,
)?;
# Ok::<(), std::io::Error>(())
```

App runner 会根据 `AppConfig::diagnostic_console` 把新 diagnostics 镜像到 stderr。结构化事件仍然保存在 `Diagnostics` resource 中。

## EngineDiagnosticKind

`EngineDiagnosticKind` 是引擎内置诊断枚举，可转换成 `DiagnosticEvent`。例如 camera / render pipeline 常见配置问题会走这条路径。

```rust
diagnostics.report_once(EngineDiagnosticKind::CameraMissingProjection { entity });
```

## 设计建议

推荐：

- 用稳定 id：`render.texture.missing`、`asset.manifest.invalid`。
- 用 subsystem 表明来源。
- message 写事实，title 写用户可扫读摘要，help 写下一步。
- 对反复出现的问题使用 `with_once_key`。
- 在 UI/editor 中用 cursor 增量读取。

不推荐：

- 把 diagnostics 当日志 spam。
- 在热循环里无节制创建大量 string。
- 用 panic 处理可恢复配置问题。

## 测试

```bash
cargo test diagnostics
```
