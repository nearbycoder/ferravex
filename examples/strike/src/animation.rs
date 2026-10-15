//! Authored eight-bone operators and four-bone first-person weapons. Simulation
//! remains authoritative: these players consume replicated state for presentation.
use ferravex_fps::{Player, RELOAD_TICKS, TICK_RATE};
use ferravex_play::{
    animation::*,
    scene::{IDENTITY, Joint, Skeleton, Transform},
};
use ferravex_render::{Mat4, Quat, skinning::SkinPalette};
const IDLE: usize = 0;
const WALK: usize = 1;
const RUN: usize = 2;
const AIR: usize = 3;
const AIM: usize = 4;
const RELOAD: usize = 5;
const FIRE: usize = 6;
pub const FOOTSTEP: u32 = 1;
const MAGAZINE_OUT: u32 = 2;
const MAGAZINE_IN: u32 = 3;
fn translation(x: f32, y: f32, z: f32) -> Transform {
    Transform {
        translation: [x, y, z],
        ..Default::default()
    }
}
fn vector(keys: &[(f32, [f32; 3])]) -> Vec3Track {
    Vec3Track::new(
        keys.iter()
            .map(|(time, value)| Keyframe {
                time: *time,
                value: *value,
            })
            .collect(),
        Interpolation::Linear,
    )
    .unwrap()
}
fn rotation(keys: &[(f32, Quat)]) -> QuatTrack {
    QuatTrack::new(
        keys.iter()
            .map(|(time, value)| Keyframe {
                time: *time,
                value: value.to_array(),
            })
            .collect(),
        Interpolation::Linear,
    )
    .unwrap()
}
fn rot(joint: u16, keys: &[(f32, Quat)]) -> JointTrack {
    JointTrack {
        joint,
        translation: None,
        rotation: Some(rotation(keys)),
        scale: None,
    }
}
fn pos(joint: u16, keys: &[(f32, [f32; 3])]) -> JointTrack {
    JointTrack {
        joint,
        translation: Some(vector(keys)),
        rotation: None,
        scale: None,
    }
}
fn clip(
    joints: usize,
    seconds: f32,
    mode: LoopMode,
    tracks: Vec<JointTrack>,
    events: Vec<AnimationEvent>,
) -> AnimationClip {
    AnimationClip::new(joints, seconds, mode, tracks, events).unwrap()
}
fn skeleton(bind: &[Transform], parents: &[Option<usize>]) -> Skeleton {
    let mut world = vec![Mat4::IDENTITY; bind.len()];
    let joints = bind
        .iter()
        .zip(parents)
        .enumerate()
        .map(|(i, (t, parent))| {
            let local = Mat4::from_cols_array(&t.matrix().unwrap());
            world[i] = parent.map_or(local, |p| world[p] * local);
            Joint {
                parent: *parent,
                inverse_bind: world[i].inverse().to_cols_array(),
            }
        })
        .collect();
    Skeleton::new(joints).unwrap()
}
pub struct OperatorRig {
    skeleton: Skeleton,
    graph: AnimationGraph,
}
impl OperatorRig {
    pub fn new() -> Self {
        let bind = vec![
            Transform::default(),
            translation(0., 1.1, 0.),
            translation(0., 0.53, 0.),
            translation(-0.16, 0.81, 0.),
            translation(0.16, 0.81, 0.),
            translation(-0.32, 0.18, -0.15),
            translation(0.32, 0.18, -0.15),
            translation(-0.10, -0.18, -0.34),
        ];
        let skeleton = skeleton(
            &bind,
            &[
                None,
                Some(0),
                Some(1),
                Some(0),
                Some(0),
                Some(1),
                Some(1),
                Some(6),
            ],
        );
        let idle = clip(
            8,
            2.,
            LoopMode::Repeat,
            vec![pos(
                1,
                &[
                    (0., [0., 1.1, 0.]),
                    (1., [0., 1.115, 0.]),
                    (2., [0., 1.1, 0.]),
                ],
            )],
            vec![],
        );
        let gait = |duration: f32, amplitude: f32| {
            let mut tracks = Vec::new();
            for (joint, sign) in [(3, 1.), (4, -1.), (5, -0.18), (6, 0.18)] {
                tracks.push(rot(
                    joint,
                    &[
                        (0., Quat::IDENTITY),
                        (duration * 0.25, Quat::from_rotation_x(amplitude * sign)),
                        (duration * 0.5, Quat::IDENTITY),
                        (duration * 0.75, Quat::from_rotation_x(-amplitude * sign)),
                        (duration, Quat::IDENTITY),
                    ],
                ));
            }
            tracks.push(pos(
                1,
                &[
                    (0., [0., 1.1, 0.]),
                    (duration * 0.25, [0., 1.13, 0.]),
                    (duration * 0.5, [0., 1.1, 0.]),
                    (duration * 0.75, [0., 1.13, 0.]),
                    (duration, [0., 1.1, 0.]),
                ],
            ));
            clip(
                8,
                duration,
                LoopMode::Repeat,
                tracks,
                vec![
                    AnimationEvent {
                        time: duration * 0.25,
                        id: FOOTSTEP,
                    },
                    AnimationEvent {
                        time: duration * 0.75,
                        id: FOOTSTEP,
                    },
                ],
            )
        };
        let air = clip(
            8,
            0.5,
            LoopMode::Clamp,
            vec![
                rot(3, &[(0., Quat::from_rotation_x(-0.25))]),
                rot(4, &[(0., Quat::from_rotation_x(0.35))]),
            ],
            vec![],
        );
        let aim = clip(
            8,
            1.,
            LoopMode::Clamp,
            vec![
                rot(
                    2,
                    &[
                        (0., Quat::from_rotation_x(-0.7)),
                        (1., Quat::from_rotation_x(0.7)),
                    ],
                ),
                rot(
                    5,
                    &[
                        (0., Quat::from_rotation_x(-0.8)),
                        (1., Quat::from_rotation_x(0.8)),
                    ],
                ),
                rot(
                    6,
                    &[
                        (0., Quat::from_rotation_x(-0.8)),
                        (1., Quat::from_rotation_x(0.8)),
                    ],
                ),
            ],
            vec![],
        );
        let seconds = RELOAD_TICKS as f32 / TICK_RATE as f32;
        let reload = clip(
            8,
            seconds,
            LoopMode::Clamp,
            vec![
                rot(
                    5,
                    &[
                        (0., Quat::IDENTITY),
                        (seconds * 0.35, Quat::from_rotation_x(0.8)),
                        (seconds * 0.75, Quat::from_rotation_x(0.4)),
                        (seconds, Quat::IDENTITY),
                    ],
                ),
                rot(
                    7,
                    &[
                        (0., Quat::IDENTITY),
                        (seconds * 0.25, Quat::from_rotation_z(-0.35)),
                        (seconds * 0.8, Quat::from_rotation_z(-0.35)),
                        (seconds, Quat::IDENTITY),
                    ],
                ),
            ],
            vec![],
        );
        let fire = clip(
            8,
            0.12,
            LoopMode::Clamp,
            vec![pos(
                7,
                &[
                    (0., [-0.1, -0.18, -0.34]),
                    (0.025, [-0.1, -0.18, -0.26]),
                    (0.12, [-0.1, -0.18, -0.34]),
                ],
            )],
            vec![],
        );
        let bank = AnimationBank::new(
            bind,
            vec![
                idle,
                gait(0.7, 0.5),
                gait(0.5, 0.75),
                air,
                aim,
                reload,
                fire,
            ],
        )
        .unwrap();
        let states = (0..4)
            .map(|clip| AnimationState { clip, speed: 1. })
            .collect();
        let mut edges = Vec::new();
        for from in 0..4_u16 {
            let mut edge = |to, conditions| {
                if from != to {
                    edges.push(AnimationTransition {
                        from,
                        to,
                        seconds: 0.12,
                        exit_time: 0.,
                        conditions,
                    });
                }
            };
            edge(
                AIR as u16,
                vec![AnimationCondition::Less {
                    parameter: 1,
                    value: 0.5,
                }],
            );
            edge(
                RUN as u16,
                vec![
                    AnimationCondition::Greater {
                        parameter: 1,
                        value: 0.5,
                    },
                    AnimationCondition::Greater {
                        parameter: 0,
                        value: 5.8,
                    },
                ],
            );
            edge(
                WALK as u16,
                vec![
                    AnimationCondition::Greater {
                        parameter: 1,
                        value: 0.5,
                    },
                    AnimationCondition::Greater {
                        parameter: 0,
                        value: 0.2,
                    },
                    AnimationCondition::Less {
                        parameter: 0,
                        value: 5.8,
                    },
                ],
            );
            edge(
                IDLE as u16,
                vec![
                    AnimationCondition::Greater {
                        parameter: 1,
                        value: 0.5,
                    },
                    AnimationCondition::Less {
                        parameter: 0,
                        value: 0.2,
                    },
                ],
            );
        }
        Self {
            skeleton,
            graph: AnimationGraph::new(&bank, states, edges, 2).unwrap(),
        }
    }
}
pub struct OperatorMotion {
    player: GraphPlayer,
    world: [[f32; 16]; 8],
    matrices: [[f32; 16]; 8],
    palette: SkinPalette,
    events: EventBuffer<8>,
    fire: f32,
    previous_ammo: u8,
    alive: bool,
}
impl OperatorMotion {
    pub fn new(rig: &OperatorRig) -> Self {
        Self {
            player: GraphPlayer::new(&rig.graph, IDLE).unwrap(),
            world: [IDENTITY; 8],
            matrices: [IDENTITY; 8],
            palette: SkinPalette::new(&[IDENTITY; 8]).unwrap(),
            events: EventBuffer::default(),
            fire: 1.,
            previous_ammo: 30,
            alive: true,
        }
    }
    pub fn update(&mut self, rig: &OperatorRig, p: &Player, dt: f32) {
        if !self.alive && p.health > 0 {
            self.player.reset(IDLE).unwrap();
            self.fire = 1.;
        }
        let speed = (p.velocity.x * p.velocity.x + p.velocity.z * p.velocity.z).sqrt();
        self.player.set_parameter(0, speed).unwrap();
        self.player
            .set_parameter(1, if p.grounded { 1. } else { 0. })
            .unwrap();
        self.player.advance(dt, &mut self.events).unwrap();
        let actor = self.player.animator_mut();
        actor
            .apply_layer(
                AIM,
                ((p.pitch + 1.) * 0.5).clamp(0., 1.),
                1.,
                None,
                LayerMode::Additive,
            )
            .unwrap();
        if p.reload_ticks > 0 {
            actor
                .apply_layer(
                    RELOAD,
                    (RELOAD_TICKS - p.reload_ticks.min(RELOAD_TICKS)) as f32 / TICK_RATE as f32,
                    1.,
                    None,
                    LayerMode::Additive,
                )
                .unwrap();
        }
        if p.ammo < self.previous_ammo {
            self.fire = 0.;
        }
        self.fire = (self.fire + dt).min(1.);
        if self.fire < 0.12 {
            actor
                .apply_layer(FIRE, self.fire, 1., None, LayerMode::Additive)
                .unwrap();
        }
        self.alive = p.health > 0;
        self.previous_ammo = p.ammo;
        rig.skeleton
            .skin_palette_into(actor.pose(), &mut self.world, &mut self.matrices)
            .unwrap();
        self.palette.set(&self.matrices).unwrap();
    }
    pub fn palette(&self) -> &SkinPalette {
        &self.palette
    }
    pub fn footsteps(&self) -> usize {
        self.events
            .hits()
            .iter()
            .filter(|e| e.id == FOOTSTEP)
            .count()
    }
}
pub struct WeaponMotion {
    skeleton: Skeleton,
    player: Animator,
    world: [[f32; 16]; 4],
    matrices: [[f32; 16]; 4],
    palette: SkinPalette,
    events: EventBuffer<8>,
    fire: f32,
    was_reload: bool,
    was_alive: bool,
}
impl WeaponMotion {
    pub fn new() -> Self {
        let bind = vec![
            Transform::default(),
            translation(0., -0.18, -0.06),
            translation(-0.07, -0.10, -0.32),
            translation(0.045, -0.19, 0.10),
        ];
        let skeleton = skeleton(&bind, &[None, Some(0), Some(0), Some(0)]);
        let idle = clip(
            4,
            2.,
            LoopMode::Repeat,
            vec![pos(
                0,
                &[(0., [0.; 3]), (1., [0., 0.003, 0.]), (2., [0.; 3])],
            )],
            vec![],
        );
        let walk = clip(
            4,
            0.6,
            LoopMode::Repeat,
            vec![pos(
                0,
                &[
                    (0., [0.; 3]),
                    (0.15, [0.003, 0.008, 0.]),
                    (0.3, [0.; 3]),
                    (0.45, [-0.003, 0.008, 0.]),
                    (0.6, [0.; 3]),
                ],
            )],
            vec![],
        );
        let duration = RELOAD_TICKS as f32 / TICK_RATE as f32;
        let reload = clip(
            4,
            duration,
            LoopMode::Clamp,
            vec![
                rot(
                    0,
                    &[
                        (0., Quat::IDENTITY),
                        (
                            0.4,
                            Quat::from_rotation_z(-0.45) * Quat::from_rotation_x(0.15),
                        ),
                        (
                            1.5,
                            Quat::from_rotation_z(-0.45) * Quat::from_rotation_x(0.15),
                        ),
                        (duration, Quat::IDENTITY),
                    ],
                ),
                pos(
                    1,
                    &[
                        (0., [0., -0.18, -0.06]),
                        (0.5, [0., -0.6, -0.06]),
                        (1.1, [0., -0.6, -0.06]),
                        (1.5, [0., -0.18, -0.06]),
                        (duration, [0., -0.18, -0.06]),
                    ],
                ),
                pos(
                    2,
                    &[
                        (0., [-0.07, -0.10, -0.32]),
                        (0.5, [-0.07, -0.5, -0.06]),
                        (1.1, [-0.07, -0.5, -0.06]),
                        (1.5, [-0.07, -0.10, -0.32]),
                        (duration, [-0.07, -0.10, -0.32]),
                    ],
                ),
            ],
            vec![
                AnimationEvent {
                    time: 0.5,
                    id: MAGAZINE_OUT,
                },
                AnimationEvent {
                    time: 1.5,
                    id: MAGAZINE_IN,
                },
            ],
        );
        let fire = clip(
            4,
            0.12,
            LoopMode::Clamp,
            vec![JointTrack {
                joint: 0,
                translation: Some(vector(&[
                    (0., [0.; 3]),
                    (0.025, [0., -0.012, 0.045]),
                    (0.12, [0.; 3]),
                ])),
                rotation: Some(rotation(&[
                    (0., Quat::IDENTITY),
                    (0.025, Quat::from_rotation_x(0.025)),
                    (0.12, Quat::IDENTITY),
                ])),
                scale: None,
            }],
            vec![],
        );
        let bank = AnimationBank::new(bind, vec![idle, walk, reload, fire]).unwrap();
        Self {
            skeleton,
            player: Animator::new(&bank, 0).unwrap(),
            world: [IDENTITY; 4],
            matrices: [IDENTITY; 4],
            palette: SkinPalette::new(&[IDENTITY; 4]).unwrap(),
            events: EventBuffer::default(),
            fire: 1.,
            was_reload: false,
            was_alive: true,
        }
    }
    pub fn update(&mut self, p: &Player, dt: f32, fired: bool) -> usize {
        if !self.was_alive && p.health > 0 {
            self.player.play(0).unwrap();
            self.fire = 1.;
            self.was_reload = false;
        }
        self.was_alive = p.health > 0;
        let reloading = p.reload_ticks > 0;
        let speed = (p.velocity.x * p.velocity.x + p.velocity.z * p.velocity.z).sqrt();
        self.player
            .transition(
                if reloading {
                    2
                } else if speed > 0.2 && p.grounded {
                    1
                } else {
                    0
                },
                0.12,
            )
            .unwrap();
        self.player
            .set_speed(if !reloading && self.player.clip() == 1 {
                (speed / 6.).clamp(0.2, 1.4)
            } else {
                1.
            })
            .unwrap();
        let expected = (RELOAD_TICKS - p.reload_ticks.min(RELOAD_TICKS)) as f32 / TICK_RATE as f32;
        if reloading && (!self.was_reload || (self.player.timeline().time() - expected).abs() > 0.2)
        {
            // Late snapshots/resume seek silently; presentation never invents
            // missed magazine events or changes the authoritative ammo count.
            self.player.seek(expected).unwrap();
        }
        self.was_reload = reloading;
        self.player.advance(dt, &mut self.events).unwrap();
        if fired {
            self.fire = 0.;
        }
        self.fire = (self.fire + dt).min(1.);
        if self.fire < 0.12 && !reloading {
            self.player
                .apply_layer(3, self.fire, 1., None, LayerMode::Additive)
                .unwrap();
        }
        self.skeleton
            .skin_palette_into(self.player.pose(), &mut self.world, &mut self.matrices)
            .unwrap();
        self.palette.set(&self.matrices).unwrap();
        self.events
            .hits()
            .iter()
            .filter(|e| e.id == MAGAZINE_OUT || e.id == MAGAZINE_IN)
            .count()
    }
    pub fn palette(&self) -> &SkinPalette {
        &self.palette
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operators_walk_aim_reload_and_fire_without_mutating_game_state() {
        let rig = OperatorRig::new();
        let mut actor = OperatorMotion::new(&rig);
        let mut p = Player::new(1, ferravex_render::Vec3::ZERO);
        p.velocity.x = 3.;
        let original = p.clone();
        let mut steps = 0;
        for _ in 0..120 {
            actor.update(&rig, &p, 1. / 60.);
            steps += actor.footsteps();
        }
        assert!(steps >= 4);
        assert_eq!(p, original);
        assert_eq!(actor.player.state(), WALK);
        let idle = actor.palette().matrices().to_vec();
        p.pitch = 0.7;
        p.reload_ticks = 80;
        p.ammo = 29;
        actor.update(&rig, &p, 1. / 60.);
        assert_ne!(actor.palette().matrices(), idle);
        assert!(
            actor
                .palette()
                .matrices()
                .iter()
                .flatten()
                .all(|v| v.is_finite())
        );
        p.grounded = false;
        actor.update(&rig, &p, 1. / 60.);
        assert_eq!(actor.player.state(), AIR);
        p.health = 0;
        actor.update(&rig, &p, 1. / 60.);
        p.health = 100;
        p.grounded = true;
        p.velocity.x = 0.;
        actor.update(&rig, &p, 1. / 60.);
        assert_eq!(actor.player.state(), IDLE);
    }
    #[test]
    fn weapon_reload_detaches_magazine_emits_events_and_recovers_on_respawn() {
        let mut weapon = WeaponMotion::new();
        let mut p = Player::new(0, ferravex_render::Vec3::ZERO);
        let mut clicks = 0;
        p.reload_ticks = RELOAD_TICKS;
        for frame in 0..120 {
            p.reload_ticks =
                RELOAD_TICKS.saturating_sub((frame as f32 * TICK_RATE as f32 / 60.) as u16);
            clicks += weapon.update(&p, 1. / 60., false);
            if frame == 60 {
                assert!(weapon.player.pose()[1].translation[1] < -0.5);
            }
        }
        assert_eq!(clicks, 2);
        p.reload_ticks = 0;
        p.ammo = 30;
        weapon.update(&p, 1. / 60., true);
        assert!(weapon.fire < 0.12);
        p.health = 0;
        weapon.update(&p, 0., false);
        p.health = 100;
        weapon.update(&p, 1. / 60., false);
        assert!(weapon.fire >= 0.12);
        assert!(
            weapon
                .palette()
                .matrices()
                .iter()
                .flatten()
                .all(|v| v.is_finite())
        );
    }
    #[test]
    fn late_reload_snapshot_resynchronizes_silently_and_bob_speed_tracks_motion() {
        let mut weapon = WeaponMotion::new();
        let mut p = Player::new(0, ferravex_render::Vec3::ZERO);
        p.velocity.x = 1.;
        weapon.update(&p, 1. / 60., false);
        assert!((weapon.player.timeline().speed() - 0.2).abs() < 0.001);
        p.reload_ticks = RELOAD_TICKS;
        weapon.update(&p, 1. / 60., false);
        p.reload_ticks = RELOAD_TICKS - 90;
        assert_eq!(weapon.update(&p, 0., false), 0);
        assert!((weapon.player.timeline().time() - 90. / TICK_RATE as f32).abs() < 0.001);
        assert_eq!(
            weapon.update(&p, 0.1, false),
            1,
            "only the future magazine-in event should fire"
        );
    }
    #[test]
    fn authored_meshes_match_rig_joint_counts_and_bind_bounds() {
        let op = crate::geometry::build_operator();
        let rifle = crate::geometry::build_rifle();
        assert_eq!(op.joint_count(), 8);
        assert_eq!(rifle.joint_count(), 4);
        let actor = OperatorMotion::new(&OperatorRig::new());
        assert!(op.bounds(actor.palette()).is_ok());
        assert!(rifle.bounds(WeaponMotion::new().palette()).is_ok());
    }
}
