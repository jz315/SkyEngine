use super::cell::UnsafeWorldCell;
use super::function::{
    into_boxed_system, ErasedSystem, ExclusiveSystem, IntoSystem, RegisteredExclusive,
};
use super::stage::{
    stage_id, CommandDiagnostics, First, FixedOverflow, FixedStageDiagnostics, FixedStep,
    FixedUpdate, Last, PostUpdate, PreUpdate, ScheduleBuildError, ScheduleDiagnostics,
    ScheduleError, StageDiagnostics, StageLabel, StagePolicy, StageSegmentDiagnostics,
    SystemDiagnostics, TickReport, Update,
};
use crate::ecs::{CommandBuffer, World};
use rayon::prelude::*;
use std::any::TypeId;
use std::time::Instant;

const DEFAULT_PARALLEL_WAVE_MIN_SYSTEMS: usize = 3;

impl CommandDiagnostics {
    fn record_enqueued(&mut self, count: usize) {
        self.last_enqueued = count;
        self.last_applied = 0;
        self.last_discarded = 0;
        self.total_enqueued = self.total_enqueued.saturating_add(count as u64);
    }

    fn record_applied(&mut self, count: usize) {
        self.last_applied = count;
        self.total_applied = self.total_applied.saturating_add(count as u64);
    }

    fn record_discarded(&mut self, count: usize) {
        self.record_enqueued(count);
        self.last_discarded = count;
        self.total_discarded = self.total_discarded.saturating_add(count as u64);
    }
}

#[derive(Clone, Copy)]
enum StageNode {
    Ordinary(usize),
    Exclusive(usize),
}

struct CompiledSegment {
    ordinary: Vec<usize>,
    waves: Vec<Vec<usize>>,
    exclusive_after: Option<usize>,
}

pub(crate) struct SystemStage {
    id: TypeId,
    name: &'static str,
    ancestors: Vec<TypeId>,
    policy: StagePolicy,
    fixed_explicit: bool,
    accumulator: f64,
    parallel_wave_min_systems: usize,
    nodes: Vec<StageNode>,
    ordinary: Vec<Box<dyn ErasedSystem>>,
    commands: Vec<CommandBuffer>,
    command_diagnostics: Vec<CommandDiagnostics>,
    exclusive: Vec<RegisteredExclusive>,
    compiled: Vec<CompiledSegment>,
    dirty: bool,
}

impl SystemStage {
    fn new<L: StageLabel>() -> Self {
        Self::with_ancestors::<L>(Vec::new())
    }

    fn with_ancestors<L: StageLabel>(ancestors: Vec<TypeId>) -> Self {
        assert!(!L::name().is_empty(), "stage label name cannot be empty");
        let is_fixed_update = stage_id::<L>() == stage_id::<FixedUpdate>();
        Self {
            id: stage_id::<L>(),
            name: L::name(),
            ancestors,
            policy: if is_fixed_update {
                StagePolicy::Fixed(FixedStep::hz(60))
            } else {
                StagePolicy::EveryFrame
            },
            fixed_explicit: false,
            accumulator: 0.0,
            parallel_wave_min_systems: DEFAULT_PARALLEL_WAVE_MIN_SYSTEMS,
            nodes: Vec::new(),
            ordinary: Vec::new(),
            commands: Vec::new(),
            command_diagnostics: Vec::new(),
            exclusive: Vec::new(),
            compiled: Vec::new(),
            dirty: true,
        }
    }

    fn compile(&mut self) {
        if !self.dirty {
            return;
        }
        self.compiled.clear();
        let mut pending = Vec::new();
        for node in self.nodes.iter().copied() {
            match node {
                StageNode::Ordinary(index) => pending.push(index),
                StageNode::Exclusive(index) => {
                    self.compiled
                        .push(self.compile_segment(std::mem::take(&mut pending), Some(index)));
                }
            }
        }
        if !pending.is_empty() || self.compiled.is_empty() {
            self.compiled.push(self.compile_segment(pending, None));
        }
        self.dirty = false;
    }

