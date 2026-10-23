//! Offline guidance uses actual visibility, never modifies aim or awards hits.
use super::*;
use ferravex_engine::play::ai::{Candidate, Observer, Sight, VisibilityQuery, WorkBudget};

struct CourtyardSight<'a>(&'a Arena);
impl VisibilityQuery for CourtyardSight<'_> {
    fn clear(&mut self, from: Vec3, to: Vec3) -> bool {
        let delta = to - from;
        let distance = delta.length();
        distance <= f32::EPSILON || self.0.raycast(from, delta / distance, distance).is_none()
    }
}
impl Lab {
    pub(super) fn update_orientation(&mut self) {
        let player = self.game.player(0).unwrap();
        if self.peer.mode != 0 || !player.is_alive() {
            self.target_selector.reset();
            self.target_hint = None;
            return;
        }
        let origin = player.eye_position();
        let candidates = self
            .game
            .players
            .iter()
            .filter(|p| p.id != 0 && p.is_alive())
            .map(|p| Candidate {
                id: u64::from(p.id),
                position: p.eye_position(),
                groups: 1,
            });
        let report = self
            .target_selector
            .select_visible(
                candidates,
                Observer {
                    position: origin,
                    forward: forward(player.yaw, player.pitch),
                },
                Sight::new(55., std::f32::consts::TAU, 1).unwrap(),
                &mut WorkBudget::new(16, 16, 0),
                &mut CourtyardSight(&self.game.arena),
                |target| -(target.position - origin).length(),
            )
            .expect("finite match poses and bounded sight queries");
        self.target_hint = report.target;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guide_retains_near_equal_targets_but_never_tracks_through_cover_or_in_peers() {
        let mut lab = Lab::new_interactive();
        lab.game.arena.solids.clear();
        lab.game.players.retain(|p| p.id < 3);
        lab.game.player_mut(0).unwrap().position = Vec3::ZERO;
        lab.game.player_mut(1).unwrap().position = vec3(0., 0., -10.);
        lab.game.player_mut(2).unwrap().position = vec3(0., 0., -11.);
        lab.update_orientation();
        assert_eq!(lab.target_hint.unwrap().id, 1);
        lab.game.player_mut(2).unwrap().position.z = -9.;
        lab.update_orientation();
        assert_eq!(
            lab.target_hint.unwrap().id,
            1,
            "one meter does not switch guidance"
        );
        lab.game.player_mut(2).unwrap().position.z = -7.;
        lab.update_orientation();
        assert_eq!(lab.target_hint.unwrap().id, 2);
        lab.game.arena.solids.push(ferravex_fps::Obstacle {
            center: vec3(0., 2., -3.),
            half_extents: vec3(5., 3., 0.5),
        });
        lab.update_orientation();
        assert!(lab.target_hint.is_none());
        lab.game.arena.solids.clear();
        lab.peer.set_mode(2);
        lab.update_orientation();
        assert!(
            lab.target_hint.is_none(),
            "no peer opponent tracking assistance"
        );
    }
}
