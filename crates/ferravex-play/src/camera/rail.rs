//! Optional presentation-only intro/result rails, sampled at caller-owned seconds.
//!
//! Cubics use polynomial **parameter** time, not arc length. Routes use their
//! measured polyline arc length (including a closed route's seam). Convert a
//! curve with [`tessellate_route_into`] for approximate constant-distance travel;
//! an incomplete tessellation never replaces the last good route. Neither path
//! aims the camera along its tangent: orientation, roll, view distance and FOV
//! come from authored [`RailKey3`] poses through the existing [`Transition3`].
//! Segments prepare one normalized transition at construction. Each query samples
//! a copy once; it never retargets a live transition or steps hidden simulation.
//!
//! Time is finite, nonnegative f64 seconds since this rail's start. Seeking is
//! allowed, including backward and across many loops. Boundaries are right
//! continuous: all zero-duration cuts at a boundary apply before the following
//! segment starts. Once mode clamps at the exact final key; loop mode wraps at
//! the exact duration, reporting an absolute cycle number. These are observations,
//! not queued events or evidence of gameplay completion. Reduced motion always
//! returns the exact final key as a finished cut, even for loops and held players.
//!
//! A raw sample reports cuts at start, exact authored cut boundaries and loop
//! seams. [`RailPlayer3`] additionally marks backward seeks, crossed cuts/loops,
//! initialization and reset as discontinuities. Hold retains the displayed sample;
//! the host must freeze/rebase its own playhead before resume (no automatic clock
//! offset). Reset forgets the pose/hold; the next query initializes at supplied
//! time. Renderer interpolation/history must honor cuts. Subject history,
//! collisions, accepted teleports, lens matrices and look-input tuning stay host
//! owned. Local menus must continue authoritative network/replay progression.
//!
//! Storage/work: at most 64 borrowed segments, 1024 batch samples, 4096 route
//! points and 8191 tessellation work nodes. Queries allocate nothing. Orientation
//! fraction rounds to f32 (as in Transition3); position uses f64 path progress.
//! Pose/lens bounds and shortest-arc half-turn ties are Transition3's conventions.
//! Intermediate target rounding can still fail; errors preserve player state and
//! the entire batch destination. A valid endpoint pair is not a guarantee of all
//! interior samples. Loop counts >= 2^53 are rejected rather than cast/saturated;
//! f64 time's existing loss of fractional precision cannot be recovered. Loop
//! phase uses remainder of the supplied binary seconds, without epsilon snapping
//! to decimal boundaries (for example, 1 second is just under ten stored 0.1s
//! durations). Cycle counting follows that same remainder convention.
//!
//! # Executable optional intro/result recipe
//!
//! The host chooses this optional view without changing game defaults or ticks.
//! This headless recipe exercises real authoring, sampling, hold, restart and an
//! accepted Rig3 teleport handoff. No wall clock, renderer or subject mutation.
//!
//! ```rust
//! use ferravex_math::{Vec3, curves::{CubicBezier3, TessellationOptions}, projection::NdcDepth};
//! use ferravex_play::{navigation::Route3, camera::{Pose3, Rig3, Input3, Mode3,
//!     Target3, NoObstructions, transition::{Perspective3, Transition3, ViewHandedness}, rail::*}};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let lens = Perspective3 { aspect: 16./9., near: 0.1, far: 1000.,
//!         handedness: ViewHandedness::Right, depth: NdcDepth::ZeroToOne };
//!     let key = |x| RailKey3 { pose: Pose3 { position: Vec3::X*x,
//!         target: Vec3::X*x - Vec3::Z*4., ..Pose3::default() }, projection: lens };
//!     let curve = CubicBezier3::new([Vec3::ZERO, Vec3::Y, Vec3::X*4.+Vec3::Y, Vec3::X*4.])?;
//!     let mut route = Route3::new(128).map_err(RailError::Route)?;
//!     let mut scratch = [Vec3::ZERO; 128];
//!     tessellate_route_into(curve, &mut route, &mut scratch, TessellationOptions {
//!         tolerance: 0.01, max_depth: 16, max_work: 1023 })?;
//!     let segments = [RailSegment3::new(RailPath3::Route(&route), key(0.), key(4.), 2.)?];
//!     let rail = Rail3::new(&segments, RailMode::Once)?;
//!     let mut intro = RailPlayer3::new(rail);
//!     assert!(intro.sample_at(0., false)?.cut);
//!     let visible = intro.sample_at(0.5, false)?;
//!     intro.hold();
//!     assert_eq!(intro.sample_at(90., false)?, visible);
//!     intro.resume(); // The host discarded paused time, retaining playhead 0.5.
//!     assert_eq!(intro.sample_at(0.5, false)?.pose, visible.pose);
//!     assert!(intro.sample_at(2., false)?.finished);
//!     intro.reset(); // Restart also resets host simulation/subject history.
//!     assert_eq!(intro.displayed(), None);
//!     assert!(intro.sample_at(0., false)?.cut);
//!     intro.hold();
//!     let result = intro.sample_at(0.2, true)?; // Motion preference overrides hold.
//!     assert!(result.cut && result.finished);
//!     assert_eq!(result.pose, key(4.).pose);
//!
//!     // The host has accepted a teleport and reset its subject history first.
//!     let mut rig = Rig3::default();
//!     let accepted = rig.snap(Input3 { target: Target3::at(Vec3::X*100.),
//!         mode: Mode3::Follow { eye_offset: Vec3::Z*5., look_offset: Vec3::ZERO },
//!         fov_y: 1. }, &NoObstructions)?;
//!     let mut live = Transition3::new();
//!     live.cut(result.pose, result.projection)?;
//!     live.retarget_frame(&accepted, lens, 1.)?;
//!     assert_eq!(live.pose(), Some(accepted.pose));
//!     Ok(())
//! }
//! ```

