use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;

use crate::asset::{
    Asset, AssetError, AssetId, AssetInstallContext, AssetLoadContext, AssetRuntimeFactory,
    AssetServer, Handle, LoadedAsset, TextureAsset,
};
use crate::video::types::VideoError;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoFrame {
    texture: Handle<TextureAsset>,
    duration: Duration,
}

impl VideoFrame {
    #[must_use]
    pub fn new(texture: Handle<TextureAsset>, duration: Duration) -> Self {
        Self { texture, duration }
    }

    #[must_use]
    pub fn texture(self) -> Handle<TextureAsset> {
        self.texture
    }

    #[must_use]
    pub fn duration(self) -> Duration {
        self.duration
    }
}

#[derive(Clone, Debug)]
pub struct VideoClip {
    width: u32,
    height: u32,
    frames: Arc<[VideoFrame]>,
    cumulative_ends: Arc<[f64]>,
    total_duration: Duration,
}

impl Asset for VideoClip {
    const TYPE: &'static str = "video_clip";
}

impl VideoClip {
    pub fn new(
        width: u32,
        height: u32,
        frames: impl Into<Vec<VideoFrame>>,
    ) -> Result<Self, VideoError> {
        let frames = frames.into();
        if frames.is_empty() {
            return Err(VideoError::EmptyClip);
        }
        if frames.iter().any(|frame| frame.duration.is_zero()) {
            return Err(VideoError::InvalidFrameDuration);
        }

        let mut total = 0.0f64;
        let mut cumulative_ends = Vec::with_capacity(frames.len());
        for frame in &frames {
            total += frame.duration.as_secs_f64();
            cumulative_ends.push(total);
        }

        Ok(Self {
            width,
            height,
            frames: Arc::<[VideoFrame]>::from(frames),
            cumulative_ends: Arc::<[f64]>::from(cumulative_ends),
            total_duration: Duration::from_secs_f64(total),
        })
    }

    pub fn from_textures(
        width: u32,
        height: u32,
        fps: f32,
        textures: impl Into<Vec<Handle<TextureAsset>>>,
    ) -> Result<Self, VideoError> {
        if !fps.is_finite() || fps <= 0.0 {
            return Err(VideoError::InvalidPlaybackRate { playback_rate: fps });
        }
        let duration = Duration::from_secs_f64(1.0 / fps as f64);
        let frames = textures
            .into()
            .into_iter()
            .map(|texture| VideoFrame::new(texture, duration))
            .collect::<Vec<_>>();
        Self::new(width, height, frames)
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn frames(&self) -> &[VideoFrame] {
        &self.frames
    }

    #[must_use]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    #[must_use]
    pub fn total_duration(&self) -> Duration {
        self.total_duration
    }

    #[must_use]
    pub fn frame(&self, index: usize) -> Option<VideoFrame> {
        self.frames.get(index).copied()
    }

    #[must_use]
    pub fn frame_index_at(&self, seconds: f64) -> usize {
        if self.frames.len() == 1 {
            return 0;
        }
        let seconds = seconds.clamp(0.0, self.total_duration.as_secs_f64());
        const EPSILON: f64 = 1e-9;
        for (index, end) in self.cumulative_ends.iter().enumerate() {
            if seconds < *end - EPSILON {
                return index;
            }
        }
        self.frames.len() - 1
    }
}

pub fn register_video_asset_factories(asset_server: &AssetServer) {
    asset_server.register_factory(VideoClipFactory);
}

pub(crate) struct VideoClipFactory;

impl AssetRuntimeFactory for VideoClipFactory {
    type Asset = VideoClip;
    type Loaded = VideoClipDescriptor;

    fn load(&self, ctx: AssetLoadContext<'_>) -> Result<LoadedAsset<Self::Loaded>, AssetError> {
        let descriptor: VideoClipDescriptor =
            serde_json::from_slice(ctx.bytes).map_err(|error| AssetError::Json {
                path: ctx.cooked_root.join(&ctx.entry.cooked_path),
                message: error.to_string(),
            })?;
        let dependencies = descriptor
            .frames
            .iter()
            .map(|frame| frame.texture)
            .collect::<Vec<_>>();
        Ok(LoadedAsset::new(descriptor).with_dependencies(dependencies))
    }

    fn install(
        &self,
        loaded: &Self::Loaded,
        _ctx: AssetInstallContext<'_>,
    ) -> Result<Self::Asset, AssetError> {
        let frames = loaded
            .frames
            .iter()
            .map(|frame| {
                let duration_seconds = frame.duration_seconds();
                if !duration_seconds.is_finite() || duration_seconds <= 0.0 {
                    return Err(AssetError::InvalidCookedAsset {
                        id: None,
                        message: format!(
                            "video frame duration must be finite and greater than zero (got {duration_seconds})"
                        ),
                    });
                }
                Ok(VideoFrame::new(
                    Handle::new(frame.texture),
                    Duration::from_secs_f64(duration_seconds),
                ))
            })
            .collect::<Result<Vec<_>, AssetError>>()?;
        VideoClip::new(loaded.width, loaded.height, frames).map_err(|error| {
            AssetError::InvalidCookedAsset {
                id: None,
                message: error.to_string(),
            }
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct VideoClipDescriptor {
    width: u32,
    height: u32,
    frames: Vec<VideoFrameDescriptor>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct VideoFrameDescriptor {
    texture: AssetId,
    #[serde(default)]
    duration_ms: Option<f64>,
    #[serde(default)]
    duration_seconds: Option<f64>,
}

impl VideoFrameDescriptor {
    fn duration_seconds(&self) -> f64 {
        self.duration_seconds
            .or_else(|| self.duration_ms.map(|ms| ms / 1000.0))
            .unwrap_or(1.0 / 30.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texture_handle() -> Handle<TextureAsset> {
        Handle::new(AssetId::new())
    }

    #[test]
    fn frame_index_uses_cumulative_durations() {
        let clip = VideoClip::new(
            2,
            2,
            [
                VideoFrame::new(texture_handle(), Duration::from_millis(100)),
                VideoFrame::new(texture_handle(), Duration::from_millis(200)),
                VideoFrame::new(texture_handle(), Duration::from_millis(100)),
            ],
        )
        .unwrap();

        assert_eq!(clip.frame_index_at(0.0), 0);
        assert_eq!(clip.frame_index_at(0.099), 0);
        assert_eq!(clip.frame_index_at(0.100), 1);
        assert_eq!(clip.frame_index_at(0.299), 1);
        assert_eq!(clip.frame_index_at(0.300), 2);
        assert_eq!(clip.frame_index_at(99.0), 2);
    }
}
