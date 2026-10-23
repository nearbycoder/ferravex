//! Checked authoring of the existing axis-aligned [`Arena`], without a new map
//! format. Raw arena literals, ID-modulo spawn selection and legacy origin
//! fallback remain available. Validation never moves, normalizes or snaps input.
//!
//! Bounds/counts match `checkpoint::arena_valid`: 256 solids, [`MAX_PLAYERS`]
//! spawns, solid center/half components at most 8192 in magnitude, strictly
//! positive half extents, and spawn components at most 4096 in magnitude.
//! Character clearance is opt-in: geometry accepted without it is not a promise
//! that a particular character shape can use it (notably planar `Box2`).
//!
//! This runnable recipe rejects late malformed/blocked input, then uses the
//! accepted arena through the existing spawn and real 64 Hz match step paths:
//! ```
//! use ferravex_fps::{InputFrame, Match, Obstacle, TICK_RATE};
//! use ferravex_fps::arena_authoring::*;
//! use ferravex_math::Vec3;
//! let options = AuthoringOptions {
//!     clearance: Some(SpawnClearance::default()),
//!     ..AuthoringOptions::default()
//! };
//! let mut builder = ArenaBuilder::new(options)?;
//! builder.try_push_solid(Obstacle {
//!     center: Vec3::new(3., 1., 0.), half_extents: Vec3::ONE,
//! }).unwrap();
//! builder.try_push_spawn(Vec3::ZERO).unwrap();
//! let rejected = builder.try_push_solid(Obstacle {
//!     center: Vec3::ZERO, half_extents: Vec3::splat(-1.),
//! }).unwrap_err();
//! assert!(matches!(rejected.error, ArenaAuthoringError::Solid { index: 1, .. }));
//! builder.try_push_spawn(Vec3::new(3., 0., 0.)).unwrap();
//! let rejected = builder.finish().unwrap_err();
//! assert!(matches!(rejected.error, ArenaAuthoringError::SpawnClearance {
//!     spawn: SpawnSite::Authored(1), solid: Some(0), ..
//! }));
//! // Recover the builder and reuse its bounded storage after rejection.
//! let mut builder = rejected.input;
//! builder.reset();
//! builder.try_push_solid(Obstacle {
//!     center: Vec3::new(3., 1., 0.), half_extents: Vec3::ONE,
//! }).unwrap();
//! builder.try_push_spawn(Vec3::ZERO).unwrap();
//! let mut game = Match::new(builder.finish().unwrap());
//! assert!(game.spawn_player(0));
//! for _ in 0..TICK_RATE {
//!     game.step(&[(0, InputFrame { move_z: 1., ..InputFrame::default() })]);
//! }
//! assert_eq!(game.tick, TICK_RATE);
//! assert!(game.player(0).unwrap().position.z < -1.);
//! # Ok::<(), ArenaAuthoringError>(())
//! ```
use crate::{Arena, MAX_PLAYERS, Obstacle, PLAYER_HEIGHT, PLAYER_RADIUS};
use ferravex_math::Vec3;
use ferravex_physics::character::{CharacterShape, ClearanceError, Terrain, check_clearance};

/// Checkpoint-compatible hard upper count, including author-supplied limits.
pub const MAX_SOLIDS: usize = 256;
/// Checkpoint-compatible hard upper spawn count.
pub const MAX_SPAWNS: usize = MAX_PLAYERS;

/// Requested storage/content limits. Zero is valid; larger than the hard maxima
/// is rejected, never silently clamped. Builder storage is reserved once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaLimits {
    pub solids: usize,
    pub spawns: usize,
}
impl Default for ArenaLimits {
    fn default() -> Self {
        Self {
            solids: MAX_SOLIDS,
            spawns: MAX_SPAWNS,
        }
    }
}