    fn compile_segment(
        &self,
        ordinary: Vec<usize>,
        exclusive_after: Option<usize>,
    ) -> CompiledSegment {
        let mut levels: Vec<(usize, usize)> = Vec::with_capacity(ordinary.len());
        let mut waves: Vec<Vec<usize>> = Vec::new();
        for &index in &ordinary {
            let mut level = 0usize;
            for &(previous, previous_level) in &levels {
                if self.ordinary[index]
                    .access()
                    .conflicts(self.ordinary[previous].access())
                {
                    level = level.max(previous_level + 1);
                }
            }
            if waves.len() <= level {
                waves.resize_with(level + 1, Vec::new);
            }
            waves[level].push(index);
            levels.push((index, level));
        }
        CompiledSegment {
            ordinary,
            waves,
            exclusive_after,
        }
    }

    fn initialize(&mut self, world: &mut World) -> Result<(), ScheduleError> {
        for node in self.nodes.iter().copied() {
            match node {
                StageNode::Ordinary(index) => self.ordinary[index].initialize(world)?,
                StageNode::Exclusive(index) => {
                    let exclusive = &mut self.exclusive[index];
                    if !exclusive.initialized {
                        exclusive.system.init(world);
                        exclusive.initialized = true;
                    }
                }
            }
        }
        Ok(())
    }

    fn execute(&mut self, world: &mut World, report: &mut TickReport) {
        self.compile();
        let parallel_wave_min_systems = self.parallel_wave_min_systems;
        let SystemStage {
            ordinary,
            commands,
            command_diagnostics,
            exclusive,
            compiled,
            ..
        } = self;

        for segment in compiled {
            let CompiledSegment {
                ordinary: segment_systems,
                waves,
                exclusive_after,
            } = segment;

            for &index in segment_systems.iter() {
                ordinary[index].prepare(world).unwrap_or_else(|error| {
                    panic!("schedule resource invariant violated after frame preflight: {error}")
                });
            }

            for wave in waves {
                if wave.is_empty() {
                    continue;
                }
                let ran_parallel =
                    run_wave(ordinary, commands, wave, world, parallel_wave_min_systems);
                report.waves_run = report.waves_run.saturating_add(1);
                if ran_parallel {
                    report.parallel_waves_run = report.parallel_waves_run.saturating_add(1);
                } else {
                    report.sequential_waves_run = report.sequential_waves_run.saturating_add(1);
                }
                report.systems_run = report.systems_run.saturating_add(wave.len() as u32);
            }

            for &index in segment_systems.iter() {
                let count = commands[index].len();
                command_diagnostics[index].record_enqueued(count);
                commands[index].apply(world);
                command_diagnostics[index].record_applied(count);
            }

            if let Some(index) = *exclusive_after {
                let system = &mut exclusive[index];
                debug_assert!(!system.name.is_empty());
                #[cfg(feature = "profile")]
                let _scope =
                    sky_profile::profile_scope!("ecs", format!("exclusive_system:{}", system.name));
                system.system.run(world);
                report.waves_run = report.waves_run.saturating_add(1);
                report.sequential_waves_run = report.sequential_waves_run.saturating_add(1);
                report.systems_run = report.systems_run.saturating_add(1);
            }
        }
    }

    fn discard_commands(&mut self) {
        for (commands, diagnostics) in self.commands.iter_mut().zip(&mut self.command_diagnostics) {
            let count = commands.len();
            if count != 0 {
                diagnostics.record_discarded(count);
            }
            commands.clear();
        }
    }

    fn shutdown(&mut self, world: &mut World) {
        for node in self.nodes.iter().rev().copied() {
            match node {
                StageNode::Ordinary(index) => self.ordinary[index].shutdown(),
                StageNode::Exclusive(index) => {
                    let exclusive = &mut self.exclusive[index];
                    if exclusive.initialized {
                        exclusive.system.teardown(world);
                        exclusive.initialized = false;
                    }
                }
            }
        }
    }
}

struct SystemBase(*mut Box<dyn ErasedSystem>);
struct CommandBase(*mut CommandBuffer);
struct WorldBase(*mut World);

// Safety: a compiled wave contains unique indices and pairwise-disjoint access
// sets. Vectors are not structurally modified until every Rayon job joins.
unsafe impl Send for SystemBase {}
unsafe impl Sync for SystemBase {}
unsafe impl Send for CommandBase {}
unsafe impl Sync for CommandBase {}
unsafe impl Send for WorldBase {}
unsafe impl Sync for WorldBase {}

