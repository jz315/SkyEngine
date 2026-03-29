use super::{Bundle, EntityId, World};

trait WorldCommand {
    fn apply(self: Box<Self>, world: &mut World);
}

struct FnCommand<F>(Option<F>);

impl<F> WorldCommand for FnCommand<F>
where
    F: FnOnce(&mut World) + 'static,
{
    fn apply(mut self: Box<Self>, world: &mut World) {
        (self.0.take().unwrap())(world);
    }
}

#[derive(Default)]
pub struct Commands {
    queue: Vec<Box<dyn WorldCommand>>,
}

impl Commands {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    fn push<F>(&mut self, f: F)
    where
        F: FnOnce(&mut World) + 'static,
    {
        self.queue.push(Box::new(FnCommand(Some(f))));
    }


    pub fn spawn<B>(&mut self, bundle: B)
    where
        B: Bundle,
    {
        self.push(move |world| {
            world.spawn(bundle);
        });
    }

    pub fn despawn(&mut self, entity: EntityId) {
        self.push(move |world| {
            world.despawn(entity);
        });
    }

    pub fn insert<T>(&mut self, entity: EntityId, component: T)
    where
        T: Copy + 'static,
    {
        self.push(move |world| {
            world.insert(entity, component);
        });
    }

    pub fn remove<T>(&mut self, entity: EntityId)
    where
        T: 'static,
    {
        self.push(move |world| {
            world.remove::<T>(entity);
        });
    }

    pub fn insert_resource<R>(&mut self, resource: R)
    where
        R: 'static,
    {
        self.push(move |world| {
            world.insert_resource(resource);
        });
    }

    pub fn remove_resource<R>(&mut self)
    where
        R: 'static,
    {
        self.push(move |world| {
            world.remove_resource::<R>();
        });
    }

    pub fn apply(&mut self, world: &mut World) {
        for command in self.queue.drain(..) {
            command.apply(world);
        }
    }
}
