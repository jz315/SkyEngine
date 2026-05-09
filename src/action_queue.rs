/// Small drainable queue for domain-level semantic actions.
///
/// Raw input and UI events should be translated into an `ActionQueue<T>` by
/// collector systems. Domain systems then drain the queue in one place and
/// mutate runtime state in a predictable order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionQueue<T> {
    actions: Vec<T>,
}

impl<T> ActionQueue<T> {
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    pub fn push(&mut self, action: T) {
        self.actions.push(action);
    }

    pub fn extend<I>(&mut self, actions: I)
    where
        I: IntoIterator<Item = T>,
    {
        self.actions.extend(actions);
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.actions.iter()
    }

    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        self.actions.drain(..)
    }

    pub fn clear(&mut self) {
        self.actions.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }
}

impl<T> Default for ActionQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::ActionQueue;

    #[test]
    fn action_queue_drains_in_insertion_order() {
        let mut queue = ActionQueue::new();
        queue.push(1);
        queue.extend([2, 3]);

        assert_eq!(queue.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(queue.drain().collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!(queue.is_empty());
    }
}
