//! The raccoon: a small, very determined capsule on the shared character motor.
//!
//! Verbs: run, sneak, jump, grab / drop / place, throw, chitter, belly-slide,
//! push, hide (bushes, under tables, in the basket) and the two hold-to-use
//! interactions (the bell rope and, eventually, the banquet tablecloth).
use super::park::{PropKind, Role};
use super::props::{Heft, Holder, hold_pose, place, set_carried};
use super::{Event, Game, L_PROP, L_STATIC, flat, heading, yaw_of};
use ferravex_physics::character::{Character, CharacterInput, CharacterSettings, CharacterShape};
use ferravex_physics::d3::{QueryOptions, RigidBodyPose};
use ferravex_play::gameplay::interaction::{Interaction, InteractionInput};
use ferravex_play::gameplay::inventory::{Inventory, Stack};
use ferravex_play::gameplay::pickup::Magnet;
use ferravex_render::prelude::{Vec2, Vec3, vec2, vec3};

/// Inventory item id for shiny bottle caps.
pub const SHINY: u32 = 1;
pub const HEIGHT: f32 = 0.6;
pub const CROUCH: f32 = 0.34;
pub const RADIUS: f32 = 0.24;
pub const RUN: f32 = 5.4;
pub const SNEAK: f32 = 2.1;
pub const SLIDE_TIME: f32 = 0.85;
pub const SLIDE_SPEED: f32 = 8.6;
pub const WRIGGLES: u8 = 6;
/// Pretend raccoon mass for weight and stomps on dynamic supports.
pub const MASS: f32 = 6.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hide {
    None,
    Bush(usize),
    Table(usize),
    Basket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    Bell,
    Cloth,
}

/// What the context button would do right now (for prompts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prompt {
    None,
    Grab(usize),
    Drop(usize),
    Place(usize),
    Load,
    Basket,
    LeaveBasket,
    Ring,
    Yank,
    Wriggle,
}

pub struct Raccoon {
    pub motor: Character,
    pub shape: CharacterShape,
    pub settings: CharacterSettings,
    pub yaw: f32,
    pub prev_pos: Vec3,
    pub prev_yaw: f32,
    pub held: Option<usize>,
    pub hide: Hide,
    pub slide: f32,
    pub slide_dir: Vec3,
    pub slide_from: Vec3,
    pub chitter_cd: f32,
    /// Seconds since the last chitter (animation and bubbles).
    pub chitter_age: f32,
    pub scruffed: Option<usize>,
    pub wriggle: u8,
    pub stun: f32,
    pub grace: f32,
    pub using: Interaction,
    pub use_target: Option<Use>,
    pub was_grounded: bool,
    pub fall_speed: f32,
    pub air_time: f32,
    pub speed: f32,
    pub sneaking: bool,
    pub cape: bool,
    pub pouch: Inventory<4>,
    pub celebrate: f32,
    pub prompt: Prompt,
    pub support_body: Option<ferravex_physics::RigidBodyHandle>,
    pub basket_peek: f32,
    /// Mid belly-flop (airborne dive); cleared on landing.
    pub flop: bool,
}

fn settings() -> CharacterSettings {
    CharacterSettings {
        speed: RUN,
        ground_acceleration: 42.,
        air_acceleration: 14.,
        gravity: 22.,
        jump_speed: 7.4,
        step_height: 0.26,
        ground_snap: 0.12,
        membership_mask: L_STATIC | L_PROP,
        max_candidates: 4096,
        coyote_time: 0.12,
        jump_buffer: 0.14,
        ..CharacterSettings::default()
    }
}

impl Raccoon {
    pub fn new(park: &super::park::Park, world: &ferravex_physics::d3::PhysicsWorld) -> Self {
        let shape = CharacterShape::Capsule {
            radius: RADIUS,
            height: HEIGHT,
        };
        let settings = settings();
        let mut motor = Character::new(park.spawn + Vec3::Y * 0.01);
        let _ = motor.teleport(world, shape, settings, park.spawn + Vec3::Y * 0.01);
        let yaw = yaw_of(vec3(0.6, 0., -1.));
        Self {
            motor,
            shape,
            settings,
            yaw,
            prev_pos: motor.position,
            prev_yaw: yaw,
            held: None,
            hide: Hide::None,
            slide: 0.,
            slide_dir: Vec3::ZERO,
            slide_from: Vec3::ZERO,
            chitter_cd: 0.,
            chitter_age: 10.,
            scruffed: None,
            wriggle: 0,
            stun: 0.,
            grace: 0.,
            using: Interaction::new(0.6).expect("authored hold"),
            use_target: None,
            was_grounded: true,
            fall_speed: 0.,
            air_time: 0.,
            speed: 0.,
            sneaking: false,
            cape: false,
            pouch: Inventory::new().expect("small pouch"),
            celebrate: 0.,
            prompt: Prompt::None,
            support_body: None,
            basket_peek: 0.,
            flop: false,
        }
    }

