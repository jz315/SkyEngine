use super::World;

// ---------------------------------------------------------------------------
// System trait
// ---------------------------------------------------------------------------

/// A runnable unit of logic that operates on a [`World`].
///
/// Implement `init` for one-time setup, `run` for per-tick logic, and
/// `teardown` for cleanup on [`World::shutdown`].
///
/// Closures `FnMut(&mut World)` automatically implement `System`.
pub trait System: 'static {
    fn init(&mut self, _world: &mut World) {}
    fn run(&mut self, world: &mut World);
    fn teardown(&mut self, _world: &mut World) {}
}

impl<F> System for F
where
    F: FnMut(&mut World) + 'static,
{
    fn run(&mut self, world: &mut World) {
        self(world);
    }
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

pub(crate) struct RegisteredSystem {
    pub initialized: bool,
    #[cfg(feature = "profile")]
    pub name: String,
    pub system: Box<dyn System>,
}

impl RegisteredSystem {
    #[cfg(feature = "profile")]
    fn new<S: System>(name: String, system: S) -> Self {
        Self {
            initialized: false,
            name,
            system: Box::new(system),
        }
    }

    #[cfg(not(feature = "profile"))]
    fn new<S: System>(system: S) -> Self {
        Self {
            initialized: false,
            system: Box::new(system),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum TickPolicy {
    EveryFrame,
    Fixed(f32),
}

pub(crate) struct SystemGroup {
    pub name: String,
    pub tick_policy: TickPolicy,
    pub systems: Vec<RegisteredSystem>,
    pub accumulator: f32,
}

impl SystemGroup {
    fn new(name: String) -> Self {
        Self {
            name,
            tick_policy: TickPolicy::EveryFrame,
            systems: Vec::new(),
            accumulator: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Schedule — lives inside World, stores groups.
// Execution logic is in World::tick (to get &mut World for systems).
// ---------------------------------------------------------------------------

#[derive(Default)]
pub(crate) struct Schedule {
    pub groups: Vec<SystemGroup>,
    pub last_tick: Option<std::time::Instant>,
}

impl Schedule {
    pub fn find_or_create_group(&mut self, name: &str) -> usize {
        if let Some(index) = self.groups.iter().position(|g| g.name == name) {
            return index;
        }
        let index = self.groups.len();
        self.groups.push(SystemGroup::new(name.to_string()));
        index
    }
}

// ---------------------------------------------------------------------------
// GroupBuilder — returned by world.group("name")
// ---------------------------------------------------------------------------

/// Builder returned by [`World::group`] for configuring a system group.
pub struct GroupBuilder<'a> {
    schedule: &'a mut Schedule,
    group_index: usize,
}

impl<'a> GroupBuilder<'a> {
    pub(crate) fn new(schedule: &'a mut Schedule, group_index: usize) -> Self {
        Self {
            schedule,
            group_index,
        }
    }

    /// Sets this group to run at a fixed time step (in seconds).
    ///
    /// The group accumulates delta time and runs its systems once per
    /// step, potentially multiple times per frame.
    pub fn fixed(&mut self, dt: f32) -> &mut Self {
        self.schedule.groups[self.group_index].tick_policy = TickPolicy::Fixed(dt);
        self
    }

    /// Adds a system to this group. Systems within a group run in the
    /// order they are added.
    pub fn add<S: System>(&mut self, system: S) -> &mut Self {
        #[cfg(feature = "profile")]
        {
            self.add_named(std::any::type_name::<S>(), system)
        }
        #[cfg(not(feature = "profile"))]
        {
            self.schedule.groups[self.group_index]
                .systems
                .push(RegisteredSystem::new(system));
            self
        }
    }

    /// Adds a system with a stable profiling/debug name.
    pub fn add_named<S: System>(&mut self, name: impl Into<String>, system: S) -> &mut Self {
        #[cfg(feature = "profile")]
        {
            self.schedule.groups[self.group_index]
                .systems
                .push(RegisteredSystem::new(name.into(), system));
        }
        #[cfg(not(feature = "profile"))]
        {
            let _ = name;
            self.schedule.groups[self.group_index]
                .systems
                .push(RegisteredSystem::new(system));
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::System;
    use crate::ecs::World;

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[derive(Default, Debug, PartialEq)]
    struct LifecycleCounts {
        inited: u32,
        ran: u32,
        torn_down: u32,
    }

    #[derive(Default, Debug, PartialEq)]
    struct SeenCount {
        value: usize,
    }

    #[derive(Default, Debug, PartialEq)]
    struct Trace {
        entries: Vec<&'static str>,
    }

    struct LifecycleSystem;

    impl System for LifecycleSystem {
        fn init(&mut self, world: &mut World) {
            world.get_resource_mut::<LifecycleCounts>().unwrap().inited += 1;
            world.spawn((Position { x: 1.0, y: 2.0 },));
        }

        fn run(&mut self, world: &mut World) {
            world.get_resource_mut::<LifecycleCounts>().unwrap().ran += 1;
            let mut query = world.query::<&Position>();
            let count = query.count(world);
            world.get_resource_mut::<SeenCount>().unwrap().value = count;
        }

        fn teardown(&mut self, world: &mut World) {
            world
                .get_resource_mut::<LifecycleCounts>()
                .unwrap()
                .torn_down += 1;
        }
    }

    struct TraceSystem(&'static str);

    impl System for TraceSystem {
        fn run(&mut self, world: &mut World) {
            world
                .get_resource_mut::<Trace>()
                .unwrap()
                .entries
                .push(self.0);
        }
    }

    #[test]
    fn lifecycle_runs_init_once_run_each_and_teardown_on_shutdown() {
        let mut world = World::new();
        world.insert_resource(LifecycleCounts::default());
        world.insert_resource(SeenCount::default());

        world.group("sim").add(LifecycleSystem);

        world.tick_with_delta(0.016);
        world.tick_with_delta(0.016);
        world.shutdown();

        assert_eq!(
            world.get_resource::<LifecycleCounts>(),
            Some(&LifecycleCounts {
                inited: 1,
                ran: 2,
                torn_down: 1,
            })
        );
        assert_eq!(
            world.get_resource::<SeenCount>(),
            Some(&SeenCount { value: 1 })
        );
    }

    #[test]
    fn groups_run_in_creation_order() {
        let mut world = World::new();
        world.insert_resource(Trace::default());

        world.group("first").add(TraceSystem("A"));
        world.group("second").add(TraceSystem("B"));
        world.group("third").add(TraceSystem("C"));

        world.tick_with_delta(0.016);

        assert_eq!(
            world.get_resource::<Trace>(),
            Some(&Trace {
                entries: vec!["A", "B", "C"],
            })
        );
    }

    #[test]
    fn closure_system_works_directly() {
        let mut world = World::new();
        world.insert_resource(SeenCount::default());

        world.group("spawn").add(|world: &mut World| {
            world.spawn((Position { x: 1.0, y: 2.0 },));
        });
        world.group("count").add(|world: &mut World| {
            let mut count = 0usize;
            let mut query = world.query::<&Position>();
            query.for_each(&mut *world, |_position| {
                count += 1;
            });
            world.get_resource_mut::<SeenCount>().unwrap().value = count;
        });

        world.tick_with_delta(0.016);

        assert_eq!(
            world.get_resource::<SeenCount>(),
            Some(&SeenCount { value: 1 })
        );
    }

    #[test]
    fn fixed_group_runs_multiple_times_with_accumulator() {
        let mut world = World::new();
        world.insert_resource(Trace::default());

        world.group("physics").fixed(0.02).add(TraceSystem("tick"));

        world.tick_with_delta(0.05);

        assert_eq!(
            world.get_resource::<Trace>(),
            Some(&Trace {
                entries: vec!["tick", "tick"],
            })
        );
    }

    #[test]
    fn world_time_delta_differs_per_group() {
        let mut world = World::new();

        #[derive(Default, Debug, PartialEq)]
        struct Deltas(Vec<f32>);

        world.insert_resource(Deltas::default());

        world.group("normal").add(|world: &mut World| {
            let dt = world.time.delta;
            world.get_resource_mut::<Deltas>().unwrap().0.push(dt);
        });

        world.group("fixed").fixed(0.02).add(|world: &mut World| {
            let dt = world.time.delta;
            world.get_resource_mut::<Deltas>().unwrap().0.push(dt);
        });

        world.tick_with_delta(0.05);

        let deltas = &world.get_resource::<Deltas>().unwrap().0;
        assert_eq!(deltas[0], 0.05);
        assert_eq!(deltas[1], 0.02);
        assert_eq!(deltas[2], 0.02);
        assert_eq!(world.time.delta, world.time.frame_delta);
        assert_eq!(world.time.frame_delta, 0.05);
        assert_eq!(world.time.raw_delta, 0.05);
    }

    #[test]
    fn world_time_tracks_raw_and_scaled_frame_delta() {
        let mut world = World::new();
        world.time.time_scale = 0.5;

        world.tick_with_frame_delta(0.1, 0.25);

        assert!((world.time.delta - 0.05).abs() < f32::EPSILON);
        assert!((world.time.frame_delta - 0.05).abs() < f32::EPSILON);
        assert_eq!(world.time.raw_delta, 0.25);
        assert!((world.time.elapsed - 0.05).abs() < f32::EPSILON);
        assert_eq!(world.time.raw_elapsed, 0.25);
        assert_eq!(world.time.frame_count, 1);
    }

    #[cfg(feature = "profile")]
    #[test]
    fn add_named_sets_system_profile_name() {
        let mut schedule = super::Schedule::default();
        let index = schedule.find_or_create_group("named");
        super::GroupBuilder::new(&mut schedule, index).add_named("stable_name", |_: &mut World| {});

        assert_eq!(schedule.groups[0].systems[0].name, "stable_name");
    }

    #[test]
    fn appending_to_existing_group_preserves_order() {
        let mut world = World::new();
        world.insert_resource(Trace::default());

        world.group("a").add(TraceSystem("first"));
        world.group("b").add(TraceSystem("second"));
        world.group("a").add(TraceSystem("third"));

        world.tick_with_delta(0.016);

        assert_eq!(
            world.get_resource::<Trace>(),
            Some(&Trace {
                entries: vec!["first", "third", "second"],
            })
        );
    }
}
