use std::any::TypeId;
use std::fmt;

/// Type-level identity for one schedule stage.
pub trait StageLabel: Send + Sync + 'static {
    fn name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

macro_rules! core_stages {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
            pub struct $name;

            impl StageLabel for $name {}
        )+
    };
}

core_stages!(First, FixedUpdate, PreUpdate, Update, PostUpdate, Last);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedOverflow {
    /// Discard backlog beyond the configured substep budget.
    Drop,
    /// Preserve backlog for later frames.
    Carry,
}

/// Validated fixed-timestep configuration.
#[derive(Clone, Copy, Debug)]
pub struct FixedStep {
    pub(crate) seconds: f64,
    pub(crate) max_substeps: u32,
    pub(crate) overflow: FixedOverflow,
}

impl FixedStep {
    pub fn hz(hz: u32) -> Self {
        Self::try_hz(hz).expect("fixed-step frequency must be greater than zero")
    }

    pub fn try_hz(hz: u32) -> Result<Self, ScheduleBuildError> {
        if hz == 0 {
            return Err(ScheduleBuildError::InvalidFixedStep(
                "frequency must be greater than zero".into(),
            ));
        }
        Ok(Self {
            seconds: 1.0 / f64::from(hz),
            max_substeps: 8,
            overflow: FixedOverflow::Drop,
        })
    }

    pub fn seconds(seconds: f64) -> Result<Self, ScheduleBuildError> {
        if !seconds.is_finite() || seconds <= 0.0 {
            return Err(ScheduleBuildError::InvalidFixedStep(
                "duration must be finite and greater than zero".into(),
            ));
        }
        Ok(Self {
            seconds,
            max_substeps: 8,
            overflow: FixedOverflow::Drop,
        })
    }

    pub fn max_substeps(mut self, max_substeps: u32) -> Self {
        assert!(max_substeps > 0, "fixed max_substeps must be non-zero");
        self.max_substeps = max_substeps;
        self
    }

    pub fn overflow(mut self, overflow: FixedOverflow) -> Self {
        self.overflow = overflow;
        self
    }

    pub fn step_seconds(self) -> f64 {
        self.seconds
    }

    pub fn substep_limit(self) -> u32 {
        self.max_substeps
    }

