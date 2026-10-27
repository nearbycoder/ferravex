//! Skeletons, authored clips and animation state for every character.
//!
//! The raccoon (and Biscuit, who shares its quadruped rig) is driven by an
//! [`AnimationGraph`] whose transitions read scalar parameters (speed,
//! grounded, sliding, sneaking, scruffed, celebrating) with cross-fades, plus
//! additive layers for carrying and chittering. Picnickers use a biped bank
//! and a plain [`Animator`] whose cross-fades follow their simulation pose.
//! Everything here is presentation: it reads accepted state and never feeds
//! back into the rules.
use crate::sim::folk::{Kind, Pose};
use crate::sim::park::Act;
use ferravex_play::animation::*;
use ferravex_play::scene::{IDENTITY, Joint, Skeleton, Transform};
use ferravex_render::prelude::{Mat4, Quat, Vec3};
use ferravex_render::skinning::SkinPalette;

pub const STEP: u32 = 1;

fn t(x: f32, y: f32, z: f32) -> Transform {
    Transform {
        translation: [x, y, z],
        ..Default::default()
    }
}
fn vec_track(keys: &[(f32, [f32; 3])]) -> Vec3Track {
    Vec3Track::new(
        keys.iter()
            .map(|(time, value)| Keyframe {
                time: *time,
                value: *value,
            })
            .collect(),
        Interpolation::Linear,
    )
    .expect("authored keys")
}
fn rot_track(keys: &[(f32, Quat)]) -> QuatTrack {
    QuatTrack::new(
        keys.iter()
            .map(|(time, value)| Keyframe {
                time: *time,
                value: value.to_array(),
            })
            .collect(),
        Interpolation::Linear,
    )
    .expect("authored keys")
}
fn rot(joint: u16, keys: &[(f32, Quat)]) -> JointTrack {
    JointTrack {
        joint,
        translation: None,
        rotation: Some(rot_track(keys)),
        scale: None,
    }
}
fn pos(joint: u16, keys: &[(f32, [f32; 3])]) -> JointTrack {
    JointTrack {
        joint,
        translation: Some(vec_track(keys)),
        rotation: None,
        scale: None,
    }
}
fn both(joint: u16, p: &[(f32, [f32; 3])], r: &[(f32, Quat)]) -> JointTrack {
    JointTrack {
        joint,
        translation: Some(vec_track(p)),
        rotation: Some(rot_track(r)),
        scale: None,
    }
}
fn scale(joint: u16, keys: &[(f32, [f32; 3])]) -> JointTrack {
    JointTrack {
        joint,
        translation: None,
        rotation: None,
        scale: Some(vec_track(keys)),
    }
}
fn clip(
    joints: usize,
    seconds: f32,
    mode: LoopMode,
    tracks: Vec<JointTrack>,
    events: Vec<AnimationEvent>,
) -> AnimationClip {
    AnimationClip::new(joints, seconds, mode, tracks, events).expect("authored clip")
}
fn skeleton(bind: &[Transform], parents: &[Option<usize>]) -> Skeleton {
    let mut world = vec![Mat4::IDENTITY; bind.len()];
    let joints = bind
        .iter()
        .zip(parents)
        .enumerate()
        .map(|(i, (tr, parent))| {
            let local = Mat4::from_cols_array(&tr.matrix().expect("finite bind"));
            world[i] = parent.map_or(local, |p| world[p] * local);
            Joint {
                parent: *parent,
                inverse_bind: world[i].inverse().to_cols_array(),
            }
        })
        .collect();
    Skeleton::new(joints).expect("authored skeleton")
}
fn x(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}
fn y(a: f32) -> Quat {
    Quat::from_rotation_y(a)
}
fn z(a: f32) -> Quat {
    Quat::from_rotation_z(a)
}
/// A four-key swing between `a` and `b` and back, over `d` seconds.
fn swing(joint: u16, d: f32, a: Quat, b: Quat) -> JointTrack {
    rot(
        joint,
        &[
            (0., Quat::IDENTITY),
            (d * 0.25, a),
            (d * 0.5, Quat::IDENTITY),
            (d * 0.75, b),
            (d, Quat::IDENTITY),
        ],
    )
}