impl SystemBase {
    unsafe fn get(&self, index: usize) -> *mut Box<dyn ErasedSystem> {
        unsafe { self.0.add(index) }
    }
}

impl CommandBase {
    unsafe fn get(&self, index: usize) -> *mut CommandBuffer {
        unsafe { self.0.add(index) }
    }
}

impl WorldBase {
    unsafe fn cell(&self) -> UnsafeWorldCell<'_> {
        unsafe { UnsafeWorldCell::new(self.0) }
    }
}

fn run_wave(
    systems: &mut [Box<dyn ErasedSystem>],
    commands: &mut [CommandBuffer],
    wave: &[usize],
    world: &mut World,
    parallel_wave_min_systems: usize,
) -> bool {
    let systems = SystemBase(systems.as_mut_ptr());
    let commands = CommandBase(commands.as_mut_ptr());
    let world = WorldBase(std::ptr::from_mut(world));

    let should_parallel =
        wave.len() >= parallel_wave_min_systems && rayon::current_num_threads() > 1;
    if !should_parallel {
        for &index in wave {
            unsafe {
                let system = &mut **systems.get(index);
                debug_assert!(!system.name().is_empty());
                let commands = commands.get(index);
                #[cfg(feature = "profile")]
                let _scope =
                    sky_profile::profile_scope!("ecs", format!("system:{}", system.name()));
                system.run(world.cell(), commands);
            }
        }
        return false;
    }

    wave.par_iter().for_each(|&index| unsafe {
        let system = &mut **systems.get(index);
        debug_assert!(!system.name().is_empty());
        let commands = commands.get(index);
        #[cfg(feature = "profile")]
        let _scope = sky_profile::profile_scope!("ecs", format!("system:{}", system.name()));
        system.run(world.cell(), commands);
    });
    true
}

pub(crate) struct Schedule {
    pub(crate) stages: Vec<SystemStage>,
    pub(crate) last_tick: Option<Instant>,
    needs_initialization: bool,
    validated_resource_epoch: Option<u64>,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            stages: vec![
                SystemStage::new::<First>(),
                SystemStage::new::<FixedUpdate>(),
                SystemStage::new::<PreUpdate>(),
                SystemStage::new::<Update>(),
                SystemStage::new::<PostUpdate>(),
                SystemStage::new::<Last>(),
            ],
            last_tick: None,
            needs_initialization: true,
            validated_resource_epoch: None,
        }
    }
}

impl Schedule {
    pub(crate) fn stage_index<L: StageLabel>(&self) -> Option<usize> {
        let id = stage_id::<L>();
        self.stages.iter().position(|stage| stage.id == id)
    }

    pub(crate) fn insert_after<Anchor, L>(&mut self) -> Result<usize, ScheduleBuildError>
    where
        Anchor: StageLabel,
        L: StageLabel,
    {
        if self.stages.iter().any(|stage| stage.id == stage_id::<L>()) {
            return Err(ScheduleBuildError::DuplicateStage(L::name()));
        }
        let Some(anchor_index) = self
            .stages
            .iter()
            .position(|stage| stage.id == stage_id::<Anchor>())
        else {
            return Err(ScheduleBuildError::UnknownStageAnchor(Anchor::name()));
        };
        let anchor_id = self.stages[anchor_index].id;
        let mut ancestors = self.stages[anchor_index].ancestors.clone();
        ancestors.push(anchor_id);
        let mut insertion_index = anchor_index + 1;
        while self
            .stages
            .get(insertion_index)
            .is_some_and(|stage| stage.ancestors.contains(&anchor_id))
        {
            insertion_index += 1;
        }
        self.stages
            .insert(insertion_index, SystemStage::with_ancestors::<L>(ancestors));
        Ok(insertion_index)
    }

    fn preflight_required_resources(&mut self, world: &World) -> Result<(), ScheduleError> {
        let resource_epoch = world.resource_epoch();
        if self.validated_resource_epoch == Some(resource_epoch) {
            return Ok(());
        }
        for stage in &self.stages {
            for node in stage.nodes.iter().copied() {
                let StageNode::Ordinary(index) = node else {
                    continue;
                };
                let system = &stage.ordinary[index];
                if let Some(resource) = system.access().first_missing_resource(world) {
                    return Err(ScheduleError::MissingResource {
                        system: system.name().to_owned(),
                        resource,
                    });
                }
            }
        }
        self.validated_resource_epoch = Some(resource_epoch);
        Ok(())
    }

