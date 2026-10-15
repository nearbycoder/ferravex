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
#[derive(Clone, Copy, Debug)]
pub struct Motion {
    pub position: Vec3,
    pub translation: Vec3,
    pub grounded: bool,
    pub ground_normal: Vec3,
    pub support: Option<Support>,
    pub stepped: bool,
    pub blocked: bool,
    pub candidates: usize,
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
    let support = if delta.y <= 0. {
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
    };
    move_inner(&mut queries, position, displacement, was_grounded)
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
    /// Advance at a fixed dt in (0, 0.1]. Terrain must already have this tick's pose.
    /// All state updates commit together; invalid input or exhausted queries leave self unchanged.
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
        };
        let mut carry = Vec3::ZERO;
        if let Some(support) = next.support
            && let Some(o) = world.obstacle(support.id)
        {
            if !o.valid() {
                return Err(CharacterError::InvalidObstacle);
            }
            if !o.sensor && o.membership & settings.membership_mask != 0 {
                let delta =
                    o.position + o.rotation * support.local_anchor - support.previous_anchor;
                if !bounded(delta) {
                    return Err(CharacterError::InvalidInput);
                }
                q.ignored = Some(o.id);
                let motion = move_inner(&mut q, next.position, delta, false)?;
                q.ignored = None;
                next.position = motion.position;
                carry = motion.translation / dt;
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
        let motion = move_inner(
            &mut q,
            next.position,
            next.velocity * dt,
            was_grounded && !jumped,
        )?;
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
            carried: carry * dt,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CharacterStep {
    pub motion: Motion,
    pub jumped: bool,
    pub carried: Vec3,
}
