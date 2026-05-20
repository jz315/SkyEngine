use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::asset::{Assets, Handle};
use crate::ecs::World;
use crate::render::SpriteRenderer;
use crate::video::assets::{VideoClip, VideoFrame};
use crate::video::commands::{VideoCommand, VideoCommands};
use crate::video::types::{
    VideoError, VideoInstanceId, VideoPlaybackSettings, VideoPlaybackState, VideoPlayer2D,
};

#[derive(Clone)]
pub struct VideoServer {
    inner: Arc<Mutex<VideoServerInner>>,
    commands: VideoCommands,
}

impl std::fmt::Debug for VideoServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoServer").finish_non_exhaustive()
    }
}

impl VideoServer {
    pub fn new(assets: Assets) -> Self {
        let next_instance = Arc::new(AtomicU64::new(1));
        let commands = VideoCommands::new(next_instance.clone());
        Self {
            inner: Arc::new(Mutex::new(VideoServerInner {
                assets,
                next_instance,
                instances: HashMap::default(),
            })),
            commands,
        }
    }

    #[must_use]
    pub fn commands(&self) -> VideoCommands {
        self.commands.clone()
    }

    pub fn play(
        &self,
        clip: Handle<VideoClip>,
        settings: VideoPlaybackSettings,
    ) -> Result<VideoInstanceId, VideoError> {
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        let instance = inner.reserve_instance();
        inner.play_reserved(instance, clip, settings)?;
        Ok(instance)
    }

    pub fn stop(&self, instance: VideoInstanceId) -> Result<(), VideoError> {
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        let record = inner
            .instances
            .get_mut(&instance)
            .ok_or(VideoError::InstanceNotFound { instance })?;
        record.state = VideoPlaybackState::Stopped;
        Ok(())
    }

    pub fn pause(&self, instance: VideoInstanceId) -> Result<(), VideoError> {
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        let record = inner
            .instances
            .get_mut(&instance)
            .ok_or(VideoError::InstanceNotFound { instance })?;
        if record.state == VideoPlaybackState::Playing {
            record.state = VideoPlaybackState::Paused;
        }
        Ok(())
    }

    pub fn resume(&self, instance: VideoInstanceId) -> Result<(), VideoError> {
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        let record = inner
            .instances
            .get_mut(&instance)
            .ok_or(VideoError::InstanceNotFound { instance })?;
        if matches!(
            record.state,
            VideoPlaybackState::Paused | VideoPlaybackState::Finished
        ) {
            record.state = VideoPlaybackState::Playing;
        }
        Ok(())
    }

    pub fn seek(&self, instance: VideoInstanceId, seconds: f64) -> Result<(), VideoError> {
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        inner.seek(instance, seconds)
    }

    pub fn set_looped(&self, instance: VideoInstanceId, looped: bool) -> Result<(), VideoError> {
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        let record = inner
            .instances
            .get_mut(&instance)
            .ok_or(VideoError::InstanceNotFound { instance })?;
        record.settings.looped = looped;
        Ok(())
    }

    pub fn set_playback_rate(
        &self,
        instance: VideoInstanceId,
        playback_rate: f32,
    ) -> Result<(), VideoError> {
        validate_playback_rate(playback_rate)?;
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        let record = inner
            .instances
            .get_mut(&instance)
            .ok_or(VideoError::InstanceNotFound { instance })?;
        record.settings.playback_rate = playback_rate;
        Ok(())
    }

    pub fn update(&self, dt_seconds: f32) {
        if !dt_seconds.is_finite() || dt_seconds <= 0.0 {
            return;
        }
        self.inner
            .lock()
            .expect("video server mutex poisoned")
            .update(dt_seconds as f64);
    }

