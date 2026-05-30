# SkyEngine Video

`sky_engine::video` 是可选视频运行时。它现在提供三层能力：

- `VideoClip`：基于 `TextureAsset` 的帧序列，适合可预烘焙的 Galgame 动态背景和短过场。
- `VideoFrameQueue` / `VideoClock` / `DecodedVideoFrame`：解码后端共用的 PTS 队列、丢帧策略和播放时钟。
- `GpuVideoFrameBuffer`：真正的 GPU 热路径。它持有一张稳定 `wgpu` texture，并用 `queue.write_texture()` 原地更新。
- `FfmpegVideoPlayer`：`video-ffmpeg` feature 下的真实 MP4/容器解码播放器，后台线程用 FFmpeg demux/decode/sws_scale，主线程按 PTS 上传最新到期帧。

真实 `.mp4` 解码后端启用独立 feature，避免没有系统 FFmpeg 的机器构建普通游戏时被拖住：

```toml
sky_engine = { version = "...", features = ["video-ffmpeg"] }
```

`video-ffmpeg` 依赖本机 FFmpeg 开发库；默认 `video` feature 不包含它。

Windows 上通常需要先通过 vcpkg 安装 FFmpeg 开发包，或提供可被 `pkg-config` 发现的 FFmpeg 库；否则 `ffmpeg-sys-next` 会在 build script 阶段失败。这个约束来自 FFmpeg FFI 链接层，不影响默认 `video` feature。

```toml
sky_engine = { version = "...", features = ["video"] }
```

`video` 会启用 `app` 和 `asset`，窗口应用会自动插入：

```rust,no_run
VideoServer
VideoCommands
```

## Assets

核心 asset：

- `VideoClip`
- `VideoFrame`
- `DecodedVideoFrame`
- `VideoFrameQueue`
- `VideoClock`
- `GpuVideoFrameBuffer`
- `FfmpegVideoPlayer`（`video-ffmpeg`）

`VideoClip` 由一组 `Handle<TextureAsset>` 帧和每帧时长组成：

```rust,no_run
use sky_engine::video::VideoClip;

let clip = VideoClip::from_textures(1280, 720, 24.0, frame_textures)?;
let clip_handle = asset_server.insert_runtime(clip);
# Ok::<(), sky_engine::video::VideoError>(())
```

也可以注册视频 asset factory，从 cooked JSON 描述加载：

```rust,no_run
use sky_engine::video::register_video_asset_factories;

register_video_asset_factories(&asset_server);
```

Cooked JSON 形状：

```json
{
  "width": 1280,
  "height": 720,
  "frames": [
    { "texture": "00000000-0000-0000-0000-000000000000", "duration_ms": 41.6667 }
  ]
}
```

推荐的源码文件扩展名是 `.skyvideo`，asset cooker 会把帧里的相对路径解析为 texture asset id，并写入依赖：

```json
{
  "width": 1280,
  "height": 720,
  "fps": 24,
  "frames": [
    "opening/0001.png",
    { "texture": "opening/0002.png", "duration_ms": 41.6667 }
  ]
}
```

路径默认相对 `.skyvideo` 文件所在目录；找不到时再按 asset root 解析。

## Playback

```rust,no_run
use sky_engine::video::{VideoPlaybackSettings, VideoServer};

let video = VideoServer::new(asset_server.clone());
let instance = video.play(
    clip_handle,
    VideoPlaybackSettings::default().looped(true),
)?;

video.update(dt);
let frame = video.current_frame(instance);
# Ok::<(), sky_engine::video::VideoError>(())
```

`video.stats()` 返回 `VideoServerStats`，包含播放实例数、Playing/Paused/Finished/Stopped 分布、当前引用的 clip 数、当前帧 texture 数、当前帧 texture 字节数、累计播放启动失败数和最后一次播放失败摘要。这个快照只描述 `VideoServer` 自己的 playback/frame residency，不写入 asset core 的 `AssetStats`。`app` 服务会在 stats 变化时发布 `video.stats` 结构化诊断事件；当播放启动失败计数增加时，另发 `video.play.failed` warning 事件，包含失败增量、累计值和最后失败摘要。当当前帧 texture 字节数变为非零或尺寸级别发生变化时，另发 `video.frame.resident` info 事件，包含当前帧 texture 字节数、texture 数、实例数、播放中实例数和 clip 数。

