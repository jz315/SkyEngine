//! Feature-gated profiling facade for SkyEngine.
//!
//! `sky_profile` keeps instrumentation cheap when profiling is disabled and
//! writes AI-readable JSONL plus a compact summary when `SKY_PROFILE=1`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SCHEMA_VERSION: u32 = 1;

thread_local! {
    static CURRENT_FRAME: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Runtime profiling configuration derived from environment variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileConfig {
    pub enabled: bool,
    pub json_enabled: bool,
    pub tracy_enabled: bool,
    pub gpu_enabled: bool,
    pub output_root: PathBuf,
    pub run_id: String,
    pub frame_limit: Option<u64>,
}

impl ProfileConfig {
    pub fn from_env() -> Self {
        let run_id = default_run_id();
        Self::from_env_lookup(run_id, |key| std::env::var(key).ok())
    }

    pub fn from_env_lookup(run_id: String, mut lookup: impl FnMut(&str) -> Option<String>) -> Self {
        let enabled = env_flag_lookup(&mut lookup, "SKY_PROFILE")
            .or_else(|| env_flag_lookup(&mut lookup, "SKY_APP_PROFILE"))
            .or_else(|| env_flag_lookup(&mut lookup, "SKY_APP_STARTUP_PROFILE"))
            .or_else(|| env_flag_lookup(&mut lookup, "SKY_GPU_PROFILE"))
            .unwrap_or(false);

        let backends = lookup("SKY_PROFILE_BACKENDS").unwrap_or_default();
        let requested_backends = parse_backend_list(&backends);
        let json_enabled = enabled
            && (requested_backends.is_empty() || requested_backends.iter().any(|b| b == "json"));
        let tracy_enabled = enabled
            && cfg!(feature = "tracy")
            && (requested_backends.is_empty() || requested_backends.iter().any(|b| b == "tracy"));
        let gpu_enabled =
            enabled && env_flag_lookup(&mut lookup, "SKY_PROFILE_GPU").unwrap_or(true);
        let output_root = lookup("SKY_PROFILE_OUTPUT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("target").join("sky-profile"));
        let frame_limit = lookup("SKY_PROFILE_FRAMES").and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                trimmed.parse::<u64>().ok()
            }
        });

        Self {
            enabled,
            json_enabled,
            tracy_enabled,
            gpu_enabled,
            output_root,
            run_id,
            frame_limit,
        }
    }

    pub fn run_dir(&self) -> PathBuf {
        self.output_root.join(&self.run_id)
    }
}

/// One completed CPU/GPU profiling scope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileEvent {
    pub schema_version: u32,
    pub run_id: String,
    pub frame: Option<u64>,
    pub thread: String,
    pub category: String,
    pub name: String,
    pub start_ns: u64,
    pub duration_ns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_duration_ns: Option<u64>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
}

impl ProfileEvent {
    pub fn new(
        run_id: String,
        frame: Option<u64>,
        category: impl Into<String>,
        name: impl Into<String>,
        start_ns: u64,
        duration_ns: u64,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            run_id,
            frame,
            thread: current_thread_name(),
            category: category.into(),
            name: name.into(),
            start_ns,
            duration_ns,
            gpu_duration_ns: None,
            metadata: Map::new(),
        }
    }

    pub fn with_gpu_duration_ns(mut self, gpu_duration_ns: u64) -> Self {
        self.gpu_duration_ns = Some(gpu_duration_ns);
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// One frame-level profiling record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileFrame {
    pub schema_version: u32,
    pub run_id: String,
    pub frame: u64,
    pub cpu_frame_ms: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_frame_ms: Option<f32>,
    pub draw_calls: usize,
    pub passes: usize,
    pub uploaded_render_assets: usize,
    pub uploaded_render_asset_bytes: usize,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub metadata: Map<String, Value>,
}

impl ProfileFrame {
    pub fn new(run_id: String, frame: u64) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            run_id,
            frame,
            cpu_frame_ms: 0.0,
            gpu_frame_ms: None,
            draw_calls: 0,
            passes: 0,
            uploaded_render_assets: 0,
            uploaded_render_asset_bytes: 0,
            metadata: Map::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileSummary {
    pub schema_version: u32,
    pub run_id: String,
    pub generated_unix_ms: u128,
    pub scopes: Vec<ProfileScopeSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfileScopeSummary {
    pub category: String,
    pub name: String,
    pub count: u64,
    pub avg_ns: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub max_ns: u64,
}

pub trait ProfileSink: Send + Sync + 'static {
    fn record_event(&self, event: &ProfileEvent);
    fn record_frame(&self, frame: &ProfileFrame);
    fn flush_summary(&self, summary: &ProfileSummary);
}

pub struct ScopeGuard {
    category: &'static str,
    name: String,
    start: Instant,
    start_ns: u64,
    enabled: bool,
    #[cfg(feature = "tracy")]
    _tracy_span: Option<profiling::tracy_client::Span>,
}

impl ScopeGuard {
    pub fn new(
        category: &'static str,
        name: impl Into<String>,
        file: &'static str,
        line: u32,
    ) -> Self {
        let name = name.into();
        let profiler = profiler();
        let enabled = profiler.enabled();
        let start = Instant::now();
        let start_ns = profiler.elapsed_ns(start);
        #[cfg(feature = "tracy")]
        let tracy_span = profiler.start_tracy_span(&name, file, line);
        let _ = (file, line);
        Self {
            category,
            name,
            start,
            start_ns,
            enabled,
            #[cfg(feature = "tracy")]
            _tracy_span: tracy_span,
        }
    }

