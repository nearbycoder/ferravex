//! Deferred host/world mutations at an explicit synchronization boundary.
//! The command count is bounded; closures own arbitrary host-controlled payloads.
//! Panics propagate: commands already applied are not rolled back.
use std::collections::VecDeque;
type Command<W> = Box<dyn FnOnce(&mut W)>;
pub struct CommandBuffer<W> {
    commands: VecDeque<Command<W>>,
    capacity: usize,
}
impl<W> CommandBuffer<W> {
    pub fn new(capacity: usize) -> Self {
        Self {
            commands: VecDeque::new(),
            capacity,
        }
    }
    pub fn push<F: FnOnce(&mut W) + 'static>(&mut self, command: F) -> Result<(), F> {
        if self.commands.len() >= self.capacity {
            return Err(command);
        }
        self.commands.push_back(Box::new(command));
        Ok(())
    }
    /// Apply at most limit commands, in insertion order. Remaining commands wait.
    pub fn apply(&mut self, world: &mut W, limit: usize) -> usize {
        let count = limit.min(self.commands.len());
        for _ in 0..count {
            self.commands.pop_front().unwrap()(world);
        }
        count
    }
    pub fn len(&self) -> usize {
        self.commands.len()
    }
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
    pub fn clear(&mut self) {
        self.commands.clear();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn world_changes_only_at_sync_and_respects_budget() {
        let mut world = crate::ecs::World::new();
        let mut commands = CommandBuffer::new(2);
        assert!(
            commands
                .push(|w: &mut crate::ecs::World| {
                    w.spawn((11_u32,));
                })
                .is_ok()
        );
        assert!(
            commands
                .push(|w: &mut crate::ecs::World| {
                    w.spawn((22_u32,));
                })
                .is_ok()
        );
        assert!(
            commands
                .push(|_: &mut crate::ecs::World| panic!("rejected command ran"))
                .is_err()
        );
        assert_eq!(world.query::<&u32>().into_iter().count(), 0);
        assert_eq!(commands.apply(&mut world, 1), 1);
        assert_eq!(
            world
                .query::<&u32>()
                .into_iter()
                .map(|v| *v)
                .collect::<Vec<_>>(),
            [11]
        );
        assert_eq!(commands.len(), 1);
        assert_eq!(commands.apply(&mut world, 3), 1);
        assert_eq!(world.query::<&u32>().into_iter().count(), 2);
        assert!(commands.is_empty());
    }
}
