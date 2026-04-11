use super::*;
use std::path::Path;

use crate::render::live2d::model::Live2DModel;

impl Default for Live2DMotionPlayer {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            idle_group_index: None,
            motion_queue: Vec::new(),
            current_priority: MotionPriority::None,
            reserved_priority: MotionPriority::None,
            user_time_seconds: 0.0,
            next_handle: 1,
            rng_state: MOTION_RNG_SEED,
            pending_events: Vec::new(),
            pending_started: Vec::new(),
            pending_finished: Vec::new(),
            pending_sounds: Vec::new(),
            began_motion_handler: None,
            finished_motion_handler: None,
        }
    }
}

impl Live2DMotionPlayer {
    pub(crate) fn reset_state(&mut self) {
        self.motion_queue.clear();
        self.current_priority = MotionPriority::None;
        self.reserved_priority = MotionPriority::None;
        self.user_time_seconds = 0.0;
        self.rng_state = MOTION_RNG_SEED;
        self.pending_events.clear();
        self.pending_started.clear();
        self.pending_finished.clear();
        self.pending_sounds.clear();
    }

    pub fn from_model_json(
        json: &serde_json::Value,
        base_dir: &std::path::Path,
        model: &mut Live2DModel,
    ) -> Result<Option<Self>, String> {
        let Some(motion_groups) = motion_groups_from_model_json(json) else {
            return Ok(None);
        };

        // Extract EyeBlink / LipSync parameter indices from model3.json Groups
        let eye_blink_parameter_indices = extract_group_parameter_indices(json, "EyeBlink", model);
        let lip_sync_parameter_indices = extract_group_parameter_indices(json, "LipSync", model);

        let mut groups = Vec::new();
        let mut idle_group_index = None;
        for (group_name, entries) in motion_groups {
            let mut motions = Vec::new();
            for entry in entries {
                let Some(file) = entry.get("File").and_then(|value| value.as_str()) else {
                    continue;
                };
                let path = base_dir.join(file);
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| format!("failed to read motion {}: {e}", path.display()))?;
                let mut motion = Live2DMotion::from_json_str(
                    motion_name_from_path(file),
                    &text,
                    model,
                    entry
                        .get("FadeInTime")
                        .and_then(|value| value.as_f64())
                        .map(|value| value as f32),
                    entry
                        .get("FadeOutTime")
                        .and_then(|value| value.as_f64())
                        .map(|value| value as f32),
                )?;
                // Inject effect IDs (matching Full Demo SetEffectIds)
                motion.eye_blink_parameter_indices = eye_blink_parameter_indices.clone();
                motion.lip_sync_parameter_indices = lip_sync_parameter_indices.clone();
                motion.sound_path = entry
                    .get("Sound")
                    .and_then(|value| value.as_str())
                    .map(|value| base_dir.join(value).to_string_lossy().to_string());
                motions.push(motion);
            }

            if motions.is_empty() {
                continue;
            }

            if group_name == "Idle" {
                idle_group_index = Some(groups.len());
            }
            groups.push(MotionGroup {
                name: group_name.to_string(),
                motions,
            });
        }

        if groups.is_empty() {
            return Ok(None);
        }

