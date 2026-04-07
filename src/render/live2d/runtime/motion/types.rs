// Minimal `motion3.json` runtime ported from the Cubism / SakuraEngine path.
//
// Scope for now:
// Live2D motion playback utilities.
//
// Parses `motion3.json` files, evaluates segment curves, and applies
// weighted parameter updates to a [`Live2DModel`](super::model::Live2DModel).
//
// Supports the Full Demo update pipeline:
// - Model-level curves (`EyeBlink` / `LipSync`) participate in parameter evaluation
// - Effect IDs from `model3.json` Groups are injected per-motion
// - Motion priority queue with `Idle` / `Normal` / `Force` preemption

use crate::render::live2d::model::Live2DModel;

use super::parsing::{bezier_evaluate, bezier_evaluate_restricted, linear_evaluate, parse_curve};
use super::player::{curve_fade_weight, parse_motion_user_events, resolve_motion_fade_seconds};

pub(super) const DEFAULT_MOTION_FADE_SECONDS: f32 = 1.0;
pub(super) const MOTION_RNG_SEED: u64 = 0xDEAD_BEEF_CAFE;

/// Motion priority levels matching the Cubism Full Demo.
///
/// Higher priority motions preempt lower ones. `Force` always succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MotionPriority {
    None = 0,
    Idle = 1,
    Normal = 2,
    Force = 3,
}

pub type MotionHandle = u64;
pub const INVALID_MOTION_HANDLE: MotionHandle = 0;

/// Which model axis a motion curve targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MotionCurveTarget {
    Model,
    Parameter,
    PartOpacity,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MotionPoint {
    pub(super) time: f32,
    pub(super) value: f32,
}

#[derive(Debug, Clone)]
pub(super) enum MotionSegment {
    Linear([MotionPoint; 2]),
    Bezier([MotionPoint; 4]),
    BezierRestricted([MotionPoint; 4]),
    Stepped([MotionPoint; 2]),
    InverseStepped([MotionPoint; 2]),
}

impl MotionSegment {
    fn end_time(&self) -> f32 {
        match self {
            Self::Linear(points) | Self::Stepped(points) | Self::InverseStepped(points) => {
                points[1].time
            }
            Self::Bezier(points) | Self::BezierRestricted(points) => points[3].time,
        }
    }

    fn value_at(&self, time: f32) -> f32 {
        match self {
            Self::Linear(points) => linear_evaluate(*points, time),
            Self::Bezier(points) => bezier_evaluate(*points, time),
            Self::BezierRestricted(points) => bezier_evaluate_restricted(*points, time),
            Self::Stepped(points) => points[0].value,
            Self::InverseStepped(points) => points[1].value,
        }
    }

    fn start_point(&self) -> MotionPoint {
        match self {
            Self::Linear(points) | Self::Stepped(points) | Self::InverseStepped(points) => {
                points[0]
            }
            Self::Bezier(points) | Self::BezierRestricted(points) => points[0],
        }
    }

