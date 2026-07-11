use super::access::AccessSet;
use super::cell::{SystemParamContext, UnsafeWorldCell};
use super::stage::ScheduleError;
use crate::ecs::query::{
    count_matches, matches_nothing, par_for_each, par_for_each_chunk,
    par_for_each_chunk_with_entities, par_for_each_with_entity, prepare_job_cache,
    prepared_job_snapshot, run_for_each, run_for_each_chunk, run_for_each_chunk_with_entities,
    run_for_each_with_entity, ParallelJobCache, ParallelJobSnapshot, PreparedCache,
    QueryDescriptor,
};
use crate::ecs::{Commands, EntityId, QueryFilter, QuerySpec, World};
use std::cell::Cell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

pub(crate) struct SystemMeta {
    pub(crate) access: AccessSet,
}

impl SystemMeta {
    pub(crate) fn new() -> Self {
        Self {
            access: AccessSet::default(),
        }
    }
}

pub(crate) enum ParamError {
    MissingResource(&'static str),
}

/// Hidden implementation contract for scheduler-provided function parameters.
///
/// # Safety
///
/// `register` must declare every direct World access made by `get`. Returned
/// references must remain inside one invocation and match the registered
/// component/resource types and access modes.
pub(crate) unsafe trait SystemParam {
    type State: Send + 'static;
    type Item<'w>;

    fn register(meta: &mut SystemMeta);
    fn init(world: &mut World) -> Result<Self::State, ParamError>;
    fn prepare(state: &mut Self::State, world: &World) -> Result<(), ParamError>;

    unsafe fn get<'w>(
        world: UnsafeWorldCell<'w>,
        state: &'w mut Self::State,
        context: SystemParamContext<'w>,
    ) -> Self::Item<'w>;
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

pub(crate) struct ViewState<Q: QuerySpec, F: QueryFilter> {
    descriptor: QueryDescriptor,
    prepared: PreparedCache,
    active_iteration: Cell<bool>,
    marker: PhantomData<fn() -> (Q, F)>,
}

impl<Q: QuerySpec, F: QueryFilter> ViewState<Q, F> {
    fn new() -> Self {
        Self {
            descriptor: Q::descriptor(),
            prepared: PreparedCache::default(),
            active_iteration: Cell::new(false),
            marker: PhantomData,
        }
    }
}

pub(crate) struct ParViewState<Q: QuerySpec, F: QueryFilter> {
    view: ViewState<Q, F>,
    parallel_jobs: ParallelJobCache,
}

impl<Q: QuerySpec, F: QueryFilter> ParViewState<Q, F> {
    fn new() -> Self {
        Self {
            view: ViewState::new(),
            parallel_jobs: ParallelJobCache::default(),
        }
    }
}

/// A scheduler-issued typed component view.
///
/// The view cannot perform structural mutation. Use [`Commands`] for deferred
/// spawn/despawn/insert/remove operations. This sequential capability does not
/// build parallel stripe jobs; request [`ParView`] when the system calls a
/// `par_*` iterator.
///
/// ```compile_fail
/// use sky_ecs::View;
///
/// struct Position;
/// fn invalid(view: View<&mut Position>) {
///     view.par_for_each(|_| {});
/// }
/// ```
pub struct View<'w, Q: QuerySpec, F: QueryFilter = ()> {
    world: UnsafeWorldCell<'w>,
    archetypes: &'w [crate::ecs::query::CachedArchetype],
    active_iteration: &'w Cell<bool>,
    marker: PhantomData<fn() -> (Q, F)>,
}

/// A scheduler-issued component view with serially prepared parallel jobs.
///
/// `ParView` makes parallel preparation explicit in the system signature. Its
/// iterators still use the shared query runner's automatic small-workload
/// sequential fallback.
pub struct ParView<'w, Q: QuerySpec, F: QueryFilter = ()> {
    world: UnsafeWorldCell<'w>,
    archetypes: &'w [crate::ecs::query::CachedArchetype],
    parallel_jobs: &'w ParallelJobCache,
    active_iteration: &'w Cell<bool>,
    marker: PhantomData<fn() -> (Q, F)>,
}

struct ViewIterationGuard<'a>(&'a Cell<bool>);