use super::{
    Pose3,
    transition::{Perspective3, Transition3, TransitionError},
};
use crate::navigation::{MAX_ROUTE_POINTS, NavigationError, Route3};
use ferravex_math::{
    Vec3,
    curves::{CubicBezier3, CurveError, Tessellation, TessellationOptions, TessellationStatus},
};

/// Maximum authored segments, including zero-duration cuts. See module recipe.
pub const MAX_RAIL_SEGMENTS: usize = 64;
/// Maximum times accepted by one atomic batch. See [`Rail3::sample_into`].
pub const MAX_RAIL_SAMPLES: usize = 1024;
/// Maximum curve subdivision work accepted by [`tessellate_route_into`].
pub const MAX_RAIL_TESSELLATION_WORK: usize = 8191;

/// Authored world-space look pose and explicit host projection intent.
/// See module recipe. Existing public camera literals/defaults are unchanged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RailKey3 {
    pub pose: Pose3,
    pub projection: Perspective3,
}

/// Position source; orientation is independent. See [`RailSegment3::new`].
#[derive(Clone, Copy)]
pub enum RailPath3<'a> {
    /// An explicit zero-duration discontinuity; permits projection changes.
    Cut,
    /// Full cubic, uniformly in polynomial parameter, without tessellation.
    Cubic(CubicBezier3),
    /// Full immutable route, uniformly in measured polyline arc distance.
    /// A closed route ends at its first point; zero-length nonempty routes work.
    Route(&'a Route3),
}

/// Playback topology. Loop seams are explicit cuts, even if poses happen to match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailMode {
    Once,
    Loop,
}