    pub fn disabled() -> Self {
        Self {
            category: "",
            name: String::new(),
            start: Instant::now(),
            start_ns: 0,
            enabled: false,
            #[cfg(feature = "tracy")]
            _tracy_span: None,
        }
    }
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        if !self.enabled {
            return;
        }
        let duration_ns = nanos_u64(self.start.elapsed());
        let frame = current_frame();
        let event = profiler().make_event(
            frame,
            self.category,
            self.name.clone(),
            self.start_ns,
            duration_ns,
        );
        record_event(event);
    }
}

pub struct FrameGuard {
    previous: Option<u64>,
}

impl FrameGuard {
    pub fn new(frame: u64) -> Self {
        let previous = CURRENT_FRAME.with(|slot| {
            let previous = slot.get();
            slot.set(Some(frame));
            previous
        });
        Self { previous }
    }
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        CURRENT_FRAME.with(|slot| slot.set(self.previous));
    }
}

#[macro_export]
macro_rules! profile_scope {
    ($category:expr, $name:expr) => {
        if $crate::enabled() {
            $crate::ScopeGuard::new($category, $name, file!(), line!())
        } else {
            $crate::ScopeGuard::disabled()
        }
    };
}

#[macro_export]
macro_rules! profile_frame {
    ($frame:expr) => {
        $crate::FrameGuard::new($frame)
    };
}

#[macro_export]
macro_rules! profile_counter {
    ($category:expr, $name:expr, $value:expr) => {
        if $crate::enabled() {
            $crate::record_counter($category, $name, $value as f64)
        }
    };
}

pub fn enabled() -> bool {
    profiler().enabled()
}

pub fn config() -> &'static ProfileConfig {
    &profiler().config
}

pub fn current_frame() -> Option<u64> {
    CURRENT_FRAME.with(Cell::get)
}

pub fn record_event(event: ProfileEvent) {
    profiler().record_event(event);
}

pub fn record_gpu_event(
    category: &'static str,
    name: impl Into<String>,
    cpu_start_ns: u64,
    cpu_duration_ns: u64,
    gpu_duration_ns: u64,
) {
    if !enabled() {
        return;
    }
    let event = profiler()
        .make_event(
            current_frame(),
            category,
            name,
            cpu_start_ns,
            cpu_duration_ns,
        )
        .with_gpu_duration_ns(gpu_duration_ns);
    record_event(event);
}

pub fn record_counter(category: &'static str, name: impl Into<String>, value: f64) {
    if !enabled() {
        return;
    }
    let event = profiler()
        .make_event(
            current_frame(),
            category,
            name,
            profiler().elapsed_now_ns(),
            0,
        )
        .with_metadata("counter", value);
    record_event(event);
}

pub fn elapsed_ns_since_start(instant: Instant) -> u64 {
    profiler().elapsed_ns(instant)
}

pub fn record_frame(mut frame: ProfileFrame) {
    let profiler = profiler();
    if !profiler.enabled() || profiler.frame_limit_reached(frame.frame) {
        return;
    }
    frame.run_id = profiler.config.run_id.clone();
    profiler.record_frame(frame);
}

pub fn flush() {
    profiler().flush_summary();
}

pub fn run_id() -> String {
    profiler().config.run_id.clone()
}

struct Profiler {
    config: ProfileConfig,
    start: Instant,
    writer: Mutex<Option<JsonProfileWriter>>,
    summary: Mutex<SummaryAccumulator>,
    event_count: AtomicU64,
    frame_count: AtomicU64,
    #[cfg(feature = "tracy")]
    tracy_client: OnceLock<profiling::tracy_client::Client>,
}