    pub(crate) fn initialize(&mut self, world: &mut World) -> Result<bool, ScheduleError> {
        if !self.needs_initialization {
            return Ok(false);
        }
        for stage in &mut self.stages {
            debug_assert!(!stage.name.is_empty());
            stage.initialize(world)?;
        }
        self.needs_initialization = false;
        Ok(true)
    }

    pub(crate) fn diagnostics(&mut self) -> ScheduleDiagnostics {
        let stages = self
            .stages
            .iter_mut()
            .map(|stage| {
                stage.compile();
                let fixed = match stage.policy {
                    StagePolicy::EveryFrame => None,
                    StagePolicy::Fixed(step) => Some(FixedStageDiagnostics {
                        step_seconds: step.seconds,
                        max_substeps: step.max_substeps,
                        overflow: step.overflow,
                        accumulated_seconds: stage.accumulator,
                    }),
                };
                let segments = stage
                    .compiled
                    .iter()
                    .map(|segment| StageSegmentDiagnostics {
                        waves: segment
                            .waves
                            .iter()
                            .map(|wave| {
                                wave.iter()
                                    .map(|&index| SystemDiagnostics {
                                        registration_index: index,
                                        name: stage.ordinary[index].name().to_owned(),
                                        access: stage.ordinary[index].access().diagnostics(),
                                        commands: stage.command_diagnostics[index],
                                    })
                                    .collect()
                            })
                            .collect(),
                        exclusive_after: segment
                            .exclusive_after
                            .map(|index| stage.exclusive[index].name.clone()),
                    })
                    .collect();
                StageDiagnostics {
                    name: stage.name,
                    fixed,
                    parallel_wave_min_systems: stage.parallel_wave_min_systems,
                    segments,
                }
            })
            .collect();
        ScheduleDiagnostics { stages }
    }

    pub(crate) fn run(
        &mut self,
        world: &mut World,
        frame_delta: f32,
        raw_delta: f32,
    ) -> Result<TickReport, ScheduleError> {
        self.preflight_required_resources(world)?;
        let initialized = self.initialize(world).unwrap_or_else(|error| {
            panic!("schedule initialization invariant violated after frame preflight: {error}")
        });
        if initialized {
            self.preflight_required_resources(world)
                .unwrap_or_else(|error| {
                    panic!("schedule initialization invalidated frame resources: {error}")
                });
        }

        let raw_delta = non_negative_finite(raw_delta);
        let frame_delta = non_negative_finite(frame_delta);
        let scaled_delta = non_negative_finite(frame_delta * world.time.time_scale);
        world.time.raw_delta = raw_delta;
        world.time.frame_delta = scaled_delta;
        world.time.delta = scaled_delta;
        world.time.fixed_alpha = 0.0;
        world.time.frame_count = world.time.frame_count.saturating_add(1);

        let mut report = TickReport {
            frame: world.time.frame_count,
            ..TickReport::default()
        };

        for stage in &mut self.stages {
            #[cfg(feature = "profile")]
            let _scope = sky_profile::profile_scope!("ecs", format!("stage:{}", stage.name));
            match stage.policy {
                StagePolicy::EveryFrame => {
                    world.time.delta = scaled_delta;
                    stage.execute(world, &mut report);
                }
                StagePolicy::Fixed(step) => {
                    stage.accumulator += f64::from(scaled_delta);
                    let mut substeps = 0u32;
                    while stage.accumulator >= step.seconds && substeps < step.max_substeps {
                        world.time.delta = step.seconds as f32;
                        stage.execute(world, &mut report);
                        stage.accumulator -= step.seconds;
                        substeps += 1;
                        report.fixed_substeps = report.fixed_substeps.saturating_add(1);
                    }
                    if stage.accumulator >= step.seconds && step.overflow == FixedOverflow::Drop {
                        let retained = stage.accumulator % step.seconds;
                        report.dropped_fixed_time += stage.accumulator - retained;
                        stage.accumulator = retained;
                    }
                    if stage.id == stage_id::<FixedUpdate>() {
                        world.time.fixed_alpha =
                            (stage.accumulator / step.seconds).clamp(0.0, 1.0) as f32;
                    }
                }
            }
        }

        world.time.delta = world.time.frame_delta;
        world.time.elapsed += scaled_delta;
        world.time.raw_elapsed += raw_delta;
        Ok(report)
    }