/// Rejected authoring/query; no partial player or batch output is committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RailError {
    /// Empty/oversized rail, batch, scratch or tessellation work budget.
    Capacity,
    /// Nonfinite/negative time or duration; durations exceed 3600 seconds.
    InvalidTime,
    /// Positive-duration cut or zero-duration moving segment.
    InvalidCut,
    /// Empty route or position source does not meet both authored eyes.
    InvalidPath,
    /// Adjacent end/start keys differ. Insert an explicit cut instead.
    Disconnected,
    /// A loop has no positive duration, or adding a duration lost its boundary.
    InvalidTimeline,
    /// Loop count or rounded interior pose is not representable.
    NumericalLimit,
    Transition(TransitionError),
    Curve(CurveError),
    Route(NavigationError),
    /// Scratch contains only a prefix; the destination route is unchanged.
    IncompleteCurve(TessellationStatus),
}
impl std::fmt::Display for RailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "camera rail: {self:?}")
    }
}
impl std::error::Error for RailError {}
impl From<TransitionError> for RailError {
    fn from(e: TransitionError) -> Self {
        match e {
            TransitionError::NumericalLimit => Self::NumericalLimit,
            other => Self::Transition(other),
        }
    }
}

/// Transactionally replace an open route only after a complete curve tessellation.
///
/// Scratch is disposable work space and may change on failure. The previous
/// route is always retained on error, including its topology and capacity. Work
/// above 8191 or scratch above 4096 points is rejected before tessellation. A
/// plain `RailPath3::Route` trusts the supplied route's authored provenance;
/// use this adapter instead of installing an unchecked tessellation prefix.
/// See the executable module recipe.
pub fn tessellate_route_into(
    curve: CubicBezier3,
    route: &mut Route3,
    scratch: &mut [Vec3],
    options: TessellationOptions,
) -> Result<Tessellation, RailError> {
    if scratch.len() > MAX_ROUTE_POINTS || options.max_work > MAX_RAIL_TESSELLATION_WORK {
        return Err(RailError::Capacity);
    }
    let result = curve
        .tessellate_into(scratch, options)
        .map_err(RailError::Curve)?;
    if result.status != TessellationStatus::Complete || result.covered_until != 1. {
        return Err(RailError::IncompleteCurve(result.status));
    }
    route
        .set(&scratch[..result.written])
        .map_err(RailError::Route)?;
    Ok(result)
}

/// Immutable prepared segment. Borrowed routes cannot change during playback.
/// See module recipe; construction allocates nothing, as do subsequent samples.
#[derive(Clone, Copy)]
pub struct RailSegment3<'a> {
    path: RailPath3<'a>,
    start: RailKey3,
    end: RailKey3,
    duration: f64,
    orientation: Transition3,
}
impl<'a> RailSegment3<'a> {
    /// Prepare shortest-arc orientation, distance and FOV once via Transition3.
    /// Positive durations are finite `0 < seconds <= 3600` and require identical
    /// projection descriptors. Full path endpoints must exactly equal key eyes.
    /// Cuts require zero seconds, validate both keys and allow a different lens.
    /// All route vertices/cubic controls must lie within the transition eye bound
    /// (abs component <= 1e6); degenerate look poses are rejected, stationary
    /// paths are allowed. See module recipe for a checked construction.
    pub fn new(
        path: RailPath3<'a>,
        start: RailKey3,
        end: RailKey3,
        seconds: f64,
    ) -> Result<Self, RailError> {
        if !seconds.is_finite() || !(0. ..=3600.).contains(&seconds) {
            return Err(RailError::InvalidTime);
        }
        if matches!(path, RailPath3::Cut) != (seconds == 0.) {
            return Err(RailError::InvalidCut);
        }
        let mut orientation = Transition3::new();
        orientation.cut(start.pose, start.projection)?;
        let mut validate = Transition3::new();
        validate.cut(end.pose, end.projection)?;
        if seconds > 0. {
            orientation.retarget(end.pose, end.projection, 0.25)?;
            let valid_eye = |v: &Vec3| v.is_finite() && v.abs().max_element() <= 1e6;
            let (first, last) = match path {
                RailPath3::Cut => unreachable!(),
                RailPath3::Cubic(curve) => {
                    let controls = curve.control_points();
                    if !controls.iter().all(valid_eye) {
                        return Err(RailError::InvalidPath);
                    }
                    (controls[0], controls[3])
                }
                RailPath3::Route(route) => {
                    if route.points().is_empty() || !route.points().iter().all(valid_eye) {
                        return Err(RailError::InvalidPath);
                    }
                    (
                        route
                            .sample_progress(0.)
                            .map_err(RailError::Route)?
                            .unwrap()
                            .position,
                        route
                            .sample_progress(1.)
                            .map_err(RailError::Route)?
                            .unwrap()
                            .position,
                    )
                }
            };
            if first != start.pose.position || last != end.pose.position {
                return Err(RailError::InvalidPath);
            }
        }
        Ok(Self {
            path,
            start,
            end,
            duration: seconds,
            orientation,
        })
    }

