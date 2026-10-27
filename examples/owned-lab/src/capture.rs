//! Debug-build-only visual review hooks for browser screenshots. This module
//! is not compiled into release Wasm or any native build, so shipped behavior,
//! the browser ABI and the CLI are unchanged. A review harness calls these
//! exports between frames; gameplay still runs through real input and ticks.
use super::*;

fn aim_at(target: Vec3) {
    let eye = with_lab(|lab| lab.game.player(0).unwrap().eye_position());
    let delta = target - eye;
    interactive::debug_aim(
        delta.x.atan2(-delta.z),
        delta.y.atan2(delta.x.hypot(delta.z)),
    );
}

/// Aim at the nearest live operator (`head` 1 for the head, 0 for the chest).
/// Returns the operator id, or 0 when none is alive.
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_debug_aim(head: u32) -> u32 {
    let target = with_lab(|lab| {
        let eye = lab.game.player(0).unwrap().eye_position();
        lab.game
            .players
            .iter()
            .filter(|p| p.id != 0 && p.is_alive())
            .filter(|p| {
                let to = p.eye_position() - eye;
                let distance = to.length();
                lab.game
                    .arena
                    .raycast(eye, to / distance, distance)
                    .is_none()
            })
            .min_by(|a, b| {
                (a.position - eye)
                    .length()
                    .total_cmp(&(b.position - eye).length())
            })
            .map(|p| (p.id, p.position + p.velocity / 64.))
    });
    let Some((id, position)) = target else {
        return 0;
    };
    aim_at(position + vec3(0., if head != 0 { 1.66 } else { 1.05 }, 0.));
    u32::from(id)
}

/// Aim at the qualifier bell.
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_debug_aim_bell() {
    aim_at(qualifier::BELL);
}

/// Pretend a stored personal best existed (0 clears it).
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_debug_best(score: u32) {
    with_lab_mut(|lab| lab.bests.pretend((score != 0).then_some(score)));
}

/// Stage operator `id` `distance` meters from the player, `angle` radians
/// right of their facing, for a framed screenshot. Hits still go through
/// the real match.
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_debug_stage(id: u32, distance: f32, angle: f32) {
    with_lab_mut(|lab| {
        let player = lab.game.player(0).unwrap();
        let (origin, yaw) = (player.position, player.yaw + angle);
        if let Some(target) = u8::try_from(id).ok().and_then(|id| lab.game.player_mut(id)) {
            target.position = origin + vec3(yaw.sin(), 0., -yaw.cos()) * distance;
            target.velocity = Vec3::ZERO;
        }
    });
}
