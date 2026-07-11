use super::stage::SystemAccessDiagnostics;
use crate::ecs::{ComponentType, World};
use std::any::TypeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccessMode {
    Read,
    Write,
}

#[derive(Clone, Debug)]
struct ComponentAccess {
    id: usize,
    name: String,
    mode: AccessMode,
}

#[derive(Clone, Debug)]
struct ResourceAccess {
    id: TypeId,
    name: &'static str,
    mode: AccessMode,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AccessSet {
    components: Vec<ComponentAccess>,
    resources: Vec<ResourceAccess>,
    commands: bool,
}

impl AccessSet {
    pub(crate) fn add_component(&mut self, component: ComponentType, mutable: bool) {
        let mode = if mutable {
            AccessMode::Write
        } else {
            AccessMode::Read
        };
        if let Some(existing) = self
            .components
            .iter()
            .find(|access| access.id == component.id())
        {
            assert!(
                existing.mode == AccessMode::Read && mode == AccessMode::Read,
                "system parameter conflict: component `{}` is requested as both {} and {}",
                component.name,
                mode_name(existing.mode),
                mode_name(mode),
            );
            return;
        }
        self.components.push(ComponentAccess {
            id: component.id(),
            name: component.name.clone(),
            mode,
        });
    }

    pub(crate) fn add_resource<R: 'static>(&mut self, mutable: bool) {
        let id = TypeId::of::<R>();
        let name = std::any::type_name::<R>();
        let mode = if mutable {
            AccessMode::Write
        } else {
            AccessMode::Read
        };
        if let Some(existing) = self.resources.iter().find(|access| access.id == id) {
            assert!(
                existing.mode == AccessMode::Read && mode == AccessMode::Read,
                "system parameter conflict: resource `{name}` is requested as both {} and {}",
                mode_name(existing.mode),
                mode_name(mode),
            );
            return;
        }
        self.resources.push(ResourceAccess { id, name, mode });
    }

    pub(crate) fn add_commands(&mut self) {
        assert!(
            !self.commands,
            "system parameter conflict: Commands may appear only once"
        );
        self.commands = true;
    }

    pub(crate) fn conflicts(&self, other: &Self) -> bool {
        for left in &self.components {
            for right in &other.components {
                if left.id == right.id
                    && (left.mode == AccessMode::Write || right.mode == AccessMode::Write)
                {
                    return true;
                }
            }
        }
        for left in &self.resources {
            for right in &other.resources {
                if left.id == right.id
                    && (left.mode == AccessMode::Write || right.mode == AccessMode::Write)
                {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn first_missing_resource(&self, world: &World) -> Option<&'static str> {
        self.resources
            .iter()
            .find(|access| !world.contains_resource_id(access.id))
            .map(|access| access.name)
    }

    pub(crate) fn diagnostics(&self) -> SystemAccessDiagnostics {
        let mut diagnostics = SystemAccessDiagnostics {
            uses_commands: self.commands,
            ..SystemAccessDiagnostics::default()
        };
        for access in &self.components {
            match access.mode {
                AccessMode::Read => {
                    diagnostics.component_reads.push(access.name.clone());
                    diagnostics.component_read_ids.push(access.id);
                }
                AccessMode::Write => {
                    diagnostics.component_writes.push(access.name.clone());
                    diagnostics.component_write_ids.push(access.id);
                }
            }
        }
        for access in &self.resources {
            match access.mode {
                AccessMode::Read => {
                    diagnostics.resource_reads.push(access.name.to_owned());
                    diagnostics.resource_read_ids.push(access.id);
                }
                AccessMode::Write => {
                    diagnostics.resource_writes.push(access.name.to_owned());
                    diagnostics.resource_write_ids.push(access.id);
                }
            }
        }
        diagnostics
    }
}

fn mode_name(mode: AccessMode) -> &'static str {
    match mode {
        AccessMode::Read => "read",
        AccessMode::Write => "write",
    }
}
