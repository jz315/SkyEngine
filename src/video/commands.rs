use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::asset::Handle;
use crate::video::assets::VideoClip;
use crate::video::types::{VideoInstanceId, VideoPlaybackSettings};

#[derive(Clone)]
pub struct VideoCommands {
    queue: Arc<Mutex<Vec<VideoCommand>>>,
    next_instance: Arc<AtomicU64>,
}

impl std::fmt::Debug for VideoCommands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoCommands").finish_non_exhaustive()
    }
}

impl VideoCommands {
    pub(crate) fn new(next_instance: Arc<AtomicU64>) -> Self {
        Self {
            queue: Arc::new(Mutex::new(Vec::new())),
            next_instance,
        }
    }

    pub fn play(
        &self,
        clip: Handle<VideoClip>,
        settings: VideoPlaybackSettings,
    ) -> VideoInstanceId {
        let instance = self.reserve_instance();
        self.queue
            .lock()
            .expect("video commands mutex poisoned")
            .push(VideoCommand::Play {
                instance,
                clip,
                settings,
            });
        instance
    }

    pub fn stop(&self, instance: VideoInstanceId) {
        self.queue
            .lock()
            .expect("video commands mutex poisoned")
            .push(VideoCommand::Stop { instance });
    }

    pub fn pause(&self, instance: VideoInstanceId) {
        self.queue
            .lock()
            .expect("video commands mutex poisoned")
            .push(VideoCommand::Pause { instance });
    }

    pub fn resume(&self, instance: VideoInstanceId) {
        self.queue
            .lock()
            .expect("video commands mutex poisoned")
            .push(VideoCommand::Resume { instance });
    }

    pub fn seek(&self, instance: VideoInstanceId, seconds: f64) {
        self.queue
            .lock()
            .expect("video commands mutex poisoned")
            .push(VideoCommand::Seek { instance, seconds });
    }

    pub fn set_looped(&self, instance: VideoInstanceId, looped: bool) {
        self.queue
            .lock()
            .expect("video commands mutex poisoned")
            .push(VideoCommand::SetLooped { instance, looped });
    }

    pub fn set_playback_rate(&self, instance: VideoInstanceId, playback_rate: f32) {
        self.queue
            .lock()
            .expect("video commands mutex poisoned")
            .push(VideoCommand::SetPlaybackRate {
                instance,
                playback_rate,
            });
    }

    pub(crate) fn drain(&self) -> Vec<VideoCommand> {
        let mut queue = self.queue.lock().expect("video commands mutex poisoned");
        std::mem::take(&mut *queue)
    }

    pub(crate) fn reserve_instance(&self) -> VideoInstanceId {
        VideoInstanceId(self.next_instance.fetch_add(1, Ordering::Relaxed))
    }
}

pub(crate) enum VideoCommand {
    Play {
        instance: VideoInstanceId,
        clip: Handle<VideoClip>,
        settings: VideoPlaybackSettings,
    },
    Stop {
        instance: VideoInstanceId,
    },
    Pause {
        instance: VideoInstanceId,
    },
    Resume {
        instance: VideoInstanceId,
    },
    Seek {
        instance: VideoInstanceId,
        seconds: f64,
    },
    SetLooped {
        instance: VideoInstanceId,
        looped: bool,
    },
    SetPlaybackRate {
        instance: VideoInstanceId,
        playback_rate: f32,
    },
}
