//! Rigid-body park: static colliders, prank props and joint mechanisms.
//!
//! Everything a raccoon can bump, push, carry or launch is a real body in the
//! first-party `ferravex-physics` world. The seesaw, the village bell and the
//! swinging sign are hinge joints; the bunting is a chain of distance joints
//! that sways in a deterministic breeze. Carried props leave every collision
//! layer and are teleported to their holder each tick.
use super::park::{DecoKind, Park, PropKind, Role};
use super::{Event, Game, L_DECOR, L_OCCLUDE, L_PROP, L_STATIC, Surface, flat};
use ferravex_physics::JointMotor;
use ferravex_physics::RigidBodyHandle;
use ferravex_physics::d3::{
    ColliderBuilder, JointBuilder, PhysicsWorld, RigidBodyBuilder, RigidBodyPose, TeleportOptions,
    VelocityPolicy,
};
use ferravex_render::prelude::{Quat, Vec3, vec3};

pub const MAX_PROPS: usize = 32;
pub const MAX_LINKS: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    None,
    Raccoon,
    Folk(usize),
    /// The folded blanket riding on the tea tray.
    Tray(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heft {
    Light,
    Medium,
    Heavy,
}

#[derive(Clone, Copy, Debug)]
pub struct Prop {
    pub kind: PropKind,
    pub body: RigidBodyHandle,
    pub home: Vec3,
    pub home_rot: Quat,
    pub owner: Option<usize>,
    pub holder: Holder,
    /// Owners stop returning a prop once its prank is done.
    pub settled: bool,
    pub pos: Vec3,
    pub rot: Quat,
    pub prev_pos: Vec3,
    pub prev_rot: Quat,
    pub vel: Vec3,
    /// Box half extents; balls store their radius in every axis.
    pub half: Vec3,
    pub ball: bool,
    /// The tea tray carries the folded picnic blanket.
    pub loaded: bool,
    pub stashed: bool,
    pub splash_cooldown: f32,
    /// Seconds since the prop was last moved by anyone (for owner noticing).
    pub still: f32,
}

impl Prop {
    pub fn heft(&self) -> Heft {
        match self.kind {
            PropKind::Melon | PropKind::Crate | PropKind::Cake => Heft::Heavy,
            PropKind::Marrow | PropKind::Tray | PropKind::Teapot => Heft::Medium,
            _ => Heft::Light,
        }
    }
    pub fn displaced(&self) -> bool {
        flat(self.pos - self.home).length() > 1.0 || (self.pos.y - self.home.y).abs() > 0.6
    }
    pub fn radius(&self) -> f32 {
        if self.ball {
            self.half.x
        } else {
            self.half.x.max(self.half.z)
        }
    }
    pub fn surface(&self) -> Surface {
        match self.kind {
            PropKind::Crate | PropKind::Tray => Surface::Wood,
            PropKind::Dish | PropKind::Teapot => Surface::Crockery,
            PropKind::Melon | PropKind::Apple | PropKind::Marrow | PropKind::Pinecone => {
                Surface::Fruit
            }
            _ => Surface::Soft,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub body: RigidBodyHandle,
    pub pos: Vec3,
    pub prev: Vec3,
    pub string: u8,
}

/// Jointed set pieces with their live poses for rendering.
pub struct Mechanisms {
    pub seesaw: RigidBodyHandle,
    pub seesaw_pivot: Vec3,
    pub seesaw_rot: Quat,
    pub seesaw_prev: Quat,
    pub bell: RigidBodyHandle,
    pub bell_pivot: Vec3,
    pub bell_rot: Quat,
    pub bell_prev: Quat,
    pub sign: RigidBodyHandle,
    pub sign_pivot: Vec3,
    pub sign_rot: Quat,
    pub sign_prev: Quat,
    pub links: Vec<Link>,
    pub anchors: Vec<(Vec3, u8)>,
}

pub const SEESAW_HALF: Vec3 = Vec3::new(2.2, 0.06, 0.27);
pub const SEESAW_LIMIT: f32 = 0.25;

fn static_box(world: &mut PhysicsWorld, center: Vec3, half: Vec3, rotation: Quat, occlude: bool) {
    let membership = L_STATIC | if occlude { L_OCCLUDE } else { 0 };
    world
        .insert_compound(
            RigidBodyBuilder::fixed()
                .translation(center)
                .rotation(rotation.to_scaled_axis()),
            [ColliderBuilder::cuboid(half.x, half.y, half.z)
                .friction(0.7)
                .collision_layers(membership, L_PROP)],
        )
        .expect("static collider");
}

fn shape(kind: PropKind) -> (Vec3, bool, f32) {
    // (half extents or radius, ball?, density)
    match kind {
        PropKind::Pie => (vec3(0.22, 0.05, 0.22), false, 62.),
        PropKind::Marrow => (vec3(0.32, 0.11, 0.11), false, 64.),
        PropKind::Pinecone => (Vec3::splat(0.11), true, 36.),
        PropKind::Sheet => (vec3(0.2, 0.012, 0.15), false, 35.),
        PropKind::Tray => (vec3(0.34, 0.025, 0.24), false, 50.),
        PropKind::Blanket => (vec3(0.25, 0.08, 0.2), false, 19.),
        PropKind::Melon => (Vec3::splat(0.27), true, 48.),
        PropKind::Cushion => (vec3(0.17, 0.04, 0.17), false, 22.),
        PropKind::Crate => (Vec3::splat(0.35), false, 9.),
        PropKind::Cake => (vec3(0.35, 0.12, 0.35), false, 16.),
        PropKind::Dish => (vec3(0.16, 0.025, 0.16), false, 78.),
        PropKind::Teapot => (vec3(0.12, 0.1, 0.1), false, 83.),
        PropKind::Jelly => (vec3(0.13, 0.1, 0.13), false, 37.),
        PropKind::Apple => (Vec3::splat(0.07), true, 140.),
    }
}

fn friction(kind: PropKind) -> f32 {
    match kind {
        PropKind::Tray => 0.012,
        PropKind::Melon | PropKind::Apple | PropKind::Pinecone => 0.5,
        PropKind::Crate => 0.65,
        _ => 0.55,
    }
}

pub fn build(park: &Park, world: &mut PhysicsWorld) -> (Vec<Prop>, Mechanisms) {
    // Ground: one generous slab whose top is y = 0.
    static_box(
        world,
        vec3(0., -0.5, 0.),
        vec3(40., 0.5, 40.),
        Quat::IDENTITY,
        false,
    );
    for s in park.solids.iter().chain(&park.ramps) {
        static_box(world, s.center, s.half, s.rotation, s.occlude);
    }
    for t in &park.tables {
        let top = t.height - t.top.y * 0.5;
        let mut parts = vec![
            ColliderBuilder::cuboid(t.top.x * 0.5, t.top.y * 0.5, t.top.z * 0.5)
                .translation(vec3(0., top, 0.))
                .friction(0.7)
                .collision_layers(L_STATIC, L_PROP),
        ];
        let leg = (t.height - t.top.y) * 0.5;
        for (sx, sz) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
            parts.push(
                ColliderBuilder::cuboid(0.04, leg, 0.04)
                    .translation(vec3(
                        sx * (t.top.x * 0.5 - 0.14),
                        leg,
                        sz * (t.top.z * 0.5 - 0.1),
                    ))
                    .collision_layers(L_STATIC, L_PROP),
            );
        }
        world
            .insert_compound(RigidBodyBuilder::fixed().translation(t.center), parts)
            .expect("table collider");
    }
    for tree in &park.trees {
        let r = 0.22 * tree.scale;
        static_box(
            world,
            tree.at + Vec3::Y * 1.6,
            vec3(r, 1.6, r),
            Quat::IDENTITY,
            true,
        );
    }
    // Bell arch posts, crossbeam and signpost.
    let arch = park.deco(DecoKind::BellArch);
    for x in [-1.4, 1.4] {
        static_box(
            world,
            arch.at + vec3(x, 1.65, 0.),
            vec3(0.12, 1.65, 0.12),
            Quat::IDENTITY,
            false,
        );
    }
    let sign = park.deco(DecoKind::Sign);
    static_box(
        world,
        sign.at + vec3(0., 1.35, 0.),
        vec3(0.07, 1.35, 0.07),
        Quat::IDENTITY,
        false,
    );
    for (p, h) in &park.poles {
        static_box(
            world,
            *p + Vec3::Y * *h * 0.5,
            vec3(0.06, *h * 0.5, 0.06),
            Quat::IDENTITY,
            false,
        );
    }
    // Seesaw: a dynamic plank with end stops, hinged to a fixed pivot.
    let seesaw_deco = park.deco(DecoKind::Seesaw);
    let seesaw_pivot = seesaw_deco.at + Vec3::Y * 0.6;
    let anchor = world
        .insert_body(RigidBodyBuilder::fixed().translation(seesaw_pivot))
        .expect("seesaw pivot");
    let tilt = Quat::from_rotation_z(SEESAW_LIMIT);
    let h = SEESAW_HALF;
    let seesaw = world
        .insert_compound(
            RigidBodyBuilder::dynamic()
                .translation(seesaw_pivot)
                .rotation(tilt.to_scaled_axis())
                .angular_damping(0.4),
            [
                ColliderBuilder::cuboid(h.x, h.y, h.z)
                    .density(8.)
                    .friction(0.8)
                    .collision_layers(L_PROP, L_STATIC | L_PROP),
                ColliderBuilder::cuboid(0.05, 0.07, h.z)
                    .translation(vec3(-h.x + 0.05, h.y + 0.07, 0.))
                    .density(4.)
                    .collision_layers(L_PROP, L_STATIC | L_PROP),
                ColliderBuilder::cuboid(0.05, 0.07, h.z)
                    .translation(vec3(h.x - 0.05, h.y + 0.07, 0.))
                    .density(4.)
                    .collision_layers(L_PROP, L_STATIC | L_PROP),
            ],
        )
        .expect("seesaw plank");
    world
        .insert_joint(
            anchor,
            seesaw,
            JointBuilder::hinge()
                .axes(Vec3::Z, Vec3::Z)
                .limits(-SEESAW_LIMIT, SEESAW_LIMIT)
                .contacts_enabled(false),
        )
        .expect("seesaw hinge");
    // Village bell under the arch, free to swing about X.
    let bell_pivot = arch.at + vec3(0., 3.0, 0.);
    let bell_anchor = world
        .insert_body(RigidBodyBuilder::fixed().translation(bell_pivot))
        .expect("bell anchor");
    let bell = world
        .insert_compound(
            RigidBodyBuilder::dynamic()
                .translation(bell_pivot)
                .angular_damping(0.35),
            [ColliderBuilder::cuboid(0.22, 0.24, 0.22)
                .translation(vec3(0., -0.38, 0.))
                .density(120.)
                .collision_layers(L_DECOR, 0)],
        )
        .expect("bell");
    world
        .insert_joint(
            bell_anchor,
            bell,
            JointBuilder::hinge()
                .axes(Vec3::X, Vec3::X)
                .contacts_enabled(false),
        )
        .expect("bell hinge");
    // Sign board hanging from its arm, with a gentle return motor.
    let sign_pivot = sign.at + vec3(0.75, 2.55, 0.);
    let sign_anchor = world
        .insert_body(RigidBodyBuilder::fixed().translation(sign_pivot))
        .expect("sign anchor");
    let sign_body = world
        .insert_compound(
            RigidBodyBuilder::dynamic()
                .translation(sign_pivot)
                .angular_damping(0.6),
            [ColliderBuilder::cuboid(0.72, 0.3, 0.035)
                .translation(vec3(0., -0.42, 0.))
                .density(60.)
                .collision_layers(L_DECOR, 0)],
        )
        .expect("sign board");
    world
        .insert_joint(
            sign_anchor,
            sign_body,
            JointBuilder::hinge()
                .axes(Vec3::X, Vec3::X)
                .limits(-0.9, 0.9)
                .motor(JointMotor::position(0., 0.6, 1.5, 0.6))
                .contacts_enabled(false),
        )
        .expect("sign hinge");
    // Bunting: distance-joint chains between pole tops.
    let mut links = Vec::with_capacity(MAX_LINKS);
    let mut anchors = Vec::new();
    for (string, b) in park.bunting.iter().enumerate() {
        let string = string as u8;
        let start = world
            .insert_body(RigidBodyBuilder::fixed().translation(b.from))
            .expect("bunting anchor");
        let end = world
            .insert_body(RigidBodyBuilder::fixed().translation(b.to))
            .expect("bunting anchor");
        anchors.push((b.from, string));
        anchors.push((b.to, string));
        let n = b.flags.min(MAX_LINKS / park.bunting.len().max(1));
        let mut previous = start;
        let mut previous_pos = b.from;
        for k in 0..n {
            let t = (k + 1) as f32 / (n + 1) as f32;
            let sag = 0.9 * 4. * t * (1. - t);
            let p = b.from.lerp(b.to, t) - Vec3::Y * sag;
            let body = world
                .insert_compound(
                    RigidBodyBuilder::dynamic()
                        .translation(p)
                        .linear_damping(1.2)
                        .angular_damping(2.),
                    [ColliderBuilder::ball(0.05)
                        .density(60.)
                        .collision_layers(L_DECOR, 0)],
                )
                .expect("bunting link");
            world
                .insert_joint(
                    previous,
                    body,
                    JointBuilder::distance(previous_pos.distance(p) * 1.01).contacts_enabled(false),
                )
                .expect("bunting joint");
            links.push(Link {
                body,
                pos: p,
                prev: p,
                string,
            });
            previous = body;
            previous_pos = p;
        }
        world
            .insert_joint(
                previous,
                end,
                JointBuilder::distance(previous_pos.distance(b.to) * 1.01).contacts_enabled(false),
            )
            .expect("bunting joint");
    }
    let props = park
        .props
        .iter()
        .map(|spec| {
            let (half, ball, density) = shape(spec.kind);
            let collider = if ball {
                ColliderBuilder::ball(half.x)
            } else {
                ColliderBuilder::cuboid(half.x, half.y, half.z)
            }
            .density(density)
            .friction(friction(spec.kind))
            .restitution(if spec.kind == PropKind::Apple {
                0.3
            } else {
                0.05
            })
            .collision_layers(L_PROP, L_STATIC | L_PROP);
            let mut parts = vec![collider];
            if spec.kind == PropKind::Cake {
                for (y, r) in [(0.21, 0.25), (0.37, 0.15)] {
                    parts.push(
                        ColliderBuilder::cuboid(r, 0.08, r)
                            .translation(vec3(0., y, 0.))
                            .density(density)
                            .collision_layers(L_PROP, L_STATIC | L_PROP),
                    );
                }
            }
            let body = world
                .insert_compound(
                    RigidBodyBuilder::dynamic()
                        .translation(spec.at)
                        .rotation(spec.rotation.to_scaled_axis())
                        .linear_damping(match spec.kind {
                            PropKind::Tray => 0.35,
                            _ if ball => 0.25,
                            _ => 0.08,
                        })
                        .angular_damping(if ball { 0.6 } else { 0.3 }),
                    parts,
                )
                .expect("prop body");
            Prop {
                kind: spec.kind,
                body,
                home: spec.at,
                home_rot: spec.rotation,
                owner: spec.owner,
                holder: Holder::None,
                settled: false,
                pos: spec.at,
                rot: spec.rotation,
                prev_pos: spec.at,
                prev_rot: spec.rotation,
                vel: Vec3::ZERO,
                half,
                ball,
                loaded: false,
                stashed: false,
                splash_cooldown: 0.,
                still: 0.,
            }
        })
        .collect();
    (
        props,
        Mechanisms {
            seesaw,
            seesaw_pivot,
            seesaw_rot: tilt,
            seesaw_prev: tilt,
            bell,
            bell_pivot,
            bell_rot: Quat::IDENTITY,
            bell_prev: Quat::IDENTITY,
            sign: sign_body,
            sign_pivot,
            sign_rot: Quat::IDENTITY,
            sign_prev: Quat::IDENTITY,
            links,
            anchors,
        },
    )
}

pub fn remember_poses(game: &mut Game) {
    for p in &mut game.props {
        p.prev_pos = p.pos;
        p.prev_rot = p.rot;
    }
    let m = &mut game.mech;
    m.seesaw_prev = m.seesaw_rot;
    m.bell_prev = m.bell_rot;
    m.sign_prev = m.sign_rot;
    for l in &mut m.links {
        l.prev = l.pos;
    }
}

pub fn sync_poses(game: &mut Game, snap: bool) {
    for p in &mut game.props {
        let b = &game.world.bodies[p.body];
        let pos = b.translation();
        let moved = pos.distance(p.pos) > 0.004;
        p.pos = pos;
        p.rot = *b.rotation();
        p.vel = b.linvel();
        p.still = if moved || p.holder != Holder::None {
            0.
        } else {
            p.still + super::DT
        };
        if snap {
            p.prev_pos = p.pos;
            p.prev_rot = p.rot;
        }
    }
    let w = &game.world;
    let m = &mut game.mech;
    m.seesaw_rot = *w.bodies[m.seesaw].rotation();
    m.bell_rot = *w.bodies[m.bell].rotation();
    m.sign_rot = *w.bodies[m.sign].rotation();
    for l in &mut m.links {
        l.pos = w.bodies[l.body].translation();
    }
    if snap {
        m.seesaw_prev = m.seesaw_rot;
        m.bell_prev = m.bell_rot;
        m.sign_prev = m.sign_rot;
        for l in &mut m.links {
            l.prev = l.pos;
        }
    }
}

/// A gentle, deterministic afternoon breeze for bunting and the sign.
pub fn wind(game: &mut Game, time: f32) {
    let gust = 0.55 + 0.45 * (time * 0.37).sin() * (time * 0.23 + 1.1).sin();
    let dir = vec3((time * 0.05).cos(), 0., 0.35 + 0.3 * (time * 0.11).sin());
    game.wind = dir * gust;
    for (i, l) in game.mech.links.iter().enumerate() {
        let flutter = (time * 2.7 + i as f32 * 0.9).sin() * 0.35;
        let force = (game.wind + vec3(0., 0., flutter)) * 0.25;
        game.world.bodies[l.body].add_force(force, true);
    }
    let sign = game.mech.sign;
    let swing = (game.wind.z * 0.6 + (time * 1.3).sin() * 0.25) * 2.2;
    game.world.bodies[sign].add_torque(vec3(swing, 0., 0.), true);
}

/// Pose of a prop held by someone, from the holder's anchor point and heading.
pub fn hold_pose(anchor: Vec3, yaw: f32) -> RigidBodyPose {
    RigidBodyPose {
        translation: anchor,
        rotation: Quat::from_rotation_y(-yaw),
    }
}

pub fn set_carried(world: &mut PhysicsWorld, body: RigidBodyHandle, carried: bool) {
    let children: [Option<ferravex_physics::ColliderHandle>; 4] = {
        let list = world.bodies[body].colliders();
        std::array::from_fn(|i| list.get(i).copied())
    };
    for c in children.into_iter().flatten() {
        let collider = &mut world.colliders[c];
        if carried {
            collider.set_collision_layers(0, 0);
        } else {
            collider.set_collision_layers(L_PROP, L_STATIC | L_PROP);
        }
    }
    let b = &mut world.bodies[body];
    b.set_gravity_scale(if carried { 0. } else { 1. }, true);
    b.wake_up(true);
}

pub fn place(game: &mut Game, prop: usize, pose: RigidBodyPose, velocity: Vec3) {
    let body = game.props[prop].body;
    let _ = game
        .world
        .try_teleport_body(body, pose, TeleportOptions::new(VelocityPolicy::Clear));
    game.world.bodies[body].set_linvel(velocity, true);
    let p = &mut game.props[prop];
    p.pos = pose.translation;
    p.rot = pose.rotation;
}

/// Keep held props glued to their holder's anchor.
pub fn hold_carried(game: &mut Game) {
    for i in 0..game.props.len() {
        let anchor = match game.props[i].holder {
            Holder::None => continue,
            Holder::Raccoon => {
                let lift = game.props[i].half.y;
                (game.coon.carry_anchor(lift), game.coon.yaw)
            }
            Holder::Folk(f) => {
                let folk = &game.folk[f];
                (folk.hands(), folk.yaw)
            }
            Holder::Tray(t) => {
                let tray = &game.props[t];
                let up = tray.rot * Vec3::Y;
                (
                    tray.pos + up * (tray.half.y + 0.05),
                    super::yaw_of(tray.rot * Vec3::NEG_Z),
                )
            }
        };
        let pose = hold_pose(anchor.0, anchor.1);
        let body = game.props[i].body;
        let _ =
            game.world
                .try_teleport_body(body, pose, TeleportOptions::new(VelocityPolicy::Clear));
        let p = &mut game.props[i];
        p.pos = pose.translation;
        p.rot = pose.rotation;
        p.vel = Vec3::ZERO;
    }
}

/// Pond dunks, stash checks and recoveries that apply to every prop.
pub fn rules(game: &mut Game) {
    let den = game.park.dens[0];
    for i in 0..game.props.len() {
        let p = game.props[i];
        game.props[i].splash_cooldown = (p.splash_cooldown - super::DT).max(0.);
        game.props[i].stashed = p.holder == Holder::None && flat(p.pos - den).length() < 2.4;
        let lost = p.pos.y < -3. || p.pos.x.abs() > 34. || p.pos.z.abs() > 34.;
        if p.holder == Holder::None && (game.park.in_pond(p.pos) || lost) {
            if p.splash_cooldown <= 0. && !lost {
                game.emit(Event::Splash {
                    at: p.pos,
                    big: p.heft() != Heft::Light,
                });
                super::pranks::splash(game, i);
            }
            // The pond (or the void) politely returns things home after a dunk.
            let home = RigidBodyPose {
                translation: p.home + Vec3::Y * 0.05,
                rotation: p.home_rot,
            };
            place(game, i, home, Vec3::ZERO);
            game.props[i].splash_cooldown = 1.;
            game.props[i].loaded = false;
        }
    }
    let rope = game.park.zone(Role::Rope);
    let _ = rope;
}

/// Turn solver impacts into bump events with a game surface.
pub fn impacts(game: &mut Game) {
    let mut found = [(Vec3::ZERO, 0_f32, Surface::Wood); 6];
    let mut n = 0;
    for record in game.world.impacts() {
        if n == found.len() {
            break;
        }
        let surface = game
            .props
            .iter()
            .find(|p| {
                let b = &game.world.bodies[p.body];
                b.colliders().contains(&record.collider1)
                    || b.colliders().contains(&record.collider2)
            })
            .map_or(Surface::Wood, Prop::surface);
        found[n] = (record.point, record.peak_normal_impulse, surface);
        n += 1;
    }
    for &(at, strength, surface) in &found[..n] {
        game.emit(Event::Bump {
            at,
            strength,
            surface,
        });
    }
}

pub fn find(game: &Game, kind: PropKind) -> usize {
    game.props
        .iter()
        .position(|p| p.kind == kind)
        .expect("validated unique prop")
}

pub fn body_prop(game: &Game, body: RigidBodyHandle) -> Option<usize> {
    game.props.iter().position(|p| p.body == body)
}
