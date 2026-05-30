# SkyEngine Audio

`sky_engine::audio` 是可选音频运行时，依赖 `asset`，当前后端使用 `kira`。它提供 audio asset、`AudioServer`、command queue、bus、2D emitter/listener 组件。

```toml
sky_engine = { version = "...", features = ["audio"] }
```

窗口 demo 通常需要：

```bash
cargo run --example audio_demo --features "app audio"
```

导出：

```rust
use sky_engine::audio::{
    AudioBusId, AudioCommands, AudioConfig, AudioEmitter2D, AudioEmitterAsset, AudioError,
    AudioInstanceId, AudioListener2D, AudioPlaybackSettings, AudioServer, AudioServerStats,
    AudioSpatialSettings, AudioTween, MusicTrack, SoundClip,
};
```

## Assets

内置 audio asset：

- `SoundClip`
- `MusicTrack`

注册 factory：

```rust,no_run
use sky_engine::audio::register_audio_asset_factories;

register_audio_asset_factories(&asset_server);
```

然后可以通过 `Assets` 加载：

```rust,no_run
let jump = asset_server.load::<SoundClip>("audio/jump.ogg")?;
let music = asset_server.load::<MusicTrack>("audio/theme.ogg")?;
# Ok::<(), sky_engine::asset::AssetError>(())
```

## AudioConfig

```rust,no_run
let config = AudioConfig::default()
    .with_bus("music")
    .with_bus("sfx");
```

Bus 用名字注册，运行时通过 `AudioServer::bus_id(name)` 获取。

## AudioServer

构造：

```rust,no_run
let audio = AudioServer::new(AudioConfig::default(), asset_server);
world.insert_resource(audio);
```

状态：

```rust,no_run
audio.is_available() -> bool
audio.disabled_reason() -> Option<String>
audio.bus_id("music") -> Option<AudioBusId>
audio.stats() -> AudioServerStats
```

`AudioServerStats` 是 audio 后端本地快照，只报告 backend available 状态、不可用原因、配置 bus 数、后端 live instance 数、spatial instance 数、直接播放实例数、ECS emitter 绑定数、累计播放启动失败数和最后一次播放失败摘要。它不进入 `AssetStats`，也不让 asset core 拥有 audio residency。`app` 服务会在 stats 变化时发布 `audio.stats` 结构化诊断事件；当 backend 进入不可用状态时，另发 `audio.backend.unavailable` warning 事件，包含不可用原因、bus 数和当前 audio-side instance/binding 计数；当播放启动失败计数增加时，另发 `audio.play.failed` warning 事件，包含失败增量、累计值和最后失败摘要。

播放：

```rust,no_run
let id = audio.play_sound(clip_handle, AudioPlaybackSettings::default())?;
let id = audio.play_music(music_handle, AudioPlaybackSettings::default().looped(true))?;
# Ok::<(), sky_engine::audio::AudioError>(())
```

控制：

```rust,no_run
audio.stop(id, AudioTween::default())?;
audio.pause(id, AudioTween::default())?;
audio.resume(id, AudioTween::default())?;
audio.set_gain(id, 0.5, AudioTween::default())?;
audio.set_pitch(id, 1.2, AudioTween::default())?;
audio.set_pan(id, -0.25, AudioTween::default())?;
audio.set_bus_gain(bus, 0.8, AudioTween::default())?;
# Ok::<(), sky_engine::audio::AudioError>(())
```

每帧：

```rust,no_run
audio.update();
audio.apply_commands()?;
# Ok::<(), sky_engine::audio::AudioError>(())
```

## Commands

`AudioCommands` 是可 clone 的 command handle，适合系统里排队音频操作：

```rust,no_run
let commands = audio.commands();
commands.play_sound(handle, AudioPlaybackSettings::default());
commands.set_bus_gain(bus, 0.4, AudioTween::default());
```

随后调用：

```rust,no_run
audio.apply_commands()?;
# Ok::<(), sky_engine::audio::AudioError>(())
```

## Playback Settings

```rust,no_run
AudioPlaybackSettings::default()
    .on_bus(bus)
    .gain(0.8)
    .pitch(1.0)
    .pan(0.0)
    .looped(false)
```

Spatial：

```rust,no_run
let spatial = AudioSpatialSettings::default().with_position(x, y);
let settings = AudioPlaybackSettings::default().spatial(spatial);
```

Tween：

```rust,no_run
use std::time::Duration;

let tween = AudioTween::new(Duration::from_millis(250));
```

## ECS Components

组件：

- `AudioEmitter2D`
- `AudioListener2D`

Emitter：

```rust,no_run
world.spawn((
    Transform::from_xy(0.0, 0.0),
    AudioEmitter2D::sound(jump_handle),
));

world.spawn((
    Transform::from_xy(0.0, 0.0),
    AudioEmitter2D::music(music_handle),
));
```

Listener：

```rust,no_run
world.spawn((
    Transform::from_xy(0.0, 0.0),
    AudioListener2D::default(),
));
```

`audio + app` 下存在 ECS sync 支持，用于把 emitter/listener 的 transform 同步到 audio server。

## 失败语义

Audio backend 不可用时，`AudioServer::is_available()` 为 false，`disabled_reason()` 会给出原因。应用可以继续运行，只是音频操作会返回 `AudioError` 或被安全忽略，具体行为以当前实现为准。

## 测试和检查

```bash
cargo check --features audio
cargo check --example audio_demo --features "app audio"
```
