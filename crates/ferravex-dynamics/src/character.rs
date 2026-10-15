use crate::{math::*, *};

/// A compact kinematic sphere controller, with positive Y as its up axis.
/// The caller supplies displacement (including gravity) and owns velocity.
#[derive(Clone, Copy, Debug)]
pub struct CharacterSettings {
    pub radius: f32,
    pub skin: f32,
    pub ground_snap: f32,
    pub step_height: f32,
    /// A surface is walkable if its upward normal component meets this value.
    pub min_ground_normal: f32,
    pub max_iterations: u32,
}
impl Default for CharacterSettings {
    fn default() -> Self {
        Self {
            radius: 0.4,
            skin: 0.002,
            ground_snap: 0.08,
            step_height: 0.25,
            min_ground_normal: 0.7,
            max_iterations: 5,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterMotion<const D: usize> {
    pub position: [f32; D],
    pub translation: [f32; D],
    pub grounded: bool,
    pub stepped: bool,
    pub blocked: bool,
}
impl<const D: usize> PhysicsWorld<D> {
    /// Sweep/slide a sphere through the queried world, with low-step climbing and
    /// downward ground snap. This does not insert/move a body or apply impulses.
    /// Exclude the character's body with the filter if it is also in this world.
    /// Slopes use collider normals; currently terrain is spheres and AABBs only.
    /// Starting penetrations are not repaired: spawn outside solid colliders.
    pub fn move_character(
        &self,
        origin: [f32; D],
        displacement: [f32; D],
        settings: CharacterSettings,
        filter: QueryFilter,
    ) -> Result<CharacterMotion<D>, PhysicsError> {
        if !valid(origin) || !valid(displacement) {
            return Err(PhysicsError::InvalidBody);
        }
        if !Shape::<D>::ball(settings.radius).valid()
            || !settings.skin.is_finite()
            || !(1e-5..=0.1).contains(&settings.skin)
            || !settings.ground_snap.is_finite()
            || !(0. ..=10.).contains(&settings.ground_snap)
            || !settings.step_height.is_finite()
            || !(0. ..=10.).contains(&settings.step_height)
            || !settings.min_ground_normal.is_finite()
            || !(0.01..=1.).contains(&settings.min_ground_normal)
            || !(1..=16).contains(&settings.max_iterations)
        {
            return Err(PhysicsError::InvalidConfiguration);
        }
        let down = unit(1, -1.);
        let up = unit(1, 1.);
        let mut position = origin;
        let grounded_at_start = self
            .cast_ball(
                origin,
                settings.radius,
                down,
                settings.ground_snap + settings.skin,
                filter,
            )
            .is_some_and(|hit| hit.normal[1] >= settings.min_ground_normal);
        let mut grounded = false;
        let mut stepped = false;
        let mut blocked = false;
        let mut remaining = displacement;
        for _ in 0..settings.max_iterations {
            let distance = length(remaining);
            if distance < 1e-7 {
                break;
            }
            let direction = mul(remaining, distance.recip());
            let Some(hit) = self.cast_ball(
                position,
                settings.radius,
                direction,
                distance + settings.skin,
                filter,
            ) else {
                position = add(position, remaining);
                break;
            };
            // Ignore a surface beyond the requested move but inside the skin.
            if hit.distance > distance + settings.skin * 0.5 {
                position = add(position, remaining);
                break;
            }
            let advance = (hit.distance - settings.skin).max(0.).min(distance);
            position = add(position, mul(direction, advance));
            remaining = mul(remaining, (distance - advance) / distance);
            grounded |= hit.normal[1] >= settings.min_ground_normal;
            blocked = true;
            if grounded_at_start
                && displacement[1] <= 0.
                && settings.step_height > 0.
                && hit.normal[1] < settings.min_ground_normal
            {
                let horizontal = std::array::from_fn(|i| if i == 1 { 0. } else { remaining[i] });
                let distance = length(horizontal);
                if distance > 1e-6
                    && self
                        .cast_ball(
                            position,
                            settings.radius,
                            up,
                            settings.step_height + settings.skin,
                            filter,
                        )
                        .is_none()
                {
                    let raised = add(position, mul(up, settings.step_height));
                    if self
                        .cast_ball(
                            raised,
                            settings.radius,
                            mul(horizontal, distance.recip()),
                            distance + settings.skin,
                            filter,
                        )
                        .is_none()
                    {
                        let across = add(raised, horizontal);
                        if let Some(landing) = self.cast_ball(
                            across,
                            settings.radius,
                            down,
                            settings.step_height + settings.ground_snap,
                            filter,
                        ) && landing.normal[1] >= settings.min_ground_normal
                        {
                            position = add(
                                across,
                                mul(down, (landing.distance - settings.skin).max(0.)),
                            );
                            grounded = true;
                            stepped = true;
                            break;
                        }
                    }
                }
            }
            let mut normal = hit.normal;
            if normal[1] > 0. && normal[1] < settings.min_ground_normal {
                // Steep faces block horizontal intent without turning it uphill.
                normal[1] = 0.;
                normal = normalize(normal).unwrap_or(hit.normal);
            }
            let into = dot(remaining, normal);
            if into < 0. {
                remaining = sub(remaining, mul(normal, into));
            }
            if advance < 1e-7 && into >= -1e-7 {
                break;
            }
        }
        if displacement[1] <= 0.
            && let Some(ground) = self.cast_ball(
                position,
                settings.radius,
                down,
                settings.ground_snap + settings.skin,
                filter,
            )
            && ground.normal[1] >= settings.min_ground_normal
        {
            position = add(
                position,
                mul(down, (ground.distance - settings.skin).max(0.)),
            );
            grounded = true;
        }
        if !valid(position) {
            return Err(PhysicsError::NumericalLimit);
        }
        Ok(CharacterMotion {
            position,
            translation: sub(position, origin),
            grounded,
            stepped,
            blocked,
        })
    }
}