impl Drop for ViewIterationGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl<Q, F> View<'_, Q, F>
where
    Q: QuerySpec,
    F: QueryFilter,
{
    #[inline(always)]
    fn world(&self) -> &World {
        unsafe { self.world.world() }
    }

    #[inline]
    fn begin_iteration(&self) -> ViewIterationGuard<'_> {
        assert!(
            !self.active_iteration.replace(true),
            "a View cannot be iterated recursively"
        );
        ViewIterationGuard(self.active_iteration)
    }

    pub fn for_each<Func>(&self, f: Func)
    where
        Func: for<'a> FnMut(Q::Item<'a>),
    {
        let _iteration = self.begin_iteration();
        run_for_each::<Q, _>(self.world(), self.archetypes, f);
    }

    pub fn for_each_with_entity<Func>(&self, f: Func)
    where
        Func: for<'a> FnMut(EntityId, Q::Item<'a>),
    {
        let _iteration = self.begin_iteration();
        run_for_each_with_entity::<Q, _>(self.world(), self.archetypes, f);
    }

    pub fn for_each_chunk<Func>(&self, f: Func)
    where
        Func: for<'a> FnMut(Q::Chunk<'a>),
    {
        let _iteration = self.begin_iteration();
        run_for_each_chunk::<Q, _>(self.world(), self.archetypes, f);
    }

    pub fn for_each_chunk_with_entities<Func>(&self, f: Func)
    where
        Func: for<'a> FnMut(&'a [EntityId], Q::Chunk<'a>),
    {
        let _iteration = self.begin_iteration();
        run_for_each_chunk_with_entities::<Q, _>(self.world(), self.archetypes, f);
    }

    pub fn count(&self) -> usize {
        count_matches(self.world(), self.archetypes)
    }

    pub fn is_empty(&self) -> bool {
        matches_nothing(self.world(), self.archetypes)
    }

    pub fn cached_archetype_count(&self) -> usize {
        self.archetypes.len()
    }
}

// Safety: query descriptor access is registered exactly, preparation freezes
// matching archetypes, and the scheduler calls `get` only for a validated wave.
unsafe impl<'marker, Q, F> SystemParam for View<'marker, Q, F>
where
    Q: QuerySpec + 'static,
    F: QueryFilter + 'static,
    for<'a> Q::Item<'a>: Send,
    for<'a> Q::Chunk<'a>: Send,
{
    type State = ViewState<Q, F>;
    type Item<'w> = View<'w, Q, F>;

    fn register(meta: &mut SystemMeta) {
        for component in Q::descriptor().components {
            meta.access.add_component(component.ty, component.mutable);
        }
    }

    fn init(_world: &mut World) -> Result<Self::State, ParamError> {
        Ok(ViewState::new())
    }

    fn prepare(state: &mut Self::State, world: &World) -> Result<(), ParamError> {
        state.prepared.prepare::<F>(world, &state.descriptor);
        Ok(())
    }

    unsafe fn get<'w>(
        world: UnsafeWorldCell<'w>,
        state: &'w mut Self::State,
        _context: SystemParamContext<'w>,
    ) -> Self::Item<'w> {
        View {
            world,
            archetypes: state.prepared.archetypes.as_slice(),
            active_iteration: &state.active_iteration,
            marker: PhantomData,
        }
    }
}

