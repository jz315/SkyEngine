use rapier2d::prelude::{
    BroadPhaseBvh, CCDSolver, ColliderHandle, ColliderSet, EventHandler, ImpulseJointSet,
    IntegrationParameters, IslandManager, MultibodyJointSet, NarrowPhase, PhysicsPipeline,
    QueryFilter, QueryPipeline, RigidBodyHandle, RigidBodySet, Vector,
};

pub(crate) struct RapierBackend {
    pub(crate) pipeline: PhysicsPipeline,
    pub(crate) integration_parameters: IntegrationParameters,
    pub(crate) islands: IslandManager,
    pub(crate) broad_phase: BroadPhaseBvh,
    pub(crate) narrow_phase: NarrowPhase,
    pub(crate) bodies: RigidBodySet,
    pub(crate) colliders: ColliderSet,
    pub(crate) impulse_joints: ImpulseJointSet,
    pub(crate) multibody_joints: MultibodyJointSet,
    pub(crate) ccd_solver: CCDSolver,
}

impl RapierBackend {
    pub(crate) fn new(fixed_dt: f32) -> Self {
        let mut integration_parameters = IntegrationParameters::default();
        configure_timestep(&mut integration_parameters, fixed_dt);

        Self {
            pipeline: PhysicsPipeline::new(),
            integration_parameters,
            islands: IslandManager::new(),
            broad_phase: BroadPhaseBvh::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
        }
    }

    #[inline]
    pub(crate) fn set_timestep(&mut self, dt: f32) {
        configure_timestep(&mut self.integration_parameters, dt);
    }

    #[inline]
    pub(crate) fn body_count(&self) -> usize {
        self.bodies.len()
    }

    #[inline]
    pub(crate) fn collider_count(&self) -> usize {
        self.colliders.len()
    }

    pub(crate) fn query_pipeline<'a>(&'a self, filter: QueryFilter<'a>) -> QueryPipeline<'a> {
        self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            filter,
        )
    }

    pub(crate) fn step(&mut self, gravity: Vector, event_handler: &dyn EventHandler) {
        self.pipeline.step(
            gravity,
            &self.integration_parameters,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            &(),
            event_handler,
        );
    }

    pub(crate) fn remove_body(&mut self, handle: RigidBodyHandle) {
        let _ = self.bodies.remove(
            handle,
            &mut self.islands,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            true,
        );
    }

    pub(crate) fn remove_collider(&mut self, handle: ColliderHandle) {
        let _ = self
            .colliders
            .remove(handle, &mut self.islands, &mut self.bodies, true);
    }
}

fn configure_timestep(parameters: &mut IntegrationParameters, dt: f32) {
    parameters.dt = dt.max(f32::EPSILON);
    parameters.min_ccd_dt = parameters.dt / 100.0;
}