    pub fn pos(&self) -> Vec3 {
        self.motor.position
    }

    /// Mouth/arms anchor for a held prop whose bottom sits `lift` below center.
    pub fn carry_anchor(&self, lift: f32) -> Vec3 {
        let forward = heading(self.yaw);
        let reach = if self.slide > 0. { 0.32 } else { 0.38 };
        self.motor.position + forward * reach + Vec3::Y * (0.5 + lift.min(0.3))
    }

    pub fn hidden(&self) -> bool {
        self.hide != Hide::None
    }

    /// Can picnickers currently notice the raccoon at all?
    pub fn visible_to_folk(&self) -> bool {
        self.scruffed.is_none() && self.grace <= 0. && self.hide != Hide::Basket
    }
}

pub fn begin_tick(game: &mut Game) {
    let c = &mut game.coon;
    c.prev_pos = c.motor.position;
    c.prev_yaw = c.yaw;
    c.chitter_cd = (c.chitter_cd - super::DT).max(0.);
    c.chitter_age += super::DT;
    c.stun = (c.stun - super::DT).max(0.);
    c.grace = (c.grace - super::DT).max(0.);
    c.celebrate = (c.celebrate - super::DT).max(0.);
}

fn grab_candidate(game: &Game) -> Option<usize> {
    let c = &game.coon;
    let feet = c.pos();
    let forward = heading(c.yaw);
    let mut best = None;
    let mut best_score = f32::INFINITY;
    for (i, p) in game.props.iter().enumerate() {
        if p.holder != Holder::None {
            continue;
        }
        let d = flat(p.pos - feet);
        let dy = p.pos.y - feet.y;
        let reach = 0.75 + p.radius();
        if d.length() > reach || !(-0.45..=1.3).contains(&dy) {
            continue;
        }
        // Prefer what is in front of the nose.
        let facing = d.normalize_or_zero().dot(forward);
        let score = d.length() - facing * 0.35;
        if facing > -0.4 && score < best_score {
            best_score = score;
            best = Some(i);
        }
    }
    best
}

fn use_candidate(game: &Game) -> Option<Use> {
    let feet = game.coon.pos();
    let rope = game.park.zone(Role::Rope);
    if flat(feet - rope.at).length() < rope.radius && feet.y < 0.8 {
        return Some(Use::Bell);
    }
    let cloth = game.park.zone(Role::Cloth);
    if game.todo.finale_open()
        && !game.coon.cape
        && flat(feet - cloth.at).length() < cloth.radius
        && feet.y < 0.8
    {
        return Some(Use::Cloth);
    }
    None
}

fn near_basket(game: &Game) -> bool {
    let b = game.park.zone(Role::Basket);
    flat(game.coon.pos() - b.at).length() < b.radius && game.coon.pos().y < 0.5
}

fn tray_near(game: &Game, at: Vec3) -> Option<usize> {
    game.props.iter().position(|p| {
        p.kind == PropKind::Tray
            && p.holder == Holder::None
            && !p.loaded
            && flat(p.pos - at).length() < 1.0
            && (p.pos.y - at.y).abs() < 1.2
    })
}