impl<Q, F> ParView<'_, Q, F>
where
    Q: QuerySpec,
    F: QueryFilter,
{
    #[inline(always)]
    fn world(&self) -> &World {
        unsafe { self.world.world() }
    }

    #[inline]
    fn begin_iteration(&self) -> ViewIterationGuard<'_> {
        assert!(
            !self.active_iteration.replace(true),
            "a ParView cannot be iterated recursively"
        );
        ViewIterationGuard(self.active_iteration)
    }

    #[inline]
    fn parallel_snapshot(&self) -> ParallelJobSnapshot {
        prepared_job_snapshot(self.parallel_jobs)
    }

    pub fn par_for_each<Func>(&self, f: Func)
    where
        for<'a> Q::Item<'a>: Send,
        Func: for<'a> Fn(Q::Item<'a>) + Send + Sync,
    {
        let _iteration = self.begin_iteration();
        let world = self.world;
        let jobs = self.parallel_snapshot();
        par_for_each::<Q, _>(self.archetypes, unsafe { world.world() }, jobs, f);
    }

    pub fn par_for_each_with_entity<Func>(&self, f: Func)
    where
        for<'a> Q::Item<'a>: Send,
        Func: for<'a> Fn(EntityId, Q::Item<'a>) + Send + Sync,
    {
        let _iteration = self.begin_iteration();
        let world = self.world;
        let jobs = self.parallel_snapshot();
        par_for_each_with_entity::<Q, _>(self.archetypes, unsafe { world.world() }, jobs, f);
    }

    pub fn par_for_each_chunk<Func>(&self, f: Func)
    where
        for<'a> Q::Chunk<'a>: Send,
        Func: for<'a> Fn(Q::Chunk<'a>) + Send + Sync,
    {
        let _iteration = self.begin_iteration();
        let world = self.world;
        let jobs = self.parallel_snapshot();
        par_for_each_chunk::<Q, _>(self.archetypes, unsafe { world.world() }, jobs, f);
    }

    pub fn par_for_each_chunk_with_entities<Func>(&self, f: Func)
    where
        for<'a> Q::Chunk<'a>: Send,
        Func: for<'a> Fn(&'a [EntityId], Q::Chunk<'a>) + Send + Sync,
    {
        let _iteration = self.begin_iteration();
        let world = self.world;
        let jobs = self.parallel_snapshot();
        par_for_each_chunk_with_entities::<Q, _>(
            self.archetypes,
            unsafe { world.world() },
            jobs,
            f,
        );
    }

    pub fn count(&self) -> usize {
        count_matches(self.world(), self.archetypes)
    }

    pub fn is_empty(&self) -> bool {
        matches_nothing(self.world(), self.archetypes)
    }

    pub fn cached_archetype_count(&self) -> usize {
        self.archetypes.len()
    }
}

// Safety: access registration matches View, while stripe discovery and pointer
// resolution finish in the serial prepare pass before the system can run.
unsafe impl<'marker, Q, F> SystemParam for ParView<'marker, Q, F>
where
    Q: QuerySpec + 'static,
    F: QueryFilter + 'static,
    for<'a> Q::Item<'a>: Send,
    for<'a> Q::Chunk<'a>: Send,
{
    type State = ParViewState<Q, F>;
    type Item<'w> = ParView<'w, Q, F>;

    fn register(meta: &mut SystemMeta) {
        for component in Q::descriptor().components {
            meta.access.add_component(component.ty, component.mutable);
        }
    }

    fn init(_world: &mut World) -> Result<Self::State, ParamError> {
        Ok(ParViewState::new())
    }

    fn prepare(state: &mut Self::State, world: &World) -> Result<(), ParamError> {
        state
            .view
            .prepared
            .prepare::<F>(world, &state.view.descriptor);
        prepare_job_cache(
            &mut state.parallel_jobs,
            state.view.prepared.archetypes.as_slice(),
            world,
        );
        Ok(())
    }

    unsafe fn get<'w>(
        world: UnsafeWorldCell<'w>,
        state: &'w mut Self::State,
        _context: SystemParamContext<'w>,
    ) -> Self::Item<'w> {
        ParView {
            world,
            archetypes: state.view.prepared.archetypes.as_slice(),
            parallel_jobs: &state.parallel_jobs,
            active_iteration: &state.view.active_iteration,
            marker: PhantomData,
        }
    }
}

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Shared system access to a `Sync` resource.
pub struct Res<'w, T: 'static>(&'w T);

impl<T: 'static> Deref for Res<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

/// Exclusive system access to a `Send` resource.
///
/// Core [`crate::ecs::Time`] is intentionally read-only in ordinary systems:
///
/// ```compile_fail
/// use sky_ecs::{ResMut, Time, Update, World};
///
/// fn overwrite_time(_time: ResMut<Time>) {}
///
/// let mut world = World::new();
/// world.stage(Update).add(overwrite_time);
/// ```
pub struct ResMut<'w, T: 'static>(&'w mut T);

impl<T: 'static> Deref for ResMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T: 'static> DerefMut for ResMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

pub(crate) struct ResourceState<T> {
    ptr: *mut T,
    cached_world: Option<std::sync::Arc<()>>,
    cached_world_address: *const World,
    cached_epoch: Option<u64>,
}

// Safety: T's worker bound is enforced by the corresponding SystemParam impl,
// and the pointer is refreshed serially before each structurally frozen stage.
unsafe impl<T> Send for ResourceState<T> {}

// Safety: shared resource access is registered as read-only and T is Sync.
unsafe impl<'marker, T: Sync + 'static> SystemParam for Res<'marker, T> {
    type State = ResourceState<T>;
    type Item<'w> = Res<'w, T>;

    fn register(meta: &mut SystemMeta) {
        meta.access.add_resource::<T>(false);
    }

    fn init(_world: &mut World) -> Result<Self::State, ParamError> {
        Ok(ResourceState {
            ptr: std::ptr::null_mut(),
            cached_world: None,
            cached_world_address: std::ptr::null(),
            cached_epoch: None,
        })
    }

    fn prepare(state: &mut Self::State, world: &World) -> Result<(), ParamError> {
        let same_world = state
            .cached_world
            .as_ref()
            .is_some_and(|cached| std::sync::Arc::ptr_eq(cached, world.cache_token()))
            && std::ptr::eq(state.cached_world_address, world);
        if same_world && state.cached_epoch == Some(world.resource_epoch()) && !state.ptr.is_null()
        {
            return Ok(());
        }
        state.ptr = world
            .resource_ptr::<T>()
            .ok_or(ParamError::MissingResource(std::any::type_name::<T>()))?;
        state.cached_world = Some(std::sync::Arc::clone(world.cache_token()));
        state.cached_world_address = std::ptr::from_ref(world);
        state.cached_epoch = Some(world.resource_epoch());
        Ok(())
    }

    unsafe fn get<'w>(
        _world: UnsafeWorldCell<'w>,
        state: &'w mut Self::State,
        _context: SystemParamContext<'w>,
    ) -> Self::Item<'w> {
        Res(unsafe { &*state.ptr })
    }
}

