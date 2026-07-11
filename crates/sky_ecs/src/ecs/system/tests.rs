use super::*;
use crate::ecs::{CommandBuffer, Commands, EntityId, Time, World};
use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Position(f32);

#[derive(Clone, Copy)]
struct Velocity(f32);

#[derive(Default)]
struct Trace(Vec<&'static str>);

fn movement(entities: View<(&mut Position, &Velocity)>) {
    entities.for_each(|(position, velocity)| position.0 += velocity.0);
}

fn parallel_movement(entities: ParView<(&mut Position, &Velocity)>) {
    entities.par_for_each(|(position, velocity)| position.0 += velocity.0);
}

fn record_a(mut trace: ResMut<Trace>) {
    trace.0.push("a");
}

fn record_b(mut trace: ResMut<Trace>) {
    trace.0.push("b");
}

fn queue_spawn(mut commands: Commands<'_>) {
    commands.spawn((Position(7.0),));
}

#[test]
fn typed_system_reads_and_writes_components() {
    let mut world = World::new();
    world.spawn((Position(1.0), Velocity(2.0)));
    world.stage(Update).add(movement);

    world.tick_with_delta(0.016).unwrap();

    assert_eq!(world.query::<&Position>().count(), 1);
    world
        .query::<&Position>()
        .for_each(|position| assert_eq!(*position, Position(3.0)));
}

#[test]
fn par_view_prepares_parallel_jobs_explicitly() {
    let mut world = World::new();
    world.spawn((Position(1.0), Velocity(2.0)));
    world.stage(Update).add(parallel_movement);

    world.tick_with_delta(0.016).unwrap();
    world.spawn((Position(3.0), Velocity(4.0)));
    world.tick_with_delta(0.016).unwrap();

    let mut positions = Vec::new();
    world
        .query::<&Position>()
        .for_each(|position| positions.push(position.0));
    assert_eq!(positions, vec![5.0, 7.0]);
}

#[test]
fn conflicting_resource_writes_preserve_registration_order() {
    let mut world = World::new();
    world.insert_resource(Trace::default());
    world.stage(Update).add(record_a).add(record_b);

    let report = world.tick_with_delta(0.016).unwrap();

    assert_eq!(world.get_resource::<Trace>().unwrap().0, vec!["a", "b"]);
    assert!(report.waves_run >= 2);
}

#[test]
fn commands_apply_at_stage_boundary() {
    let mut world = World::new();
    world.stage(Update).add_named("queue_spawn", queue_spawn);
    world.stage(PostUpdate).add(|entities: View<&Position>| {
        assert_eq!(entities.count(), 1);
    });

    world.tick_with_delta(0.016).unwrap();
    assert_eq!(world.entity_count(), 1);

    let diagnostics = world.schedule_diagnostics();
    let system = diagnostics
        .stages
        .iter()
        .flat_map(|stage| &stage.segments)
        .flat_map(|segment| &segment.waves)
        .flatten()
        .find(|system| system.name == "queue_spawn")
        .unwrap();
    assert_eq!(system.commands.last_enqueued, 1);
    assert_eq!(system.commands.last_applied, 1);
    assert_eq!(system.commands.last_discarded, 0);
    assert_eq!(system.commands.total_enqueued, 1);
    assert_eq!(system.commands.total_applied, 1);
}

#[test]
fn exclusive_system_is_a_barrier() {
    let mut world = World::new();
    world.insert_resource(Trace::default());
    world
        .stage(Update)
        .add(record_a)
        .add_exclusive(|world: &mut World| {
            world.get_resource_mut::<Trace>().unwrap().0.push("x");
        })
        .add(record_b);

    world.tick_with_delta(0.016).unwrap();
    assert_eq!(
        world.get_resource::<Trace>().unwrap().0,
        vec!["a", "x", "b"]
    );
}

#[test]
fn command_buffer_remains_available_for_manual_use() {
    let mut world = World::new();
    let entity = world.spawn((Position(1.0),));
    let mut commands = CommandBuffer::new();
    commands.despawn(entity);
    commands.apply(&mut world);
    assert!(!world.contains(entity));
}

struct PanicOnDrop(bool);

impl Drop for PanicOnDrop {
    fn drop(&mut self) {
        assert!(!self.0, "intentional payload drop panic");
    }
}

#[test]
fn command_buffer_restores_empty_invariant_when_apply_panics() {
    let mut world = World::new();
    world.insert_resource(PanicOnDrop(true));
    let mut commands = CommandBuffer::new();
    commands.insert_resource(PanicOnDrop(false));
    commands.spawn((Position(3.0),));

    let panic = catch_unwind(AssertUnwindSafe(|| commands.apply(&mut world)));

    assert!(panic.is_err());
    assert!(commands.is_empty());
    assert_eq!(commands.len(), 0);
    assert_eq!(world.entity_count(), 0);
    assert!(world.is_poisoned());

    let tick = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.tick_with_delta(0.016);
    }));
    assert!(tick.is_err());

    commands.spawn((Position(8.0),));
    let second_apply = catch_unwind(AssertUnwindSafe(|| commands.apply(&mut world)));
    assert!(second_apply.is_err());
    assert_eq!(commands.len(), 1);
}

