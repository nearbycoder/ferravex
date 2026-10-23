//! Bounded hierarchical tags for engine content and system filters.
//! Tags are case-sensitive ASCII segments separated by dots. A stored child
//! satisfies its parent query; `render.mesh` never matches `render.meshes`.
//!
//! [`TagPredicate`](crate::tags::TagPredicate) combines required (all), optional alternatives (any), and
//! excluded (none) queries. Build and validate a predicate once, then match
//! existing [`TagSet`](crate::tags::TagSet) values without allocating:
//!
//! ```
//! use ferravex_engine::tags::{TagError, TagPredicate, TagSet};
//!
//! let visible_actor = TagPredicate::default()
//!     .with_all(&["actor", "render.mesh"])?
//!     .with_any(&["team.friendly", "team.neutral"])?
//!     .with_none(&["state.hidden", "state.dead"])?;
//!
//! let mut actor = TagSet::default();
//! for tag in ["actor.npc", "render.mesh.static", "team.friendly"] {
//!     actor.insert(tag)?;
//! }
//! assert!(visible_actor.matches(&actor));
//! actor.insert("state.hidden.cloaked")?;
//! assert!(!visible_actor.matches(&actor));
//! # Ok::<(), TagError>(())
//! ```
use std::collections::BTreeSet;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagError {
    Invalid,
    Capacity,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagSet {
    tags: BTreeSet<String>,
}

/// A validated conjunction of required/all, optional/any and excluded/none tags.
///
/// Every required query must match, at least one optional query must match when
/// that clause is nonempty, and no excluded query may match. All clauses use
/// [`TagSet::matches`] hierarchy and case rules. An empty clause imposes no
/// restriction, so [`Self::default`] matches every set, including an empty set.
/// In particular, an empty optional clause differs from
/// [`TagSet::matches_any`] with an empty slice, which returns `false`.
///
/// Each clause owns at most 256 distinct queries of at most 128 ASCII bytes,
/// using the same validation and duplicate handling as [`TagSet::insert`].
/// Contradictory clauses are allowed and simply never match; exclusions always
/// veto a match, even when a required or optional query also matches.
/// Construction can allocate; matching does not allocate or mutate either value.
///
/// ```
/// use ferravex_engine::tags::{TagError, TagPredicate, TagSet};
///
/// let predicate = TagPredicate::default().with_all(&["render.mesh"])?;
/// let mut tags = TagSet::default();
/// tags.insert("render.mesh.animated")?;
/// assert!(predicate.matches(&tags));
/// assert_eq!(
///     TagPredicate::default().with_none(&["state..hidden"]),
///     Err(TagError::Invalid),
/// );
/// # Ok::<(), TagError>(())
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagPredicate {
    required: TagSet,
    optional: TagSet,
    excluded: TagSet,
}

impl TagPredicate {
    /// Replace the required clause: every query must match.
    ///
    /// Other clauses are preserved. An empty slice clears this restriction.
    /// Repeated calls replace rather than append queries.
    ///
    /// # Errors
    ///
    /// Returns [`TagError::Invalid`] for a malformed query, or
    /// [`TagError::Capacity`] when there are more than 256 distinct queries.
    ///
    /// ```
    /// use ferravex_engine::tags::{TagError, TagPredicate, TagSet};
    ///
    /// let predicate = TagPredicate::default().with_all(&["actor", "team.blue"])?;
    /// let mut tags = TagSet::default();
    /// tags.insert("actor.player")?;
    /// assert!(!predicate.matches(&tags));
    /// tags.insert("team.blue")?;
    /// assert!(predicate.matches(&tags));
    /// # Ok::<(), TagError>(())
    /// ```
    pub fn with_all(mut self, queries: &[&str]) -> Result<Self, TagError> {
        self.required = Self::checked_queries(queries)?;
        Ok(self)
    }

    /// Replace the optional alternatives: at least one query must match.
    ///
    /// Other clauses are preserved. An empty slice clears this restriction,
    /// allowing sets with no tags to pass this clause. Repeated calls replace
    /// rather than append alternatives.
    ///
    /// # Errors
    ///
    /// Returns [`TagError::Invalid`] for a malformed query, or
    /// [`TagError::Capacity`] when there are more than 256 distinct queries.
    ///
    /// ```
    /// use ferravex_engine::tags::{TagError, TagPredicate, TagSet};
    ///
    /// let predicate = TagPredicate::default().with_any(&["team.blue", "team.red"])?;
    /// let mut tags = TagSet::default();
    /// assert!(!predicate.matches(&tags));
    /// tags.insert("team.red.player")?;
    /// assert!(predicate.matches(&tags));
    /// # Ok::<(), TagError>(())
    /// ```
    pub fn with_any(mut self, queries: &[&str]) -> Result<Self, TagError> {
        self.optional = Self::checked_queries(queries)?;
        Ok(self)
    }

    /// Replace the excluded clause: no query may match.
    ///
    /// Other clauses are preserved. An empty slice clears this restriction.
    /// Repeated calls replace rather than append queries.
    ///
    /// # Errors
    ///
    /// Returns [`TagError::Invalid`] for a malformed query, or
    /// [`TagError::Capacity`] when there are more than 256 distinct queries.
    ///
    /// ```
    /// use ferravex_engine::tags::{TagError, TagPredicate, TagSet};
    ///
    /// let predicate = TagPredicate::default().with_none(&["state.hidden"])?;
    /// let mut tags = TagSet::default();
    /// assert!(predicate.matches(&tags));
    /// tags.insert("state.hidden.cloaked")?;
    /// assert!(!predicate.matches(&tags));
    /// # Ok::<(), TagError>(())
    /// ```
    pub fn with_none(mut self, queries: &[&str]) -> Result<Self, TagError> {
        self.excluded = Self::checked_queries(queries)?;
        Ok(self)
    }

    /// Match all three clauses against a tag set without allocating.
    ///
    /// ```
    /// use ferravex_engine::tags::{TagError, TagPredicate, TagSet};
    ///
    /// let predicate = TagPredicate::default()
    ///     .with_all(&["actor"])?
    ///     .with_none(&["state.dead"])?;
    /// let mut tags = TagSet::default();
    /// tags.insert("actor.enemy")?;
    /// assert!(predicate.matches(&tags));
    /// tags.insert("state.dead")?;
    /// assert!(!predicate.matches(&tags));
    /// # Ok::<(), TagError>(())
    /// ```
    pub fn matches(&self, tags: &TagSet) -> bool {
        self.required.iter().all(|query| tags.matches(query))
            && (self.optional.tags.is_empty()
                || self.optional.iter().any(|query| tags.matches(query)))
            && !self.excluded.iter().any(|query| tags.matches(query))
    }

    fn checked_queries(queries: &[&str]) -> Result<TagSet, TagError> {
        let mut tags = TagSet::default();
        for query in queries {
            tags.insert(query)?;
        }
        Ok(tags)
    }
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

    fn tag_set(queries: &[&str]) -> TagSet {
        let mut tags = TagSet::default();
        for query in queries {
            tags.insert(query).unwrap();
        }
        tags
    }

    #[test]
    fn predicate_requires_all_and_an_alternative_and_vetoes_exclusions() {
        let predicate = TagPredicate::default()
            .with_all(&["actor", "render.mesh"])
            .unwrap()
            .with_any(&["team.blue", "team.red"])
            .unwrap()
            .with_none(&["state.hidden", "state.dead"])
            .unwrap();
        let cases: &[(&[&str], bool)] = &[
            (&[], false),
            (&["actor", "render.mesh"], false),
            (&["actor", "team.blue"], false),
            (&["render.mesh", "team.blue"], false),
            (&["actor", "render.mesh", "team.blue"], true),
            (&["actor", "render.mesh", "team.red"], true),
            (&["actor", "render.mesh", "team.blue", "team.red"], true),
            (&["actor", "render.mesh", "team.green"], false),
            (
                &["actor", "render.mesh", "team.blue", "state.hidden"],
                false,
            ),
            (
                &["actor", "render.mesh", "team.red", "state.dead.corpse"],
                false,
            ),
        ];
        for &(queries, expected) in cases {
            assert_eq!(
                predicate.matches(&tag_set(queries)),
                expected,
                "{queries:?}"
            );
        }
    }

    #[test]
    fn predicate_clauses_share_hierarchical_segment_and_case_rules() {
        let required = TagPredicate::default().with_all(&["render.mesh"]).unwrap();
        let optional = TagPredicate::default().with_any(&["render.mesh"]).unwrap();
        let excluded = TagPredicate::default().with_none(&["render.mesh"]).unwrap();
        for (query, matches) in [
            ("render.mesh", true),
            ("render.mesh.static", true),
            ("render", false),
            ("render.meshes", false),
            ("render.mesh_extra", false),
            ("render.mesh-extra", false),
            ("Render.mesh", false),
            ("render.Mesh", false),
        ] {
            let tags = tag_set(&[query]);
            assert_eq!(required.matches(&tags), matches, "required: {query}");
            assert_eq!(optional.matches(&tags), matches, "optional: {query}");
            assert_eq!(excluded.matches(&tags), !matches, "excluded: {query}");
        }
    }

    #[test]
    fn empty_predicate_clauses_impose_no_restriction() {
        let empty = TagSet::default();
        let populated = tag_set(&["actor.player"]);
        let predicate = TagPredicate::default()
            .with_all(&[])
            .unwrap()
            .with_any(&[])
            .unwrap()
            .with_none(&[])
            .unwrap();
        assert_eq!(predicate, TagPredicate::default());
        assert!(predicate.matches(&empty));
        assert!(predicate.matches(&populated));
        assert!(!empty.matches_any(&[]));
        assert!(!populated.matches_any(&[]));
        assert!(empty.matches_all(&[]));
        assert!(populated.matches_all(&[]));

        assert!(
            predicate
                .clone()
                .with_none(&["actor"])
                .unwrap()
                .matches(&empty)
        );
        assert!(
            !predicate
                .clone()
                .with_all(&["actor"])
                .unwrap()
                .matches(&empty)
        );
        assert!(!predicate.with_any(&["actor"]).unwrap().matches(&empty));
    }

    #[test]
    fn replacing_or_clearing_one_clause_preserves_the_other_clauses() {
        let base = TagPredicate::default()
            .with_all(&["actor"])
            .unwrap()
            .with_any(&["team.blue"])
            .unwrap()
            .with_none(&["state.hidden"])
            .unwrap();
        let required = base.clone().with_all(&["prop"]).unwrap();
        assert!(required.matches(&tag_set(&["prop", "team.blue"])));
        assert!(!required.matches(&tag_set(&["actor", "team.blue"])));
        assert!(!required.matches(&tag_set(&["prop"])));
        assert!(!required.matches(&tag_set(&["prop", "team.blue", "state.hidden"])));

        let optional = base.clone().with_any(&["team.red"]).unwrap();
        assert!(optional.matches(&tag_set(&["actor", "team.red"])));
        assert!(!optional.matches(&tag_set(&["actor", "team.blue"])));
        assert!(!optional.matches(&tag_set(&["team.red"])));
        assert!(!optional.matches(&tag_set(&["actor", "team.red", "state.hidden"])));

        let excluded = base.clone().with_none(&["state.dead"]).unwrap();
        assert!(excluded.matches(&tag_set(&["actor", "team.blue", "state.hidden"])));
        assert!(!excluded.matches(&tag_set(&["actor", "team.blue", "state.dead"])));
        assert!(!excluded.matches(&tag_set(&["team.blue"])));
        assert!(!excluded.matches(&tag_set(&["actor"])));

        assert!(
            base.clone()
                .with_all(&[])
                .unwrap()
                .matches(&tag_set(&["team.blue"]))
        );
        assert!(
            base.clone()
                .with_any(&[])
                .unwrap()
                .matches(&tag_set(&["actor"]))
        );
        assert!(base.clone().with_none(&[]).unwrap().matches(&tag_set(&[
            "actor",
            "team.blue",
            "state.hidden"
        ])));
        assert!(base.matches(&tag_set(&["actor", "team.blue"])));
        assert!(!base.matches(&tag_set(&["actor", "team.blue", "state.hidden"])));
    }

    #[test]
    fn contradictory_clauses_are_valid_and_exclusions_win() {
        let tags = tag_set(&["actor.enemy"]);
        let predicate = TagPredicate::default()
            .with_all(&["actor.enemy"])
            .unwrap()
            .with_none(&["actor"])
            .unwrap();
        assert!(!predicate.matches(&tags));
        assert!(!predicate.matches(&TagSet::default()));

        let predicate = TagPredicate::default()
            .with_any(&["actor", "prop"])
            .unwrap()
            .with_none(&["actor"])
            .unwrap();
        assert!(!predicate.matches(&tags));
        assert!(!predicate.matches(&tag_set(&["actor.enemy", "prop"])));
        assert!(predicate.matches(&tag_set(&["prop"])));
    }

    #[test]
    fn every_clause_validates_all_queries_and_tag_length_boundaries() {
        let longest = "a".repeat(128);
        let too_long = "a".repeat(129);
        for builder in [
            TagPredicate::with_all,
            TagPredicate::with_any,
            TagPredicate::with_none,
        ] {
            for bad in ["", ".a", "a.", "a..b", "a/b", "a b", "é", "a\0b", &too_long] {
                assert_eq!(
                    builder(TagPredicate::default(), &["actor", bad]),
                    Err(TagError::Invalid),
                    "{bad:?}"
                );
            }
            assert!(builder(TagPredicate::default(), &[&longest]).is_ok());
            assert!(builder(TagPredicate::default(), &["A0_b-c.D9"]).is_ok());
        }
    }

    #[test]
    fn clauses_keep_tag_set_capacity_and_ignore_duplicates_at_capacity() {
        let names: Vec<_> = (0..257).map(|index| format!("tag.{index}")).collect();
        let queries: Vec<_> = names.iter().map(String::as_str).collect();
        let mut duplicate = queries[..256].to_vec();
        duplicate.extend_from_slice(&queries[..256]);
        for builder in [
            TagPredicate::with_all,
            TagPredicate::with_any,
            TagPredicate::with_none,
        ] {
            let full = builder(TagPredicate::default(), &queries[..256]).unwrap();
            assert_eq!(
                builder(TagPredicate::default(), &duplicate),
                Ok(full.clone())
            );
            assert_eq!(
                builder(TagPredicate::default(), &queries),
                Err(TagError::Capacity)
            );
            // Replacing a full clause does not consume capacity from its old contents.
            assert!(builder(full, &["replacement"]).is_ok());
        }

        let predicate = TagPredicate::default()
            .with_all(&queries[..256])
            .unwrap()
            .with_any(&queries[..256])
            .unwrap()
            .with_none(&queries[..256])
            .unwrap();
        let tags = tag_set(&queries[..256]);
        assert!(!predicate.matches(&tags));
        assert!(predicate.with_none(&[]).unwrap().matches(&tags));
    }

    #[test]
    fn predicate_owns_queries_and_observes_tag_set_changes() {
        let predicate = {
            let owned_query = String::from("actor");
            TagPredicate::default().with_all(&[&owned_query]).unwrap()
        };
        let mut tags = TagSet::default();
        assert!(!predicate.matches(&tags));
        tags.insert("actor.player").unwrap();
        assert!(predicate.matches(&tags));
        assert_eq!(tags.iter().collect::<Vec<_>>(), ["actor.player"]);
        tags.remove("actor.player");
        assert!(!predicate.matches(&tags));
    }
}
