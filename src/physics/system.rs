use crate::ecs::{ExclusiveSystem, FixedStep, FixedUpdate, World};
use crate::plugin::{Plugin, PluginError, PluginResult};

use super::{PhysicsConfig2D, PhysicsEvents, PhysicsWorld2D};

struct PhysicsStepSystem;

impl ExclusiveSystem for PhysicsStepSystem {
    fn run(&mut self, world: &mut World) {
        step_physics(world);
    }
}

struct PhysicsInstalled2D;

/// Plugin that installs the physics world resources and fixed-step physics system.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicsPlugin {
    pub config: PhysicsConfig2D,
}

impl PhysicsPlugin {
    pub fn new(config: PhysicsConfig2D) -> Self {
        Self { config }
    }
}

impl Plugin for PhysicsPlugin {
    fn name(&self) -> &'static str {
        "physics"
    }

    fn install(self, world: &mut World) -> PluginResult {
        install_physics_plugin(world, self.config)
    }
}

/// Installs the physics world resources and fixed-step physics system.
///
/// This inserts or updates [`PhysicsWorld2D`], ensures [`PhysicsEvents`] exists,
/// and registers one system in [`FixedUpdate`]. Calling this more than
/// once updates the config without adding duplicate systems.
///
/// Systems in [`PreUpdate`](crate::ecs::PreUpdate) run after fixed simulation;
/// put fixed-step control systems before physics inside [`FixedUpdate`].
fn install_physics_plugin(world: &mut World, config: PhysicsConfig2D) -> PluginResult {
    let step = FixedStep::seconds(f64::from(config.fixed_dt))
        .map_err(|error| PluginError::new("physics", error.to_string()))?;
    let already_installed = world.get_resource::<PhysicsInstalled2D>().is_some();
    {
        let mut stage = world.stage(FixedUpdate);
        stage
            .fixed(step)
            .map_err(|error| PluginError::new("physics", error.to_string()))?;
        if !already_installed {
            stage.add_exclusive(PhysicsStepSystem);
        }
    }
    if let Some(physics) = world.get_resource_mut::<PhysicsWorld2D>() {
        physics.set_config(config);
    } else {
        world.insert_resource(PhysicsWorld2D::new(config));
    }
    if world.get_resource::<PhysicsEvents>().is_none() {
        world.insert_resource(PhysicsEvents::default());
    }
    if !already_installed {
        world.insert_resource(PhysicsInstalled2D);
    }
    Ok(())
}

/// Runs one physics step immediately using `world.time.delta`.
///
/// Most apps should use [`PhysicsPlugin`] and let the scheduler call this.
/// Manual stepping is useful for deterministic tests or apps that disable
/// `RunnerPlugin::game().with_auto_tick(false)` and tick explicitly from
/// `AppState::update`.
pub fn step_physics(world: &mut World) {
    let Some(mut physics) = world.remove_resource::<PhysicsWorld2D>() else {
        return;
    };
    let mut events = world.remove_resource::<PhysicsEvents>().unwrap_or_default();

    let dt = world.time.delta;
    physics.step_world(world, &mut events, dt);
    physics.write_back(world);

    world.insert_resource(events);
    world.insert_resource(physics);
}