    fn end_point(&self) -> MotionPoint {
        match self {
            Self::Linear(points) | Self::Stepped(points) | Self::InverseStepped(points) => {
                points[1]
            }
            Self::Bezier(points) | Self::BezierRestricted(points) => points[3],
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct MotionCurve {
    pub(super) target: MotionCurveTarget,
    /// Curve identifier from motion3.json (e.g. "ParamAngleX", "EyeBlink").
    pub(super) id: String,
    pub(super) parameter_index: Option<usize>,
    pub(super) part_opacity_parameter_index: Option<usize>,
    pub(super) segments: Vec<MotionSegment>,
    /// Per-curve fade-in time in seconds. Negative means "use motion-level fade".
    pub(super) fade_in_time: f32,
    /// Per-curve fade-out time in seconds. Negative means "use motion-level fade".
    pub(super) fade_out_time: f32,
}

impl MotionCurve {
    pub(super) fn evaluate(&self, time: f32, correction_end_time: Option<f32>) -> Option<f32> {
        let first = self.segments.first()?;
        if time <= first.end_time() {
            return Some(first.value_at(time));
        }

        for segment in &self.segments {
            if time <= segment.end_time() {
                return Some(segment.value_at(time));
            }
        }

        let last = self.segments.last()?;
        if let Some(end_time) = correction_end_time {
            let last_end_time = last.end_time();
            if time < end_time && last_end_time < end_time {
                let correction_points = [
                    last.end_point(),
                    MotionPoint {
                        time: end_time,
                        value: first.start_point().value,
                    },
                ];

                return Some(match last {
                    MotionSegment::Linear(_)
                    | MotionSegment::Bezier(_)
                    | MotionSegment::BezierRestricted(_) => {
                        linear_evaluate(correction_points, time)
                    }
                    MotionSegment::Stepped(_) => correction_points[0].value,
                    MotionSegment::InverseStepped(_) => correction_points[1].value,
                });
            }
        }

        Some(last.value_at(last.end_time()))
    }
}

#[derive(Debug, Clone)]
pub(super) struct MotionUserEvent {
    pub(super) time_seconds: f32,
    pub(super) value: String,
}

#[derive(Debug, Clone)]
pub(super) struct Live2DMotion {
    pub(super) name: String,
    pub(super) duration: f32,
    pub(super) source_frame_rate: f32,
    pub(super) fade_in_seconds: f32,
    pub(super) fade_out_seconds: f32,
    pub(super) is_looping: bool,
    pub(super) is_loop_fade_in_enabled: bool,
    pub(super) sound_path: Option<String>,
    pub(super) user_events: Vec<MotionUserEvent>,
    pub(super) curves: Vec<MotionCurve>,
    /// Parameter indices for EyeBlink effect (from model3.json Groups).
    pub(super) eye_blink_parameter_indices: Vec<usize>,
    /// Parameter indices for LipSync effect (from model3.json Groups).
    pub(super) lip_sync_parameter_indices: Vec<usize>,
}

impl Live2DMotion {
    pub(super) fn from_json_str(
        name: String,
        text: &str,
        model: &mut Live2DModel,
        fade_in_override: Option<f32>,
        fade_out_override: Option<f32>,
    ) -> Result<Self, String> {
        let json: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("motion JSON parse error: {e}"))?;

        let meta = json
            .get("Meta")
            .ok_or_else(|| "motion JSON missing Meta".to_string())?;
        let duration = meta
            .get("Duration")
            .and_then(|value| value.as_f64())
            .map(|value| value as f32)
            .ok_or_else(|| "motion JSON missing Meta.Duration".to_string())?;
        let is_looping = meta
            .get("Loop")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let source_frame_rate = meta
            .get("Fps")
            .and_then(|value| value.as_f64())
            .map(|value| value as f32)
            .unwrap_or(30.0)
            .max(f32::EPSILON);
        let are_beziers_restricted = meta
            .get("AreBeziersRestricted")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        let curves = json
            .get("Curves")
            .and_then(|value| value.as_array())
            .ok_or_else(|| "motion JSON missing Curves".to_string())?;

        let mut parsed_curves = Vec::with_capacity(curves.len());
        for (curve_index, curve) in curves.iter().enumerate() {
            parsed_curves.push(parse_curve(
                curve_index,
                curve,
                model,
                are_beziers_restricted,
            )?);
        }
        let user_events = parse_motion_user_events(&json)?;

        Ok(Self {
            name,
            duration,
            source_frame_rate,
            fade_in_seconds: resolve_motion_fade_seconds(&json, "FadeInTime", fade_in_override),
            fade_out_seconds: resolve_motion_fade_seconds(&json, "FadeOutTime", fade_out_override),
            is_looping,
            is_loop_fade_in_enabled: true,
            sound_path: None,
            user_events,
            curves: parsed_curves,
            // Effect IDs are injected by the player after construction
            eye_blink_parameter_indices: Vec::new(),
            lip_sync_parameter_indices: Vec::new(),
        })
    }

    pub(super) fn effective_loop_duration(&self) -> f32 {
        let duration = self.duration.max(0.0);
        if self.is_looping {
            duration + 1.0 / self.source_frame_rate.max(f32::EPSILON)
        } else {
            duration
        }
    }

    pub(super) fn sample_time(&self, elapsed: f32) -> f32 {
        let mut time = elapsed.max(0.0);
        let duration = self.effective_loop_duration();

        if self.is_looping {
            while time > duration {
                time -= duration;
            }
            time
        } else {
            time.clamp(0.0, self.duration.max(0.0))
        }
    }

    /// Apply motion curves to the model, matching `CubismMotion::DoUpdateParameters`.
    ///
    /// Model-level curves (`EyeBlink` / `LipSync`) are evaluated first and then
    /// multiplied/added into matching Parameter curves. Unmatched effect params
    /// are applied directly.
    ///
    /// `fade_weight` is the motion-level fade (= fadeIn * fadeOut).
    /// `elapsed`, `time_to_end`, `motion_fade_in`, `motion_fade_out` are used
    /// for per-curve FadeInTime/FadeOutTime overrides.
    pub(super) fn apply(
        &self,
        model: &mut Live2DModel,
        time: f32,
        fade_weight: f32,
        fade_in_elapsed: f32,
        time_to_end: f32,
        motion_fade_in: f32,
        motion_fade_out: f32,
    ) {
        let correction_end_time = self.is_looping.then_some(self.effective_loop_duration());
        let fade_weight = fade_weight.clamp(0.0, 1.0);

        // ── Phase 1: evaluate Model curves to extract effect values ──
        let mut eye_blink_value: Option<f32> = None;
        let mut lip_sync_value: Option<f32> = None;

        for curve in &self.curves {
            if curve.target != MotionCurveTarget::Model {
                continue;
            }
            let Some(value) = curve.evaluate(time, correction_end_time) else {
                continue;
            };
            match curve.id.as_str() {
                "EyeBlink" => eye_blink_value = Some(value),
                "LipSync" => lip_sync_value = Some(value),
                "Opacity" => model.set_model_opacity(value),
                _ => {}
            }
        }

        // ── Phase 2: evaluate Parameter curves with effect overlay ──
        let eb_len = self.eye_blink_parameter_indices.len();
        let ls_len = self.lip_sync_parameter_indices.len();
        let mut eye_blink_applied = vec![false; eb_len];
        let mut lip_sync_applied = vec![false; ls_len];

        for curve in &self.curves {
            if curve.target != MotionCurveTarget::Parameter {
                continue;
            }
            let Some(parameter_index) = curve.parameter_index else {
                continue;
            };
            let Some(mut value) = curve.evaluate(time, correction_end_time) else {
                continue;
            };

            // EyeBlink effect: multiply into matching parameter
            if let Some(eb_val) = eye_blink_value {
                if let Some(pos) = self
                    .eye_blink_parameter_indices
                    .iter()
                    .position(|&idx| idx == parameter_index)
                {
                    value *= eb_val;
                    eye_blink_applied[pos] = true;
                }
            }

            // LipSync effect: add into matching parameter
            if let Some(ls_val) = lip_sync_value {
                if let Some(pos) = self
                    .lip_sync_parameter_indices
                    .iter()
                    .position(|&idx| idx == parameter_index)
                {
                    value += ls_val;
                    lip_sync_applied[pos] = true;
                }
            }

            // Per-curve fade: if this curve has its own FadeInTime/FadeOutTime, use it
            let w = curve_fade_weight(
                curve,
                fade_weight,
                fade_in_elapsed,
                time_to_end,
                motion_fade_in,
                motion_fade_out,
            );
            model.set_parameter_weighted_by_index(parameter_index, value, w);
        }

        // Apply effect to parameters that were NOT covered by a Parameter curve
        if let Some(eb_val) = eye_blink_value {
            for (i, &idx) in self.eye_blink_parameter_indices.iter().enumerate() {
                if !eye_blink_applied[i] {
                    model.set_parameter_weighted_by_index(idx, eb_val, fade_weight);
                }
            }
        }
        if let Some(ls_val) = lip_sync_value {
            for (i, &idx) in self.lip_sync_parameter_indices.iter().enumerate() {
                if !lip_sync_applied[i] {
                    model.set_parameter_weighted_by_index(idx, ls_val, fade_weight);
                }
            }
        }

        // ── Phase 3: PartOpacity curves ──
        for curve in &self.curves {
            if curve.target != MotionCurveTarget::PartOpacity {
                continue;
            }
            if let Some(parameter_index) = curve.part_opacity_parameter_index {
                if let Some(value) = curve.evaluate(time, correction_end_time) {
                    model.set_parameter_by_index(parameter_index, value);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct MotionGroup {
    pub(super) name: String,
    pub(super) motions: Vec<Live2DMotion>,
}

/// An entry in the motion queue. Multiple entries can be active simultaneously
/// during cross-fade transitions (matching `CubismMotionQueueManager`).
#[derive(Debug, Clone)]
pub(super) struct MotionQueueEntry {
    pub(super) handle: MotionHandle,
    pub(super) group_index: usize,
    pub(super) motion_index: usize,
    pub(super) priority: MotionPriority,
    pub(super) available: bool,
    pub(super) started: bool,
    pub(super) start_time_seconds: f32,
    pub(super) fade_in_start_time_seconds: f32,
    pub(super) end_time_seconds: f32,
    pub(super) state_time_seconds: f32,
    pub(super) state_weight: f32,
    pub(super) last_event_check_seconds: f32,
    pub(super) fade_out_seconds: f32,
    pub(super) triggered_fade_out: bool,
    /// Elapsed time since this motion started playing.
    pub(super) elapsed_seconds: f32,
    /// Whether this motion has finished and should be removed.
    pub(super) finished: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MotionFiredEvent {
    pub group_name: String,
    pub motion_name: String,
    pub value: String,
    pub time_seconds: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotionStartedEvent {
    pub handle: MotionHandle,
    pub group_name: String,
    pub motion_name: String,
    pub priority: MotionPriority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotionFinishedEvent {
    pub handle: MotionHandle,
    pub group_name: String,
    pub motion_name: String,
    pub priority: MotionPriority,
}

/// Borrowed motion entry returned by [`Live2DMotionPlayer::motion_entries`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MotionEntryRef<'a> {
    pub group_name: &'a str,
    pub motion_name: &'a str,
    pub index_in_group: usize,
}

/// Motion player with Cubism-like timing, fade, and priority queue.
///
/// Supports priority-based motion preemption matching the Full Demo:
/// - `Idle` motions auto-cycle when nothing else plays
/// - `Normal` motions preempt Idle
/// - `Force` always preempts
pub struct Live2DMotionPlayer {
    pub(super) groups: Vec<MotionGroup>,
    pub(super) idle_group_index: Option<usize>,
    /// Active motion queue — multiple motions can overlap during cross-fade.
    pub(super) motion_queue: Vec<MotionQueueEntry>,
    pub(super) current_priority: MotionPriority,
    pub(super) reserved_priority: MotionPriority,
    pub(super) user_time_seconds: f32,
    pub(super) next_handle: MotionHandle,
    pub(super) rng_state: u64,
    pub(super) pending_events: Vec<MotionFiredEvent>,
    pub(super) pending_started: Vec<MotionStartedEvent>,
    pub(super) pending_finished: Vec<MotionFinishedEvent>,
    pub(super) pending_sounds: Vec<String>,
    pub(super) began_motion_handler: Option<Box<dyn FnMut(&MotionStartedEvent)>>,
    pub(super) finished_motion_handler: Option<Box<dyn FnMut(&MotionFinishedEvent)>>,
}