/// Deliberate handling of an arena with no authored spawn entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EmptySpawnPolicy {
    /// Checked authoring requires at least one authored spawn.
    #[default]
    Reject,
    /// Keep the empty vector and `Arena::spawn`'s origin fallback. When clearance
    /// is enabled, check that origin too; do not insert a new spawn entry.
    LegacyOrigin,
}

/// Optional current-pose clearance. All arena solids and implicit ground use
/// the existing adapter and all membership layers. Touching (within the motor's
/// `1e-5` tolerance) is clear. Budget exhaustion never proves clearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnClearance {
    /// Capsule or planar Box2, with the existing character geometry limits.
    pub shape: CharacterShape,
    /// Per-spawn candidate processing budget, in `0..=1_000_000`. The ground
    /// always consumes one candidate, even far above the ground.
    pub max_candidates: usize,
}
impl Default for SpawnClearance {
    fn default() -> Self {
        Self {
            shape: CharacterShape::Capsule {
                radius: PLAYER_RADIUS,
                height: PLAYER_HEIGHT,
            },
            max_candidates: MAX_SOLIDS + 1,
        }
    }
}

/// Settings for validation and the reusable builder. Defaults require authored
/// spawns, use checkpoint caps and leave character clearance disabled.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AuthoringOptions {
    pub limits: ArenaLimits,
    pub empty_spawns: EmptySpawnPolicy,
    pub clearance: Option<SpawnClearance>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolidError {
    NonFiniteCenter,
    NonFiniteHalfExtents,
    /// Zero is rejected too, matching both checkpoint and cuboid geometry.
    NonPositiveHalfExtents,
    /// Center +/- half extents overflowed; checked before component range limits.
    NonFiniteBounds,
    CenterLimit,
    HalfExtentsLimit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnError {
    NonFinite,
    PositionLimit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnSite {
    Authored(usize),
    LegacyOrigin,
}

/// Source indices are zero-based vector positions. A rejected push uses the
/// next accepted position; rejection does not consume an index. For clearance,
/// `solid` identifies the blocker/invalid solid when known; `None` means implicit
/// ground or an input/budget failure. `error` is the actual character query error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaAuthoringError {
    InvalidLimits {
        limits: ArenaLimits,
    },
    InvalidClearance,
    SolidCapacity {
        index: usize,
        limit: usize,
    },
    SpawnCapacity {
        index: usize,
        limit: usize,
    },
    Solid {
        index: usize,
        problem: SolidError,
    },
    Spawn {
        index: usize,
        problem: SpawnError,
    },
    EmptySpawns,
    SpawnClearance {
        spawn: SpawnSite,
        solid: Option<usize>,
        error: ClearanceError,
    },
}
impl std::fmt::Display for ArenaAuthoringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "arena authoring: {self:?}")
    }
}
impl std::error::Error for ArenaAuthoringError {}

/// A failed ownership-taking operation returns the exact rejected input.
#[derive(Debug)]
pub struct Rejected<T> {
    pub error: ArenaAuthoringError,
    pub input: T,
}

fn validate_options(options: AuthoringOptions) -> Result<(), ArenaAuthoringError> {
    if options.limits.solids > MAX_SOLIDS || options.limits.spawns > MAX_SPAWNS {
        return Err(ArenaAuthoringError::InvalidLimits {
            limits: options.limits,
        });
    }
    if options
        .clearance
        .is_some_and(|c| !c.shape.valid() || c.max_candidates > 1_000_000)
    {
        return Err(ArenaAuthoringError::InvalidClearance);
    }
    Ok(())
}
fn validate_solid(solid: Obstacle, index: usize) -> Result<(), ArenaAuthoringError> {
    let problem = if !solid.center.is_finite() {
        Some(SolidError::NonFiniteCenter)
    } else if !solid.half_extents.is_finite() {
        Some(SolidError::NonFiniteHalfExtents)
    } else if solid.half_extents.min_element() <= 0. {
        Some(SolidError::NonPositiveHalfExtents)
    } else if !solid.min().is_finite() || !solid.max().is_finite() {
        Some(SolidError::NonFiniteBounds)
    } else if solid.center.abs().max_element() > 8192. {
        Some(SolidError::CenterLimit)
    } else if solid.half_extents.max_element() > 8192. {
        Some(SolidError::HalfExtentsLimit)
    } else {
        None
    };
    problem.map_or(Ok(()), |problem| {
        Err(ArenaAuthoringError::Solid { index, problem })
    })
}
fn validate_spawn(spawn: Vec3, index: usize) -> Result<(), ArenaAuthoringError> {
    let problem = if !spawn.is_finite() {
        Some(SpawnError::NonFinite)
    } else if spawn.abs().max_element() > 4096. {
        Some(SpawnError::PositionLimit)
    } else {
        None
    };
    problem.map_or(Ok(()), |problem| {
        Err(ArenaAuthoringError::Spawn { index, problem })
    })
}