/// Placement for a held prop: on top of whatever surface is in front of the
/// raccoon (tables, benches, the plinth, the seesaw) or simply in front.
fn drop_point(game: &Game, held: usize) -> Vec3 {
    let c = &game.coon;
    let p = &game.props[held];
    let mut front = c.pos() + heading(c.yaw) * (0.5 + p.radius() * 0.6);
    // Aiming near the seesaw? Its friendly little cup pulls the drop onto the plank.
    let pivot = game.mech.seesaw_pivot;
    if (front.z - pivot.z).abs() < 0.65 && (front.x - pivot.x).abs() < 2.1 {
        front.z = pivot.z;
    }
    let probe = front + Vec3::Y * 1.45;
    let options = QueryOptions::new(1.9)
        .layers(L_STATIC | L_PROP)
        .ignore_body(p.body);
    if let Some(hit) = game
        .world
        .raycast_with_options(probe.to_array(), [0., -1., 0.], options)
    {
        let y = hit.point[1];
        if y <= c.pos().y + 1.25 && y >= c.pos().y - 0.6 {
            let mut at = vec3(front.x, y + p.half.y + 0.03, front.z);
            // The seesaw has a friendly little cup at each end: snap to its line.
            if hit.body == Some(game.mech.seesaw) {
                at.z = game.mech.seesaw_pivot.z;
            }
            return at;
        }
    }
    c.carry_anchor(p.half.y)
}

fn release(game: &mut Game, held: usize, at: Vec3, velocity: Vec3) {
    let body = game.props[held].body;
    set_carried(&mut game.world, body, false);
    game.props[held].holder = Holder::None;
    let yaw = game.coon.yaw;
    place(game, held, hold_pose(at, yaw), velocity);
    game.coon.held = None;
}

pub fn drop_held(game: &mut Game, velocity: Vec3) {
    if let Some(held) = game.coon.held {
        let at = game.coon.carry_anchor(game.props[held].half.y);
        release(game, held, at, velocity);
        game.emit(Event::Dropped { prop: held, at });
    }
}

fn grab(game: &mut Game, i: usize) {
    // Taking the blanket off a loaded tray unloads it.
    if game.props[i].kind == PropKind::Blanket && game.props[i].holder == Holder::None {
        for p in &mut game.props {
            if p.kind == PropKind::Tray {
                p.loaded = false;
            }
        }
    }
    let body = game.props[i].body;
    set_carried(&mut game.world, body, true);
    game.props[i].holder = Holder::Raccoon;
    game.coon.held = Some(i);
    game.stats.grabs += 1;
    let at = game.props[i].pos;
    game.emit(Event::Grabbed { prop: i, at });
    super::folk::notice_theft(game, i);
}