        Ok(Some(Self {
            groups,
            idle_group_index,
            ..Default::default()
        }))
    }

    pub(crate) fn fresh_clone(&self) -> Self {
        Self {
            groups: self.groups.clone(),
            idle_group_index: self.idle_group_index,
            ..Default::default()
        }
    }

    pub fn update(&mut self, model: &mut Live2DModel, delta_time_seconds: f32) -> bool {
        if self.groups.is_empty() {
            return false;
        }
        if self.is_finished() {
            return false;
        }

        let mut updated = false;
        let dt = delta_time_seconds.max(0.0);
        self.user_time_seconds += dt;

        // Iterate all queue entries — multiple can be active during cross-fade
        for i in 0..self.motion_queue.len() {
            if self.motion_queue[i].finished || !self.motion_queue[i].available {
                continue;
            }

            let (handle, group_index, motion_index, priority) = {
                let entry = &self.motion_queue[i];
                (
                    entry.handle,
                    entry.group_index,
                    entry.motion_index,
                    entry.priority,
                )
            };
            let (
                motion_duration,
                effective_loop_duration,
                motion_fade_in_seconds,
                motion_fade_out_seconds,
                is_looping,
                is_loop_fade_in_enabled,
            ) = {
                let motion = &self.groups[group_index].motions[motion_index];
                (
                    motion.duration.max(0.0),
                    motion.effective_loop_duration(),
                    motion.fade_in_seconds,
                    motion.fade_out_seconds,
                    motion.is_looping,
                    motion.is_loop_fade_in_enabled,
                )
            };
            self.setup_motion_queue_entry(i, motion_duration, is_looping);

            let (elapsed, fade_in_elapsed, end_time_seconds) = {
                let entry = &self.motion_queue[i];
                (
                    (self.user_time_seconds - entry.start_time_seconds).max(0.0),
                    (self.user_time_seconds - entry.fade_in_start_time_seconds).max(0.0),
                    entry.end_time_seconds,
                )
            };
            let motion_time = self.groups[group_index].motions[motion_index].sample_time(elapsed);

            // Compute motion-level fade-in
            let fade_in = motion_fade_weight(fade_in_elapsed, motion_fade_in_seconds);

            // Compute motion-level fade-out
            let time_to_end = if end_time_seconds < 0.0 {
                -1.0
            } else {
                (end_time_seconds - self.user_time_seconds).max(0.0)
            };
            let fade_out = if motion_fade_out_seconds <= f32::EPSILON || end_time_seconds < 0.0 {
                1.0
            } else {
                easing_sine((time_to_end / motion_fade_out_seconds).clamp(0.0, 1.0))
            };

            let fade_weight = (fade_in * fade_out).clamp(0.0, 1.0);
            {
                let motion = &self.groups[group_index].motions[motion_index];
                motion.apply(
                    model,
                    motion_time,
                    fade_weight,
                    fade_in_elapsed,
                    time_to_end,
                    fade_in,
                    fade_out,
                );
            }
            self.motion_queue[i].elapsed_seconds = elapsed;
            self.motion_queue[i].state_time_seconds = self.user_time_seconds;
            self.motion_queue[i].state_weight = fade_weight;
            updated = true;

            // Check finish conditions
            let reached_end_time =
                end_time_seconds > 0.0 && self.user_time_seconds >= end_time_seconds;
            let reached_natural_end = !is_looping && elapsed >= motion_duration;
            let reached_loop_wrap = is_looping
                && effective_loop_duration > f32::EPSILON
                && elapsed >= effective_loop_duration;

            if reached_natural_end {
                self.motion_queue[i].finished = true;
                self.emit_finished(group_index, motion_index, priority, handle, false);
            } else if reached_loop_wrap {
                self.update_for_next_loop(i, motion_time, is_loop_fade_in_enabled);
                self.emit_finished(group_index, motion_index, priority, handle, true);
                if reached_end_time {
                    self.motion_queue[i].finished = true;
                }
            } else if reached_end_time {
                // Cubism finishes interrupted motions silently when fade-out completes.
                self.motion_queue[i].finished = true;
            }

            let started_at_seconds = self.motion_queue[i].start_time_seconds;
            let previous_elapsed =
                self.motion_queue[i].last_event_check_seconds - started_at_seconds;
            let next_elapsed = self.user_time_seconds - started_at_seconds;
            self.collect_fired_events(group_index, motion_index, previous_elapsed, next_elapsed);
            self.motion_queue[i].last_event_check_seconds = self.user_time_seconds;

            if !self.motion_queue[i].finished {
                self.apply_triggered_fade_out(i);
            }
        }

        // Remove finished entries
        self.motion_queue.retain(|e| !e.finished);

        // Clear priority when all motions finished
        if self.is_finished() {
            self.current_priority = MotionPriority::None;
        }

        updated
    }

    pub fn motion_count(&self) -> usize {
        self.groups.iter().map(|group| group.motions.len()).sum()
    }

    pub fn group_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.groups.iter().map(|group| group.name.as_str())
    }

    pub fn motion_entries(&self) -> impl Iterator<Item = MotionEntryRef<'_>> + '_ {
        self.groups.iter().flat_map(|group| {
            group
                .motions
                .iter()
                .enumerate()
                .map(move |(index_in_group, motion)| MotionEntryRef {
                    group_name: group.name.as_str(),
                    motion_name: motion.name.as_str(),
                    index_in_group,
                })
        })
    }

    pub fn motion_names(&self) -> impl Iterator<Item = String> + '_ {
        self.motion_entries()
            .map(|entry| format!("{}/{}", entry.group_name, entry.motion_name))
    }

    pub fn take_fired_events(&mut self) -> Vec<MotionFiredEvent> {
        std::mem::take(&mut self.pending_events)
    }

    pub fn take_started_motions(&mut self) -> Vec<MotionStartedEvent> {
        std::mem::take(&mut self.pending_started)
    }

    pub fn take_finished_motions(&mut self) -> Vec<MotionFinishedEvent> {
        std::mem::take(&mut self.pending_finished)
    }

    pub fn take_started_sounds(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_sounds)
    }

    pub fn set_began_motion_handler<F>(&mut self, handler: F)
    where
        F: FnMut(&MotionStartedEvent) + 'static,
    {
        self.began_motion_handler = Some(Box::new(handler));
    }

    pub fn clear_began_motion_handler(&mut self) {
        self.began_motion_handler = None;
    }

    pub fn set_finished_motion_handler<F>(&mut self, handler: F)
    where
        F: FnMut(&MotionFinishedEvent) + 'static,
    {
        self.finished_motion_handler = Some(Box::new(handler));
    }

    pub fn clear_finished_motion_handler(&mut self) {
        self.finished_motion_handler = None;
    }

    pub fn is_finished(&self) -> bool {
        self.motion_queue.iter().all(|entry| entry.finished)
    }

    pub fn is_finished_handle(&self, handle: MotionHandle) -> bool {
        if handle == INVALID_MOTION_HANDLE {
            return true;
        }
        self.motion_queue
            .iter()
            .find(|entry| entry.handle == handle)
            .is_none_or(|entry| entry.finished)
    }

    pub fn reserve_priority(&self) -> MotionPriority {
        self.reserved_priority
    }

    pub fn reserve_motion(&mut self, priority: MotionPriority) -> bool {
        if priority <= self.reserved_priority || priority <= self.current_priority {
            return false;
        }
        self.reserved_priority = priority;
        true
    }

    pub fn stop_all_motions(&mut self) {
        self.motion_queue.clear();
        self.current_priority = MotionPriority::None;
        self.reserved_priority = MotionPriority::None;
    }

    pub fn start_idle_motion_if_finished(&mut self) -> bool {
        if !self.is_finished() {
            return false;
        }
        self.start_next_idle().is_some()
    }

    /// Set a motion by group name and index, using `PriorityForce` (always succeeds).
    pub fn set_motion(&mut self, group_name: &str, index_in_group: usize) -> bool {
        self.start_motion_priority(group_name, index_in_group, MotionPriority::Force)
            .is_some()
    }

    /// Set a motion by flattened index, using `PriorityForce` (always succeeds).
    pub fn set_motion_by_index(&mut self, index: usize) -> bool {
        let mut remaining = index;
        for group_index in 0..self.groups.len() {
            let group_len = self.groups[group_index].motions.len();
            if remaining < group_len {
                let group_name = self.groups[group_index].name.clone();
                return self
                    .start_motion_priority(&group_name, remaining, MotionPriority::Force)
                    .is_some();
            }
            remaining -= group_len;
        }
        false
    }

    /// Start a specific motion with priority-based preemption.
    ///
    /// Returns `true` if the motion was started, `false` if a higher-priority
    /// motion is already playing and prevented the switch.
    pub fn start_motion_priority(
        &mut self,
        group_name: &str,
        index_in_group: usize,
        priority: MotionPriority,
    ) -> Option<MotionHandle> {
        if priority == MotionPriority::Force {
            self.reserved_priority = priority;
        } else if self.reserved_priority != priority && !self.reserve_motion(priority) {
            return None;
        }

        let Some(group_index) = self.find_group_index(group_name) else {
            return None;
        };
        self.start_motion_internal(group_index, index_in_group, priority)
    }

    /// Start a random motion from the named group with the given priority.
    pub fn start_random_motion(&mut self, group_name: &str, priority: MotionPriority) -> bool {
        self.start_random_motion_handle(group_name, priority)
            .is_some()
    }

    pub fn start_random_motion_handle(
        &mut self,
        group_name: &str,
        priority: MotionPriority,
    ) -> Option<MotionHandle> {
        let Some(group_index) = self.find_group_index(group_name) else {
            return None;
        };
        let count = self.groups[group_index].motions.len();
        if count == 0 {
            return None;
        }

        if priority != MotionPriority::Force
            && self.reserved_priority != priority
            && !self.reserve_motion(priority)
        {
            return None;
        }
        if priority == MotionPriority::Force {
            self.reserved_priority = priority;
        }

        let index = self.next_random_u32() as usize % count;
        self.start_motion_internal(group_index, index, priority)
    }

    /// The current effective priority of the active motion (or `None`).
    pub fn current_priority(&self) -> MotionPriority {
        self.current_priority
    }

    pub(super) fn start_next_idle(&mut self) -> Option<MotionHandle> {
        let Some(idle_group_index) = self.idle_group_index else {
            return None;
        };
        let idle_motion_count = self.groups[idle_group_index].motions.len();
        if idle_motion_count == 0 {
            return None;
        }

        let motion_index = self.next_random_u32() as usize % idle_motion_count;
        let handle = self.allocate_handle();

        self.motion_queue.push(MotionQueueEntry {
            handle,
            group_index: idle_group_index,
            motion_index,
            priority: MotionPriority::Idle,
            available: true,
            started: false,
            start_time_seconds: -1.0,
            fade_in_start_time_seconds: 0.0,
            end_time_seconds: -1.0,
            state_time_seconds: 0.0,
            state_weight: 0.0,
            last_event_check_seconds: self.user_time_seconds,
            fade_out_seconds: 0.0,
            triggered_fade_out: false,
            elapsed_seconds: 0.0,
            finished: false,
        });
        self.record_started_sound(idle_group_index, motion_index);
        self.current_priority = MotionPriority::Idle;
        Some(handle)
    }

    fn start_motion_internal(
        &mut self,
        group_index: usize,
        motion_index: usize,
        priority: MotionPriority,
    ) -> Option<MotionHandle> {
        let Some(group) = self.groups.get(group_index) else {
            return None;
        };
        if motion_index >= group.motions.len() {
            return None;
        }

        // Mark all existing motions for fade-out (cross-fade)
        for existing in &mut self.motion_queue {
            if existing.finished {
                continue;
            }
            existing.fade_out_seconds =
                self.groups[existing.group_index].motions[existing.motion_index].fade_out_seconds;
            existing.triggered_fade_out = true;
        }

        let handle = self.allocate_handle();
        // Push the new motion onto the queue
        self.motion_queue.push(MotionQueueEntry {
            handle,
            group_index,
            motion_index,
            priority,
            available: true,
            started: false,
            start_time_seconds: -1.0,
            fade_in_start_time_seconds: 0.0,
            end_time_seconds: -1.0,
            state_time_seconds: 0.0,
            state_weight: 0.0,
            last_event_check_seconds: self.user_time_seconds,
            fade_out_seconds: 0.0,
            triggered_fade_out: false,
            elapsed_seconds: 0.0,
            finished: false,
        });
        self.record_started_sound(group_index, motion_index);
        if priority == self.reserved_priority {
            self.reserved_priority = MotionPriority::None;
        }
        self.current_priority = priority;
        Some(handle)
    }

    fn find_group_index(&self, group_name: &str) -> Option<usize> {
        self.groups
            .iter()
            .position(|group| group.name == group_name)
    }

    fn next_random_u32(&mut self) -> u32 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        (self.rng_state >> 32) as u32
    }

    fn allocate_handle(&mut self) -> MotionHandle {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.saturating_add(1).max(1);
        handle
    }

    fn record_started_sound(&mut self, group_index: usize, motion_index: usize) {
        if let Some(sound_path) = self.groups[group_index].motions[motion_index]
            .sound_path
            .clone()
        {
            self.pending_sounds.push(sound_path);
        }
    }

    fn setup_motion_queue_entry(
        &mut self,
        entry_index: usize,
        motion_duration: f32,
        is_looping: bool,
    ) {
        let entry = &mut self.motion_queue[entry_index];
        if entry.started || !entry.available || entry.finished {
            return;
        }

        entry.started = true;
        entry.start_time_seconds = self.user_time_seconds;
        entry.fade_in_start_time_seconds = self.user_time_seconds;
        if entry.end_time_seconds < 0.0 {
            entry.end_time_seconds = if is_looping || motion_duration <= 0.0 {
                -1.0
            } else {
                entry.start_time_seconds + motion_duration
            };
        }

        let (group_index, motion_index, priority, handle) = (
            entry.group_index,
            entry.motion_index,
            entry.priority,
            entry.handle,
        );
        self.emit_started(group_index, motion_index, priority, handle);
    }

    fn apply_triggered_fade_out(&mut self, entry_index: usize) {
        let entry = &mut self.motion_queue[entry_index];
        if !entry.triggered_fade_out {
            return;
        }

        let new_end_time_seconds = self.user_time_seconds + entry.fade_out_seconds;
        if entry.end_time_seconds < 0.0 || new_end_time_seconds < entry.end_time_seconds {
            entry.end_time_seconds = new_end_time_seconds;
        }
    }

    fn update_for_next_loop(
        &mut self,
        entry_index: usize,
        wrapped_time_seconds: f32,
        is_loop_fade_in_enabled: bool,
    ) {
        let entry = &mut self.motion_queue[entry_index];
        entry.start_time_seconds = self.user_time_seconds - wrapped_time_seconds;
        if is_loop_fade_in_enabled {
            entry.fade_in_start_time_seconds = self.user_time_seconds - wrapped_time_seconds;
        }
    }

    fn emit_started(
        &mut self,
        group_index: usize,
        motion_index: usize,
        priority: MotionPriority,
        handle: MotionHandle,
    ) {
        let event = MotionStartedEvent {
            handle,
            group_name: self.groups[group_index].name.clone(),
            motion_name: self.groups[group_index].motions[motion_index].name.clone(),
            priority,
        };
        if let Some(handler) = self.began_motion_handler.as_mut() {
            handler(&event);
        }
        self.pending_started.push(event);
    }

    fn emit_finished(
        &mut self,
        group_index: usize,
        motion_index: usize,
        priority: MotionPriority,
        handle: MotionHandle,
        is_loop_cycle: bool,
    ) {
        let event = MotionFinishedEvent {
            handle,
            group_name: self.groups[group_index].name.clone(),
            motion_name: self.groups[group_index].motions[motion_index].name.clone(),
            priority,
            is_loop_cycle,
        };
        if let Some(handler) = self.finished_motion_handler.as_mut() {
            handler(&event);
        }
        self.pending_finished.push(event);
    }

    pub(super) fn collect_fired_events(
        &mut self,
        group_index: usize,
        motion_index: usize,
        previous_elapsed: f32,
        next_elapsed: f32,
    ) {
        let (group_name, motion_name, user_events) = {
            let motion = &self.groups[group_index].motions[motion_index];
            (
                self.groups[group_index].name.clone(),
                motion.name.clone(),
                motion.user_events.clone(),
            )
        };

        if user_events.is_empty() {
            return;
        }
        for event in &user_events {
            if event.time_seconds > previous_elapsed && event.time_seconds <= next_elapsed {
                self.pending_events.push(MotionFiredEvent {
                    group_name: group_name.clone(),
                    motion_name: motion_name.clone(),
                    value: event.value.clone(),
                    time_seconds: event.time_seconds,
                });
            }
        }
    }
}