fn clearance_at(
    arena: &Arena,
    clearance: SpawnClearance,
    feet: Vec3,
    spawn: SpawnSite,
) -> Result<(), ArenaAuthoringError> {
    check_clearance(
        arena,
        clearance.shape,
        feet,
        u32::MAX,
        clearance.max_candidates,
    )
    .map(|_| ())
    .map_err(|error| {
        let mut solid = match error {
            ClearanceError::Blocked { obstacle } => obstacle.checked_sub(1).map(|i| i as usize),
            _ => None,
        };
        if error == ClearanceError::InvalidObstacle {
            // The query intentionally returns no ID for malformed/unsupported
            // geometry. Diagnose using the SAME adapter and SAME kernel on
            // each visited singleton, only on this error path. No second
            // collision implementation or scratch vector is involved.
            arena.visit(clearance.shape.bounds(feet), &mut |obstacle| {
                if solid.is_none()
                    && check_clearance(&[obstacle][..], clearance.shape, feet, u32::MAX, 1)
                        == Err(ClearanceError::InvalidObstacle)
                {
                    solid = obstacle.id.checked_sub(1).map(|i| i as usize);
                }
            });
        }
        ArenaAuthoringError::SpawnClearance {
            spawn,
            solid,
            error,
        }
    })
}

/// Validate every supplied entry before accepting an arena. Returns the first
/// error in deterministic order: options, counts, solid order, spawn order, empty
/// policy, then clearance in spawn order. A clear early spawn cannot hide a late
/// failure. Does not allocate or mutate. See the module's executed recipe.
pub fn validate_arena(arena: &Arena, options: AuthoringOptions) -> Result<(), ArenaAuthoringError> {
    validate_options(options)?;
    if arena.solids.len() > options.limits.solids {
        return Err(ArenaAuthoringError::SolidCapacity {
            index: options.limits.solids,
            limit: options.limits.solids,
        });
    }
    if arena.spawns.len() > options.limits.spawns {
        return Err(ArenaAuthoringError::SpawnCapacity {
            index: options.limits.spawns,
            limit: options.limits.spawns,
        });
    }
    for (index, &solid) in arena.solids.iter().enumerate() {
        validate_solid(solid, index)?;
    }
    for (index, &spawn) in arena.spawns.iter().enumerate() {
        validate_spawn(spawn, index)?;
    }
    if arena.spawns.is_empty() && options.empty_spawns == EmptySpawnPolicy::Reject {
        return Err(ArenaAuthoringError::EmptySpawns);
    }
    if let Some(clearance) = options.clearance {
        if arena.spawns.is_empty() {
            clearance_at(arena, clearance, Vec3::ZERO, SpawnSite::LegacyOrigin)?;
        } else {
            for (index, &feet) in arena.spawns.iter().enumerate() {
                clearance_at(arena, clearance, feet, SpawnSite::Authored(index))?;
            }
        }
    }
    Ok(())
}

