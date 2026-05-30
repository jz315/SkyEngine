use crate::diagnostics::{DiagnosticEvent, DiagnosticSeverity, Diagnostics};
use crate::ecs::World;

#[cfg(feature = "audio")]
use crate::audio::{AudioServer, AudioServerStats};
#[cfg(feature = "video")]
use crate::video::{VideoServer, VideoServerStats};

fn optional_field(value: Option<&str>) -> &str {
    value.unwrap_or("none")
}

fn ensure_diagnostics(world: &mut World) -> Diagnostics {
    if let Some(diagnostics) = world.get_resource::<Diagnostics>() {
        return diagnostics.clone();
    }
    let diagnostics = Diagnostics::new();
    world.insert_resource(diagnostics.clone());
    diagnostics
}

#[cfg(feature = "audio")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AudioDiagnosticsState {
    last_stats: Option<AudioServerStats>,
}

#[cfg(feature = "audio")]
pub(crate) fn publish_audio_diagnostics(world: &mut World, audio: &AudioServer) {
    let diagnostics = ensure_diagnostics(world);
    let frame = Some(world.time.frame_count);
    let stats = audio.stats();
    let mut state = world
        .remove_resource::<AudioDiagnosticsState>()
        .unwrap_or_default();
    state.publish_stats(&diagnostics, frame, stats);
    world.insert_resource(state);
}

#[cfg(feature = "audio")]
impl AudioDiagnosticsState {
    fn publish_stats(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        stats: AudioServerStats,
    ) {
        let previous_failures = self
            .last_stats
            .as_ref()
            .map(|stats| stats.failed_play_requests)
            .unwrap_or(0);
        let failure_delta = stats.failed_play_requests.saturating_sub(previous_failures);
        let previous_backend_available = self
            .last_stats
            .as_ref()
            .map(|stats| stats.backend_available)
            .unwrap_or(true);
        if self.last_stats.as_ref() == Some(&stats) {
            return;
        }

        if !stats.backend_available && previous_backend_available {
            diagnostics.push(
                DiagnosticEvent::new(
                    "audio",
                    "audio.backend.unavailable",
                    DiagnosticSeverity::Warning,
                    "Audio backend is unavailable",
                )
                .with_frame(frame)
                .with_field(
                    "disabled_reason",
                    optional_field(stats.disabled_reason.as_deref()),
                )
                .with_field("configured_buses", stats.configured_buses)
                .with_field("direct_instances", stats.direct_instances)
                .with_field("emitter_instances", stats.emitter_instances),
            );
        }

        if failure_delta > 0 {
            diagnostics.push(
                DiagnosticEvent::new(
                    "audio",
                    "audio.play.failed",
                    DiagnosticSeverity::Warning,
                    "Audio backend play requests failed",
                )
                .with_frame(frame)
                .with_field("failed_delta", failure_delta)
                .with_field("failed_play_requests", stats.failed_play_requests)
                .with_field(
                    "last_play_failure",
                    optional_field(stats.last_play_failure.as_deref()),
                ),
            );
        }

        diagnostics.push(
            DiagnosticEvent::new(
                "audio",
                "audio.stats",
                DiagnosticSeverity::Info,
                "Audio backend stats changed",
            )
            .with_frame(frame)
            .with_field("backend_available", stats.backend_available)
            .with_field(
                "disabled_reason",
                optional_field(stats.disabled_reason.as_deref()),
            )
            .with_field("configured_buses", stats.configured_buses)
            .with_field("backend_instances", stats.backend_instances)
            .with_field("backend_spatial_instances", stats.backend_spatial_instances)
            .with_field("direct_instances", stats.direct_instances)
            .with_field("emitter_instances", stats.emitter_instances)
            .with_field("failed_play_requests", stats.failed_play_requests)
            .with_field(
                "last_play_failure",
                optional_field(stats.last_play_failure.as_deref()),
            ),
        );
        self.last_stats = Some(stats);
    }
}

#[cfg(feature = "video")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VideoDiagnosticsState {
    last_stats: Option<VideoServerStats>,
}