#[test]
fn scheduled_command_apply_panic_poison_is_visible_after_schedule_restoration() {
    let mut world = World::new();
    world.insert_resource(PanicOnDrop(true));
    world.stage(Update).add(|mut commands: Commands<'_>| {
        commands.insert_resource(PanicOnDrop(false));
    });

    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.tick_with_delta(0.016);
    }));
    assert!(panic.is_err());
    assert!(world.is_poisoned());
    assert!(world.schedule_diagnostics().stages.len() >= 6);

    let retry = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.tick_with_delta(0.016);
    }));
    assert!(retry.is_err());
    world.shutdown();
}

#[test]
fn command_buffer_restores_empty_invariant_when_clear_panics() {
    let mut world = World::new();
    let mut commands = CommandBuffer::new();
    commands.insert_resource(PanicOnDrop(true));

    let panic = catch_unwind(AssertUnwindSafe(|| commands.clear()));

    assert!(panic.is_err());
    assert!(commands.is_empty());
    commands.spawn((Position(4.0),));
    commands.apply(&mut world);
    assert_eq!(world.entity_count(), 1);
}

#[test]
fn entity_id_type_remains_send_for_command_writers() {
    fn assert_send<T: Send>() {}
    assert_send::<EntityId>();
}

#[derive(Default)]
struct ReadCountA(usize);

#[derive(Default)]
struct ReadCountB(usize);

fn count_positions_a(positions: View<&Position>, mut count: ResMut<ReadCountA>) {
    count.0 = positions.count();
}

fn count_positions_b(positions: View<&Position>, mut count: ResMut<ReadCountB>) {
    count.0 = positions.count();
}

#[test]
fn compatible_reads_share_one_wave() {
    let mut world = World::new();
    world.spawn((Position(1.0),));
    world.insert_resource(ReadCountA::default());
    world.insert_resource(ReadCountB::default());
    world
        .stage(Update)
        .add(count_positions_a)
        .add(count_positions_b);

    let report = world.tick_with_delta(0.016).unwrap();

    assert_eq!(report.waves_run, 1);
    assert_eq!(report.parallel_waves_run, 0);
    assert_eq!(report.sequential_waves_run, 1);
    assert_eq!(report.systems_run, 2);
    assert_eq!(world.get_resource::<ReadCountA>().unwrap().0, 1);
    assert_eq!(world.get_resource::<ReadCountB>().unwrap().0, 1);
}