/// Check an owned raw arena without copying/reformatting its vectors. Failure
/// returns those same vectors, even for a malformed last entry. Caller-owned
/// vector capacities are preserved; only builder-owned storage promises bounded
/// reservation. See [`validate_arena`] for validation ordering.
/// ```
/// use ferravex_fps::{Arena, arena_authoring::*};
/// let arena = try_from_arena(Arena::default(), AuthoringOptions::default()).unwrap();
/// assert_eq!(arena.spawns.len(), MAX_SPAWNS);
/// ```
pub fn try_from_arena(arena: Arena, options: AuthoringOptions) -> Result<Arena, Rejected<Arena>> {
    match validate_arena(&arena, options) {
        Ok(()) => Ok(arena),
        Err(error) => Err(Rejected {
            error,
            input: arena,
        }),
    }
}

/// Bounded reusable storage for authoring the existing Arena. Each push checks
/// geometry/counts only; final validation checks empty policy and clearance
/// against ALL final solids, including solids added after spawns. No successful
/// push promises final clearance. Storage never grows after construction.
#[derive(Debug)]
pub struct ArenaBuilder {
    options: AuthoringOptions,
    arena: Arena,
}
impl ArenaBuilder {
    /// Validate options before reserving each vector at its explicit count cap.
    /// As with standard Vec construction, allocator exhaustion is not recovered.
    pub fn new(options: AuthoringOptions) -> Result<Self, ArenaAuthoringError> {
        validate_options(options)?;
        Ok(Self {
            options,
            arena: Arena {
                solids: Vec::with_capacity(options.limits.solids),
                spawns: Vec::with_capacity(options.limits.spawns),
            },
        })
    }
    /// Accepted entries in original order; immutable to preserve checked pushes.
    pub fn arena(&self) -> &Arena {
        &self.arena
    }
    /// Copy the options retained across reset/rejection.
    pub fn options(&self) -> AuthoringOptions {
        self.options
    }
    /// Append at the returned stable index; error preserves accepted entries and
    /// capacity, returning the exact rejected obstacle.
    pub fn try_push_solid(&mut self, input: Obstacle) -> Result<usize, Rejected<Obstacle>> {
        let index = self.arena.solids.len();
        let result = if index == self.options.limits.solids {
            Err(ArenaAuthoringError::SolidCapacity {
                index,
                limit: self.options.limits.solids,
            })
        } else {
            validate_solid(input, index)
        };
        result.map_err(|error| Rejected { error, input })?;
        self.arena.solids.push(input);
        Ok(index)
    }
    /// Append at the returned stable index; error preserves accepted entries and
    /// returns the exact rejected spawn. Clearance waits for final validation.
    pub fn try_push_spawn(&mut self, input: Vec3) -> Result<usize, Rejected<Vec3>> {
        let index = self.arena.spawns.len();
        let result = if index == self.options.limits.spawns {
            Err(ArenaAuthoringError::SpawnCapacity {
                index,
                limit: self.options.limits.spawns,
            })
        } else {
            validate_spawn(input, index)
        };
        result.map_err(|error| Rejected { error, input })?;
        self.arena.spawns.push(input);
        Ok(index)
    }
    /// Validate final content in place; failure retains storage and all entries.
    pub fn validate(&self) -> Result<(), ArenaAuthoringError> {
        validate_arena(&self.arena, self.options)
    }
    /// Clear accepted entries without reallocating or changing options. Source
    /// indices restart at zero and no clearance/query history is retained.
    pub fn reset(&mut self) {
        self.arena.solids.clear();
        self.arena.spawns.clear();
    }
    /// Produce the SAME Arena type with no clone/allocation or data rewriting.
    /// Failure returns this complete builder for inspection/reset/reuse.
    // Returning the inline builder deliberately preserves ownership without a
    // failure-path Box allocation. Its size is bounded independently of content.
    #[allow(clippy::result_large_err)]
    pub fn finish(self) -> Result<Arena, Rejected<Self>> {
        match self.validate() {
            Ok(()) => Ok(self.arena),
            Err(error) => Err(Rejected { error, input: self }),
        }
    }
}