pub fn actions(game: &mut Game, input: Input) {
    let c = &game.coon;
    if let Some(f) = c.scruffed {
        if input.jump || input.grab {
            game.coon.wriggle += 1;
            game.stats.wriggles += 1;
            let at = game.coon.pos();
            game.emit(Event::Wriggle {
                at,
                count: game.coon.wriggle,
            });
            if game.coon.wriggle >= WRIGGLES {
                super::folk::wriggle_free(game, f);
            }
        }
        return;
    }
    if c.stun > 0. {
        return;
    }
    if c.hide == Hide::Basket {
        game.coon.basket_peek = (game.coon.basket_peek + super::DT).min(1.);
        if input.grab || input.jump {
            let b = game.park.zone(Role::Basket);
            let out = b.at + heading(game.coon.yaw) * 0.95 + Vec3::Y * 0.02;
            let (world, coon) = (&game.world, &mut game.coon);
            if coon
                .motor
                .teleport(world, coon.shape, coon.settings, out)
                .is_ok()
            {
                coon.hide = Hide::None;
                game.emit(Event::Unhid { at: out });
            }
        }
        return;
    }
    if input.chitter && game.coon.chitter_cd <= 0. {
        game.coon.chitter_cd = 1.1;
        game.coon.chitter_age = 0.;
        game.stats.chitters += 1;
        let at = game.coon.pos();
        game.emit(Event::Chitter { at });
        super::folk::noise(game, at, 9.5, 200);
    }
    if input.throw
        && let Some(held) = game.coon.held
    {
        let c = &game.coon;
        let v = heading(c.yaw) * 6.2 + Vec3::Y * 3.4 + flat(c.motor.velocity) * 0.6;
        let at = c.carry_anchor(game.props[held].half.y) + heading(c.yaw) * 0.1;
        release(game, held, at, v);
        game.stats.throws += 1;
        game.emit(Event::Thrown { prop: held, at });
    }
    // Context button: a held prop is put down first; otherwise rope/cloth,
    // then the nearest grabbable prop, then the picnic basket.
    let target = use_candidate(game);
    if input.grab {
        if let Some(held) = game.coon.held {
            let at = drop_point(game, held);
            if game.props[held].kind == PropKind::Blanket
                && let Some(tray) = tray_near(game, at)
            {
                release(game, held, at, Vec3::ZERO);
                game.props[tray].loaded = true;
                game.props[held].holder = Holder::Tray(tray);
                set_carried(&mut game.world, game.props[held].body, true);
                let at = game.props[tray].pos;
                game.emit(Event::Dropped { prop: held, at });
            } else {
                release(game, held, at, Vec3::ZERO);
                game.emit(Event::Dropped { prop: held, at });
            }
        } else if target.is_none() {
            if let Some(i) = grab_candidate(game) {
                grab(game, i);
            } else if near_basket(game) {
                let b = game.park.zone(Role::Basket);
                game.coon.motor.position = b.at + Vec3::Y * 0.02;
                game.coon.motor.velocity = Vec3::ZERO;
                game.coon.hide = Hide::Basket;
                game.coon.basket_peek = 0.;
                game.stats.hides += 1;
                game.emit(Event::Hid {
                    at: b.at,
                    basket: true,
                });
            }
        }
    }
    // Hold-to-use: the bell rope rings again and again; the cloth yanks once.
    let held_use = target.filter(|_| game.coon.held.is_none());
    let pair = held_use.map(|t| (1_u64, t as u64 + 10));
    let outcome = game.coon.using.step_checked(
        super::DT,
        InteractionInput {
            pair,
            held: input.grab_held || input.grab,
            in_range: held_use.is_some(),
            focused: true,
            permitted: true,
            visible: true,
        },
    );
    game.coon.use_target = held_use;
    if let Ok(o) = outcome
        && o.completed
    {
        match held_use {
            Some(Use::Bell) => {
                super::pranks::ring_bell(game);
                game.coon.using.cancel();
            }
            Some(Use::Cloth) => super::pranks::yank_cloth(game),
            None => {}
        }
    }
    let c = &game.coon;
    let heavy = c.held.is_some_and(|h| game.props[h].heft() == Heft::Heavy);
    if input.slide && !c.motor.grounded && !c.flop && !heavy && c.air_time > 0.08 {
        // Belly-flop: abandon all dignity and dive straight down.
        let coon = &mut game.coon;
        coon.flop = true;
        coon.motor.velocity.y = coon.motor.velocity.y.min(-9.5);
        coon.motor.velocity.x *= 0.35;
        coon.motor.velocity.z *= 0.35;
        let at = coon.pos();
        game.emit(Event::Slide { at });
        return;
    }
    if input.slide && c.slide <= 0. && c.motor.grounded && c.speed > 1.2 && !heavy {
        let dir = if flat(c.motor.velocity).length() > 0.5 {
            flat(c.motor.velocity).normalize()
        } else {
            heading(c.yaw)
        };
        let from = c.pos();
        let coon = &mut game.coon;
        coon.slide = SLIDE_TIME;
        coon.slide_dir = dir;
        coon.slide_from = from;
        let mut shape = coon.shape;
        if coon
            .motor
            .try_set_height(
                &game.world,
                &mut shape,
                CROUCH,
                coon.settings.membership_mask,
                coon.settings.max_candidates,
            )
            .is_ok()
        {
            coon.shape = shape;
        }
        game.stats.slides += 1;
        game.emit(Event::Slide { at: from });
    }
}

use super::Input;