pub(super) fn motion_groups_from_model_json(
    json: &serde_json::Value,
) -> Option<Vec<(&str, &Vec<serde_json::Value>)>> {
    json.get("FileReferences")
        .and_then(|value| value.get("Motions"))
        .and_then(|value| value.as_object())
        .map(|groups| {
            groups
                .iter()
                .filter_map(|(group_name, entries)| {
                    entries.as_array().map(|arr| (group_name.as_str(), arr))
                })
                .collect()
        })
}

fn motion_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(path)
        .trim_end_matches(".motion3")
        .to_string()
}

pub(super) fn resolve_motion_fade_seconds(
    json: &serde_json::Value,
    field: &str,
    override_value: Option<f32>,
) -> f32 {
    override_value
        .or_else(|| {
            json.get("Meta")
                .and_then(|meta| meta.get(field))
                .and_then(|value| value.as_f64())
                .map(|value| value as f32)
        })
        .filter(|value| *value >= 0.0)
        .unwrap_or(DEFAULT_MOTION_FADE_SECONDS)
}

pub(super) fn parse_motion_user_events(
    json: &serde_json::Value,
) -> Result<Vec<MotionUserEvent>, String> {
    let Some(user_events) = json.get("UserData") else {
        return Ok(Vec::new());
    };
    let Some(user_events) = user_events.as_array() else {
        return Err("motion JSON UserData must be an array".to_string());
    };

    let mut parsed = Vec::with_capacity(user_events.len());
    for (index, event) in user_events.iter().enumerate() {
        let time_seconds = event
            .get("Time")
            .and_then(|value| value.as_f64())
            .map(|value| value as f32)
            .ok_or_else(|| format!("motion user event {index} missing Time"))?;
        let value = event
            .get("Value")
            .and_then(|value| value.as_str())
            .ok_or_else(|| format!("motion user event {index} missing Value"))?;
        parsed.push(MotionUserEvent {
            time_seconds,
            value: value.to_string(),
        });
    }

    Ok(parsed)
}