// --- Quadruped (raccoon, Biscuit).
pub const Q_JOINTS: usize = 10;
pub const Q_IDLE: usize = 0;
pub const Q_TROT: usize = 1;
pub const Q_SNEAK: usize = 2;
pub const Q_AIR: usize = 3;
pub const Q_SLIDE: usize = 4;
pub const Q_CELEBRATE: usize = 5;
pub const Q_SCRUFF: usize = 6;
const Q_CARRY: usize = 7;
const Q_CHITTER: usize = 8;
const Q_WAG: usize = 9;
const Q_SIT: usize = 10;

const BODY: [f32; 3] = [0., 0.33, 0.05];

pub struct QuadRig {
    pub skeleton: Skeleton,
    pub graph: AnimationGraph,
    pub bank: AnimationBank,
}

impl Default for QuadRig {
    fn default() -> Self {
        Self::new()
    }
}

impl QuadRig {
    pub fn new() -> Self {
        let bind = vec![
            Transform::default(),
            t(BODY[0], BODY[1], BODY[2]),
            t(0., 0.02, -0.25),
            t(0., 0.12, -0.2),
            t(0., 0.06, 0.22),
            t(0., 0., 0.18),
            t(-0.1, -0.08, -0.07),
            t(0.1, -0.08, -0.07),
            t(-0.1, -0.07, 0.12),
            t(0.1, -0.07, 0.12),
        ];
        let skeleton = skeleton(
            &bind,
            &[
                None,
                Some(0),
                Some(1),
                Some(2),
                Some(1),
                Some(4),
                Some(2),
                Some(2),
                Some(1),
                Some(1),
            ],
        );
        let n = Q_JOINTS;
        let idle = clip(
            n,
            2.4,
            LoopMode::Repeat,
            vec![
                pos(1, &[(0., BODY), (1.2, [0., 0.338, 0.05]), (2.4, BODY)]),
                swing(4, 2.4, y(0.3), y(-0.3)),
                swing(5, 2.4, y(0.35), y(-0.35)),
                rot(
                    3,
                    &[
                        (0., Quat::IDENTITY),
                        (0.9, z(0.08) * x(0.05)),
                        (1.8, z(-0.06)),
                        (2.4, Quat::IDENTITY),
                    ],
                ),
            ],
            vec![],
        );
        let gait = |d: f32, amp: f32, low: f32| {
            let body = |dy: f32| [0., BODY[1] - low + dy, BODY[2]];
            clip(
                n,
                d,
                LoopMode::Repeat,
                vec![
                    swing(6, d, x(-amp), x(amp)),
                    swing(9, d, x(-amp), x(amp)),
                    swing(7, d, x(amp), x(-amp)),
                    swing(8, d, x(amp), x(-amp)),
                    both(
                        1,
                        &[
                            (0., body(0.)),
                            (d * 0.25, body(0.035)),
                            (d * 0.5, body(0.)),
                            (d * 0.75, body(0.035)),
                            (d, body(0.)),
                        ],
                        &[
                            (0., x(low * 0.5)),
                            (d * 0.5, x(low * 0.5 + 0.04)),
                            (d, x(low * 0.5)),
                        ],
                    ),
                    rot(
                        4,
                        &[
                            (0., x(-0.35 + low)),
                            (d * 0.5, x(-0.25 + low)),
                            (d, x(-0.35 + low)),
                        ],
                    ),
                    swing(5, d, y(0.2), y(-0.2)),
                    swing(3, d, x(0.06), x(-0.04)),
                ],
                vec![
                    AnimationEvent {
                        time: d * 0.25,
                        id: STEP,
                    },
                    AnimationEvent {
                        time: d * 0.75,
                        id: STEP,
                    },
                ],
            )
        };
        let air = clip(
            n,
            0.4,
            LoopMode::Clamp,
            vec![
                rot(6, &[(0., x(-0.2)), (0.4, x(-0.8))]),
                rot(7, &[(0., x(-0.2)), (0.4, x(-0.7))]),
                rot(8, &[(0., x(0.3)), (0.4, x(0.8))]),
                rot(9, &[(0., x(0.3)), (0.4, x(0.75))]),
                rot(4, &[(0., x(-0.4)), (0.4, x(-0.9))]),
                rot(3, &[(0., x(0.1)), (0.4, x(0.25))]),
            ],
            vec![],
        );
        let slide = clip(
            n,
            0.25,
            LoopMode::Clamp,
            vec![
                pos(1, &[(0., BODY), (0.25, [0., 0.17, 0.05])]),
                rot(6, &[(0., Quat::IDENTITY), (0.25, x(-1.35))]),
                rot(7, &[(0., Quat::IDENTITY), (0.25, x(-1.3))]),
                rot(8, &[(0., Quat::IDENTITY), (0.25, x(1.35))]),
                rot(9, &[(0., Quat::IDENTITY), (0.25, x(1.3))]),
                rot(3, &[(0., Quat::IDENTITY), (0.25, x(0.35))]),
                rot(4, &[(0., Quat::IDENTITY), (0.25, x(-0.15))]),
            ],
            vec![],
        );
        let cheer = clip(
            n,
            0.9,
            LoopMode::Repeat,
            vec![
                both(
                    1,
                    &[
                        (0., [0., 0.42, 0.12]),
                        (0.45, [0., 0.5, 0.12]),
                        (0.9, [0., 0.42, 0.12]),
                    ],
                    &[(0., x(1.05)), (0.45, x(1.15)), (0.9, x(1.05))],
                ),
                rot(8, &[(0., x(-1.0)), (0.9, x(-1.0))]),
                rot(9, &[(0., x(-1.0)), (0.9, x(-1.0))]),
                swing(6, 0.45, x(-1.2) * z(-0.6), x(-0.4) * z(-0.3)),
                swing(7, 0.45, x(-0.4) * z(0.3), x(-1.2) * z(0.6)),
                rot(3, &[(0., x(-0.6)), (0.9, x(-0.6))]),
                swing(4, 0.45, y(0.5), y(-0.5)),
            ],
            vec![],
        );
        let scruff = clip(
            n,
            0.7,
            LoopMode::Repeat,
            vec![
                both(
                    1,
                    &[(0., [0., 0.3, 0.05]), (0.7, [0., 0.3, 0.05])],
                    &[(0., x(-0.35)), (0.35, x(-0.28)), (0.7, x(-0.35))],
                ),
                swing(6, 0.7, x(0.5), x(0.1)),
                swing(7, 0.7, x(0.1), x(0.5)),
                swing(8, 0.7, x(0.6), x(0.2)),
                swing(9, 0.7, x(0.2), x(0.6)),
                swing(4, 0.7, y(0.6) * x(0.5), y(-0.6) * x(0.5)),
                rot(3, &[(0., x(-0.3)), (0.7, x(-0.3))]),
            ],
            vec![],
        );
        let carry = clip(
            n,
            1.,
            LoopMode::Clamp,
            vec![
                rot(3, &[(0., Quat::IDENTITY), (1., x(-0.35))]),
                rot(2, &[(0., Quat::IDENTITY), (1., x(-0.12))]),
            ],
            vec![],
        );
        let chitter = clip(
            n,
            0.42,
            LoopMode::Clamp,
            vec![
                rot(
                    3,
                    &[
                        (0., Quat::IDENTITY),
                        (0.07, x(-0.25)),
                        (0.14, x(0.1)),
                        (0.21, x(-0.25)),
                        (0.28, x(0.1)),
                        (0.42, Quat::IDENTITY),
                    ],
                ),
                scale(
                    4,
                    &[
                        (0., [1., 1., 1.]),
                        (0.1, [1.5, 1.5, 1.1]),
                        (0.42, [1., 1., 1.]),
                    ],
                ),
            ],
            vec![],
        );
        let wag = clip(
            n,
            0.3,
            LoopMode::Repeat,
            vec![swing(4, 0.3, y(0.7), y(-0.7))],
            vec![],
        );
        let sit = clip(
            n,
            1.,
            LoopMode::Repeat,
            vec![
                both(
                    1,
                    &[(0., [0., 0.25, 0.1]), (1., [0., 0.25, 0.1])],
                    &[(0., x(0.55)), (1., x(0.55))],
                ),
                rot(8, &[(0., x(-1.2)), (1., x(-1.2))]),
                rot(9, &[(0., x(-1.2)), (1., x(-1.2))]),
                rot(6, &[(0., x(-0.5)), (1., x(-0.5))]),
                rot(7, &[(0., x(-0.5)), (1., x(-0.5))]),
                rot(3, &[(0., x(-0.4)), (1., x(-0.4))]),
            ],
            vec![],
        );
        let bank = AnimationBank::new(
            bind,
            vec![
                idle,
                gait(0.4, 0.7, 0.),
                gait(0.7, 0.4, 0.07),
                air,
                slide,
                cheer,
                scruff,
                carry,
                chitter,
                wag,
                sit,
            ],
        )
        .expect("quadruped bank");
        let states = (0..7)
            .map(|clip| AnimationState { clip, speed: 1. })
            .collect();
        use AnimationCondition::{Greater, Less};
        let mut edges = Vec::new();
        for from in 0..7_u16 {
            let mut edge = |to: usize, seconds: f32, conditions: Vec<AnimationCondition>| {
                if from as usize != to {
                    edges.push(AnimationTransition {
                        from,
                        to: to as u16,
                        seconds,
                        exit_time: 0.,
                        conditions,
                    });
                }
            };
            let g = |p: u8| Greater {
                parameter: p,
                value: 0.5,
            };
            let l = |p: u8| Less {
                parameter: p,
                value: 0.5,
            };
            edge(Q_SCRUFF, 0.1, vec![g(4)]);
            edge(Q_SLIDE, 0.08, vec![g(2), l(4)]);
            edge(Q_AIR, 0.1, vec![l(1), l(2), l(4)]);
            edge(Q_CELEBRATE, 0.15, vec![g(5), g(1), l(2), l(4)]);
            edge(
                Q_SNEAK,
                0.15,
                vec![
                    g(1),
                    g(3),
                    Greater {
                        parameter: 0,
                        value: 0.25,
                    },
                    l(2),
                    l(4),
                    l(5),
                ],
            );
            edge(
                Q_TROT,
                0.12,
                vec![
                    g(1),
                    l(3),
                    Greater {
                        parameter: 0,
                        value: 0.45,
                    },
                    l(2),
                    l(4),
                    l(5),
                ],
            );
            edge(
                Q_IDLE,
                0.2,
                vec![
                    g(1),
                    Less {
                        parameter: 0,
                        value: 0.25,
                    },
                    l(2),
                    l(4),
                    l(5),
                ],
            );
        }
        let graph = AnimationGraph::new(&bank, states, edges, 6).expect("raccoon graph");
        Self {
            skeleton,
            graph,
            bank,
        }
    }
}