#[test]
fn parallel_wave_threshold_is_predictable_and_configurable() {
    let mut parallel_world = World::new();
    parallel_world
        .stage(Update)
        .add(|| {})
        .add(|| {})
        .add(|| {});
    let parallel_report = parallel_world.tick_with_delta(0.016).unwrap();
    if rayon::current_num_threads() > 1 {
        assert_eq!(parallel_report.parallel_waves_run, 1);
        assert_eq!(parallel_report.sequential_waves_run, 0);
    } else {
        assert_eq!(parallel_report.parallel_waves_run, 0);
        assert_eq!(parallel_report.sequential_waves_run, 1);
    }

    let mut sequential_world = World::new();
    sequential_world
        .stage(Update)
        .parallel_wave_min_systems(4)
        .unwrap()
        .add(|| {})
        .add(|| {})
        .add(|| {});
    let sequential_report = sequential_world.tick_with_delta(0.016).unwrap();
    assert_eq!(sequential_report.parallel_waves_run, 0);
    assert_eq!(sequential_report.sequential_waves_run, 1);

    assert!(matches!(
        sequential_world.stage(Update).parallel_wave_min_systems(1),
        Err(ScheduleBuildError::InvalidParallelWaveMinimum(1))
    ));
}

#[derive(Default)]
struct ObservedPosition(f32);

fn increment_position(positions: View<&mut Position>) {
    positions.for_each(|position| position.0 += 1.0);
}

fn observe_position(positions: View<&Position>, mut observed: ResMut<ObservedPosition>) {
    positions.for_each(|position| observed.0 = position.0);
}

fn double_position(positions: View<&mut Position>) {
    positions.for_each(|position| position.0 *= 2.0);
}

#[test]
fn conflicting_component_access_forms_stable_ordered_waves() {
    let mut world = World::new();
    world.spawn((Position(1.0),));
    world.insert_resource(ObservedPosition::default());
    world
        .stage(Update)
        .add(increment_position)
        .add(observe_position)
        .add(double_position);

    let report = world.tick_with_delta(0.016).unwrap();

    assert_eq!(report.waves_run, 3);
    assert_eq!(world.get_resource::<ObservedPosition>().unwrap().0, 2.0);
    world
        .query::<&Position>()
        .for_each(|position| assert_eq!(position.0, 4.0));
}

#[derive(Default)]
struct LocalTraceA(Vec<u32>);

#[derive(Default)]
struct LocalTraceB(Vec<u32>);

fn local_a(mut local: Local<u32>, mut trace: ResMut<LocalTraceA>) {
    *local += 1;
    trace.0.push(*local);
}

fn local_b(mut local: Local<u32>, mut trace: ResMut<LocalTraceB>) {
    *local += 1;
    trace.0.push(*local);
}

#[test]
fn local_state_is_persistent_and_isolated_per_system() {
    let mut world = World::new();
    world.insert_resource(LocalTraceA::default());
    world.insert_resource(LocalTraceB::default());
    world.stage(Update).add(local_a).add(local_b);

    world.tick_with_delta(0.016).unwrap();
    world.tick_with_delta(0.016).unwrap();

    assert_eq!(world.get_resource::<LocalTraceA>().unwrap().0, vec![1, 2]);
    assert_eq!(world.get_resource::<LocalTraceB>().unwrap().0, vec![1, 2]);
}

struct RequiredResource(u32);

#[derive(Default)]
struct RequiredValue(u32);

fn requires_resource(required: Res<RequiredResource>, mut value: ResMut<RequiredValue>) {
    value.0 = required.0;
}

#[test]
fn missing_resource_is_reported_and_schedule_can_retry() {
    let mut world = World::new();
    world.insert_resource(RequiredValue::default());
    world.insert_resource(ReadCountA::default());
    world
        .stage(Update)
        .add(|mut runs: ResMut<ReadCountA>| runs.0 += 1);
    world
        .stage(PostUpdate)
        .add_named("requires_resource", requires_resource);

    let error = world.tick_with_delta(0.016).unwrap_err();
    assert_eq!(
        error,
        ScheduleError::MissingResource {
            system: "requires_resource".to_owned(),
            resource: std::any::type_name::<RequiredResource>(),
        }
    );
    assert_eq!(world.get_resource::<ReadCountA>().unwrap().0, 0);
    assert_eq!(world.time.frame_count, 0);
    assert_eq!(world.time.elapsed, 0.0);

    world.insert_resource(RequiredResource(42));
    world.tick_with_delta(0.016).unwrap();
    assert_eq!(world.get_resource::<ReadCountA>().unwrap().0, 1);
    assert_eq!(world.time.frame_count, 1);
    assert_eq!(world.get_resource::<RequiredValue>().unwrap().0, 42);
}

