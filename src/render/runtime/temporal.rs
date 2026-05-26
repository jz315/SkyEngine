use rustc_hash::FxHashMap;

use crate::math::Mat4;
use crate::render::view::{SceneView, TemporalViewState};

#[derive(Debug, Clone, Copy)]
struct TemporalViewHistory {
    view_proj: [f32; 16],
    jitter: [f32; 2],
    projection: crate::math::Projection,
    target_size: [u32; 2],
}

#[derive(Debug, Default)]
pub(crate) struct TemporalViewTracker {
    frame_index: u64,
    temporal_aa_enabled: bool,
    histories: FxHashMap<u64, TemporalViewHistory>,
}

impl TemporalViewTracker {
    pub(crate) fn update_views(
        &mut self,
        views: &mut [SceneView],
        temporal_aa_enabled: bool,
        jitter_scale: f32,
    ) {
        let frame_index = self.frame_index;
        let temporal_aa_recreated = temporal_aa_enabled && !self.temporal_aa_enabled;
        let jitter_scale = jitter_scale.max(0.0);

        for view in views {
            if view.history_key() == 0 {
                view.set_history_key(fallback_view_history_key(view.execution_order()));
            }

            if view.is_shadow() {
                view.set_temporal_state(TemporalViewState::from_current_view_proj(
                    view.view_uniform.view_proj,
                ));
                continue;
            }

            let jitter = if temporal_aa_enabled {
                let base_jitter = halton_jitter(frame_index, view.target_size);
                [base_jitter[0] * jitter_scale, base_jitter[1] * jitter_scale]
            } else {
                [0.0, 0.0]
            };
            if temporal_aa_enabled {
                apply_wicked_projection_jitter(view, jitter);
            }

            let current_view_proj = view.view_uniform.view_proj;
            let previous = self.histories.get(&view.history_key()).copied();
            let history_reset = previous.is_none_or(|history| {
                history.projection != view.projection || history.target_size != view.target_size
            }) || temporal_aa_recreated;
            let previous_view_proj = if history_reset {
                current_view_proj
            } else {
                previous
                    .map(|history| history.view_proj)
                    .unwrap_or(current_view_proj)
            };
            let previous_jitter = if history_reset {
                [0.0, 0.0]
            } else {
                previous.map(|history| history.jitter).unwrap_or([0.0, 0.0])
            };

            view.set_temporal_state(TemporalViewState {
                current_view_proj,
                previous_view_proj,
                jitter,
                previous_jitter,
                history_reset,
                frame_index,
            });

            self.histories.insert(
                view.history_key(),
                TemporalViewHistory {
                    view_proj: current_view_proj,
                    jitter,
                    projection: view.projection,
                    target_size: view.target_size,
                },
            );
        }

        self.temporal_aa_enabled = temporal_aa_enabled;
        self.frame_index = self.frame_index.wrapping_add(1);
    }
}

#[inline]
pub(crate) fn halton_jitter(frame_index: u64, target_size: [u32; 2]) -> [f32; 2] {
    let sample_index = frame_index % 256 + 1;
    let width = target_size[0].max(1) as f32;
    let height = target_size[1].max(1) as f32;
    [
        (halton(sample_index, 2) * 2.0 - 1.0) / width,
        (halton(sample_index, 3) * 2.0 - 1.0) / height,
    ]
}

#[inline]
fn fallback_view_history_key(execution_order: i32) -> u64 {
    0x7f00_0000_0000_0000u64 ^ (execution_order as i64 as u64)
}

fn apply_wicked_projection_jitter(view: &mut SceneView, jitter: [f32; 2]) {
    if jitter[0] == 0.0 && jitter[1] == 0.0 {
        return;
    }

    let mut uniform = view.view_uniform;
    uniform.projection = wicked_jittered_projection(uniform.projection, jitter);
    uniform.view_proj = (Mat4::from_cols_array(uniform.projection)
        * Mat4::from_cols_array(uniform.view))
    .to_cols_array();
    view.set_jittered_view_uniform(uniform);
}

fn wicked_jittered_projection(mut projection: [f32; 16], jitter: [f32; 2]) -> [f32; 16] {
    // WickedEngine applies jitter as `P = P * XMMatrixTranslation(jitter.x, jitter.y, 0)`
    // in its row-vector math. SkyEngine uses column vectors, so this is the equivalent
    // clip-space translation: clip.xy += jitter.xy * clip.w.
    let row_w = [projection[3], projection[7], projection[11], projection[15]];
    projection[0] += jitter[0] * row_w[0];
    projection[4] += jitter[0] * row_w[1];
    projection[8] += jitter[0] * row_w[2];
    projection[12] += jitter[0] * row_w[3];
    projection[1] += jitter[1] * row_w[0];
    projection[5] += jitter[1] * row_w[1];
    projection[9] += jitter[1] * row_w[2];
    projection[13] += jitter[1] * row_w[3];
    projection
}