impl Profiler {
    fn new() -> Self {
        let config = ProfileConfig::from_env();
        let writer = if config.enabled && config.json_enabled {
            match JsonProfileWriter::new(&config) {
                Ok(writer) => Some(writer),
                Err(error) => {
                    eprintln!("[SkyEngine][Profile] failed to create JSON profile writer: {error}");
                    None
                }
            }
        } else {
            None
        };
        Self {
            config,
            start: Instant::now(),
            writer: Mutex::new(writer),
            summary: Mutex::new(SummaryAccumulator::default()),
            event_count: AtomicU64::new(0),
            frame_count: AtomicU64::new(0),
            #[cfg(feature = "tracy")]
            tracy_client: OnceLock::new(),
        }
    }

    fn enabled(&self) -> bool {
        self.config.enabled
    }

    fn frame_limit_reached(&self, frame: u64) -> bool {
        self.config.frame_limit.is_some_and(|limit| frame >= limit)
    }

    fn make_event(
        &self,
        frame: Option<u64>,
        category: impl Into<String>,
        name: impl Into<String>,
        start_ns: u64,
        duration_ns: u64,
    ) -> ProfileEvent {
        ProfileEvent::new(
            self.config.run_id.clone(),
            frame,
            category,
            name,
            start_ns,
            duration_ns,
        )
    }

    fn elapsed_now_ns(&self) -> u64 {
        nanos_u64(self.start.elapsed())
    }

    fn elapsed_ns(&self, instant: Instant) -> u64 {
        let duration = instant.saturating_duration_since(self.start);
        nanos_u64(duration)
    }

    fn record_event(&self, event: ProfileEvent) {
        if !self.enabled() {
            return;
        }
        if event
            .frame
            .is_some_and(|frame| self.frame_limit_reached(frame))
        {
            return;
        }
        self.event_count.fetch_add(1, Ordering::Relaxed);
        self.summary.lock().unwrap().record(&event);
        if let Some(writer) = self.writer.lock().unwrap().as_mut() {
            writer.write_event(&event);
        }
        self.flush_summary();
    }

    fn record_frame(&self, frame: ProfileFrame) {
        if !self.enabled() {
            return;
        }
        self.frame_count.fetch_add(1, Ordering::Relaxed);
        if let Some(writer) = self.writer.lock().unwrap().as_mut() {
            writer.write_frame(&frame);
        }
        self.flush_summary();
    }

    fn flush_summary(&self) {
        let summary = self.summary.lock().unwrap().summary(&self.config.run_id);
        if let Some(writer) = self.writer.lock().unwrap().as_mut() {
            writer.write_summary(&summary);
            writer.flush();
        }
    }

    #[cfg(feature = "tracy")]
    fn start_tracy_span(
        &self,
        name: &str,
        file: &'static str,
        line: u32,
    ) -> Option<profiling::tracy_client::Span> {
        if !self.config.tracy_enabled {
            return None;
        }
        let client = self
            .tracy_client
            .get_or_init(profiling::tracy_client::Client::start)
            .clone();
        Some(client.span_alloc(Some(name), "sky_profile", file, line, 0))
    }
}

struct JsonProfileWriter {
    events: BufWriter<File>,
    frames: BufWriter<File>,
    summary_path: PathBuf,
}

impl JsonProfileWriter {
    fn new(config: &ProfileConfig) -> std::io::Result<Self> {
        let dir = config.run_dir();
        fs::create_dir_all(&dir)?;
        let events = BufWriter::new(File::create(dir.join("events.jsonl"))?);
        let frames = BufWriter::new(File::create(dir.join("frames.jsonl"))?);
        Ok(Self {
            events,
            frames,
            summary_path: dir.join("summary.json"),
        })
    }

    fn write_event(&mut self, event: &ProfileEvent) {
        if serde_json::to_writer(&mut self.events, event).is_ok() {
            let _ = self.events.write_all(b"\n");
        }
    }

    fn write_frame(&mut self, frame: &ProfileFrame) {
        if serde_json::to_writer(&mut self.frames, frame).is_ok() {
            let _ = self.frames.write_all(b"\n");
        }
    }

    fn write_summary(&mut self, summary: &ProfileSummary) {
        if let Ok(file) = File::create(&self.summary_path) {
            let mut writer = BufWriter::new(file);
            let _ = serde_json::to_writer_pretty(&mut writer, summary);
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
        }
    }

    fn flush(&mut self) {
        let _ = self.events.flush();
        let _ = self.frames.flush();
    }
}

#[derive(Default)]
struct SummaryAccumulator {
    durations_by_scope: BTreeMap<(String, String), Vec<u64>>,
}

impl SummaryAccumulator {
    fn record(&mut self, event: &ProfileEvent) {
        if event.duration_ns == 0 {
            return;
        }
        self.durations_by_scope
            .entry((event.category.clone(), event.name.clone()))
            .or_default()
            .push(event.duration_ns);
    }