// Safety: exclusive resource access is registered as a write and T is Send.
unsafe impl<'marker, T: Send + 'static> SystemParam for ResMut<'marker, T> {
    type State = ResourceState<T>;
    type Item<'w> = ResMut<'w, T>;

    fn register(meta: &mut SystemMeta) {
        meta.access.add_resource::<T>(true);
    }

    fn init(_world: &mut World) -> Result<Self::State, ParamError> {
        Ok(ResourceState {
            ptr: std::ptr::null_mut(),
            cached_world: None,
            cached_world_address: std::ptr::null(),
            cached_epoch: None,
        })
    }

    fn prepare(state: &mut Self::State, world: &World) -> Result<(), ParamError> {
        let same_world = state
            .cached_world
            .as_ref()
            .is_some_and(|cached| std::sync::Arc::ptr_eq(cached, world.cache_token()))
            && std::ptr::eq(state.cached_world_address, world);
        if same_world && state.cached_epoch == Some(world.resource_epoch()) && !state.ptr.is_null()
        {
            return Ok(());
        }
        state.ptr = world
            .resource_ptr::<T>()
            .ok_or(ParamError::MissingResource(std::any::type_name::<T>()))?;
        state.cached_world = Some(std::sync::Arc::clone(world.cache_token()));
        state.cached_world_address = std::ptr::from_ref(world);
        state.cached_epoch = Some(world.resource_epoch());
        Ok(())
    }

    unsafe fn get<'w>(
        _world: UnsafeWorldCell<'w>,
        state: &'w mut Self::State,
        _context: SystemParamContext<'w>,
    ) -> Self::Item<'w> {
        ResMut(unsafe { &mut *state.ptr })
    }
}

// ---------------------------------------------------------------------------
// Local and Commands
// ---------------------------------------------------------------------------

