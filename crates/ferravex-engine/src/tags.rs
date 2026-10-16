//! Bounded hierarchical tags for engine content and system filters.
//! Tags are case-sensitive ASCII segments separated by dots. A stored child
//! satisfies its parent query; `render.mesh` never matches `render.meshes`.
use std::collections::BTreeSet;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagError {
    Invalid,
    Capacity,
}
#[derive(Clone, Default)]
pub struct TagSet {
    tags: BTreeSet<String>,
}
fn valid(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 128
        && tag.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        })
}
impl TagSet {
    pub fn insert(&mut self, tag: &str) -> Result<bool, TagError> {
        if !valid(tag) {
            return Err(TagError::Invalid);
        }
        if self.tags.contains(tag) {
            return Ok(false);
        }
        if self.tags.len() == 256 {
            return Err(TagError::Capacity);
        }
        Ok(self.tags.insert(tag.into()))
    }
    pub fn remove(&mut self, tag: &str) -> bool {
        self.tags.remove(tag)
    }
    pub fn contains(&self, tag: &str) -> bool {
        self.tags.contains(tag)
    }
    pub fn matches(&self, query: &str) -> bool {
        if !valid(query) {
            return false;
        }
        self.tags.iter().any(|tag| {
            tag == query
                || tag
                    .strip_prefix(query)
                    .is_some_and(|suffix| suffix.starts_with('.'))
        })
    }
    pub fn matches_all(&self, queries: &[&str]) -> bool {
        queries.iter().all(|q| self.matches(q))
    }
    pub fn matches_any(&self, queries: &[&str]) -> bool {
        queries.iter().any(|q| self.matches(q))
    }
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.tags.iter().map(String::as_str)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn segment_boundaries_validation_and_composite_queries() {
        let mut t = TagSet::default();
        t.insert("render.mesh.static").unwrap();
        t.insert("audio.music").unwrap();
        assert!(t.matches("render"));
        assert!(t.matches("render.mesh"));
        assert!(!t.contains("render.mesh"));
        assert!(!t.matches("render.me"));
        assert!(!t.matches("render.meshes"));
        assert!(!t.matches("Render"));
        assert!(t.matches_all(&["render", "audio"]));
        assert!(t.matches_any(&["missing", "audio"]));
        assert!(t.matches_all(&[]));
        assert!(!t.matches_any(&[]));
        for bad in ["", "a..b", ".a", "a.", "a/b"] {
            assert_eq!(t.insert(bad), Err(TagError::Invalid));
            assert!(!t.matches(bad));
        }
        assert_eq!(t.insert("audio.music"), Ok(false));
        assert!(t.remove("audio.music"));
        assert!(!t.matches("audio"));
    }
}