pub fn movement(game: &mut Game, input: Input) {
    if let Some(f) = game.coon.scruffed {
        let hands = game.folk[f].hands() - Vec3::Y * 0.35;
        let c = &mut game.coon;
        c.motor.position = hands;
        c.motor.velocity = Vec3::ZERO;
        c.yaw = game.folk[f].yaw + std::f32::consts::PI;
        c.speed = 0.;
        c.prompt = Prompt::Wriggle;
        return;
    }
    if game.coon.hide == Hide::Basket {
        game.coon.speed = 0.;
        game.coon.prompt = Prompt::LeaveBasket;
        return;
    }
    let playing = game.phase == super::Phase::Playing && game.coon.stun <= 0.;
    let mut m = if playing { input.movement } else { Vec2::ZERO };
    if m.length() > 1. {
        m = m.normalize();
    }
    let forward = heading(input.cam_yaw);
    let right = heading(input.cam_yaw + std::f32::consts::FRAC_PI_2);
    let wish = right * m.x + forward * m.y;
    let amount = m.length();
    // A gentle nudge only turns the raccoon in place; a half press sneaks.
    let turning = amount > 0.02 && amount < 0.2;
    let mut direction = if turning { Vec3::ZERO } else { wish };
    let heft = game.coon.held.map(|h| game.props[h].heft());
    let sneaking = (0.2..0.62).contains(&amount);
    let mut settings = game.coon.settings;
    let mut jump = playing && (input.jump || input.jump_held);
    settings.speed = if sneaking {
        SNEAK / amount.max(0.05)
    } else {
        RUN
    } * match heft {
        Some(Heft::Heavy) => 0.58,
        Some(Heft::Medium) => 0.82,
        _ => 1.,
    };
    if heft == Some(Heft::Heavy) {
        jump = false;
        settings.jump_speed = 0.;
    }
    if game.coon.slide > 0. {
        let t = (game.coon.slide / SLIDE_TIME).clamp(0., 1.);
        direction = game.coon.slide_dir;
        settings.speed = SLIDE_SPEED * (0.35 + 0.65 * t);
        settings.ground_acceleration = 160.;
        jump = false;
        game.coon.slide -= super::DT;
    }
    let before = game.coon.motor.velocity;
    let was_grounded = game.coon.motor.grounded;
    let coon = &mut game.coon;
    let step = coon.motor.step(
        &game.world,
        coon.shape,
        settings,
        CharacterInput { direction, jump },
        super::DT,
    );
    let crouched =
        matches!(game.coon.shape, CharacterShape::Capsule { height, .. } if height < HEIGHT);
    if game.coon.slide <= 0. && crouched {
        let mut shape = game.coon.shape;
        if game
            .coon
            .motor
            .try_set_height(
                &game.world,
                &mut shape,
                HEIGHT,
                game.coon.settings.membership_mask,
                game.coon.settings.max_candidates,
            )
            .is_ok()
        {
            game.coon.shape = shape;
            // A finished belly-slide: how far did that go?
            let distance = flat(game.coon.pos() - game.coon.slide_from).length();
            super::pranks::slide_finished(game, distance);
        }
    }
    if step.is_err() {
        // Wedged by a moving body (a seesaw end, a tumbling crate): recover
        // upward, then back to the last good spot, rather than freezing.
        let here = game.coon.pos();
        let back = game.coon.prev_pos;
        let den = game.park.dens[0];
        let (world, coon) = (&game.world, &mut game.coon);
        for target in [
            here + Vec3::Y * 0.2,
            back + Vec3::Y * 0.35,
            den + Vec3::Y * 0.1,
        ] {
            if coon
                .motor
                .teleport(world, coon.shape, coon.settings, target)
                .is_ok()
            {
                break;
            }
        }
        coon.motor.velocity = Vec3::ZERO;
    }
    if step.is_ok_and(|s| s.jumped) {
        game.stats.jumps += 1;
        let at = game.coon.pos();
        game.emit(Event::Jumped { at });
    }
    let c = &mut game.coon;
    let planar = flat(c.motor.velocity);
    c.speed = planar.length();
    c.sneaking = sneaking && c.slide <= 0.;
    if c.speed > 0.3 && c.slide <= 0. {
        let want = yaw_of(planar);
        c.yaw = super::lerp_angle(c.yaw, want, (14. * super::DT).min(1.));
    } else if amount > 0.02 && c.slide <= 0. {
        let want = yaw_of(wish);
        c.yaw = super::lerp_angle(c.yaw, want, (10. * super::DT).min(1.));
    }
    c.yaw = wrap(c.yaw);
    if !c.motor.grounded {
        c.air_time += super::DT;
        c.fall_speed = c.fall_speed.max(-c.motor.velocity.y);
    }
    let landed = c.motor.grounded && !was_grounded;
    let fall = c.fall_speed.max(-before.y);
    let flopped = landed && c.flop;
    if c.motor.grounded {
        c.air_time = 0.;
        c.fall_speed = 0.;
        c.flop = false;
    }
    // Weight and stomps on whatever dynamic thing is underfoot.
    let feet = c.pos();
    // A small sphere finds what is underfoot even when standing on an edge.
    let options = QueryOptions::new(0.4).layers(L_PROP);
    let support = game
        .world
        .spherecast_with_options(
            (feet + Vec3::Y * 0.3).to_array(),
            [0., -1., 0.],
            RADIUS * 0.9,
            options,
        )
        .and_then(|hit| {
            hit.body
                .map(|b| (b, Vec3::from_array(hit.point) - Vec3::Y * RADIUS * 0.9))
        });
    game.coon.support_body = support.map(|s| s.0);
    if game.coon.motor.grounded
        && let Some((body, point)) = support
    {
        let weight = MASS * 9.81 * super::DT;
        game.world.bodies[body].apply_impulse_at_point(-Vec3::Y * weight, point, true);
        if landed && fall > 2.5 {
            let strength = MASS * fall * if flopped { 1.05 } else { 0.8 };
            game.world.bodies[body].apply_impulse_at_point(-Vec3::Y * strength, point, true);
            game.emit(Event::Stomp {
                at: point,
                strength,
            });
            if body == game.mech.seesaw {
                catapult(game, point, strength);
            }
        }
    }
    if landed {
        let at = game.coon.pos();
        game.emit(Event::Landed { at, speed: fall });
    }
    push_props(game, direction, amount);
    hiding(game);
    game.coon.prompt = prompt(game);
}

