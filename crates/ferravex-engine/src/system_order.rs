//! Stable topological ordering for sequential engine stages. No systems execute here.
//! Up to 256 named systems; equal-priority nodes preserve registration order.
use std::collections::BTreeSet;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderError {
    InvalidName,
    Duplicate,
    Capacity,
    UnknownSystem,
    Cycle,
}
#[derive(Default)]
pub struct SystemOrder {
    names: Vec<String>,
    edges: BTreeSet<(usize, usize)>,
}
impl SystemOrder {
    pub fn add(&mut self, name: &str) -> Result<(), OrderError> {
        if name.is_empty() || name.len() > 128 {
            return Err(OrderError::InvalidName);
        }
        if self.names.iter().any(|n| n == name) {
            return Err(OrderError::Duplicate);
        }
        if self.names.len() == 256 {
            return Err(OrderError::Capacity);
        }
        self.names.push(name.into());
        Ok(())
    }
    /// Declare `first` before `second`. Both names must already be registered.
    pub fn before(&mut self, first: &str, second: &str) -> Result<(), OrderError> {
        let find = |name: &str| {
            self.names
                .iter()
                .position(|n| n == name)
                .ok_or(OrderError::UnknownSystem)
        };
        let a = find(first)?;
        let b = find(second)?;
        self.edges.insert((a, b));
        Ok(())
    }
    pub fn resolve(&self) -> Result<Vec<&str>, OrderError> {
        let mut degree = vec![0; self.names.len()];
        for &(_, b) in &self.edges {
            degree[b] += 1;
        }
        let mut ready: BTreeSet<_> = degree
            .iter()
            .enumerate()
            .filter_map(|(i, &n)| (n == 0).then_some(i))
            .collect();
        let mut out = Vec::new();
        while let Some(a) = ready.pop_first() {
            out.push(self.names[a].as_str());
            for &(_, b) in self.edges.range((a, 0)..=(a, usize::MAX)) {
                degree[b] -= 1;
                if degree[b] == 0 {
                    ready.insert(b);
                }
            }
        }
        if out.len() == self.names.len() {
            Ok(out)
        } else {
            Err(OrderError::Cycle)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_dependencies_duplicates_and_cycles() {
        let mut s = SystemOrder::default();
        for n in ["render", "input", "physics", "audio"] {
            s.add(n).unwrap();
        }
        s.before("input", "physics").unwrap();
        s.before("physics", "render").unwrap();
        s.before("physics", "render").unwrap();
        assert_eq!(
            s.resolve().unwrap(),
            ["input", "physics", "render", "audio"]
        );
        assert_eq!(s.add("input"), Err(OrderError::Duplicate));
        assert_eq!(s.before("missing", "input"), Err(OrderError::UnknownSystem));
        s.before("render", "input").unwrap();
        assert_eq!(s.resolve(), Err(OrderError::Cycle));
        let mut self_cycle = SystemOrder::default();
        self_cycle.add("x").unwrap();
        self_cycle.before("x", "x").unwrap();
        assert!(self_cycle.resolve().is_err());
    }
}