    pub fn apply_commands(&self) -> Result<(), VideoError> {
        for command in self.commands.drain() {
            match command {
                VideoCommand::Play {
                    instance,
                    clip,
                    settings,
                } => self
                    .inner
                    .lock()
                    .expect("video server mutex poisoned")
                    .play_reserved(instance, clip, settings)?,
                VideoCommand::Stop { instance } => self.stop(instance)?,
                VideoCommand::Pause { instance } => self.pause(instance)?,
                VideoCommand::Resume { instance } => self.resume(instance)?,
                VideoCommand::Seek { instance, seconds } => self.seek(instance, seconds)?,
                VideoCommand::SetLooped { instance, looped } => {
                    self.set_looped(instance, looped)?;
                }
                VideoCommand::SetPlaybackRate {
                    instance,
                    playback_rate,
                } => self.set_playback_rate(instance, playback_rate)?,
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn state(&self, instance: VideoInstanceId) -> Option<VideoPlaybackState> {
        self.inner
            .lock()
            .expect("video server mutex poisoned")
            .instances
            .get(&instance)
            .map(|record| record.state)
    }

    #[must_use]
    pub fn elapsed_seconds(&self, instance: VideoInstanceId) -> Option<f64> {
        self.inner
            .lock()
            .expect("video server mutex poisoned")
            .instances
            .get(&instance)
            .map(|record| record.elapsed_seconds)
    }

    #[must_use]
    pub fn current_frame(&self, instance: VideoInstanceId) -> Option<VideoFrame> {
        let inner = self.inner.lock().expect("video server mutex poisoned");
        let record = inner.instances.get(&instance)?;
        let clip = inner.assets.try_get(&record.clip)?;
        clip.frame(record.frame_index)
    }

    #[must_use]
    pub fn current_frame_index(&self, instance: VideoInstanceId) -> Option<usize> {
        self.inner
            .lock()
            .expect("video server mutex poisoned")
            .instances
            .get(&instance)
            .map(|record| record.frame_index)
    }

    pub fn sync_world(&self, world: &mut World) -> Result<(), VideoError> {
        let mut query = world.query::<(&mut VideoPlayer2D, &mut SpriteRenderer)>();
        query.for_each(world, |(player, sprite)| {
            self.sync_player(player, sprite);
        });
        Ok(())
    }

    fn sync_player(&self, player: &mut VideoPlayer2D, sprite: &mut SpriteRenderer) {
        if !player.enabled {
            return;
        }

        if player.instance.is_none() && player.autoplay {
            player.instance = self.try_play_autoplay(player.clip.clone(), player.settings);
            player.last_frame_index = None;
        }

        let Some(instance) = player.instance else {
            return;
        };

        let (frame_index, texture) = {
            let inner = self.inner.lock().expect("video server mutex poisoned");
            let Some(record) = inner.instances.get(&instance) else {
                player.instance = None;
                player.last_frame_index = None;
                return;
            };
            if record.state == VideoPlaybackState::Stopped {
                player.instance = None;
                player.last_frame_index = None;
                return;
            }
            let Some(clip) = inner.assets.try_get(&record.clip) else {
                return;
            };
            let Some(frame) = clip.frame(record.frame_index) else {
                return;
            };
            let Some(texture) = frame
                .resident_texture()
                .or_else(|| inner.assets.load_handle(frame.texture()).ok())
            else {
                return;
            };
            (record.frame_index, texture)
        };

        if player.last_frame_index != Some(frame_index) {
            sprite.texture = Some(texture);
            player.last_frame_index = Some(frame_index);
        }
    }

    fn try_play_autoplay(
        &self,
        clip: Handle<VideoClip>,
        settings: VideoPlaybackSettings,
    ) -> Option<VideoInstanceId> {
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        if inner.assets.try_get(&clip).is_none() {
            let _ = inner.assets.load_id::<VideoClip>(clip.id());
            return None;
        }
        let instance = inner.reserve_instance();
        inner.play_reserved(instance, clip, settings).ok()?;
        Some(instance)
    }
}

struct VideoServerInner {
    assets: Assets,
    next_instance: Arc<AtomicU64>,
    instances: HashMap<VideoInstanceId, VideoInstanceRecord>,
}

impl VideoServerInner {
    fn reserve_instance(&self) -> VideoInstanceId {
        VideoInstanceId(self.next_instance.fetch_add(1, Ordering::Relaxed))
    }

    fn play_reserved(
        &mut self,
        instance: VideoInstanceId,
        clip: Handle<VideoClip>,
        settings: VideoPlaybackSettings,
    ) -> Result<(), VideoError> {
        validate_playback_rate(settings.playback_rate)?;
        let clip_asset = match self.assets.try_get(&clip) {
            Some(clip_asset) => clip_asset,
            None => {
                let _ = self.assets.load_id::<VideoClip>(clip.id());
                return Err(crate::asset::AssetError::AssetNotInstalled {
                    id: clip.id(),
                    state: self.assets.state(&clip),
                }
                .into());
            }
        };
        if clip_asset.frame_count() == 0 {
            return Err(VideoError::EmptyClip);
        }

        self.instances.insert(
            instance,
            VideoInstanceRecord {
                clip,
                settings,
                elapsed_seconds: 0.0,
                frame_index: 0,
                state: if settings.start_paused {
                    VideoPlaybackState::Paused
                } else {
                    VideoPlaybackState::Playing
                },
            },
        );
        Ok(())
    }

    fn update(&mut self, dt_seconds: f64) {
        let ids = self.instances.keys().copied().collect::<Vec<_>>();
        for instance in ids {
            let Some(record) = self.instances.get_mut(&instance) else {
                continue;
            };
            if record.state != VideoPlaybackState::Playing {
                continue;
            }
            let Some(clip) = self.assets.try_get(&record.clip) else {
                continue;
            };
            let total = clip.total_duration().as_secs_f64();
            if total <= 0.0 {
                record.state = VideoPlaybackState::Finished;
                continue;
            }

            record.elapsed_seconds += dt_seconds * record.settings.playback_rate as f64;
            if record.elapsed_seconds >= total {
                if record.settings.looped {
                    record.elapsed_seconds %= total;
                } else {
                    record.elapsed_seconds = total;
                    record.state = VideoPlaybackState::Finished;
                }
            }
            record.frame_index = clip.frame_index_at(record.elapsed_seconds);
        }
    }

    fn seek(&mut self, instance: VideoInstanceId, seconds: f64) -> Result<(), VideoError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(VideoError::InvalidSeekTime { seconds });
        }
        let record = self
            .instances
            .get_mut(&instance)
            .ok_or(VideoError::InstanceNotFound { instance })?;
        let clip = self.assets.get(&record.clip)?;
        let total = clip.total_duration().as_secs_f64();
        record.elapsed_seconds = seconds.min(total);
        record.frame_index = clip.frame_index_at(record.elapsed_seconds);
        if record.state == VideoPlaybackState::Finished && record.elapsed_seconds < total {
            record.state = VideoPlaybackState::Paused;
        }
        Ok(())
    }
}

struct VideoInstanceRecord {
    clip: Handle<VideoClip>,
    settings: VideoPlaybackSettings,
    elapsed_seconds: f64,
    frame_index: usize,
    state: VideoPlaybackState,
}

fn validate_playback_rate(playback_rate: f32) -> Result<(), VideoError> {
    if !playback_rate.is_finite() || playback_rate <= 0.0 {
        return Err(VideoError::InvalidPlaybackRate { playback_rate });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::asset::{AssetConfig, TextureAsset};

    use super::*;

    fn server_with_clip() -> (VideoServer, Handle<VideoClip>) {
        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let a = assets.insert_runtime(TextureAsset::white_pixel());
        let b = assets.insert_runtime(TextureAsset::checkerboard(
            2,
            1,
            [255, 0, 0, 255],
            [0, 0, 255, 255],
        ));
        let clip = VideoClip::from_textures(1, 1, 10.0, [a, b]).unwrap();
        let clip = assets.insert_runtime(clip);
        (VideoServer::new(assets), clip)
    }

    #[test]
    fn update_advances_frames_and_finishes() {
        let (server, clip) = server_with_clip();
        let instance = server
            .play(clip, VideoPlaybackSettings::default())
            .expect("play should start");

        assert_eq!(server.current_frame_index(instance), Some(0));
        server.update(0.11);
        assert_eq!(server.current_frame_index(instance), Some(1));
        server.update(0.2);
        assert_eq!(server.state(instance), Some(VideoPlaybackState::Finished));
    }

    #[test]
    fn looped_playback_wraps() {
        let (server, clip) = server_with_clip();
        let instance = server
            .play(clip, VideoPlaybackSettings::default().looped(true))
            .expect("play should start");

        server.update(0.25);
        assert_eq!(server.state(instance), Some(VideoPlaybackState::Playing));
        assert_eq!(server.current_frame_index(instance), Some(0));
    }
}
