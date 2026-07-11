use crate::ecs::{CommandBuffer, World};
use std::marker::PhantomData;
use std::ptr::NonNull;

/// Scheduler-only capability to a structurally frozen World.
#[derive(Clone, Copy)]
pub(crate) struct UnsafeWorldCell<'w> {
    ptr: NonNull<World>,
    marker: PhantomData<&'w World>,
}

// Safety: this capability is created only for a compiled wave whose access
// sets are pairwise disjoint. Ordinary systems cannot mutate World structure.
unsafe impl Send for UnsafeWorldCell<'_> {}
unsafe impl Sync for UnsafeWorldCell<'_> {}

impl<'w> UnsafeWorldCell<'w> {
    pub(crate) unsafe fn new(world: *mut World) -> Self {
        Self {
            ptr: NonNull::new(world).expect("scheduler world pointer must not be null"),
            marker: PhantomData,
        }
    }

    pub(crate) unsafe fn world(self) -> &'w World {
        unsafe { self.ptr.as_ref() }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SystemParamContext<'w> {
    commands: NonNull<CommandBuffer>,
    marker: PhantomData<&'w mut CommandBuffer>,
}

impl<'w> SystemParamContext<'w> {
    pub(crate) unsafe fn new(commands: *mut CommandBuffer) -> Self {
        Self {
            commands: NonNull::new(commands).expect("system command buffer must not be null"),
            marker: PhantomData,
        }
    }

    pub(crate) fn commands(self) -> *mut CommandBuffer {
        self.commands.as_ptr()
    }
}