fn motion_fade_weight(time_to_edge: f32, fade_seconds: f32) -> f32 {
    if fade_seconds <= f32::EPSILON {
        1.0
    } else {
        easing_sine(time_to_edge / fade_seconds)
    }
}

fn easing_sine(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    0.5 - 0.5 * (value * std::f32::consts::PI).cos()
}

/// Compute the effective fade weight for a single curve.
///
/// If the curve has its own FadeInTime/FadeOutTime (>= 0), those override
/// the motion-level fade values. Otherwise, the motion-level `fade_weight`
/// is used directly. This matches `CubismMotion::DoUpdateParameters`'s
/// per-curve fade path.
pub(super) fn curve_fade_weight(
    curve: &MotionCurve,
    motion_fade_weight: f32,
    elapsed: f32,
    time_to_end: f32,
    motion_fade_in: f32,
    motion_fade_out: f32,
) -> f32 {
    if curve.fade_in_time < 0.0 && curve.fade_out_time < 0.0 {
        // Both negative → use motion-level fade weight as-is
        return motion_fade_weight;
    }

    // Per-curve fade-in
    let fin = if curve.fade_in_time < 0.0 {
        motion_fade_in
    } else if curve.fade_in_time <= f32::EPSILON {
        1.0
    } else {
        easing_sine((elapsed / curve.fade_in_time).clamp(0.0, 1.0))
    };

    // Per-curve fade-out
    let fout = if curve.fade_out_time < 0.0 {
        motion_fade_out
    } else if curve.fade_out_time <= f32::EPSILON || time_to_end < 0.0 {
        1.0
    } else {
        easing_sine((time_to_end / curve.fade_out_time).clamp(0.0, 1.0))
    };

    (fin * fout).clamp(0.0, 1.0)
}
