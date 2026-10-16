//! Bounded reusable object slots with generation-checked handles.
//! Handles are scoped to the originating pool. Exhausted generations retire a slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle {
    index: usize,
    generation: u64,
}
struct Slot<T> {
    generation: u64,
    value: Option<T>,
}
pub struct Pool<T> {
    slots: Vec<Slot<T>>,
    free: Vec<usize>,
    capacity: usize,
    len: usize,
}
impl<T> Pool<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            capacity,
            len: 0,
        }
    }
    pub fn insert(&mut self, value: T) -> Result<Handle, T> {
        let index = if let Some(i) = self.free.pop() {
            i
        } else {
            if self.slots.len() >= self.capacity {
                return Err(value);
            }
            self.slots.push(Slot {
                generation: 0,
                value: None,
            });
            self.slots.len() - 1
        };
        let slot = &mut self.slots[index];
        slot.value = Some(value);
        self.len += 1;
        Ok(Handle {
            index,
            generation: slot.generation,
        })
    }
    pub fn get(&self, h: Handle) -> Option<&T> {
        let s = self.slots.get(h.index)?;
        (s.generation == h.generation)
            .then_some(s.value.as_ref())
            .flatten()
    }
    pub fn get_mut(&mut self, h: Handle) -> Option<&mut T> {
        let s = self.slots.get_mut(h.index)?;
        if s.generation != h.generation {
            return None;
        }
        s.value.as_mut()
    }
    pub fn remove(&mut self, h: Handle) -> Option<T> {
        let s = self.slots.get_mut(h.index)?;
        if s.generation != h.generation {
            return None;
        }
        let value = s.value.take()?;
        self.len -= 1;
        if let Some(next) = s.generation.checked_add(1) {
            s.generation = next;
            self.free.push(h.index);
        }
        Some(value)
    }
    pub fn iter(&self) -> impl Iterator<Item = (Handle, &T)> {
        self.slots.iter().enumerate().filter_map(|(index, s)| {
            s.value.as_ref().map(|v| {
                (
                    Handle {
                        index,
                        generation: s.generation,
                    },
                    v,
                )
            })
        })
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reuse_rejects_stale_handles_and_preserves_live_objects() {
        let mut p = Pool::new(2);
        let a = p.insert("a").unwrap();
        let b = p.insert("b").unwrap();
        assert_eq!(p.insert("full"), Err("full"));
        assert_eq!(p.remove(a), Some("a"));
        let c = p.insert("c").unwrap();
        assert_ne!(a, c);
        assert_eq!(a.index, c.index);
        assert_eq!(p.get(a), None);
        assert_eq!(p.remove(a), None);
        *p.get_mut(c).unwrap() = "updated";
        assert_eq!(
            p.iter().map(|(_, v)| *v).collect::<Vec<_>>(),
            ["updated", "b"]
        );
        assert_eq!(p.get(b), Some(&"b"));
        p.remove(b);
        p.remove(c);
        assert!(p.is_empty());
    }
    #[test]
    fn generation_exhaustion_retires_slot() {
        let mut p = Pool::new(1);
        let h = p.insert(1).unwrap();
        p.slots[h.index].generation = u64::MAX;
        let h = Handle {
            generation: u64::MAX,
            ..h
        };
        assert_eq!(p.remove(h), Some(1));
        assert_eq!(p.insert(2), Err(2));
        assert!(p.get(h).is_none());
    }
}