/// Per-actor quadruped presentation state; storage is allocated once.
pub struct QuadMotion {
    player: GraphPlayer,
    world: [[f32; 16]; Q_JOINTS],
    pub matrices: [[f32; 16]; Q_JOINTS],
    pub palette: SkinPalette,
    pub events: EventBuffer<8>,
    chitter: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct QuadInput {
    pub speed: f32,
    pub grounded: bool,
    pub sliding: bool,
    pub sneaking: bool,
    pub scruffed: bool,
    pub celebrate: bool,
    pub carrying: bool,
    /// Seconds since a chitter (or a bark).
    pub chitter: f32,
    pub wag: bool,
    pub sit: bool,
}

impl QuadMotion {
    pub fn new(rig: &QuadRig) -> Self {
        Self {
            player: GraphPlayer::new(&rig.graph, Q_IDLE).expect("idle state"),
            world: [IDENTITY; Q_JOINTS],
            matrices: [IDENTITY; Q_JOINTS],
            palette: SkinPalette::new(&[IDENTITY; Q_JOINTS]).expect("palette"),
            events: EventBuffer::default(),
            chitter: 10.,
        }
    }

    pub fn reset(&mut self) {
        let _ = self.player.reset(Q_IDLE);
    }

    pub fn update(&mut self, rig: &QuadRig, i: QuadInput, dt: f32) {
        let dt = dt.clamp(0., 0.1);
        let flag = |b: bool| if b { 1. } else { 0. };
        let p = &mut self.player;
        let _ = p.set_parameter(0, i.speed);
        let _ = p.set_parameter(1, flag(i.grounded));
        let _ = p.set_parameter(2, flag(i.sliding));
        let _ = p.set_parameter(3, flag(i.sneaking));
        let _ = p.set_parameter(4, flag(i.scruffed));
        let _ = p.set_parameter(5, flag(i.celebrate));
        // Gait cadence follows actual travel speed.
        let rate = match p.state() {
            Q_TROT => (i.speed / 4.2).clamp(0.5, 1.6),
            Q_SNEAK => (i.speed / 1.8).clamp(0.4, 1.4),
            Q_SCRUFF if i.speed > 0.5 => 2.2,
            _ => 1.,
        };
        let _ = p.animator_mut().set_speed(rate);
        self.events.clear();
        let _ = p.advance(dt, &mut self.events);
        let a = p.animator_mut();
        if i.sit {
            let _ = a.apply_layer(Q_SIT, 0.5, 1., None, LayerMode::Override);
        }
        if i.carrying {
            let _ = a.apply_layer(Q_CARRY, 1., 1., None, LayerMode::Additive);
        }
        self.chitter = i.chitter;
        if i.chitter < 0.42 {
            let _ = a.apply_layer(Q_CHITTER, i.chitter, 1., None, LayerMode::Additive);
        }
        if i.wag {
            let phase = (self.player.animator().timeline().time() * 3.3) % 0.3;
            let _ =
                self.player
                    .animator_mut()
                    .apply_layer(Q_WAG, phase, 1., None, LayerMode::Override);
        }
        let _ = rig.skeleton.skin_palette_into(
            self.player.animator().pose(),
            &mut self.world,
            &mut self.matrices,
        );
        let _ = self.palette.set(&self.matrices);
    }