    fn key_at(&self, fraction: f64) -> Result<RailKey3, RailError> {
        if fraction == 0. {
            return Ok(self.start);
        }
        if fraction == 1. {
            return Ok(self.end);
        }
        let position = match self.path {
            RailPath3::Cut => return Ok(self.end),
            RailPath3::Cubic(curve) => curve.position(fraction).map_err(RailError::Curve)?,
            RailPath3::Route(route) => {
                route
                    .sample_progress(fraction)
                    .map_err(RailError::Route)?
                    .unwrap()
                    .position
            }
        };
        let mut orientation = self.orientation;
        // One normalized sample of an already prepared segment, no time stepping.
        let delta = ((fraction * 0.25) as f32).min(0.25_f32.next_down());
        let base = orientation.advance(delta)?.unwrap();
        let pose = Pose3 {
            position,
            target: Vec3::from_array(std::array::from_fn(|i| {
                (f64::from(position[i]) + f64::from(base.target[i]) - f64::from(base.position[i]))
                    as f32
            })),
            ..base
        };
        let mut validate = Transition3::new();
        validate
            .cut(pose, self.start.projection)
            .map_err(|_| RailError::NumericalLimit)?;
        Ok(RailKey3 {
            pose,
            projection: self.start.projection,
        })
    }
}

/// A presentation observation; never an obstruction-tested camera frame.
/// `cycle` is an absolute loop count, not a count of events since the last call.
/// `looped` denotes an exact loop seam for raw queries, or any crossed loop for
/// a player. `cut` requests renderer history discard. See module time semantics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RailSample3 {
    pub pose: Pose3,
    pub projection: Perspective3,
    pub segment: usize,
    pub cycle: u64,
    pub looped: bool,
    pub cut: bool,
    pub finished: bool,
}

