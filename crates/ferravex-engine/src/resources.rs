//! Typed singleton engine resources without string keys or unsafe downcasts.
//! At most 256 distinct types. Values are host-owned and may have arbitrary size.
//! This single-threaded store does not impose Send/Sync on resources.
use std::{
    any::{Any, TypeId},
    collections::HashMap,
};
#[derive(Default)]
pub struct Resources {
    values: HashMap<TypeId, Box<dyn Any>>,
}
impl Resources {
    /// Replace an existing value of T, returning it. Capacity failure returns the input.
    pub fn insert<T: Any>(&mut self, value: T) -> Result<Option<T>, T> {
        let id = TypeId::of::<T>();
        if !self.values.contains_key(&id) && self.values.len() == 256 {
            return Err(value);
        }
        Ok(self
            .values
            .insert(id, Box::new(value))
            .map(|old| *old.downcast::<T>().expect("resource type invariant")))
    }
    pub fn get<T: Any>(&self) -> Option<&T> {
        self.values.get(&TypeId::of::<T>())?.downcast_ref()
    }
    pub fn get_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.values.get_mut(&TypeId::of::<T>())?.downcast_mut()
    }
    pub fn remove<T: Any>(&mut self) -> Option<T> {
        self.values
            .remove(&TypeId::of::<T>())
            .and_then(|old| old.downcast::<T>().ok())
            .map(|v| *v)
    }
    pub fn contains<T: Any>(&self) -> bool {
        self.values.contains_key(&TypeId::of::<T>())
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn clear(&mut self) {
        self.values.clear();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn types_replace_independently_and_resources_drop_once() {
        use std::cell::Cell;
        use std::rc::Rc;
        struct Guard(Rc<Cell<u32>>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let mut r = Resources::default();
        assert_eq!(r.insert(3_u32), Ok(None));
        assert_eq!(r.insert(4_u64), Ok(None));
        assert_eq!(r.insert(5_u32), Ok(Some(3)));
        *r.get_mut::<u64>().unwrap() = 9;
        assert_eq!(r.get::<u32>(), Some(&5));
        assert_eq!(r.remove::<u64>(), Some(9));
        let drops = Rc::new(Cell::new(0));
        assert!(r.insert(Guard(drops.clone())).is_ok());
        assert_eq!(drops.get(), 0);
        r.clear();
        assert_eq!(drops.get(), 1);
        assert!(r.is_empty());
        assert!(r.get::<u32>().is_none());
    }
}
