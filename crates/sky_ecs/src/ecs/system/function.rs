use super::access::AccessSet;
use super::cell::{SystemParamContext, UnsafeWorldCell};
use super::param::{map_param_error, SystemMeta, SystemParam};
use super::stage::ScheduleError;
use crate::ecs::{CommandBuffer, World};
use std::marker::PhantomData;

pub(crate) trait ErasedSystem: Send {
    fn name(&self) -> &str;
    fn access(&self) -> &AccessSet;
    fn initialize(&mut self, world: &mut World) -> Result<(), ScheduleError>;
    fn prepare(&mut self, world: &World) -> Result<(), ScheduleError>;
    unsafe fn run(&mut self, world: UnsafeWorldCell<'_>, commands: *mut CommandBuffer);
    fn shutdown(&mut self);
}

pub(crate) trait SystemFunction<Marker>: Send + 'static {
    type Params: SystemParam;

    fn run<'w>(&'w mut self, params: <Self::Params as SystemParam>::Item<'w>);
}

impl<Func> SystemFunction<fn()> for Func
where
    Func: FnMut() + Send + 'static,
{
    type Params = ();

    fn run(&mut self, _params: ()) {
        self();
    }
}

macro_rules! impl_system_function {
    ($(($Param:ident, $value:ident)),+ $(,)?) => {
        impl<Func, $($Param: SystemParam),+> SystemFunction<fn($($Param,)+)> for Func
        where
            Func: Send + 'static,
            for<'a> &'a mut Func:
                FnMut($($Param),+) +
                FnMut($($Param::Item<'a>),+),
        {
            type Params = ($($Param,)+);

            fn run<'w>(
                &'w mut self,
                params: <Self::Params as SystemParam>::Item<'w>,
            ) {
                #[allow(clippy::too_many_arguments)]
                fn call_inner<$($Param,)+>(
                    mut function: impl FnMut($($Param),+),
                    $($value: $Param),+
                ) {
                    function($($value),+);
                }

                let ($($value,)+) = params;
                call_inner(self, $($value),+);
            }
        }
    };
}

macro_rules! impl_all_system_functions {
    (@prefix [$($prefix:tt)*];) => {};
    (@prefix [$($prefix:tt)*]; ($Param:ident, $value:ident), $($tail:tt)*) => {
        impl_system_function!($($prefix)* ($Param, $value));
        impl_all_system_functions!(
            @prefix [$($prefix)* ($Param, $value),];
            $($tail)*
        );
    };
}

impl_all_system_functions!(
    @prefix [];
    (P0, p0),
    (P1, p1),
    (P2, p2),
    (P3, p3),
    (P4, p4),
    (P5, p5),
    (P6, p6),
    (P7, p7),
    (P8, p8),
    (P9, p9),
    (P10, p10),
    (P11, p11),
    (P12, p12),
    (P13, p13),
    (P14, p14),
    (P15, p15),
);

struct FunctionSystem<Func, Marker>
where
    Func: SystemFunction<Marker>,
{
    name: String,
    function: Func,
    access: AccessSet,
    state: Option<<Func::Params as SystemParam>::State>,
    marker: PhantomData<fn() -> Marker>,
}

impl<Func, Marker> FunctionSystem<Func, Marker>
where
    Func: SystemFunction<Marker>,
{
    fn new(name: String, function: Func) -> Self {
        let mut meta = SystemMeta::new();
        Func::Params::register(&mut meta);
        Self {
            name,
            function,
            access: meta.access,
            state: None,
            marker: PhantomData,
        }
    }
}

impl<Func, Marker> ErasedSystem for FunctionSystem<Func, Marker>
where
    Func: SystemFunction<Marker>,
    Marker: 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn access(&self) -> &AccessSet {
        &self.access
    }

    fn initialize(&mut self, world: &mut World) -> Result<(), ScheduleError> {
        if self.state.is_none() {
            self.state = Some(
                Func::Params::init(world).map_err(|error| map_param_error(&self.name, error))?,
            );
        }
        Ok(())
    }

    fn prepare(&mut self, world: &World) -> Result<(), ScheduleError> {
        let state = self
            .state
            .as_mut()
            .expect("typed system must be initialized before prepare");
        Func::Params::prepare(state, world).map_err(|error| map_param_error(&self.name, error))
    }

    unsafe fn run(&mut self, world: UnsafeWorldCell<'_>, commands: *mut CommandBuffer) {
        let state = self
            .state
            .as_mut()
            .expect("typed system must be initialized before run");
        let context = unsafe { SystemParamContext::new(commands) };
        let params = unsafe { Func::Params::get(world, state, context) };
        self.function.run(params);
    }

    fn shutdown(&mut self) {
        self.state = None;
    }
}

#[allow(private_interfaces)]
mod sealed {
    use super::{ErasedSystem, FunctionSystem, SystemFunction};

    pub trait IntoSystemSealed<Marker>: Send + 'static {
        fn into_system(self, name: String) -> Box<dyn ErasedSystem>;
    }

    impl<Func, Marker> IntoSystemSealed<Marker> for Func
    where
        Func: SystemFunction<Marker>,
        Marker: 'static,
    {
        fn into_system(self, name: String) -> Box<dyn ErasedSystem> {
            Box::new(FunctionSystem::<Func, Marker>::new(name, self))
        }
    }
}

/// Conversion contract implemented automatically for typed system functions.
pub trait IntoSystem<Marker>: sealed::IntoSystemSealed<Marker> + Send + 'static {}

impl<Func, Marker> IntoSystem<Marker> for Func where
    Func: sealed::IntoSystemSealed<Marker> + Send + 'static
{
}

pub(crate) fn into_boxed_system<Func, Marker>(function: Func, name: String) -> Box<dyn ErasedSystem>
where
    Func: IntoSystem<Marker>,
    Marker: 'static,
{
    sealed::IntoSystemSealed::into_system(function, name)
}

/// Explicit full-World system. Exclusive systems are serial schedule barriers.
pub trait ExclusiveSystem: 'static {
    fn init(&mut self, _world: &mut World) {}
    fn run(&mut self, world: &mut World);
    fn teardown(&mut self, _world: &mut World) {}
}

impl<F> ExclusiveSystem for F
where
    F: FnMut(&mut World) + 'static,
{
    fn run(&mut self, world: &mut World) {
        self(world);
    }
}

pub(crate) struct RegisteredExclusive {
    pub(crate) name: String,
    pub(crate) initialized: bool,
    pub(crate) system: Box<dyn ExclusiveSystem>,
}

impl RegisteredExclusive {
    pub(crate) fn new<S: ExclusiveSystem>(name: String, system: S) -> Self {
        Self {
            name,
            initialized: false,
            system: Box::new(system),
        }
    }
}
