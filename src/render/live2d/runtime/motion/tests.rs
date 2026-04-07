use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    use crate::render::live2d::runtime::motion::player::{
        motion_groups_from_model_json, resolve_motion_fade_seconds,
    };
    use crate::render::live2d::Live2DModel;
    use serde_json::json;

    #[test]
    fn parses_linear_segments() {
        let segments = parse_segments(&[0.0, 1.0, 0.0, 1.0, 2.0], false).unwrap();
        assert_eq!(segments.len(), 1);
        match &segments[0] {
            MotionSegment::Linear(points) => {
                assert_eq!(points[0].time, 0.0);
                assert_eq!(points[1].time, 1.0);
                assert_eq!(points[1].value, 2.0);
            }
            _ => panic!("expected linear"),
        }
    }

    #[test]
    fn evaluates_linear_segment() {
        let value = linear_evaluate(
            [
                MotionPoint {
                    time: 0.0,
                    value: 0.0,
                },
                MotionPoint {
                    time: 1.0,
                    value: 10.0,
                },
            ],
            0.25,
        );
        assert!((value - 2.5).abs() < 0.001);
    }

    #[test]
    fn bezier_uses_curve_time_axis() {
        let value = bezier_evaluate(
            [
                MotionPoint {
                    time: 0.0,
                    value: 0.0,
                },
                MotionPoint {
                    time: 0.0,
                    value: 1.0 / 3.0,
                },
                MotionPoint {
                    time: 0.0,
                    value: 2.0 / 3.0,
                },
                MotionPoint {
                    time: 1.0,
                    value: 1.0,
                },
            ],
            0.125,
        );
        assert!((value - 0.5).abs() < 0.01);
    }

    #[test]
    fn restricted_bezier_uses_normalized_endpoints() {
        let segments = parse_segments(
            &[0.0, 0.0, 1.0, 0.0, 1.0 / 3.0, 0.0, 2.0 / 3.0, 1.0, 1.0],
            true,
        )
        .expect("restricted bezier should parse");

        match &segments[0] {
            MotionSegment::BezierRestricted(points) => {
                let value = bezier_evaluate_restricted(*points, 0.125);
                assert!((value - 0.125).abs() < 0.01);
            }
            _ => panic!("expected restricted bezier"),
        }
    }

    #[test]
    fn curve_correction_bridges_loop_endpoint() {
        let curve = MotionCurve {
            target: MotionCurveTarget::Parameter,
            id: "ParamAngleX".to_string(),
            parameter_index: Some(0),
            part_opacity_parameter_index: None,
            segments: vec![MotionSegment::Linear([
                MotionPoint {
                    time: 0.0,
                    value: 0.0,
                },
                MotionPoint {
                    time: 1.0,
                    value: 1.0,
                },
            ])],
            fade_in_time: -1.0,
            fade_out_time: -1.0,
        };

        let value = curve
            .evaluate(1.25, Some(1.5))
            .expect("curve should evaluate inside correction span");
        assert!((value - 0.5).abs() < 0.01);
    }

    #[test]
    fn finds_multiple_motion_groups_under_file_references() {
        let json = json!({
            "FileReferences": {
                "Motions": {
                    "Idle": [
                        { "File": "motions/a.motion3.json" },
                        { "File": "motions/b.motion3.json" }
                    ],
                    "TapBody": [
                        { "File": "motions/c.motion3.json" }
                    ]
                }
            }
        });

        let groups = motion_groups_from_model_json(&json).expect("groups should parse");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "Idle");
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].0, "TapBody");
        assert_eq!(groups[1].1.len(), 1);
    }

    #[test]
    fn preserves_motion_group_insertion_order_from_json() {
        let json = json!({
            "FileReferences": {
                "Motions": {
                    "ZGroup": [
                        { "File": "motions/z.motion3.json" }
                    ],
                    "AGroup": [
                        { "File": "motions/a.motion3.json" }
                    ]
                }
            }
        });

        let groups = motion_groups_from_model_json(&json).expect("groups should parse");
        let names: Vec<&str> = groups.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, vec!["ZGroup", "AGroup"]);
    }

    #[test]
    fn motion_fade_seconds_read_from_meta_block() {
        let json = json!({
            "Meta": {
                "FadeInTime": 0.25,
                "FadeOutTime": 0.75
            }
        });

        assert!((resolve_motion_fade_seconds(&json, "FadeInTime", None) - 0.25).abs() < 0.0001);
        assert!((resolve_motion_fade_seconds(&json, "FadeOutTime", None) - 0.75).abs() < 0.0001);
    }

    fn dummy_motion(name: &str) -> Live2DMotion {
        Live2DMotion {
            name: name.to_string(),
            duration: 1.0,
            source_frame_rate: 30.0,
            fade_in_seconds: 0.5,
            fade_out_seconds: 0.5,
            is_looping: false,
            is_loop_fade_in_enabled: true,
            sound_path: None,
            user_events: Vec::new(),
            curves: Vec::new(),
            eye_blink_parameter_indices: Vec::new(),
            lip_sync_parameter_indices: Vec::new(),
        }
    }

    #[test]
    fn set_motion_by_index_spans_multiple_groups() {
        let mut player = Live2DMotionPlayer {
            groups: vec![
                MotionGroup {
                    name: "Idle".to_string(),
                    motions: vec![dummy_motion("a"), dummy_motion("b")],
                },
                MotionGroup {
                    name: "TapBody".to_string(),
                    motions: vec![dummy_motion("tap")],
                },
            ],
            idle_group_index: Some(0),
            ..Default::default()
        };

        assert!(player.set_motion_by_index(2));
        // The newest entry in the queue should be TapBody[0]
        let last = player.motion_queue.last().unwrap();
        assert_eq!(last.group_index, 1);
        assert_eq!(last.motion_index, 0);
        assert_eq!(player.current_priority(), MotionPriority::Force);
    }

    #[test]
    fn motion_names_include_group_prefix() {
        let player = Live2DMotionPlayer {
            groups: vec![
                MotionGroup {
                    name: "Idle".to_string(),
                    motions: vec![dummy_motion("a")],
                },
                MotionGroup {
                    name: "TapBody".to_string(),
                    motions: vec![dummy_motion("tap")],
                },
            ],
            idle_group_index: Some(0),
            ..Default::default()
        };

        let names: Vec<String> = player.motion_names().collect();
        assert_eq!(names, vec!["Idle/a", "TapBody/tap"]);
    }

    #[test]
    fn reserve_motion_follows_official_priority_rules() {
        let mut player = Live2DMotionPlayer::default();

        assert!(player.reserve_motion(MotionPriority::Idle));
        assert_eq!(player.reserve_priority(), MotionPriority::Idle);
        assert!(!player.reserve_motion(MotionPriority::Idle));
        assert!(player.reserve_motion(MotionPriority::Normal));
        assert_eq!(player.reserve_priority(), MotionPriority::Normal);

        player.current_priority = MotionPriority::Normal;
        assert!(!player.reserve_motion(MotionPriority::Idle));
        assert!(!player.reserve_motion(MotionPriority::Normal));
        assert!(player.reserve_motion(MotionPriority::Force));
        assert_eq!(player.reserve_priority(), MotionPriority::Force);
    }

    #[test]
    fn start_motion_priority_updates_current_and_clears_reserve() {
        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "TapBody".to_string(),
                motions: vec![dummy_motion("tap")],
            }],
            ..Default::default()
        };

        assert!(player.reserve_motion(MotionPriority::Normal));
        let handle = player.start_motion_priority("TapBody", 0, MotionPriority::Normal);
        assert!(handle.is_some());
        assert_eq!(player.current_priority(), MotionPriority::Normal);
        assert_eq!(player.reserve_priority(), MotionPriority::None);
    }

    #[test]
    fn cross_fade_marks_old_motions_for_fadeout() {
        let mut player = Live2DMotionPlayer {
            groups: vec![
                MotionGroup {
                    name: "Idle".to_string(),
                    motions: vec![dummy_motion("idle_a")],
                },
                MotionGroup {
                    name: "TapBody".to_string(),
                    motions: vec![dummy_motion("tap")],
                },
            ],
            idle_group_index: Some(0),
            ..Default::default()
        };

        // Start idle
        assert!(player.set_motion("Idle", 0));
        assert_eq!(player.motion_queue.len(), 1);
        assert!(!player.motion_queue[0].triggered_fade_out);

        // Start TapBody — old idle should get fade-out triggered
        assert!(player.set_motion("TapBody", 0));
        assert_eq!(player.motion_queue.len(), 2);
        assert!(player.motion_queue[0].triggered_fade_out);
        assert!(!player.motion_queue[1].triggered_fade_out);
    }

    #[test]
    fn fade_out_trigger_takes_effect_after_current_update() {
        let mut old = dummy_motion("old");
        old.duration = 10.0;
        old.fade_in_seconds = 0.0;
        old.fade_out_seconds = 1.0;

        let mut new = dummy_motion("new");
        new.duration = 10.0;
        new.fade_in_seconds = 0.0;
        new.fade_out_seconds = 1.0;

        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "TapBody".to_string(),
                motions: vec![old, new],
            }],
            ..Default::default()
        };

        let mut model = dummy_model();
        assert!(player
            .start_motion_priority("TapBody", 0, MotionPriority::Normal)
            .is_some());
        assert!(player.update(&mut model, 0.0));
        assert!(player.update(&mut model, 1.0));

        assert!(player
            .start_motion_priority("TapBody", 1, MotionPriority::Force)
            .is_some());
        assert!(player.update(&mut model, 0.0));

        assert_eq!(player.motion_queue[0].state_weight, 1.0);
        assert_eq!(
            player.motion_queue[0].end_time_seconds,
            player.user_time_seconds + 1.0
        );
    }

    #[test]
    fn idle_start_emits_motion_sound() {
        let mut idle = dummy_motion("idle_a");
        idle.sound_path = Some("sounds/idle.wav".to_string());

        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "Idle".to_string(),
                motions: vec![idle],
            }],
            idle_group_index: Some(0),
            ..Default::default()
        };

        player.start_next_idle();

        assert_eq!(
            player.take_started_sounds(),
            vec!["sounds/idle.wav".to_string()]
        );
        assert!(player.take_started_motions().is_empty());
    }

    #[test]
    fn start_idle_motion_only_runs_when_finished() {
        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "Idle".to_string(),
                motions: vec![dummy_motion("idle_a")],
            }],
            idle_group_index: Some(0),
            ..Default::default()
        };

        assert!(player.start_idle_motion_if_finished());
        assert!(!player.is_finished());
        assert_eq!(player.current_priority(), MotionPriority::Idle);
        assert!(!player.start_idle_motion_if_finished());
    }

    #[test]
    fn stop_all_motions_resets_manager_state() {
        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "TapBody".to_string(),
                motions: vec![dummy_motion("tap")],
            }],
            ..Default::default()
        };

        assert!(player
            .start_motion_priority("TapBody", 0, MotionPriority::Force)
            .is_some());
        assert_eq!(player.current_priority(), MotionPriority::Force);

        player.stop_all_motions();

        assert!(player.is_finished());
        assert_eq!(player.current_priority(), MotionPriority::None);
        assert_eq!(player.reserve_priority(), MotionPriority::None);
        assert!(player.motion_queue.is_empty());
    }

    #[test]
    fn reset_state_clears_runtime_progress_but_keeps_handle_counter() {
        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "TapBody".to_string(),
                motions: vec![dummy_motion("tap")],
            }],
            motion_queue: vec![MotionQueueEntry {
                handle: 7,
                group_index: 0,
                motion_index: 0,
                priority: MotionPriority::Normal,
                available: true,
                started: true,
                start_time_seconds: 1.0,
                fade_in_start_time_seconds: 1.0,
                end_time_seconds: 2.0,
                state_time_seconds: 1.0,
                state_weight: 0.5,
                last_event_check_seconds: 1.0,
                fade_out_seconds: 0.5,
                triggered_fade_out: true,
                elapsed_seconds: 1.0,
                finished: false,
            }],
            current_priority: MotionPriority::Force,
            reserved_priority: MotionPriority::Normal,
            user_time_seconds: 3.0,
            next_handle: 42,
            pending_events: vec![MotionFiredEvent {
                group_name: "TapBody".to_string(),
                motion_name: "tap".to_string(),
                value: "event".to_string(),
                time_seconds: 0.2,
            }],
            pending_started: vec![MotionStartedEvent {
                handle: 7,
                group_name: "TapBody".to_string(),
                motion_name: "tap".to_string(),
                priority: MotionPriority::Normal,
            }],
            pending_finished: vec![MotionFinishedEvent {
                handle: 7,
                group_name: "TapBody".to_string(),
                motion_name: "tap".to_string(),
                priority: MotionPriority::Normal,
            }],
            pending_sounds: vec!["sound.wav".to_string()],
            ..Default::default()
        };

        player.reset_state();

        assert!(player.motion_queue.is_empty());
        assert_eq!(player.current_priority(), MotionPriority::None);
        assert_eq!(player.reserve_priority(), MotionPriority::None);
        assert!(player.user_time_seconds.abs() < 0.0001);
        assert!(player.take_fired_events().is_empty());
        assert!(player.take_started_motions().is_empty());
        assert!(player.take_finished_motions().is_empty());
        assert!(player.take_started_sounds().is_empty());
        assert_eq!(player.next_handle, 42);
    }

    #[test]
    fn motion_handles_follow_queue_lifetime() {
        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "TapBody".to_string(),
                motions: vec![dummy_motion("tap")],
            }],
            ..Default::default()
        };

        let handle = player
            .start_motion_priority("TapBody", 0, MotionPriority::Normal)
            .expect("motion should start");

        assert!(!player.is_finished_handle(handle));

        player.motion_queue[0].finished = true;
        player.motion_queue.retain(|entry| !entry.finished);

        assert!(player.is_finished_handle(handle));
        assert!(player.is_finished_handle(INVALID_MOTION_HANDLE));
        assert!(player.is_finished_handle(handle + 999));
    }

    #[test]
    fn started_and_finished_motion_events_are_queued() {
        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "TapBody".to_string(),
                motions: vec![dummy_motion("tap")],
            }],
            ..Default::default()
        };

        let handle = player
            .start_motion_priority("TapBody", 0, MotionPriority::Normal)
            .expect("motion should start");

        assert!(player.take_started_motions().is_empty());

        let mut model = dummy_model();
        assert!(player.update(&mut model, 0.0));
        let started = player.take_started_motions();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].handle, handle);
        assert_eq!(started[0].motion_name, "tap");
        assert!(player.update(&mut model, 1.1));

        let finished = player.take_finished_motions();
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].handle, handle);
        assert_eq!(finished[0].group_name, "TapBody");
    }

    #[test]
    fn motion_finishes_when_elapsed_hits_exact_duration() {
        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "TapBody".to_string(),
                motions: vec![dummy_motion("tap")],
            }],
            ..Default::default()
        };

        let handle = player
            .start_motion_priority("TapBody", 0, MotionPriority::Normal)
            .expect("motion should start");

        let mut model = dummy_model();
        assert!(player.update(&mut model, 0.0));
        assert!(player.update(&mut model, 1.0));
        assert!(player.is_finished_handle(handle));

        let finished = player.take_finished_motions();
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].handle, handle);
    }

    #[test]
    fn began_and_finished_handlers_are_called() {
        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "TapBody".to_string(),
                motions: vec![dummy_motion("tap")],
            }],
            ..Default::default()
        };
        let began = Rc::new(RefCell::new(Vec::<MotionHandle>::new()));
        let finished = Rc::new(RefCell::new(Vec::<MotionHandle>::new()));

        {
            let began = Rc::clone(&began);
            player.set_began_motion_handler(move |event| began.borrow_mut().push(event.handle));
        }
        {
            let finished = Rc::clone(&finished);
            player
                .set_finished_motion_handler(move |event| finished.borrow_mut().push(event.handle));
        }

        let handle = player
            .start_motion_priority("TapBody", 0, MotionPriority::Normal)
            .expect("motion should start");
        assert!(began.borrow().is_empty());

        let mut model = dummy_model();
        assert!(player.update(&mut model, 0.0));
        assert_eq!(*began.borrow(), vec![handle]);
        assert!(player.update(&mut model, 1.1));
        assert_eq!(*finished.borrow(), vec![handle]);
    }

    #[test]
    fn started_motion_event_is_emitted_when_motion_first_updates() {
        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "TapBody".to_string(),
                motions: vec![dummy_motion("tap")],
            }],
            ..Default::default()
        };

        let handle = player
            .start_motion_priority("TapBody", 0, MotionPriority::Normal)
            .expect("motion should start");
        assert!(player.take_started_motions().is_empty());

        let mut model = dummy_model();
        assert!(player.update(&mut model, 0.0));

        let started = player.take_started_motions();
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].handle, handle);
    }

    #[test]
    fn setup_motion_queue_entry_initializes_official_time_fields() {
        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "TapBody".to_string(),
                motions: vec![dummy_motion("tap")],
            }],
            ..Default::default()
        };

        let handle = player
            .start_motion_priority("TapBody", 0, MotionPriority::Normal)
            .expect("motion should start");
        assert_eq!(handle, 1);
        assert!(!player.motion_queue[0].started);

        let mut model = dummy_model();
        assert!(player.update(&mut model, 0.25));

        let entry = &player.motion_queue[0];
        assert!(entry.started);
        assert_eq!(entry.start_time_seconds, 0.25);
        assert_eq!(entry.fade_in_start_time_seconds, 0.25);
        assert_eq!(entry.last_event_check_seconds, 0.25);
        assert!(entry.end_time_seconds > entry.start_time_seconds);
        assert_eq!(entry.state_time_seconds, 0.25);
    }

    #[test]
    fn looping_motion_resets_fade_in_start_when_loop_fade_in_enabled() {
        let mut looping = dummy_motion("loop");
        looping.duration = 1.0;
        looping.is_looping = true;
        looping.fade_in_seconds = 0.5;
        looping.is_loop_fade_in_enabled = true;

        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "Idle".to_string(),
                motions: vec![looping],
            }],
            idle_group_index: Some(0),
            ..Default::default()
        };

        let _ = player.start_idle_motion_if_finished();
        let mut model = dummy_model();
        assert!(player.update(&mut model, 0.0));
        assert!(player.update(&mut model, 0.6));
        let first_fade_in_start = player.motion_queue[0].fade_in_start_time_seconds;
        assert!(player.update(&mut model, 0.6));

        assert!(player.motion_queue[0].start_time_seconds > 0.0);
        assert!(player.motion_queue[0].fade_in_start_time_seconds > first_fade_in_start);
    }

    #[test]
    fn looping_motion_waits_for_correction_span_before_restarting() {
        let mut looping = dummy_motion("loop");
        looping.duration = 1.0;
        looping.source_frame_rate = 2.0;
        looping.is_looping = true;

        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "Idle".to_string(),
                motions: vec![looping],
            }],
            idle_group_index: Some(0),
            ..Default::default()
        };

        let _ = player.start_idle_motion_if_finished();
        let mut model = dummy_model();
        assert!(player.update(&mut model, 0.0));
        assert!(player.update(&mut model, 1.25));

        let start_before_correction = player.motion_queue[0].start_time_seconds;
        assert!((start_before_correction - 0.0).abs() < 0.0001);

        assert!(player.update(&mut model, 0.26));
        assert!(player.motion_queue[0].start_time_seconds > start_before_correction);
    }

    fn dummy_model() -> Live2DModel {
        let moc_bytes = std::fs::read(
            "CubismSdkForNative/CubismSdkForNative-5-r.5/Samples/Resources/Haru/Haru.moc3",
        )
        .expect("sample moc3 should exist");
        Live2DModel::from_moc3_bytes(&moc_bytes).expect("sample moc3 should load")
    }

    #[test]
    fn looping_motion_events_fire_across_multiple_wraps() {
        let mut looping = dummy_motion("loop");
        looping.duration = 1.0;
        looping.is_looping = true;
        looping.user_events = vec![
            MotionUserEvent {
                time_seconds: 0.2,
                value: "a".to_string(),
            },
            MotionUserEvent {
                time_seconds: 0.7,
                value: "b".to_string(),
            },
        ];

        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "Idle".to_string(),
                motions: vec![looping],
            }],
            idle_group_index: Some(0),
            ..Default::default()
        };

        player.collect_fired_events(0, 0, 0.6, 2.3);

        let events = player.take_fired_events();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].value, "b");
        assert_eq!(events[1].value, "a");
        assert_eq!(events[2].value, "b");
        assert_eq!(events[3].value, "a");
    }

    #[test]
    fn motion_events_use_exclusive_lower_and_inclusive_upper_bounds() {
        let mut motion = dummy_motion("tap");
        motion.duration = 1.0;
        motion.user_events = vec![
            MotionUserEvent {
                time_seconds: 0.2,
                value: "lower".to_string(),
            },
            MotionUserEvent {
                time_seconds: 0.8,
                value: "upper".to_string(),
            },
        ];

        let mut player = Live2DMotionPlayer {
            groups: vec![MotionGroup {
                name: "TapBody".to_string(),
                motions: vec![motion],
            }],
            ..Default::default()
        };

        player.collect_fired_events(0, 0, 0.2, 0.8);

        let events = player.take_fired_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value, "upper");
    }
}