/// Bounded immutable timeline over caller-owned prepared segments.
#[derive(Clone, Copy)]
pub struct Rail3<'a> {
    segments: &'a [RailSegment3<'a>],
    mode: RailMode,
    duration: f64,
}
impl<'a> Rail3<'a> {
    /// Validate capacity, exact adjacent-key continuity and representable timing.
    /// Loops require positive total duration; a Once rail may contain only cuts.
    /// No allocation or root registration is needed beyond `camera::rail`.
    /// See module recipe.
    pub fn new(segments: &'a [RailSegment3<'a>], mode: RailMode) -> Result<Self, RailError> {
        if segments.is_empty() || segments.len() > MAX_RAIL_SEGMENTS {
            return Err(RailError::Capacity);
        }
        let mut duration = 0.;
        for (index, segment) in segments.iter().enumerate() {
            if index > 0 && segments[index - 1].end != segment.start {
                return Err(RailError::Disconnected);
            }
            let next = duration + segment.duration;
            if segment.duration > 0. && next <= duration {
                return Err(RailError::InvalidTimeline);
            }
            duration = next;
        }
        if mode == RailMode::Loop && duration == 0. {
            return Err(RailError::InvalidTimeline);
        }
        Ok(Self {
            segments,
            mode,
            duration,
        })
    }
    /// Total authored seconds, before wrapping. See module recipe.
    pub fn duration(&self) -> f64 {
        self.duration
    }

    /// Stateless absolute-time query. Seeking/backward sampling retains no state.
    /// Reduced motion selects the final key immediately, ignoring loop topology.
    /// Finite nonnegative time is required even in reduced-motion mode.
    /// See module recipe and [`RailPlayer3`] for crossed-boundary detection.
    pub fn sample_at(&self, seconds: f64, reduced_motion: bool) -> Result<RailSample3, RailError> {
        check_time(seconds)?;
        let last = self.segments.len() - 1;
        let make = |key: RailKey3, segment, cycle, looped, cut, finished| RailSample3 {
            pose: key.pose,
            projection: key.projection,
            segment,
            cycle,
            looped,
            cut,
            finished,
        };
        if reduced_motion {
            return Ok(make(self.segments[last].end, last, 0, false, true, true));
        }
        let (time, cycle) = if self.mode == RailMode::Loop {
            let mut cycles = (seconds / self.duration).floor();
            if cycles >= 9007199254740992. {
                return Err(RailError::NumericalLimit);
            }
            // Division can round a quotient just below an integer up to it.
            // Fused residual testing keeps the cycle consistent with remainder,
            // including 1.0 % 0.1, without losing the retained phase.
            if cycles.mul_add(self.duration, -seconds) > 0. {
                cycles -= 1.;
            }
            (seconds % self.duration, cycles as u64)
        } else {
            (seconds.min(self.duration), 0)
        };
        let looped = cycle > 0 && time == 0.;
        let mut cut = time == 0.;
        let mut start = 0.;
        for (index, segment) in self.segments.iter().enumerate() {
            let end = start + segment.duration;
            if segment.duration == 0. {
                cut |= time == start;
            } else if time < end {
                let key = segment.key_at(((time - start) / segment.duration).clamp(0., 1.))?;
                return Ok(make(key, index, cycle, looped, cut, false));
            }
            start = end;
        }
        Ok(make(
            self.segments[last].end,
            last,
            cycle,
            looped,
            cut,
            true,
        ))
    }

    /// Sample up to 1024 supplied times into a caller slice. Entire destination
    /// (including unused tail) is unchanged on any error, including a late numeric
    /// failure. Success writes exactly `times.len()` entries. Empty input succeeds.
    /// Validates in one pass, then writes in a second: bounded O(times * segments).
    /// No allocation; immutable paths guarantee repeatability within the call.
    ///
    /// ```
    /// use ferravex_math::{Vec3, projection::NdcDepth};
    /// use ferravex_play::camera::{Pose3, transition::*, rail::*};
    /// let key = RailKey3 { pose: Pose3::default(), projection: Perspective3 {
    ///     aspect: 1., near: 0.1, far: 100., handedness: ViewHandedness::Right,
    ///     depth: NdcDepth::ZeroToOne } };
    /// let segments = [RailSegment3::new(RailPath3::Cut, key, key, 0.)?];
    /// let rail = Rail3::new(&segments, RailMode::Once)?;
    /// let sentinel = rail.sample_at(0., false)?;
    /// let mut out = [sentinel; 3];
    /// assert!(rail.sample_into(&[0., f64::NAN], false, &mut out).is_err());
    /// assert_eq!(out, [sentinel; 3]);
    /// assert_eq!(rail.sample_into(&[0., 2.], false, &mut out)?, 2);
    /// assert_eq!(out[2], sentinel);
    /// # Ok::<(), RailError>(())
    /// ```
    pub fn sample_into(
        &self,
        times: &[f64],
        reduced_motion: bool,
        output: &mut [RailSample3],
    ) -> Result<usize, RailError> {
        if times.len() > MAX_RAIL_SAMPLES || output.len() < times.len() {
            return Err(RailError::Capacity);
        }
        for &time in times {
            self.sample_at(time, reduced_motion)?;
        }
        for (&time, slot) in times.iter().zip(output.iter_mut()) {
            *slot = self.sample_at(time, reduced_motion)?;
        }
        Ok(times.len())
    }

    fn crosses_cut(&self, from: f64, to: f64) -> bool {
        let mut boundary = 0.;
        self.segments.iter().any(|segment| {
            let crossed = segment.duration == 0. && from < boundary && boundary <= to;
            boundary += segment.duration;
            crossed
        })
    }
}
fn check_time(seconds: f64) -> Result<(), RailError> {
    if seconds.is_finite() && seconds >= 0. {
        Ok(())
    } else {
        Err(RailError::InvalidTime)
    }
}