    pub(crate) fn discard_commands(&mut self) {
        for stage in &mut self.stages {
            stage.discard_commands();
        }
    }

    pub(crate) fn shutdown(&mut self, world: &mut World) {
        self.needs_initialization = true;
        for stage in self.stages.iter_mut().rev() {
            stage.shutdown(world);
        }
    }
}

#[inline]
fn non_negative_finite(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

pub struct StageBuilder<'a> {
    schedule: &'a mut Schedule,
    stage_index: usize,
}

impl<'a> StageBuilder<'a> {
    pub(crate) fn new(schedule: &'a mut Schedule, stage_index: usize) -> Self {
        Self {
            schedule,
            stage_index,
        }
    }

    /// Configures this stage as fixed-step.
    ///
    /// [`FixedUpdate`] starts at 60 Hz; its first explicit configuration may
    /// replace that default. The first explicit configuration then wins.
    /// Repeating an equivalent value is idempotent; a different later value is
    /// rejected instead of silently changing simulation semantics.
    pub fn fixed(&mut self, step: FixedStep) -> Result<&mut Self, ScheduleBuildError> {
        let stage = &mut self.schedule.stages[self.stage_index];
        if stage.fixed_explicit {
            if matches!(stage.policy, StagePolicy::Fixed(existing) if existing.equivalent_to(step))
            {
                return Ok(self);
            }
            return Err(ScheduleBuildError::ConflictingFixedStep(stage.name));
        }
        stage.policy = StagePolicy::Fixed(step);
        stage.fixed_explicit = true;
        stage.accumulator = 0.0;
        Ok(self)
    }

    /// Sets the minimum compatible wave size that uses the Rayon pool.
    ///
    /// The default is 3, avoiding pool overhead for the common two-system
    /// case. Set this to 2 for stages whose compatible system pairs are known
    /// to be substantial, or to `usize::MAX` to force serial wave dispatch.
    pub fn parallel_wave_min_systems(
        &mut self,
        minimum: usize,
    ) -> Result<&mut Self, ScheduleBuildError> {
        if minimum < 2 {
            return Err(ScheduleBuildError::InvalidParallelWaveMinimum(minimum));
        }
        self.schedule.stages[self.stage_index].parallel_wave_min_systems = minimum;
        Ok(self)
    }

    pub fn add<Func, Marker>(&mut self, system: Func) -> &mut Self
    where
        Func: IntoSystem<Marker>,
        Marker: 'static,
    {
        self.add_named(std::any::type_name::<Func>(), system)
    }

    pub fn add_named<Func, Marker>(&mut self, name: impl Into<String>, system: Func) -> &mut Self
    where
        Func: IntoSystem<Marker>,
        Marker: 'static,
    {
        let name = name.into();
        assert!(!name.is_empty(), "system name cannot be empty");
        let stage = &mut self.schedule.stages[self.stage_index];
        let index = stage.ordinary.len();
        stage
            .ordinary
            .push(into_boxed_system::<Func, Marker>(system, name));
        stage.commands.push(CommandBuffer::new());
        stage
            .command_diagnostics
            .push(CommandDiagnostics::default());
        stage.nodes.push(StageNode::Ordinary(index));
        stage.dirty = true;
        self.schedule.needs_initialization = true;
        self.schedule.validated_resource_epoch = None;
        self
    }

    pub fn add_exclusive<S: ExclusiveSystem>(&mut self, system: S) -> &mut Self {
        self.add_exclusive_named(std::any::type_name::<S>(), system)
    }

    pub fn add_exclusive_named<S: ExclusiveSystem>(
        &mut self,
        name: impl Into<String>,
        system: S,
    ) -> &mut Self {
        let name = name.into();
        assert!(!name.is_empty(), "exclusive system name cannot be empty");
        let stage = &mut self.schedule.stages[self.stage_index];
        let index = stage.exclusive.len();
        stage.exclusive.push(RegisteredExclusive::new(name, system));
        stage.nodes.push(StageNode::Exclusive(index));
        stage.dirty = true;
        self.schedule.needs_initialization = true;
        self
    }
}