#[test]
fn failed_wall_clock_tick_does_not_consume_the_first_clock_sample() {
    let mut world = World::new();
    world.insert_resource(RequiredValue::default());
    world.stage(Update).add(requires_resource);

    assert!(matches!(
        world.tick(),
        Err(ScheduleError::MissingResource { .. })
    ));

    world.insert_resource(RequiredResource(3));
    world.tick().unwrap();
    assert_eq!(world.time.raw_delta, 0.0);
    assert_eq!(world.time.frame_count, 1);
}

#[test]
fn removing_a_required_resource_mid_frame_is_an_invariant_panic() {
    let mut world = World::new();
    world.insert_resource(RequiredResource(7));
    world.insert_resource(RequiredValue::default());
    let mut remove_once = true;
    world.stage(Update).add_exclusive(move |world: &mut World| {
        if std::mem::take(&mut remove_once) {
            world.remove_resource::<RequiredResource>();
        }
    });
    world
        .stage(PostUpdate)
        .add_named("requires_resource", requires_resource);

    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.tick_with_delta(0.016);
    }));
    assert!(panic.is_err());

    world.insert_resource(RequiredResource(11));
    world.tick_with_delta(0.016).unwrap();
    assert_eq!(world.get_resource::<RequiredValue>().unwrap().0, 11);
}

struct RemoveRequiredDuringInit;

impl ExclusiveSystem for RemoveRequiredDuringInit {
    fn init(&mut self, world: &mut World) {
        world.remove_resource::<RequiredResource>();
    }

    fn run(&mut self, _world: &mut World) {}
}

#[test]
fn initialization_cannot_silently_invalidate_frame_resources() {
    let mut world = World::new();
    world.insert_resource(RequiredResource(5));
    world.insert_resource(RequiredValue::default());
    world.stage(First).add_exclusive(RemoveRequiredDuringInit);
    world.stage(FixedUpdate).add(requires_resource);

    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.tick_with_delta(0.0);
    }));
    assert!(panic.is_err());
    assert_eq!(world.time.frame_count, 0);

    world.insert_resource(RequiredResource(9));
    world.tick_with_delta(1.0 / 60.0).unwrap();
    assert_eq!(world.get_resource::<RequiredValue>().unwrap().0, 9);
}

#[derive(Default)]
struct ConfigValue(u32);

#[derive(Default)]
struct ConfigTrace(Vec<u32>);

fn read_config(config: Res<ConfigValue>, mut trace: ResMut<ConfigTrace>) {
    trace.0.push(config.0);
}

#[test]
fn resource_cache_refreshes_after_replacement() {
    let mut world = World::new();
    world.insert_resource(ConfigValue(1));
    world.insert_resource(ConfigTrace::default());
    world.stage(Update).add(read_config);

    world.tick_with_delta(0.016).unwrap();
    world.insert_resource(ConfigValue(2));
    world.tick_with_delta(0.016).unwrap();

    assert_eq!(world.get_resource::<ConfigTrace>().unwrap().0, vec![1, 2]);
}

#[derive(Default)]
struct FrameTrace(Vec<u64>);

fn read_time(time: Res<Time>, mut trace: ResMut<FrameTrace>) {
    trace.0.push(time.frame_count);
}

#[test]
fn time_pointer_refreshes_when_world_moves() {
    let mut world = World::new();
    world.insert_resource(FrameTrace::default());
    world.stage(Update).add(read_time);
    world.tick_with_delta(0.016).unwrap();

    let mut other = World::new();
    std::mem::swap(&mut world, &mut other);
    other.tick_with_delta(0.016).unwrap();

    assert_eq!(other.get_resource::<FrameTrace>().unwrap().0, vec![1, 2]);
}

#[test]
fn time_is_a_permanent_resource_view_of_world_frame_state() {
    let mut world = World::new();
    assert!(world.contains_resource::<Time>());
    assert!(std::ptr::eq(
        world.get_resource::<Time>().unwrap(),
        &world.time
    ));

    world.get_resource_mut::<Time>().unwrap().time_scale = 0.5;
    assert_eq!(world.time.time_scale, 0.5);

    let insert = catch_unwind(AssertUnwindSafe(|| {
        world.insert_resource(Time::default());
    }));
    assert!(insert.is_err());
    let remove = catch_unwind(AssertUnwindSafe(|| {
        world.remove_resource::<Time>();
    }));
    assert!(remove.is_err());
}