    fn summary(&self, run_id: &str) -> ProfileSummary {
        let mut scopes = Vec::with_capacity(self.durations_by_scope.len());
        for ((category, name), durations) in &self.durations_by_scope {
            if durations.is_empty() {
                continue;
            }
            let mut sorted = durations.clone();
            sorted.sort_unstable();
            let sum = sorted.iter().copied().sum::<u64>();
            scopes.push(ProfileScopeSummary {
                category: category.clone(),
                name: name.clone(),
                count: sorted.len() as u64,
                avg_ns: sum / sorted.len() as u64,
                p50_ns: percentile(&sorted, 50),
                p95_ns: percentile(&sorted, 95),
                p99_ns: percentile(&sorted, 99),
                max_ns: sorted.last().copied().unwrap_or(0),
            });
        }
        ProfileSummary {
            schema_version: SCHEMA_VERSION,
            run_id: run_id.to_string(),
            generated_unix_ms: unix_ms(),
            scopes,
        }
    }
}

fn profiler() -> &'static Profiler {
    static PROFILER: OnceLock<Profiler> = OnceLock::new();
    PROFILER.get_or_init(Profiler::new)
}

fn env_flag_lookup(lookup: &mut impl FnMut(&str) -> Option<String>, key: &str) -> Option<bool> {
    lookup(key).map(|value| {
        let value = value.trim();
        !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
    })
}

fn parse_backend_list(backends: &str) -> Vec<String> {
    backends
        .split(',')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .collect()
}

fn default_run_id() -> String {
    format!("{}-{}", unix_ms(), std::process::id())
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn nanos_u64(duration: Duration) -> u64 {
    duration.as_nanos().min(u128::from(u64::MAX)) as u64
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() - 1) * percentile).div_ceil(100);
    sorted[index]
}

fn current_thread_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .map(str::to_string)
        .unwrap_or_else(|| format!("{:?}", thread.id()))
}

impl<T: ProfileSink> ProfileSink for std::sync::Arc<T> {
    fn record_event(&self, event: &ProfileEvent) {
        (**self).record_event(event);
    }

    fn record_frame(&self, frame: &ProfileFrame) {
        (**self).record_frame(frame);
    }

    fn flush_summary(&self, summary: &ProfileSummary) {
        (**self).flush_summary(summary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_parsing_defaults_to_json_and_optional_tracy() {
        let config = ProfileConfig::from_env_lookup("run".to_string(), |key| match key {
            "SKY_PROFILE" => Some("1".to_string()),
            _ => None,
        });

        assert!(config.enabled);
        assert!(config.json_enabled);
        assert_eq!(config.tracy_enabled, cfg!(feature = "tracy"));
        assert!(config.gpu_enabled);
        assert_eq!(
            config.output_root,
            std::path::Path::new("target").join("sky-profile")
        );
    }

    #[test]
    fn env_parsing_respects_backend_and_frame_overrides() {
        let config = ProfileConfig::from_env_lookup("run".to_string(), |key| match key {
            "SKY_PROFILE" => Some("true".to_string()),
            "SKY_PROFILE_BACKENDS" => Some("json".to_string()),
            "SKY_PROFILE_GPU" => Some("0".to_string()),
            "SKY_PROFILE_OUTPUT" => Some("profile-out".to_string()),
            "SKY_PROFILE_FRAMES" => Some("3".to_string()),
            _ => None,
        });

        assert!(config.enabled);
        assert!(config.json_enabled);
        assert!(!config.tracy_enabled);
        assert!(!config.gpu_enabled);
        assert_eq!(config.output_root, PathBuf::from("profile-out"));
        assert_eq!(config.frame_limit, Some(3));
    }

    #[test]
    fn summary_percentiles_are_serializable() {
        let mut summary = SummaryAccumulator::default();
        for duration in [10, 20, 30, 40, 50] {
            summary.record(&ProfileEvent::new(
                "run".to_string(),
                Some(1),
                "render",
                "execute",
                0,
                duration,
            ));
        }
        let summary = summary.summary("run");
        let scope = &summary.scopes[0];
        assert_eq!(scope.category, "render");
        assert_eq!(scope.name, "execute");
        assert_eq!(scope.count, 5);
        assert_eq!(scope.p50_ns, 30);
        assert_eq!(scope.p95_ns, 50);
        assert!(serde_json::to_string(&summary).unwrap().contains("execute"));
    }

    #[test]
    fn event_metadata_round_trips() {
        let event = ProfileEvent::new("run".to_string(), None, "ecs", "system", 10, 20)
            .with_metadata("system_index", 2u64)
            .with_gpu_duration_ns(30);
        let text = serde_json::to_string(&event).unwrap();
        let decoded: ProfileEvent = serde_json::from_str(&text).unwrap();
        assert_eq!(decoded.gpu_duration_ns, Some(30));
        assert_eq!(decoded.metadata["system_index"], Value::from(2u64));
    }
}