    pub fn steps(&self) -> usize {
        self.events.hits().iter().filter(|e| e.id == STEP).count()
    }

    /// Posed head transform (bind-space → model space) for hats.
    pub fn head(&self) -> Mat4 {
        Mat4::from_cols_array(&self.matrices[3])
    }
    pub fn tail(&self) -> Mat4 {
        Mat4::from_cols_array(&self.matrices[5])
    }
}

// --- Biped (picnickers).
pub const B_JOINTS: usize = 8;
const B_IDLE: usize = 0;
const B_WALK: usize = 1;
const B_RUN: usize = 2;
const B_SIT: usize = 3;
const B_CONDUCT: usize = 4;
const B_WORK: usize = 5;
const B_CHEER: usize = 6;
const B_STARTLED: usize = 7;
const B_CARRY: usize = 8;
const B_LOOK: usize = 9;
const B_PUFF: usize = 10;
const B_BOW: usize = 11;
const B_DANCE: usize = 12;
const B_WATER: usize = 13;

const HIPS: [f32; 3] = [0., 0.95, 0.];

pub struct BipedRig {
    pub skeleton: Skeleton,
    pub bank: AnimationBank,
}

impl Default for BipedRig {
    fn default() -> Self {
        Self::new()
    }
}

impl BipedRig {
    pub fn new() -> Self {
        let bind = vec![
            Transform::default(),
            t(HIPS[0], HIPS[1], HIPS[2]),
            t(0., 0.3, 0.),
            t(0., 0.35, 0.),
            t(-0.275, 0.18, 0.),
            t(0.275, 0.18, 0.),
            t(-0.1, -0.05, 0.),
            t(0.1, -0.05, 0.),
        ];
        let skeleton = skeleton(
            &bind,
            &[
                None,
                Some(0),
                Some(1),
                Some(2),
                Some(2),
                Some(2),
                Some(1),
                Some(1),
            ],
        );
        let n = B_JOINTS;
        let hips = |dy: f32| [0., HIPS[1] + dy, 0.];
        let idle = clip(
            n,
            3.,
            LoopMode::Repeat,
            vec![
                pos(1, &[(0., hips(0.)), (1.5, hips(-0.012)), (3., hips(0.))]),
                swing(2, 3., x(0.02), x(-0.01)),
                swing(4, 3., z(-0.05), z(-0.02)),
                swing(5, 3., z(0.05), z(0.02)),
            ],
            vec![],
        );
        let gait = |d: f32, leg: f32, arm: f32, lean: f32, bob: f32| {
            clip(
                n,
                d,
                LoopMode::Repeat,
                vec![
                    swing(6, d, x(-leg), x(leg)),
                    swing(7, d, x(leg), x(-leg)),
                    swing(4, d, x(arm), x(-arm)),
                    swing(5, d, x(-arm), x(arm)),
                    rot(2, &[(0., x(lean)), (d, x(lean))]),
                    pos(
                        1,
                        &[
                            (0., hips(0.)),
                            (d * 0.25, hips(bob)),
                            (d * 0.5, hips(0.)),
                            (d * 0.75, hips(bob)),
                            (d, hips(0.)),
                        ],
                    ),
                ],
                vec![
                    AnimationEvent {
                        time: d * 0.25,
                        id: STEP,
                    },
                    AnimationEvent {
                        time: d * 0.75,
                        id: STEP,
                    },
                ],
            )
        };
        let sit = clip(
            n,
            2.,
            LoopMode::Repeat,
            vec![
                pos(1, &[(0., [0., 0.5, 0.08]), (2., [0., 0.5, 0.08])]),
                rot(6, &[(0., x(-1.45)), (2., x(-1.45))]),
                rot(7, &[(0., x(-1.45)), (2., x(-1.45))]),
                swing(4, 2., x(-0.7) * z(0.2), x(-0.8) * z(0.25)),
                swing(5, 2., x(-0.8) * z(-0.25), x(-0.7) * z(-0.2)),
                swing(3, 2., x(0.12), x(0.15)),
            ],
            vec![],
        );
        let conduct = clip(
            n,
            1.,
            LoopMode::Repeat,
            vec![
                rot(
                    5,
                    &[
                        (0., x(-1.9)),
                        (0.25, x(-1.3) * z(0.4)),
                        (0.5, x(-1.9) * z(-0.2)),
                        (0.75, x(-1.4) * z(0.5)),
                        (1., x(-1.9)),
                    ],
                ),
                swing(4, 1., x(-0.9) * z(-0.3), x(-0.6) * z(-0.1)),
                swing(2, 1., y(0.12), y(-0.12)),
                pos(
                    1,
                    &[
                        (0., hips(0.)),
                        (0.25, hips(0.03)),
                        (0.5, hips(0.)),
                        (0.75, hips(0.03)),
                        (1., hips(0.)),
                    ],
                ),
            ],
            vec![],
        );
        let work = clip(
            n,
            1.2,
            LoopMode::Repeat,
            vec![
                swing(4, 1.2, x(-0.9), x(-0.6)),
                swing(5, 1.2, x(-0.6), x(-0.95)),
                rot(2, &[(0., x(0.25)), (0.6, x(0.32)), (1.2, x(0.25))]),
                swing(3, 1.2, y(0.15), y(-0.15)),
            ],
            vec![],
        );
        let cheer = clip(
            n,
            0.6,
            LoopMode::Repeat,
            vec![
                rot(4, &[(0., z(-2.6)), (0.3, z(-2.3)), (0.6, z(-2.6))]),
                rot(5, &[(0., z(2.6)), (0.3, z(2.3)), (0.6, z(2.6))]),
                pos(1, &[(0., hips(0.)), (0.3, hips(0.12)), (0.6, hips(0.))]),
                rot(3, &[(0., x(-0.2)), (0.6, x(-0.2))]),
            ],
            vec![],
        );
        let startled = clip(
            n,
            0.5,
            LoopMode::Clamp,
            vec![
                rot(4, &[(0., Quat::IDENTITY), (0.15, z(-2.4) * x(-0.3))]),
                rot(5, &[(0., Quat::IDENTITY), (0.15, z(2.4) * x(-0.3))]),
                pos(
                    1,
                    &[
                        (0., hips(0.)),
                        (0.15, hips(0.18)),
                        (0.35, hips(0.02)),
                        (0.5, hips(0.)),
                    ],
                ),
                rot(2, &[(0., Quat::IDENTITY), (0.15, x(-0.25))]),
                rot(3, &[(0., Quat::IDENTITY), (0.15, x(-0.3))]),
            ],
            vec![],
        );
        let carry = clip(
            n,
            0.7,
            LoopMode::Repeat,
            vec![
                rot(4, &[(0., x(-1.2) * z(0.25)), (0.7, x(-1.2) * z(0.25))]),
                rot(5, &[(0., x(-1.2) * z(-0.25)), (0.7, x(-1.2) * z(-0.25))]),
                swing(6, 0.7, x(-0.45), x(0.45)),
                swing(7, 0.7, x(0.45), x(-0.45)),
                rot(2, &[(0., x(-0.1)), (0.7, x(-0.1))]),
            ],
            vec![
                AnimationEvent {
                    time: 0.175,
                    id: STEP,
                },
                AnimationEvent {
                    time: 0.525,
                    id: STEP,
                },
            ],
        );
        let look = clip(
            n,
            2.4,
            LoopMode::Repeat,
            vec![
                rot(
                    3,
                    &[
                        (0., Quat::IDENTITY),
                        (0.6, y(0.7)),
                        (1.2, Quat::IDENTITY),
                        (1.8, y(-0.7)),
                        (2.4, Quat::IDENTITY),
                    ],
                ),
                rot(5, &[(0., z(0.1) * x(-0.4)), (2.4, z(0.1) * x(-0.4))]),
                rot(4, &[(0., x(-0.2)), (2.4, x(-0.2))]),
                swing(2, 2.4, y(0.15), y(-0.15)),
            ],
            vec![],
        );
        let puff = clip(
            n,
            0.8,
            LoopMode::Repeat,
            vec![
                rot(2, &[(0., x(0.6)), (0.4, x(0.65)), (0.8, x(0.6))]),
                rot(4, &[(0., x(-0.7)), (0.8, x(-0.7))]),
                rot(5, &[(0., x(-0.7)), (0.8, x(-0.7))]),
                pos(
                    1,
                    &[(0., hips(-0.12)), (0.4, hips(-0.1)), (0.8, hips(-0.12))],
                ),
                rot(6, &[(0., x(-0.3)), (0.8, x(-0.3))]),
                rot(7, &[(0., x(-0.3)), (0.8, x(-0.3))]),
            ],
            vec![],
        );
        let bow = clip(
            n,
            1.4,
            LoopMode::Repeat,
            vec![
                rot(
                    2,
                    &[
                        (0., Quat::IDENTITY),
                        (0.5, x(0.9)),
                        (0.9, x(0.9)),
                        (1.4, Quat::IDENTITY),
                    ],
                ),
                rot(
                    5,
                    &[
                        (0., Quat::IDENTITY),
                        (0.5, x(-0.6) * z(-0.3)),
                        (1.4, Quat::IDENTITY),
                    ],
                ),
            ],
            vec![],
        );
        let dance = clip(
            n,
            0.8,
            LoopMode::Repeat,
            vec![
                swing(2, 0.8, z(0.2), z(-0.2)),
                swing(4, 0.8, z(-1.8), z(-0.6)),
                swing(5, 0.8, z(0.6), z(1.8)),
                pos(
                    1,
                    &[
                        (0., hips(0.)),
                        (0.2, hips(0.07)),
                        (0.4, hips(0.)),
                        (0.6, hips(0.07)),
                        (0.8, hips(0.)),
                    ],
                ),
                swing(3, 0.8, z(0.15), z(-0.15)),
            ],
            vec![],
        );
        let water = clip(
            n,
            1.6,
            LoopMode::Repeat,
            vec![
                swing(5, 1.6, x(-1.3) * y(0.3), x(-1.2) * y(-0.3)),
                rot(4, &[(0., x(-0.2)), (1.6, x(-0.2))]),
                rot(2, &[(0., x(0.15)), (1.6, x(0.15))]),
                rot(3, &[(0., x(0.2)), (1.6, x(0.2))]),
            ],
            vec![],
        );
        let bank = AnimationBank::new(
            bind,
            vec![
                idle,
                gait(0.95, 0.45, 0.35, 0.03, 0.03),
                gait(0.55, 0.8, 0.7, 0.18, 0.07),
                sit,
                conduct,
                work,
                cheer,
                startled,
                carry,
                look,
                puff,
                bow,
                dance,
                water,
            ],
        )
        .expect("biped bank");
        Self { skeleton, bank }
    }
}

pub struct BipedMotion {
    animator: Animator,
    clip: usize,
    world: [[f32; 16]; B_JOINTS],
    pub matrices: [[f32; 16]; B_JOINTS],
    pub palette: SkinPalette,
    pub events: EventBuffer<8>,
}

pub fn biped_clip(pose: Pose) -> usize {
    match pose {
        Pose::Idle => B_IDLE,
        Pose::Walk => B_WALK,
        Pose::Run => B_RUN,
        Pose::Sit => B_SIT,
        Pose::Look => B_LOOK,
        Pose::Carry => B_CARRY,
        Pose::Startled => B_STARTLED,
        Pose::Cheer => B_CHEER,
        Pose::Puff => B_PUFF,
        Pose::Act(act) => match act {
            Act::Conduct => B_CONDUCT,
            Act::Bow => B_BOW,
            Act::Dance | Act::Play => B_DANCE,
            Act::Water | Act::Feed => B_WATER,
            Act::Sit => B_SIT,
            Act::Chat | Act::Snack => B_IDLE,
            _ => B_WORK,
        },
    }
}

impl BipedMotion {
    pub fn new(rig: &BipedRig) -> Self {
        Self {
            animator: Animator::new(&rig.bank, B_IDLE).expect("idle clip"),
            clip: B_IDLE,
            world: [IDENTITY; B_JOINTS],
            matrices: [IDENTITY; B_JOINTS],
            palette: SkinPalette::new(&[IDENTITY; B_JOINTS]).expect("palette"),
            events: EventBuffer::default(),
        }
    }

