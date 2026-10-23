use super::{
    geometry::{Hit, Queries},
    *,
};
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JumpState {
    /// Seconds remaining. Include both timers and held in prediction snapshots/replays.
    pub coyote: f32,
    pub buffer: f32,
    pub held: bool,
}
impl JumpState {
    fn valid(self) -> bool {
        self.coyote.is_finite()
            && (0. ..=0.5).contains(&self.coyote)
            && self.buffer.is_finite()
            && (0. ..=0.5).contains(&self.buffer)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Support {
    pub id: u64,
    pub local_anchor: Vec3,
    pub previous_anchor: Vec3,
}
impl Support {
    fn new(o: Obstacle, feet: Vec3) -> Self {
        Self {
            id: o.id,
            local_anchor: o.rotation.conjugate() * (feet - o.position),
            previous_anchor: feet,
        }
    }
}
/// Overlap corrections performed during one successful move, step or teleport.
/// This excludes requested motion, platform carry and ground snap. Errors still
/// return no partial motion and leave a [`Character`] unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Recovery {
    /// Sum of all separation corrections, including corrections before platform carry.
    pub translation: Vec3,
    /// Sum of correction lengths, bounded by [`CharacterSettings::max_recovery`].
    pub distance: f32,
    /// Number of applied separation corrections; clearance checks are not counted.
    pub iterations: u32,
    /// Largest penetration depth encountered before a correction (excluding skin).
    pub max_depth: f32,
}
/// Motion requested by the previous supporting obstacle during a fixed tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SupportMotion {
    pub id: u64,
    /// World-space displacement of the saved anchor, including platform rotation.
    pub requested: Vec3,
    /// Swept displacement actually applied, excluding overlap correction and snap.
    /// Only this displacement contributes platform velocity when jumping.
    pub applied: Vec3,
    /// Whether the platform carry encountered an obstacle.
    pub blocked: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub position: Vec3,
    /// Displacement from this sweep's origin. In [`Character::step`] this is the
    /// final velocity-driven sweep, after carry and preliminary overlap recovery.
    pub translation: Vec3,
    pub grounded: bool,
    pub ground_normal: Vec3,
    pub support: Option<Support>,
    pub stepped: bool,
    pub blocked: bool,
    pub candidates: usize,
    /// Corrections across the entire public operation, including platform carry
    /// and preliminary recovery in [`Character::step`].
    pub recovery: Recovery,
    /// Previous support's carry for [`Character::step`], even if the character
    /// jumped or lost support later this tick. `None` for a sweep/teleport or if
    /// the old support was absent, removed, a sensor or excluded by the mask.
    /// [`Self::support`] instead describes walkable support at the final pose.
    pub support_motion: Option<SupportMotion>,
    normals: [Vec3; 16],
    normal_count: usize,
}
fn slide(mut value: Vec3, normal: Vec3, min_ground: f32) -> Vec3 {
    let into = value.dot(normal);
    if into < 0. {
        value -= normal * into;
    }
    if normal.y > 0. && normal.y < min_ground && value.y > 0. {
        value.y = 0.;
        let horizontal = Vec3::new(normal.x, 0., normal.z).normalize_or_zero();
        let into = value.dot(horizontal);
        if into < 0. {
            value -= horizontal * into;
        }
    }
    value
}
fn ground<T: Terrain + ?Sized>(
    queries: &mut Queries<'_, T>,
    p: Vec3,
    distance: f32,
) -> Result<Option<Hit>, CharacterError> {
    Ok(queries
        .cast(p, -Vec3::Y * distance)?
        .filter(|h| h.normal.y >= queries.settings.min_ground_normal))
}
fn try_step<T: Terrain + ?Sized>(
    q: &mut Queries<'_, T>,
    p: Vec3,
    remaining: Vec3,
) -> Result<Option<(Vec3, Hit)>, CharacterError> {
    let up = Vec3::Y * q.settings.step_height;
    let across = Vec3::new(remaining.x, 0., remaining.z);
    if across.length_squared() < 1e-12 || q.cast(p, up)?.is_some() {
        return Ok(None);
    }
    let raised = p + up;
    if q.cast(raised, across)?.is_some() {
        return Ok(None);
    }
    let destination = raised + across;
    let distance = q.settings.step_height + q.settings.ground_snap;
    let Some(mut hit) = q.cast(destination, -Vec3::Y * distance)? else {
        return Ok(None);
    };
    if hit.normal.y < q.settings.min_ground_normal {
        // A capsule meets a short stair's rounded Minkowski edge before its center
        // reaches the tread. Classify that authored tread, not the round edge, while
        // still sweeping the full capsule and advancing only the requested XY/XZ move.
        let ObstacleShape::Box { half } = hit.obstacle.shape else {
            return Ok(None);
        };
        let top = hit.obstacle.rotation * Vec3::Y;
        if top.y < q.settings.min_ground_normal || hit.normal.y <= 0. {
            return Ok(None);
        }
        let plane = hit.obstacle.position + top * half.y;
        let height = (plane.dot(top) - destination.x * top.x - destination.z * top.z) / top.y;
        if height - p.y > q.settings.step_height + q.settings.skin || height < p.y - q.settings.skin
        {
            return Ok(None);
        }
        hit.normal = top;
    }
    Ok(Some((destination - Vec3::Y * distance * hit.fraction, hit)))
}
fn move_inner<T: Terrain + ?Sized>(
    q: &mut Queries<'_, T>,
    origin: Vec3,
    delta: Vec3,
    was_grounded: bool,
    snap: bool,
) -> Result<Motion, CharacterError> {
    let mut p = q.recover(origin)?;
    let mut remaining = delta;
    let mut result = Motion {
        position: p,
        translation: Vec3::ZERO,
        grounded: false,
        ground_normal: Vec3::Y,
        support: None,
        stepped: false,
        blocked: false,
        candidates: 0,
        recovery: Recovery::default(),
        support_motion: None,
        normals: [Vec3::ZERO; 16],
        normal_count: 0,
    };
    let mut stair_support = None;
    for _ in 0..q.settings.max_iterations {
        if remaining.length_squared() < 1e-12 {
            break;
        }
        let Some(hit) = q.cast(p, remaining)? else {
            p += remaining;
            break;
        };
        p += remaining * hit.fraction;
        remaining *= 1. - hit.fraction;
        result.blocked = true;
        if was_grounded
            && delta.y <= 0.
            && hit.normal.y < q.settings.min_ground_normal
            && q.settings.step_height > 0.
            && let Some((stepped, landing)) = try_step(q, p, remaining)?
        {
            p = stepped;
            stair_support = Some(landing);
            result.stepped = true;
            break;
        }
        if result.normal_count < result.normals.len() {
            result.normals[result.normal_count] = hit.normal;
            result.normal_count += 1;
        }
        remaining = slide(remaining, hit.normal, q.settings.min_ground_normal);
    }
    let support = if snap && delta.y <= 0. {
        ground(q, p, q.settings.ground_snap + q.settings.skin)?
    } else {
        None
    };
    if let Some(hit) = support {
        p -= Vec3::Y * (q.settings.ground_snap + q.settings.skin) * hit.fraction;
    }
    if let Some(hit) = support.or(stair_support) {
        result.grounded = true;
        result.ground_normal = hit.normal;
        result.support = Some(Support::new(hit.obstacle, p));
    }
    if !bounded(p) {
        return Err(CharacterError::InvalidInput);
    }
    result.position = p;
    result.translation = p - origin;
    result.candidates = q.candidates;
    result.recovery = q.recovery;
    Ok(result)
}
/// Sweep/slide without applying acceleration, gravity or jump state. Displacement
/// may be long; no destination-only collision tests or discrete movement substeps.
/// A bounded starting-overlap recovery is attempted; errors leave caller state intact.
pub fn move_and_slide<T: Terrain + ?Sized>(
    world: &T,
    shape: CharacterShape,
    settings: CharacterSettings,
    position: Vec3,
    displacement: Vec3,
    was_grounded: bool,
) -> Result<Motion, CharacterError> {
    if !shape.valid()
        || !settings.valid()
        || !bounded(position)
        || !bounded(displacement)
        || (shape.planar() && (position.z != 0. || displacement.z != 0.))
    {
        return Err(CharacterError::InvalidInput);
    }
    let mut queries = Queries {
        world,
        shape,
        settings,
        candidates: 0,
        ignored: None,
        recovery: Recovery::default(),
    };
    move_inner(&mut queries, position, displacement, was_grounded, true)
}
#[derive(Clone, Copy, Debug, Default)]
pub struct CharacterInput {
    /// Desired horizontal direction, clamped to unit length. Y is ignored.
    pub direction: Vec3,
    /// Current held state; the motor buffers only a new press, avoiding automatic bunny hops.
    pub jump: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Character {
    pub position: Vec3,
    /// Velocity relative to a supporting platform; airborne velocity is in world space.
    pub velocity: Vec3,
    pub grounded: bool,
    pub ground_normal: Vec3,
    pub jump: JumpState,
    pub support: Option<Support>,
}
impl Character {
    pub fn new(feet: Vec3) -> Self {
        Self {
            position: feet,
            velocity: Vec3::ZERO,
            grounded: false,
            ground_normal: Vec3::Y,
            jump: Default::default(),
            support: None,
        }
    }
    /// Change the caller-owned shape's full sole-to-top height at the current
    /// feet position. Capsule radius or Box2 half-width stays exactly unchanged;
    /// Box2 stores half of `height` in `half.y`. Shape limits still apply.
    ///
    /// Queries the target shape even for shrinking or an unchanged height. Only
    /// a successful complete clearance query commits `shape`; every error leaves
    /// both shape and motor unchanged. The motor is borrowed immutably: velocity,
    /// jump timers/held state, grounded state, normal and support anchors all stay
    /// intact. No input edge, tick, recovery, carry or snap is performed. Terrain
    /// is tested at its current pose; pending platform carry remains for `step`.
    ///
    /// Pass the accepted shape to later steps. Use the movement settings' mask
    /// and candidate limit for consistent filtering. This operation permits a
    /// zero candidate limit as documented by [`check_clearance`].
    ///
    /// This runnable slice-Terrain recipe keeps support across crouching and a
    /// rejected stand, then stands in clear space and receives platform carry:
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_physics::character::{Character, CharacterSettings, CharacterShape,
    ///     ClearanceError, Obstacle};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let settings = CharacterSettings::default();
    /// let mut shape = CharacterShape::Capsule { radius: 0.3, height: 1.8 };
    /// let mut platform = Obstacle::cuboid(7, -Vec3::Y * 0.1, Vec3::new(4., 0.1, 4.));
    /// let mut motor = Character::new(Vec3::Y * settings.skin);
    /// motor.teleport(&[platform][..], shape, settings, motor.position)?;
    /// let saved = motor;
    /// motor.try_set_height(&[platform][..], &mut shape, 1.,
    ///                      settings.membership_mask, settings.max_candidates)?;
    /// let ceiling = Obstacle::cuboid(9, Vec3::Y * 1.5, Vec3::new(2., 0.1, 2.));
    /// assert_eq!(motor.try_set_height(&[platform, ceiling][..], &mut shape, 1.8,
    ///                                settings.membership_mask, settings.max_candidates),
    ///            Err(ClearanceError::Blocked { obstacle: 9 }));
    /// assert_eq!(shape, CharacterShape::Capsule { radius: 0.3, height: 1. });
    /// assert_eq!(motor, saved);
    /// motor.try_set_height(&[platform][..], &mut shape, 1.8,
    ///                      settings.membership_mask, settings.max_candidates)?;
    /// platform.position.x += 0.1;
    /// let tick = motor.step(&[platform][..], shape, settings, Default::default(), 1. / 120.)?;
    /// assert!((tick.carried.x - 0.1).abs() < 1e-5);
    /// # Ok(())
    /// # }
    /// ```
    pub fn try_set_height<T: Terrain + ?Sized>(
        &self,
        world: &T,
        shape: &mut CharacterShape,
        height: f32,
        membership_mask: u32,
        max_candidates: usize,
    ) -> Result<Clearance, ClearanceError> {
        if !shape.valid()
            || !bounded(self.velocity)
            || !self.ground_normal.is_finite()
            || !self.jump.valid()
            || (shape.planar() && self.velocity.z != 0.)
            || self
                .support
                .is_some_and(|s| !bounded(s.local_anchor) || !bounded(s.previous_anchor))
        {
            return Err(ClearanceError::InvalidInput);
        }
        let next = match *shape {
            CharacterShape::Capsule { radius, .. } => CharacterShape::Capsule { radius, height },
            CharacterShape::Box2 { half } => CharacterShape::Box2 {
                half: Vec2::new(half.x, height * 0.5),
            },
        };
        let clearance =
            check_clearance(world, next, self.position, membership_mask, max_candidates)?;
        *shape = next;
        Ok(clearance)
    }

    /// Reset to checked feet coordinates without querying terrain. Clears velocity,
    /// support, grounded state and all jump state. Invalid coordinates or shape
    /// leave `self` unchanged. Use [`Self::teleport`] to resolve overlaps immediately.
    ///
    /// Hosts must reset both previous/current interpolation poses to `self.position`
    /// after success, and discard pending gameplay edges on a restart. This method
    /// does not advance time or consume an input edge.
    ///
    /// ```
    /// use ferravex_math::{Vec2, Vec3};
    /// use ferravex_physics::character::{Character, CharacterShape, JumpState};
    /// let shape = CharacterShape::Box2 { half: Vec2::new(0.3, 0.8) };
    /// let mut motor = Character::new(Vec3::ZERO);
    /// motor.velocity = Vec3::X * 4.;
    /// motor.reset(shape, Vec3::Y * 2.)?;
    /// let (previous, current) = (motor.position, motor.position);
    /// assert_eq!(previous, current);
    /// assert_eq!(motor.velocity, Vec3::ZERO);
    /// assert_eq!(motor.jump, JumpState::default());
    /// # Ok::<(), ferravex_physics::character::CharacterError>(())
    /// ```
    pub fn reset(&mut self, shape: CharacterShape, feet: Vec3) -> Result<(), CharacterError> {
        if !shape.valid() || !bounded(feet) || (shape.planar() && feet.z != 0.) {
            return Err(CharacterError::InvalidInput);
        }
        *self = Self::new(feet);
        Ok(())
    }
    /// Reset at a destination, recover initial overlaps and probe/snap to walkable
    /// ground using current terrain poses. This does not sweep from the old pose.
    /// Velocity and jump state are cleared; support is acquired at the destination
    /// with fresh anchors. Every error leaves all previous state intact.
    ///
    /// The returned translation is relative to `feet`, not the old position. Set
    /// both host interpolation poses to the **resolved** `self.position` after
    /// success. Reset camera history and pending gameplay edges as appropriate for
    /// the host's restart; no fixed tick is consumed here.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_physics::character::{Character, CharacterShape, Obstacle};
    /// let shape = CharacterShape::Capsule { radius: 0.3, height: 1.8 };
    /// let floor = Obstacle::cuboid(7, -Vec3::Y * 0.5, Vec3::new(4., 0.5, 4.));
    /// let mut motor = Character::new(Vec3::Y * 10.);
    /// let report = motor.teleport(&[floor][..], shape, Default::default(), -Vec3::Y * 0.1)?;
    /// assert!(report.recovery.distance > 0.1);
    /// assert!(report.grounded);
    /// assert_eq!(report.support.unwrap().id, 7);
    /// let (previous, current) = (motor.position, motor.position);
    /// assert_eq!(previous, current);
    /// # Ok::<(), ferravex_physics::character::CharacterError>(())
    /// ```
    pub fn teleport<T: Terrain + ?Sized>(
        &mut self,
        world: &T,
        shape: CharacterShape,
        settings: CharacterSettings,
        feet: Vec3,
    ) -> Result<Motion, CharacterError> {
        let motion = move_and_slide(world, shape, settings, feet, Vec3::ZERO, false)?;
        *self = Self {
            grounded: motion.grounded,
            ground_normal: motion.ground_normal,
            support: motion.support,
            ..Self::new(motion.position)
        };
        Ok(motion)
    }
    /// Advance at a fixed dt in (0, 0.1]. Terrain must already have this tick's pose.
    /// All state updates commit together; invalid input or exhausted queries leave self unchanged.
    /// The returned recovery/candidate totals cover the whole tick. Carry is
    /// reported separately from the final support, so jumping or walking off a
    /// platform does not hide its contribution earlier in the tick.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_physics::character::{Character, CharacterShape, Obstacle};
    /// let shape = CharacterShape::Capsule { radius: 0.3, height: 1.8 };
    /// let mut platform = Obstacle::cuboid(7, -Vec3::Y * 0.1, Vec3::new(4., 0.1, 4.));
    /// let mut motor = Character::new(Vec3::ZERO);
    /// motor.teleport(&[platform][..], shape, Default::default(), Vec3::Y * 0.002)?;
    /// platform.position.x += 0.1; // Move terrain before advancing the motor.
    /// let tick = motor.step(&[platform][..], shape, Default::default(), Default::default(), 1. / 120.)?;
    /// let carry = tick.motion.support_motion.unwrap();
    /// assert_eq!(carry.id, 7);
    /// assert!((carry.applied.x - 0.1).abs() < 1e-5);
    /// assert!(!carry.blocked);
    /// assert_eq!(tick.carried, carry.applied);
    /// # Ok::<(), ferravex_physics::character::CharacterError>(())
    /// ```
    pub fn step<T: Terrain + ?Sized>(
        &mut self,
        world: &T,
        shape: CharacterShape,
        settings: CharacterSettings,
        input: CharacterInput,
        dt: f32,
    ) -> Result<CharacterStep, CharacterError> {
        if !dt.is_finite()
            || !(0. ..=0.1).contains(&dt)
            || dt == 0.
            || !settings.valid()
            || !shape.valid()
            || !bounded(self.position)
            || !bounded(self.velocity)
            || !self.ground_normal.is_finite()
            || !input.direction.is_finite()
            || !self.jump.valid()
            || (shape.planar() && (self.position.z != 0. || self.velocity.z != 0.))
            || self
                .support
                .is_some_and(|s| !bounded(s.local_anchor) || !bounded(s.previous_anchor))
        {
            return Err(CharacterError::InvalidInput);
        }
        let mut next = *self;
        let mut q = Queries {
            world,
            shape,
            settings,
            candidates: 0,
            ignored: None,
            recovery: Recovery::default(),
        };
        let mut carry = Vec3::ZERO;
        let mut support_motion = None;
        if let Some(support) = next.support
            && let Some(o) = world.obstacle(support.id)
        {
            if o.id != support.id || !o.valid() {
                return Err(CharacterError::InvalidObstacle);
            }
            if !o.sensor && o.membership & settings.membership_mask != 0 {
                // Validate planar support before ignoring it in the carry sweep.
                geometry::separation(shape, next.position, o)?;
                let delta =
                    o.position + o.rotation * support.local_anchor - support.previous_anchor;
                if !bounded(delta) || (shape.planar() && delta.z != 0.) {
                    return Err(CharacterError::InvalidInput);
                }
                q.ignored = Some(o.id);
                let recovery_before = q.recovery.translation;
                let motion = move_inner(&mut q, next.position, delta, false, false)?;
                q.ignored = None;
                next.position = motion.position;
                let recovered = q.recovery.translation - recovery_before;
                let applied = motion.translation - recovered;
                carry = applied / dt;
                support_motion = Some(SupportMotion {
                    id: o.id,
                    requested: delta,
                    applied,
                    blocked: motion.blocked,
                });
            }
        }
        next.position = q.recover(next.position)?;
        let on_ground = next.velocity.y <= 0.
            && ground(&mut q, next.position, settings.ground_snap + settings.skin)?.is_some();
        let was_grounded = next.grounded || on_ground;
        next.jump.coyote = if was_grounded {
            settings.coyote_time
        } else {
            (next.jump.coyote - dt).max(0.)
        };
        next.jump.buffer = if input.jump && !next.jump.held {
            settings.jump_buffer.max(dt)
        } else {
            (next.jump.buffer - dt).max(0.)
        };
        next.jump.held = input.jump;
        let jumped = next.jump.buffer > 0. && (was_grounded || next.jump.coyote > 0.);
        let wish = Vec3::new(
            input.direction.x,
            0.,
            if shape.planar() {
                0.
            } else {
                input.direction.z
            },
        )
        .clamp_length_max(1.)
            * settings.speed;
        let horizontal = Vec3::new(next.velocity.x, 0., next.velocity.z);
        let acceleration = if was_grounded {
            settings.ground_acceleration
        } else {
            settings.air_acceleration
        };
        next.velocity += (wish - horizontal).clamp_length_max(acceleration * dt);
        if jumped {
            next.velocity.y = settings.jump_speed;
            next.velocity += carry;
            next.jump.buffer = 0.;
            next.jump.coyote = 0.;
            next.grounded = false;
        }
        next.velocity.y = (next.velocity.y - settings.gravity * dt).max(-settings.terminal_speed);
        let mut motion = move_inner(
            &mut q,
            next.position,
            next.velocity * dt,
            was_grounded && !jumped,
            true,
        )?;
        motion.support_motion = support_motion;
        next.position = motion.position;
        next.grounded = motion.grounded;
        next.ground_normal = motion.ground_normal;
        next.support = motion.support;
        for normal in &motion.normals[..motion.normal_count] {
            next.velocity = slide(next.velocity, *normal, settings.min_ground_normal);
        }
        if next.grounded {
            next.velocity.y = 0.;
        }
        if !bounded(next.velocity) {
            return Err(CharacterError::InvalidInput);
        }
        *self = next;
        Ok(CharacterStep {
            motion,
            jumped,
            carried: support_motion.map_or(Vec3::ZERO, |support| support.applied),
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CharacterStep {
    pub motion: Motion,
    pub jumped: bool,
    /// Applied platform carry, excluding initial-overlap correction and ground snap.
    /// See [`Motion::support_motion`] for the requested displacement and blockage.
    pub carried: Vec3,
}
