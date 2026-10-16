//! One pending command, consumed only when the host can execute it.
use super::{LoopError, seconds};

#[derive(Clone, Copy, Debug)]
pub struct CommandBuffer<T: Copy> {
    command: Option<T>,
    remaining: f64,
    lifetime: f64,
}
impl<T: Copy> CommandBuffer<T> {
    pub fn new(lifetime: f32) -> Result<Self, LoopError> {
        let lifetime = seconds(lifetime)?;
        if lifetime == 0. {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            command: None,
            remaining: 0.,
            lifetime,
        })
    }
    /// A new edge replaces the previous command. Holding a button is not an edge.
    pub fn push(&mut self, command: T) {
        self.command = Some(command);
        self.remaining = self.lifetime;
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), LoopError> {
        self.remaining = (self.remaining - seconds(dt)?).max(0.);
        if self.remaining <= 1e-9 {
            self.clear();
        }
        Ok(())
    }
    pub fn take_if(&mut self, ready: bool) -> Option<T> {
        if ready {
            self.remaining = 0.;
            self.command.take()
        } else {
            None
        }
    }
    pub fn clear(&mut self) {
        self.command = None;
        self.remaining = 0.;
    }
    pub fn pending(&self) -> Option<T> {
        self.command
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_commands_survive_then_execute_once_or_expire() {
        let mut b = CommandBuffer::new(0.12).unwrap();
        b.push(7);
        assert_eq!(b.take_if(false), None);
        b.advance(0.1).unwrap();
        assert_eq!(b.take_if(true), Some(7));
        assert_eq!(b.take_if(true), None);
        b.push(9);
        b.advance(0.2).unwrap();
        assert_eq!(b.take_if(true), None);
        b.push(2);
        assert_eq!(b.advance(f32::NAN), Err(LoopError::Invalid));
        assert_eq!(b.pending(), Some(2));
        b.push(3);
        assert_eq!(b.take_if(true), Some(3));
    }
}