    pub fn update(&mut self, rig: &BipedRig, kind: Kind, pose: Pose, speed: f32, dt: f32) {
        let mut clip = biped_clip(pose);
        if clip == B_WALK && speed > 2.6 {
            clip = B_RUN;
        }
        if clip != self.clip {
            let fade = if clip == B_STARTLED { 0.06 } else { 0.22 };
            if clip == B_STARTLED {
                let _ = self.animator.play(clip);
            } else {
                let _ = self.animator.transition(clip, fade);
            }
            self.clip = clip;
        }
        let rate = match clip {
            B_WALK => (speed / 1.4).clamp(0.6, 1.8),
            B_RUN => (speed / 3.8).clamp(0.7, 1.5),
            B_CARRY => (speed / 1.8).clamp(0.5, 1.4),
            _ => 1.,
        } * if kind == Kind::Pip { 1.25 } else { 1. };
        let _ = self.animator.set_speed(rate);
        self.events.clear();
        let _ = self.animator.advance(dt.clamp(0., 0.1), &mut self.events);
        let _ = rig.skeleton.skin_palette_into(
            self.animator.pose(),
            &mut self.world,
            &mut self.matrices,
        );
        let _ = self.palette.set(&self.matrices);
    }

    pub fn steps(&self) -> usize {
        self.events.hits().iter().filter(|e| e.id == STEP).count()
    }

