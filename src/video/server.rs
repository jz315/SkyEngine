use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::asset::{
    Asset, AssetEvent, AssetEventCursor, AssetEventKind, AssetId, AssetState, Assets, Handle,
    TextureAsset,
};
use crate::ecs::World;
use crate::render::SpriteRenderer;
use crate::video::assets::{VideoClip, VideoFrame};
use crate::video::commands::{VideoCommand, VideoCommands};
use crate::video::playback::rgba_len;
use crate::video::types::{
    VideoError, VideoInstanceId, VideoPlaybackSettings, VideoPlaybackState, VideoPlayer2D,
    VideoServerStats,
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
        let asset_event_cursor = assets.event_cursor();
        Self {
            inner: Arc::new(Mutex::new(VideoServerInner {
                assets,
                asset_event_cursor,
                next_instance,
                instances: HashMap::default(),
                failed_play_requests: 0,
                last_play_failure: None,
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
        if let Err(error) = inner.play_reserved(instance, clip, settings) {
            inner.record_failed_play_request(&error);
            return Err(error);
        }
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

    #[must_use]
    pub fn stats(&self) -> VideoServerStats {
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        inner.consume_asset_events();
        inner.stats()
    }

    pub fn apply_commands(&self) -> Result<(), VideoError> {
        for command in self.commands.drain() {
            match command {
                VideoCommand::Play {
                    instance,
                    clip,
                    settings,
                } => {
                    let mut inner = self.inner.lock().expect("video server mutex poisoned");
                    if let Err(error) = inner.play_reserved(instance, clip, settings) {
                        inner.record_failed_play_request(&error);
                        return Err(error);
                    }
                }
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
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        inner.consume_asset_events();
        inner.instances.get(&instance).map(|record| record.state)
    }

    #[must_use]
    pub fn elapsed_seconds(&self, instance: VideoInstanceId) -> Option<f64> {
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        inner.consume_asset_events();
        inner
            .instances
            .get(&instance)
            .map(|record| record.elapsed_seconds)
    }

    #[must_use]
    pub fn current_frame(&self, instance: VideoInstanceId) -> Option<VideoFrame> {
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        inner.consume_asset_events();
        let record = inner.instances.get(&instance)?;
        let clip = inner.assets.try_get(&record.clip)?;
        clip.frame(record.frame_index)
    }

    #[must_use]
    pub fn current_frame_index(&self, instance: VideoInstanceId) -> Option<usize> {
        let mut inner = self.inner.lock().expect("video server mutex poisoned");
        inner.consume_asset_events();
        inner
            .instances
            .get(&instance)
            .map(|record| record.frame_index)
    }

    pub fn sync_world(&self, world: &mut World) -> Result<(), VideoError> {
        self.inner
            .lock()
            .expect("video server mutex poisoned")
            .consume_asset_events();
        let mut query = world.query_mut::<(&mut VideoPlayer2D, &mut SpriteRenderer)>();
        query.for_each(|(player, sprite)| {
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

        let texture_changed =
            sprite.texture.as_ref().map(|handle| handle.id()) != Some(texture.id());
        if player.last_frame_index != Some(frame_index) || texture_changed {
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
    asset_event_cursor: AssetEventCursor,
    next_instance: Arc<AtomicU64>,
    instances: HashMap<VideoInstanceId, VideoInstanceRecord>,
    failed_play_requests: u64,
    last_play_failure: Option<String>,
}

impl VideoServerInner {
    fn reserve_instance(&self) -> VideoInstanceId {
        VideoInstanceId(self.next_instance.fetch_add(1, Ordering::Relaxed))
    }

    fn record_failed_play_request(&mut self, error: &VideoError) {
        self.failed_play_requests = self.failed_play_requests.saturating_add(1);
        self.last_play_failure = Some(error.to_string());
    }

    fn consume_asset_events(&mut self) {
        let events = self.assets.events_since(&mut self.asset_event_cursor);
        for event in &events {
            self.handle_asset_event(event);
        }
    }

    fn handle_asset_event(&mut self, event: &AssetEvent) {
        if event.asset_type != VideoClip::TYPE {
            return;
        }

        match event.kind {
            AssetEventKind::Loaded => {}
            AssetEventKind::Installed | AssetEventKind::Reloaded => {
                self.refresh_instances_for_clip(event.id)
            }
            AssetEventKind::ReloadQueued => {}
            AssetEventKind::Unloaded => self.stop_instances_for_clip(event.id),
            AssetEventKind::Failed if event.state == AssetState::Installed => {}
            AssetEventKind::Failed => self.stop_instances_for_clip(event.id),
        }
    }

    fn refresh_instances_for_clip(&mut self, clip_id: AssetId) {
        let ids = self
            .instances
            .iter()
            .filter_map(|(instance, record)| (record.clip.id() == clip_id).then_some(*instance))
            .collect::<Vec<_>>();

        for instance in ids {
            self.refresh_instance_clip(instance);
        }
    }

    fn stop_instances_for_clip(&mut self, clip_id: AssetId) {
        for record in self.instances.values_mut() {
            if record.clip.id() == clip_id {
                record.state = VideoPlaybackState::Stopped;
            }
        }
    }

    fn refresh_instance_clip(&mut self, instance: VideoInstanceId) {
        let Some(record) = self.instances.get_mut(&instance) else {
            return;
        };
        recompute_record_frame(&self.assets, record);
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

    fn stats(&self) -> VideoServerStats {
        let mut stats = VideoServerStats {
            instances: self.instances.len(),
            ..Default::default()
        };
        let mut clips = HashSet::new();
        let mut texture_ids = HashSet::new();

        for record in self.instances.values() {
            clips.insert(record.clip.id());
            match record.state {
                VideoPlaybackState::Playing => stats.playing_instances += 1,
                VideoPlaybackState::Paused => stats.paused_instances += 1,
                VideoPlaybackState::Finished => stats.finished_instances += 1,
                VideoPlaybackState::Stopped => {
                    stats.stopped_instances += 1;
                    continue;
                }
            }

            let Some(clip) = self.assets.try_get(&record.clip) else {
                continue;
            };
            let Some(frame) = clip.frame(record.frame_index) else {
                continue;
            };
            let texture_id = frame
                .resident_texture()
                .map(|texture| texture.id())
                .unwrap_or_else(|| frame.texture().id());
            if texture_ids.insert(texture_id) {
                stats.current_frame_texture_bytes = stats
                    .current_frame_texture_bytes
                    .saturating_add(current_frame_texture_bytes(&self.assets, texture_id, &clip));
            }
        }

        stats.distinct_clips = clips.len();
        stats.current_frame_textures = texture_ids.len();
        stats.failed_play_requests = self.failed_play_requests;
        stats.last_play_failure = self.last_play_failure.clone();
        stats
    }
}

struct VideoInstanceRecord {
    clip: Handle<VideoClip>,
    settings: VideoPlaybackSettings,
    elapsed_seconds: f64,
    frame_index: usize,
    state: VideoPlaybackState,
}

fn current_frame_texture_bytes(assets: &Assets, texture_id: AssetId, clip: &VideoClip) -> usize {
    assets.try_get_id::<TextureAsset>(texture_id).map_or_else(
        || rgba_len(clip.width(), clip.height()).unwrap_or(0),
        |texture| texture.pixels().len(),
    )
}

fn recompute_record_frame(assets: &Assets, record: &mut VideoInstanceRecord) {
    let Some(clip) = assets.try_get(&record.clip) else {
        record.state = VideoPlaybackState::Stopped;
        return;
    };

    let total = clip.total_duration().as_secs_f64();
    if total <= 0.0 {
        record.elapsed_seconds = 0.0;
        record.frame_index = 0;
        record.state = VideoPlaybackState::Finished;
        return;
    }

    if record.elapsed_seconds >= total {
        if record.settings.looped {
            record.elapsed_seconds %= total;
        } else {
            record.elapsed_seconds = total;
            if record.state == VideoPlaybackState::Playing {
                record.state = VideoPlaybackState::Finished;
            }
        }
    }

    record.frame_index = clip.frame_index_at(record.elapsed_seconds);
}

fn validate_playback_rate(playback_rate: f32) -> Result<(), VideoError> {
    if !playback_rate.is_finite() || playback_rate <= 0.0 {
        return Err(VideoError::InvalidPlaybackRate { playback_rate });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::asset::{AssetConfig, AssetId, TextureAsset, WeakHandle};
    use crate::ecs::World;
    use crate::render::SpriteRenderer;

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

    #[test]
    fn stats_report_backend_local_video_playback_tracking() {
        let (server, clip) = server_with_clip();
        let instance = server
            .play(clip, VideoPlaybackSettings::default())
            .expect("play should start");

        let stats = server.stats();
        assert_eq!(stats.instances, 1);
        assert_eq!(stats.playing_instances, 1);
        assert_eq!(stats.paused_instances, 0);
        assert_eq!(stats.finished_instances, 0);
        assert_eq!(stats.stopped_instances, 0);
        assert_eq!(stats.distinct_clips, 1);
        assert_eq!(stats.current_frame_textures, 1);
        assert_eq!(stats.current_frame_texture_bytes, 4);
        assert_eq!(stats.failed_play_requests, 0);
        assert_eq!(stats.last_play_failure, None);

        server.pause(instance).unwrap();
        let stats = server.stats();
        assert_eq!(stats.playing_instances, 0);
        assert_eq!(stats.paused_instances, 1);
        assert_eq!(stats.current_frame_textures, 1);
        assert_eq!(stats.current_frame_texture_bytes, 4);

        server.stop(instance).unwrap();
        let stats = server.stats();
        assert_eq!(stats.instances, 1);
        assert_eq!(stats.stopped_instances, 1);
        assert_eq!(stats.current_frame_textures, 0);
        assert_eq!(stats.current_frame_texture_bytes, 0);
    }

    #[test]
    fn stats_estimate_current_frame_bytes_when_texture_is_not_installed() {
        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let frame =
            VideoFrame::from_weak(WeakHandle::new(AssetId::new()), Duration::from_millis(100));
        let clip = assets.insert_runtime(VideoClip::new(3, 2, [frame]).unwrap());
        let server = VideoServer::new(assets);
        server
            .play(clip, VideoPlaybackSettings::default())
            .expect("play should start");

        let stats = server.stats();
        assert_eq!(stats.current_frame_textures, 1);
        assert_eq!(stats.current_frame_texture_bytes, 24);
    }

    #[test]
    fn stats_count_failed_video_play_requests() {
        let (server, clip) = server_with_clip();

        let error = server
            .play(clip, VideoPlaybackSettings::default().playback_rate(0.0))
            .expect_err("invalid playback rate should reject play request");

        assert!(matches!(error, VideoError::InvalidPlaybackRate { .. }));
        let stats = server.stats();
        assert_eq!(stats.failed_play_requests, 1);
        assert_eq!(stats.instances, 0);
        assert!(stats
            .last_play_failure
            .as_deref()
            .is_some_and(|message| message.contains("Video playback rate")));
    }

    #[test]
    fn sync_world_updates_sprite_when_reloaded_clip_swaps_texture_at_same_frame() {
        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let first = assets.insert_runtime(TextureAsset::white_pixel());
        let second = assets.insert_runtime(TextureAsset::checkerboard(
            2,
            1,
            [255, 0, 0, 255],
            [0, 0, 255, 255],
        ));
        let clip =
            assets.insert_runtime(VideoClip::from_textures(1, 1, 10.0, [first.clone()]).unwrap());
        let server = VideoServer::new(assets.clone());
        let mut world = World::new();
        let entity = world.spawn((VideoPlayer2D::new(clip.clone()), SpriteRenderer::default()));

        server.sync_world(&mut world).unwrap();
        assert_eq!(
            world
                .get::<SpriteRenderer>(entity)
                .unwrap()
                .texture
                .as_ref()
                .map(|handle| handle.id()),
            Some(first.id())
        );

        assets
            .replace_runtime(
                &clip,
                VideoClip::from_textures(1, 1, 10.0, [second.clone()]).unwrap(),
            )
            .unwrap();
        server.sync_world(&mut world).unwrap();

        assert_eq!(
            world
                .get::<SpriteRenderer>(entity)
                .unwrap()
                .texture
                .as_ref()
                .map(|handle| handle.id()),
            Some(second.id())
        );
        assert_eq!(
            world.get::<VideoPlayer2D>(entity).unwrap().last_frame_index,
            Some(0)
        );
    }

    #[test]
    fn clip_installed_event_clamps_paused_instance_frame_index() {
        let assets = Assets::with_empty_manifest(AssetConfig::default());
        let first = assets.insert_runtime(TextureAsset::white_pixel());
        let second = assets.insert_runtime(TextureAsset::checkerboard(
            2,
            1,
            [255, 0, 0, 255],
            [0, 0, 255, 255],
        ));
        let clip = assets
            .insert_runtime(VideoClip::from_textures(1, 1, 10.0, [first.clone(), second]).unwrap());
        let server = VideoServer::new(assets.clone());
        let instance = server
            .play(clip.clone(), VideoPlaybackSettings::default())
            .expect("play should start");

        server.update(0.11);
        assert_eq!(server.current_frame_index(instance), Some(1));
        server.pause(instance).unwrap();

        assets
            .replace_runtime(
                &clip,
                VideoClip::from_textures(1, 1, 10.0, [first.clone()]).unwrap(),
            )
            .unwrap();

        assert_eq!(server.current_frame_index(instance), Some(0));
        assert_eq!(server.state(instance), Some(VideoPlaybackState::Paused));
    }
}
