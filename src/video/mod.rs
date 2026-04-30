mod assets;
mod commands;
#[cfg(feature = "video-ffmpeg")]
mod ffmpeg;
mod playback;
mod server;
mod streaming;
mod types;

pub use assets::{register_video_asset_factories, VideoClip, VideoFrame};
pub use commands::VideoCommands;
#[cfg(feature = "video-ffmpeg")]
pub use ffmpeg::{FfmpegVideoMetadata, FfmpegVideoOptions, FfmpegVideoPlayer, FfmpegVideoUpdate};
pub use playback::{DecodedVideoFrame, VideoClock, VideoFrameQueue, VideoFrameSelection};
pub use server::VideoServer;
pub use streaming::{GpuVideoFrameBuffer, VideoFrameBuffer};
pub use types::{
    VideoError, VideoInstanceId, VideoPlaybackSettings, VideoPlaybackState, VideoPlayer2D,
};
