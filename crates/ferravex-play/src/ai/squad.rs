//! Last-known radio contacts and exclusive, expiring cover leases. The host owns
//! faction membership, sensing, reachability and weapon authority.
//!
//! Inspect leases with [`SquadBoard::reservations`] and promptly release abandoned
//! cover with [`SquadBoard::retain_reservations`]. Neither operation refreshes a
//! lease; [`SquadBoard::advance`] ages leases using supplied simulation time.
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
/// A value snapshot of an active cover lease, in simulation time.
///
/// Changing this value does not change the board. It is not an ownership token
/// or a guarantee of reachability, safety or continued ownership after the board
/// changes. See [`SquadBoard::reservations`] for a compiling inspection example.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverReservation {
    /// Nonzero member ID supplied to [`SquadBoard::reserve`].
    pub owner: u64,
    /// The reserved cover point.
    pub position: Vec3,
    /// Remaining simulation seconds, greater than zero and at most 1.5.
    pub remaining_seconds: f32,
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
    /// Age contacts and leases by supplied simulation seconds.
    ///
    /// Call once per actual fixed tick, including while a local menu is open if
    /// authoritative simulation continues. Leases expire when their remaining
    /// time reaches zero; contacts expire at three seconds. Inspection and
    /// retention do not advance time. Nonfinite `dt` or values outside `0..=60`
    /// return [`AiError::InvalidInput`] without changing the board.
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
    /// Inspect active cover leases in the original cover-array order.
    ///
    /// Visits at most `C` slots without allocation, queries or timer changes.
    /// An owner has at most one lease, and duplicate cover points appear at most
    /// once. Use `find` to inspect a particular member's lease.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::ai::{CoverReservation, SquadBoard, VisibilityQuery, WorkBudget};
    /// struct Wall;
    /// impl VisibilityQuery for Wall {
    ///     fn clear(&mut self, _: Vec3, _: Vec3) -> bool { false }
    /// }
    /// let mut board = SquadBoard::new([Vec3::X]).unwrap();
    /// board.reserve(7, Vec3::ZERO, Vec3::Z,
    ///     &mut WorkBudget::new(1, 1, 0), &mut Wall, |_| true).unwrap();
    /// board.advance(0.5).unwrap();
    /// let lease: CoverReservation = board.reservations().find(|r| r.owner == 7).unwrap();
    /// assert_eq!(lease.position, Vec3::X);
    /// assert_eq!(lease.remaining_seconds, 1.0);
    /// board.advance(1.0).unwrap();
    /// assert!(board.reservations().next().is_none());
    /// ```
    pub fn reservations(&self) -> impl Iterator<Item = CoverReservation> + '_ {
        self.leases
            .iter()
            .zip(&self.cover)
            .filter_map(|(lease, &position)| {
                lease.map(|lease| CoverReservation {
                    owner: lease.owner,
                    position,
                    remaining_seconds: lease.seconds,
                })
            })
    }
    /// Immediately release leases for which `keep` returns false, returning the
    /// number released. For example, remove members that left the squad or whose
    /// reserved cover is no longer reachable.
    ///
    /// Calls `keep` once per active lease in original cover-array order, at most
    /// `C` times. This scan allocates nothing and performs no visibility queries;
    /// the host controls any work in `keep`. Retained leases keep their remaining
    /// lifetime, and radio contacts are unchanged. This does not replace calling
    /// [`Self::advance`] each simulation tick. Call before new reservations when
    /// departed members' cover should be immediately available to their peers.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::ai::{SquadBoard, VisibilityQuery, WorkBudget};
    /// struct Wall;
    /// impl VisibilityQuery for Wall {
    ///     fn clear(&mut self, _: Vec3, _: Vec3) -> bool { false }
    /// }
    /// let mut board = SquadBoard::new([Vec3::X, Vec3::Z]).unwrap();
    /// for owner in [7, 8] {
    ///     board.reserve(owner, Vec3::ZERO, Vec3::Y,
    ///         &mut WorkBudget::new(2, 2, 0), &mut Wall, |_| true).unwrap();
    /// }
    /// let active_members = [8]; // Member 7 departed without explicitly releasing.
    /// assert_eq!(board.retain_reservations(|r| active_members.contains(&r.owner)), 1);
    /// assert_eq!(board.reserve(9, Vec3::ZERO, Vec3::Y,
    ///     &mut WorkBudget::new(2, 2, 0), &mut Wall, |_| true).unwrap(), Some(Vec3::X));
    /// ```
    pub fn retain_reservations(&mut self, mut keep: impl FnMut(CoverReservation) -> bool) -> usize {
        let mut released = 0;
        for (slot, &position) in self.leases.iter_mut().zip(&self.cover) {
            if slot.is_some_and(|lease| {
                !keep(CoverReservation {
                    owner: lease.owner,
                    position,
                    remaining_seconds: lease.seconds,
                })
            }) {
                *slot = None;
                released += 1;
            }
        }
        released
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
    /// Equal distances prefer the first cover-array entry; duplicate points are
    /// considered only at their first entry. Success replaces the owner's old
    /// lease with a 1.5-second lease. Failure leaves any existing lease unchanged,
    /// including its remaining lifetime.
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
    fn reserve_at<const C: usize>(
        board: &mut SquadBoard<C>,
        owner: u64,
        from: Vec3,
    ) -> Option<Vec3> {
        board
            .reserve(
                owner,
                from,
                Vec3::Y,
                &mut WorkBudget::new(64, 64, 0),
                &mut Wall,
                |_| true,
            )
            .unwrap()
    }
    #[test]
    fn snapshots_preserve_cover_order_and_duplicate_exclusivity() {
        let mut board = SquadBoard::new([Vec3::Z, Vec3::X, Vec3::Z]).unwrap();
        assert!(board.reservations().next().is_none());
        assert_eq!(reserve_at(&mut board, 9, Vec3::ZERO), Some(Vec3::Z));
        assert_eq!(reserve_at(&mut board, 2, Vec3::ZERO), Some(Vec3::X));
        assert_eq!(reserve_at(&mut board, 3, Vec3::ZERO), None);
        board.advance(0.5).unwrap();
        let expected = [
            CoverReservation {
                owner: 9,
                position: Vec3::Z,
                remaining_seconds: 1.,
            },
            CoverReservation {
                owner: 2,
                position: Vec3::X,
                remaining_seconds: 1.,
            },
        ];
        assert!(board.reservations().eq(expected));
        let mut snapshot = board.reservations().next().unwrap();
        snapshot.owner = 3;
        snapshot.remaining_seconds = 60.;
        assert_ne!(snapshot, board.reservations().next().unwrap());
        assert!(board.reservations().eq(expected));
        board.advance(1.).unwrap();
        assert!(board.reservations().next().is_none());
        assert_eq!(reserve_at(&mut board, 3, Vec3::ZERO), Some(Vec3::Z));
    }
    #[test]
    fn retention_releases_abandoned_cover_without_refreshing_survivors_or_radio() {
        let mut board = SquadBoard::new([Vec3::X, Vec3::Z, -Vec3::X]).unwrap();
        board
            .report(Candidate {
                id: 42,
                position: Vec3::Y,
                groups: 1,
            })
            .unwrap();
        for owner in [9, 7, 8] {
            assert!(reserve_at(&mut board, owner, Vec3::ZERO).is_some());
        }
        board.advance(0.5).unwrap();
        let contact = board.contact().unwrap();
        let before: Vec<_> = board.reservations().collect();
        let mut visited = Vec::new();
        assert_eq!(
            board.retain_reservations(|r| {
                visited.push(r);
                // Member 9 departed, and the third point became unreachable.
                r.owner != 9 && r.position != -Vec3::X
            }),
            2
        );
        assert_eq!(visited, before);
        assert!(board.reservations().eq([before[1]]));
        let after = board.contact().unwrap();
        assert_eq!(after.candidate.id, contact.candidate.id);
        assert_eq!(after.candidate.position, contact.candidate.position);
        assert_eq!(after.candidate.groups, contact.candidate.groups);
        assert_eq!(after.revision, contact.revision);
        assert_eq!(after.age, contact.age);
        assert_eq!(reserve_at(&mut board, 1, Vec3::ZERO), Some(Vec3::X));
        board.advance(1.).unwrap();
        assert!(board.reservations().eq([CoverReservation {
            owner: 1,
            position: Vec3::X,
            remaining_seconds: 0.5,
        }]));
    }
    #[test]
    fn retained_cover_still_expires_on_the_exact_simulation_boundary() {
        let mut board = SquadBoard::new([Vec3::X]).unwrap();
        reserve_at(&mut board, 1, Vec3::ZERO).unwrap();
        for _ in 0..5 {
            board.advance(0.25).unwrap();
            assert_eq!(board.retain_reservations(|_| true), 0);
            assert_eq!(board.reservations().count(), 1);
        }
        assert_eq!(board.reservations().next().unwrap().remaining_seconds, 0.25);
        assert_eq!(reserve_at(&mut board, 2, Vec3::ZERO), None);
        board.advance(0.).unwrap();
        assert_eq!(board.reservations().next().unwrap().remaining_seconds, 0.25);
        board.advance(0.25).unwrap();
        assert!(board.reservations().next().is_none());
        assert_eq!(reserve_at(&mut board, 2, Vec3::ZERO), Some(Vec3::X));
    }
    #[test]
    fn failed_reservations_do_not_refresh_an_existing_lease() {
        let mut board = SquadBoard::new([Vec3::X]).unwrap();
        reserve_at(&mut board, 1, Vec3::ZERO).unwrap();
        board.advance(1.25).unwrap();
        let before = board.reservations().next().unwrap();
        for mut budget in [WorkBudget::new(0, 1, 0), WorkBudget::new(1, 0, 0)] {
            assert_eq!(
                board.reserve(1, Vec3::ZERO, Vec3::Y, &mut budget, &mut Wall, |_| true),
                Ok(None)
            );
            assert_eq!(board.reservations().next(), Some(before));
        }
        assert_eq!(
            board.reserve(
                1,
                Vec3::ZERO,
                Vec3::Y,
                &mut WorkBudget::new(1, 1, 0),
                &mut Wall,
                |_| false,
            ),
            Ok(None)
        );
        assert_eq!(
            board.reserve(
                1,
                Vec3::ZERO,
                Vec3::Y,
                &mut WorkBudget::new(1, 1, 0),
                &mut super::super::Unoccluded,
                |_| true,
            ),
            Ok(None)
        );
        for (owner, from, threat) in [
            (0, Vec3::ZERO, Vec3::Y),
            (1, Vec3::splat(f32::INFINITY), Vec3::Y),
            (1, Vec3::ZERO, Vec3::splat(f32::NAN)),
        ] {
            let mut budget = WorkBudget::new(1, 1, 0);
            assert_eq!(
                board.reserve(owner, from, threat, &mut budget, &mut Wall, |_| true),
                Err(AiError::InvalidInput)
            );
            assert_eq!(budget.counts(), Default::default());
        }
        assert_eq!(board.reservations().next(), Some(before));
        board.advance(0.25).unwrap();
        assert!(board.reservations().next().is_none());
    }
    #[test]
    fn successful_reservation_refreshes_only_the_owners_selected_point() {
        let mut board = SquadBoard::new([Vec3::X, Vec3::Z, -Vec3::X]).unwrap();
        assert_eq!(reserve_at(&mut board, 1, Vec3::ZERO), Some(Vec3::X));
        assert_eq!(reserve_at(&mut board, 2, Vec3::ZERO), Some(Vec3::Z));
        board.advance(1.).unwrap();
        assert_eq!(reserve_at(&mut board, 1, -Vec3::X), Some(-Vec3::X));
        assert!(board.reservations().eq([
            CoverReservation {
                owner: 2,
                position: Vec3::Z,
                remaining_seconds: 0.5,
            },
            CoverReservation {
                owner: 1,
                position: -Vec3::X,
                remaining_seconds: 1.5,
            },
        ]));
        assert_eq!(reserve_at(&mut board, 3, Vec3::ZERO), Some(Vec3::X));
        board.advance(0.5).unwrap();
        assert_eq!(reserve_at(&mut board, 1, -Vec3::X), Some(-Vec3::X));
        assert_eq!(
            board
                .reservations()
                .find(|r| r.owner == 1)
                .unwrap()
                .remaining_seconds,
            1.5
        );
        assert_eq!(
            board
                .reservations()
                .find(|r| r.owner == 3)
                .unwrap()
                .remaining_seconds,
            1.
        );
        assert!(board.reservations().all(|r| r.owner != 2));
        board.release(1);
        board.release(0);
        board.release(1);
        assert!(board.reservations().all(|r| r.owner == 3));
    }
    #[test]
    fn invalid_time_leaves_reservations_and_contact_unchanged() {
        let mut board = SquadBoard::new([Vec3::X]).unwrap();
        reserve_at(&mut board, u64::MAX, Vec3::ZERO).unwrap();
        board
            .report(Candidate {
                id: 7,
                position: Vec3::Z,
                groups: 1,
            })
            .unwrap();
        board.advance(0.5).unwrap();
        let before = board.reservations().next();
        let contact = board.contact().unwrap();
        for dt in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.25, 61.] {
            assert_eq!(board.advance(dt), Err(AiError::InvalidInput));
            assert_eq!(board.reservations().next(), before);
            assert_eq!(board.contact().unwrap().age, contact.age);
            assert_eq!(board.contact().unwrap().revision, contact.revision);
        }
        board.advance(60.).unwrap();
        assert!(board.reservations().next().is_none());
        assert!(board.contact().is_none());
    }
    #[test]
    fn retention_is_bounded_and_skips_empty_released_and_expired_slots() {
        let mut empty = SquadBoard::<0>::new([]).unwrap();
        assert!(empty.reservations().next().is_none());
        assert_eq!(empty.retain_reservations(|_| panic!("empty board")), 0);
        let mut board = SquadBoard::new(std::array::from_fn::<_, 64, _>(|i| {
            Vec3::X * (i as f32 + 1.)
        }))
        .unwrap();
        for owner in 1..=64 {
            assert!(reserve_at(&mut board, owner, Vec3::ZERO).is_some());
        }
        assert_eq!(board.reservations().count(), 64);
        let mut visited = 0;
        assert_eq!(
            board.retain_reservations(|r| {
                visited += 1;
                assert_eq!(r.owner, visited);
                false
            }),
            64
        );
        assert_eq!(visited, 64);
        assert!(board.reservations().next().is_none());
        assert_eq!(board.retain_reservations(|_| panic!("released board")), 0);
        reserve_at(&mut board, 1, Vec3::ZERO).unwrap();
        board.advance(1.5).unwrap();
        assert_eq!(board.retain_reservations(|_| panic!("expired board")), 0);
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
