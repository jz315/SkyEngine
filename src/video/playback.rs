use std::collections::VecDeque;

use crate::video::types::{VideoError, VideoPlaybackState};

const DUE_EPSILON_SECONDS: f64 = 0.000_5;

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedVideoFrame {
    pub pts_seconds: f64,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl DecodedVideoFrame {
    pub fn new(
        pts_seconds: f64,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<Self, VideoError> {
        validate_frame_timestamp(pts_seconds)?;
        let expected = rgba_len(width, height)?;
        if rgba.len() != expected {
            return Err(VideoError::InvalidFrameDataLength {
                expected,
                actual: rgba.len(),
            });
        }
        Ok(Self {
            pts_seconds,
            width,
            height,
            rgba,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoFrameSelection {
    pub frame: Option<DecodedVideoFrame>,
    pub dropped_late_frames: usize,
}

#[derive(Clone, Debug)]
pub struct VideoFrameQueue {
    frames: VecDeque<DecodedVideoFrame>,
    capacity: usize,
    end_of_stream: bool,
    dropped_over_capacity: u64,
}

impl VideoFrameQueue {
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            frames: VecDeque::with_capacity(capacity.max(1)),
            capacity: capacity.max(1),
            end_of_stream: false,
            dropped_over_capacity: 0,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub fn is_end_of_stream(&self) -> bool {
        self.end_of_stream
    }

    #[must_use]
    pub fn dropped_over_capacity(&self) -> u64 {
        self.dropped_over_capacity
    }

    pub fn push(&mut self, frame: DecodedVideoFrame) -> usize {
        self.end_of_stream = false;
        let mut dropped = 0;
        while self.frames.len() >= self.capacity {
            self.frames.pop_front();
            dropped += 1;
        }
        self.dropped_over_capacity += dropped as u64;
        self.frames.push_back(frame);
        dropped
    }

    pub fn try_push(&mut self, frame: DecodedVideoFrame) -> Result<(), DecodedVideoFrame> {
        if self.frames.len() >= self.capacity {
            return Err(frame);
        }
        self.end_of_stream = false;
        self.frames.push_back(frame);
        Ok(())
    }

    pub fn take_due(&mut self, playback_seconds: f64) -> VideoFrameSelection {
        if !playback_seconds.is_finite() {
            return VideoFrameSelection {
                frame: None,
                dropped_late_frames: 0,
            };
        }

        let mut selected = None;
        let mut due_count = 0usize;
        while self
            .frames
            .front()
            .is_some_and(|frame| frame.pts_seconds <= playback_seconds + DUE_EPSILON_SECONDS)
        {
            selected = self.frames.pop_front();
            due_count += 1;
        }

        VideoFrameSelection {
            frame: selected,
            dropped_late_frames: due_count.saturating_sub(1),
        }
    }

    #[must_use]
    pub fn next_pts_seconds(&self) -> Option<f64> {
        self.frames.front().map(|frame| frame.pts_seconds)
    }

    pub fn mark_end_of_stream(&mut self) {
        self.end_of_stream = true;
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.end_of_stream = false;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoClock {
    position_seconds: f64,
    playback_rate: f32,
    state: VideoPlaybackState,
}

impl VideoClock {
    pub fn new(playback_rate: f32, start_paused: bool) -> Result<Self, VideoError> {
        validate_playback_rate(playback_rate)?;
        Ok(Self {
            position_seconds: 0.0,
            playback_rate,
            state: if start_paused {
                VideoPlaybackState::Paused
            } else {
                VideoPlaybackState::Playing
            },
        })
    }

    #[must_use]
    pub fn position_seconds(self) -> f64 {
        self.position_seconds
    }

    #[must_use]
    pub fn playback_rate(self) -> f32 {
        self.playback_rate
    }

    #[must_use]
    pub fn state(self) -> VideoPlaybackState {
        self.state
    }

    pub fn tick(&mut self, dt_seconds: f32) {
        if self.state != VideoPlaybackState::Playing || !dt_seconds.is_finite() || dt_seconds <= 0.0
        {
            return;
        }
        self.position_seconds += dt_seconds as f64 * self.playback_rate as f64;
    }

    pub fn seek(&mut self, seconds: f64) -> Result<(), VideoError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(VideoError::InvalidSeekTime { seconds });
        }
        self.position_seconds = seconds;
        if self.state == VideoPlaybackState::Finished {
            self.state = VideoPlaybackState::Paused;
        }
        Ok(())
    }

    pub fn play(&mut self) {
        if matches!(
            self.state,
            VideoPlaybackState::Paused | VideoPlaybackState::Finished
        ) {
            self.state = VideoPlaybackState::Playing;
        }
    }

    pub fn pause(&mut self) {
        if self.state == VideoPlaybackState::Playing {
            self.state = VideoPlaybackState::Paused;
        }
    }

    pub fn stop(&mut self) {
        self.position_seconds = 0.0;
        self.state = VideoPlaybackState::Stopped;
    }

    pub fn finish(&mut self) {
        self.state = VideoPlaybackState::Finished;
    }

    pub fn set_playback_rate(&mut self, playback_rate: f32) -> Result<(), VideoError> {
        validate_playback_rate(playback_rate)?;
        self.playback_rate = playback_rate;
        Ok(())
    }
}

pub(crate) fn validate_playback_rate(playback_rate: f32) -> Result<(), VideoError> {
    if !playback_rate.is_finite() || playback_rate <= 0.0 {
        return Err(VideoError::InvalidPlaybackRate { playback_rate });
    }
    Ok(())
}

pub(crate) fn validate_frame_timestamp(seconds: f64) -> Result<(), VideoError> {
    if !seconds.is_finite() {
        return Err(VideoError::InvalidFrameTimestamp { seconds });
    }
    Ok(())
}

pub(crate) fn rgba_len(width: u32, height: u32) -> Result<usize, VideoError> {
    if width == 0 || height == 0 {
        return Err(VideoError::InvalidFrameDimensions { width, height });
    }
    let pixels = width
        .checked_mul(height)
        .and_then(|value| value.checked_mul(4))
        .ok_or(VideoError::InvalidFrameDimensions { width, height })?;
    Ok(pixels as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(pts_seconds: f64) -> DecodedVideoFrame {
        DecodedVideoFrame::new(pts_seconds, 1, 1, vec![255, 0, 0, 255]).unwrap()
    }

    #[test]
    fn queue_drops_oldest_when_capacity_is_exceeded() {
        let mut queue = VideoFrameQueue::with_capacity(2);
        assert_eq!(queue.push(frame(0.0)), 0);
        assert_eq!(queue.push(frame(0.1)), 0);
        assert_eq!(queue.push(frame(0.2)), 1);

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.dropped_over_capacity(), 1);
        assert_eq!(queue.next_pts_seconds(), Some(0.1));
    }

    #[test]
    fn try_push_preserves_existing_frames_when_full() {
        let mut queue = VideoFrameQueue::with_capacity(2);
        queue.try_push(frame(0.0)).unwrap();
        queue.try_push(frame(0.1)).unwrap();

        let returned = queue.try_push(frame(0.2)).unwrap_err();
        assert_eq!(returned.pts_seconds, 0.2);
        assert_eq!(queue.next_pts_seconds(), Some(0.0));
    }

    #[test]
    fn take_due_returns_latest_due_frame_and_drops_late_frames() {
        let mut queue = VideoFrameQueue::with_capacity(8);
        queue.push(frame(0.0));
        queue.push(frame(0.033));
        queue.push(frame(0.066));
        queue.push(frame(0.100));

        let selection = queue.take_due(0.070);
        assert_eq!(selection.frame.unwrap().pts_seconds, 0.066);
        assert_eq!(selection.dropped_late_frames, 2);
        assert_eq!(queue.next_pts_seconds(), Some(0.100));
    }

    #[test]
    fn clock_respects_pause_seek_and_playback_rate() {
        let mut clock = VideoClock::new(2.0, true).unwrap();
        clock.tick(1.0);
        assert_eq!(clock.position_seconds(), 0.0);

        clock.play();
        clock.tick(0.25);
        assert_eq!(clock.position_seconds(), 0.5);

        clock.seek(3.0).unwrap();
        assert_eq!(clock.position_seconds(), 3.0);

        clock.pause();
        clock.tick(1.0);
        assert_eq!(clock.position_seconds(), 3.0);
    }
}