/// Optional retained display/hold/discontinuity adapter over a fixed rail.
/// No clock, accumulator or simulation ownership. See executable module recipe.
#[derive(Clone, Copy)]
pub struct RailPlayer3<'a> {
    rail: Rail3<'a>,
    displayed: Option<RailSample3>,
    time: f64,
    held: bool,
    generation: u64,
}
impl<'a> RailPlayer3<'a> {
    /// Empty player, generation zero; the first successful sample is a cut.
    pub fn new(rail: Rail3<'a>) -> Self {
        Self {
            rail,
            displayed: None,
            time: 0.,
            held: false,
            generation: 0,
        }
    }
    /// Last displayed observation, including its original outcome flags.
    pub fn displayed(&self) -> Option<RailSample3> {
        self.displayed
    }
    /// Local wrapping discontinuity counter; not a tick ID or global identity.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Whether ordinary valid samples retain the displayed pose/time.
    pub fn is_held(&self) -> bool {
        self.held
    }
    /// Hold current display; empty holds initialize on the next query.
    pub fn hold(&mut self) -> bool {
        let changed = !self.held;
        self.held = true;
        changed
    }
    /// Release hold; host must resume from its frozen/rebased playhead.
    pub fn resume(&mut self) -> bool {
        let changed = self.held;
        self.held = false;
        changed
    }
    /// Forget display/time, release hold, increment generation. See module recipe.
    pub fn reset(&mut self) {
        self.displayed = None;
        self.time = 0.;
        self.held = false;
        self.generation = self.generation.wrapping_add(1);
    }
    /// Atomic supplied-time query. Hold discards valid supplied time; reduced
    /// motion overrides hold and cuts to final. Invalid time still errors while
    /// held. Backward seeks and crossed cuts/loops discard renderer history.
    /// Repeated identical time/policy does not increment generation again.
    /// Resuming after reduced motion cuts, preventing stale interpolation.
    /// See module recipe.
    pub fn sample_at(
        &mut self,
        seconds: f64,
        reduced_motion: bool,
    ) -> Result<RailSample3, RailError> {
        check_time(seconds)?;
        if self.held
            && !reduced_motion
            && let Some(displayed) = self.displayed
        {
            return Ok(displayed);
        }
        let mut sample = self.rail.sample_at(seconds, reduced_motion)?;
        if let Some(previous) = self.displayed {
            let changed_cycle = sample.cycle != previous.cycle;
            sample.looped |= changed_cycle;
            let (from, to) = if self.rail.mode == RailMode::Loop {
                (self.time % self.rail.duration, seconds % self.rail.duration)
            } else {
                (self.time, seconds)
            };
            sample.cut |= seconds < self.time
                || changed_cycle
                || self.rail.crosses_cut(from, to)
                || (previous.finished && self.rail.mode == RailMode::Loop)
                || (previous.finished && !sample.finished);
            if sample.cut
                && (seconds != self.time
                    || sample.pose != previous.pose
                    || sample.projection != previous.projection
                    || sample.finished != previous.finished)
            {
                self.generation = self.generation.wrapping_add(1);
            }
        } else {
            sample.cut = true;
            self.generation = self.generation.wrapping_add(1);
        }
        self.displayed = Some(sample);
        self.time = seconds;
        if reduced_motion {
            self.held = false;
        }
        Ok(sample)
    }
}