    pub(crate) fn equivalent_to(self, other: Self) -> bool {
        let scale = self.seconds.abs().max(other.seconds.abs());
        (self.seconds - other.seconds).abs() <= scale * 1.0e-7 + f64::EPSILON
            && self.max_substeps == other.max_substeps
            && self.overflow == other.overflow
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TickReport {
    /// The resulting `Time::frame_count`.
    pub frame: u64,
    /// Number of ordinary and exclusive system invocations.
    pub systems_run: u32,
    /// Total ordinary waves plus exclusive barriers.
    pub waves_run: u32,
    /// Ordinary waves actually dispatched through Rayon.
    pub parallel_waves_run: u32,
    /// Ordinary waves and exclusive barriers run serially.
    pub sequential_waves_run: u32,
    /// Fixed-stage invocations across all fixed stages.
    pub fixed_substeps: u32,
    /// Accumulated seconds discarded by fixed stages using `Drop` overflow.
    pub dropped_fixed_time: f64,
}

/// Snapshot of a schedule's compiled stages and waves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScheduleDiagnostics {
    pub stages: Vec<StageDiagnostics>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StageDiagnostics {
    pub name: &'static str,
    pub fixed: Option<FixedStageDiagnostics>,
    pub parallel_wave_min_systems: usize,
    pub segments: Vec<StageSegmentDiagnostics>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedStageDiagnostics {
    pub step_seconds: f64,
    pub max_substeps: u32,
    pub overflow: FixedOverflow,
    pub accumulated_seconds: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StageSegmentDiagnostics {
    pub waves: Vec<Vec<SystemDiagnostics>>,
    pub exclusive_after: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemDiagnostics {
    pub registration_index: usize,
    pub name: String,
    pub access: SystemAccessDiagnostics,
    pub commands: CommandDiagnostics,
}

/// Deferred-command counts for the most recent invocation and for the
/// lifetime of one registered system.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandDiagnostics {
    /// Commands produced by the system's most recent completed or unwound invocation.
    pub last_enqueued: usize,
    /// Commands in the most recent buffer whose entire apply pass completed.
    pub last_applied: usize,
    /// Still-pending commands discarded when the most recent invocation unwound.
    pub last_discarded: usize,
    /// Lifetime sum of commands produced by completed or unwound invocations.
    pub total_enqueued: u64,
    /// Lifetime sum from buffers whose entire apply pass completed.
    pub total_applied: u64,
    /// Lifetime sum of still-pending commands discarded during unwinding.
    pub total_discarded: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemAccessDiagnostics {
    pub component_reads: Vec<String>,
    pub component_writes: Vec<String>,
    pub resource_reads: Vec<String>,
    pub resource_writes: Vec<String>,
    pub uses_commands: bool,
    pub(crate) component_read_ids: Vec<usize>,
    pub(crate) component_write_ids: Vec<usize>,
    pub(crate) resource_read_ids: Vec<TypeId>,
    pub(crate) resource_write_ids: Vec<TypeId>,
}

impl SystemAccessDiagnostics {
    /// Explains the first conflicting component/resource, if any.
    pub fn conflict_reason(&self, other: &Self) -> Option<String> {
        first_typed_conflict(
            &self.component_write_ids,
            &self.component_writes,
            &self.component_read_ids,
            &self.component_reads,
            &other.component_write_ids,
            &other.component_read_ids,
        )
        .map(|name| format!("component `{name}`"))
        .or_else(|| {
            first_typed_conflict(
                &self.resource_write_ids,
                &self.resource_writes,
                &self.resource_read_ids,
                &self.resource_reads,
                &other.resource_write_ids,
                &other.resource_read_ids,
            )
            .map(|name| format!("resource `{name}`"))
        })
    }
}

fn first_typed_conflict<'a, Id: Eq>(
    left_write_ids: &[Id],
    left_writes: &'a [String],
    left_read_ids: &[Id],
    left_reads: &'a [String],
    right_write_ids: &[Id],
    right_read_ids: &[Id],
) -> Option<&'a str> {
    left_write_ids
        .iter()
        .position(|id| right_write_ids.contains(id) || right_read_ids.contains(id))
        .map(|index| left_writes[index].as_str())
        .or_else(|| {
            left_read_ids
                .iter()
                .position(|id| right_write_ids.contains(id))
                .map(|index| left_reads[index].as_str())
        })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduleBuildError {
    DuplicateStage(&'static str),
    UnknownStage(&'static str),
    UnknownStageAnchor(&'static str),
    InvalidFixedStep(String),
    ConflictingFixedStep(&'static str),
    InvalidParallelWaveMinimum(usize),
}

impl fmt::Display for ScheduleBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateStage(stage) => write!(f, "stage `{stage}` is already installed"),
            Self::UnknownStage(stage) => write!(
                f,
                "stage `{stage}` is not installed; install custom stages with `insert_stage_after`"
            ),
            Self::UnknownStageAnchor(stage) => write!(f, "stage anchor `{stage}` is not installed"),
            Self::InvalidFixedStep(message) => write!(f, "invalid fixed step: {message}"),
            Self::ConflictingFixedStep(stage) => {
                write!(
                    f,
                    "stage `{stage}` already has a different explicit fixed step"
                )
            }
            Self::InvalidParallelWaveMinimum(minimum) => write!(
                f,
                "parallel wave minimum must be at least 2 systems, got {minimum}"
            ),
        }
    }
}

impl std::error::Error for ScheduleBuildError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduleError {
    MissingResource {
        system: String,
        resource: &'static str,
    },
}

impl fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingResource { system, resource } => {
                write!(
                    f,
                    "system `{system}` requires missing resource `{resource}`"
                )
            }
        }
    }
}

impl std::error::Error for ScheduleError {}

#[derive(Clone, Copy)]
pub(crate) enum StagePolicy {
    EveryFrame,
    Fixed(FixedStep),
}

pub(crate) fn stage_id<L: StageLabel>() -> TypeId {
    TypeId::of::<L>()
}