#[derive(Clone, Copy)]
struct SpawnOrder(u8);

fn slow_first_spawn(mut commands: Commands<'_>) {
    std::thread::sleep(Duration::from_millis(10));
    commands.spawn((SpawnOrder(1),));
}

fn fast_second_spawn(mut commands: Commands<'_>) {
    commands.spawn((SpawnOrder(2),));
}

#[test]
fn command_merge_order_does_not_depend_on_worker_completion() {
    let mut world = World::new();
    world
        .stage(Update)
        .add(slow_first_spawn)
        .add(fast_second_spawn);

    let report = world.tick_with_delta(0.016).unwrap();
    let mut order = Vec::new();
    world
        .query::<&SpawnOrder>()
        .for_each(|marker| order.push(marker.0));

    assert_eq!(report.waves_run, 1);
    assert_eq!(order, vec![1, 2]);
}

#[derive(Default)]
struct VisibilityToken;

#[derive(Default)]
struct ObservedCount(usize);

fn queue_before_later_wave(mut commands: Commands<'_>, _token: ResMut<VisibilityToken>) {
    commands.spawn((Position(9.0),));
}

fn count_in_later_wave(
    positions: View<&Position>,
    _token: Res<VisibilityToken>,
    mut observed: ResMut<ObservedCount>,
) {
    observed.0 = positions.count();
}

#[test]
fn automatic_wave_boundaries_do_not_flush_commands() {
    let mut world = World::new();
    world.insert_resource(VisibilityToken);
    world.insert_resource(ObservedCount::default());
    world
        .stage(Update)
        .add(queue_before_later_wave)
        .add(count_in_later_wave);

    let report = world.tick_with_delta(0.016).unwrap();

    assert_eq!(report.waves_run, 2);
    assert_eq!(world.get_resource::<ObservedCount>().unwrap().0, 0);
    assert_eq!(world.query::<&Position>().count(), 1);
}

#[test]
fn panic_discards_pending_commands_and_restores_schedule() {
    let should_panic = Arc::new(AtomicBool::new(true));
    let system_flag = Arc::clone(&should_panic);
    let mut world = World::new();
    world.stage(Update).add(move |mut commands: Commands<'_>| {
        commands.spawn((Position(5.0),));
        if system_flag.swap(false, Ordering::SeqCst) {
            panic!("intentional system panic");
        }
    });

    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.tick_with_delta(0.016);
    }));
    assert!(panic.is_err());
    assert_eq!(world.entity_count(), 0);
    assert!(!world.is_poisoned());

    let diagnostics = world.schedule_diagnostics();
    let commands = diagnostics
        .stages
        .iter()
        .find(|stage| stage.name == std::any::type_name::<Update>())
        .unwrap()
        .segments[0]
        .waves[0][0]
        .commands;
    assert_eq!(commands.last_enqueued, 1);
    assert_eq!(commands.last_applied, 0);
    assert_eq!(commands.last_discarded, 1);
    assert_eq!(commands.total_discarded, 1);

    world.tick_with_delta(0.016).unwrap();
    assert_eq!(world.entity_count(), 1);

    let diagnostics = world.schedule_diagnostics();
    let commands = diagnostics
        .stages
        .iter()
        .find(|stage| stage.name == std::any::type_name::<Update>())
        .unwrap()
        .segments[0]
        .waves[0][0]
        .commands;
    assert_eq!(commands.last_enqueued, 1);
    assert_eq!(commands.last_applied, 1);
    assert_eq!(commands.last_discarded, 0);
    assert_eq!(commands.total_enqueued, 2);
    assert_eq!(commands.total_applied, 1);
    assert_eq!(commands.total_discarded, 1);
}