    pub fn head(&self) -> Mat4 {
        Mat4::from_cols_array(&self.matrices[3])
    }
}

/// Model matrix for a character at `pos` facing `yaw` (0 = -Z) with `scale`.
pub fn model(pos: Vec3, yaw: f32, scale: f32) -> Mat4 {
    Mat4::from_scale_rotation_translation(Vec3::splat(scale), Quat::from_rotation_y(-yaw), pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rigs_build_and_the_graph_follows_parameters() {
        let rig = QuadRig::new();
        let mut m = QuadMotion::new(&rig);
        m.update(
            &rig,
            QuadInput {
                speed: 5.,
                grounded: true,
                ..Default::default()
            },
            0.1,
        );
        m.update(
            &rig,
            QuadInput {
                speed: 5.,
                grounded: true,
                ..Default::default()
            },
            0.5,
        );
        assert_eq!(m.player.state(), Q_TROT);
        m.update(
            &rig,
            QuadInput {
                grounded: false,
                ..Default::default()
            },
            0.1,
        );
        assert_eq!(m.player.state(), Q_AIR);
        let biped = BipedRig::new();
        let mut b = BipedMotion::new(&biped);
        for _ in 0..30 {
            b.update(&biped, Kind::Nana, Pose::Walk, 1.4, 1. / 30.);
        }
        assert!(b.steps() <= 2);
        assert!(b.matrices.iter().flatten().all(|v| v.is_finite()));
    }
}
