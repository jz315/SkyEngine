use crate::ecs::World;
use crate::gpu::GpuContext;
use crate::input::Input;

use super::{
    UiBackend, UiBackendId, UiBeginFrameContext, UiCaptureState, UiError, UiRenderContext,
};

/// Backend-neutral host for installed game UI backends.
#[derive(Default)]
pub struct UiHost {
    backends: Vec<Box<dyn UiBackend>>,
    capture: UiCaptureState,
}

impl UiHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<B>(&mut self, backend: B)
    where
        B: UiBackend,
    {
        let id = backend.id();
        if let Some(existing) = self.backends.iter_mut().find(|backend| backend.id() == id) {
            *existing = Box::new(backend);
        } else {
            self.backends.push(Box::new(backend));
        }
        self.refresh_capture();
    }

    pub fn contains(&self, id: UiBackendId) -> bool {
        self.backends.iter().any(|backend| backend.id() == id)
    }

    pub fn backend_mut<B>(&mut self) -> Option<&mut B>
    where
        B: UiBackend,
    {
        self.backends
            .iter_mut()
            .find_map(|backend| backend.as_any_mut().downcast_mut::<B>())
    }

    pub fn begin_frame(&mut self, world: &mut World, input: &Input, surface_size: [f32; 2]) {
        for backend in &mut self.backends {
            backend.begin_frame(UiBeginFrameContext {
                world,
                input,
                surface_size,
            });
        }
        self.refresh_capture();
    }

    pub fn render_overlays(
        &mut self,
        world: &mut World,
        gpu: &mut GpuContext,
    ) -> Result<(), UiError> {
        for backend in &mut self.backends {
            backend.render_overlay(UiRenderContext { world, gpu })?;
        }
        self.refresh_capture();
        Ok(())
    }

    pub fn capture(&self) -> UiCaptureState {
        self.capture
    }

    pub fn wants_pointer(&self) -> bool {
        self.capture.wants_pointer
    }

    pub fn wants_keyboard(&self) -> bool {
        self.capture.wants_keyboard
    }

    fn refresh_capture(&mut self) {
        let mut capture = UiCaptureState::default();
        for backend in &self.backends {
            capture.merge(backend.capture());
        }
        self.capture = capture;
    }
}

pub fn ensure_ui_host(world: &mut World) {
    if world.get_resource::<UiHost>().is_none() {
        world.insert_resource(UiHost::default());
    }
}

pub fn try_with_ui_host_mut<R>(
    world: &mut World,
    f: impl FnOnce(&mut UiHost, &mut World) -> R,
) -> Option<R> {
    let mut host = world.remove_resource::<UiHost>()?;
    let result = f(&mut host, world);
    world.insert_resource(host);
    Some(result)
}

pub fn with_ui_backend_mut<B, R>(world: &mut World, f: impl FnOnce(&mut B) -> R) -> Option<R>
where
    B: UiBackend,
{
    try_with_ui_host_mut(world, |host, _world| host.backend_mut::<B>().map(f)).flatten()
}

pub fn update_ui_backends(world: &mut World, input: &Input, surface_size: [f32; 2]) {
    super::super::ensure_ui_resources(world);
    try_with_ui_host_mut(world, |host, world| {
        host.begin_frame(world, input, surface_size);
    });
}

pub fn render_ui_overlays(world: &mut World, gpu: &mut GpuContext) -> Result<(), UiError> {
    super::super::ensure_ui_resources(world);
    try_with_ui_host_mut(world, |host, world| host.render_overlays(world, gpu)).unwrap_or(Ok(()))
}

pub fn ui_wants_pointer(world: &World) -> bool {
    world
        .get_resource::<UiHost>()
        .is_some_and(UiHost::wants_pointer)
}

pub fn ui_wants_keyboard(world: &World) -> bool {
    world
        .get_resource::<UiHost>()
        .is_some_and(UiHost::wants_keyboard)
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use crate::ecs::World;

    use super::super::{
        UiBackend, UiBackendId, UiBeginFrameContext, UiCaptureState, UiError, UiRenderContext,
    };
    use super::{try_with_ui_host_mut, UiHost};

    struct CaptureBackend {
        id: UiBackendId,
        capture: UiCaptureState,
    }

    impl UiBackend for CaptureBackend {
        fn id(&self) -> UiBackendId {
            self.id
        }

        fn begin_frame(&mut self, _ctx: UiBeginFrameContext<'_>) {}

        fn render_overlay(&mut self, _ctx: UiRenderContext<'_>) -> Result<(), UiError> {
            Ok(())
        }

        fn capture(&self) -> UiCaptureState {
            self.capture
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    #[test]
    fn host_aggregates_capture() {
        let mut host = UiHost::default();
        host.register(CaptureBackend {
            id: UiBackendId::new("pointer"),
            capture: UiCaptureState::new(true, false),
        });
        host.register(CaptureBackend {
            id: UiBackendId::new("keyboard"),
            capture: UiCaptureState::new(false, true),
        });

        assert!(host.wants_pointer());
        assert!(host.wants_keyboard());
    }

    #[test]
    fn registering_existing_backend_replaces_it() {
        let mut host = UiHost::default();
        let id = UiBackendId::new("backend");
        host.register(CaptureBackend {
            id,
            capture: UiCaptureState::new(true, false),
        });
        host.register(CaptureBackend {
            id,
            capture: UiCaptureState::new(false, true),
        });

        assert!(!host.wants_pointer());
        assert!(host.wants_keyboard());
    }

    #[test]
    fn host_can_be_temporarily_removed_while_world_is_mutable() {
        let mut world = World::new();
        world.insert_resource(UiHost::default());

        let updated = try_with_ui_host_mut(&mut world, |host, world| {
            world.insert_resource(12_u32);
            host.register(CaptureBackend {
                id: UiBackendId::new("capture"),
                capture: UiCaptureState::new(true, false),
            });
            true
        });

        assert_eq!(updated, Some(true));
        assert!(world.get_resource::<UiHost>().unwrap().wants_pointer());
        assert_eq!(world.get_resource::<u32>(), Some(&12));
    }
}