#[test]
fn panicking_command_payload_cannot_abort_schedule_unwind_cleanup() {
    let first = Arc::new(AtomicBool::new(true));
    let system_first = Arc::clone(&first);
    let mut world = World::new();
    world.stage(Update).add(move |mut commands: Commands<'_>| {
        if system_first.swap(false, Ordering::SeqCst) {
            commands.insert_resource(PanicOnDrop(true));
            panic!("primary system panic");
        }
    });

    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.tick_with_delta(0.016);
    }));
    assert!(panic.is_err());

    world.tick_with_delta(0.016).unwrap();
    assert!(!world.contains_resource::<PanicOnDrop>());
}

#[derive(Default)]
struct FixedDeltas(Vec<f32>);

struct ExtraFixed;

impl StageLabel for ExtraFixed {}

struct StageA;
struct StageNested;
struct StageB;
struct UninstalledStage;

impl StageLabel for StageA {}
impl StageLabel for StageNested {}
impl StageLabel for StageB {}
impl StageLabel for UninstalledStage {}

#[test]
fn custom_stages_must_be_installed_explicitly() {
    let mut world = World::new();
    assert!(matches!(
        world.try_stage(UninstalledStage),
        Err(ScheduleBuildError::UnknownStage(name))
            if name == std::any::type_name::<UninstalledStage>()
    ));

    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _ = world.stage(UninstalledStage);
    }));
    assert!(panic.is_err());
    assert!(!world
        .schedule_diagnostics()
        .stages
        .iter()
        .any(|stage| stage.name == std::any::type_name::<UninstalledStage>()));
}

#[test]
fn repeated_stage_insertions_preserve_sibling_and_subtree_order() {
    let mut world = World::new();
    world.insert_resource(Trace::default());
    world.insert_stage_after(Update, StageA).unwrap();
    world.insert_stage_after(Update, StageB).unwrap();
    world.insert_stage_after(StageA, StageNested).unwrap();

    world
        .stage(Update)
        .add_exclusive(|world: &mut World| world.get_resource_mut::<Trace>().unwrap().0.push("u"));
    world
        .stage(StageA)
        .add_exclusive(|world: &mut World| world.get_resource_mut::<Trace>().unwrap().0.push("a"));
    world.stage(StageNested).add_exclusive(|world: &mut World| {
        world.get_resource_mut::<Trace>().unwrap().0.push("nested");
    });
    world
        .stage(StageB)
        .add_exclusive(|world: &mut World| world.get_resource_mut::<Trace>().unwrap().0.push("b"));

    world.tick_with_delta(0.0).unwrap();

    assert_eq!(
        world.get_resource::<Trace>().unwrap().0,
        vec!["u", "a", "nested", "b"]
    );
}

fn record_fixed_delta(time: Res<Time>, mut deltas: ResMut<FixedDeltas>) {
    deltas.0.push(time.delta);
}

#[test]
fn fixed_update_has_an_explicit_60_hz_default() {
    let mut world = World::new();
    world.insert_resource(FixedDeltas::default());
    world.stage(FixedUpdate).add(record_fixed_delta);

    let report = world.tick_with_delta(1.0 / 60.0).unwrap();

    assert_eq!(report.fixed_substeps, 1);
    let deltas = &world.get_resource::<FixedDeltas>().unwrap().0;
    assert_eq!(deltas.len(), 1);
    assert!((deltas[0] - 1.0 / 60.0).abs() < 1.0e-6);
}

#[test]
fn explicit_fixed_configuration_is_idempotent_but_not_last_writer_wins() {
    let mut world = World::new();
    let step = FixedStep::hz(50);
    let mut stage = world.stage(FixedUpdate);
    assert!(stage.fixed(step).is_ok());
    assert!(stage.fixed(step).is_ok());
    assert!(stage
        .fixed(FixedStep::seconds(f64::from(1.0_f32 / 50.0)).unwrap())
        .is_ok());
    assert!(matches!(
        stage.fixed(FixedStep::hz(60)),
        Err(ScheduleBuildError::ConflictingFixedStep(name))
            if name == std::any::type_name::<FixedUpdate>()
    ));
}

