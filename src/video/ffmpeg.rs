use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ffmpeg_next::{
    self as ffmpeg,
    format::Pixel,
    media::Type as MediaType,
    software::scaling::{context::Context as ScalingContext, flag::Flags as ScalingFlags},
    util::frame::video::Video as FfmpegVideoFrame,
};

use crate::gpu::GpuContext;
use crate::render::expert::Texture;
use crate::video::playback::{
    validate_playback_rate, DecodedVideoFrame, VideoClock, VideoFrameQueue,
};
use crate::video::{GpuVideoFrameBuffer, VideoError, VideoPlaybackState};

const DEFAULT_QUEUE_CAPACITY: usize = 8;
const DEFAULT_CHANNEL_CAPACITY: usize = 16;
const DEFAULT_FRAME_RATE: f64 = 30.0;
const SEEK_TIME_BASE: f64 = 1_000_000.0;

#[derive(Clone, Debug, PartialEq)]
pub struct FfmpegVideoMetadata {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub duration_seconds: Option<f64>,
    pub frame_rate: Option<f64>,
}

impl FfmpegVideoMetadata {
    pub fn probe(path: impl AsRef<Path>) -> Result<Self, VideoError> {
        probe_video(path.as_ref())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FfmpegVideoOptions {
    pub looped: bool,
    pub playback_rate: f32,
    pub start_paused: bool,
    pub queue_capacity: usize,
    pub decode_channel_capacity: usize,
    pub texture_format: wgpu::TextureFormat,
}

impl FfmpegVideoOptions {
    #[must_use]
    pub fn looped(mut self, looped: bool) -> Self {
        self.looped = looped;
        self
    }

    #[must_use]
    pub fn playback_rate(mut self, playback_rate: f32) -> Self {
        self.playback_rate = playback_rate;
        self
    }

    #[must_use]
    pub fn start_paused(mut self, start_paused: bool) -> Self {
        self.start_paused = start_paused;
        self
    }

    #[must_use]
    pub fn queue_capacity(mut self, queue_capacity: usize) -> Self {
        self.queue_capacity = queue_capacity.max(1);
        self
    }

    #[must_use]
    pub fn decode_channel_capacity(mut self, decode_channel_capacity: usize) -> Self {
        self.decode_channel_capacity = decode_channel_capacity.max(1);
        self
    }

    #[must_use]
    pub fn texture_format(mut self, texture_format: wgpu::TextureFormat) -> Self {
        self.texture_format = texture_format;
        self
    }
}

impl Default for FfmpegVideoOptions {
    fn default() -> Self {
        Self {
            looped: false,
            playback_rate: 1.0,
            start_paused: false,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            decode_channel_capacity: DEFAULT_CHANNEL_CAPACITY,
            texture_format: wgpu::TextureFormat::Rgba8UnormSrgb,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FfmpegVideoUpdate {
    pub state: VideoPlaybackState,
    pub uploaded_frame: bool,
    pub uploaded_pts_seconds: Option<f64>,
    pub dropped_late_frames: usize,
    pub dropped_over_capacity: usize,
    pub queued_frames: usize,
    pub end_of_stream: bool,
}

pub struct FfmpegVideoPlayer {
    metadata: FfmpegVideoMetadata,
    options: FfmpegVideoOptions,
    clock: VideoClock,
    queue: VideoFrameQueue,
    frame: GpuVideoFrameBuffer,
    decoder: Option<DecoderThread>,
    events: Option<Receiver<DecoderEvent>>,
    pending_event: Option<DecoderEvent>,
    last_uploaded_pts_seconds: Option<f64>,
}

impl FfmpegVideoPlayer {
    pub fn open(gpu: &GpuContext, path: impl AsRef<Path>) -> Result<Self, VideoError> {
        Self::open_with_options(gpu, path, FfmpegVideoOptions::default())
    }

    pub fn open_with_options(
        gpu: &GpuContext,
        path: impl AsRef<Path>,
        options: FfmpegVideoOptions,
    ) -> Result<Self, VideoError> {
        validate_playback_rate(options.playback_rate)?;
        let metadata = probe_video(path.as_ref())?;
        let frame =
            GpuVideoFrameBuffer::new(gpu, metadata.width, metadata.height, options.texture_format)?;
        let clock = VideoClock::new(options.playback_rate, options.start_paused)?;
        let queue = VideoFrameQueue::with_capacity(options.queue_capacity);
        let mut player = Self {
            metadata,
            options,
            clock,
            queue,
            frame,
            decoder: None,
            events: None,
            pending_event: None,
            last_uploaded_pts_seconds: None,
        };
        player.restart_decoder(0.0)?;
        Ok(player)
    }

    #[must_use]
    pub fn metadata(&self) -> &FfmpegVideoMetadata {
        &self.metadata
    }

    #[must_use]
    pub fn options(&self) -> FfmpegVideoOptions {
        self.options
    }

    #[must_use]
    pub fn texture(&self) -> &Texture {
        self.frame.texture()
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.frame.width()
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.frame.height()
    }

    #[must_use]
    pub fn state(&self) -> VideoPlaybackState {
        self.clock.state()
    }

    #[must_use]
    pub fn position_seconds(&self) -> f64 {
        self.clock.position_seconds()
    }

    #[must_use]
    pub fn last_uploaded_pts_seconds(&self) -> Option<f64> {
        self.last_uploaded_pts_seconds
    }

    pub fn update(
        &mut self,
        gpu: &GpuContext,
        dt_seconds: f32,
    ) -> Result<FfmpegVideoUpdate, VideoError> {
        if self.state() == VideoPlaybackState::Stopped {
            return Ok(self.update_result(false, None, 0, 0));
        }

        let dropped_over_capacity = self.drain_decoder_events()?;
        self.clock.tick(dt_seconds);

        let selection = self.queue.take_due(self.clock.position_seconds());
        let mut uploaded_pts_seconds = None;
        let dropped_late_frames = selection.dropped_late_frames;
        if let Some(frame) = selection.frame {
            uploaded_pts_seconds = Some(frame.pts_seconds);
            self.upload_frame(gpu, frame)?;
        }

        if self.queue.is_end_of_stream() && self.queue.is_empty() {
            if self.options.looped {
                self.clock.seek(0.0)?;
                self.clock.play();
                self.queue.clear();
                self.restart_decoder(0.0)?;
            } else {
                self.clock.finish();
                self.stop_decoder()?;
            }
        }

        Ok(self.update_result(
            uploaded_pts_seconds.is_some(),
            uploaded_pts_seconds,
            dropped_late_frames,
            dropped_over_capacity,
        ))
    }

    pub fn play(&mut self) -> Result<(), VideoError> {
        if matches!(
            self.clock.state(),
            VideoPlaybackState::Stopped | VideoPlaybackState::Finished
        ) {
            self.clock.seek(0.0)?;
            self.clock.play();
            self.queue.clear();
            self.restart_decoder(0.0)?;
        } else {
            self.clock.play();
            if self.decoder.is_none() {
                self.restart_decoder(self.clock.position_seconds())?;
            }
        }
        Ok(())
    }

    pub fn pause(&mut self) {
        self.clock.pause();
    }

    pub fn stop(&mut self) -> Result<(), VideoError> {
        self.clock.stop();
        self.queue.clear();
        self.pending_event = None;
        self.last_uploaded_pts_seconds = None;
        self.stop_decoder()
    }

    pub fn seek(&mut self, seconds: f64) -> Result<(), VideoError> {
        self.clock.seek(seconds)?;
        self.queue.clear();
        self.last_uploaded_pts_seconds = None;
        if self.clock.state() != VideoPlaybackState::Stopped {
            self.restart_decoder(seconds)?;
        }
        Ok(())
    }

    pub fn set_looped(&mut self, looped: bool) {
        self.options.looped = looped;
    }

    pub fn set_playback_rate(&mut self, playback_rate: f32) -> Result<(), VideoError> {
        self.clock.set_playback_rate(playback_rate)?;
        self.options.playback_rate = playback_rate;
        Ok(())
    }

    fn update_result(
        &self,
        uploaded_frame: bool,
        uploaded_pts_seconds: Option<f64>,
        dropped_late_frames: usize,
        dropped_over_capacity: usize,
    ) -> FfmpegVideoUpdate {
        FfmpegVideoUpdate {
            state: self.clock.state(),
            uploaded_frame,
            uploaded_pts_seconds,
            dropped_late_frames,
            dropped_over_capacity,
            queued_frames: self.queue.len(),
            end_of_stream: self.queue.is_end_of_stream(),
        }
    }

    fn upload_frame(
        &mut self,
        gpu: &GpuContext,
        decoded: DecodedVideoFrame,
    ) -> Result<(), VideoError> {
        if self.frame.width() != decoded.width || self.frame.height() != decoded.height {
            self.frame = GpuVideoFrameBuffer::new(
                gpu,
                decoded.width,
                decoded.height,
                self.options.texture_format,
            )?;
        }
        self.frame.write_rgba8(gpu, &decoded.rgba)?;
        self.last_uploaded_pts_seconds = Some(decoded.pts_seconds);
        Ok(())
    }

    fn drain_decoder_events(&mut self) -> Result<usize, VideoError> {
        let mut decoder_disconnected = false;
        let dropped_over_capacity = 0usize;

        if let Some(event) = self.pending_event.take() {
            if let Some(event) = self.apply_decoder_event(event)? {
                self.pending_event = Some(event);
                return Ok(dropped_over_capacity);
            }
        }

        loop {
            let event = match self.events.as_ref() {
                Some(events) => events.try_recv(),
                None => break,
            };
            match event {
                Ok(event) => {
                    if let Some(event) = self.apply_decoder_event(event)? {
                        self.pending_event = Some(event);
                        break;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    decoder_disconnected = true;
                    break;
                }
            }
        }

        if decoder_disconnected && self.decoder.is_some() {
            self.queue.mark_end_of_stream();
        }
        Ok(dropped_over_capacity)
    }

    fn restart_decoder(&mut self, start_at_seconds: f64) -> Result<(), VideoError> {
        self.stop_decoder()?;
        self.pending_event = None;
        let (decoder, events) = spawn_decoder(
            self.metadata.path.clone(),
            start_at_seconds,
            self.options.decode_channel_capacity,
        )?;
        self.decoder = Some(decoder);
        self.events = Some(events);
        Ok(())
    }

    fn stop_decoder(&mut self) -> Result<(), VideoError> {
        self.events = None;
        if let Some(mut decoder) = self.decoder.take() {
            decoder.stop()?;
        }
        Ok(())
    }

    fn apply_decoder_event(
        &mut self,
        event: DecoderEvent,
    ) -> Result<Option<DecoderEvent>, VideoError> {
        match event {
            DecoderEvent::Frame(frame) => match self.queue.try_push(frame) {
                Ok(()) => Ok(None),
                Err(frame) => Ok(Some(DecoderEvent::Frame(frame))),
            },
            DecoderEvent::EndOfStream => {
                self.queue.mark_end_of_stream();
                Ok(None)
            }
            DecoderEvent::Error(message) => Err(VideoError::Decode { message }),
        }
    }
}

impl Drop for FfmpegVideoPlayer {
    fn drop(&mut self) {
        let _ = self.stop_decoder();
    }
}

enum DecoderControl {
    Stop,
}

enum DecoderEvent {
    Frame(DecodedVideoFrame),
    EndOfStream,
    Error(String),
}

struct DecoderThread {
    control: std::sync::mpsc::Sender<DecoderControl>,
    join: Option<JoinHandle<()>>,
}

impl DecoderThread {
    fn stop(&mut self) -> Result<(), VideoError> {
        let _ = self.control.send(DecoderControl::Stop);
        if let Some(join) = self.join.take() {
            join.join().map_err(|_| VideoError::DecoderThreadPanic)?;
        }
        Ok(())
    }
}

impl Drop for DecoderThread {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn spawn_decoder(
    path: PathBuf,
    start_at_seconds: f64,
    channel_capacity: usize,
) -> Result<(DecoderThread, Receiver<DecoderEvent>), VideoError> {
    let (event_tx, event_rx) = sync_channel(channel_capacity.max(1));
    let (control_tx, control_rx) = std::sync::mpsc::channel();
    let join = thread::Builder::new()
        .name("sky_video_ffmpeg_decoder".to_string())
        .spawn({
            let path = path.clone();
            move || match decode_video_stream(&path, start_at_seconds, &event_tx, &control_rx) {
                Ok(DecodeExit::Finished) => {
                    let _ = send_decoder_event(&event_tx, &control_rx, DecoderEvent::EndOfStream);
                }
                Ok(DecodeExit::Stopped) => {}
                Err(error) => {
                    let _ = send_decoder_event(
                        &event_tx,
                        &control_rx,
                        DecoderEvent::Error(error.to_string()),
                    );
                }
            }
        })
        .map_err(|error| VideoError::Decode {
            message: format!("failed to spawn ffmpeg decoder thread: {error}"),
        })?;

    Ok((
        DecoderThread {
            control: control_tx,
            join: Some(join),
        },
        event_rx,
    ))
}

enum DecodeExit {
    Finished,
    Stopped,
}

fn decode_video_stream(
    path: &Path,
    start_at_seconds: f64,
    events: &SyncSender<DecoderEvent>,
    control: &Receiver<DecoderControl>,
) -> Result<DecodeExit, VideoError> {
    ffmpeg::init().map_err(map_ffmpeg_error)?;
    let mut input = ffmpeg::format::input(path).map_err(map_ffmpeg_error)?;

    let (video_stream_index, time_base, stream_start_seconds, frame_interval, mut decoder) = {
        let stream = input
            .streams()
            .best(MediaType::Video)
            .ok_or_else(|| VideoError::Decode {
                message: "input has no video stream".to_string(),
            })?;
        let stream_time_base = stream.time_base();
        let stream_start_seconds = stream_start_seconds(stream.start_time(), stream_time_base);
        let frame_rate = rational_to_f64(stream.avg_frame_rate())
            .or_else(|| rational_to_f64(stream.rate()))
            .unwrap_or(DEFAULT_FRAME_RATE);
        let context_decoder = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
            .map_err(map_ffmpeg_error)?;
        (
            stream.index(),
            stream_time_base,
            stream_start_seconds,
            1.0 / frame_rate.max(0.000_1),
            context_decoder
                .decoder()
                .video()
                .map_err(map_ffmpeg_error)?,
        )
    };

    if start_at_seconds > 0.0 {
        let seek_position = (start_at_seconds * SEEK_TIME_BASE).round() as i64;
        input
            .seek(seek_position, ..seek_position)
            .map_err(map_ffmpeg_error)?;
        decoder.flush();
    }

    let mut scaler = None;
    let mut rgba_frame = FfmpegVideoFrame::empty();
    let mut next_fallback_pts = start_at_seconds.max(0.0);

    for (stream, packet) in input.packets() {
        if stop_requested(control) {
            return Ok(DecodeExit::Stopped);
        }
        if stream.index() != video_stream_index {
            continue;
        }

        decoder.send_packet(&packet).map_err(map_ffmpeg_error)?;
        if !receive_and_send_frames(
            &mut decoder,
            &mut scaler,
            &mut rgba_frame,
            time_base,
            stream_start_seconds,
            frame_interval,
            &mut next_fallback_pts,
            start_at_seconds,
            events,
            control,
        )? {
            return Ok(DecodeExit::Stopped);
        }
    }

    decoder.send_eof().map_err(map_ffmpeg_error)?;
    if !receive_and_send_frames(
        &mut decoder,
        &mut scaler,
        &mut rgba_frame,
        time_base,
        stream_start_seconds,
        frame_interval,
        &mut next_fallback_pts,
        start_at_seconds,
        events,
        control,
    )? {
        return Ok(DecodeExit::Stopped);
    }

    Ok(DecodeExit::Finished)
}

#[allow(clippy::too_many_arguments)]
fn receive_and_send_frames(
    decoder: &mut ffmpeg::decoder::Video,
    scaler: &mut Option<ScalingContext>,
    rgba_frame: &mut FfmpegVideoFrame,
    time_base: ffmpeg::Rational,
    stream_start_seconds: f64,
    frame_interval: f64,
    next_fallback_pts: &mut f64,
    start_at_seconds: f64,
    events: &SyncSender<DecoderEvent>,
    control: &Receiver<DecoderControl>,
) -> Result<bool, VideoError> {
    loop {
        if stop_requested(control) {
            return Ok(false);
        }

        let mut decoded = FfmpegVideoFrame::empty();
        match decoder.receive_frame(&mut decoded) {
            Ok(()) => {}
            Err(ffmpeg::Error::Other {
                errno: ffmpeg::error::EAGAIN,
            }) => return Ok(true),
            Err(ffmpeg::Error::Eof) => return Ok(true),
            Err(error) => return Err(map_ffmpeg_error(error)),
        }

        let mut pts_seconds = frame_pts_seconds(&decoded, time_base, stream_start_seconds)
            .unwrap_or(*next_fallback_pts);
        if pts_seconds < 0.0 {
            pts_seconds = 0.0;
        }
        *next_fallback_pts = pts_seconds + frame_interval;

        if pts_seconds + frame_interval < start_at_seconds {
            continue;
        }

        ensure_scaler(scaler, rgba_frame, &decoded)?;
        scaler
            .as_mut()
            .expect("scaler should exist after ensure_scaler")
            .run(&decoded, rgba_frame)
            .map_err(map_ffmpeg_error)?;

        let rgba = copy_rgba_frame(rgba_frame)?;
        let frame =
            DecodedVideoFrame::new(pts_seconds, rgba_frame.width(), rgba_frame.height(), rgba)?;
        if !send_decoder_event(events, control, DecoderEvent::Frame(frame)) {
            return Ok(false);
        }
    }
}

fn ensure_scaler(
    scaler: &mut Option<ScalingContext>,
    rgba_frame: &mut FfmpegVideoFrame,
    decoded: &FfmpegVideoFrame,
) -> Result<(), VideoError> {
    let needs_new = scaler.as_ref().is_none_or(|scaler| {
        let input = scaler.input();
        input.format != decoded.format()
            || input.width != decoded.width()
            || input.height != decoded.height()
    });
    if needs_new {
        *scaler = Some(
            ScalingContext::get(
                decoded.format(),
                decoded.width(),
                decoded.height(),
                Pixel::RGBA,
                decoded.width(),
                decoded.height(),
                ScalingFlags::BILINEAR,
            )
            .map_err(map_ffmpeg_error)?,
        );
        *rgba_frame = FfmpegVideoFrame::empty();
    }
    Ok(())
}

fn copy_rgba_frame(frame: &FfmpegVideoFrame) -> Result<Vec<u8>, VideoError> {
    let width = frame.width();
    let height = frame.height();
    let row_bytes = width as usize * 4;
    let stride = frame.stride(0);
    if stride < row_bytes {
        return Err(VideoError::Decode {
            message: format!("decoded RGBA stride {stride} is smaller than row size {row_bytes}"),
        });
    }

    let data = frame.data(0);
    let mut rgba = vec![0; row_bytes * height as usize];
    for row in 0..height as usize {
        let src_start = row * stride;
        let src_end = src_start + row_bytes;
        let dst_start = row * row_bytes;
        let dst_end = dst_start + row_bytes;
        rgba[dst_start..dst_end].copy_from_slice(&data[src_start..src_end]);
    }
    Ok(rgba)
}

fn probe_video(path: &Path) -> Result<FfmpegVideoMetadata, VideoError> {
    ffmpeg::init().map_err(map_ffmpeg_error)?;
    let input = ffmpeg::format::input(path).map_err(map_ffmpeg_error)?;
    let container_duration = input.duration();
    let stream = input
        .streams()
        .best(MediaType::Video)
        .ok_or_else(|| VideoError::Decode {
            message: "input has no video stream".to_string(),
        })?;
    let context_decoder = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
        .map_err(map_ffmpeg_error)?;
    let decoder = context_decoder
        .decoder()
        .video()
        .map_err(map_ffmpeg_error)?;
    let time_base = stream.time_base();
    let stream_duration = stream.duration();

    Ok(FfmpegVideoMetadata {
        path: path.to_path_buf(),
        width: decoder.width(),
        height: decoder.height(),
        duration_seconds: stream_duration_seconds(stream_duration, time_base)
            .or_else(|| container_duration_seconds(container_duration)),
        frame_rate: rational_to_f64(stream.avg_frame_rate())
            .or_else(|| rational_to_f64(stream.rate()))
            .or_else(|| decoder.frame_rate().and_then(rational_to_f64)),
    })
}

fn send_decoder_event(
    sender: &SyncSender<DecoderEvent>,
    control: &Receiver<DecoderControl>,
    mut event: DecoderEvent,
) -> bool {
    loop {
        if stop_requested(control) {
            return false;
        }
        match sender.try_send(event) {
            Ok(()) => return true,
            Err(TrySendError::Full(returned)) => {
                event = returned;
                thread::sleep(Duration::from_millis(1));
            }
            Err(TrySendError::Disconnected(_)) => return false,
        }
    }
}

fn stop_requested(control: &Receiver<DecoderControl>) -> bool {
    match control.try_recv() {
        Ok(DecoderControl::Stop) | Err(TryRecvError::Disconnected) => true,
        Err(TryRecvError::Empty) => false,
    }
}

fn frame_pts_seconds(
    frame: &FfmpegVideoFrame,
    time_base: ffmpeg::Rational,
    stream_start_seconds: f64,
) -> Option<f64> {
    let time_base = rational_to_f64(time_base)?;
    let pts = frame.timestamp().or_else(|| frame.pts())?;
    let seconds = pts as f64 * time_base - stream_start_seconds;
    seconds.is_finite().then_some(seconds)
}

fn stream_start_seconds(start_time: i64, time_base: ffmpeg::Rational) -> f64 {
    if start_time <= 0 || start_time == ffmpeg::ffi::AV_NOPTS_VALUE {
        return 0.0;
    }
    rational_to_f64(time_base)
        .map(|time_base| start_time as f64 * time_base)
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
        .unwrap_or(0.0)
}

fn stream_duration_seconds(duration: i64, time_base: ffmpeg::Rational) -> Option<f64> {
    if duration <= 0 || duration == ffmpeg::ffi::AV_NOPTS_VALUE {
        return None;
    }
    rational_to_f64(time_base)
        .map(|time_base| duration as f64 * time_base)
        .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
}

fn container_duration_seconds(duration: i64) -> Option<f64> {
    if duration <= 0 || duration == ffmpeg::ffi::AV_NOPTS_VALUE {
        return None;
    }
    let seconds = duration as f64 / SEEK_TIME_BASE;
    (seconds.is_finite() && seconds > 0.0).then_some(seconds)
}

fn rational_to_f64(value: ffmpeg::Rational) -> Option<f64> {
    if value.numerator() <= 0 || value.denominator() <= 0 {
        return None;
    }
    let value = f64::from(value);
    (value.is_finite() && value > 0.0).then_some(value)
}

fn map_ffmpeg_error(error: ffmpeg::Error) -> VideoError {
    VideoError::Decode {
        message: error.to_string(),
    }
}
