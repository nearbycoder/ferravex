//! Explicit event visibility boundaries for ordered engine systems.
//! Sends enter a pending buffer; advance publishes it and expires the previous frame.
//! Each buffer holds at most `capacity` events (total retained count <= 2*capacity).
pub struct FrameEvents<T> {
    visible: Vec<T>,
    pending: Vec<T>,
    capacity: usize,
}
impl<T> FrameEvents<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            visible: Vec::new(),
            pending: Vec::new(),
            capacity,
        }
    }
    pub fn send(&mut self, event: T) -> Result<(), T> {
        if self.pending.len() >= self.capacity {
            return Err(event);
        }
        self.pending.push(event);
        Ok(())
    }
    pub fn read(&self) -> &[T] {
        &self.visible
    }
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
    pub fn advance(&mut self) {
        self.visible.clear();
        std::mem::swap(&mut self.visible, &mut self.pending);
    }
    pub fn clear(&mut self) {
        self.visible.clear();
        self.pending.clear();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn listeners_share_stable_frame_and_pending_events_wait() {
        let mut e = FrameEvents::new(2);
        e.send(1).unwrap();
        assert!(e.read().is_empty());
        e.advance();
        assert_eq!(e.read(), [1]);
        e.send(2).unwrap();
        e.send(3).unwrap();
        assert_eq!(e.send(4), Err(4));
        assert_eq!(e.read(), [1]);
        assert_eq!(e.read(), [1]);
        e.advance();
        assert_eq!(e.read(), [2, 3]);
        e.advance();
        assert!(e.read().is_empty());
        e.send(9).unwrap();
        e.clear();
        e.advance();
        assert!(e.read().is_empty());
        assert_eq!(FrameEvents::new(0).send(1), Err(1));
    }
}