/// A stomp on one end of the seesaw flings whatever sits on the other end.
/// The hinge carries the plank; this tops up the passenger so the launch is
/// reliably comedic rather than at the mercy of a single solver step.
fn catapult(game: &mut Game, point: Vec3, strength: f32) {
    let pivot = game.mech.seesaw_pivot;
    let side = (point.x - pivot.x).signum();
    let far = pivot + vec3(-side * 1.8, 0., 0.);
    for i in 0..game.props.len() {
        let p = &game.props[i];
        if p.holder != Holder::None || flat(p.pos - far).length() > 0.7 || p.pos.y > pivot.y + 1. {
            continue;
        }
        let up = (strength * 0.3).clamp(0., 9.);
        let v = vec3(-side * 0.9, up, 0.);
        let body = p.body;
        game.world.bodies[body].set_linvel(v, true);
        game.world.bodies[body].set_angvel(vec3(0., 0., side * 6.), true);
    }
}

fn push_props(game: &mut Game, direction: Vec3, amount: f32) {
    let c = &game.coon;
    let sliding = c.slide > 0.;
    let dir = if sliding {
        c.slide_dir
    } else {
        direction.normalize_or_zero()
    };
    if dir == Vec3::ZERO || (!sliding && amount < 0.2) {
        return;
    }
    // Standing on the tea tray? Scoot it along: it is, after all, a sled.
    if let Some(body) = c.support_body
        && game
            .props
            .iter()
            .any(|p| p.body == body && p.kind == PropKind::Tray)
    {
        let b = &game.world.bodies[body];
        let need = ((if sliding { 4.5 } else { 2.4 }) - b.linvel().dot(dir)).max(0.);
        let impulse = b.mass() * need * 0.2;
        game.world.bodies[body].apply_impulse(dir * impulse, true);
    }
    let c = &game.coon;
    let center = c.pos() + Vec3::Y * 0.28 + dir * (RADIUS + 0.12);
    let mut hits = [None; 6];
    let options = QueryOptions::new(0.04).layers(L_PROP);
    let report = game
        .world
        .overlap_sphere_into(center.to_array(), 0.26, options, &mut hits);
    let Ok(report) = report else { return };
    let mut pushed: [Option<ferravex_physics::RigidBodyHandle>; 6] = [None; 6];
    // Push toward a target speed (mass-aware), rather than with a fixed
    // impulse, so a featherweight tea tray is nudged, not catapulted.
    let target = if sliding { 4.5 } else { 1.6 * amount.max(0.5) };
    for (k, hit) in hits[..report.written].iter().flatten().enumerate() {
        let Some(body) = hit.body else { continue };
        if pushed.contains(&Some(body)) || Some(body) == game.coon.support_body {
            continue;
        }
        pushed[k] = Some(body);
        let b = &game.world.bodies[body];
        let along = b.linvel().dot(dir);
        let need = (target - along).max(0.);
        let mass = b.mass().min(8.);
        let impulse = mass * need * if sliding { 0.6 } else { 0.18 };
        let point = Vec3::from_array(hit.collider_point);
        game.world.bodies[body].apply_impulse_at_point(
            (dir + Vec3::Y * 0.05) * impulse,
            point,
            true,
        );
    }
}