fn halton(mut index: u64, base: u64) -> f32 {
    debug_assert!(base > 1);
    let mut result = 0.0f32;
    let mut fraction = 1.0f32 / base as f32;
    while index > 0 {
        result += (index % base) as f32 * fraction;
        index /= base;
        fraction /= base as f32;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::component::Transform;
    use crate::render::view::{Projection, ProjectionViewUniformExt, SceneView, ViewportRect};

    fn test_view(transform: Transform, projection: Projection) -> SceneView {
        let target_size = [128, 64];
        let view_uniform = projection.view_uniform(transform, target_size);
        SceneView::new(
            0,
            ViewportRect::new(0, 0, target_size[0], target_size[1]),
            target_size,
            false,
            u32::MAX,
            transform,
            projection,
            view_uniform,
            false,
        )
        .with_history_key(42)
    }

    #[test]
    fn scene_view_tracks_previous_view_projection() {
        let mut tracker = TemporalViewTracker::default();
        let mut first = vec![test_view(
            Transform::from_xyz(0.0, 0.0, 8.0),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        )];
        tracker.update_views(&mut first, true, 1.0);
        let first_temporal = first[0].temporal;

        let mut second = vec![test_view(
            Transform::from_xyz(2.0, 0.0, 8.0),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        )];
        tracker.update_views(&mut second, true, 1.0);

        assert!(!second[0].temporal.history_reset);
        assert_eq!(
            second[0].temporal.previous_view_proj,
            first_temporal.current_view_proj
        );
        assert_ne!(
            second[0].temporal.current_view_proj,
            first_temporal.current_view_proj
        );
    }

    #[test]
    fn taa_jitter_changes_per_frame() {
        let jitter_a = halton_jitter(0, [128, 64]);
        let jitter_b = halton_jitter(1, [128, 64]);
        let jitter_a_again = halton_jitter(0, [128, 64]);

        assert_eq!(jitter_a, jitter_a_again);
        assert_ne!(jitter_a, jitter_b);
        assert!((jitter_a[0] - 0.0).abs() <= f32::EPSILON);
        assert!((jitter_a[1] + 0.0052083335).abs() <= 0.000001);
        assert_eq!(jitter_a, halton_jitter(256, [128, 64]));
    }

    #[test]
    fn history_reset_on_projection_change() {
        let mut tracker = TemporalViewTracker::default();
        let mut first = vec![test_view(
            Transform::from_xyz(0.0, 0.0, 8.0),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        )];
        tracker.update_views(&mut first, true, 1.0);

        let mut second = vec![test_view(
            Transform::from_xyz(0.0, 0.0, 8.0),
            Projection::orthographic_fixed(128.0, 64.0),
        )];
        tracker.update_views(&mut second, true, 1.0);

        assert!(second[0].temporal.history_reset);
        assert_eq!(
            second[0].temporal.previous_view_proj,
            second[0].temporal.current_view_proj
        );
    }

    #[test]
    fn temporal_tracker_applies_wicked_jitter_to_view_projection() {
        let mut tracker = TemporalViewTracker::default();
        let mut views = vec![test_view(
            Transform::from_xyz(0.0, 0.0, 8.0),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        )];
        let unjittered = views[0].view_uniform.view_proj;
        let unjittered_projection = views[0].view_uniform.projection;

        tracker.update_views(&mut views, true, 1.0);

        assert_eq!(views[0].temporal.jitter, halton_jitter(0, [128, 64]));
        assert_ne!(views[0].view_uniform.view_proj, unjittered);
        assert_eq!(views[0].unjittered_view_proj_matrix, unjittered);
        assert_eq!(views[0].unjittered_projection_matrix, unjittered_projection);
        assert_eq!(
            views[0].temporal.current_view_proj,
            views[0].view_uniform.view_proj
        );
    }

    #[test]
    fn temporal_tracker_zeroes_jitter_when_taa_disabled() {
        let mut tracker = TemporalViewTracker::default();
        let mut views = vec![test_view(
            Transform::from_xyz(0.0, 0.0, 8.0),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        )];
        let unjittered = views[0].view_uniform.view_proj;

        tracker.update_views(&mut views, false, 1.0);

        assert_eq!(views[0].temporal.jitter, [0.0, 0.0]);
        assert_eq!(views[0].view_uniform.view_proj, unjittered);
    }

    #[test]
    fn temporal_tracker_resets_when_taa_resources_are_recreated() {
        let mut tracker = TemporalViewTracker::default();
        let mut enabled = vec![test_view(
            Transform::from_xyz(0.0, 0.0, 8.0),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        )];
        tracker.update_views(&mut enabled, true, 1.0);

        let mut disabled = vec![test_view(
            Transform::from_xyz(0.0, 0.0, 8.0),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        )];
        tracker.update_views(&mut disabled, false, 1.0);

        let mut reenabled = vec![test_view(
            Transform::from_xyz(0.0, 0.0, 8.0),
            Projection::perspective(60.0f32.to_radians(), 0.1, 100.0),
        )];
        tracker.update_views(&mut reenabled, true, 1.0);

        assert!(reenabled[0].temporal.history_reset);
    }
}