## Streaming Frames

原型或 ECS sprite 路径可以写入稳定的 `VideoFrameBuffer`，然后把它的 texture handle 交给 `SpriteRenderer`：

```rust,no_run
use sky_engine::asset::TextureColorSpace;
use sky_engine::video::VideoFrameBuffer;

let buffer = VideoFrameBuffer::new(&asset_server, 1280, 720, TextureColorSpace::Srgb)?;
sprite.texture = Some(buffer.handle());

// 每当解码出一帧 RGBA8：
buffer.write_rgba8(&asset_server, rgba_pixels)?;
# Ok::<(), sky_engine::video::VideoError>(())
```

高性能路径应直接写入 `GpuVideoFrameBuffer`，避免每帧经过 AssetServer 或重新创建 GPU texture：

```rust,no_run
use sky_engine::video::GpuVideoFrameBuffer;

let frame = GpuVideoFrameBuffer::new(
    gpu,
    1920,
    1080,
    wgpu::TextureFormat::Rgba8UnormSrgb,
)?;

// 每当解码出一帧 RGBA8：
frame.write_rgba8(gpu, rgba_pixels)?;
let resident_bytes = frame.resident_bytes();
batch.set_texture(frame.texture());
# Ok::<(), sky_engine::video::VideoError>(())
```

`GpuVideoFrameBuffer::resident_bytes()` 只报告这个 GPU frame buffer 自己的 texture 字节估算；它不经过 `AssetStats`，也不代表全局视频内存预算。

`examples/video_demo.rs` 使用的就是这条 GPU 稳定纹理路径。

## MP4 / FFmpeg

`video-ffmpeg` 提供真实视频文件播放：

```rust,no_run
use sky_engine::video::{FfmpegVideoOptions, FfmpegVideoPlayer};

let mut player = FfmpegVideoPlayer::open_with_options(
    gpu,
    "opening.mp4",
    FfmpegVideoOptions::default().looped(true),
)?;

// 每帧：
let update = player.update(gpu, dt)?;
batch.set_texture(player.texture());
# Ok::<(), sky_engine::video::VideoError>(())
```

这条路径的结构是：

1. 解码线程读取容器 packet，选择最佳 video stream。
2. FFmpeg 解码到原始视频帧，当前基线用 `swscale` 转为 RGBA8。
3. 解码帧进入有界 channel，再进入 `VideoFrameQueue`。
4. 主线程根据 `VideoClock` 的播放时间取 `PTS <= clock` 的最新帧，丢弃更旧的 late frames。
5. 选中的帧用 `GpuVideoFrameBuffer::write_rgba8()` 原地上传，不重建 texture，不经过 AssetServer。

示例：

```bash
cargo run --example mp4_video_demo --features video-ffmpeg --release -- path/to/video.mp4
```

当前 FFmpeg 后端是跨平台软件解码基线，已经支持 MP4/H.264 等由本机 FFmpeg 支持的格式。后续更高阶路径应在这个接口下继续扩展：NV12/YUV 平面直接上传、shader YUV->RGB、以及 Media Foundation / VideoToolbox / VAAPI / DXVA 等硬件解码后端。

## ECS Sprite Sync

`VideoPlayer2D` 可以挂在带 `SpriteRenderer` 的实体上。App runner 每帧会在渲染前推进视频，并把当前帧写到 `SpriteRenderer.texture`：

```rust,no_run
use sky_engine::render::SpriteRenderer;
use sky_engine::video::{VideoPlaybackSettings, VideoPlayer2D};

world.spawn((
    Transform::default(),
    SpriteRenderer::new(640.0, 360.0),
    VideoPlayer2D::new(clip_handle)
        .with_settings(VideoPlaybackSettings::default().looped(true)),
));
```

## 检查

```bash
cargo check --features video
cargo check --example video_demo --features video
cargo test --features video video::
cargo test --features asset asset::cook

# 需要本机 FFmpeg 开发库
cargo check --features video-ffmpeg
cargo check --example mp4_video_demo --features video-ffmpeg
```