#[cfg(feature = "video")]
pub(crate) fn publish_video_diagnostics(world: &mut World, video: &VideoServer) {
    let diagnostics = ensure_diagnostics(world);
    let frame = Some(world.time.frame_count);
    let stats = video.stats();
    let mut state = world
        .remove_resource::<VideoDiagnosticsState>()
        .unwrap_or_default();
    state.publish_stats(&diagnostics, frame, stats);
    world.insert_resource(state);
}

#[cfg(feature = "video")]
impl VideoDiagnosticsState {
    fn publish_stats(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        stats: VideoServerStats,
    ) {
        let previous_failures = self
            .last_stats
            .as_ref()
            .map(|stats| stats.failed_play_requests)
            .unwrap_or(0);
        let failure_delta = stats.failed_play_requests.saturating_sub(previous_failures);
        let previous_frame_bytes = self
            .last_stats
            .as_ref()
            .map(|stats| stats.current_frame_texture_bytes)
            .unwrap_or(0);
        if self.last_stats.as_ref() == Some(&stats) {
            return;
        }

        if failure_delta > 0 {
            diagnostics.push(
                DiagnosticEvent::new(
                    "video",
                    "video.play.failed",
                    DiagnosticSeverity::Warning,
                    "Video backend play requests failed",
                )
                .with_frame(frame)
                .with_field("failed_delta", failure_delta)
                .with_field("failed_play_requests", stats.failed_play_requests)
                .with_field(
                    "last_play_failure",
                    optional_field(stats.last_play_failure.as_deref()),
                ),
            );
        }

        if stats.current_frame_texture_bytes > 0
            && previous_frame_bytes != stats.current_frame_texture_bytes
        {
            diagnostics.push(
                DiagnosticEvent::new(
                    "video",
                    "video.frame.resident",
                    DiagnosticSeverity::Info,
                    "Video backend has resident current-frame texture bytes",
                )
                .with_frame(frame)
                .with_field(
                    "current_frame_texture_bytes",
                    stats.current_frame_texture_bytes,
                )
                .with_field("current_frame_textures", stats.current_frame_textures)
                .with_field("instances", stats.instances)
                .with_field("playing_instances", stats.playing_instances)
                .with_field("distinct_clips", stats.distinct_clips),
            );
        }

        diagnostics.push(
            DiagnosticEvent::new(
                "video",
                "video.stats",
                DiagnosticSeverity::Info,
                "Video backend stats changed",
            )
            .with_frame(frame)
            .with_field("instances", stats.instances)
            .with_field("playing_instances", stats.playing_instances)
            .with_field("paused_instances", stats.paused_instances)
            .with_field("finished_instances", stats.finished_instances)
            .with_field("stopped_instances", stats.stopped_instances)
            .with_field("distinct_clips", stats.distinct_clips)
            .with_field("current_frame_textures", stats.current_frame_textures)
            .with_field(
                "current_frame_texture_bytes",
                stats.current_frame_texture_bytes,
            )
            .with_field("failed_play_requests", stats.failed_play_requests)
            .with_field(
                "last_play_failure",
                optional_field(stats.last_play_failure.as_deref()),
            ),
        );
        self.last_stats = Some(stats);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field<'a>(event: &'a DiagnosticEvent, key: &str) -> Option<&'a str> {
        event
            .fields
            .iter()
            .find(|field| field.key == key)
            .map(|field| field.value.as_str())
    }

    #[cfg(feature = "audio")]
    #[test]
    fn publish_audio_diagnostics_reports_backend_local_stats() {
        let assets =
            crate::asset::Assets::with_empty_manifest(crate::asset::AssetConfig::default());
        let audio = AudioServer::new(
            crate::audio::AudioConfig {
                enabled: false,
                extra_buses: vec!["ambience".to_string()],
                ..Default::default()
            },
            assets,
        );
        let mut world = World::new();

        publish_audio_diagnostics(&mut world, &audio);
        publish_audio_diagnostics(&mut world, &audio);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let event = events
            .iter()
            .find(|event| event.code == "audio.stats")
            .expect("audio stats event should be emitted");

        assert_eq!(
            events
                .iter()
                .filter(|event| event.code == "audio.stats")
                .count(),
            1
        );
        assert_eq!(event.category, "audio");
        assert_eq!(field(event, "backend_available"), Some("false"));
        assert_eq!(
            field(event, "disabled_reason"),
            Some("audio explicitly disabled in config")
        );
        assert_eq!(field(event, "configured_buses"), Some("5"));
        assert_eq!(field(event, "backend_instances"), Some("0"));
        assert_eq!(field(event, "direct_instances"), Some("0"));
        assert_eq!(field(event, "failed_play_requests"), Some("0"));
        assert_eq!(field(event, "last_play_failure"), Some("none"));
    }

    #[cfg(feature = "audio")]
    #[test]
    fn publish_audio_diagnostics_reports_backend_unavailable_once() {
        let assets =
            crate::asset::Assets::with_empty_manifest(crate::asset::AssetConfig::default());
        let audio = AudioServer::new(
            crate::audio::AudioConfig {
                enabled: false,
                extra_buses: vec!["ambience".to_string()],
                ..Default::default()
            },
            assets,
        );
        let mut world = World::new();

        publish_audio_diagnostics(&mut world, &audio);
        publish_audio_diagnostics(&mut world, &audio);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let event = events
            .iter()
            .find(|event| event.code == "audio.backend.unavailable")
            .expect("audio backend unavailable event should be emitted");

        assert_eq!(
            events
                .iter()
                .filter(|event| event.code == "audio.backend.unavailable")
                .count(),
            1
        );
        assert_eq!(event.severity, DiagnosticSeverity::Warning);
        assert_eq!(
            field(event, "disabled_reason"),
            Some("audio explicitly disabled in config")
        );
        assert_eq!(field(event, "configured_buses"), Some("5"));
        assert_eq!(field(event, "direct_instances"), Some("0"));
        assert_eq!(field(event, "emitter_instances"), Some("0"));
    }

    #[cfg(feature = "audio")]
    #[test]
    fn publish_audio_diagnostics_reports_failed_play_delta() {
        let assets =
            crate::asset::Assets::with_empty_manifest(crate::asset::AssetConfig::default());
        let track = assets.insert_runtime(crate::audio::MusicTrack {
            bytes: std::sync::Arc::<[u8]>::from(Vec::<u8>::new()),
        });
        let audio = AudioServer::new(
            crate::audio::AudioConfig {
                enabled: false,
                ..Default::default()
            },
            assets,
        );
        let mut world = World::new();

        let _ = audio.play_music(track, crate::audio::AudioPlaybackSettings::default());
        publish_audio_diagnostics(&mut world, &audio);
        publish_audio_diagnostics(&mut world, &audio);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let event = events
            .iter()
            .find(|event| event.code == "audio.play.failed")
            .expect("audio failed-play event should be emitted");

        assert_eq!(
            events
                .iter()
                .filter(|event| event.code == "audio.play.failed")
                .count(),
            1
        );
        assert_eq!(event.severity, DiagnosticSeverity::Warning);
        assert_eq!(field(event, "failed_delta"), Some("1"));
        assert_eq!(field(event, "failed_play_requests"), Some("1"));
        assert!(field(event, "last_play_failure")
            .is_some_and(|message| message.contains("Audio backend unavailable")));
    }

    #[cfg(feature = "video")]
    #[test]
    fn publish_video_diagnostics_reports_backend_local_stats() {
        let assets =
            crate::asset::Assets::with_empty_manifest(crate::asset::AssetConfig::default());
        let texture = assets.insert_runtime(crate::asset::TextureAsset::white_pixel());
        let clip = crate::video::VideoClip::from_textures(1, 1, 10.0, [texture]).unwrap();
        let clip = assets.insert_runtime(clip);
        let video = VideoServer::new(assets);
        video
            .play(clip, crate::video::VideoPlaybackSettings::default())
            .expect("play should start");
        let mut world = World::new();

        publish_video_diagnostics(&mut world, &video);
        publish_video_diagnostics(&mut world, &video);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let event = events
            .iter()
            .find(|event| event.code == "video.stats")
            .expect("video stats event should be emitted");

        assert_eq!(
            events
                .iter()
                .filter(|event| event.code == "video.stats")
                .count(),
            1
        );
        assert_eq!(event.category, "video");
        assert_eq!(field(event, "instances"), Some("1"));
        assert_eq!(field(event, "playing_instances"), Some("1"));
        assert_eq!(field(event, "distinct_clips"), Some("1"));
        assert_eq!(field(event, "current_frame_textures"), Some("1"));
        assert_eq!(field(event, "current_frame_texture_bytes"), Some("4"));
        assert_eq!(field(event, "failed_play_requests"), Some("0"));
        assert_eq!(field(event, "last_play_failure"), Some("none"));
    }

    #[cfg(feature = "video")]
    #[test]
    fn publish_video_diagnostics_reports_frame_residency_bytes() {
        let assets =
            crate::asset::Assets::with_empty_manifest(crate::asset::AssetConfig::default());
        let first_texture = assets.insert_runtime(crate::asset::TextureAsset::white_pixel());
        let first_clip =
            crate::video::VideoClip::from_textures(1, 1, 10.0, [first_texture]).unwrap();
        let first_clip = assets.insert_runtime(first_clip);
        let video = VideoServer::new(assets.clone());
        video
            .play(first_clip, crate::video::VideoPlaybackSettings::default())
            .expect("play should start");
        let mut world = World::new();

        publish_video_diagnostics(&mut world, &video);
        publish_video_diagnostics(&mut world, &video);

        let larger_texture = assets.insert_runtime(crate::asset::TextureAsset::new(
            2,
            1,
            crate::asset::TextureColorSpace::Srgb,
            std::sync::Arc::<[u8]>::from(vec![255; 8]),
        ));
        let larger_clip =
            crate::video::VideoClip::from_textures(2, 1, 10.0, [larger_texture]).unwrap();
        let larger_clip = assets.insert_runtime(larger_clip);
        video
            .play(larger_clip, crate::video::VideoPlaybackSettings::default())
            .expect("second play should start");
        publish_video_diagnostics(&mut world, &video);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let residency = events
            .iter()
            .filter(|event| event.code == "video.frame.resident")
            .collect::<Vec<_>>();
        assert_eq!(residency.len(), 2);
        assert_eq!(residency[0].severity, DiagnosticSeverity::Info);
        assert_eq!(
            field(residency[0], "current_frame_texture_bytes"),
            Some("4")
        );
        assert_eq!(field(residency[0], "current_frame_textures"), Some("1"));
        assert_eq!(field(residency[0], "instances"), Some("1"));
        assert_eq!(
            field(residency[1], "current_frame_texture_bytes"),
            Some("12")
        );
        assert_eq!(field(residency[1], "current_frame_textures"), Some("2"));
        assert_eq!(field(residency[1], "instances"), Some("2"));
        assert_eq!(field(residency[1], "distinct_clips"), Some("2"));
    }

    #[cfg(feature = "video")]
    #[test]
    fn publish_video_diagnostics_reports_failed_play_delta() {
        let assets =
            crate::asset::Assets::with_empty_manifest(crate::asset::AssetConfig::default());
        let texture = assets.insert_runtime(crate::asset::TextureAsset::white_pixel());
        let clip = crate::video::VideoClip::from_textures(1, 1, 10.0, [texture]).unwrap();
        let clip = assets.insert_runtime(clip);
        let video = VideoServer::new(assets);
        let mut world = World::new();

        let _ = video.play(
            clip,
            crate::video::VideoPlaybackSettings::default().playback_rate(0.0),
        );
        publish_video_diagnostics(&mut world, &video);
        publish_video_diagnostics(&mut world, &video);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let event = events
            .iter()
            .find(|event| event.code == "video.play.failed")
            .expect("video failed-play event should be emitted");

        assert_eq!(
            events
                .iter()
                .filter(|event| event.code == "video.play.failed")
                .count(),
            1
        );
        assert_eq!(event.severity, DiagnosticSeverity::Warning);
        assert_eq!(field(event, "failed_delta"), Some("1"));
        assert_eq!(field(event, "failed_play_requests"), Some("1"));
        assert!(field(event, "last_play_failure")
            .is_some_and(|message| message.contains("Video playback rate")));
    }
}