#[test]
fn fixed_alpha_belongs_to_builtin_fixed_update_only() {
    let mut world = World::new();
    world.insert_stage_after(FixedUpdate, ExtraFixed).unwrap();
    world
        .stage(ExtraFixed)
        .fixed(FixedStep::seconds(0.1).unwrap())
        .unwrap();

    world.tick_with_delta(0.01).unwrap();

    assert!((world.time.fixed_alpha - 0.6).abs() < 1.0e-5);
}

#[test]
fn fixed_drop_is_bounded_and_reports_discarded_time() {
    let mut world = World::new();
    world.insert_resource(FixedDeltas::default());
    world
        .stage(FixedUpdate)
        .fixed(
            FixedStep::seconds(0.1)
                .unwrap()
                .max_substeps(2)
                .overflow(FixedOverflow::Drop),
        )
        .unwrap()
        .add(record_fixed_delta);

    let report = world.tick_with_delta(0.35).unwrap();

    assert_eq!(report.fixed_substeps, 2);
    assert!((report.dropped_fixed_time - 0.1).abs() < 1.0e-5);
    assert!((world.time.fixed_alpha - 0.5).abs() < 1.0e-5);
    assert_eq!(world.get_resource::<FixedDeltas>().unwrap().0.len(), 2);
}

#[test]
fn fixed_carry_retains_backlog_for_the_next_tick() {
    let mut world = World::new();
    world.insert_resource(FixedDeltas::default());
    world
        .stage(FixedUpdate)
        .fixed(
            FixedStep::seconds(0.1)
                .unwrap()
                .max_substeps(2)
                .overflow(FixedOverflow::Carry),
        )
        .unwrap()
        .add(record_fixed_delta);

    let first = world.tick_with_delta(0.35).unwrap();
    let second = world.tick_with_delta(0.0).unwrap();

    assert_eq!(first.fixed_substeps, 2);
    assert_eq!(first.dropped_fixed_time, 0.0);
    assert_eq!(second.fixed_substeps, 1);
    assert!((world.time.fixed_alpha - 0.5).abs() < 1.0e-5);
}

#[test]
fn invalid_fixed_steps_are_rejected() {
    assert!(FixedStep::try_hz(0).is_err());
    assert!(FixedStep::seconds(0.0).is_err());
    assert!(FixedStep::seconds(-1.0).is_err());
    assert!(FixedStep::seconds(f64::NAN).is_err());
    assert!(FixedStep::seconds(f64::INFINITY).is_err());
}

#[test]
fn non_finite_tick_inputs_do_not_poison_time_or_fixed_accumulators() {
    let mut world = World::new();
    world.insert_resource(FixedDeltas::default());
    world
        .stage(FixedUpdate)
        .fixed(FixedStep::seconds(0.1).unwrap())
        .unwrap()
        .add(record_fixed_delta);

    let report = world
        .tick_with_frame_delta(f32::INFINITY, f32::NAN)
        .unwrap();
    assert_eq!(report.fixed_substeps, 0);
    assert_eq!(world.time.frame_delta, 0.0);
    assert_eq!(world.time.raw_delta, 0.0);
    assert_eq!(world.time.fixed_alpha, 0.0);

    world.time.time_scale = f32::INFINITY;
    let report = world.tick_with_delta(0.25).unwrap();
    assert_eq!(report.fixed_substeps, 0);
    assert_eq!(world.time.frame_delta, 0.0);
    assert!(world.time.elapsed.is_finite());
}

fn invalid_resource_access(_write: ResMut<Trace>, _read: Res<Trace>) {}

fn invalid_component_access(_write: View<&mut Position>, _read: View<&Position>) {}

#[test]
fn overlapping_parameters_are_rejected_at_registration() {
    let mut resource_world = World::new();
    let resource_error = catch_unwind(AssertUnwindSafe(|| {
        resource_world.stage(Update).add(invalid_resource_access);
    }));
    assert!(resource_error.is_err());

    let mut component_world = World::new();
    let component_error = catch_unwind(AssertUnwindSafe(|| {
        component_world.stage(Update).add(invalid_component_access);
    }));
    assert!(component_error.is_err());
}

#[derive(Default)]
struct ArityResult(u32);

