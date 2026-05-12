use crate::ecs::{System, World};
use crate::plugin::{Plugin, PluginResult};

use super::{PhysicsConfig2D, PhysicsEvents, PhysicsWorld2D};

struct PhysicsStepSystem;

impl System for PhysicsStepSystem {
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
        install_physics_plugin(world, self.config);
        Ok(())
    }
}

/// Installs the physics world resources and fixed-step physics system.
///
/// This inserts or updates [`PhysicsWorld2D`], ensures [`PhysicsEvents`] exists,
/// and registers one fixed `"physics"` group system. Calling this more than
/// once updates the config without adding duplicate systems.
///
/// Group order matters: if you want input/control systems to affect the same
/// physics tick, create those groups before installing [`PhysicsPlugin`].
fn install_physics_plugin(world: &mut World, config: PhysicsConfig2D) {
    if let Some(physics) = world.get_resource_mut::<PhysicsWorld2D>() {
        physics.set_config(config);
    } else {
        world.insert_resource(PhysicsWorld2D::new(config));
    }
    if world.get_resource::<PhysicsEvents>().is_none() {
        world.insert_resource(PhysicsEvents::default());
    }
    let already_installed = world.get_resource::<PhysicsInstalled2D>().is_some();
    {
        let mut group = world.group("physics");
        group.fixed(config.fixed_dt);
        if !already_installed {
            group.add(PhysicsStepSystem);
        }
    }
    if !already_installed {
        world.insert_resource(PhysicsInstalled2D);
    }
}

/// Runs one physics step immediately using `world.time.delta`.
///
/// Most apps should use [`PhysicsPlugin`] and let the scheduler call this.
/// Manual stepping is useful for deterministic tests or apps that disable
/// `AppConfig::auto_tick` and tick explicitly from `AppState::update`.
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