fn hiding(game: &mut Game) {
    let p = game.coon.pos() + Vec3::Y * 0.1;
    let next = if let Some(b) = game.park.in_bush(p) {
        Hide::Bush(b)
    } else if let Some(t) = game.park.under_table(p) {
        Hide::Table(t)
    } else {
        Hide::None
    };
    let before = game.coon.hide;
    if before == next {
        return;
    }
    game.coon.hide = next;
    if next != Hide::None && before == Hide::None {
        game.stats.hides += 1;
        game.emit(Event::Hid {
            at: p,
            basket: false,
        });
    } else if next == Hide::None {
        game.emit(Event::Unhid { at: p });
    }
}

fn prompt(game: &Game) -> Prompt {
    if let Some(held) = game.coon.held {
        let at = drop_point(game, held);
        if game.props[held].kind == PropKind::Blanket && tray_near(game, at).is_some() {
            return Prompt::Load;
        }
        return if at.y > game.coon.pos().y + 0.4 {
            Prompt::Place(held)
        } else {
            Prompt::Drop(held)
        };
    }
    match use_candidate(game) {
        Some(Use::Bell) => return Prompt::Ring,
        Some(Use::Cloth) => return Prompt::Yank,
        None => {}
    }
    if let Some(i) = grab_candidate(game) {
        return Prompt::Grab(i);
    }
    if near_basket(game) {
        return Prompt::Basket;
    }
    Prompt::None
}

/// Shiny bottle caps drift toward a nearby raccoon and into the cheek pouch.
pub fn collect_shinies(game: &mut Game) {
    let magnet = Magnet {
        radius: 1.8,
        collect_radius: 0.4,
        speed: 6.5,
    };
    let p = game.coon.pos() + Vec3::Y * 0.3;
    for i in 0..game.shinies.len() {
        let (at, taken) = game.shinies[i];
        if taken || (at.y - p.y).abs() > 1.1 || game.coon.scruffed.is_some() {
            continue;
        }
        let Ok(step) = magnet.step(vec2(at.x, at.z), vec2(p.x, p.z), super::DT) else {
            continue;
        };
        let next = vec3(step.position.x, at.y + (p.y - at.y) * 0.08, step.position.y);
        game.shinies[i].0 = next;
        if step.collected
            && game
                .coon
                .pouch
                .add(Stack {
                    item: SHINY,
                    amount: 1,
                    limit: 99,
                })
                .is_ok()
        {
            game.shinies[i].1 = true;
            let count = game.coon.pouch.count(SHINY);
            game.emit(Event::Shiny { at: next, count });
            if count as usize == game.shinies.len() {
                super::pranks::bonus(game, super::pranks::Bonus::Magpie, next);
            }
        }
    }
}

/// Discover burrows (respawn anchors) and finish the escape.
pub fn dens(game: &mut Game) {
    if game.coon.scruffed.is_some() {
        return;
    }
    let p = game.coon.pos();
    for i in 0..game.park.dens.len() {
        if flat(p - game.park.dens[i]).length() < 2.2 {
            if game.dens_found & (1 << i) == 0 {
                game.dens_found |= 1 << i;
                let _ = game.dens.activate(i);
                game.emit(Event::DenFound {
                    den: i,
                    at: game.park.dens[i],
                });
            }
            if i == 0 {
                super::pranks::reached_den(game);
            }
        }
    }
}

/// Put the raccoon down after being carried (or shooed) at `at`.
pub fn set_down(game: &mut Game, at: Vec3, yaw: f32) {
    let (world, coon) = (&game.world, &mut game.coon);
    let mut target = at + Vec3::Y * 0.05;
    if coon
        .motor
        .teleport(world, coon.shape, coon.settings, target)
        .is_err()
    {
        target += Vec3::Y * 0.5;
        let _ = coon
            .motor
            .teleport(world, coon.shape, coon.settings, target);
    }
    coon.motor.velocity = Vec3::ZERO;
    coon.yaw = yaw;
    coon.prev_pos = coon.motor.position;
    coon.prev_yaw = yaw;
    coon.scruffed = None;
    coon.wriggle = 0;
    coon.slide = 0.;
}

pub fn wrap(angle: f32) -> f32 {
    let t = std::f32::consts::TAU;
    let a = angle.rem_euclid(t);
    if a > std::f32::consts::PI { a - t } else { a }
}

pub fn pose_for(at: Vec3, yaw: f32) -> RigidBodyPose {
    hold_pose(at, yaw)
}