#[allow(clippy::too_many_arguments)]
fn sixteen_parameters(
    mut p0: Local<u8>,
    mut p1: Local<u8>,
    mut p2: Local<u8>,
    mut p3: Local<u8>,
    mut p4: Local<u8>,
    mut p5: Local<u8>,
    mut p6: Local<u8>,
    mut p7: Local<u8>,
    mut p8: Local<u8>,
    mut p9: Local<u8>,
    mut p10: Local<u8>,
    mut p11: Local<u8>,
    mut p12: Local<u8>,
    mut p13: Local<u8>,
    mut p14: Local<u8>,
    mut result: ResMut<ArityResult>,
) {
    *p0 += 1;
    *p1 += 1;
    *p2 += 1;
    *p3 += 1;
    *p4 += 1;
    *p5 += 1;
    *p6 += 1;
    *p7 += 1;
    *p8 += 1;
    *p9 += 1;
    *p10 += 1;
    *p11 += 1;
    *p12 += 1;
    *p13 += 1;
    *p14 += 1;
    result.0 = u32::from(*p0)
        + u32::from(*p1)
        + u32::from(*p2)
        + u32::from(*p3)
        + u32::from(*p4)
        + u32::from(*p5)
        + u32::from(*p6)
        + u32::from(*p7)
        + u32::from(*p8)
        + u32::from(*p9)
        + u32::from(*p10)
        + u32::from(*p11)
        + u32::from(*p12)
        + u32::from(*p13)
        + u32::from(*p14);
}

#[test]
fn function_systems_support_sixteen_parameters() {
    let mut world = World::new();
    world.insert_resource(ArityResult::default());
    world.stage(Update).add(sixteen_parameters);

    world.tick_with_delta(0.016).unwrap();
    assert_eq!(world.get_resource::<ArityResult>().unwrap().0, 15);
    world.tick_with_delta(0.016).unwrap();
    assert_eq!(world.get_resource::<ArityResult>().unwrap().0, 30);
}

struct LifecycleSystem {
    name: &'static str,
    events: Rc<RefCell<Vec<String>>>,
}

impl ExclusiveSystem for LifecycleSystem {
    fn init(&mut self, _world: &mut World) {
        self.events.borrow_mut().push(format!("init:{}", self.name));
    }

    fn run(&mut self, _world: &mut World) {
        self.events.borrow_mut().push(format!("run:{}", self.name));
    }

    fn teardown(&mut self, _world: &mut World) {
        self.events
            .borrow_mut()
            .push(format!("teardown:{}", self.name));
    }
}

#[test]
fn exclusive_lifecycle_allows_non_send_state_and_tears_down_in_reverse() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut world = World::new();
    world
        .stage(Update)
        .add_exclusive(LifecycleSystem {
            name: "a",
            events: Rc::clone(&events),
        })
        .add_exclusive(LifecycleSystem {
            name: "b",
            events: Rc::clone(&events),
        });

    world.tick_with_delta(0.016).unwrap();
    world.shutdown();

    assert_eq!(
        *events.borrow(),
        vec![
            "init:a",
            "init:b",
            "run:a",
            "run:b",
            "teardown:b",
            "teardown:a",
        ]
    );
}

#[test]
fn diagnostics_expose_compiled_waves_access_and_exclusive_barriers() {
    let mut world = World::new();
    world
        .stage(Update)
        .add_named("increment", increment_position)
        .add_named("observe", observe_position)
        .add_exclusive_named("barrier", |_world: &mut World| {});

    let diagnostics = world.schedule_diagnostics();
    let update = diagnostics
        .stages
        .iter()
        .find(|stage| stage.name == std::any::type_name::<Update>())
        .unwrap();

    assert_eq!(update.segments[0].waves.len(), 2);
    assert_eq!(update.segments[0].waves[0][0].name, "increment");
    assert_eq!(update.segments[0].waves[1][0].name, "observe");
    assert_eq!(
        update.segments[0].waves[0][0]
            .access
            .conflict_reason(&update.segments[0].waves[1][0].access),
        Some(format!("component `{}`", std::any::type_name::<Position>()))
    );
    assert_eq!(
        update.segments[0].exclusive_after.as_deref(),
        Some("barrier")
    );
}