pub struct Local<'w, T: 'static>(&'w mut T);

impl<T: 'static> Deref for Local<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T: 'static> DerefMut for Local<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

// Safety: Local never accesses World and its state belongs to one system.
unsafe impl<'marker, T: Default + Send + 'static> SystemParam for Local<'marker, T> {
    type State = T;
    type Item<'w> = Local<'w, T>;

    fn register(_meta: &mut SystemMeta) {}

    fn init(_world: &mut World) -> Result<Self::State, ParamError> {
        Ok(T::default())
    }

    fn prepare(_state: &mut Self::State, _world: &World) -> Result<(), ParamError> {
        Ok(())
    }

    unsafe fn get<'w>(
        _world: UnsafeWorldCell<'w>,
        state: &'w mut Self::State,
        _context: SystemParamContext<'w>,
    ) -> Self::Item<'w> {
        Local(state)
    }
}

// Safety: Commands accesses only the invocation-private deferred buffer.
unsafe impl SystemParam for Commands<'_> {
    type State = ();
    type Item<'w> = Commands<'w>;

    fn register(meta: &mut SystemMeta) {
        meta.access.add_commands();
    }

    fn init(_world: &mut World) -> Result<Self::State, ParamError> {
        Ok(())
    }

    fn prepare(_state: &mut Self::State, _world: &World) -> Result<(), ParamError> {
        Ok(())
    }

    unsafe fn get<'w>(
        _world: UnsafeWorldCell<'w>,
        _state: &'w mut Self::State,
        context: SystemParamContext<'w>,
    ) -> Self::Item<'w> {
        unsafe { Commands::from_ptr(context.commands()) }
    }
}

// ---------------------------------------------------------------------------
// Parameter tuples
// ---------------------------------------------------------------------------

unsafe impl SystemParam for () {
    type State = ();
    type Item<'w> = ();

    fn register(_meta: &mut SystemMeta) {}

    fn init(_world: &mut World) -> Result<Self::State, ParamError> {
        Ok(())
    }

    fn prepare(_state: &mut Self::State, _world: &World) -> Result<(), ParamError> {
        Ok(())
    }

    unsafe fn get<'w>(
        _world: UnsafeWorldCell<'w>,
        _state: &'w mut Self::State,
        _context: SystemParamContext<'w>,
    ) -> Self::Item<'w> {
    }
}

macro_rules! impl_system_param_tuple {
    ($(($Param:ident, $state:ident)),+ $(,)?) => {
        // Safety: each child parameter registers its complete access. Tuple
        // registration rejects overlapping exclusive capabilities.
        unsafe impl<$($Param: SystemParam),+> SystemParam for ($($Param,)+) {
            type State = ($($Param::State,)+);
            type Item<'w> = ($($Param::Item<'w>,)+);

            fn register(meta: &mut SystemMeta) {
                $($Param::register(meta);)+
            }

            fn init(world: &mut World) -> Result<Self::State, ParamError> {
                Ok(($($Param::init(world)?,)+))
            }

            fn prepare(state: &mut Self::State, world: &World) -> Result<(), ParamError> {
                let ($($state,)+) = state;
                $($Param::prepare($state, world)?;)+
                Ok(())
            }

            unsafe fn get<'w>(
                world: UnsafeWorldCell<'w>,
                state: &'w mut Self::State,
                context: SystemParamContext<'w>,
            ) -> Self::Item<'w> {
                let ($($state,)+) = state;
                ($(
                    unsafe { $Param::get(world, $state, context) },
                )+)
            }
        }
    };
}

macro_rules! impl_all_system_param_tuples {
    (@prefix [$($prefix:tt)*];) => {};
    (@prefix [$($prefix:tt)*]; ($Param:ident, $state:ident), $($tail:tt)*) => {
        impl_system_param_tuple!($($prefix)* ($Param, $state));
        impl_all_system_param_tuples!(
            @prefix [$($prefix)* ($Param, $state),];
            $($tail)*
        );
    };
}

impl_all_system_param_tuples!(
    @prefix [];
    (P0, s0),
    (P1, s1),
    (P2, s2),
    (P3, s3),
    (P4, s4),
    (P5, s5),
    (P6, s6),
    (P7, s7),
    (P8, s8),
    (P9, s9),
    (P10, s10),
    (P11, s11),
    (P12, s12),
    (P13, s13),
    (P14, s14),
    (P15, s15),
);

pub(crate) fn map_param_error(system: &str, error: ParamError) -> ScheduleError {
    match error {
        ParamError::MissingResource(resource) => ScheduleError::MissingResource {
            system: system.to_owned(),
            resource,
        },
    }
}
