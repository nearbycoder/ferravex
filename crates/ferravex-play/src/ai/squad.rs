//! Last-known radio contacts and exclusive, expiring cover leases. The host owns
//! faction membership, sensing, reachability and weapon authority.
use super::{AiError, Candidate, VisibilityQuery, WorkBudget};
use ferravex_math::Vec3;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SquadRole {
    Assault,
    FlankLeft,
    FlankRight,
    Support,
}
impl SquadRole {
    pub fn for_member(id: u64) -> Self {
        match id % 4 {
            0 => Self::Support,
            1 => Self::Assault,
            2 => Self::FlankLeft,
            _ => Self::FlankRight,
        }
    }
    /// A suggested destination, never permission to move through world geometry.
    pub fn goal(self, target: Vec3, from: Vec3, up: Vec3, radius: f32) -> Result<Vec3, AiError> {
        if !target.is_finite()
            || !from.is_finite()
            || !up.is_finite()
            || up.length_squared() < 0.5
            || !radius.is_finite()
            || !(0. ..=100.).contains(&radius)
        {
            return Err(AiError::InvalidInput);
        }
        let side = up
            .normalize_or_zero()
            .cross(target - from)
            .normalize_or_zero()
            * radius;
        Ok(match self {
            Self::FlankLeft => target + side,
            Self::FlankRight => target - side,
            _ => target,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct RadioContact {
    pub candidate: Candidate,
    pub revision: u64,
    pub age: f32,
}
#[derive(Clone, Copy, Debug)]
struct Lease {
    owner: u64,
    seconds: f32,
}
#[derive(Clone, Debug)]
pub struct SquadBoard<const C: usize> {
    cover: [Vec3; C],
    leases: [Option<Lease>; C],
    contact: Option<RadioContact>,
    revision: u64,
}
impl<const C: usize> SquadBoard<C> {
    pub fn new(cover: [Vec3; C]) -> Result<Self, AiError> {
        if C > 64 || cover.iter().any(|p| !p.is_finite()) {
            return Err(AiError::InvalidConfiguration);
        }
        Ok(Self {
            cover,
            leases: [None; C],
            contact: None,
            revision: 0,
        })
    }
    /// Call only for a validated fresh observation, never for a radio echo.
    pub fn report(&mut self, candidate: Candidate) -> Result<(), AiError> {
        if !candidate.position.is_finite() {
            return Err(AiError::InvalidInput);
        }
        self.revision = self.revision.checked_add(1).ok_or(AiError::InvalidState)?;
        self.contact = Some(RadioContact {
            candidate,
            revision: self.revision,
            age: 0.,
        });
        Ok(())
    }
    pub fn contact(&self) -> Option<RadioContact> {
        self.contact
    }
    pub fn forget(&mut self) {
        self.contact = None;
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), AiError> {
        if !dt.is_finite() || !(0. ..=60.).contains(&dt) {
            return Err(AiError::InvalidInput);
        }
        if let Some(c) = &mut self.contact {
            c.age += dt;
            if c.age >= 3. {
                self.contact = None;
            }
        }
        for l in &mut self.leases {
            if let Some(v) = l {
                v.seconds -= dt;
                if v.seconds <= 0. {
                    *l = None;
                }
            }
        }
        Ok(())
    }
    pub fn release(&mut self, owner: u64) {
        for l in &mut self.leases {
            if l.is_some_and(|v| v.owner == owner) {
                *l = None;
            }
        }
    }
    /// Tests at most C candidates and one occlusion query per eligible point.
    /// `eligible` performs cheap host checks; a successful path is still required.
    pub fn reserve(
        &mut self,
        owner: u64,
        from: Vec3,
        threat: Vec3,
        budget: &mut WorkBudget,
        query: &mut impl VisibilityQuery,
        mut eligible: impl FnMut(Vec3) -> bool,
    ) -> Result<Option<Vec3>, AiError> {
        if owner == 0 || !from.is_finite() || !threat.is_finite() {
            return Err(AiError::InvalidInput);
        }
        let mut best = None;
        let mut score = f32::INFINITY;
        for (i, p) in self.cover.iter().copied().enumerate() {
            if !budget.candidate() {
                break;
            }
            if self.cover[..i].contains(&p) {
                continue;
            }
            if self.leases[i].is_some_and(|l| l.owner != owner) || !eligible(p) {
                continue;
            }
            if !budget.query() {
                break;
            }
            if query.clear(p, threat) {
                continue;
            }
            let cost = p.distance(from);
            if cost < score {
                score = cost;
                best = Some(i);
            }
        }
        if let Some(i) = best {
            self.release(owner);
            self.leases[i] = Some(Lease {
                owner,
                seconds: 1.5,
            });
            Ok(Some(self.cover[i]))
        } else {
            Ok(None)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Wall;
    impl VisibilityQuery for Wall {
        fn clear(&mut self, _: Vec3, _: Vec3) -> bool {
            false
        }
    }
    #[test]
    fn leases_are_exclusive_expire_and_respect_budget() {
        let mut board = SquadBoard::new([Vec3::X, Vec3::Z]).unwrap();
        let mut b = WorkBudget::new(9, 9, 0);
        assert_eq!(
            board
                .reserve(1, Vec3::ZERO, Vec3::Y, &mut b, &mut Wall, |_| true)
                .unwrap(),
            Some(Vec3::X)
        );
        assert_eq!(
            board
                .reserve(2, Vec3::ZERO, Vec3::Y, &mut b, &mut Wall, |_| true)
                .unwrap(),
            Some(Vec3::Z)
        );
        assert_eq!(
            board
                .reserve(3, Vec3::ZERO, Vec3::Y, &mut b, &mut Wall, |_| true)
                .unwrap(),
            None
        );
        board.advance(1.5).unwrap();
        assert!(
            board
                .reserve(
                    3,
                    Vec3::ZERO,
                    Vec3::Y,
                    &mut WorkBudget::new(0, 0, 0),
                    &mut Wall,
                    |_| true
                )
                .unwrap()
                .is_none()
        );
        board.release(2);
        assert!(
            board
                .reserve(3, Vec3::ZERO, Vec3::Y, &mut b, &mut Wall, |_| true)
                .unwrap()
                .is_some()
        );
    }
    #[test]
    fn radio_is_last_known_and_flanks_differ() {
        let mut b = SquadBoard::<0>::new([]).unwrap();
        b.report(Candidate {
            id: 7,
            position: Vec3::Z,
            groups: 1,
        })
        .unwrap();
        let rev = b.contact().unwrap().revision;
        b.advance(1.).unwrap();
        assert_eq!(b.contact().unwrap().revision, rev);
        assert_eq!(b.contact().unwrap().age, 1.);
        b.advance(2.).unwrap();
        assert!(b.contact().is_none());
        assert_ne!(
            SquadRole::FlankLeft
                .goal(Vec3::Z, Vec3::ZERO, Vec3::Y, 3.)
                .unwrap(),
            SquadRole::FlankRight
                .goal(Vec3::Z, Vec3::ZERO, Vec3::Y, 3.)
                .unwrap()
        );
    }
}
